use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiResult},
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct NotificationRow {
    id: Uuid,
    event: String,
    created_at: DateTime<Utc>,
    viewed_at: Option<DateTime<Utc>>,
    actor_name: Option<String>,
    actor_color: Option<String>,
    document_id: Uuid,
    document_url_id: String,
    document_title: String,
    document_emoji: Option<String>,
    /// for group mentions
    group_name: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ListReq {
    limit: Option<i64>,
}

/// Notifications of the current user in the active workspace, newest first, with the unread count.
/// Documents moved to the trash are left out.
pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let team = auth.team()?;
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT n.id, n.event, n.created_at, n.viewed_at, a.name AS actor_name, a.color AS actor_color, \
         d.id AS document_id, d.url_id AS document_url_id, d.title AS document_title, d.emoji AS document_emoji, \
         g.name AS group_name \
         FROM notifications n JOIN documents d ON d.id = n.document_id LEFT JOIN users a ON a.id = n.actor_id \
         LEFT JOIN groups g ON g.id = n.group_id \
         WHERE n.user_id = $1 AND n.team_id = $2 AND d.deleted_at IS NULL ORDER BY n.created_at DESC LIMIT $3",
    )
    .bind(auth.user.id)
    .bind(team)
    .bind(b.limit.unwrap_or(25).clamp(1, 100))
    .fetch_all(&st.db)
    .await?;
    let (unread,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM notifications n JOIN documents d ON d.id = n.document_id \
         WHERE n.user_id = $1 AND n.team_id = $2 AND n.viewed_at IS NULL AND d.deleted_at IS NULL",
    )
    .bind(auth.user.id)
    .bind(team)
    .fetch_one(&st.db)
    .await?;
    ok(json!({ "notifications": rows, "unread": unread }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MarkReq {
    /// one notification; without it every notification of the active workspace
    id: Option<Uuid>,
    /// the notifications about this document (it has been opened)
    document_id: Option<Uuid>,
}

pub async fn mark_read(State(st): State<AppState>, auth: Auth, Json(b): Json<MarkReq>) -> ApiResult<Json<Value>> {
    sqlx::query(
        "UPDATE notifications SET viewed_at = now() WHERE user_id = $1 AND team_id = $2 AND viewed_at IS NULL \
         AND ($3::uuid IS NULL OR id = $3) AND ($4::uuid IS NULL OR document_id = $4)",
    )
    .bind(auth.user.id)
    .bind(auth.team()?)
    .bind(b.id)
    .bind(b.document_id)
    .execute(&st.db)
    .await?;
    ok(json!({}))
}
