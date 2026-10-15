use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::Auth,
    error::{ok, ApiResult},
    models::{Access, DocRow, DOC_COLUMNS, DOC_FROM},
    perm::{load_doc, readable_collection_ids},
    AppState,
};

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let ids = readable_collection_ids(&st.db, &auth).await?;
    let sql = format!(
        "SELECT {DOC_COLUMNS} {DOC_FROM} JOIN stars st ON st.document_id = d.id AND st.user_id = $1 \
         WHERE d.team_id = $2 AND d.deleted_at IS NULL AND d.archived_at IS NULL \
         AND (d.collection_id = ANY($3) OR (d.collection_id IS NULL AND d.created_by = $1)) \
         AND (d.published_at IS NOT NULL OR d.created_by = $1) ORDER BY st.created_at"
    );
    let docs: Vec<DocRow> = sqlx::query_as(&sql)
        .bind(auth.user.id)
        .bind(auth.team()?)
        .bind(&ids)
        .fetch_all(&st.db)
        .await?;
    ok(docs)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarReq {
    document_id: String,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<StarReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    sqlx::query("INSERT INTO stars (user_id, document_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(auth.user.id)
        .bind(d.id)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<StarReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    sqlx::query("DELETE FROM stars WHERE user_id = $1 AND document_id = $2")
        .bind(auth.user.id)
        .bind(d.id)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}
