//! Documents pinned at the top of the home page or of a collection, as in Outline. Admins pin on
//! the home page; who can edit a collection pins in it. Everybody sees the pins of the documents
//! they can read.

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    api::documents::{access_sql, key_visible},
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Access, DocRow, DOC_COLUMNS, DOC_FROM},
    perm::{load_collection, load_doc, readable_collection_ids},
    AppState,
};

/// The pins of the home page (`collection` None) or of a collection that the user can read, in order
async fn visible(st: &AppState, auth: &Auth, collection: Option<Uuid>) -> ApiResult<Vec<(Uuid, DocRow)>> {
    let ids = readable_collection_ids(&st.db, auth).await?;
    let access = access_sql(auth, false);
    let sql = format!(
        "SELECT {DOC_COLUMNS}, p.id AS pin_id {DOC_FROM} JOIN pins p ON p.document_id = d.id \
         WHERE {access} AND p.team_id = $2 AND p.collection_id IS NOT DISTINCT FROM $4 \
         AND d.archived_at IS NULL AND d.published_at IS NOT NULL ORDER BY p.position, p.created_at"
    );
    let rows: Vec<sqlx::postgres::PgRow> =
        sqlx::query(&sql).bind(auth.user.id).bind(auth.team()?).bind(&ids).bind(collection).fetch_all(&st.db).await?;
    let mut out = Vec::new();
    for r in rows {
        use sqlx::{FromRow, Row};
        let pin: Uuid = r.try_get("pin_id")?;
        out.push((pin, DocRow::from_row(&r)?));
    }
    let allowed: Vec<Uuid> = key_visible(auth, out.iter().map(|(_, d)| d.clone()).collect()).into_iter().map(|d| d.id).collect();
    Ok(out.into_iter().filter(|(_, d)| allowed.contains(&d.id)).collect())
}

/// Can `auth` pin documents on the home page (None) or in a collection?
async fn can_pin(st: &AppState, auth: &Auth, collection: Option<Uuid>) -> ApiResult<bool> {
    if auth.require_writer().is_err() {
        return Ok(false);
    }
    match collection {
        None => Ok(auth.is_admin()),
        Some(c) => Ok(load_collection(&st.db, auth, &c.to_string(), Access::Write).await.is_ok()),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListReq {
    collection_id: Option<Uuid>,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let pins = visible(&st, &auth, b.collection_id).await?;
    let out: Vec<Value> = pins
        .iter()
        .map(|(id, d)| {
            let mut doc = crate::api::documents::summary(d);
            // the beginning of the text, for the card
            doc["text"] = json!(d.text.chars().take(400).collect::<String>());
            json!({ "id": id, "documentId": d.id, "collectionId": b.collection_id, "document": doc })
        })
        .collect();
    ok(json!({ "pins": out, "canPin": can_pin(&st, &auth, b.collection_id).await? }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocReq {
    document_id: String,
}

/// Where a document is pinned, and where the user may pin it: for the document menu
pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<DocReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let pins: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as("SELECT id, collection_id FROM pins WHERE document_id = $1")
        .bind(d.id)
        .fetch_all(&st.db)
        .await?;
    let home = pins.iter().find(|p| p.1.is_none()).map(|p| p.0);
    let collection = pins.iter().find(|p| p.1.is_some() && p.1 == d.collection_id).map(|p| p.0);
    let pinnable = d.published_at.is_some() && !d.template && d.archived_at.is_none();
    ok(json!({
        "home": home,
        "collection": collection,
        "canPinHome": pinnable && can_pin(&st, &auth, None).await?,
        "canPinCollection": pinnable && d.collection_id.is_some() && can_pin(&st, &auth, d.collection_id).await?,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReq {
    document_id: String,
    /// pin in the collection of the document; without it, on the home page
    collection_id: Option<Uuid>,
    index: Option<usize>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    if d.published_at.is_none() || d.template {
        return Err(ApiError::bad_request("Only published documents can be pinned"));
    }
    if b.collection_id.is_some() && b.collection_id != d.collection_id {
        return Err(ApiError::bad_request("The document is not in this collection"));
    }
    if !can_pin(&st, &auth, b.collection_id).await? {
        return Err(ApiError::forbidden(if b.collection_id.is_some() {
            "Only who can edit the collection can pin documents in it"
        } else {
            "Only admins can pin documents on the home page"
        }));
    }
    sqlx::query(
        "INSERT INTO pins (team_id, collection_id, document_id, position, created_by) \
         SELECT $1, $2, $3, COALESCE(MAX(position) + 1, 0), $4 FROM pins WHERE team_id = $1 AND collection_id IS NOT DISTINCT FROM $2 \
         ON CONFLICT DO NOTHING",
    )
    .bind(d.team_id)
    .bind(b.collection_id)
    .bind(d.id)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM pins WHERE document_id = $1 AND collection_id IS NOT DISTINCT FROM $2")
        .bind(d.id)
        .bind(b.collection_id)
        .fetch_one(&st.db)
        .await?;
    if let Some(i) = b.index {
        move_to(&st, &auth, id, b.collection_id, i).await?;
    }
    let data = json!({ "where": if b.collection_id.is_some() { "collection" } else { "home" } });
    audit::record(&st, &auth, "documents.pin", Target::document(d.id, &d.title), data).await;
    ok(json!({ "id": id }))
}

/// The pin `id` of the current workspace, if `auth` may change it
async fn manageable(st: &AppState, auth: &Auth, id: Uuid) -> ApiResult<(Option<Uuid>, Uuid)> {
    let row: Option<(Option<Uuid>, Uuid)> = sqlx::query_as("SELECT collection_id, document_id FROM pins WHERE id = $1 AND team_id = $2")
        .bind(id)
        .bind(auth.team()?)
        .fetch_optional(&st.db)
        .await?;
    let (collection, doc) = row.ok_or_else(|| ApiError::not_found("Pin not found"))?;
    if !can_pin(st, auth, collection).await? {
        return Err(ApiError::forbidden("You cannot change the pinned documents here"));
    }
    Ok((collection, doc))
}

/// Moves a pin to `index` among the visible ones and renumbers them
async fn move_to(st: &AppState, auth: &Auth, id: Uuid, collection: Option<Uuid>, index: usize) -> ApiResult<()> {
    let mut order: Vec<Uuid> = visible(st, auth, collection).await?.into_iter().map(|(p, _)| p).collect();
    let from = order.iter().position(|p| *p == id).ok_or_else(|| ApiError::not_found("Pin not found"))?;
    order.remove(from);
    order.insert(index.min(order.len()), id);
    sqlx::query(
        "UPDATE pins SET position = x.pos::int FROM unnest($1::uuid[]) WITH ORDINALITY AS x(id, pos) \
         WHERE pins.id = x.id AND pins.team_id = $2",
    )
    .bind(&order)
    .bind(auth.team()?)
    .execute(&st.db)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct UpdateReq {
    id: Uuid,
    index: usize,
}

/// Changes the position of a pin: { id, index }
pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let (collection, _) = manageable(&st, &auth, b.id).await?;
    move_to(&st, &auth, b.id, collection, b.index).await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (collection, doc) = manageable(&st, &auth, b.id).await?;
    sqlx::query("DELETE FROM pins WHERE id = $1").bind(b.id).execute(&st.db).await?;
    let title: Option<(String,)> = sqlx::query_as("SELECT title FROM documents WHERE id = $1").bind(doc).fetch_optional(&st.db).await?;
    let data = json!({ "where": if collection.is_some() { "collection" } else { "home" } });
    audit::record(&st, &auth, "documents.unpin", Target::document(doc, &title.map(|t| t.0).unwrap_or_default()), data).await;
    ok(json!({ "success": true }))
}
