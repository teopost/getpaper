use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::User,
    util::{random_string, valid_role, validate_email},
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    id: Uuid,
    team_id: Uuid,
    email: String,
    role: String,
    token: String,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateReq {
    email: String,
    role: Option<String>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let team = auth.team()?;
    let email = validate_email(&b.email)?;
    let role = b.role.unwrap_or_else(|| "member".into());
    valid_role(&role)?;
    if role == "admin" && !auth.is_admin() {
        return Err(ApiError::forbidden("Only admins can invite other admins"));
    }

    // Existing user: add them to the workspace directly
    let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE lower(email) = $1")
        .bind(&email)
        .fetch_optional(&st.db)
        .await?;
    if let Some(u) = existing {
        let res = sqlx::query(
            "INSERT INTO team_memberships (team_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(team)
        .bind(u.id)
        .bind(&role)
        .execute(&st.db)
        .await?;
        return ok(json!({
            "added": res.rows_affected() > 0,
            "alreadyMember": res.rows_affected() == 0,
            "user": { "id": u.id, "name": u.name, "email": u.email },
        }));
    }

    let invite: Invite = sqlx::query_as(
        "INSERT INTO invites (id, team_id, email, role, token, created_by) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(team)
    .bind(&email)
    .bind(&role)
    .bind(random_string(32))
    .bind(auth.user.id)
    .fetch_one(&st.db)
    .await?;
    let link = format!("{}/invite/{}", st.cfg.url.trim_end_matches('/'), invite.token);
    ok(json!({ "added": false, "invite": invite, "link": link }))
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let invites: Vec<Invite> = sqlx::query_as(
        "SELECT id, team_id, email, role, token, created_by, created_at FROM invites \
         WHERE team_id = $1 AND accepted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(auth.team()?)
    .fetch_all(&st.db)
    .await?;
    let base = st.cfg.url.trim_end_matches('/').to_string();
    let out: Vec<Value> = invites
        .into_iter()
        .map(|i| {
            let link = format!("{base}/invite/{}", i.token);
            let mut v = serde_json::to_value(&i).unwrap();
            v["link"] = json!(link);
            v
        })
        .collect();
    ok(out)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn revoke(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    sqlx::query("DELETE FROM invites WHERE id = $1 AND team_id = $2 AND accepted_at IS NULL")
        .bind(b.id)
        .bind(auth.team()?)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
pub struct TokenReq {
    token: String,
}

pub async fn info(State(st): State<AppState>, Json(b): Json<TokenReq>) -> ApiResult<Json<Value>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT i.email, i.role, t.name, u.name FROM invites i JOIN teams t ON t.id = i.team_id \
         JOIN users u ON u.id = i.created_by WHERE i.token = $1 AND i.accepted_at IS NULL",
    )
    .bind(&b.token)
    .fetch_optional(&st.db)
    .await?;
    let (email, role, team_name, inviter) = row.ok_or_else(|| ApiError::not_found("This invite is invalid or was already used"))?;
    ok(json!({ "email": email, "role": role, "teamName": team_name, "inviterName": inviter }))
}

/// Accept an invite on behalf of `user`, returning the team id joined.
pub async fn accept_invite_internal(db: &PgPool, user: &User, token: &str) -> ApiResult<Uuid> {
    let mut tx = db.begin().await?;
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "UPDATE invites SET accepted_at = now(), accepted_by = $2 WHERE token = $1 AND accepted_at IS NULL \
         RETURNING id, team_id, role",
    )
    .bind(token)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let (_, team_id, role) = row.ok_or_else(|| ApiError::not_found("This invite is invalid or was already used"))?;
    sqlx::query("INSERT INTO team_memberships (team_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(team_id)
        .bind(user.id)
        .bind(role)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(team_id)
}

pub async fn accept(State(st): State<AppState>, auth: Auth, Json(b): Json<TokenReq>) -> ApiResult<Json<Value>> {
    let team_id = accept_invite_internal(&st.db, &auth.user, &b.token).await?;
    if let Some(h) = &auth.session_hash {
        sqlx::query("UPDATE sessions SET team_id = $1 WHERE token_hash = $2")
            .bind(team_id)
            .bind(h)
            .execute(&st.db)
            .await?;
    }
    ok(json!({ "teamId": team_id }))
}
