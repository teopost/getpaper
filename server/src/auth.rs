use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap},
};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    models::User,
    util::sha256_hex,
    AppState,
};

pub const COOKIE_NAME: &str = "accessToken";

#[derive(Debug, Clone)]
pub struct Auth {
    pub user: User,
    pub team_id: Option<Uuid>,
    pub role: Option<String>,
    /// Hash of the session token (None when authenticated with an API key)
    pub session_hash: Option<String>,
}

impl Auth {
    pub fn team(&self) -> ApiResult<Uuid> {
        self.team_id
            .ok_or_else(|| ApiError::forbidden("You are not a member of any workspace"))
    }
    pub fn role(&self) -> &str {
        self.role.as_deref().unwrap_or("")
    }
    pub fn is_admin(&self) -> bool {
        self.role() == "admin"
    }
    pub fn can_write(&self) -> bool {
        matches!(self.role(), "admin" | "member")
    }
    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden("Only workspace admins can do this"))
        }
    }
    pub fn require_superadmin(&self) -> ApiResult<()> {
        if self.user.is_superadmin {
            Ok(())
        } else {
            Err(ApiError::forbidden("Only superadmins can change the general settings"))
        }
    }
    pub fn require_writer(&self) -> ApiResult<()> {
        if self.can_write() {
            Ok(())
        } else {
            Err(ApiError::forbidden("Viewers cannot make changes"))
        }
    }
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    for v in headers.get_all(header::COOKIE) {
        if let Ok(s) = v.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(t) = part.strip_prefix(&format!("{COOKIE_NAME}=")) {
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

#[async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let token = extract_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        let hash = sha256_hex(&token);
        let db = &st.db;

        let (user_id, preferred_team, is_session): (Uuid, Option<Uuid>, bool) = if token.starts_with("ol_api_") {
            let row: Option<(Uuid, Uuid)> =
                sqlx::query_as("UPDATE api_keys SET last_used_at = now() WHERE secret_hash = $1 RETURNING user_id, team_id")
                    .bind(&hash)
                    .fetch_optional(db)
                    .await?;
            let (u, t) = row.ok_or_else(ApiError::unauthorized)?;
            (u, Some(t), false)
        } else {
            let row: Option<(Uuid, Option<Uuid>)> =
                sqlx::query_as("SELECT user_id, team_id FROM sessions WHERE token_hash = $1 AND expires_at > now()")
                    .bind(&hash)
                    .fetch_optional(db)
                    .await?;
            let (u, t) = row.ok_or_else(ApiError::unauthorized)?;
            (u, t, true)
        };

        let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(ApiError::unauthorized)?;

        let mut membership: Option<(Uuid, String)> = None;
        if let Some(t) = preferred_team {
            membership = sqlx::query_as("SELECT team_id, role FROM team_memberships WHERE user_id = $1 AND team_id = $2")
                .bind(user_id)
                .bind(t)
                .fetch_optional(db)
                .await?;
            if membership.is_none() && !is_session {
                return Err(ApiError::unauthorized());
            }
        }
        if membership.is_none() && is_session {
            membership = sqlx::query_as(
                "SELECT team_id, role FROM team_memberships WHERE user_id = $1 ORDER BY created_at ASC LIMIT 1",
            )
            .bind(user_id)
            .fetch_optional(db)
            .await?;
            if let Some((t, _)) = &membership {
                sqlx::query("UPDATE sessions SET team_id = $1 WHERE token_hash = $2")
                    .bind(t)
                    .bind(&hash)
                    .execute(db)
                    .await?;
            }
        }

        if user.last_active_at.map(|t| (chrono::Utc::now() - t).num_minutes() > 5).unwrap_or(true) {
            sqlx::query("UPDATE users SET last_active_at = now() WHERE id = $1")
                .bind(user_id)
                .execute(db)
                .await?;
        }

        Ok(Auth {
            user,
            team_id: membership.as_ref().map(|m| m.0),
            role: membership.map(|m| m.1),
            session_hash: if is_session { Some(hash) } else { None },
        })
    }
}
