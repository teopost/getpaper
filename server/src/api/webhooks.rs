//! Settings → Webhooks: the webhooks of the workspace, for its admins (see crate::webhooks).

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    audit::{self, Actor, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    util::{non_empty, random_string},
    webhooks::{self, GROUPS},
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct WebhookRow {
    id: Uuid,
    name: String,
    url: String,
    secret: String,
    events: Vec<String>,
    enabled: bool,
    failures: i32,
    created_at: DateTime<Utc>,
    created_by_name: Option<String>,
    last_status: Option<i32>,
    last_error: Option<String>,
    last_delivery_at: Option<DateTime<Utc>>,
}

const SELECT: &str = "SELECT w.id, w.name, w.url, w.secret, w.events, w.enabled, w.failures, w.created_at, u.name AS created_by_name, \
    l.status AS last_status, l.error AS last_error, l.created_at AS last_delivery_at \
    FROM webhooks w LEFT JOIN users u ON u.id = w.created_by \
    LEFT JOIN LATERAL (SELECT status, error, created_at FROM webhook_deliveries WHERE webhook_id = w.id AND event <> 'webhooks.test' \
    ORDER BY created_at DESC LIMIT 1) l ON true";

async fn row(st: &AppState, auth: &Auth, id: Uuid) -> ApiResult<WebhookRow> {
    sqlx::query_as(&format!("{SELECT} WHERE w.id = $1 AND w.team_id = $2"))
        .bind(id)
        .bind(auth.team()?)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Webhook not found"))
}

fn check_events(events: &[String]) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = events.iter().map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect();
    out.sort();
    out.dedup();
    if let Some(e) = out.iter().find(|e| !GROUPS.contains(&e.split('.').next().unwrap_or(""))) {
        return Err(ApiError::bad_request(&format!("Unknown event: {e}")));
    }
    Ok(out)
}

async fn check_url(url: &str) -> ApiResult<String> {
    let url = non_empty(url, "URL")?;
    webhooks::resolve(&url).await.map_err(|e| ApiError::bad_request(&format!("This URL can't be used: {e}")))?;
    Ok(url)
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let rows: Vec<WebhookRow> =
        sqlx::query_as(&format!("{SELECT} WHERE w.team_id = $1 ORDER BY w.created_at")).bind(auth.team()?).fetch_all(&st.db).await?;
    ok(json!({ "webhooks": rows, "groups": GROUPS }))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    url: String,
    #[serde(default)]
    events: Vec<String>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let name = non_empty(&b.name, "Name")?;
    let url = check_url(&b.url).await?;
    let events = check_events(&b.events)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO webhooks (id, team_id, name, url, secret, events, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(id)
        .bind(auth.team()?)
        .bind(&name)
        .bind(&url)
        .bind(format!("gp_whsec_{}", random_string(32)))
        .bind(&events)
        .bind(auth.user.id)
        .execute(&st.db)
        .await?;
    audit::record(&st, &auth, "webhooks.create", Target::new("webhook", id, &name), json!({ "url": url, "events": events })).await;
    ok(row(&st, &auth, id).await?)
}

#[derive(Deserialize)]
pub struct UpdateReq {
    id: Uuid,
    name: Option<String>,
    url: Option<String>,
    events: Option<Vec<String>>,
    enabled: Option<bool>,
    /// a new secret
    #[serde(default, rename = "rotateSecret")]
    rotate_secret: bool,
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let before = row(&st, &auth, b.id).await?;
    let name = b.name.as_deref().map(|n| non_empty(n, "Name")).transpose()?;
    let url = match &b.url {
        Some(u) if u.trim() != before.url => Some(check_url(u).await?),
        _ => None,
    };
    let events = b.events.as_deref().map(check_events).transpose()?;
    sqlx::query(
        "UPDATE webhooks SET name = COALESCE($2, name), url = COALESCE($3, url), events = COALESCE($4, events), \
         enabled = COALESCE($5, enabled), failures = CASE WHEN $5 THEN 0 ELSE failures END, \
         secret = CASE WHEN $6 THEN $7 ELSE secret END, updated_at = now() WHERE id = $1",
    )
    .bind(b.id)
    .bind(&name)
    .bind(&url)
    .bind(&events)
    .bind(b.enabled)
    .bind(b.rotate_secret)
    .bind(format!("gp_whsec_{}", random_string(32)))
    .execute(&st.db)
    .await?;
    let after = row(&st, &auth, b.id).await?;
    let data = json!({ "url": after.url, "events": after.events, "enabled": after.enabled, "secretRotated": b.rotate_secret });
    audit::record(&st, &auth, "webhooks.update", Target::new("webhook", after.id, &after.name), data).await;
    ok(after)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let w = row(&st, &auth, b.id).await?;
    sqlx::query("DELETE FROM webhooks WHERE id = $1").bind(w.id).execute(&st.db).await?;
    audit::record(&st, &auth, "webhooks.delete", Target::new("webhook", w.id, &w.name), json!({ "url": w.url })).await;
    ok(json!({ "success": true }))
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct DeliveryRow {
    id: Uuid,
    event: String,
    status: Option<i32>,
    error: Option<String>,
    attempts: i32,
    duration_ms: Option<i32>,
    created_at: DateTime<Utc>,
}

/// The last 50 deliveries of a webhook (kept for a week)
pub async fn deliveries(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let w = row(&st, &auth, b.id).await?;
    let rows: Vec<DeliveryRow> = sqlx::query_as(
        "SELECT id, event, status, error, attempts, duration_ms, created_at FROM webhook_deliveries \
         WHERE webhook_id = $1 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(w.id)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

/// Sends a "webhooks.test" event now and tells how it went
pub async fn test(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    let w = row(&st, &auth, b.id).await?;
    let (team,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(auth.team()?).fetch_one(&st.db).await?;
    let actor = Actor::user(&auth.user);
    let target = Target { kind: "webhook", id: Some(w.id), name: w.name.clone() };
    let body = webhooks::payload("webhooks.test", (auth.team()?, &team), &actor, Some(&target), &json!({}), st.cfg.url.trim_end_matches('/'));
    let (status, error) = webhooks::deliver(&st, w.id, &w.url, &w.secret, "webhooks.test", body).await;
    ok(json!({ "status": status, "error": error }))
}
