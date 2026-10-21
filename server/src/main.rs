mod api;
mod audit;
mod auth;
mod error;
mod html_md;
mod instance;
mod mail;
mod mentions;
mod models;
mod perm;
mod util;

use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue},
    routing::{get, post},
    Router,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

pub struct MicrosoftConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Directory (tenant) id, or "organizations" / "common" for multi-tenant apps
    pub tenant: String,
    pub authority: String,
    /// If not empty, only these email domains may sign in with Microsoft
    pub allowed_domains: Vec<String>,
    /// Create accounts for unknown users on first sign-in
    pub auto_provision: bool,
    /// Path of the redirect URI registered in Entra ID. "/auth/azure.callback" reuses the
    /// registration of an Outline instance on the same domain.
    pub callback_path: String,
}

/// Paths the Microsoft sign-in can return to
pub const MICROSOFT_CALLBACK_PATHS: &[&str] = &["/auth/microsoft/callback", "/auth/azure.callback"];

pub struct Config {
    pub url: String,
    pub upload_dir: PathBuf,
    pub static_dir: PathBuf,
    pub allow_signup: bool,
    pub secure_cookies: bool,
    pub max_upload_mb: usize,
    pub microsoft: Option<MicrosoftConfig>,
    /// Email/password authentication enabled
    pub password_login: bool,
    /// Preferences used when a user hasn't chosen their own (overridable in the general settings)
    pub defaults: Defaults,
    /// Accounts that are always superadmins (SUPERADMIN_EMAILS), e.g. to recover access
    pub superadmin_emails: Vec<String>,
    /// Outgoing email (SMTP_*); None disables invitation emails
    pub smtp: Option<mail::SmtpConfig>,
}

pub const LANGUAGES: &[&str] = &["en", "it"];
pub const THEMES: &[&str] = &["system", "light", "dark"];
pub const CONTENT_WIDTHS: &[&str] = &["normal", "wide", "full"];

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    pub language: String,
    pub theme: String,
    pub content_width: String,
}

fn env_choice(k: &str, allowed: &[&str], default: &str) -> String {
    match std::env::var(k).map(|v| v.trim().to_lowercase()) {
        Ok(v) if allowed.contains(&v.as_str()) => v,
        Ok(v) if !v.is_empty() => {
            tracing::warn!("{k}={v} is not one of {allowed:?}, using {default}");
            default.into()
        }
        _ => default.into(),
    }
}

fn env_list(k: &str) -> Vec<String> {
    std::env::var(k)
        .unwrap_or_default()
        .split([',', ' ', ';'])
        .map(|s| s.trim().trim_start_matches('@').to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub instance: instance::SharedSettings,
}

impl AppState {
    /// Current installation settings
    pub fn settings(&self) -> instance::InstanceSettings {
        self.instance.read().unwrap().clone()
    }
    /// Defaults for users without a preference: general settings, else environment
    pub fn defaults(&self) -> Defaults {
        self.settings().defaults(&self.cfg.defaults)
    }
}

fn env_bool(k: &str, default: bool) -> bool {
    std::env::var(k)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tower_http=info".into()))
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let url = std::env::var("URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let microsoft = match (
        std::env::var("MICROSOFT_CLIENT_ID").ok().filter(|v| !v.trim().is_empty()),
        std::env::var("MICROSOFT_CLIENT_SECRET").ok().filter(|v| !v.trim().is_empty()),
    ) {
        (Some(client_id), Some(client_secret)) => Some(MicrosoftConfig {
            client_id: client_id.trim().to_string(),
            client_secret: client_secret.trim().to_string(),
            tenant: std::env::var("MICROSOFT_TENANT_ID")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "organizations".into()),
            authority: std::env::var("MICROSOFT_AUTHORITY")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "https://login.microsoftonline.com".into())
                .trim_end_matches('/')
                .to_string(),
            allowed_domains: env_list("MICROSOFT_ALLOWED_DOMAINS"),
            auto_provision: env_bool("MICROSOFT_AUTO_PROVISION", true),
            callback_path: env_choice("MICROSOFT_CALLBACK_PATH", MICROSOFT_CALLBACK_PATHS, MICROSOFT_CALLBACK_PATHS[0]),
        }),
        _ => None,
    };
    if microsoft.is_some() {
        tracing::info!("Microsoft sign-in enabled");
    }
    // without another way to sign in, password login can't be disabled
    let password_login = env_bool("PASSWORD_LOGIN", true) || microsoft.is_none();
    let cfg = Config {
        secure_cookies: url.starts_with("https://"),
        url,
        upload_dir: std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "/data/uploads".into()).into(),
        static_dir: std::env::var("STATIC_DIR").unwrap_or_else(|_| "./public".into()).into(),
        allow_signup: env_bool("ALLOW_SIGNUP", true),
        max_upload_mb: std::env::var("MAX_UPLOAD_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(25),
        microsoft,
        password_login,
        defaults: Defaults {
            language: env_choice("DEFAULT_LANGUAGE", LANGUAGES, "en"),
            theme: env_choice("DEFAULT_THEME", THEMES, "system"),
            content_width: env_choice("DEFAULT_CONTENT_WIDTH", CONTENT_WIDTHS, "normal"),
        },
        superadmin_emails: env_list("SUPERADMIN_EMAILS"),
        smtp: mail::SmtpConfig::from_env(),
    };
    if let Some(s) = &cfg.smtp {
        tracing::info!(host = %s.host, port = s.port, secure = %s.secure, "email enabled");
    }
    std::fs::create_dir_all(&cfg.upload_dir)?;

    let db = loop {
        match PgPoolOptions::new().max_connections(20).connect(&database_url).await {
            Ok(p) => break p,
            Err(e) => {
                tracing::warn!("waiting for database: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    sqlx::migrate!("./migrations").run(&db).await?;
    if !cfg.superadmin_emails.is_empty() {
        sqlx::query("UPDATE users SET is_superadmin = true WHERE lower(email) = ANY($1)")
            .bind(&cfg.superadmin_emails)
            .execute(&db)
            .await?;
    }
    let settings = instance::load(&db).await.map_err(|e| anyhow::anyhow!("cannot load settings: {}", e.message))?;
    tracing::info!("database migrated");

    // periodic cleanup of expired sessions, old trash and old audit events
    let instance = Arc::new(std::sync::RwLock::new(settings));
    {
        let db = db.clone();
        let instance = instance.clone();
        tokio::spawn(async move {
            loop {
                let retention = instance.read().map(|s| s.audit_retention_days).unwrap_or(0);
                audit::cleanup(&db, retention).await;
                let _ = sqlx::query("DELETE FROM sessions WHERE expires_at < now()").execute(&db).await;
                let _ = sqlx::query(
                    "DELETE FROM documents WHERE deleted_at IS NOT NULL AND deleted_at < now() - interval '30 days'",
                )
                .execute(&db)
                .await;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
    }

    let static_dir: PathBuf = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./public".into()).into();
    let index = static_dir.join("index.html");
    let body_limit = cfg.max_upload_mb * 1024 * 1024 + 1024 * 1024;
    let state = AppState { db, cfg: Arc::new(cfg), instance };

    let app = Router::new()
        .nest("/api", api::router())
        .route("/files/:id/:name", get(api::attachments::serve))
        .route("/auth/microsoft", get(api::sso::start))
        .route("/auth/microsoft/callback", get(api::sso::callback))
        // same redirect URI as Outline (see MICROSOFT_CALLBACK_PATH)
        .route("/auth/azure.callback", get(api::sso::callback))
        .route("/s/:share", get(api::sso::shared_page))
        .route("/mcp", post(api::mcp::handle).get(api::mcp::get_not_allowed).delete(api::mcp::get_not_allowed))
        // Only static files are compressed. API answers go out whole with a Content-Length: behind
        // Microsoft Entra application proxy, compressed chunked API responses reached browsers truncated.
        // hashed build files: cached for good; a missing one is a 404, not the app page
        .merge(
            Router::new()
                .nest_service("/assets", ServeDir::new(static_dir.join("assets")))
                .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, |res: &axum::response::Response| {
                    res.status().is_success().then(|| HeaderValue::from_static("public, max-age=31536000, immutable"))
                }))
                .layer(CompressionLayer::new()),
        )
        // the app page must be revalidated, or browsers keep an old one pointing to removed files
        .fallback_service(
            Router::new()
                .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(index)))
                .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-cache")))
                .layer(CompressionLayer::new()),
        )
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("listening on :{port}");
    // the connection address is the client address in the audit log when there is no reverse proxy
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
    Ok(())
}
