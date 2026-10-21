//! General settings of the installation (superadmins only).

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    instance::{self, InstanceSettings},
    AppState,
};

async fn superadmins(st: &AppState) -> ApiResult<Value> {
    let rows: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, name, email, color FROM users WHERE is_superadmin ORDER BY name").fetch_all(&st.db).await?;
    Ok(json!(rows
        .into_iter()
        .map(|(id, name, email, color)| json!({
            "id": id, "name": name, "email": email, "color": color,
            "fromEnvironment": st.cfg.superadmin_emails.contains(&email.to_lowercase()),
        }))
        .collect::<Vec<_>>()))
}

async fn info_value(st: &AppState) -> ApiResult<Value> {
    Ok(json!({
        "settings": st.settings(),
        // fallbacks shown when a default is not set here
        "environmentDefaults": st.cfg.defaults,
        "superadmins": superadmins(st).await?,
    }))
}

pub async fn info(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    auth.require_superadmin()?;
    ok(info_value(&st).await?)
}

/// Replaces the settings; fields left out keep their value, `null` clears an optional one.
pub async fn update(State(st): State<AppState>, auth: Auth, Json(patch): Json<Value>) -> ApiResult<Json<Value>> {
    auth.require_superadmin()?;
    let Value::Object(patch) = patch else { return Err(ApiError::bad_request("Expected an object")) };
    let mut current = serde_json::to_value(st.settings()).map_err(|e| ApiError::internal(e.to_string()))?;
    for (k, v) in patch {
        // empty strings from the form mean "not set"
        let v = match v.as_str().map(str::trim) {
            Some("") => Value::Null,
            Some(s) => Value::String(s.to_string()),
            None => v,
        };
        current[k] = v;
    }
    let next: InstanceSettings = serde_json::from_value(current).map_err(|e| ApiError::bad_request(format!("Invalid settings: {e}")))?;
    next.validate()?;
    let before = serde_json::to_value(st.settings()).unwrap_or_default();
    let after = serde_json::to_value(&next).unwrap_or_default();
    let changes: serde_json::Map<String, Value> = after
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
        .map(|(k, v)| (k.clone(), json!({ "from": before.get(k.as_str()), "to": v })))
        .collect();
    // turning the audit log off is recorded while it is still on, turning it on once it is on
    let turning_off = next.audit_retention_days == 0;
    if turning_off && !changes.is_empty() {
        audit::record_in(&st, &auth, None, "instance.update", None, Value::Object(changes.clone())).await;
    }
    instance::save(&st.db, &next, auth.user.id).await?;
    *st.instance.write().unwrap() = next;
    if !turning_off && !changes.is_empty() {
        audit::record_in(&st, &auth, None, "instance.update", None, Value::Object(changes)).await;
    }
    tracing::info!(user = %auth.user.email, "general settings updated");
    ok(info_value(&st).await?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestEmailReq {
    /// only connect to the mail server, send nothing
    #[serde(default)]
    check_only: bool,
}

/// Sends a sample invitation to the superadmin's own address (or only checks the connection).
pub async fn test_email(State(st): State<AppState>, auth: Auth, Json(b): Json<TestEmailReq>) -> ApiResult<Json<Value>> {
    auth.require_superadmin()?;
    let smtp = st.cfg.smtp.as_ref().ok_or_else(|| ApiError::bad_request("Email is not configured on this server (SMTP_HOST)"))?;
    if b.check_only {
        smtp.check().await?;
        return ok(json!({ "connected": true, "host": smtp.host, "port": smtp.port }));
    }
    let lang = auth.user.language.clone().unwrap_or_else(|| st.defaults().language);
    let settings = st.settings();
    let brand = settings.header_color.or(settings.accent_color).unwrap_or_else(|| "#1f2937".into());
    let base = st.cfg.url.trim_end_matches('/').to_string();
    let mail = crate::mail::invite(&crate::mail::InviteMail {
        lang: &lang,
        inviter: &auth.user.name,
        team: &st.settings().product_name(),
        product: &st.settings().product_name(),
        role: "member",
        email: &auth.user.email,
        link: &base,
        base_url: &base,
        added: false,
        microsoft: st.cfg.microsoft.is_some(),
        brand_color: &brand,
    });
    smtp.send(&auth.user.email, Some(&auth.user.name), &mail).await?;
    ok(json!({ "sentTo": auth.user.email }))
}

#[derive(Deserialize)]
pub struct EmailReq {
    email: String,
}

pub async fn add_superadmin(State(st): State<AppState>, auth: Auth, Json(b): Json<EmailReq>) -> ApiResult<Json<Value>> {
    auth.require_superadmin()?;
    let r = sqlx::query("UPDATE users SET is_superadmin = true WHERE lower(email) = lower($1)")
        .bind(b.email.trim())
        .execute(&st.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found("No account with this email: the person must sign in at least once"));
    }
    let target: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE lower(email) = lower($1)").bind(b.email.trim()).fetch_optional(&st.db).await?;
    if let Some((id,)) = target {
        audit::record_in(&st, &auth, None, "instance.superadmin_add", Target::user_id(&st.db, id).await, json!({})).await;
    }
    ok(info_value(&st).await?)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn remove_superadmin(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_superadmin()?;
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE is_superadmin").fetch_one(&st.db).await?;
    if count <= 1 {
        return Err(ApiError::bad_request("The installation must have at least one superadmin"));
    }
    let email: Option<(String,)> = sqlx::query_as("SELECT lower(email) FROM users WHERE id = $1").bind(b.id).fetch_optional(&st.db).await?;
    if email.is_some_and(|(e,)| st.cfg.superadmin_emails.contains(&e)) {
        return Err(ApiError::bad_request("This superadmin is set in SUPERADMIN_EMAILS and can only be removed there"));
    }
    sqlx::query("UPDATE users SET is_superadmin = false WHERE id = $1").bind(b.id).execute(&st.db).await?;
    audit::record_in(&st, &auth, None, "instance.superadmin_remove", Target::user_id(&st.db, b.id).await, json!({})).await;
    ok(info_value(&st).await?)
}
