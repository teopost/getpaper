use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::Access,
    perm::load_doc,
    util::non_empty,
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct CommentRow {
    id: Uuid,
    document_id: Uuid,
    parent_comment_id: Option<Uuid>,
    text: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    created_by: Uuid,
    created_by_name: String,
    created_by_color: String,
    /// the commented passage, on the first comment of a thread
    anchor_text: Option<String>,
    anchor_prefix: Option<String>,
    anchor_suffix: Option<String>,
}

const COMMENT_SELECT: &str = "SELECT c.id, c.document_id, c.parent_comment_id, c.text, c.created_at, c.updated_at, \
    c.resolved_at, c.created_by, u.name AS created_by_name, u.color AS created_by_color, \
    c.anchor_text, c.anchor_prefix, c.anchor_suffix \
    FROM comments c JOIN users u ON u.id = c.created_by";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReq {
    document_id: String,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let rows: Vec<CommentRow> = sqlx::query_as(&format!("{COMMENT_SELECT} WHERE c.document_id = $1 ORDER BY c.created_at"))
        .bind(d.id)
        .fetch_all(&st.db)
        .await?;
    ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReq {
    document_id: String,
    text: String,
    parent_comment_id: Option<Uuid>,
    /// the passage of the text the comment is about (first comment of a thread only)
    anchor: Option<Anchor>,
}

#[derive(Deserialize)]
pub struct Anchor {
    text: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    suffix: String,
}

/// at most `n` characters, for the stored anchor
fn clip(s: &str, n: usize, from_end: bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else if from_end {
        chars[chars.len() - n..].iter().collect()
    } else {
        chars[..n].iter().collect()
    }
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let text = non_empty(&b.text, "Comment")?;
    let anchor = b.anchor.filter(|a| b.parent_comment_id.is_none() && !a.text.trim().is_empty());
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO comments (id, document_id, parent_comment_id, created_by, text, anchor_text, anchor_prefix, anchor_suffix) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(d.id)
    .bind(b.parent_comment_id)
    .bind(auth.user.id)
    .bind(&text)
    .bind(anchor.as_ref().map(|a| clip(&a.text, 1000, false)))
    .bind(anchor.as_ref().map(|a| clip(&a.prefix, 40, true)))
    .bind(anchor.as_ref().map(|a| clip(&a.suffix, 40, false)))
    .execute(&st.db)
    .await?;
    let c: CommentRow = sqlx::query_as(&format!("{COMMENT_SELECT} WHERE c.id = $1"))
        .bind(id)
        .fetch_one(&st.db)
        .await?;
    let thread = b.parent_comment_id.unwrap_or(id);
    crate::notify::comment_created(&st, &auth.user, &d, id, thread, text.as_str());
    crate::webhooks::dispatch(&st, &auth, d.team_id, "comments.create", crate::audit::Target::document(d.id, &d.title), json!({ "commentId": id, "parentCommentId": b.parent_comment_id, "text": text }));
    ok(c)
}

async fn load_comment(st: &AppState, auth: &Auth, id: Uuid) -> ApiResult<(Uuid, Access)> {
    let (created_by, access, _) = load_comment_doc(st, auth, id).await?;
    Ok((created_by, access))
}

async fn load_comment_doc(st: &AppState, auth: &Auth, id: Uuid) -> ApiResult<(Uuid, Access, crate::models::DocRow)> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as("SELECT document_id, created_by FROM comments WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    let (doc_id, created_by) = row.ok_or_else(|| ApiError::not_found("Comment not found"))?;
    let (d, access) = load_doc(&st.db, auth, &doc_id.to_string(), Access::Read).await?;
    Ok((created_by, access, d))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    id: Uuid,
    text: String,
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let (created_by, _) = load_comment(&st, &auth, b.id).await?;
    if created_by != auth.user.id {
        return Err(ApiError::forbidden("You can only edit your own comments"));
    }
    let text = non_empty(&b.text, "Comment")?;
    sqlx::query("UPDATE comments SET text = $2, updated_at = now() WHERE id = $1")
        .bind(b.id)
        .bind(text)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (created_by, _) = load_comment(&st, &auth, b.id).await?;
    if created_by != auth.user.id && !auth.is_admin() {
        return Err(ApiError::forbidden("You can only delete your own comments"));
    }
    sqlx::query("DELETE FROM comments WHERE id = $1").bind(b.id).execute(&st.db).await?;
    ok(json!({ "success": true }))
}

pub async fn resolve(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (created_by, access, d) = load_comment_doc(&st, &auth, b.id).await?;
    if created_by != auth.user.id && access != Access::Write {
        return Err(ApiError::forbidden("You cannot resolve this comment"));
    }
    let (resolved,): (bool,) = sqlx::query_as(
        "UPDATE comments SET resolved_at = CASE WHEN resolved_at IS NULL THEN now() ELSE NULL END, \
         resolved_by = CASE WHEN resolved_at IS NULL THEN $2 ELSE NULL END WHERE id = $1 RETURNING resolved_at IS NOT NULL",
    )
    .bind(b.id)
    .bind(auth.user.id)
    .fetch_one(&st.db)
    .await?;
    if resolved {
        crate::notify::comment_resolved(&st, &auth.user, &d, b.id);
        crate::webhooks::dispatch(&st, &auth, d.team_id, "comments.resolve", crate::audit::Target::document(d.id, &d.title), json!({ "commentId": b.id }));
    }
    ok(json!({ "success": true }))
}
