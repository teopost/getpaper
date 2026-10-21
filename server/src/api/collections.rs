use std::collections::{HashMap, HashSet};

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
    models::{Access, Collection},
    perm::{compute_collection_access, load_collection},
    util::{new_url_id, non_empty},
    AppState,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionOut {
    #[serde(flatten)]
    c: Collection,
    access: &'static str,
}

fn out(c: Collection, a: Access) -> CollectionOut {
    CollectionOut { c, access: a.as_str() }
}

/// "private" (or "") => NULL, otherwise "read" / "read_write"
fn parse_permission(p: &str) -> ApiResult<Option<String>> {
    match p {
        "" | "private" => Ok(None),
        "read" | "read_write" => Ok(Some(p.to_string())),
        _ => Err(ApiError::bad_request("Invalid permission")),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReq {
    team_id: Uuid,
}

/// Collections of another workspace of the user (e.g. targets when moving a document).
pub async fn list_for_workspace(State(st): State<AppState>, auth: Auth, Json(b): Json<WorkspaceReq>) -> ApiResult<Json<Value>> {
    let a = auth.for_team(&st.db, b.team_id).await?;
    list(State(st), a).await
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let team = auth.team()?;
    let cols: Vec<Collection> =
        sqlx::query_as("SELECT * FROM collections WHERE team_id = $1 AND deleted_at IS NULL ORDER BY sort_index, name")
            .bind(team)
            .fetch_all(&st.db)
            .await?;
    let mems: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT cm.collection_id, max(cm.permission) FROM collection_grants cm JOIN collections c ON c.id = cm.collection_id \
         WHERE cm.user_id = $1 AND c.team_id = $2 GROUP BY cm.collection_id",
    )
    .bind(auth.user.id)
    .bind(team)
    .fetch_all(&st.db)
    .await?;
    let mems: HashMap<Uuid, String> = mems.into_iter().collect();
    let result: Vec<CollectionOut> = cols
        .into_iter()
        // an API key limited to some collections lists only those
        .filter(|c| auth.key.as_ref().map_or(true, |k| k.allows_collection(c.id)))
        .filter_map(|c| {
            let a = compute_collection_access(auth.role(), &c, mems.get(&c.id).map(|s| s.as_str()));
            if a == Access::None {
                None
            } else {
                Some(out(c, a))
            }
        })
        .collect();
    ok(result)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: String,
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, a) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    ok(out(c, a))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    permission: Option<String>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    if auth.key.as_ref().is_some_and(|k| k.collections.is_some()) {
        return Err(ApiError::forbidden("This API key is limited to some collections and can't create new ones"));
    }
    let team = auth.team()?;
    let name = non_empty(&b.name, "Name")?;
    let permission = parse_permission(b.permission.as_deref().unwrap_or("read_write"))?;
    let (max,): (Option<f64>,) = sqlx::query_as("SELECT max(sort_index) FROM collections WHERE team_id = $1")
        .bind(team)
        .fetch_one(&st.db)
        .await?;
    let c: Collection = sqlx::query_as(
        "INSERT INTO collections (id, url_id, team_id, name, description, color, icon, permission, sort_index, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(new_url_id())
    .bind(team)
    .bind(&name)
    .bind(b.description.unwrap_or_default())
    .bind(b.color.unwrap_or_else(|| "#4E5C6E".into()))
    .bind(b.icon)
    .bind(&permission)
    .bind(max.unwrap_or(0.0) + 1.0)
    .bind(auth.user.id)
    .fetch_one(&st.db)
    .await?;
    if permission.is_none() {
        sqlx::query("INSERT INTO collection_memberships (collection_id, user_id, permission) VALUES ($1, $2, 'read_write')")
            .bind(c.id)
            .bind(auth.user.id)
            .execute(&st.db)
            .await?;
    }
    audit::record(&st, &auth, "collections.create", Target::collection(c.id, &c.name), json!({ "permission": c.permission })).await;
    ok(out(c, Access::Write))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    id: String,
    name: Option<String>,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    permission: Option<String>,
    sharing: Option<bool>,
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let (c, a) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    let name = match &b.name {
        Some(n) => Some(non_empty(n, "Name")?),
        None => None,
    };
    let (set_perm, perm) = match &b.permission {
        Some(p) => (true, parse_permission(p)?),
        None => (false, None),
    };
    let before = (c.name.clone(), c.permission.clone(), c.sharing);
    let c: Collection = sqlx::query_as(
        "UPDATE collections SET name = COALESCE($2, name), description = COALESCE($3, description), \
         color = COALESCE($4, color), icon = COALESCE($5, icon), sharing = COALESCE($6, sharing), \
         permission = CASE WHEN $7 THEN $8 ELSE permission END, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(c.id)
    .bind(name)
    .bind(&b.description)
    .bind(&b.color)
    .bind(&b.icon)
    .bind(b.sharing)
    .bind(set_perm)
    .bind(perm)
    .fetch_one(&st.db)
    .await?;
    if set_perm && c.permission.is_none() {
        // make sure the editor keeps access to a newly-private collection
        sqlx::query(
            "INSERT INTO collection_memberships (collection_id, user_id, permission) VALUES ($1, $2, 'read_write') ON CONFLICT DO NOTHING",
        )
        .bind(c.id)
        .bind(auth.user.id)
        .execute(&st.db)
        .await?;
    }
    // only what matters for access and finding it again: name, permission, public sharing
    let mut changes = serde_json::Map::new();
    if before.0 != c.name {
        changes.insert("name".into(), json!({ "from": before.0, "to": c.name }));
    }
    if before.1 != c.permission {
        changes.insert("permission".into(), json!({ "from": before.1, "to": c.permission }));
    }
    if before.2 != c.sharing {
        changes.insert("sharing".into(), json!({ "from": before.2, "to": c.sharing }));
    }
    if !changes.is_empty() {
        let name = if before.1 != c.permission { "collections.permission" } else { "collections.update" };
        audit::record(&st, &auth, name, Target::collection(c.id, &c.name), Value::Object(changes)).await;
    }
    ok(out(c, a))
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    if !(auth.is_admin() || c.created_by == Some(auth.user.id)) {
        return Err(ApiError::forbidden("Only admins or the creator can delete a collection"));
    }
    let mut tx = st.db.begin().await?;
    sqlx::query("UPDATE collections SET deleted_at = now() WHERE id = $1").bind(c.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE documents SET deleted_at = now() WHERE collection_id = $1 AND deleted_at IS NULL")
        .bind(c.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit::record(&st, &auth, "collections.delete", Target::collection(c.id, &c.name), json!({})).await;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
pub struct MoveReq {
    id: String,
    index: usize,
}

pub async fn move_collection(State(st): State<AppState>, auth: Auth, Json(b): Json<MoveReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    let ids: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM collections WHERE team_id = $1 AND deleted_at IS NULL AND id <> $2 ORDER BY sort_index, name")
            .bind(c.team_id)
            .bind(c.id)
            .fetch_all(&st.db)
            .await?;
    let mut ids: Vec<Uuid> = ids.into_iter().map(|r| r.0).collect();
    let idx = b.index.min(ids.len());
    ids.insert(idx, c.id);
    let mut tx = st.db.begin().await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE collections SET sort_index = $2 WHERE id = $1")
            .bind(id)
            .bind(i as f64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    ok(json!({ "success": true }))
}

#[derive(FromRow, Clone)]
struct NodeRow {
    id: Uuid,
    url_id: String,
    title: String,
    emoji: Option<String>,
    parent_document_id: Option<Uuid>,
    /// when the active public link was created, None without one
    shared_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    id: Uuid,
    url_id: String,
    title: String,
    emoji: Option<String>,
    /// readable on the internet through a public link (/s/…) that currently works
    shared_at: Option<DateTime<Utc>>,
    children: Vec<Node>,
}

fn build(parent: Option<Uuid>, by_parent: &HashMap<Option<Uuid>, Vec<NodeRow>>, depth: usize) -> Vec<Node> {
    if depth > 50 {
        return vec![];
    }
    by_parent
        .get(&parent)
        .map(|rows| {
            rows.iter()
                .map(|r| Node {
                    id: r.id,
                    url_id: r.url_id.clone(),
                    title: r.title.clone(),
                    emoji: r.emoji.clone(),
                    shared_at: r.shared_at,
                    children: build(Some(r.id), by_parent, depth + 1),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn documents(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    // public links work only while sharing is allowed by the installation, the workspace and the collection
    let sharing = crate::api::shares::sharing_enabled(&st, c.team_id, Some(c.id)).await?;
    let rows: Vec<NodeRow> = sqlx::query_as(
        "SELECT d.id, d.url_id, d.title, d.emoji, d.parent_document_id, \
         CASE WHEN $2 THEN (SELECT min(s.created_at) FROM shares s WHERE s.document_id = d.id AND s.revoked_at IS NULL) END AS shared_at \
         FROM documents d WHERE d.collection_id = $1 \
         AND d.published_at IS NOT NULL AND d.deleted_at IS NULL AND d.archived_at IS NULL AND d.template = false \
         ORDER BY d.position, d.title",
    )
    .bind(c.id)
    .bind(sharing)
    .fetch_all(&st.db)
    .await?;
    let ids: HashSet<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut by_parent: HashMap<Option<Uuid>, Vec<NodeRow>> = HashMap::new();
    for r in rows {
        let key = r.parent_document_id.filter(|p| ids.contains(p));
        by_parent.entry(key).or_default().push(r);
    }
    ok(build(None, &by_parent, 0))
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct MemberRow {
    id: Uuid,
    name: String,
    email: String,
    color: String,
    permission: String,
    created_at: DateTime<Utc>,
}

pub async fn memberships(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.color, cm.permission, cm.created_at FROM collection_memberships cm \
         JOIN users u ON u.id = cm.user_id WHERE cm.collection_id = $1 ORDER BY u.name",
    )
    .bind(c.id)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUserReq {
    id: String,
    user_id: Uuid,
    permission: Option<String>,
}

pub async fn add_user(State(st): State<AppState>, auth: Auth, Json(b): Json<AddUserReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    let perm = b.permission.unwrap_or_else(|| "read_write".into());
    if perm != "read" && perm != "read_write" {
        return Err(ApiError::bad_request("Invalid permission"));
    }
    let member: Option<(Uuid,)> = sqlx::query_as("SELECT user_id FROM team_memberships WHERE team_id = $1 AND user_id = $2")
        .bind(c.team_id)
        .bind(b.user_id)
        .fetch_optional(&st.db)
        .await?;
    if member.is_none() {
        return Err(ApiError::bad_request("User is not a member of this workspace"));
    }
    sqlx::query(
        "INSERT INTO collection_memberships (collection_id, user_id, permission) VALUES ($1, $2, $3) \
         ON CONFLICT (collection_id, user_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(c.id)
    .bind(b.user_id)
    .bind(&perm)
    .execute(&st.db)
    .await?;
    let user = Target::user_id(&st.db, b.user_id).await.map(|t| t.name);
    audit::record(&st, &auth, "collections.add_user", Target::collection(c.id, &c.name), json!({ "user": user, "permission": perm })).await;
    crate::notify::collection_added(&st, &auth.user, c.team_id, c.id, b.user_id);
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveUserReq {
    id: String,
    user_id: Uuid,
}

pub async fn remove_user(State(st): State<AppState>, auth: Auth, Json(b): Json<RemoveUserReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    sqlx::query("DELETE FROM collection_memberships WHERE collection_id = $1 AND user_id = $2")
        .bind(c.id)
        .bind(b.user_id)
        .execute(&st.db)
        .await?;
    let user = Target::user_id(&st.db, b.user_id).await.map(|t| t.name);
    audit::record(&st, &auth, "collections.remove_user", Target::collection(c.id, &c.name), json!({ "user": user })).await;
    ok(json!({ "success": true }))
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct GroupMemberRow {
    id: Uuid,
    name: String,
    member_count: i64,
    permission: String,
    created_at: DateTime<Utc>,
}

/// Groups with access to the collection
pub async fn group_memberships(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    let rows: Vec<GroupMemberRow> = sqlx::query_as(
        "SELECT g.id, g.name, (SELECT count(*) FROM group_users gu WHERE gu.group_id = g.id) AS member_count, \
         cg.permission, cg.created_at FROM collection_group_memberships cg JOIN groups g ON g.id = cg.group_id \
         WHERE cg.collection_id = $1 ORDER BY lower(g.name)",
    )
    .bind(c.id)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddGroupReq {
    id: String,
    group_id: Uuid,
    permission: Option<String>,
}

pub async fn add_group(State(st): State<AppState>, auth: Auth, Json(b): Json<AddGroupReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    let perm = b.permission.unwrap_or_else(|| "read_write".into());
    if perm != "read" && perm != "read_write" {
        return Err(ApiError::bad_request("Invalid permission"));
    }
    let group: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM groups WHERE id = $1 AND team_id = $2")
        .bind(b.group_id)
        .bind(c.team_id)
        .fetch_optional(&st.db)
        .await?;
    if group.is_none() {
        return Err(ApiError::not_found("Group not found"));
    }
    sqlx::query(
        "INSERT INTO collection_group_memberships (collection_id, group_id, permission) VALUES ($1, $2, $3) \
         ON CONFLICT (collection_id, group_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(c.id)
    .bind(b.group_id)
    .bind(&perm)
    .execute(&st.db)
    .await?;
    let group = Target::group_id(&st.db, b.group_id).await.map(|t| t.name);
    audit::record(&st, &auth, "collections.add_group", Target::collection(c.id, &c.name), json!({ "group": group, "permission": perm })).await;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveGroupReq {
    id: String,
    group_id: Uuid,
}

pub async fn remove_group(State(st): State<AppState>, auth: Auth, Json(b): Json<RemoveGroupReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Write).await?;
    sqlx::query("DELETE FROM collection_group_memberships WHERE collection_id = $1 AND group_id = $2")
        .bind(c.id)
        .bind(b.group_id)
        .execute(&st.db)
        .await?;
    let group = Target::group_id(&st.db, b.group_id).await.map(|t| t.name);
    audit::record(&st, &auth, "collections.remove_group", Target::collection(c.id, &c.name), json!({ "group": group })).await;
    ok(json!({ "success": true }))
}

pub async fn export(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let (c, _) = load_collection(&st.db, &auth, &b.id, Access::Read).await?;
    let docs: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT title, emoji, text FROM documents WHERE collection_id = $1 AND published_at IS NOT NULL \
         AND deleted_at IS NULL AND archived_at IS NULL AND template = false ORDER BY position, title",
    )
    .bind(c.id)
    .fetch_all(&st.db)
    .await?;
    let count = docs.len();
    let mut md = format!("# {}\n\n{}\n\n", c.name, c.description);
    for (title, emoji, text) in docs {
        let t = match emoji {
            Some(e) => format!("{e} {title}"),
            None => title,
        };
        md.push_str(&format!("---\n\n## {t}\n\n{}\n\n", crate::html_md::html_tables_to_gfm(&text)));
    }
    audit::record(&st, &auth, "collections.export", Target::collection(c.id, &c.name), json!({ "documents": count })).await;
    ok(json!({ "name": c.name, "markdown": md }))
}
