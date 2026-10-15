use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap},
};
use std::sync::Arc;

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
    /// Role in `team_id`; for API keys already limited by the key (see `key_role`)
    pub role: Option<String>,
    /// Hash of the session token (None when authenticated with an API key)
    pub session_hash: Option<String>,
    /// Limits of the API key used, None for browser sessions
    pub key: Option<Arc<KeyScope>>,
}

/// What an API key allows besides the user's own permissions.
#[derive(Debug)]
pub struct KeyScope {
    /// false: read-only (acts as a viewer)
    pub write: bool,
    /// workspaces the key can be used in; the first is the default one
    pub teams: Vec<Uuid>,
    /// when Some, only these collections are visible
    pub collections: Option<Vec<Uuid>>,
}

impl KeyScope {
    pub fn allows_collection(&self, id: Uuid) -> bool {
        self.collections.as_ref().map_or(true, |c| c.contains(&id))
    }
}

/// Role of a user acting through an API key: read-only keys act as viewers, the others keep the
/// user's role for content. Workspace administration is never allowed with a key (`require_admin`).
pub fn key_role(user_role: &str, key: &KeyScope) -> String {
    if key.write {
        user_role.into()
    } else {
        "viewer".into()
    }
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
        if self.key.is_some() {
            return Err(ApiError::forbidden("API keys can't administer the workspace"));
        }
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden("Only workspace admins can do this"))
        }
    }
    pub fn require_superadmin(&self) -> ApiResult<()> {
        // the general settings can't be changed through an API key
        if self.user.is_superadmin && self.key.is_none() {
            Ok(())
        } else {
            Err(ApiError::forbidden("Only superadmins can change the general settings"))
        }
    }
    /// The same user acting in another workspace (MCP `workspace` argument), with the role there.
    pub async fn for_team(&self, db: &sqlx::PgPool, team: Uuid) -> ApiResult<Auth> {
        if let Some(k) = &self.key {
            if !k.teams.contains(&team) {
                return Err(ApiError::forbidden("This API key can't be used in that workspace"));
            }
        }
        let role: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2")
            .bind(self.user.id)
            .bind(team)
            .fetch_optional(db)
            .await?;
        let role = role.ok_or_else(|| ApiError::forbidden("You are not a member of that workspace"))?.0;
        let mut a = self.clone();
        a.team_id = Some(team);
        a.role = Some(match &self.key {
            Some(k) => key_role(&role, k),
            None => role,
        });
        Ok(a)
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

        let mut key: Option<Arc<KeyScope>> = None;
        let (user_id, preferred_team, is_session): (Uuid, Option<Uuid>, bool) = if token.starts_with("ol_api_") {
            let row: Option<(Uuid, Uuid, Uuid, String, bool, bool)> = sqlx::query_as(
                "UPDATE api_keys SET last_used_at = now() WHERE secret_hash = $1 AND (expires_at IS NULL OR expires_at > now()) \
                 RETURNING id, user_id, team_id, access, all_workspaces, collections_restricted",
            )
            .bind(&hash)
            .fetch_optional(db)
            .await?;
            let (kid, u, default_team, access, all_workspaces, restricted) = row.ok_or_else(ApiError::unauthorized)?;
            let mut teams: Vec<Uuid> = if all_workspaces {
                sqlx::query_as::<_, (Uuid,)>("SELECT team_id FROM team_memberships WHERE user_id = $1 ORDER BY created_at")
                    .bind(u)
                    .fetch_all(db)
                    .await?
            } else {
                sqlx::query_as::<_, (Uuid,)>(
                    "SELECT k.team_id FROM api_key_workspaces k JOIN team_memberships m ON m.team_id = k.team_id AND m.user_id = $2 \
                     WHERE k.api_key_id = $1",
                )
                .bind(kid)
                .bind(u)
                .fetch_all(db)
                .await?
            }
            .into_iter()
            .map(|r| r.0)
            .collect();
            // the default workspace first
            if let Some(i) = teams.iter().position(|t| *t == default_team) {
                teams.swap(0, i);
            }
            let collections = if restricted {
                Some(
                    sqlx::query_as::<_, (Uuid,)>("SELECT collection_id FROM api_key_collections WHERE api_key_id = $1")
                        .bind(kid)
                        .fetch_all(db)
                        .await?
                        .into_iter()
                        .map(|r| r.0)
                        .collect(),
                )
            } else {
                None
            };
            // REST clients can pick the workspace with "X-Workspace: <id>"
            let wanted = parts
                .headers
                .get("x-workspace")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| Uuid::parse_str(v.trim()).ok());
            let team = match wanted {
                Some(w) if teams.contains(&w) => w,
                Some(_) => return Err(ApiError::forbidden("This API key can't be used in that workspace")),
                None => *teams.first().ok_or_else(ApiError::unauthorized)?,
            };
            key = Some(Arc::new(KeyScope { write: access == "write", teams, collections }));
            (u, Some(team), false)
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

        let role = match (&key, membership.as_ref()) {
            (Some(k), Some(m)) => Some(key_role(&m.1, k)),
            (_, m) => m.map(|m| m.1.clone()),
        };
        Ok(Auth {
            user,
            team_id: membership.as_ref().map(|m| m.0),
            role,
            session_hash: if is_session { Some(hash) } else { None },
            key,
        })
    }
}
