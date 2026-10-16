//! Groups of people in a workspace, modelled on Outline: workspace admins create and delete them;
//! workspace admins and the group's own admins rename them and manage their members. Every
//! member of the workspace can see them (to mention them or give them access to a collection).

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    util::non_empty,
    AppState,
};

const MAX_NAME: usize = 255;
const MAX_DESCRIPTION: usize = 1000;

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub description: String,
    pub disable_mentions: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub member_count: i64,
    /// the current user's permission in the group, None when not a member
    pub my_permission: Option<String>,
}

/// Binds: $1 current user id
const GROUP_SELECT: &str = "SELECT g.id, g.team_id, g.name, g.description, g.disable_mentions, g.created_at, g.updated_at, \
    (SELECT count(*) FROM group_users gu WHERE gu.group_id = g.id) AS member_count, \
    (SELECT gu.permission FROM group_users gu WHERE gu.group_id = g.id AND gu.user_id = $1) AS my_permission \
    FROM groups g";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupOut {
    #[serde(flatten)]
    group: GroupRow,
    /// the current user can rename the group and manage its members
    can_update: bool,
}

fn can_update(auth: &Auth, g: &GroupRow) -> bool {
    auth.key.is_none() && (auth.is_admin() || g.my_permission.as_deref() == Some("admin"))
}

fn out(auth: &Auth, g: GroupRow) -> GroupOut {
    GroupOut { can_update: can_update(auth, &g), group: g }
}

/// Group of the active workspace
async fn load(db: &PgPool, auth: &Auth, id: Uuid) -> ApiResult<GroupRow> {
    sqlx::query_as::<_, GroupRow>(&format!("{GROUP_SELECT} WHERE g.id = $2 AND g.team_id = $3"))
        .bind(auth.user.id)
        .bind(id)
        .bind(auth.team()?)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ApiError::not_found("Group not found"))
}

async fn load_for_update(db: &PgPool, auth: &Auth, id: Uuid) -> ApiResult<GroupRow> {
    let g = load(db, auth, id).await?;
    if !can_update(auth, &g) {
        return Err(ApiError::forbidden("Only workspace admins and group admins can change this group"));
    }
    Ok(g)
}

fn valid_name(name: &str) -> ApiResult<String> {
    let name = non_empty(name, "Name")?;
    if name.chars().count() > MAX_NAME {
        return Err(ApiError::bad_request(format!("Name must be {MAX_NAME} characters or less")));
    }
    Ok(name)
}

fn valid_description(d: &str) -> ApiResult<String> {
    let d = d.trim();
    if d.chars().count() > MAX_DESCRIPTION {
        return Err(ApiError::bad_request(format!("Description must be {MAX_DESCRIPTION} characters or less")));
    }
    Ok(d.to_string())
}

async fn check_unique(db: &PgPool, team: Uuid, name: &str, except: Option<Uuid>) -> ApiResult<()> {
    let taken: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM groups WHERE team_id = $1 AND lower(name) = lower($2) AND id IS DISTINCT FROM $3")
        .bind(team)
        .bind(name)
        .bind(except)
        .fetch_optional(db)
        .await?;
    if taken.is_some() {
        return Err(ApiError::bad_request("A group with this name already exists"));
    }
    Ok(())
}

#[derive(Deserialize, Default)]
pub struct ListReq {
    query: Option<String>,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let q = b.query.unwrap_or_default();
    let rows: Vec<GroupRow> = sqlx::query_as(&format!(
        "{GROUP_SELECT} WHERE g.team_id = $2 AND ($3 = '' OR g.name ILIKE '%' || $3 || '%') ORDER BY lower(g.name)"
    ))
    .bind(auth.user.id)
    .bind(auth.team()?)
    .bind(q.trim())
    .fetch_all(&st.db)
    .await?;
    ok(rows.into_iter().map(|g| out(&auth, g)).collect::<Vec<_>>())
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let g = load(&st.db, &auth, b.id).await?;
    ok(out(&auth, g))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReq {
    name: String,
    description: Option<String>,
    disable_mentions: Option<bool>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let team = auth.team()?;
    let name = valid_name(&b.name)?;
    let description = valid_description(b.description.as_deref().unwrap_or(""))?;
    check_unique(&st.db, team, &name, None).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO groups (id, team_id, name, description, disable_mentions, created_by) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(id)
        .bind(team)
        .bind(&name)
        .bind(&description)
        .bind(b.disable_mentions.unwrap_or(false))
        .bind(auth.user.id)
        .execute(&st.db)
        .await?;
    let g = load(&st.db, &auth, id).await?;
    ok(out(&auth, g))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReq {
    id: Uuid,
    name: Option<String>,
    description: Option<String>,
    disable_mentions: Option<bool>,
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let g = load_for_update(&st.db, &auth, b.id).await?;
    let name = match &b.name {
        Some(n) => {
            let n = valid_name(n)?;
            check_unique(&st.db, g.team_id, &n, Some(g.id)).await?;
            Some(n)
        }
        None => None,
    };
    let description = b.description.as_deref().map(valid_description).transpose()?;
    sqlx::query(
        "UPDATE groups SET name = COALESCE($2, name), description = COALESCE($3, description), \
         disable_mentions = COALESCE($4, disable_mentions), updated_at = now() WHERE id = $1",
    )
    .bind(g.id)
    .bind(name)
    .bind(description)
    .bind(b.disable_mentions)
    .execute(&st.db)
    .await?;
    let g = load(&st.db, &auth, g.id).await?;
    ok(out(&auth, g))
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let g = load(&st.db, &auth, b.id).await?;
    sqlx::query("DELETE FROM groups WHERE id = $1").bind(g.id).execute(&st.db).await?;
    ok(json!({ "success": true }))
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
    let g = load(&st.db, &auth, b.id).await?;
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.color, gu.permission, gu.created_at FROM group_users gu \
         JOIN users u ON u.id = gu.user_id WHERE gu.group_id = $1 ORDER BY lower(u.name)",
    )
    .bind(g.id)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUserReq {
    id: Uuid,
    user_id: Uuid,
    /// "member" (default) or "admin"; adding someone already in the group changes their permission
    permission: Option<String>,
}

pub async fn add_user(State(st): State<AppState>, auth: Auth, Json(b): Json<AddUserReq>) -> ApiResult<Json<Value>> {
    let g = load_for_update(&st.db, &auth, b.id).await?;
    let permission = b.permission.unwrap_or_else(|| "member".into());
    if permission != "member" && permission != "admin" {
        return Err(ApiError::bad_request("Invalid permission"));
    }
    let member: Option<(Uuid,)> = sqlx::query_as("SELECT user_id FROM team_memberships WHERE team_id = $1 AND user_id = $2")
        .bind(g.team_id)
        .bind(b.user_id)
        .fetch_optional(&st.db)
        .await?;
    if member.is_none() {
        return Err(ApiError::bad_request("User is not a member of this workspace"));
    }
    sqlx::query(
        "INSERT INTO group_users (group_id, user_id, permission, created_by) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (group_id, user_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(g.id)
    .bind(b.user_id)
    .bind(permission)
    .bind(auth.user.id)
    .execute(&st.db)
    .await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveUserReq {
    id: Uuid,
    user_id: Uuid,
}

pub async fn remove_user(State(st): State<AppState>, auth: Auth, Json(b): Json<RemoveUserReq>) -> ApiResult<Json<Value>> {
    let g = load_for_update(&st.db, &auth, b.id).await?;
    sqlx::query("DELETE FROM group_users WHERE group_id = $1 AND user_id = $2")
        .bind(g.id)
        .bind(b.user_id)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
