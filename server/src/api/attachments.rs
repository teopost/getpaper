use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    AppState,
};

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "file".into()
    } else {
        s.chars().take(100).collect()
    }
}

pub async fn create(State(st): State<AppState>, auth: Auth, mut mp: Multipart) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let team = auth.team()?;
    let mut document_id: Option<Uuid> = None;
    let mut saved: Option<(Uuid, String, String, usize)> = None;
    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        match field.name().unwrap_or("") {
            "documentId" => {
                document_id = field.text().await.ok().and_then(|t| Uuid::parse_str(&t).ok());
            }
            "file" => {
                let name = sanitize(field.file_name().unwrap_or("file"));
                let ct = field
                    .content_type()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "application/octet-stream".into());
                let bytes = field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?;
                if bytes.len() > st.cfg.max_upload_mb * 1024 * 1024 {
                    return Err(ApiError::bad_request("File is too large"));
                }
                let id = Uuid::new_v4();
                tokio::fs::write(st.cfg.upload_dir.join(id.to_string()), &bytes).await?;
                saved = Some((id, name, ct, bytes.len()));
            }
            _ => {}
        }
    }
    let (id, name, ct, size) = saved.ok_or_else(|| ApiError::bad_request("file is required"))?;
    sqlx::query(
        "INSERT INTO attachments (id, team_id, user_id, document_id, name, content_type, size) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(team)
    .bind(auth.user.id)
    .bind(document_id)
    .bind(&name)
    .bind(&ct)
    .bind(size as i64)
    .execute(&st.db)
    .await?;
    ok(json!({
        "id": id,
        "name": name,
        "contentType": ct,
        "size": size,
        "url": format!("/files/{id}/{name}"),
    }))
}

/// Attachments are addressed by an unguessable UUID so that images keep working in publicly shared documents.
pub async fn serve(State(st): State<AppState>, Path((id, _name)): Path<(Uuid, String)>) -> Response {
    let row: Option<(String, String)> = match sqlx::query_as("SELECT name, content_type FROM attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.db)
        .await
    {
        Ok(r) => r,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some((name, ct)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bytes = match tokio::fs::read(st.cfg.upload_dir.join(id.to_string())).await {
        Ok(b) => b,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let safe_inline = ct.starts_with("image/") || ct == "application/pdf" || ct.starts_with("video/") || ct.starts_with("audio/");
    // SVG is shown inline (diagrams) but can't run scripts or load anything, also when opened on its own
    let svg = ct == "image/svg+xml";
    let (ct, disposition) = if safe_inline {
        (ct, format!("inline; filename=\"{name}\""))
    } else {
        ("application/octet-stream".to_string(), format!("attachment; filename=\"{name}\""))
    };
    let mut res = Response::builder().header(header::CONTENT_TYPE, ct);
    if svg {
        res = res.header(header::CONTENT_SECURITY_POLICY, "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:");
    }
    res
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .unwrap()
}
