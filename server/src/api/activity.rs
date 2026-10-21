//! Settings → Activity for workspace admins (changes to documents, people, audit log) and the audit
//! log of the installation for superadmins (sign-ins, general settings, workspaces).
//!
//! Superadmins see only the events of the installation: as everywhere else, they don't see the
//! content of workspaces they are not members of. None of this is available through API keys.

use axum::{extract::State, Json};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    AppState,
};

const MAX_LIMIT: i64 = 1000;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    user_id: Option<Uuid>,
    collection_id: Option<Uuid>,
    /// auth, members, content, sharing, data, settings
    category: Option<String>,
    /// name, email or target contains
    q: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl Filter {
    fn page(&self) -> (i64, i64) {
        (self.limit.unwrap_or(100).clamp(1, MAX_LIMIT), self.offset.unwrap_or(0).max(0))
    }
    fn range(&self, default_days: Option<i64>) -> (DateTime<Utc>, DateTime<Utc>) {
        let to = self.to.unwrap_or_else(Utc::now);
        let from = self.from.unwrap_or_else(|| match default_days {
            Some(d) => to - Duration::days(d),
            None => DateTime::<Utc>::UNIX_EPOCH,
        });
        (from, to)
    }
    fn q(&self) -> Option<String> {
        self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(|q| format!("%{}%", q.replace('%', "\\%").replace('_', "\\_")))
    }
}

/// Event name patterns of a category
fn category(c: Option<&str>) -> ApiResult<Option<Vec<&'static str>>> {
    Ok(match c.unwrap_or("") {
        "" | "all" => None,
        "auth" => Some(vec!["auth.%"]),
        "members" => Some(vec!["users.%", "invites.%", "groups.%"]),
        "content" => Some(vec!["documents.%", "collections.%"]),
        "sharing" => Some(vec!["shares.%"]),
        "data" => Some(vec!["data.%", "workspace.%", "%.export"]),
        "settings" => Some(vec!["teams.%", "instance.%", "api_keys.%"]),
        _ => return Err(ApiError::bad_request("Unknown category")),
    })
}

fn require_activity(auth: &Auth) -> ApiResult<()> {
    auth.require_admin().map_err(|_| ApiError::forbidden("Only workspace admins can see the activity"))
}

// ---------------------------------------------------------------------------
// changes to documents (from the revision history)
// ---------------------------------------------------------------------------

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct ChangeRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    document_id: Uuid,
    url_id: String,
    title: String,
    emoji: Option<String>,
    collection_id: Option<Uuid>,
    collection_name: Option<String>,
    user_id: Uuid,
    user_name: String,
    user_color: String,
    user_avatar_url: Option<String>,
    /// the first version of the document
    is_first: bool,
    deleted: bool,
}

/// Recent edits of published documents: one entry per editing session (the history groups the
/// saves of the same person within 5 minutes).
pub async fn changes(State(st): State<AppState>, auth: Auth, Json(f): Json<Filter>) -> ApiResult<Json<Value>> {
    require_activity(&auth)?;
    let (limit, offset) = f.page();
    let (from, to) = f.range(None);
    let rows: Vec<ChangeRow> = sqlx::query_as(
        "SELECT r.id, r.created_at, d.id AS document_id, d.url_id, d.title, d.emoji, d.collection_id, c.name AS collection_name, \
         u.id AS user_id, u.name AS user_name, u.color AS user_color, u.avatar_url AS user_avatar_url, \
         NOT EXISTS (SELECT 1 FROM revisions r2 WHERE r2.document_id = r.document_id AND r2.created_at < r.created_at) AS is_first, \
         d.deleted_at IS NOT NULL AS deleted \
         FROM revisions r JOIN documents d ON d.id = r.document_id JOIN users u ON u.id = r.user_id \
         LEFT JOIN collections c ON c.id = d.collection_id \
         WHERE d.team_id = $1 AND d.template = false AND ($2::uuid IS NULL OR r.user_id = $2) \
         AND ($3::uuid IS NULL OR d.collection_id = $3) AND r.created_at >= $4 AND r.created_at <= $5 \
         AND ($6::text IS NULL OR d.title ILIKE $6) \
         ORDER BY r.created_at DESC LIMIT $7 OFFSET $8",
    )
    .bind(auth.team()?)
    .bind(f.user_id)
    .bind(f.collection_id)
    .bind(from)
    .bind(to)
    .bind(f.q())
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;
    let more = rows.len() as i64 > limit;
    ok(json!({ "items": rows.into_iter().take(limit as usize).collect::<Vec<_>>(), "hasMore": more }))
}

// ---------------------------------------------------------------------------
// people
// ---------------------------------------------------------------------------

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct PersonRow {
    id: Uuid,
    name: String,
    email: String,
    color: String,
    avatar_url: Option<String>,
    role: String,
    last_active_at: Option<DateTime<Utc>>,
    last_sign_in_at: Option<DateTime<Utc>>,
    sign_ins: i64,
    documents_published: i64,
    documents_edited: i64,
    edits: i64,
    last_edit_at: Option<DateTime<Utc>>,
    /// only when the general settings allow it
    documents_viewed: Option<i64>,
}

/// The members with what they did in the period (default: the last 30 days).
pub async fn people(State(st): State<AppState>, auth: Auth, Json(f): Json<Filter>) -> ApiResult<Json<Value>> {
    require_activity(&auth)?;
    let (from, to) = f.range(Some(30));
    let settings = st.settings();
    let rows: Vec<PersonRow> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.color, u.avatar_url, m.role, u.last_active_at, \
         (SELECT max(e.created_at) FROM events e WHERE e.actor_id = u.id AND e.name = 'auth.signin') AS last_sign_in_at, \
         (SELECT count(*) FROM events e WHERE e.actor_id = u.id AND e.name = 'auth.signin' AND e.created_at BETWEEN $2 AND $3) AS sign_ins, \
         (SELECT count(*) FROM documents d WHERE d.team_id = $1 AND d.created_by = u.id AND d.template = false \
            AND d.published_at BETWEEN $2 AND $3) AS documents_published, \
         (SELECT count(DISTINCT r.document_id) FROM revisions r JOIN documents d ON d.id = r.document_id \
            WHERE d.team_id = $1 AND r.user_id = u.id AND r.created_at BETWEEN $2 AND $3) AS documents_edited, \
         (SELECT count(*) FROM revisions r JOIN documents d ON d.id = r.document_id \
            WHERE d.team_id = $1 AND r.user_id = u.id AND r.created_at BETWEEN $2 AND $3) AS edits, \
         (SELECT max(r.created_at) FROM revisions r JOIN documents d ON d.id = r.document_id \
            WHERE d.team_id = $1 AND r.user_id = u.id) AS last_edit_at, \
         CASE WHEN $4 THEN (SELECT count(*) FROM views v JOIN documents d ON d.id = v.document_id \
            WHERE d.team_id = $1 AND v.user_id = u.id AND v.last_viewed_at BETWEEN $2 AND $3) END AS documents_viewed \
         FROM team_memberships m JOIN users u ON u.id = m.user_id WHERE m.team_id = $1 \
         ORDER BY u.last_active_at DESC NULLS LAST, lower(u.name)",
    )
    .bind(auth.team()?)
    .bind(from)
    .bind(to)
    .bind(settings.activity_show_views)
    .fetch_all(&st.db)
    .await?;
    ok(json!({
        "items": rows,
        "from": from,
        "to": to,
        "showViews": settings.activity_show_views,
        "auditRetentionDays": settings.audit_retention_days,
    }))
}

// ---------------------------------------------------------------------------
// audit log
// ---------------------------------------------------------------------------

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct EventRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    team_id: Option<Uuid>,
    team_name: Option<String>,
    name: String,
    actor_id: Option<Uuid>,
    actor_name: Option<String>,
    actor_email: Option<String>,
    target_type: Option<String>,
    target_id: Option<Uuid>,
    target_name: Option<String>,
    /// to link the document or collection when it still exists
    target_url_id: Option<String>,
    data: Value,
    ip: Option<String>,
    user_agent: Option<String>,
    via: Option<String>,
}

const EVENT_SELECT: &str = "SELECT e.id, e.created_at, e.team_id, t.name AS team_name, e.name, e.actor_id, \
     COALESCE(u.name, e.actor_name) AS actor_name, COALESCE(u.email, e.actor_email) AS actor_email, \
     e.target_type, e.target_id, e.target_name, \
     CASE e.target_type \
       WHEN 'document' THEN (SELECT d.url_id FROM documents d WHERE d.id = e.target_id AND d.deleted_at IS NULL) \
       WHEN 'collection' THEN (SELECT c.url_id FROM collections c WHERE c.id = e.target_id AND c.deleted_at IS NULL) \
     END AS target_url_id, \
     e.data, e.ip, e.user_agent, e.via \
     FROM events e LEFT JOIN teams t ON t.id = e.team_id LEFT JOIN users u ON u.id = e.actor_id";

/// The filters shared by both logs, from $3 on: $3 user, $4 name patterns, $5 search, $6 from, $7 to
const EVENT_FILTERS: &str = "AND ($3::uuid IS NULL OR e.actor_id = $3) AND ($4::text[] IS NULL OR e.name LIKE ANY($4)) \
     AND ($5::text IS NULL OR e.actor_name ILIKE $5 OR e.actor_email ILIKE $5 OR e.target_name ILIKE $5 OR e.ip ILIKE $5) \
     AND e.created_at >= $6 AND e.created_at <= $7 \
     ORDER BY e.created_at DESC LIMIT $1 OFFSET $2";

async fn events_page(st: &AppState, sql: String, team: Option<Uuid>, f: &Filter) -> ApiResult<Json<Value>> {
    let (limit, offset) = f.page();
    let (from, to) = f.range(None);
    let names: Option<Vec<String>> = category(f.category.as_deref())?.map(|v| v.into_iter().map(String::from).collect());
    let mut q = sqlx::query_as::<_, EventRow>(&sql)
        .bind(limit + 1)
        .bind(offset)
        .bind(f.user_id)
        .bind(names)
        .bind(f.q())
        .bind(from)
        .bind(to);
    if let Some(t) = team {
        q = q.bind(t);
    }
    let rows = q.fetch_all(&st.db).await?;
    let more = rows.len() as i64 > limit;
    let settings = st.settings();
    ok(json!({
        "items": rows.into_iter().take(limit as usize).collect::<Vec<_>>(),
        "hasMore": more,
        "auditRetentionDays": settings.audit_retention_days,
    }))
}

/// The audit log of the workspace: its events and the sign-ins of its members.
pub async fn events(State(st): State<AppState>, auth: Auth, Json(f): Json<Filter>) -> ApiResult<Json<Value>> {
    require_activity(&auth)?;
    let sql = format!(
        "{EVENT_SELECT} WHERE (e.team_id = $8 OR (e.team_id IS NULL AND e.name LIKE 'auth.%' \
         AND e.actor_id IN (SELECT user_id FROM team_memberships WHERE team_id = $8))) {EVENT_FILTERS}"
    );
    events_page(&st, sql, Some(auth.team()?), &f).await
}

/// The audit log of the installation (superadmins): sign-ins, accounts, general settings, workspaces.
pub async fn instance_events(State(st): State<AppState>, auth: Auth, Json(f): Json<Filter>) -> ApiResult<Json<Value>> {
    auth.require_superadmin().map_err(|_| ApiError::forbidden("Only superadmins can see the audit log of the installation"))?;
    let sql = format!("{EVENT_SELECT} WHERE e.team_id IS NULL {EVENT_FILTERS}");
    events_page(&st, sql, None, &f).await
}
