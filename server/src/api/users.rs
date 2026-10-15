use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::User,
    util::{hash_password, non_empty, valid_role, verify_password},
    AppState, CONTENT_WIDTHS, LANGUAGES, THEMES,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    id: Uuid,
    name: String,
    email: String,
    color: String,
    avatar_url: Option<String>,
    role: String,
    is_superadmin: bool,
    created_at: DateTime<Utc>,
    last_active_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Default)]
pub struct ListReq {
    query: Option<String>,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let q = b.query.unwrap_or_default();
    let members: Vec<Member> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.color, u.avatar_url, m.role, u.is_superadmin, m.created_at, u.last_active_at \
         FROM team_memberships m JOIN users u ON u.id = m.user_id WHERE m.team_id = $1 \
         AND ($2 = '' OR u.name ILIKE '%' || $2 || '%' OR u.email ILIKE '%' || $2 || '%') ORDER BY u.name",
    )
    .bind(auth.team()?)
    .bind(q.trim())
    .fetch_all(&st.db)
    .await?;
    ok(members)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReq {
    name: Option<String>,
    color: Option<String>,
    // preferences: "" resets to the installation default
    content_width: Option<String>,
    language: Option<String>,
    theme: Option<String>,
}

fn check_choice(v: &Option<String>, allowed: &[&str], what: &str) -> ApiResult<()> {
    match v.as_deref() {
        None | Some("") => Ok(()),
        Some(v) if allowed.contains(&v) => Ok(()),
        _ => Err(ApiError::bad_request(&format!("{what} must be one of: {}", allowed.join(", ")))),
    }
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let name = match &b.name {
        Some(n) => Some(non_empty(n, "Name")?),
        None => None,
    };
    check_choice(&b.content_width, CONTENT_WIDTHS, "Content width")?;
    check_choice(&b.language, LANGUAGES, "Language")?;
    check_choice(&b.theme, THEMES, "Theme")?;
    let u: User = sqlx::query_as(
        "UPDATE users SET name = COALESCE($2, name), color = COALESCE($3, color), \
         content_width = CASE WHEN $4::text IS NULL THEN content_width ELSE NULLIF($4, '') END, \
         language = CASE WHEN $5::text IS NULL THEN language ELSE NULLIF($5, '') END, \
         theme = CASE WHEN $6::text IS NULL THEN theme ELSE NULLIF($6, '') END, \
         updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(auth.user.id)
    .bind(name)
    .bind(&b.color)
    .bind(&b.content_width)
    .bind(&b.language)
    .bind(&b.theme)
    .fetch_one(&st.db)
    .await?;
    ok(u)
}

#[derive(Deserialize)]
pub struct RoleReq {
    id: Uuid,
    role: String,
}

async fn admin_count(st: &AppState, team: Uuid) -> ApiResult<i64> {
    let (c,): (i64,) = sqlx::query_as("SELECT count(*) FROM team_memberships WHERE team_id = $1 AND role = 'admin'")
        .bind(team)
        .fetch_one(&st.db)
        .await?;
    Ok(c)
}

pub async fn update_role(State(st): State<AppState>, auth: Auth, Json(b): Json<RoleReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    valid_role(&b.role)?;
    let team = auth.team()?;
    let current: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(b.id)
        .fetch_optional(&st.db)
        .await?;
    let current = current.ok_or_else(|| ApiError::not_found("User not found"))?.0;
    if current == "admin" && b.role != "admin" && admin_count(&st, team).await? <= 1 {
        return Err(ApiError::bad_request("A workspace must have at least one admin"));
    }
    sqlx::query("UPDATE team_memberships SET role = $3 WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(b.id)
        .bind(&b.role)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn remove(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let team = auth.team()?;
    // users may leave a workspace themselves; admins may remove others
    if b.id != auth.user.id {
        auth.require_admin()?;
    }
    let current: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(b.id)
        .fetch_optional(&st.db)
        .await?;
    let current = current.ok_or_else(|| ApiError::not_found("User not found"))?.0;
    if current == "admin" && admin_count(&st, team).await? <= 1 {
        return Err(ApiError::bad_request("A workspace must have at least one admin"));
    }
    let mut tx = st.db.begin().await?;
    sqlx::query("DELETE FROM team_memberships WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(b.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM collection_memberships WHERE user_id = $2 AND collection_id IN (SELECT id FROM collections WHERE team_id = $1)",
    )
    .bind(team)
    .bind(b.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE sessions SET team_id = NULL WHERE user_id = $2 AND team_id = $1")
        .bind(team)
        .bind(b.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM api_keys WHERE user_id = $2 AND team_id = $1")
        .bind(team)
        .bind(b.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordReq {
    #[serde(default)]
    current_password: String,
    new_password: String,
}

pub async fn change_password(State(st): State<AppState>, auth: Auth, Json(b): Json<PasswordReq>) -> ApiResult<Json<Value>> {
    if !st.cfg.password_login {
        return Err(ApiError::forbidden("Password sign-in is disabled, use Microsoft sign-in"));
    }
    // accounts created with SSO have no password yet: they can set one without the current password
    let has_password = !auth.user.password_hash.is_empty();
    if has_password && !verify_password(&b.current_password, &auth.user.password_hash) {
        return Err(ApiError::bad_request("Current password is incorrect"));
    }
    if b.new_password.len() < 8 {
        return Err(ApiError::bad_request("Password must be at least 8 characters"));
    }
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
        .bind(auth.user.id)
        .bind(hash_password(&b.new_password)?)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
