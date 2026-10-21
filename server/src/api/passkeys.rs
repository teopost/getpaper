//! Passkeys (WebAuthn), when PASSKEYS=true: people add them in their profile, then sign in with
//! one without typing their email (discoverable credentials). The relying party is the host of URL,
//! so passkeys only work on that address.
//!
//! The state of a registration or of a sign-in in progress stays in memory for 5 minutes.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, response::Response, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;
use webauthn_rs::prelude::{DiscoverableAuthentication, DiscoverableKey, Passkey, PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential};
use webauthn_rs::{Webauthn, WebauthnBuilder};

use super::session::{create_session, with_cookie};
use crate::{
    audit::{self, Actor, ClientMeta, Target},
    auth::{session_cookie, Auth},
    error::{ok, ApiError, ApiResult},
    models::User,
    util::{non_empty, random_string},
    AppState,
};

/// The relying party for the public address of the installation
pub fn webauthn(url: &str) -> Result<Webauthn, String> {
    let origin = webauthn_rs::prelude::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    let rp_id = origin.host_str().ok_or("URL has no host")?.to_string();
    WebauthnBuilder::new(&rp_id, &origin)
        .and_then(|b| b.rp_name("GetPaper").build())
        .map_err(|e| format!("cannot set up passkeys for {url}: {e}"))
}

enum Pending {
    Register { user: Uuid, state: PasskeyRegistration },
    SignIn(DiscoverableAuthentication),
}

static PENDING: LazyLock<Mutex<HashMap<String, (Instant, Pending)>>> = LazyLock::new(Default::default);
const PENDING_FOR: Duration = Duration::from_secs(300);

fn keep(p: Pending) -> String {
    let id = random_string(32);
    let mut map = PENDING.lock().unwrap();
    map.retain(|_, (at, _)| at.elapsed() < PENDING_FOR);
    map.insert(id.clone(), (Instant::now(), p));
    id
}

fn take(id: &str) -> Option<Pending> {
    let mut map = PENDING.lock().unwrap();
    map.remove(id).filter(|(at, _)| at.elapsed() < PENDING_FOR).map(|(_, p)| p)
}

fn relying_party(st: &AppState) -> ApiResult<&Webauthn> {
    st.cfg.passkeys.as_ref().ok_or_else(|| ApiError::forbidden("Passkeys are not enabled on this server"))
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct PasskeyRow {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

/// The passkeys of the signed-in user
pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    relying_party(&st)?;
    let rows: Vec<PasskeyRow> = sqlx::query_as("SELECT id, name, created_at, last_used_at FROM passkeys WHERE user_id = $1 ORDER BY created_at")
        .bind(auth.user.id)
        .fetch_all(&st.db)
        .await?;
    ok(rows)
}

/// Starts adding a passkey: the options for navigator.credentials.create()
pub async fn register_start(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let rp = relying_party(&st)?;
    if auth.session_hash.is_none() {
        return Err(ApiError::forbidden("Passkeys can only be added from the app"));
    }
    let existing: Vec<(Value,)> = sqlx::query_as("SELECT passkey FROM passkeys WHERE user_id = $1").bind(auth.user.id).fetch_all(&st.db).await?;
    let exclude: Vec<_> = existing
        .into_iter()
        .filter_map(|(v,)| serde_json::from_value::<Passkey>(v).ok())
        .map(|p| p.cred_id().clone())
        .collect();
    let (options, state) = rp
        .start_passkey_registration(auth.user.id, &auth.user.email, &auth.user.name, Some(exclude))
        .map_err(|e| ApiError::internal(format!("passkey registration: {e}")))?;
    let mut options = serde_json::to_value(options).map_err(|e| ApiError::internal(e.to_string()))?;
    // the passkey must be stored on the device (discoverable), to sign in without typing the email
    if let Some(sel) = options.pointer_mut("/publicKey/authenticatorSelection") {
        sel["residentKey"] = json!("required");
        sel["requireResidentKey"] = json!(true);
    }
    let id = keep(Pending::Register { user: auth.user.id, state });
    ok(json!({ "challengeId": id, "options": options }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterFinishReq {
    challenge_id: String,
    name: String,
    credential: RegisterPublicKeyCredential,
}

pub async fn register_finish(State(st): State<AppState>, auth: Auth, Json(b): Json<RegisterFinishReq>) -> ApiResult<Json<Value>> {
    let rp = relying_party(&st)?;
    let name = non_empty(&b.name, "Name")?;
    let Some(Pending::Register { user, state }) = take(&b.challenge_id) else {
        return Err(ApiError::bad_request("The request expired, please try again"));
    };
    if user != auth.user.id {
        return Err(ApiError::bad_request("The request expired, please try again"));
    }
    let passkey = rp
        .finish_passkey_registration(&b.credential, &state)
        .map_err(|e| ApiError::bad_request(&format!("The passkey could not be added: {e}")))?;
    let cred_id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref() as &[u8]);
    let id = Uuid::new_v4();
    let r = sqlx::query("INSERT INTO passkeys (id, user_id, name, credential_id, passkey) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (credential_id) DO NOTHING")
        .bind(id)
        .bind(auth.user.id)
        .bind(name.chars().take(100).collect::<String>())
        .bind(&cred_id)
        .bind(serde_json::to_value(&passkey).map_err(|e| ApiError::internal(e.to_string()))?)
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::bad_request("This passkey is already registered"));
    }
    audit::record_in(&st, &auth, None, "users.passkey_add", Target::user(&auth.user), json!({ "name": name })).await;
    ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let row: Option<(String,)> = sqlx::query_as("DELETE FROM passkeys WHERE id = $1 AND user_id = $2 RETURNING name")
        .bind(b.id)
        .bind(auth.user.id)
        .fetch_optional(&st.db)
        .await?;
    let (name,) = row.ok_or_else(|| ApiError::not_found("Passkey not found"))?;
    audit::record_in(&st, &auth, None, "users.passkey_remove", Target::user(&auth.user), json!({ "name": name })).await;
    ok(json!({ "success": true }))
}

/// POST auth.passkeyStart - the options for navigator.credentials.get()
pub async fn sign_in_start(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let rp = relying_party(&st)?;
    let (options, state) = rp.start_discoverable_authentication().map_err(|e| ApiError::internal(format!("passkey sign-in: {e}")))?;
    let mut options = serde_json::to_value(options).map_err(|e| ApiError::internal(e.to_string()))?;
    // started by a button, not offered while typing in the email field
    if let Some(o) = options.as_object_mut() {
        o.remove("mediation");
    }
    let id = keep(Pending::SignIn(state));
    ok(json!({ "challengeId": id, "options": options }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInFinishReq {
    challenge_id: String,
    credential: PublicKeyCredential,
}

/// POST auth.passkeyFinish - signs in with the passkey
pub async fn sign_in_finish(State(st): State<AppState>, meta: ClientMeta, Json(b): Json<SignInFinishReq>) -> ApiResult<Response> {
    let rp = relying_party(&st)?;
    let Some(Pending::SignIn(state)) = take(&b.challenge_id) else {
        return Err(ApiError::bad_request("The request expired, please try again"));
    };
    let unknown = || ApiError::new(axum::http::StatusCode::UNAUTHORIZED, "authentication_failed", "This passkey is not registered here");
    let (user_id, cred) = rp.identify_discoverable_authentication(&b.credential).map_err(|_| unknown())?;
    let cred_id = URL_SAFE_NO_PAD.encode(cred);
    let row: Option<(Uuid, Value)> = sqlx::query_as("SELECT id, passkey FROM passkeys WHERE credential_id = $1 AND user_id = $2")
        .bind(&cred_id)
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?;
    let (pk_id, stored) = row.ok_or_else(unknown)?;
    let mut passkey: Passkey = serde_json::from_value(stored).map_err(|e| ApiError::internal(e.to_string()))?;
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1").bind(user_id).fetch_one(&st.db).await?;
    let result = match rp.finish_discoverable_authentication(&b.credential, state, &[DiscoverableKey::from(&passkey)]) {
        Ok(r) => r,
        Err(e) => {
            audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin_failed", None, json!({ "method": "passkey", "reason": e.to_string() })).await;
            return Err(unknown());
        }
    };
    if user.suspended_at.is_some() {
        audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin_failed", None, json!({ "method": "passkey", "reason": "account suspended" })).await;
        return Err(ApiError::forbidden("This account is suspended. Ask an administrator to reactivate it."));
    }
    // the signature counter and the backup state of the passkey
    if passkey.update_credential(&result).is_some() {
        sqlx::query("UPDATE passkeys SET passkey = $2 WHERE id = $1")
            .bind(pk_id)
            .bind(serde_json::to_value(&passkey).unwrap_or_default())
            .execute(&st.db)
            .await?;
    }
    sqlx::query("UPDATE passkeys SET last_used_at = now() WHERE id = $1").bind(pk_id).execute(&st.db).await?;
    let token = create_session(&st.db, user.id, None).await?;
    audit::insert(&st, &meta, None, Actor::user(&user), "auth.signin", None, json!({ "method": "passkey" })).await;
    Ok(with_cookie(session_cookie(&token, st.cfg.secure_cookies), json!({ "ok": true, "data": { "user": user } })))
}
