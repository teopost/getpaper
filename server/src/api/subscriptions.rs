//! Following a document or a collection (see `crate::subscriptions` for the notifications).

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::Access,
    perm::{load_collection, load_doc},
    AppState,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetReq {
    document_id: Option<String>,
    collection_id: Option<String>,
}

/// The document or collection, checked for read access
async fn target(st: &AppState, auth: &Auth, b: &TargetReq) -> ApiResult<(Option<Uuid>, Option<Uuid>)> {
    match (&b.document_id, &b.collection_id) {
        (Some(d), _) => {
            let (d, _) = load_doc(&st.db, auth, d, Access::Read).await?;
            if d.published_at.is_none() || d.template {
                return Err(ApiError::bad_request("Only published documents can be followed"));
            }
            Ok((Some(d.id), None))
        }
        (None, Some(c)) => Ok((None, Some(load_collection(&st.db, auth, c, Access::Read).await?.0.id))),
        _ => Err(ApiError::bad_request("documentId or collectionId is required")),
    }
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<TargetReq>) -> ApiResult<Json<Value>> {
    let (doc, col) = target(&st, &auth, &b).await?;
    let (subscribed,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM subscriptions WHERE user_id = $1 AND active \
         AND document_id IS NOT DISTINCT FROM $2 AND collection_id IS NOT DISTINCT FROM $3)",
    )
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .fetch_one(&st.db)
    .await?;
    ok(json!({ "subscribed": subscribed }))
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<TargetReq>) -> ApiResult<Json<Value>> {
    let (doc, col) = target(&st, &auth, &b).await?;
    let updated = sqlx::query(
        "UPDATE subscriptions SET active = true WHERE user_id = $1 AND document_id IS NOT DISTINCT FROM $2 AND collection_id IS NOT DISTINCT FROM $3",
    )
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .execute(&st.db)
    .await?;
    if updated.rows_affected() == 0 {
        sqlx::query("INSERT INTO subscriptions (user_id, document_id, collection_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(auth.user.id)
            .bind(doc)
            .bind(col)
            .execute(&st.db)
            .await?;
    }
    ok(json!({ "subscribed": true }))
}

/// Stops following; the row stays (inactive) so editing the document doesn't follow it again.
pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<TargetReq>) -> ApiResult<Json<Value>> {
    let (doc, col) = target(&st, &auth, &b).await?;
    sqlx::query(
        "INSERT INTO subscriptions (user_id, document_id, collection_id, active) VALUES ($1, $2, $3, false) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .execute(&st.db)
    .await?;
    sqlx::query(
        "UPDATE subscriptions SET active = false WHERE user_id = $1 AND document_id IS NOT DISTINCT FROM $2 AND collection_id IS NOT DISTINCT FROM $3",
    )
    .bind(auth.user.id)
    .bind(doc)
    .bind(col)
    .execute(&st.db)
    .await?;
    ok(json!({ "subscribed": false }))
}
