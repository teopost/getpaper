use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ApiError, ApiResult},
    models::{doc_select, Access, Collection, DocRow},
    util::{parse_id, IdRef},
};

/// Role of the user in the given team (uses the cached role for the active team).
pub async fn team_role(db: &PgPool, auth: &Auth, team_id: Uuid) -> ApiResult<Option<String>> {
    if auth.team_id == Some(team_id) {
        return Ok(auth.role.clone());
    }
    if auth.session_hash.is_none() {
        // API keys are scoped to a single workspace
        return Ok(None);
    }
    let r: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2")
        .bind(auth.user.id)
        .bind(team_id)
        .fetch_optional(db)
        .await?;
    Ok(r.map(|r| r.0))
}

pub fn compute_collection_access(role: &str, c: &Collection, membership: Option<&str>) -> Access {
    if c.deleted_at.is_some() {
        return Access::None;
    }
    let base = if role == "admin" {
        Access::Write
    } else {
        match (c.permission.as_deref(), membership) {
            (_, Some("read_write")) | (Some("read_write"), _) => Access::Write,
            (_, Some(_)) | (Some(_), _) => Access::Read,
            (None, None) => Access::None,
        }
    };
    if role == "viewer" && base == Access::Write {
        Access::Read
    } else if role.is_empty() {
        Access::None
    } else {
        base
    }
}

pub async fn collection_access(db: &PgPool, auth: &Auth, c: &Collection) -> ApiResult<Access> {
    let role = match team_role(db, auth, c.team_id).await? {
        Some(r) => r,
        None => return Ok(Access::None),
    };
    let m: Option<(String,)> =
        sqlx::query_as("SELECT permission FROM collection_memberships WHERE collection_id = $1 AND user_id = $2")
            .bind(c.id)
            .bind(auth.user.id)
            .fetch_optional(db)
            .await?;
    Ok(compute_collection_access(&role, c, m.as_ref().map(|m| m.0.as_str())))
}

pub async fn find_collection(db: &PgPool, id: &str) -> ApiResult<Collection> {
    let c: Option<Collection> = match parse_id(id) {
        IdRef::Uuid(u) => sqlx::query_as("SELECT * FROM collections WHERE id = $1").bind(u).fetch_optional(db).await?,
        IdRef::UrlId(u) => sqlx::query_as("SELECT * FROM collections WHERE url_id = $1").bind(u).fetch_optional(db).await?,
    };
    c.ok_or_else(|| ApiError::not_found("Collection not found"))
}

pub async fn load_collection(db: &PgPool, auth: &Auth, id: &str, need: Access) -> ApiResult<(Collection, Access)> {
    let c = find_collection(db, id).await?;
    let a = collection_access(db, auth, &c).await?;
    if a == Access::None {
        return Err(ApiError::not_found("Collection not found"));
    }
    if a < need {
        return Err(ApiError::forbidden("You do not have permission to edit this collection"));
    }
    Ok((c, a))
}

/// Workspaces the user belongs to and the collections they can read in each of them
/// (same rules as `readable_collection_ids`, applied with the user's role in every workspace).
pub async fn readable_collection_ids_all_teams(db: &PgPool, user_id: Uuid) -> ApiResult<(Vec<Uuid>, Vec<Uuid>)> {
    let teams: Vec<(Uuid,)> = sqlx::query_as("SELECT team_id FROM team_memberships WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    let cols: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT c.id FROM collections c JOIN team_memberships m ON m.team_id = c.team_id AND m.user_id = $1 \
         WHERE c.deleted_at IS NULL AND (m.role = 'admin' OR c.permission IS NOT NULL OR EXISTS \
         (SELECT 1 FROM collection_memberships cm WHERE cm.collection_id = c.id AND cm.user_id = $1))",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok((teams.into_iter().map(|r| r.0).collect(), cols.into_iter().map(|r| r.0).collect()))
}

pub async fn readable_collection_ids(db: &PgPool, auth: &Auth) -> ApiResult<Vec<Uuid>> {
    let team = auth.team()?;
    let rows: Vec<(Uuid,)> = if auth.is_admin() {
        sqlx::query_as("SELECT id FROM collections WHERE team_id = $1 AND deleted_at IS NULL")
            .bind(team)
            .fetch_all(db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT c.id FROM collections c WHERE c.team_id = $1 AND c.deleted_at IS NULL AND \
             (c.permission IS NOT NULL OR EXISTS (SELECT 1 FROM collection_memberships cm \
              WHERE cm.collection_id = c.id AND cm.user_id = $2))",
        )
        .bind(team)
        .bind(auth.user.id)
        .fetch_all(db)
        .await?
    };
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn find_doc(db: &PgPool, user_id: Uuid, id: &str) -> ApiResult<DocRow> {
    let d: Option<DocRow> = match parse_id(id) {
        IdRef::Uuid(u) => {
            sqlx::query_as(&format!("{} WHERE d.id = $2", doc_select()))
                .bind(user_id)
                .bind(u)
                .fetch_optional(db)
                .await?
        }
        IdRef::UrlId(u) => {
            sqlx::query_as(&format!("{} WHERE d.url_id = $2", doc_select()))
                .bind(user_id)
                .bind(u)
                .fetch_optional(db)
                .await?
        }
    };
    d.ok_or_else(|| ApiError::not_found("Document not found"))
}

pub async fn doc_access(db: &PgPool, auth: &Auth, d: &DocRow) -> ApiResult<Access> {
    let role = match team_role(db, auth, d.team_id).await? {
        Some(r) => r,
        None => return Ok(Access::None),
    };
    let is_owner = d.created_by == auth.user.id;
    if d.published_at.is_none() && !is_owner {
        return Ok(Access::None);
    }
    if d.deleted_at.is_some() {
        // trashed documents are only visible to their author and admins
        return Ok(if !(is_owner || role == "admin") {
            Access::None
        } else if role == "viewer" {
            Access::Read
        } else {
            Access::Write
        });
    }
    let a = match d.collection_id {
        None => {
            if is_owner || (d.template && role == "admin") {
                Access::Write
            } else if d.template && d.published_at.is_some() {
                // workspace-wide templates
                Access::Read
            } else {
                Access::None
            }
        }
        Some(cid) => {
            let c: Collection = sqlx::query_as("SELECT * FROM collections WHERE id = $1")
                .bind(cid)
                .fetch_one(db)
                .await?;
            let m: Option<(String,)> =
                sqlx::query_as("SELECT permission FROM collection_memberships WHERE collection_id = $1 AND user_id = $2")
                    .bind(cid)
                    .bind(auth.user.id)
                    .fetch_optional(db)
                    .await?;
            let a = compute_collection_access(&role, &c, m.as_ref().map(|m| m.0.as_str()));
            // drafts owned by the user remain editable by them
            if a == Access::None && is_owner && d.published_at.is_none() {
                Access::Write
            } else {
                a
            }
        }
    };
    if role == "viewer" && a == Access::Write {
        return Ok(Access::Read);
    }
    Ok(a)
}

pub async fn load_doc(db: &PgPool, auth: &Auth, id: &str, need: Access) -> ApiResult<(DocRow, Access)> {
    let d = find_doc(db, auth.user.id, id).await?;
    let a = doc_access(db, auth, &d).await?;
    if a == Access::None {
        return Err(ApiError::not_found("Document not found"));
    }
    if a < need {
        return Err(ApiError::forbidden("You do not have permission to edit this document"));
    }
    Ok((d, a))
}

/// ids of the document and all of its descendants
pub async fn descendant_ids(db: &PgPool, id: Uuid) -> ApiResult<Vec<Uuid>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "WITH RECURSIVE tree AS (SELECT id, 0 AS depth FROM documents WHERE id = $1 \
         UNION ALL SELECT d.id, tree.depth + 1 FROM documents d JOIN tree ON d.parent_document_id = tree.id WHERE tree.depth < 100) \
         SELECT id FROM tree",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
