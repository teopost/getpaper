//! Sign-in with a link sent by email (EMAIL_LOGIN=true, needs SMTP), for people who already have
//! an account. The link opens a page of the app that asks to confirm: link checkers of mail
//! servers open links by themselves, and must not use up the sign-in. A link works once, for 15
//! minutes; one email per address per minute at most.

use axum::{extract::State, response::Response, Json};
use serde::Deserialize;
use serde_json::json;

use super::session::create_session;
use crate::{
    audit::{self, Actor, ClientMeta},
    auth::session_cookie,
    error::{ok, ApiError, ApiResult},
    models::User,
    util::{random_string, sha256_hex},
    AppState,
};

#[derive(Deserialize)]
pub struct LinkReq {
    email: String,
    redirect: Option<String>,
}

fn safe_redirect(r: Option<String>) -> String {
    r.filter(|r| r.starts_with('/') && !r.starts_with("//") && !r.starts_with("/\\")).unwrap_or_else(|| "/".into())
}

/// POST auth.emailLink { email } - sends the link, if there is an account with this address. The
/// answer is the same either way, so it doesn't tell who has an account.
pub async fn send_link(State(st): State<AppState>, meta: ClientMeta, Json(b): Json<LinkReq>) -> ApiResult<Json<serde_json::Value>> {
    if !st.cfg.email_login {
        return Err(ApiError::forbidden("Sign-in by email is not enabled"));
    }
    let email = b.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad_request("Enter a valid email address"));
    }
    let redirect = safe_redirect(b.redirect);
    // in the background: the time of the answer doesn't tell who has an account either
    tokio::spawn(async move {
        if let Err(e) = send(&st, &meta, &email, &redirect).await {
            tracing::warn!("sign-in link for {email} not sent: {}", e.message);
        }
    });
    ok(json!({ "sent": true }))
}

async fn send(st: &AppState, meta: &ClientMeta, email: &str, redirect: &str) -> ApiResult<()> {
    let Some(smtp) = st.cfg.smtp.as_ref() else { return Ok(()) };
    let Some(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE lower(email) = $1").bind(email).fetch_optional(&st.db).await? else {
        let actor = Actor { email: Some(email), ..Default::default() };
        audit::insert(st, meta, None, actor, "auth.signin_failed", None, json!({ "method": "email", "reason": "unknown email" })).await;
        return Ok(());
    };
    if user.suspended_at.is_some() {
        return Ok(());
    }
    let (recent,): (i64,) = sqlx::query_as("SELECT count(*) FROM login_tokens WHERE user_id = $1 AND created_at > now() - interval '1 minute'")
        .bind(user.id)
        .fetch_one(&st.db)
        .await?;
    if recent > 0 {
        return Ok(());
    }
    let token = random_string(48);
    sqlx::query("INSERT INTO login_tokens (token_hash, user_id, redirect, expires_at) VALUES ($1, $2, $3, now() + interval '15 minutes')")
        .bind(sha256_hex(&token))
        .bind(user.id)
        .bind(redirect)
        .execute(&st.db)
        .await?;
    let lang = user.language.clone().unwrap_or_else(|| st.defaults().language);
    let settings = st.settings();
    let brand = settings.header_color.clone().or(settings.accent_color.clone()).unwrap_or_else(|| "#1f2937".into());
    let base = st.cfg.url.trim_end_matches('/');
    let link = format!("{base}/login/email?token={token}");
    let mail = crate::mail::sign_in_link(&lang, &settings.product_name(), &link, base, &brand);
    smtp.send(&user.email, Some(&user.name), &mail).await
}

#[derive(Deserialize)]
pub struct TokenReq {
    token: String,
}

/// POST auth.emailCallback { token } - signs in with the link
pub async fn callback(State(st): State<AppState>, meta: ClientMeta, Json(b): Json<TokenReq>) -> ApiResult<Response> {
    if !st.cfg.email_login {
        return Err(ApiError::forbidden("Sign-in by email is not enabled"));
    }
    let row: Option<(uuid::Uuid, Option<String>)> = sqlx::query_as(
        "UPDATE login_tokens SET used_at = now() WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() \
         RETURNING user_id, redirect",
    )
    .bind(sha256_hex(b.token.trim()))
    .fetch_optional(&st.db)
    .await?;
    let (user_id, redirect) = row.ok_or_else(|| ApiError::bad_request("This sign-in link has expired or was already used. Ask for a new one."))?;
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1").bind(user_id).fetch_one(&st.db).await?;
    if user.suspended_at.is_some() {
        audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin_failed", None, json!({ "method": "email", "reason": "account suspended" })).await;
        return Err(ApiError::forbidden("This account is suspended. Ask an administrator to reactivate it."));
    }
    // invitations waiting for this address: the link proves it belongs to the person
    let pending: Vec<(String,)> = sqlx::query_as("SELECT token FROM invites WHERE lower(email) = lower($1) AND accepted_at IS NULL")
        .bind(&user.email)
        .fetch_all(&st.db)
        .await?;
    let mut team = None;
    for (invite,) in pending {
        if let Ok(t) = super::invites::accept_invite_internal(&st, &user, &invite).await {
            audit::insert(&st, &meta, Some(t), Actor::user(&user), "users.join", crate::audit::Target::user(&user), json!({ "via": "invite" })).await;
            team = team.or(Some(t));
        }
    }
    let token = create_session(&st.db, user.id, team).await?;
    audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin", None, json!({ "method": "email" })).await;
    Ok(super::session::with_cookie(
        session_cookie(&token, st.cfg.secure_cookies),
        json!({ "ok": true, "data": { "redirect": safe_redirect(redirect) } }),
    ))
}
