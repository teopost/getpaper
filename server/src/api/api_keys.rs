//! Personal API keys for the REST API and the MCP server.
//!
//! A key belongs to a user and can be limited: read-only or read/write, valid in some workspaces
//! (or all those of the user, also future ones), limited to some collections, with an expiry date.
//! The effective permissions are never more than the user's (see `auth::key_role`).

use axum::{extract::State, Json};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Access, Collection},
    perm::collection_access,
    util::{non_empty, random_string, sha256_hex},
    AppState,
};

#[derive(FromRow)]
struct ApiKeyRow {
    id: Uuid,
    name: String,
    last4: String,
    access: String,
    team_id: Uuid,
    all_workspaces: bool,
    collections_restricted: bool,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

/// Every key of the user, in any workspace, with what it allows.
pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<ApiKeyRow> = sqlx::query_as(
        "SELECT id, name, last4, access, team_id, all_workspaces, collections_restricted, expires_at, created_at, last_used_at \
         FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let workspaces: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT k.api_key_id, t.id, t.name FROM api_key_workspaces k JOIN teams t ON t.id = k.team_id WHERE k.api_key_id = ANY($1) ORDER BY t.name",
    )
    .bind(&ids)
    .fetch_all(&st.db)
    .await?;
    let collections: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT k.api_key_id, c.id, c.name, t.name FROM api_key_collections k JOIN collections c ON c.id = k.collection_id \
         JOIN teams t ON t.id = c.team_id WHERE k.api_key_id = ANY($1) AND c.deleted_at IS NULL ORDER BY t.name, c.name",
    )
    .bind(&ids)
    .fetch_all(&st.db)
    .await?;
    let teams: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM teams WHERE id = ANY($1)")
        .bind(rows.iter().map(|r| r.team_id).collect::<Vec<_>>())
        .fetch_all(&st.db)
        .await?;
    let now = Utc::now();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "last4": r.last4,
                "access": r.access,
                "allWorkspaces": r.all_workspaces,
                "defaultWorkspace": teams.iter().find(|t| t.0 == r.team_id).map(|t| json!({ "id": t.0, "name": t.1 })),
                "workspaces": workspaces.iter().filter(|w| w.0 == r.id).map(|w| json!({ "id": w.1, "name": w.2 })).collect::<Vec<_>>(),
                "collections": if r.collections_restricted {
                    json!(collections.iter().filter(|c| c.0 == r.id).map(|c| json!({ "id": c.1, "name": c.2, "workspace": c.3 })).collect::<Vec<_>>())
                } else {
                    Value::Null
                },
                "expiresAt": r.expires_at,
                "expired": r.expires_at.is_some_and(|e| e <= now),
                "createdAt": r.created_at,
                "lastUsedAt": r.last_used_at,
            })
        })
        .collect();
    ok(out)
}

/// The user's workspaces (or the chosen ones) with the collections they can read, for the key form.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollectionsReq {
    workspace_ids: Option<Vec<Uuid>>,
}

async fn my_workspaces(st: &AppState, auth: &Auth) -> ApiResult<Vec<(Uuid, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT t.id, t.name, m.role FROM team_memberships m JOIN teams t ON t.id = m.team_id WHERE m.user_id = $1 AND m.suspended_at IS NULL ORDER BY t.name",
    )
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await?)
}

/// Collections of `team` the user can read (as a browser session would).
async fn readable_collections(st: &AppState, auth: &Auth, team: Uuid) -> ApiResult<Vec<Collection>> {
    let a = auth.for_team(&st.db, team).await?;
    let all: Vec<Collection> = sqlx::query_as("SELECT * FROM collections WHERE team_id = $1 AND deleted_at IS NULL ORDER BY sort_index")
        .bind(team)
        .fetch_all(&st.db)
        .await?;
    let mut out = vec![];
    for c in all {
        if collection_access(&st.db, &a, &c).await? != Access::None {
            out.push(c);
        }
    }
    Ok(out)
}

pub async fn collections(State(st): State<AppState>, auth: Auth, Json(b): Json<CollectionsReq>) -> ApiResult<Json<Value>> {
    if auth.key.is_some() {
        return Err(ApiError::forbidden("API keys can't manage API keys"));
    }
    let mut out = vec![];
    for (id, name, role) in my_workspaces(&st, &auth).await? {
        if b.workspace_ids.as_ref().is_some_and(|w| !w.contains(&id)) {
            continue;
        }
        let cols = readable_collections(&st, &auth, id).await?;
        out.push(json!({
            "id": id, "name": name, "role": role,
            "collections": cols.iter().map(|c| json!({ "id": c.id, "name": c.name, "icon": c.icon, "color": c.color })).collect::<Vec<_>>(),
        }));
    }
    ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReq {
    name: String,
    /// "read" or "write" (default)
    access: Option<String>,
    /// valid in every workspace of the user, also future ones
    #[serde(default)]
    all_workspaces: bool,
    /// when not all: these workspaces (default: the current one)
    workspace_ids: Option<Vec<Uuid>>,
    /// None: every collection; Some: only these
    collection_ids: Option<Vec<Uuid>>,
    /// None: no expiry
    expires_in_days: Option<i64>,
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<CreateReq>) -> ApiResult<Json<Value>> {
    if auth.key.is_some() {
        return Err(ApiError::forbidden("API keys can't create API keys"));
    }
    let name = non_empty(&b.name, "Name")?;
    let access = b.access.unwrap_or_else(|| "write".into());
    if access != "read" && access != "write" {
        return Err(ApiError::bad_request("access must be read or write"));
    }
    let mine: Vec<Uuid> = my_workspaces(&st, &auth).await?.into_iter().map(|w| w.0).collect();
    let workspaces: Vec<Uuid> = if b.all_workspaces {
        mine.clone()
    } else {
        let w = b.workspace_ids.clone().unwrap_or_else(|| auth.team_id.into_iter().collect());
        if w.is_empty() {
            return Err(ApiError::bad_request("Choose at least one workspace"));
        }
        if let Some(x) = w.iter().find(|t| !mine.contains(t)) {
            return Err(ApiError::forbidden(format!("You are not a member of workspace {x}")));
        }
        w
    };
    // default workspace: the current one when the key is valid there
    let default_team = match auth.team_id {
        Some(t) if workspaces.contains(&t) => t,
        _ => *workspaces.first().ok_or_else(|| ApiError::bad_request("You are not a member of any workspace"))?,
    };
    if let Some(cols) = &b.collection_ids {
        if cols.is_empty() {
            return Err(ApiError::bad_request("Choose at least one collection, or all collections"));
        }
        for cid in cols {
            let c: Option<Collection> = sqlx::query_as("SELECT * FROM collections WHERE id = $1 AND deleted_at IS NULL")
                .bind(cid)
                .fetch_optional(&st.db)
                .await?;
            let ok_col = match &c {
                Some(c) if workspaces.contains(&c.team_id) => {
                    collection_access(&st.db, &auth.for_team(&st.db, c.team_id).await?, c).await? != Access::None
                }
                _ => false,
            };
            if !ok_col {
                return Err(ApiError::bad_request("A chosen collection is not readable in the key's workspaces"));
            }
        }
    }
    let expires_at = match b.expires_in_days {
        Some(d) if (1..=3650).contains(&d) => Some(Utc::now() + Duration::days(d)),
        Some(_) => return Err(ApiError::bad_request("The expiry must be between 1 and 3650 days")),
        None => None,
    };

    let secret = format!("ol_api_{}", random_string(38));
    let id = Uuid::new_v4();
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, team_id, name, secret_hash, last4, access, all_workspaces, collections_restricted, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(auth.user.id)
    .bind(default_team)
    .bind(&name)
    .bind(sha256_hex(&secret))
    .bind(&secret[secret.len() - 4..])
    .bind(&access)
    .bind(b.all_workspaces)
    .bind(b.collection_ids.is_some())
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    if !b.all_workspaces {
        sqlx::query("INSERT INTO api_key_workspaces (api_key_id, team_id) SELECT $1, unnest($2::uuid[])")
            .bind(id)
            .bind(&workspaces)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(cols) = &b.collection_ids {
        sqlx::query("INSERT INTO api_key_collections (api_key_id, collection_id) SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(cols)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    // personal keys: recorded in every workspace where the key works
    let data = json!({ "name": name, "access": access, "allWorkspaces": b.all_workspaces, "limitedToCollections": b.collection_ids.is_some(), "expiresAt": expires_at });
    for team in &workspaces {
        audit::record_in(&st, &auth, Some(*team), "api_keys.create", Target::user(&auth.user), data.clone()).await;
    }
    ok(json!({ "id": id, "name": name, "secret": secret }))
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

pub async fn delete(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let teams: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT team_id FROM api_keys WHERE id = $1 AND user_id = $2 UNION SELECT team_id FROM api_key_workspaces WHERE api_key_id = $1",
    )
    .bind(b.id)
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await?;
    let deleted: Option<(String,)> = sqlx::query_as("DELETE FROM api_keys WHERE id = $1 AND user_id = $2 RETURNING name")
        .bind(b.id)
        .bind(auth.user.id)
        .fetch_optional(&st.db)
        .await?;
    if let Some((name,)) = deleted {
        for (team,) in teams {
            audit::record_in(&st, &auth, Some(team), "api_keys.delete", Target::user(&auth.user), json!({ "name": name })).await;
        }
    }
    ok(json!({ "success": true }))
}
