//! Installation settings managed by superadmins in the web UI (Settings → General settings).
//!
//! They apply to every workspace: data protection switches (exports, public links), branding
//! (header and accent colour) and the defaults for users without a personal preference, which
//! override DEFAULT_LANGUAGE / DEFAULT_THEME / DEFAULT_CONTENT_WIDTH from the environment.
//! Stored as one JSON document in the `settings` table and cached in memory.

use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    Defaults, CONTENT_WIDTHS, LANGUAGES, THEMES,
};

const KEY: &str = "instance";

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettings {
    /// workspace backups (Settings → Export)
    #[serde(default = "yes")]
    pub allow_backup_export: bool,
    /// Obsidian vault and single document download as Markdown
    #[serde(default = "yes")]
    pub allow_markdown_export: bool,
    /// public links to documents (/s/…); existing links stop working while disabled
    #[serde(default = "yes")]
    pub allow_public_sharing: bool,
    /// background of the top bar, "#rrggbb"
    #[serde(default)]
    pub header_color: Option<String>,
    /// buttons, links and selection, "#rrggbb"
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub default_language: Option<String>,
    #[serde(default)]
    pub default_theme: Option<String>,
    #[serde(default)]
    pub default_content_width: Option<String>,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        serde_json::from_value(json!({})).unwrap()
    }
}

pub type SharedSettings = Arc<RwLock<InstanceSettings>>;

pub async fn load(db: &PgPool) -> ApiResult<InstanceSettings> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM settings WHERE key = $1").bind(KEY).fetch_optional(db).await?;
    Ok(row.and_then(|(v,)| serde_json::from_value(v).ok()).unwrap_or_default())
}

pub async fn save(db: &PgPool, s: &InstanceSettings, user: Uuid) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_by) VALUES ($1, $2, $3) \
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_by = $3, updated_at = now()",
    )
    .bind(KEY)
    .bind(serde_json::to_value(s).map_err(|e| ApiError::internal(e.to_string()))?)
    .bind(user)
    .execute(db)
    .await?;
    Ok(())
}

fn valid_color(c: &Option<String>, what: &str) -> ApiResult<()> {
    match c.as_deref() {
        None => Ok(()),
        Some(c) if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()) => Ok(()),
        _ => Err(ApiError::bad_request(format!("{what} must be a colour like #c8102e"))),
    }
}

fn valid_choice(v: &Option<String>, allowed: &[&str], what: &str) -> ApiResult<()> {
    match v.as_deref() {
        None => Ok(()),
        Some(v) if allowed.contains(&v) => Ok(()),
        _ => Err(ApiError::bad_request(format!("{what} must be one of: {}", allowed.join(", ")))),
    }
}

impl InstanceSettings {
    pub fn validate(&self) -> ApiResult<()> {
        valid_color(&self.header_color, "Header colour")?;
        valid_color(&self.accent_color, "Accent colour")?;
        valid_choice(&self.default_language, LANGUAGES, "Default language")?;
        valid_choice(&self.default_theme, THEMES, "Default theme")?;
        valid_choice(&self.default_content_width, CONTENT_WIDTHS, "Default document width")
    }

    /// Defaults for users without a preference: these settings, else the environment.
    pub fn defaults(&self, env: &Defaults) -> Defaults {
        Defaults {
            language: self.default_language.clone().unwrap_or_else(|| env.language.clone()),
            theme: self.default_theme.clone().unwrap_or_else(|| env.theme.clone()),
            content_width: self.default_content_width.clone().unwrap_or_else(|| env.content_width.clone()),
        }
    }

    /// What every client needs, also before signing in (branding) and on public pages.
    pub fn public(&self) -> Value {
        json!({
            "allowBackupExport": self.allow_backup_export,
            "allowMarkdownExport": self.allow_markdown_export,
            "allowPublicSharing": self.allow_public_sharing,
            "headerColor": self.header_color,
            "accentColor": self.accent_color,
        })
    }
}
