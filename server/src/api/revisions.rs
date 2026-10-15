use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiResult},
    models::Access,
    perm::load_doc,
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct RevisionRow {
    id: Uuid,
    document_id: Uuid,
    title: String,
    emoji: Option<String>,
    created_at: DateTime<Utc>,
    user_id: Uuid,
    user_name: String,
    user_color: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReq {
    document_id: String,
}

pub async fn list(State(st): State<AppState>, auth: Auth, Json(b): Json<ListReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT r.id, r.document_id, r.title, r.emoji, r.created_at, r.user_id, u.name AS user_name, u.color AS user_color \
         FROM revisions r JOIN users u ON u.id = r.user_id WHERE r.document_id = $1 ORDER BY r.created_at DESC LIMIT 200",
    )
    .bind(d.id)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct RevisionFull {
    id: Uuid,
    document_id: Uuid,
    title: String,
    emoji: Option<String>,
    text: String,
    created_at: DateTime<Utc>,
    user_id: Uuid,
    user_name: String,
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let r: RevisionFull = sqlx::query_as(
        "SELECT r.id, r.document_id, r.title, r.emoji, r.text, r.created_at, r.user_id, u.name AS user_name \
         FROM revisions r JOIN users u ON u.id = r.user_id WHERE r.id = $1",
    )
    .bind(b.id)
    .fetch_one(&st.db)
    .await?;
    load_doc(&st.db, &auth, &r.document_id.to_string(), Access::Read).await?;
    ok(r)
}
