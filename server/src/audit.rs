//! Audit log: who signed in and what people did, shown in Settings → Activity (workspace admins) and
//! General settings → Audit log (superadmins, events of the installation).
//!
//! Recording never makes a request fail: errors are only logged. Nothing is recorded when the
//! retention in the general settings is 0; older events are deleted by the hourly cleanup.
//!
//! The client address is the last entry of `X-Forwarded-For` (the one added by the reverse proxy in
//! front of the application), else `X-Real-IP`, else the address of the connection.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequestParts, OriginalUri},
    http::{header, request::Parts},
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{auth::Auth, models::User, AppState};

/// Where a request comes from
#[derive(Clone, Debug, Default)]
pub struct ClientMeta {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// web, api (API key) or mcp
    pub via: &'static str,
}

impl ClientMeta {
    pub fn from_parts(parts: &Parts) -> Self {
        let h = &parts.headers;
        let text = |name: &str| h.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty());
        let forwarded = h
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .last()
            .map(str::to_string);
        let ip = forwarded
            .or_else(|| text("x-real-ip").map(str::to_string))
            .or_else(|| parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()));
        let path = parts.extensions.get::<OriginalUri>().map(|u| u.path().to_string()).unwrap_or_else(|| parts.uri.path().to_string());
        let api_key = text(header::AUTHORIZATION.as_str()).is_some_and(|v| v.starts_with("Bearer ol_api_"));
        let via = if path.starts_with("/mcp") {
            "mcp"
        } else if api_key {
            "api"
        } else {
            "web"
        };
        let user_agent = text(header::USER_AGENT.as_str()).map(|s| s.chars().take(300).collect());
        ClientMeta { ip, user_agent, via }
    }
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for ClientMeta {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientMeta::from_parts(parts))
    }
}

/// What an event is about
pub struct Target {
    pub kind: &'static str,
    pub id: Option<Uuid>,
    pub name: String,
}

impl Target {
    pub fn new(kind: &'static str, id: Uuid, name: impl Into<String>) -> Option<Target> {
        Some(Target { kind, id: Some(id), name: name.into() })
    }
    pub fn document(id: Uuid, title: &str) -> Option<Target> {
        Self::new("document", id, if title.trim().is_empty() { "Untitled" } else { title })
    }
    pub fn collection(id: Uuid, name: &str) -> Option<Target> {
        Self::new("collection", id, name)
    }
    pub fn user(u: &User) -> Option<Target> {
        Self::new("user", u.id, format!("{} <{}>", u.name, u.email))
    }
    /// A user known only by id
    pub async fn user_id(db: &PgPool, id: Uuid) -> Option<Target> {
        let row: Option<(String, String)> = sqlx::query_as("SELECT name, email FROM users WHERE id = $1").bind(id).fetch_optional(db).await.ok()?;
        let (name, email) = row?;
        Self::new("user", id, format!("{name} <{email}>"))
    }
    pub async fn collection_name(db: &PgPool, id: Option<Uuid>) -> Option<String> {
        let (name,): (String,) = sqlx::query_as("SELECT name FROM collections WHERE id = $1").bind(id?).fetch_optional(db).await.ok()??;
        Some(name)
    }
    pub async fn group_id(db: &PgPool, id: Uuid) -> Option<Target> {
        let (name,): (String,) = sqlx::query_as("SELECT name FROM groups WHERE id = $1").bind(id).fetch_optional(db).await.ok()??;
        Self::new("group", id, name)
    }
}

/// An event in the workspace of the request, by the signed-in user.
pub async fn record(st: &AppState, auth: &Auth, name: &str, target: Option<Target>, data: Value) {
    record_in(st, auth, auth.team_id, name, target, data).await
}

/// An event in another workspace (`None`: an event of the installation), by the signed-in user.
pub async fn record_in(st: &AppState, auth: &Auth, team: Option<Uuid>, name: &str, target: Option<Target>, data: Value) {
    let actor = Actor { id: Some(auth.user.id), name: Some(&auth.user.name), email: Some(&auth.user.email) };
    insert(st, &auth.meta, team, actor, name, target, data).await
}

/// Who did it, when there is no signed-in user (sign-in, failed sign-in, sign-up)
#[derive(Default)]
pub struct Actor<'a> {
    pub id: Option<Uuid>,
    pub name: Option<&'a str>,
    pub email: Option<&'a str>,
}

impl<'a> Actor<'a> {
    pub fn user(u: &'a User) -> Self {
        Actor { id: Some(u.id), name: Some(&u.name), email: Some(&u.email) }
    }
}

pub async fn insert(st: &AppState, meta: &ClientMeta, team: Option<Uuid>, actor: Actor<'_>, name: &str, target: Option<Target>, data: Value) {
    if st.settings().audit_retention_days == 0 {
        return;
    }
    let (kind, id, tname) = match target {
        Some(t) => (Some(t.kind), t.id, Some(t.name)),
        None => (None, None, None),
    };
    let r = sqlx::query(
        "INSERT INTO events (team_id, actor_id, actor_name, actor_email, name, target_type, target_id, target_name, data, ip, user_agent, via) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(team)
    .bind(actor.id)
    .bind(actor.name)
    .bind(actor.email)
    .bind(name)
    .bind(kind)
    .bind(id)
    .bind(tname)
    .bind(data)
    .bind(&meta.ip)
    .bind(&meta.user_agent)
    .bind(meta.via)
    .execute(&st.db)
    .await;
    if let Err(e) = r {
        tracing::warn!(error = %e, event = name, "cannot record the audit event");
    }
}

/// Deletes the events older than the retention (all of them when it is 0).
pub async fn cleanup(db: &PgPool, retention_days: u32) {
    let r = sqlx::query("DELETE FROM events WHERE created_at < now() - make_interval(days => $1)")
        .bind(retention_days as i32)
        .execute(db)
        .await;
    if let Err(e) = r {
        tracing::warn!(error = %e, "cannot clean up the audit log");
    }
}
