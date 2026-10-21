//! Sharing a document with people and groups of the workspace beyond its collection, as in
//! Outline. The permission ("read" or "read_write") applies to the nested documents too; it adds to
//! the access people already have through the collection (see `perm::doc_access`).

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Access, DocRow, DOC_COLUMNS, DOC_FROM},
    perm::{load_doc, readable_collection_ids},
    AppState,
};

/// The parents of a document with their distance, the document itself first (depth 0)
const ANCESTORS: &str = "WITH RECURSIVE up AS ( \
     SELECT id, parent_document_id, title, 0 AS depth FROM documents WHERE id = $1 \
     UNION ALL SELECT d.id, d.parent_document_id, d.title, up.depth + 1 FROM documents d JOIN up ON d.id = up.parent_document_id \
     WHERE up.depth < 100)";

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct UserGrant {
    id: Uuid,
    name: String,
    email: String,
    color: String,
    permission: String,
    /// given on a parent document: changed there
    inherited_from_id: Option<Uuid>,
    inherited_from_title: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct GroupGrant {
    id: Uuid,
    name: String,
    member_count: i64,
    permission: String,
    inherited_from_id: Option<Uuid>,
    inherited_from_title: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct IdReq {
    id: String,
}

/// People and groups the document is shared with, also through its parents.
pub async fn memberships(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (d, access) = load_doc(&st.db, &auth, &b.id, Access::Read).await?;
    let users: Vec<UserGrant> = sqlx::query_as(&format!(
        "{ANCESTORS} SELECT u.id, u.name, u.email, u.color, m.permission, \
         CASE WHEN up.depth > 0 THEN up.id END AS inherited_from_id, CASE WHEN up.depth > 0 THEN up.title END AS inherited_from_title, \
         m.created_at FROM up JOIN document_memberships m ON m.document_id = up.id JOIN users u ON u.id = m.user_id \
         ORDER BY up.depth, lower(u.name)"
    ))
    .bind(d.id)
    .fetch_all(&st.db)
    .await?;
    let groups: Vec<GroupGrant> = sqlx::query_as(&format!(
        "{ANCESTORS} SELECT g.id, g.name, (SELECT count(*) FROM group_users gu WHERE gu.group_id = g.id) AS member_count, m.permission, \
         CASE WHEN up.depth > 0 THEN up.id END AS inherited_from_id, CASE WHEN up.depth > 0 THEN up.title END AS inherited_from_title, \
         m.created_at FROM up JOIN document_group_memberships m ON m.document_id = up.id JOIN groups g ON g.id = m.group_id \
         ORDER BY up.depth, lower(g.name)"
    ))
    .bind(d.id)
    .fetch_all(&st.db)
    .await?;
    ok(json!({ "users": users, "groups": groups, "canManage": access == Access::Write && d.published_at.is_some() }))
}

/// The document, if the signed-in user can share it: published, and editable by them
async fn shareable(st: &AppState, auth: &Auth, id: &str) -> ApiResult<DocRow> {
    auth.require_writer()?;
    let (d, _) = load_doc(&st.db, auth, id, Access::Write).await?;
    if d.published_at.is_none() || d.template {
        return Err(ApiError::bad_request("Publish the document before sharing it with people"));
    }
    Ok(d)
}

fn valid_permission(p: Option<String>) -> ApiResult<String> {
    let p = p.unwrap_or_else(|| "read_write".into());
    if p != "read" && p != "read_write" {
        return Err(ApiError::bad_request("Invalid permission"));
    }
    Ok(p)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserReq {
    id: String,
    user_id: Uuid,
    permission: Option<String>,
}

pub async fn add_user(State(st): State<AppState>, auth: Auth, Json(b): Json<UserReq>) -> ApiResult<Json<Value>> {
    let d = shareable(&st, &auth, &b.id).await?;
    let permission = valid_permission(b.permission)?;
    let member: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM team_memberships WHERE team_id = $1 AND user_id = $2 AND suspended_at IS NULL")
            .bind(d.team_id)
            .bind(b.user_id)
            .fetch_optional(&st.db)
            .await?;
    if member.is_none() {
        return Err(ApiError::bad_request("User is not a member of this workspace"));
    }
    sqlx::query(
        "INSERT INTO document_memberships (document_id, user_id, permission, created_by) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (document_id, user_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(d.id)
    .bind(b.user_id)
    .bind(&permission)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    let user = Target::user_id(&st.db, b.user_id).await.map(|t| t.name);
    audit::record_in(&st, &auth, Some(d.team_id), "documents.add_user", Target::document(d.id, &d.title), json!({ "user": user, "permission": permission })).await;
    crate::subscriptions::notify_shared(&st, &auth.user, &d, b.user_id).await;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveUserReq {
    id: String,
    user_id: Uuid,
}

pub async fn remove_user(State(st): State<AppState>, auth: Auth, Json(b): Json<RemoveUserReq>) -> ApiResult<Json<Value>> {
    let d = shareable(&st, &auth, &b.id).await?;
    let r = sqlx::query("DELETE FROM document_memberships WHERE document_id = $1 AND user_id = $2")
        .bind(d.id)
        .bind(b.user_id)
        .execute(&st.db)
        .await?;
    if r.rows_affected() > 0 {
        let user = Target::user_id(&st.db, b.user_id).await.map(|t| t.name);
        audit::record_in(&st, &auth, Some(d.team_id), "documents.remove_user", Target::document(d.id, &d.title), json!({ "user": user })).await;
    }
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupReq {
    id: String,
    group_id: Uuid,
    permission: Option<String>,
}

pub async fn add_group(State(st): State<AppState>, auth: Auth, Json(b): Json<GroupReq>) -> ApiResult<Json<Value>> {
    let d = shareable(&st, &auth, &b.id).await?;
    let permission = valid_permission(b.permission)?;
    let group: Option<(String,)> = sqlx::query_as("SELECT name FROM groups WHERE id = $1 AND team_id = $2")
        .bind(b.group_id)
        .bind(d.team_id)
        .fetch_optional(&st.db)
        .await?;
    let (name,) = group.ok_or_else(|| ApiError::not_found("Group not found"))?;
    sqlx::query(
        "INSERT INTO document_group_memberships (document_id, group_id, permission, created_by) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (document_id, group_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(d.id)
    .bind(b.group_id)
    .bind(&permission)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    audit::record_in(&st, &auth, Some(d.team_id), "documents.add_group", Target::document(d.id, &d.title), json!({ "group": name, "permission": permission })).await;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveGroupReq {
    id: String,
    group_id: Uuid,
}

pub async fn remove_group(State(st): State<AppState>, auth: Auth, Json(b): Json<RemoveGroupReq>) -> ApiResult<Json<Value>> {
    let d = shareable(&st, &auth, &b.id).await?;
    let removed: Option<(String,)> = sqlx::query_as(
        "DELETE FROM document_group_memberships m USING groups g WHERE m.document_id = $1 AND m.group_id = $2 AND g.id = m.group_id RETURNING g.name",
    )
    .bind(d.id)
    .bind(b.group_id)
    .fetch_optional(&st.db)
    .await?;
    if let Some((name,)) = removed {
        audit::record_in(&st, &auth, Some(d.team_id), "documents.remove_group", Target::document(d.id, &d.title), json!({ "group": name })).await;
    }
    ok(json!({ "success": true }))
}

/// Documents shared with the user (or a group of theirs) in the current workspace that they
/// can't already read through the collection: the "Shared with me" section of the sidebar.
pub async fn shared_with_me(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    if auth.key.as_ref().is_some_and(|k| k.collections.is_some()) {
        return ok(Vec::<DocRow>::new());
    }
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let sql = format!(
        "SELECT {DOC_COLUMNS} {DOC_FROM} \
         WHERE d.team_id = $2 AND d.deleted_at IS NULL AND d.archived_at IS NULL AND d.published_at IS NOT NULL \
         AND NOT (d.collection_id = ANY($3)) \
         AND d.id IN (SELECT document_id FROM document_grants WHERE user_id = $1) \
         ORDER BY d.title"
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql).bind(auth.user.id).bind(auth.team()?).bind(&ids).fetch_all(&st.db).await?;
    ok(docs)
}
