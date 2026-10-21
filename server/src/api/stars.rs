use std::collections::HashMap;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Access, DocRow, DOC_COLUMNS, DOC_FROM},
    perm::{load_doc, readable_collection_ids},
    AppState,
};

/// A star on a document or on a collection, in the order chosen by the user
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Star {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document: Option<DocRow>,
}

/// The stars of the user in the current workspace that they can still read, in order
async fn visible(st: &AppState, auth: &Auth) -> ApiResult<Vec<Star>> {
    let ids = readable_collection_ids(&st.db, auth).await?;
    let sql = format!(
        "SELECT {DOC_COLUMNS} {DOC_FROM} JOIN stars st ON st.document_id = d.id AND st.user_id = $1 \
         WHERE d.team_id = $2 AND d.deleted_at IS NULL AND d.archived_at IS NULL \
         AND (d.collection_id = ANY($3) OR (d.collection_id IS NULL AND d.created_by = $1)) \
         AND (d.published_at IS NOT NULL OR d.created_by = $1)"
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .fetch_all(&st.db)
        .await?;
    let mut docs: HashMap<Uuid, DocRow> = docs.into_iter().map(|d| (d.id, d)).collect();
    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as("SELECT id, document_id, collection_id FROM stars WHERE user_id = $1 ORDER BY position, created_at")
            .bind(auth.user.id)
            .fetch_all(&st.db)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, doc, col)| match (doc, col) {
            (Some(d), _) => docs
                .remove(&d)
                .map(|d| Star { id, document_id: Some(d.id), collection_id: None, document: Some(d) }),
            (None, Some(c)) if ids.contains(&c) => Some(Star { id, document_id: None, collection_id: Some(c), document: None }),
            _ => None,
        })
        .collect())
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    ok(visible(&st, &auth).await?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarReq {
    id: Option<Uuid>,
    document_id: Option<String>,
    collection_id: Option<Uuid>,
    /// position among the stars of the workspace (0 = first); new stars go last without it
    index: Option<usize>,
}

/// The document or collection a request is about, checked for read access
async fn target(st: &AppState, auth: &Auth, b: &StarReq) -> ApiResult<(Option<Uuid>, Option<Uuid>)> {
    match (&b.document_id, b.collection_id) {
        (Some(d), _) => {
            let (d, _) = load_doc(&st.db, auth, d, Access::Read).await?;
            Ok((Some(d.id), None))
        }
        (None, Some(c)) => {
            if !readable_collection_ids(&st.db, auth).await?.contains(&c) {
                return Err(ApiError::not_found("Collection not found"));
            }
            Ok((None, Some(c)))
        }
        _ => Err(ApiError::bad_request("documentId or collectionId is required")),
    }
}

/// Moves a star to `index` among the visible ones and renumbers them
async fn move_to(st: &AppState, auth: &Auth, id: Uuid, index: usize) -> ApiResult<()> {
    let mut order: Vec<Uuid> = visible(st, auth).await?.into_iter().map(|s| s.id).collect();
    let from = order.iter().position(|s| *s == id).ok_or_else(|| ApiError::not_found("Star not found"))?;
    order.remove(from);
    order.insert(index.min(order.len()), id);
    sqlx::query(
        "UPDATE stars SET position = x.pos::int FROM unnest($1::uuid[]) WITH ORDINALITY AS x(id, pos) \
         WHERE stars.id = x.id AND stars.user_id = $2",
    )
    .bind(&order)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    Ok(())
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<StarReq>) -> ApiResult<Json<Value>> {
    let (doc, col) = target(&st, &auth, &b).await?;
    sqlx::query(
        "INSERT INTO stars (id, user_id, document_id, collection_id, position) \
         SELECT $1, $2, $3, $4, COALESCE(MAX(position) + 1, 0) FROM stars WHERE user_id = $2 ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .execute(&st.db)
    .await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM stars WHERE user_id = $1 AND document_id IS NOT DISTINCT FROM $2 AND collection_id IS NOT DISTINCT FROM $3",
    )
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .fetch_one(&st.db)
    .await?;
    if let Some(i) = b.index {
        move_to(&st, &auth, id, i).await?;
    }
    ok(json!({ "id": id }))
}

/// Changes the position of a star: { id, index }
pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<StarReq>) -> ApiResult<Json<Value>> {
    let id = b.id.ok_or_else(|| ApiError::bad_request("id is required"))?;
    let index = b.index.ok_or_else(|| ApiError::bad_request("index is required"))?;
    move_to(&st, &auth, id, index).await?;
    ok(json!({ "success": true }))
}

/// Removes a star, by its id or by the starred document or collection
pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<StarReq>) -> ApiResult<Json<Value>> {
    let (doc, col) = if b.id.is_some() { (None, None) } else { target(&st, &auth, &b).await? };
    sqlx::query("DELETE FROM stars WHERE user_id = $1 AND (id = $2 OR document_id = $3 OR collection_id = $4)")
        .bind(auth.user.id)
        .bind(b.id)
        .bind(doc)
        .bind(col)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
