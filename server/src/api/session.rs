use axum::{
    extract::State,
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    api::{invites::accept_invite_internal, teams::create_team},
    auth::{clear_cookie, session_cookie, Auth},
    error::{ok, ApiError, ApiResult},
    models::{Team, User},
    util::{hash_password, non_empty, random_color, random_string, sha256_hex, validate_email, verify_password},
    AppState,
};

pub async fn create_session(db: &PgPool, user_id: Uuid, team_id: Option<Uuid>) -> ApiResult<String> {
    let token = random_string(48);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, team_id, expires_at) VALUES ($1, $2, $3, now() + interval '30 days')",
    )
    .bind(sha256_hex(&token))
    .bind(user_id)
    .bind(team_id)
    .execute(db)
    .await?;
    Ok(token)
}

fn with_cookie(cookie: String, body: Value) -> Response {
    ([(SET_COOKIE, cookie)], Json(body)).into_response()
}

pub async fn config(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users").fetch_one(&st.db).await?;
    ok(json!({
        "allowSignup": st.cfg.password_login && (st.cfg.allow_signup || count == 0),
        "isFirstUser": count == 0,
        "passwordLogin": st.cfg.password_login,
        "microsoft": st.cfg.microsoft.is_some(),
    }))
}

#[derive(Deserialize)]
pub struct LoginReq {
    email: String,
    password: String,
}

pub async fn login(State(st): State<AppState>, Json(b): Json<LoginReq>) -> ApiResult<Response> {
    if !st.cfg.password_login {
        return Err(ApiError::forbidden("Password sign-in is disabled, use Microsoft sign-in"));
    }
    let email = b.email.trim().to_lowercase();
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE lower(email) = $1")
        .bind(&email)
        .fetch_optional(&st.db)
        .await?;
    let user = match user {
        Some(u) if verify_password(&b.password, &u.password_hash) => u,
        _ => {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "authentication_failed",
                "Invalid email or password",
            ))
        }
    };
    let token = create_session(&st.db, user.id, None).await?;
    Ok(with_cookie(
        session_cookie(&token, st.cfg.secure_cookies),
        json!({ "ok": true, "data": { "user": user } }),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterReq {
    name: String,
    email: String,
    password: String,
    team_name: Option<String>,
    invite_token: Option<String>,
}

pub async fn register(State(st): State<AppState>, Json(b): Json<RegisterReq>) -> ApiResult<Response> {
    if !st.cfg.password_login {
        return Err(ApiError::forbidden("Password sign-up is disabled, use Microsoft sign-in"));
    }
    let name = non_empty(&b.name, "Name")?;
    let email = validate_email(&b.email)?;
    if b.password.len() < 8 {
        return Err(ApiError::bad_request("Password must be at least 8 characters"));
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users").fetch_one(&st.db).await?;
    let invite_valid = match &b.invite_token {
        Some(t) => {
            let r: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM invites WHERE token = $1 AND accepted_at IS NULL")
                    .bind(t)
                    .fetch_optional(&st.db)
                    .await?;
            r.is_some()
        }
        None => false,
    };
    if !(st.cfg.allow_signup || count == 0 || invite_valid) {
        return Err(ApiError::forbidden("Sign ups are disabled. Ask an admin for an invite."));
    }
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE lower(email) = $1")
        .bind(&email)
        .fetch_optional(&st.db)
        .await?;
    if exists.is_some() {
        return Err(ApiError::bad_request("An account with this email already exists. Please sign in."));
    }

    let user: User = sqlx::query_as(
        "INSERT INTO users (id, name, email, password_hash, color) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(&name)
    .bind(&email)
    .bind(hash_password(&b.password)?)
    .bind(random_color())
    .fetch_one(&st.db)
    .await?;

    let team_id = if invite_valid {
        let token = b.invite_token.clone().unwrap();
        accept_invite_internal(&st.db, &user, &token).await?
    } else {
        let tn = b
            .team_name
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{name}'s Workspace"));
        create_team(&st.db, &user, &tn).await?.id
    };

    let token = create_session(&st.db, user.id, Some(team_id)).await?;
    Ok(with_cookie(
        session_cookie(&token, st.cfg.secure_cookies),
        json!({ "ok": true, "data": { "user": user } }),
    ))
}

pub async fn logout(State(st): State<AppState>, auth: Option<Auth>) -> ApiResult<Response> {
    if let Some(a) = auth {
        if let Some(h) = a.session_hash {
            sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(h).execute(&st.db).await?;
        }
    }
    Ok(with_cookie(clear_cookie(), json!({ "ok": true, "success": true })))
}

pub async fn info(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let team: Option<Team> = match auth.team_id {
        Some(t) => sqlx::query_as("SELECT * FROM teams WHERE id = $1").bind(t).fetch_optional(&st.db).await?,
        None => None,
    };
    let teams = crate::api::teams::my_teams(&st.db, auth.user.id).await?;
    ok(json!({
        "user": auth.user,
        "hasPassword": !auth.user.password_hash.is_empty(),
        "team": team,
        "role": auth.role,
        "teams": teams,
    }))
}
