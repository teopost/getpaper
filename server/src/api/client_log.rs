//! Errors reported by the web client (uncaught exceptions, failed renders), written to the server log
//! so problems seen only in a user's browser can be diagnosed.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;

use crate::{auth::Auth, error::{ok, ApiError, ApiResult}, AppState};

#[derive(Deserialize)]
pub struct ClientError {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    stack: String,
    #[serde(default)]
    url: String,
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect::<String>().replace('\n', " | ")
}

pub async fn log(State(_st): State<AppState>, auth: Result<Auth, ApiError>, headers: HeaderMap, Json(b): Json<ClientError>) -> ApiResult<Json<Value>> {
    let user = auth.as_ref().map(|a| a.user.email.clone()).unwrap_or_else(|_| "anonymous".into());
    let agent = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("");
    tracing::warn!(
        user = %user,
        kind = %cut(&b.kind, 40),
        url = %cut(&b.url, 300),
        agent = %cut(agent, 200),
        stack = %cut(&b.stack, 1500),
        "client error: {}",
        cut(&b.message, 500)
    );
    ok(serde_json::json!({ "logged": true }))
}
