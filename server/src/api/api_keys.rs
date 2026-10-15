use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiResult},
    util::{non_empty, random_string, sha256_hex},
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct ApiKeyRow {
    id: Uuid,
    name: String,
    last4: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<ApiKeyRow> = sqlx::query_as(
        "SELECT id, name, last4, created_at, last_used_at FROM api_keys WHERE user_id = $1 AND team_id = $2 ORDER BY created_at DESC",
    )
    .bind(auth.user.id)
    .bind(auth.team()?)
    .fetch_all(&st.db)
    .await?;
    ok(rows)
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let name = non_empty(&b.name, "Name")?;
    let secret = format!("ol_api_{}", random_string(38));
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO api_keys (id, user_id, team_id, name, secret_hash, last4) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(id)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&name)
        .bind(sha256_hex(&secret))
        .bind(&secret[secret.len() - 4..])
        .execute(&st.db)
        .await?;
    ok(json!({ "id": id, "name": name, "secret": secret }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    sqlx::query("DELETE FROM api_keys WHERE id = $1 AND user_id = $2")
        .bind(b.id)
        .bind(auth.user.id)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
