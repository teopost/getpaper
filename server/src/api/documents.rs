use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{doc_select, Access, DocRow, DOC_COLUMNS, DOC_FROM},
    perm::{descendant_ids, find_doc, load_collection, load_doc, readable_collection_ids, readable_collection_ids_all_teams},
    util::{new_url_id, parse_id, IdRef},
    AppState,
};

/// Visibility clause. Binds: $1 user id, $2 team id, $3 readable collection ids.
/// Documents visible to the user: readable collections ($3, of workspace $2 or of the workspaces in
/// $2 when `many_teams`), their own collection-less drafts, published ones or their own.
/// A key limited to some collections doesn't see collection-less documents.
fn access_sql(auth: &Auth, many_teams: bool) -> String {
    let team = if many_teams { "d.team_id = ANY($2)" } else { "d.team_id = $2" };
    let own = if auth.key.as_ref().is_some_and(|k| k.collections.is_some()) {
        ""
    } else {
        " OR (d.collection_id IS NULL AND d.created_by = $1)"
    };
    format!("{team} AND d.deleted_at IS NULL AND (d.collection_id = ANY($3){own}) AND (d.published_at IS NOT NULL OR d.created_by = $1)")
}

/// Drops documents outside the collections an API key is limited to.
fn key_visible(auth: &Auth, docs: Vec<DocRow>) -> Vec<DocRow> {
    match auth.key.as_ref().filter(|k| k.collections.is_some()) {
        Some(k) => docs.into_iter().filter(|d| d.collection_id.is_some_and(|c| k.allows_collection(c))).collect(),
        None => docs,
    }
}

fn page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (limit.unwrap_or(25).clamp(1, 100), offset.unwrap_or(0).max(0))
}

// ---------------------------------------------------------------------------
// listing
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListReq {
    collection_id: Option<Uuid>,
    parent_document_id: Option<Uuid>,
    user_id: Option<Uuid>,
    sort: Option<String>,
    direction: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let col = match b.sort.as_deref() {
        Some("title") => "d.title",
        Some("createdAt") => "d.created_at",
        Some("publishedAt") => "d.published_at",
        Some("index") => "d.position",
        _ => "d.updated_at",
    };
    let dir = if b.direction.as_deref() == Some("ASC") { "ASC" } else { "DESC" };
    let (limit, offset) = page(b.limit, b.offset);
    let access = access_sql(&auth, false);
    let sql = format!(
        "{} WHERE {access} AND d.archived_at IS NULL AND d.published_at IS NOT NULL AND d.template = false \
         AND ($4::uuid IS NULL OR d.collection_id = $4) AND ($5::uuid IS NULL OR d.parent_document_id = $5) \
         AND ($6::uuid IS NULL OR d.created_by = $6) ORDER BY {col} {dir}, d.id LIMIT $7 OFFSET $8",
        doc_select()
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .bind(b.collection_id)
        .bind(b.parent_document_id)
        .bind(b.user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    ok(docs)
}

#[derive(Deserialize, Default)]
pub struct PageReq {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn drafts(State(st): State<AppState>, auth: Auth, Json(b): Json<PageReq>) -> ApiResult<Json<Value>> {
    let (limit, offset) = page(b.limit, b.offset);
    let sql = format!(
        "{} WHERE d.team_id = $2 AND d.created_by = $1 AND d.published_at IS NULL AND d.deleted_at IS NULL \
         AND d.template = false ORDER BY d.updated_at DESC LIMIT $3 OFFSET $4",
        doc_select()
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    ok(key_visible(&auth, docs))
}

pub async fn viewed(State(st): State<AppState>, auth: Auth, Json(b): Json<PageReq>) -> ApiResult<Json<Value>> {
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let (limit, offset) = page(b.limit, b.offset);
    let access = access_sql(&auth, false);
    let sql = format!(
        "SELECT {DOC_COLUMNS} {DOC_FROM} JOIN views v ON v.document_id = d.id AND v.user_id = $1 \
         WHERE {access} AND d.archived_at IS NULL AND d.template = false ORDER BY v.last_viewed_at DESC LIMIT $4 OFFSET $5"
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    ok(docs)
}

pub async fn archived(State(st): State<AppState>, auth: Auth, Json(b): Json<PageReq>) -> ApiResult<Json<Value>> {
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let (limit, offset) = page(b.limit, b.offset);
    let access = access_sql(&auth, false);
    let sql = format!(
        "{} WHERE {access} AND d.archived_at IS NOT NULL ORDER BY d.archived_at DESC LIMIT $4 OFFSET $5",
        doc_select()
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    ok(docs)
}

pub async fn deleted(State(st): State<AppState>, auth: Auth, Json(b): Json<PageReq>) -> ApiResult<Json<Value>> {
    let (limit, offset) = page(b.limit, b.offset);
    let sql = format!(
        "{} WHERE d.team_id = $2 AND d.deleted_at IS NOT NULL AND (d.created_by = $1 OR $3) \
         ORDER BY d.deleted_at DESC LIMIT $4 OFFSET $5",
        doc_select()
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(auth.is_admin())
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    ok(key_visible(&auth, docs))
}

pub async fn templates(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let sql = format!(
        "{} WHERE d.team_id = $2 AND d.deleted_at IS NULL AND d.template = true AND d.archived_at IS NULL \
         AND (d.collection_id = ANY($3) OR d.collection_id IS NULL) AND (d.published_at IS NOT NULL OR d.created_by = $1) \
         ORDER BY d.title",
        doc_select()
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .fetch_all(&st.db)
        .await?;
    ok(key_visible(&auth, docs))
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct DocLink {
    id: Uuid,
    url_id: String,
    title: String,
    emoji: Option<String>,
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct CollectionLink {
    id: Uuid,
    url_id: String,
    name: String,
    color: String,
    icon: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoReq {
    id: Option<String>,
    share_id: Option<String>,
}

async fn doc_path(db: &PgPool, id: Uuid) -> ApiResult<Vec<DocLink>> {
    Ok(sqlx::query_as(
        "WITH RECURSIVE anc AS ( \
            SELECT id, parent_document_id, title, url_id, emoji, 0 AS depth FROM documents WHERE id = $1 \
            UNION ALL SELECT d.id, d.parent_document_id, d.title, d.url_id, d.emoji, anc.depth + 1 \
            FROM documents d JOIN anc ON d.id = anc.parent_document_id WHERE anc.depth < 50) \
         SELECT id, url_id, title, emoji FROM anc WHERE depth > 0 ORDER BY depth DESC",
    )
    .bind(id)
    .fetch_all(db)
    .await?)
}

async fn doc_children(db: &PgPool, id: Uuid) -> ApiResult<Vec<DocLink>> {
    Ok(sqlx::query_as(
        "SELECT id, url_id, title, emoji FROM documents WHERE parent_document_id = $1 AND published_at IS NOT NULL \
         AND deleted_at IS NULL AND archived_at IS NULL ORDER BY position, title",
    )
    .bind(id)
    .fetch_all(db)
    .await?)
}

async fn collection_link(db: &PgPool, id: Option<Uuid>) -> ApiResult<Option<CollectionLink>> {
    match id {
        Some(id) => Ok(sqlx::query_as("SELECT id, url_id, name, color, icon FROM collections WHERE id = $1")
            .bind(id)
            .fetch_optional(db)
            .await?),
        None => Ok(None),
    }
}

pub async fn info(State(st): State<AppState>, auth: Option<Auth>, Json(b): Json<InfoReq>) -> ApiResult<Json<Value>> {
    if let Some(share_id) = &b.share_id {
        if !st.settings().allow_public_sharing {
            return Err(ApiError::not_found("This link is no longer valid"));
        }
        let share: Option<(Uuid, String, Uuid)> = match parse_id(share_id) {
            IdRef::Uuid(u) => sqlx::query_as("SELECT id, url_id, document_id FROM shares WHERE id = $1 AND revoked_at IS NULL")
                .bind(u)
                .fetch_optional(&st.db)
                .await?,
            IdRef::UrlId(u) => {
                sqlx::query_as("SELECT id, url_id, document_id FROM shares WHERE url_id = $1 AND revoked_at IS NULL")
                    .bind(u)
                    .fetch_optional(&st.db)
                    .await?
            }
        };
        let (sid, surl, doc_id) = share.ok_or_else(|| ApiError::not_found("This link is no longer valid"))?;
        let d = find_doc(&st.db, Uuid::nil(), &doc_id.to_string()).await?;
        let (sharing, team_name): (bool, String) = sqlx::query_as("SELECT sharing, name FROM teams WHERE id = $1")
            .bind(d.team_id)
            .fetch_one(&st.db)
            .await?;
        let coll_sharing: Option<(bool,)> = match d.collection_id {
            Some(c) => sqlx::query_as("SELECT sharing FROM collections WHERE id = $1 AND deleted_at IS NULL")
                .bind(c)
                .fetch_optional(&st.db)
                .await?,
            None => None,
        };
        if !sharing
            || !coll_sharing.map(|c| c.0).unwrap_or(false)
            || d.published_at.is_none()
            || d.deleted_at.is_some()
        {
            return Err(ApiError::not_found("This link is no longer valid"));
        }
        sqlx::query("UPDATE shares SET views = views + 1, last_accessed_at = now() WHERE id = $1")
            .bind(sid)
            .execute(&st.db)
            .await?;
        return ok(json!({
            "document": d,
            "access": "read",
            "share": { "id": sid, "urlId": surl },
            "team": { "name": team_name },
            "path": [],
            "children": [],
            "collection": null,
        }));
    }

    let auth = auth.ok_or_else(ApiError::unauthorized)?;
    let id = b.id.ok_or_else(|| ApiError::bad_request("id is required"))?;
    let (d, a) = load_doc(&st.db, &auth, &id, Access::Read).await?;
    sqlx::query(
        "INSERT INTO views (user_id, document_id, count, last_viewed_at) VALUES ($1, $2, 1, now()) \
         ON CONFLICT (user_id, document_id) DO UPDATE SET count = views.count + 1, last_viewed_at = now()",
    )
    .bind(auth.user.id)
    .bind(d.id)
    .execute(&st.db)
    .await?;
    let path = doc_path(&st.db, d.id).await?;
    let children = doc_children(&st.db, d.id).await?;
    let collection = collection_link(&st.db, d.collection_id).await?;
    ok(json!({
        "document": d,
        "access": a.as_str(),
        "collection": collection,
        "path": path,
        "children": children,
    }))
}

// ---------------------------------------------------------------------------
// create / update
// ---------------------------------------------------------------------------

async fn save_revision(db: &PgPool, d: &DocRow, user_id: Uuid) -> ApiResult<()> {
    let last: Option<(Uuid, Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, user_id, created_at FROM revisions WHERE document_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(d.id)
    .fetch_optional(db)
    .await?;
    if let Some((rid, uid, at)) = last {
        if uid == user_id && Utc::now() - at < Duration::minutes(5) {
            sqlx::query("UPDATE revisions SET title = $2, text = $3, emoji = $4 WHERE id = $1")
                .bind(rid)
                .bind(&d.title)
                .bind(&d.text)
                .bind(&d.emoji)
                .execute(db)
                .await?;
            return Ok(());
        }
    }
    sqlx::query("INSERT INTO revisions (id, document_id, user_id, title, text, emoji) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(Uuid::new_v4())
        .bind(d.id)
        .bind(user_id)
        .bind(&d.title)
        .bind(&d.text)
        .bind(&d.emoji)
        .execute(db)
        .await?;
    Ok(())
}

pub struct NewDoc {
    pub title: String,
    pub text: String,
    pub emoji: Option<String>,
    pub collection_id: Option<String>,
    pub parent_document_id: Option<String>,
    pub publish: bool,
    pub template: bool,
    pub template_id: Option<String>,
}

pub async fn create_doc(st: &AppState, auth: &Auth, n: NewDoc) -> ApiResult<DocRow> {
    auth.require_writer()?;
    let mut team = auth.team()?;
    let mut collection_id: Option<Uuid> = None;
    let mut parent_id: Option<Uuid> = None;
    if let Some(p) = n.parent_document_id.as_deref().filter(|s| !s.is_empty()) {
        let (pd, _) = load_doc(&st.db, auth, p, Access::Write).await?;
        if pd.collection_id.is_none() || pd.published_at.is_none() {
            return Err(ApiError::bad_request("Nested documents can only be created inside published documents"));
        }
        parent_id = Some(pd.id);
        collection_id = pd.collection_id;
        team = pd.team_id;
    } else if let Some(c) = n.collection_id.as_deref().filter(|s| !s.is_empty()) {
        let (c, _) = load_collection(&st.db, auth, c, Access::Write).await?;
        collection_id = Some(c.id);
        team = c.team_id;
    }

    let (mut title, mut text, mut emoji) = (n.title, n.text, n.emoji.filter(|e| !e.is_empty()));
    if let Some(tid) = n.template_id.as_deref().filter(|s| !s.is_empty()) {
        let (t, _) = load_doc(&st.db, auth, tid, Access::Read).await?;
        if !t.template {
            return Err(ApiError::bad_request("Not a template"));
        }
        if title.trim().is_empty() {
            title = t.title.clone();
        }
        text = t.text.clone();
        if emoji.is_none() {
            emoji = t.emoji.clone();
        }
    }
    let publish = n.publish || n.template;
    if publish && collection_id.is_none() && !n.template {
        return Err(ApiError::bad_request("Choose a collection to publish this document in"));
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, url_id, team_id, collection_id, parent_document_id, title, emoji, text, template, \
         created_by, updated_by, published_at, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, CASE WHEN $11 THEN now() ELSE NULL END, extract(epoch from now())::float8)",
    )
    .bind(id)
    .bind(new_url_id())
    .bind(team)
    .bind(collection_id)
    .bind(parent_id)
    .bind(title.trim())
    .bind(&emoji)
    .bind(&text)
    .bind(n.template)
    .bind(auth.user.id)
    .bind(publish)
    .execute(&st.db)
    .await?;
    let d = find_doc(&st.db, auth.user.id, &id.to_string()).await?;
    if publish {
        save_revision(&st.db, &d, auth.user.id).await?;
    }
    Ok(d)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReq {
    title: Option<String>,
    text: Option<String>,
    emoji: Option<String>,
    collection_id: Option<String>,
    parent_document_id: Option<String>,
    publish: Option<bool>,
    template: Option<bool>,
    template_id: Option<String>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let d = create_doc(
        &st,
        &auth,
        NewDoc {
            title: b.title.unwrap_or_default(),
            text: b.text.unwrap_or_default(),
            emoji: b.emoji,
            collection_id: b.collection_id,
            parent_document_id: b.parent_document_id,
            publish: b.publish.unwrap_or(false),
            template: b.template.unwrap_or(false),
            template_id: b.template_id,
        },
    )
    .await?;
    ok(d)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReq {
    id: String,
    title: Option<String>,
    text: Option<String>,
    emoji: Option<String>,
    full_width: Option<bool>,
    publish: Option<bool>,
    collection_id: Option<String>,
    /// optimistic concurrency: the updatedAt the client last saw
    base_updated_at: Option<DateTime<Utc>>,
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    if let Some(base) = b.base_updated_at {
        if d.updated_by != auth.user.id && d.updated_at > base + Duration::milliseconds(1) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "conflict",
                format!("{} changed this document while you were editing. Reload to see their changes.", d.updated_by_name),
            ));
        }
    }
    let mut collection_id = d.collection_id;
    let mut publishing = false;
    if b.publish == Some(true) && d.published_at.is_none() {
        if let Some(cid) = b.collection_id.as_deref().filter(|s| !s.is_empty()) {
            let (c, _) = load_collection(&st.db, &auth, cid, Access::Write).await?;
            if c.team_id != d.team_id {
                return Err(ApiError::bad_request("Collection belongs to another workspace"));
            }
            collection_id = Some(c.id);
        } else if let Some(cid) = collection_id {
            load_collection(&st.db, &auth, &cid.to_string(), Access::Write).await?;
        }
        if collection_id.is_none() && !d.template {
            return Err(ApiError::bad_request("Choose a collection to publish this document in"));
        }
        publishing = true;
    }
    sqlx::query(
        "UPDATE documents SET title = COALESCE($2, title), text = COALESCE($3, text), \
         emoji = CASE WHEN $4::text IS NULL THEN emoji WHEN $4 = '' THEN NULL ELSE $4 END, \
         full_width = COALESCE($5, full_width), collection_id = $6, \
         published_at = CASE WHEN $7 THEN now() ELSE published_at END, \
         position = CASE WHEN $7 THEN extract(epoch from now())::float8 ELSE position END, \
         updated_by = $8, updated_at = now() WHERE id = $1",
    )
    .bind(d.id)
    .bind(b.title.as_deref().map(|t| t.trim()))
    .bind(&b.text)
    .bind(&b.emoji)
    .bind(b.full_width)
    .bind(collection_id)
    .bind(publishing)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    let nd = find_doc(&st.db, auth.user.id, &d.id.to_string()).await?;
    let changed = nd.title != d.title || nd.text != d.text || nd.emoji != d.emoji;
    if nd.published_at.is_some() && (changed || publishing) {
        save_revision(&st.db, &nd, auth.user.id).await?;
    }
    ok(nd)
}

// ---------------------------------------------------------------------------
// structure
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReq {
    id: String,
    collection_id: Option<String>,
    parent_document_id: Option<String>,
    index: Option<usize>,
}

pub async fn move_doc(State(st): State<AppState>, auth: Auth, Json(b): Json<MoveReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    if d.published_at.is_none() {
        return Err(ApiError::bad_request("Drafts cannot be moved, publish the document first"));
    }
    let tree = descendant_ids(&st.db, d.id).await?;
    let (target_collection, target_parent, target_team) =
        if let Some(p) = b.parent_document_id.as_deref().filter(|s| !s.is_empty()) {
            let (pd, _) = load_doc(&st.db, &auth, p, Access::Write).await?;
            if tree.contains(&pd.id) {
                return Err(ApiError::bad_request("Cannot move a document inside itself"));
            }
            let cid = pd
                .collection_id
                .ok_or_else(|| ApiError::bad_request("Target document is not in a collection"))?;
            (cid, Some(pd.id), pd.team_id)
        } else {
            let cid = b
                .collection_id
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("collectionId or parentDocumentId is required"))?;
            let (c, _) = load_collection(&st.db, &auth, cid, Access::Write).await?;
            (c.id, None, c.team_id)
        };
    // to another workspace: the user must edit the document (checked above) and write the target
    // (load_collection / load_doc, which check the role in the target workspace)
    let other_workspace = target_team != d.team_id;
    if other_workspace && d.template {
        return Err(ApiError::bad_request("Templates cannot be moved to another workspace"));
    }
    let siblings: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM documents WHERE collection_id = $1 AND parent_document_id IS NOT DISTINCT FROM $2 AND id <> $3 \
         AND deleted_at IS NULL AND published_at IS NOT NULL AND template = false ORDER BY position, title",
    )
    .bind(target_collection)
    .bind(target_parent)
    .bind(d.id)
    .fetch_all(&st.db)
    .await?;
    let mut order: Vec<Uuid> = siblings.into_iter().map(|r| r.0).collect();
    let idx = b.index.unwrap_or(order.len()).min(order.len());
    order.insert(idx, d.id);

    let mut tx = st.db.begin().await?;
    sqlx::query("UPDATE documents SET collection_id = $2, parent_document_id = $3, updated_at = now() WHERE id = $1")
        .bind(d.id)
        .bind(target_collection)
        .bind(target_parent)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE documents SET collection_id = $2 WHERE id = ANY($1)")
        .bind(&tree)
        .bind(target_collection)
        .execute(&mut *tx)
        .await?;
    if other_workspace {
        // the whole subtree, its attachments and public links follow; links are revoked because the
        // target workspace may not allow public sharing
        let mut all = tree.clone();
        all.push(d.id);
        sqlx::query("UPDATE documents SET team_id = $2 WHERE id = ANY($1)").bind(&all).bind(target_team).execute(&mut *tx).await?;
        sqlx::query("UPDATE attachments SET team_id = $2 WHERE document_id = ANY($1)")
            .bind(&all)
            .bind(target_team)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE shares SET team_id = $2, revoked_at = COALESCE(revoked_at, now()) WHERE document_id = ANY($1)")
            .bind(&all)
            .bind(target_team)
            .execute(&mut *tx)
            .await?;
        tracing::info!(document = %d.id, documents = all.len(), from = %d.team_id, to = %target_team, "document moved to another workspace");
    }
    for (i, id) in order.iter().enumerate() {
        sqlx::query("UPDATE documents SET position = $2 WHERE id = $1")
            .bind(id)
            .bind(i as f64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    ok(find_doc(&st.db, auth.user.id, &d.id.to_string()).await?)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: String,
}

pub async fn duplicate(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Read).await?;
    let nd = create_doc(
        &st,
        &auth,
        NewDoc {
            title: format!("{} (copy)", d.title),
            text: d.text.clone(),
            emoji: d.emoji.clone(),
            collection_id: d.collection_id.map(|c| c.to_string()),
            parent_document_id: d.parent_document_id.map(|p| p.to_string()),
            publish: d.published_at.is_some(),
            template: d.template,
            template_id: None,
        },
    )
    .await?;
    ok(nd)
}

pub async fn archive(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    if d.published_at.is_none() {
        return Err(ApiError::bad_request("Drafts cannot be archived"));
    }
    let ids = descendant_ids(&st.db, d.id).await?;
    sqlx::query("UPDATE documents SET archived_at = now() WHERE id = ANY($1) AND archived_at IS NULL")
        .bind(&ids)
        .execute(&st.db)
        .await?;
    ok(find_doc(&st.db, auth.user.id, &d.id.to_string()).await?)
}

pub async fn unpublish(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    if d.created_by != auth.user.id && !auth.is_admin() {
        return Err(ApiError::forbidden("Only the author can unpublish a document"));
    }
    if !doc_children(&st.db, d.id).await?.is_empty() {
        return Err(ApiError::bad_request("Cannot unpublish a document with nested documents"));
    }
    sqlx::query("UPDATE documents SET published_at = NULL WHERE id = $1")
        .bind(d.id)
        .execute(&st.db)
        .await?;
    ok(find_doc(&st.db, auth.user.id, &d.id.to_string()).await?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReq {
    id: String,
    revision_id: Option<Uuid>,
    collection_id: Option<String>,
}

pub async fn restore(State(st): State<AppState>, auth: Auth, Json(b): Json<RestoreReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    if let Some(rid) = b.revision_id {
        let rev: (String, String, Option<String>) =
            sqlx::query_as("SELECT title, text, emoji FROM revisions WHERE id = $1 AND document_id = $2")
                .bind(rid)
                .bind(d.id)
                .fetch_one(&st.db)
                .await?;
        sqlx::query("UPDATE documents SET title = $2, text = $3, emoji = $4, updated_by = $5, updated_at = now() WHERE id = $1")
            .bind(d.id)
            .bind(&rev.0)
            .bind(&rev.1)
            .bind(&rev.2)
            .bind(auth.user.id)
            .execute(&st.db)
            .await?;
        let nd = find_doc(&st.db, auth.user.id, &d.id.to_string()).await?;
        save_revision(&st.db, &nd, auth.user.id).await?;
        return ok(nd);
    }

    let ids = descendant_ids(&st.db, d.id).await?;
    if d.deleted_at.is_some() {
        let mut cid = d.collection_id;
        let alive: Option<(Uuid,)> = match cid {
            Some(c) => sqlx::query_as("SELECT id FROM collections WHERE id = $1 AND deleted_at IS NULL")
                .bind(c)
                .fetch_optional(&st.db)
                .await?,
            None => None,
        };
        if alive.is_none() && d.published_at.is_some() {
            let target = b
                .collection_id
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("The original collection was deleted, choose another collection"))?;
            let (c, _) = load_collection(&st.db, &auth, target, Access::Write).await?;
            cid = Some(c.id);
        } else if alive.is_some() {
            load_collection(&st.db, &auth, &cid.unwrap().to_string(), Access::Write).await?;
        }
        let parent_ok: Option<(Uuid,)> = match d.parent_document_id {
            Some(p) => sqlx::query_as(
                "SELECT id FROM documents WHERE id = $1 AND deleted_at IS NULL AND collection_id IS NOT DISTINCT FROM $2",
            )
            .bind(p)
            .bind(cid)
            .fetch_optional(&st.db)
            .await?,
            None => None,
        };
        let mut tx = st.db.begin().await?;
        sqlx::query("UPDATE documents SET deleted_at = NULL, collection_id = $2 WHERE id = ANY($1)")
            .bind(&ids)
            .bind(cid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE documents SET parent_document_id = $2 WHERE id = $1")
            .bind(d.id)
            .bind(parent_ok.map(|p| p.0))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    } else if d.archived_at.is_some() {
        let parent_archived: Option<(Uuid,)> = match d.parent_document_id {
            Some(p) => sqlx::query_as("SELECT id FROM documents WHERE id = $1 AND archived_at IS NOT NULL")
                .bind(p)
                .fetch_optional(&st.db)
                .await?,
            None => None,
        };
        let mut tx = st.db.begin().await?;
        sqlx::query("UPDATE documents SET archived_at = NULL WHERE id = ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        if parent_archived.is_some() {
            sqlx::query("UPDATE documents SET parent_document_id = NULL WHERE id = $1")
                .bind(d.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    ok(find_doc(&st.db, auth.user.id, &d.id.to_string()).await?)
}

#[derive(Deserialize)]
pub struct DeleteReq {
    id: String,
    permanent: Option<bool>,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<DeleteReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Write).await?;
    let ids = descendant_ids(&st.db, d.id).await?;
    if b.permanent == Some(true) {
        if d.deleted_at.is_none() {
            return Err(ApiError::bad_request("Move the document to the trash first"));
        }
        sqlx::query("DELETE FROM documents WHERE id = ANY($1)").bind(&ids).execute(&st.db).await?;
    } else {
        sqlx::query("UPDATE documents SET deleted_at = now() WHERE id = ANY($1) AND deleted_at IS NULL")
            .bind(&ids)
            .execute(&st.db)
            .await?;
    }
    ok(json!({ "success": true }))
}

// ---------------------------------------------------------------------------
// search / import / export
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReq {
    query: String,
    collection_id: Option<Uuid>,
    /// Search every workspace the user belongs to (session only: API keys stay in their workspace)
    #[serde(default)]
    all_workspaces: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(FromRow)]
struct SearchRow {
    #[sqlx(flatten)]
    document: DocRow,
    context: String,
    ranking: f32,
    collection_name: Option<String>,
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub async fn search(State(st): State<AppState>, auth: Auth, Json(b): Json<SearchReq>) -> ApiResult<Json<Value>> {
    let q = b.query.trim();
    if q.is_empty() {
        return ok(Vec::<Value>::new());
    }
    // API keys search the workspaces they are valid in
    let (teams, ids) = if b.all_workspaces {
        readable_collection_ids_all_teams(&st.db, &auth).await?
    } else {
        (vec![auth.team()?], readable_collection_ids(&st.db, &auth).await?)
    };
    let (limit, offset) = page(b.limit, b.offset);
    let access = access_sql(&auth, true);
    let sql = format!(
        "SELECT {DOC_COLUMNS}, \
           ts_headline('simple', d.text, q, 'StartSel=⟪, StopSel=⟫, MaxFragments=2, MinWords=8, MaxWords=24, FragmentDelimiter=\" … \"') AS context, \
           (ts_rank(d.search_vector, q) + CASE WHEN d.title ILIKE $6 THEN 1.0 ELSE 0.0 END)::real AS ranking, \
           (SELECT c.name FROM collections c WHERE c.id = d.collection_id) AS collection_name \
         {DOC_FROM} CROSS JOIN websearch_to_tsquery('simple', $4) AS q \
         WHERE {access} AND d.archived_at IS NULL AND d.template = false AND ($5::uuid IS NULL OR d.collection_id = $5) \
           AND (d.search_vector @@ q OR d.title ILIKE $6 OR d.text ILIKE $6) \
         ORDER BY ranking DESC, d.updated_at DESC LIMIT $7 OFFSET $8"
    );
    let rows: Vec<SearchRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(&teams)
        .bind(&ids)
        .bind(q)
        .bind(b.collection_id)
        .bind(format!("%{}%", escape_like(q)))
        .bind(limit)
        .bind(offset)
        .fetch_all(&st.db)
        .await?;
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({ "context": r.context, "ranking": r.ranking, "document": r.document, "collectionName": r.collection_name })
        })
        .collect();
    ok(out)
}

pub async fn export(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    if !st.settings().allow_markdown_export {
        return Err(ApiError::forbidden("Downloading documents has been disabled by the administrator"));
    }
    let (d, _) = load_doc(&st.db, &auth, &b.id, Access::Read).await?;
    let title = match &d.emoji {
        Some(e) => format!("{e} {}", d.title),
        None => d.title.clone(),
    };
    ok(format!("# {title}\n\n{}", d.text))
}

pub async fn import(State(st): State<AppState>, auth: Auth, mut mp: Multipart) -> ApiResult<Json<Value>> {
    let mut file: Option<(String, String)> = None;
    let mut collection_id = None;
    let mut parent_id = None;
    let mut publish = true;
    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let fname = field.file_name().unwrap_or("Untitled.md").to_string();
                let bytes = field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?;
                let text = String::from_utf8(bytes.to_vec())
                    .map_err(|_| ApiError::bad_request("Only UTF-8 Markdown or text files can be imported"))?;
                file = Some((fname, text));
            }
            "collectionId" => collection_id = Some(field.text().await.unwrap_or_default()),
            "parentDocumentId" => parent_id = Some(field.text().await.unwrap_or_default()),
            "publish" => publish = field.text().await.unwrap_or_default() != "false",
            _ => {}
        }
    }
    let (fname, mut text) = file.ok_or_else(|| ApiError::bad_request("file is required"))?;
    let mut title = fname
        .rsplit_once('.')
        .map(|(a, _)| a.to_string())
        .unwrap_or(fname.clone());
    let trimmed = text.trim_start().to_string();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let (first, remaining) = rest.split_once('\n').unwrap_or((rest, ""));
        title = first.trim().to_string();
        text = remaining.trim_start_matches('\n').to_string();
    }
    let d = create_doc(
        &st,
        &auth,
        NewDoc {
            title,
            text,
            emoji: None,
            collection_id,
            parent_document_id: parent_id,
            publish,
            template: false,
            template_id: None,
        },
    )
    .await?;
    ok(d)
}
