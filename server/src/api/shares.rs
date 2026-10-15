use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::Access,
    perm::load_doc,
    util::random_string,
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct ShareRow {
    id: Uuid,
    url_id: String,
    document_id: Uuid,
    document_title: String,
    document_url_id: String,
    created_by: Uuid,
    created_by_name: String,
    views: i32,
    last_accessed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

const SHARE_SELECT: &str = "SELECT s.id, s.url_id, s.document_id, d.title AS document_title, d.url_id AS document_url_id, \
    s.created_by, u.name AS created_by_name, s.views, s.last_accessed_at, s.created_at \
    FROM shares s JOIN documents d ON d.id = s.document_id JOIN users u ON u.id = s.created_by";

fn with_url(st: &AppState, s: ShareRow) -> Value {
    let url = format!("{}/s/{}", st.cfg.url.trim_end_matches('/'), s.url_id);
    let mut v = serde_json::to_value(&s).unwrap();
    v["url"] = json!(url);
    v
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.team_id = $1 AND s.revoked_at IS NULL AND d.deleted_at IS NULL \
         AND ($2 OR s.created_by = $3) ORDER BY s.created_at DESC"
    ))
    .bind(auth.team()?)
    .bind(auth.is_admin())
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await?;
    let out: Vec<Value> = rows.into_iter().map(|s| with_url(&st, s)).collect();
    ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocReq {
    document_id: String,
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<DocReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let row: Option<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.document_id = $1 AND s.revoked_at IS NULL ORDER BY s.created_at LIMIT 1"
    ))
    .bind(d.id)
    .fetch_optional(&st.db)
    .await?;
    ok(row.map(|s| with_url(&st, s)))
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<DocReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    if d.published_at.is_none() {
        return Err(ApiError::bad_request("Publish the document before sharing it"));
    }
    if !st.settings().allow_public_sharing {
        return Err(ApiError::forbidden("Public links have been disabled by the administrator"));
    }
    let (sharing,): (bool,) = sqlx::query_as("SELECT sharing FROM teams WHERE id = $1")
        .bind(d.team_id)
        .fetch_one(&st.db)
        .await?;
    if !sharing {
        return Err(ApiError::forbidden("Public sharing is disabled for this workspace"));
    }
    if let Some(cid) = d.collection_id {
        let (cs,): (bool,) = sqlx::query_as("SELECT sharing FROM collections WHERE id = $1")
            .bind(cid)
            .fetch_one(&st.db)
            .await?;
        if !cs {
            return Err(ApiError::forbidden("Public sharing is disabled for this collection"));
        }
    }
    let existing: Option<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.document_id = $1 AND s.revoked_at IS NULL ORDER BY s.created_at LIMIT 1"
    ))
    .bind(d.id)
    .fetch_optional(&st.db)
    .await?;
    if let Some(s) = existing {
        return ok(with_url(&st, s));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id, url_id, team_id, document_id, created_by) VALUES ($1, $2, $3, $4, $5)")
        .bind(id)
        // long random token: public links must not be guessable
        .bind(random_string(24))
        .bind(d.team_id)
        .bind(d.id)
        .bind(auth.user.id)
        .execute(&st.db)
        .await?;
    let s: ShareRow = sqlx::query_as(&format!("{SHARE_SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_one(&st.db)
        .await?;
    ok(with_url(&st, s))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn revoke(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as("SELECT document_id, created_by FROM shares WHERE id = $1 AND revoked_at IS NULL")
        .bind(b.id)
        .fetch_optional(&st.db)
        .await?;
    let (doc_id, created_by) = row.ok_or_else(|| ApiError::not_found("Share not found"))?;
    let (_, access) = load_doc(&st.db, &auth, &doc_id.to_string(), Access::Read).await?;
    if !(created_by == auth.user.id || auth.is_admin() || access == Access::Write) {
        return Err(ApiError::forbidden("You cannot revoke this share"));
    }
    sqlx::query("UPDATE shares SET revoked_at = now() WHERE id = $1")
        .bind(b.id)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
