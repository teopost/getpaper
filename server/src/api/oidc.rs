//! Sign-in with Google, Slack or any OpenID Connect provider (Keycloak, Okta, Authentik, …):
//! authorization code flow, with PKCE when the provider supports it. The endpoints come from the
//! discovery document of the issuer. Accounts are linked through `user_identities`; an existing
//! account is linked by email only when the provider says the address is verified.
//!
//! As for Microsoft (sso.rs), the ID token comes straight from the token endpoint over TLS,
//! authenticated with the client secret, so per OpenID Connect Core §3.1.3.7 the TLS validation
//! replaces the signature check; issuer, audience, expiry and nonce are still verified.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::sso::{email_domain, join_workspaces, login_error_clearing, read_cookie, redirect};
use crate::{
    api::session::create_session,
    audit::{self, Actor, ClientMeta, Target},
    auth::session_cookie,
    error::{ApiError, ApiResult},
    models::User,
    util::{random_color, random_string},
    AppState, OidcProvider,
};

const STATE_COOKIE: &str = "gp_oidc";

fn fail(message: &str) -> Response {
    login_error_clearing(message, STATE_COOKIE)
}

fn provider<'a>(st: &'a AppState, id: &str) -> Option<&'a OidcProvider> {
    st.cfg.oidc.iter().find(|p| p.id == id)
}

fn redirect_uri(st: &AppState, p: &OidcProvider) -> String {
    format!("{}/auth/{}/callback", st.cfg.url.trim_end_matches('/'), p.id)
}

#[derive(Clone, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
}

static DISCOVERY: LazyLock<Mutex<HashMap<String, (Instant, Discovery)>>> = LazyLock::new(Default::default);

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())
}

/// The configuration of the provider, cached for an hour
async fn discover(p: &OidcProvider) -> Result<Discovery, String> {
    if let Some((at, d)) = DISCOVERY.lock().unwrap().get(&p.issuer) {
        if at.elapsed() < Duration::from_secs(3600) {
            return Ok(d.clone());
        }
    }
    let url = format!("{}/.well-known/openid-configuration", p.issuer.trim_end_matches('/'));
    let d: Discovery = http()?
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Could not reach {}: {e}", p.name))?
        .json()
        .await
        .map_err(|e| format!("Invalid configuration of {}: {e}", p.name))?;
    if d.issuer.trim_end_matches('/') != p.issuer.trim_end_matches('/') {
        return Err(format!("The issuer of {} doesn't match its configuration", p.name));
    }
    DISCOVERY.lock().unwrap().insert(p.issuer.clone(), (Instant::now(), d.clone()));
    Ok(d)
}

#[derive(Deserialize)]
pub struct StartQuery {
    redirect: Option<String>,
    invite: Option<String>,
}

/// GET /auth/:provider - to the sign-in page of the provider
pub async fn start(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<StartQuery>) -> Response {
    let Some(p) = provider(&st, &id) else {
        return fail("This sign-in method is not configured");
    };
    let d = match discover(p).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("{e}");
            return fail(&format!("{} sign-in is not available right now", p.name));
        }
    };
    let state = random_string(32);
    let nonce = random_string(32);
    let verifier = random_string(64);
    let target = q
        .redirect
        .filter(|r| r.starts_with('/') && !r.starts_with("//") && !r.starts_with("/\\"))
        .unwrap_or_else(|| "/".into());
    let invite = q.invite.filter(|i| i.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_default();
    let cookie_value = [p.id.to_string(), state.clone(), nonce.clone(), verifier.clone(), URL_SAFE_NO_PAD.encode(target), invite].join(".");

    let Ok(mut url) = reqwest::Url::parse(&d.authorization_endpoint) else {
        return fail(&format!("Invalid configuration of {}", p.name));
    };
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &p.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri(&st, p))
            .append_pair("scope", &p.scopes)
            .append_pair("state", &state)
            .append_pair("nonce", &nonce);
        if d.code_challenge_methods_supported.iter().any(|m| m == "S256") {
            q.append_pair("code_challenge", &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())))
                .append_pair("code_challenge_method", "S256");
        }
        if p.id == "google" {
            q.append_pair("prompt", "select_account");
            // a single Google Workspace domain: only its accounts are offered
            if let [domain] = p.allowed_domains.as_slice() {
                q.append_pair("hd", domain);
            }
        }
    }
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

struct Identity {
    subject: String,
    email: String,
    email_verified: bool,
    name: String,
}

/// `email_verified` as a boolean or as the string "true", as some providers send it
fn verified(claims: &Value) -> bool {
    match claims.get("email_verified") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn audience_ok(claims: &Value, client_id: &str) -> bool {
    match claims.get("aud") {
        Some(Value::String(a)) => a == client_id,
        Some(Value::Array(a)) => a.iter().any(|v| v.as_str() == Some(client_id)),
        _ => false,
    }
}

/// Exchanges the code for the ID token and checks it
async fn exchange(st: &AppState, p: &OidcProvider, d: &Discovery, code: &str, verifier: &str, nonce: &str) -> Result<Identity, String> {
    let redirect = redirect_uri(st, p);
    let pkce = d.code_challenge_methods_supported.iter().any(|m| m == "S256");
    let mut form = vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", redirect.as_str())];
    if pkce {
        form.push(("code_verifier", verifier));
    }
    // client_secret_basic is the default of the specification; client_secret_post when it's the only one offered
    let methods = &d.token_endpoint_auth_methods_supported;
    let basic = methods.is_empty() || methods.iter().any(|m| m == "client_secret_basic");
    let mut req = http()?.post(&d.token_endpoint);
    if basic {
        req = req.basic_auth(
            url_encode(&p.client_id),
            Some(url_encode(&p.client_secret)),
        );
    } else {
        form.push(("client_id", &p.client_id));
        form.push(("client_secret", &p.client_secret));
    }
    let res = req.form(&form).send().await.map_err(|e| format!("Could not reach {}: {e}", p.name))?;
    let body: Value = res.json().await.map_err(|e| format!("Invalid answer from {}: {e}", p.name))?;
    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        let desc = body.get("error_description").and_then(|e| e.as_str()).unwrap_or("");
        tracing::warn!(provider = p.id, error = err, description = desc, "token exchange failed");
        return Err(format!("{} sign-in failed ({err})", p.name));
    }
    let id_token = body.get("id_token").and_then(|t| t.as_str()).ok_or("No id_token in response")?;
    let payload = id_token.split('.').nth(1).ok_or("Malformed id_token")?;
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).map_err(|_| "Malformed id_token")?)
            .map_err(|_| "Malformed id_token")?;
    let str_claim = |k: &str| claims.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if str_claim("iss").trim_end_matches('/') != d.issuer.trim_end_matches('/') {
        return Err("Token issuer mismatch".into());
    }
    if !audience_ok(&claims, &p.client_id) {
        return Err("Token audience mismatch".into());
    }
    if str_claim("nonce") != nonce {
        return Err("Token nonce mismatch".into());
    }
    let exp = claims.get("exp").and_then(|v| v.as_i64()).unwrap_or(0);
    if exp < chrono::Utc::now().timestamp() - 300 {
        return Err("Token expired".into());
    }
    let subject = str_claim("sub");
    if subject.is_empty() {
        return Err("Missing user identifier in token".into());
    }
    let email = str_claim("email").to_lowercase();
    if !email.contains('@') {
        return Err(format!("Your {} account has no email address", p.name));
    }
    let name = [str_claim("name"), format!("{} {}", str_claim("given_name"), str_claim("family_name")).trim().to_string(), str_claim("preferred_username")]
        .into_iter()
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| email.clone());
    Ok(Identity { subject, email, email_verified: verified(&claims) || p.trust_email, name })
}

/// application/x-www-form-urlencoded, as RFC 6749 §2.3.1 wants for HTTP Basic credentials
fn url_encode(s: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", &[("v", s)])
        .ok()
        .and_then(|u| u.query().map(|q| q.trim_start_matches("v=").to_string()))
        .unwrap_or_else(|| s.to_string())
}

/// Finds, links or creates the account of an identity
async fn resolve_user(st: &AppState, meta: &ClientMeta, p: &OidcProvider, id: &Identity, has_invite: bool) -> ApiResult<User> {
    let linked: Option<User> = sqlx::query_as(
        "SELECT u.* FROM user_identities i JOIN users u ON u.id = i.user_id WHERE i.provider = $1 AND i.subject = $2",
    )
    .bind(p.id)
    .bind(&id.subject)
    .fetch_optional(&st.db)
    .await?;
    if let Some(u) = linked {
        sqlx::query("UPDATE user_identities SET last_used_at = now(), email = $3 WHERE provider = $1 AND subject = $2")
            .bind(p.id)
            .bind(&id.subject)
            .bind(&id.email)
            .execute(&st.db)
            .await?;
        return Ok(u);
    }
    // an unverified address could belong to someone else, with their account and invitations
    if !id.email_verified {
        return Err(ApiError::forbidden(&format!("Your {} account has no verified email address", p.name)));
    }
    let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE lower(email) = $1").bind(&id.email).fetch_optional(&st.db).await?;
    let user = match existing {
        Some(u) => u,
        None => {
            let (pending,): (i64,) = sqlx::query_as("SELECT count(*) FROM invites WHERE lower(email) = $1 AND accepted_at IS NULL")
                .bind(&id.email)
                .fetch_one(&st.db)
                .await?;
            let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users").fetch_one(&st.db).await?;
            if !(p.auto_provision || has_invite || pending > 0 || users == 0) {
                return Err(ApiError::forbidden("You don't have an account yet. Ask an admin to invite you."));
            }
            let user: User = sqlx::query_as(
                "INSERT INTO users (id, name, email, password_hash, color, is_superadmin) \
                 VALUES ($1, $2, $3, '', $4, NOT EXISTS (SELECT 1 FROM users) OR lower($3) = ANY($5)) RETURNING *",
            )
            .bind(Uuid::new_v4())
            .bind(&id.name)
            .bind(&id.email)
            .bind(random_color())
            .bind(&st.cfg.superadmin_emails)
            .fetch_one(&st.db)
            .await?;
            audit::insert(st, meta, None, Actor::user(&user), "users.create", Target::user(&user), serde_json::json!({ "method": p.id })).await;
            user
        }
    };
    sqlx::query("INSERT INTO user_identities (provider, subject, user_id, email, last_used_at) VALUES ($1, $2, $3, $4, now())")
        .bind(p.id)
        .bind(&id.subject)
        .bind(user.id)
        .bind(&id.email)
        .execute(&st.db)
        .await?;
    Ok(user)
}

/// GET /auth/:provider/callback
pub async fn callback(
    State(st): State<AppState>,
    Path(pid): Path<String>,
    meta: ClientMeta,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(p) = provider(&st, &pid) else {
        return fail("This sign-in method is not configured");
    };
    if let Some(err) = q.error {
        tracing::warn!(provider = p.id, error = err, description = q.error_description.as_deref().unwrap_or(""), "sign-in error");
        return fail(&if err == "access_denied" { "Sign-in was cancelled".to_string() } else { format!("{} sign-in failed", p.name) });
    }
    let expired = || fail("Your sign-in session expired, please try again");
    let Some(cookie) = read_cookie(&headers, STATE_COOKIE) else { return expired() };
    let parts: Vec<&str> = cookie.split('.').collect();
    let [cookie_provider, state, nonce, verifier, target_b64, invite] = parts[..] else { return expired() };
    if cookie_provider != p.id {
        return expired();
    }
    if q.state.as_deref() != Some(state) {
        return fail("Invalid sign-in state, please try again");
    }
    let Some(code) = q.code else {
        return fail(&format!("{} sign-in failed", p.name));
    };
    let target = URL_SAFE_NO_PAD
        .decode(target_b64)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|t| t.starts_with('/') && !t.starts_with("//"))
        .unwrap_or_else(|| "/".into());
    let d = match discover(p).await {
        Ok(d) => d,
        Err(e) => return fail(&e),
    };
    let identity = match exchange(&st, p, &d, &code, verifier, nonce).await {
        Ok(i) => i,
        Err(e) => return fail(&e),
    };
    let failed = |reason: String| {
        let actor = Actor { name: Some(identity.name.as_str()), email: Some(identity.email.as_str()), ..Default::default() };
        audit::insert(&st, &meta, None, actor, "auth.signin_failed", None, serde_json::json!({ "method": p.id, "reason": reason }))
    };
    if !p.allowed_domains.is_empty() && (!identity.email_verified || !p.allowed_domains.contains(&email_domain(&identity.email))) {
        failed("email domain not allowed".into()).await;
        return fail("Your email domain is not allowed to sign in");
    }
    let user = match resolve_user(&st, &meta, p, &identity, !invite.is_empty()).await {
        Ok(u) => u,
        Err(e) => {
            failed(e.message.clone()).await;
            return fail(&e.message);
        }
    };
    if user.suspended_at.is_some() {
        failed("account suspended".into()).await;
        return fail("This account is suspended. Ask an administrator to reactivate it.");
    }
    let team = match join_workspaces(&st, &meta, &user, invite).await {
        Ok(t) => t,
        Err(e) => return fail(&e.message),
    };
    let token = match create_session(&st.db, user.id, team).await {
        Ok(t) => t,
        Err(e) => return fail(&e.message),
    };
    tracing::info!(user = %user.email, provider = p.id, "signed in");
    audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin", None, serde_json::json!({ "method": p.id })).await;
    redirect(&target, vec![session_cookie(&token, st.cfg.secure_cookies), super::sso::clear_cookie(STATE_COOKIE)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audience_and_verified_email() {
        assert!(audience_ok(&json!({ "aud": "abc" }), "abc"));
        assert!(audience_ok(&json!({ "aud": ["x", "abc"] }), "abc"));
        assert!(!audience_ok(&json!({ "aud": "x" }), "abc"));
        assert!(verified(&json!({ "email_verified": true })));
        assert!(verified(&json!({ "email_verified": "true" })));
        assert!(!verified(&json!({ "email_verified": false })));
        assert!(!verified(&json!({})));
    }

    #[test]
    fn basic_credentials_are_form_encoded() {
        assert_eq!(url_encode("a b+c:d"), "a+b%2Bc%3Ad");
        assert_eq!(url_encode("plain"), "plain");
    }
}
