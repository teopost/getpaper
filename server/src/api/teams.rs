use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Team, User},
    util::{new_url_id, non_empty, random_color, valid_role},
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub member_count: i64,
}

pub async fn my_teams(db: &PgPool, user_id: Uuid) -> ApiResult<Vec<TeamSummary>> {
    Ok(sqlx::query_as(
        "SELECT t.id, t.name, t.color, t.avatar_url, m.role, \
         (SELECT count(*) FROM team_memberships m2 WHERE m2.team_id = t.id AND m2.suspended_at IS NULL) AS member_count \
         FROM team_memberships m JOIN teams t ON t.id = m.team_id WHERE m.user_id = $1 AND m.suspended_at IS NULL ORDER BY t.created_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

const WELCOME_TEXT: &str = r#"Welcome to your new workspace! This is a document inside the **Welcome** collection.

## Getting started

- Use the sidebar to browse **collections** and nested documents
- Press `/` inside a document to insert headings, lists, tables, code blocks and more
- Select some text to format it with the floating toolbar
- Use **Share** to publish a document to the web with a public link
- Switch between **workspaces** from the menu at the top-left of the sidebar

:::info
Everything is saved automatically while you type.
:::

## Markdown shortcuts

| Shortcut | Result |
| --- | --- |
| `# ` | Heading |
| `- ` | Bulleted list |
| `[] ` | Todo list |
| `> ` | Quote |
| ```` ``` ```` | Code block |
"#;

pub async fn create_team(db: &PgPool, user: &User, name: &str, product: &str) -> ApiResult<Team> {
    let mut tx = db.begin().await?;
    let team: Team = sqlx::query_as("INSERT INTO teams (id, name, color, created_by) VALUES ($1, $2, $3, $4) RETURNING *")
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(random_color())
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO team_memberships (team_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(team.id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let cid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO collections (id, url_id, team_id, name, description, color, icon, permission, created_by) \
         VALUES ($1, $2, $3, 'Welcome', 'This collection is a great place to start writing.', '#0366d6', 'home', 'read_write', $4)",
    )
    .bind(cid)
    .bind(new_url_id())
    .bind(team.id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let did = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, url_id, team_id, collection_id, title, emoji, text, created_by, updated_by, published_at, position) \
         VALUES ($1, $2, $3, $4, 'Welcome to ' || $7, '👋', $5, $6, $6, now(), 1)",
    )
    .bind(did)
    .bind(new_url_id())
    .bind(team.id)
    .bind(cid)
    .bind(WELCOME_TEXT)
    .bind(user.id)
    .bind(product)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(team)
}

pub async fn info(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let team: Team = sqlx::query_as("SELECT * FROM teams WHERE id = $1")
        .bind(auth.team()?)
        .fetch_one(&st.db)
        .await?;
    ok(team)
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    ok(my_teams(&st.db, auth.user.id).await?)
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let name = non_empty(&b.name, "Name")?;
    let team = create_team(&st.db, &auth.user, &name, &st.settings().product_name()).await?;
    if let Some(h) = &auth.session_hash {
        sqlx::query("UPDATE sessions SET team_id = $1 WHERE token_hash = $2")
            .bind(team.id)
            .bind(h)
            .execute(&st.db)
            .await?;
    }
    // an event of the installation (superadmins) and of the new workspace (its admin)
    audit::record_in(&st, &auth, None, "teams.create", Target::new("workspace", team.id, team.name.clone()), json!({})).await;
    audit::record_in(&st, &auth, Some(team.id), "teams.create", Target::new("workspace", team.id, team.name.clone()), json!({})).await;
    ok(team)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn switch(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let m: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2 AND suspended_at IS NULL")
        .bind(auth.user.id)
        .bind(b.id)
        .fetch_optional(&st.db)
        .await?;
    if m.is_none() {
        return Err(ApiError::not_found("Workspace not found"));
    }
    let h = auth
        .session_hash
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("API keys cannot switch workspace"))?;
    sqlx::query("UPDATE sessions SET team_id = $1 WHERE token_hash = $2")
        .bind(b.id)
        .bind(h)
        .execute(&st.db)
        .await?;
    ok(json!({ "success": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReq {
    name: Option<String>,
    color: Option<String>,
    sharing: Option<bool>,
    default_user_role: Option<String>,
    allowed_domains: Option<String>,
    /// logo: an image uploaded to this workspace (attachments.create), "" removes it
    avatar_url: Option<String>,
}

/// The logo must be a raster image uploaded to the workspace, addressed as /files/<id>/<name>.
async fn valid_logo(st: &AppState, team: Uuid, url: &str) -> ApiResult<Option<String>> {
    if url.is_empty() {
        return Ok(None);
    }
    let id = url
        .strip_prefix("/files/")
        .and_then(|rest| rest.split('/').next())
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::bad_request("The logo must be an image uploaded to the workspace"))?;
    let ct: Option<(String,)> = sqlx::query_as("SELECT content_type FROM attachments WHERE id = $1 AND team_id = $2")
        .bind(id)
        .bind(team)
        .fetch_optional(&st.db)
        .await?;
    match ct {
        Some((ct,)) if ct.starts_with("image/") && ct != "image/svg+xml" => Ok(Some(url.to_string())),
        _ => Err(ApiError::bad_request("The logo must be an image uploaded to the workspace")),
    }
}

pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    auth.require_admin()?;
    if let Some(r) = &b.default_user_role {
        valid_role(r)?;
    }
    let name = match &b.name {
        Some(n) => Some(non_empty(n, "Name")?),
        None => None,
    };
    let team_id = auth.team()?;
    let logo = match &b.avatar_url {
        Some(url) => Some(valid_logo(&st, team_id, url.trim()).await?),
        None => None,
    };
    let before: Team = sqlx::query_as("SELECT * FROM teams WHERE id = $1").bind(team_id).fetch_one(&st.db).await?;
    let team: Team = sqlx::query_as(
        "UPDATE teams SET name = COALESCE($2, name), color = COALESCE($3, color), sharing = COALESCE($4, sharing), \
         default_user_role = COALESCE($5, default_user_role), allowed_domains = COALESCE($6, allowed_domains), \
         avatar_url = CASE WHEN $7 THEN $8 ELSE avatar_url END, \
         updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(team_id)
    .bind(name)
    .bind(&b.color)
    .bind(b.sharing)
    .bind(&b.default_user_role)
    .bind(b.allowed_domains.as_ref().map(|d| {
        d.split([',', ' ', ';', '\n'])
            .map(|x| x.trim().trim_start_matches('@').to_lowercase())
            .filter(|x| x.contains('.'))
            .collect::<Vec<_>>()
            .join(", ")
    }))
    .bind(logo.is_some())
    .bind(logo.flatten())
    .fetch_one(&st.db)
    .await?;
    let mut changes = serde_json::Map::new();
    let mut changed = |key: &str, from: Value, to: Value| {
        if from != to {
            changes.insert(key.into(), json!({ "from": from, "to": to }));
        }
    };
    changed("name", json!(before.name), json!(team.name));
    changed("sharing", json!(before.sharing), json!(team.sharing));
    changed("defaultUserRole", json!(before.default_user_role), json!(team.default_user_role));
    changed("allowedDomains", json!(before.allowed_domains), json!(team.allowed_domains));
    changed("logo", json!(before.avatar_url.is_some()), json!(team.avatar_url.is_some()));
    if !changes.is_empty() {
        audit::record(&st, &auth, "teams.update", Target::new("workspace", team.id, team.name.clone()), Value::Object(changes)).await;
    }
    ok(team)
}

#[derive(Deserialize)]
pub struct DeleteReq {
    id: Uuid,
    name: String,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<DeleteReq>) -> ApiResult<Json<Value>> {
    let role: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2 AND suspended_at IS NULL")
        .bind(auth.user.id)
        .bind(b.id)
        .fetch_optional(&st.db)
        .await?;
    if role.map(|r| r.0) != Some("admin".into()) {
        return Err(ApiError::forbidden("Only workspace admins can delete a workspace"));
    }
    let team: Team = sqlx::query_as("SELECT * FROM teams WHERE id = $1").bind(b.id).fetch_one(&st.db).await?;
    if team.name.trim() != b.name.trim() {
        return Err(ApiError::bad_request("The workspace name does not match"));
    }
    let mut tx = st.db.begin().await?;
    // documents reference themselves; delete them first to keep things simple
    sqlx::query("DELETE FROM documents WHERE team_id = $1").bind(b.id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM teams WHERE id = $1").bind(b.id).execute(&mut *tx).await?;
    tx.commit().await?;
    // the events of the workspace are deleted with it: this one is kept for the superadmins
    audit::record_in(&st, &auth, None, "teams.delete", Target::new("workspace", team.id, team.name.clone()), json!({})).await;
    ok(json!({ "success": true }))
}
