//! Microsoft Entra ID (Azure AD) single sign-on with OpenID Connect
//! (authorization code flow + PKCE).

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    audit::{self, Actor, ClientMeta, Target},
    api::session::create_session,
    auth::session_cookie,
    error::{ApiError, ApiResult},
    models::User,
    util::{random_color, random_string},
    AppState, MicrosoftConfig,
};

const STATE_COOKIE: &str = "ms_oidc";

fn redirect(location: &str, cookies: Vec<String>) -> Response {
    let mut res = (StatusCode::FOUND, [(header::LOCATION, location.to_string())]).into_response();
    for c in cookies {
        res.headers_mut().append(header::SET_COOKIE, c.parse().unwrap());
    }
    res
}

fn clear_state_cookie() -> String {
    format!("{STATE_COOKIE}=; Path=/auth; HttpOnly; SameSite=Lax; Max-Age=0")
}

fn login_error(message: &str) -> Response {
    let mut url = reqwest::Url::parse("http://x/login").unwrap();
    url.query_pairs_mut().append_pair("error", message);
    let location = format!("/login?{}", url.query().unwrap_or(""));
    redirect(&location, vec![clear_state_cookie()])
}

fn redirect_uri(st: &AppState) -> String {
    let path = st.cfg.microsoft.as_ref().map(|m| m.callback_path.as_str()).unwrap_or("/auth/microsoft/callback");
    format!("{}{path}", st.cfg.url.trim_end_matches('/'))
}

fn is_tenant_specific(ms: &MicrosoftConfig) -> bool {
    !matches!(ms.tenant.as_str(), "common" | "organizations" | "consumers")
}

#[derive(Deserialize)]
pub struct StartQuery {
    redirect: Option<String>,
    invite: Option<String>,
}

/// GET /auth/microsoft - redirect the browser to the Microsoft login page.
pub async fn start(State(st): State<AppState>, Query(q): Query<StartQuery>) -> Response {
    let Some(ms) = &st.cfg.microsoft else {
        return login_error("Microsoft sign-in is not configured");
    };
    let state = random_string(32);
    let nonce = random_string(32);
    let verifier = random_string(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let target = q
        .redirect
        .filter(|r| r.starts_with('/') && !r.starts_with("//") && !r.starts_with("/\\"))
        .unwrap_or_else(|| "/".into());
    let invite = q.invite.filter(|i| i.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_default();
    let cookie_value = [state.clone(), nonce.clone(), verifier, URL_SAFE_NO_PAD.encode(target), invite].join(".");

    let mut url = match reqwest::Url::parse(&format!("{}/{}/oauth2/v2.0/authorize", ms.authority, ms.tenant)) {
        Ok(u) => u,
        Err(_) => return login_error("Invalid MICROSOFT_AUTHORITY"),
    };
    url.query_pairs_mut()
        .append_pair("client_id", &ms.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri(&st))
        .append_pair("response_mode", "query")
        .append_pair("scope", "openid profile email")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "select_account");

    let cookie = format!(
        "{STATE_COOKIE}={cookie_value}; Path=/auth; HttpOnly; SameSite=Lax; Max-Age=600{}",
        if st.cfg.secure_cookies { "; Secure" } else { "" }
    );
    redirect(url.as_str(), vec![cookie])
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().find_map(|v| {
        v.to_str().ok()?.split(';').find_map(|p| {
            let p = p.trim();
            p.strip_prefix(&format!("{name}=")).map(|s| s.to_string())
        })
    })
}

struct Identity {
    microsoft_id: String,
    email: String,
    name: String,
}

/// Exchange the authorization code and validate the ID token.
///
/// The ID token is received directly from the token endpoint over TLS, authenticated with the client
/// secret, so per OpenID Connect Core §3.1.3.7 the TLS validation replaces the signature check.
/// Audience, expiry, nonce and tenant are still verified.
async fn exchange(st: &AppState, ms: &MicrosoftConfig, code: &str, verifier: &str, nonce: &str) -> Result<Identity, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(format!("{}/{}/oauth2/v2.0/token", ms.authority, ms.tenant))
        .form(&[
            ("client_id", ms.client_id.as_str()),
            ("client_secret", ms.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri(st)),
            ("code_verifier", verifier),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .map_err(|e| format!("Could not reach Microsoft: {e}"))?;
    let body: Value = res.json().await.map_err(|e| format!("Invalid token response: {e}"))?;
    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        let desc = body.get("error_description").and_then(|e| e.as_str()).unwrap_or("");
        tracing::warn!(error = err, description = desc, "Microsoft token exchange failed");
        return Err(format!("Microsoft sign-in failed ({err})"));
    }
    let id_token = body.get("id_token").and_then(|t| t.as_str()).ok_or("No id_token in response")?;
    let payload = id_token.split('.').nth(1).ok_or("Malformed id_token")?;
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).map_err(|_| "Malformed id_token")?,
    )
    .map_err(|_| "Malformed id_token")?;

    let str_claim = |k: &str| claims.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if str_claim("aud") != ms.client_id {
        return Err("Token audience mismatch".into());
    }
    if str_claim("nonce") != nonce {
        return Err("Token nonce mismatch".into());
    }
    let exp = claims.get("exp").and_then(|v| v.as_i64()).unwrap_or(0);
    if exp < chrono::Utc::now().timestamp() - 300 {
        return Err("Token expired".into());
    }
    let tid = str_claim("tid");
    if is_tenant_specific(ms) && !tid.eq_ignore_ascii_case(&ms.tenant) {
        return Err("This Microsoft account belongs to another organization".into());
    }
    let oid = str_claim("oid");
    if oid.is_empty() || tid.is_empty() {
        return Err("Missing user identifier in token".into());
    }
    let email = [str_claim("email"), str_claim("preferred_username"), str_claim("upn")]
        .into_iter()
        .find(|e| e.contains('@'))
        .ok_or("Your Microsoft account has no email address")?
        .to_lowercase();
    let name = Some(str_claim("name")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| email.clone());
    Ok(Identity { microsoft_id: format!("{tid}:{oid}"), email, name })
}

fn email_domain(email: &str) -> String {
    email.rsplit('@').next().unwrap_or("").to_lowercase()
}

/// Find, link or create the local user for a Microsoft identity.
async fn resolve_user(st: &AppState, meta: &ClientMeta, ms: &MicrosoftConfig, id: &Identity, has_invite: bool) -> ApiResult<User> {
    if let Some(u) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE microsoft_id = $1")
        .bind(&id.microsoft_id)
        .fetch_optional(&st.db)
        .await?
    {
        return Ok(u);
    }
    // Linking an existing account by email is only safe when the identity provider is trusted for that
    // address: a single-tenant app, or an explicit list of allowed domains.
    let trusted_email = is_tenant_specific(ms) || !ms.allowed_domains.is_empty();
    if let Some(u) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE lower(email) = $1")
        .bind(&id.email)
        .fetch_optional(&st.db)
        .await?
    {
        if !trusted_email {
            return Err(ApiError::forbidden(
                "An account with this email already exists. Sign in with your password, or ask an admin to restrict Microsoft sign-in to your organization.",
            ));
        }
        if u.microsoft_id_is_set(&st.db).await? {
            return Err(ApiError::forbidden("This account is linked to a different Microsoft identity"));
        }
        sqlx::query("UPDATE users SET microsoft_id = $2, updated_at = now() WHERE id = $1")
            .bind(u.id)
            .bind(&id.microsoft_id)
            .execute(&st.db)
            .await?;
        return Ok(u);
    }
    let (pending,): (i64,) = sqlx::query_as("SELECT count(*) FROM invites WHERE lower(email) = $1 AND accepted_at IS NULL")
        .bind(&id.email)
        .fetch_one(&st.db)
        .await?;
    let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users").fetch_one(&st.db).await?;
    if !(ms.auto_provision || has_invite || pending > 0 || users == 0) {
        return Err(ApiError::forbidden("You don't have an account yet. Ask an admin to invite you."));
    }
    let user: User = sqlx::query_as(
        "INSERT INTO users (id, name, email, password_hash, color, microsoft_id, is_superadmin) \
         VALUES ($1, $2, $3, '', $4, $5, NOT EXISTS (SELECT 1 FROM users) OR lower($3) = ANY($6)) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(&id.name)
    .bind(&id.email)
    .bind(random_color())
    .bind(&id.microsoft_id)
    .bind(&st.cfg.superadmin_emails)
    .fetch_one(&st.db)
    .await?;
    audit::insert(st, meta, None, Actor::user(&user), "users.create", Target::user(&user), serde_json::json!({ "method": "microsoft" })).await;
    Ok(user)
}

impl User {
    async fn microsoft_id_is_set(&self, db: &sqlx::PgPool) -> ApiResult<bool> {
        let (v,): (Option<String>,) = sqlx::query_as("SELECT microsoft_id FROM users WHERE id = $1")
            .bind(self.id)
            .fetch_one(db)
            .await?;
        Ok(v.is_some())
    }
}

/// Join workspaces: explicit invite, pending invites for the email, and workspaces whose
/// allowed domains include the email domain. Returns the workspace to open.
async fn join_workspaces(st: &AppState, meta: &ClientMeta, user: &User, invite: &str) -> ApiResult<Option<Uuid>> {
    let mut preferred = None;
    let joined = |team: Uuid, how: &'static str| {
        audit::insert(st, meta, Some(team), Actor::user(user), "users.join", Target::user(user), serde_json::json!({ "via": how }))
    };
    if !invite.is_empty() {
        if let Ok(t) = crate::api::invites::accept_invite_internal(&st.db, user, invite).await {
            joined(t, "invite").await;
            preferred = Some(t);
        }
    }
    let pending: Vec<(String,)> = sqlx::query_as("SELECT token FROM invites WHERE lower(email) = $1 AND accepted_at IS NULL")
        .bind(user.email.to_lowercase())
        .fetch_all(&st.db)
        .await?;
    for (token,) in pending {
        let t = crate::api::invites::accept_invite_internal(&st.db, user, &token).await.ok();
        if let Some(t) = t {
            joined(t, "invite").await;
        }
        preferred = preferred.or(t);
    }
    let domain = email_domain(&user.email);
    let teams: Vec<(Uuid, String, String)> = sqlx::query_as("SELECT id, allowed_domains, default_user_role FROM teams WHERE allowed_domains <> ''")
        .fetch_all(&st.db)
        .await?;
    for (team_id, domains, role) in teams {
        let allowed = domains
            .split([',', ' ', ';', '\n'])
            .map(|d| d.trim().trim_start_matches('@').to_lowercase())
            .any(|d| !d.is_empty() && d == domain);
        if allowed {
            let r = sqlx::query("INSERT INTO team_memberships (team_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                .bind(team_id)
                .bind(user.id)
                .bind(&role)
                .execute(&st.db)
                .await?;
            if r.rows_affected() > 0 {
                joined(team_id, "allowed domain").await;
                preferred = preferred.or(Some(team_id));
            }
        }
    }
    Ok(preferred)
}

/// GET /auth/microsoft/callback
pub async fn callback(State(st): State<AppState>, meta: ClientMeta, headers: HeaderMap, Query(q): Query<CallbackQuery>) -> Response {
    let Some(ms) = &st.cfg.microsoft else {
        return login_error("Microsoft sign-in is not configured");
    };
    if let Some(err) = q.error {
        tracing::warn!(error = err, description = q.error_description.as_deref().unwrap_or(""), "Microsoft sign-in error");
        return login_error(if err == "access_denied" { "Sign-in was cancelled" } else { "Microsoft sign-in failed" });
    }
    let Some(cookie) = read_cookie(&headers, STATE_COOKIE) else {
        return login_error("Your sign-in session expired, please try again");
    };
    let parts: Vec<&str> = cookie.split('.').collect();
    if parts.len() != 5 {
        return login_error("Your sign-in session expired, please try again");
    }
    let (state, nonce, verifier, target_b64, invite) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
    if q.state.as_deref() != Some(state) {
        return login_error("Invalid sign-in state, please try again");
    }
    let Some(code) = q.code else {
        return login_error("Microsoft sign-in failed");
    };
    let target = URL_SAFE_NO_PAD
        .decode(target_b64)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|t| t.starts_with('/') && !t.starts_with("//"))
        .unwrap_or_else(|| "/".into());

    let identity = match exchange(&st, ms, &code, verifier, nonce).await {
        Ok(i) => i,
        Err(e) => return login_error(&e),
    };
    let failed = |reason: String| {
        let actor = Actor { name: Some(identity.name.as_str()), email: Some(identity.email.as_str()), ..Default::default() };
        audit::insert(&st, &meta, None, actor, "auth.signin_failed", None, serde_json::json!({ "method": "microsoft", "reason": reason }))
    };
    if !ms.allowed_domains.is_empty() && !ms.allowed_domains.contains(&email_domain(&identity.email)) {
        failed("email domain not allowed".into()).await;
        return login_error("Your email domain is not allowed to sign in");
    }
    let user = match resolve_user(&st, &meta, ms, &identity, !invite.is_empty()).await {
        Ok(u) => u,
        Err(e) => {
            failed(e.message.clone()).await;
            return login_error(&e.message);
        }
    };
    let team = match join_workspaces(&st, &meta, &user, invite).await {
        Ok(t) => t,
        Err(e) => return login_error(&e.message),
    };
    let token = match create_session(&st.db, user.id, team).await {
        Ok(t) => t,
        Err(e) => return login_error(&e.message),
    };
    tracing::info!(user = %user.email, "signed in with Microsoft");
    audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin", None, serde_json::json!({ "method": "microsoft" })).await;
    redirect(&target, vec![session_cookie(&token, st.cfg.secure_cookies), clear_state_cookie()])
}

/// GET /s/:share - the SPA, with headers that keep public pages out of search engines.
pub async fn shared_page(State(st): State<AppState>) -> Response {
    match tokio::fs::read(st.cfg.static_dir.join("index.html")).await {
        Ok(html) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::HeaderName::from_static("x-robots-tag"), "noindex, nofollow"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            html,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
