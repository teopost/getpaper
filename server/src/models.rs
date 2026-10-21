use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    #[serde(skip)]
    pub password_hash: String,
    pub color: String,
    pub avatar_url: Option<String>,
    pub content_width: Option<String>,
    pub language: Option<String>,
    pub theme: Option<String>,
    /// manages the installation settings
    pub is_superadmin: bool,
    /// also send an email when someone mentions the user
    pub email_on_mention: bool,
    /// also send an email for followed documents and documents shared with the user
    #[sqlx(default)]
    pub email_on_subscription: bool,
    pub created_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    /// suspended by a superadmin: can't sign in anywhere
    #[sqlx(default)]
    pub suspended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub avatar_url: Option<String>,
    pub sharing: bool,
    pub default_user_role: String,
    pub allowed_domains: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: Uuid,
    pub url_id: String,
    pub team_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub icon: Option<String>,
    pub permission: Option<String>,
    pub sharing: bool,
    pub sort_index: f64,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocRow {
    pub id: Uuid,
    pub url_id: String,
    pub team_id: Uuid,
    pub collection_id: Option<Uuid>,
    pub parent_document_id: Option<Uuid>,
    pub title: String,
    pub emoji: Option<String>,
    pub text: String,
    pub template: bool,
    pub full_width: bool,
    pub position: f64,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_by_name: String,
    pub created_by_color: String,
    pub updated_by_name: String,
    pub updated_by_color: String,
    pub is_starred: bool,
}

/// Column list for DocRow; `$1` must always be bound to the current user id.
pub const DOC_COLUMNS: &str = "d.id, d.url_id, d.team_id, d.collection_id, d.parent_document_id, d.title, d.emoji, d.text, \
    d.template, d.full_width, d.position, d.created_by, d.updated_by, d.created_at, d.updated_at, d.published_at, \
    d.archived_at, d.deleted_at, cu.name AS created_by_name, cu.color AS created_by_color, \
    uu.name AS updated_by_name, uu.color AS updated_by_color, \
    EXISTS (SELECT 1 FROM stars s WHERE s.document_id = d.id AND s.user_id = $1) AS is_starred";

pub const DOC_FROM: &str = "FROM documents d JOIN users cu ON cu.id = d.created_by JOIN users uu ON uu.id = d.updated_by";

pub fn doc_select() -> String {
    format!("SELECT {DOC_COLUMNS} {DOC_FROM}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    None,
    Read,
    Write,
}

impl Access {
    pub fn as_str(&self) -> &'static str {
        match self {
            Access::None => "none",
            Access::Read => "read",
            Access::Write => "write",
        }
    }
}
