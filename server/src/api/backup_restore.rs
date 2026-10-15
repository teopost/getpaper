//! Restores a backup made by `export` (`getpaper-backup.json` + `attachments/<id>`) into the
//! current workspace.
//!
//! - Users are matched by email with the members of this workspace; content of anyone else is
//!   attributed to the user who restores.
//! - Ids and URL ids are kept when free, so links keep working; otherwise new ones are generated and
//!   the internal links (`/doc/…`, `/collection/…`, `/files/…`) in documents, revisions, comments
//!   and collection descriptions are rewritten.
//! - Collections are added after the existing ones; nothing already in the workspace is changed.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{
    export::{BACKUP_FILE, BACKUP_FORMAT, BACKUP_VERSION, LEGACY_BACKUP_FILE, LEGACY_BACKUP_FORMAT},
    import::ImportResult,
};
use crate::{
    auth::Auth,
    error::{ApiError, ApiResult},
    util::new_url_id,
    AppState,
};

/// True when the archive is a GetPaper backup.
pub fn detect(entries: &[(String, Vec<u8>)]) -> bool {
    entries.iter().any(|(p, _)| is_backup_file(p))
}

pub fn is_backup_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == BACKUP_FILE || name == LEGACY_BACKUP_FILE
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Backup {
    format: String,
    version: u32,
    #[serde(default)]
    users: Vec<BUser>,
    #[serde(default)]
    collections: Vec<BCollection>,
    #[serde(default)]
    collection_memberships: Vec<BMembership>,
    #[serde(default)]
    documents: Vec<BDocument>,
    #[serde(default)]
    revisions: Vec<BRevision>,
    #[serde(default)]
    comments: Vec<BComment>,
    #[serde(default)]
    attachments: Vec<BAttachment>,
}

#[derive(Deserialize)]
struct BUser {
    id: Uuid,
    email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BCollection {
    id: Uuid,
    url_id: String,
    name: String,
    #[serde(default)]
    description: String,
    color: String,
    icon: Option<String>,
    permission: Option<String>,
    sharing: bool,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BMembership {
    collection_id: Uuid,
    user_id: Uuid,
    permission: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BDocument {
    id: Uuid,
    url_id: String,
    collection_id: Option<Uuid>,
    parent_document_id: Option<Uuid>,
    title: String,
    emoji: Option<String>,
    text: String,
    template: bool,
    full_width: bool,
    position: f64,
    created_by: Uuid,
    updated_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BRevision {
    id: Uuid,
    document_id: Uuid,
    user_id: Uuid,
    title: String,
    emoji: Option<String>,
    text: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BComment {
    id: Uuid,
    document_id: Uuid,
    parent_comment_id: Option<Uuid>,
    created_by: Uuid,
    text: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    resolved_by: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BAttachment {
    id: Uuid,
    document_id: Option<Uuid>,
    user_id: Uuid,
    name: String,
    content_type: String,
    size: i64,
    created_at: DateTime<Utc>,
}

/// Old → new id for every id of `ids` that already exists in `table`.
async fn taken_ids(st: &AppState, table: &str, ids: &[Uuid]) -> ApiResult<HashMap<Uuid, Uuid>> {
    let taken: Vec<(Uuid,)> = sqlx::query_as(&format!("SELECT id FROM {table} WHERE id = ANY($1)")).bind(ids).fetch_all(&st.db).await?;
    Ok(taken.into_iter().map(|(id,)| (id, Uuid::new_v4())).collect())
}

async fn taken_url_ids(st: &AppState, table: &str, ids: &[String]) -> ApiResult<HashSet<String>> {
    let taken: Vec<(String,)> =
        sqlx::query_as(&format!("SELECT url_id FROM {table} WHERE url_id = ANY($1)")).bind(ids).fetch_all(&st.db).await?;
    Ok(taken.into_iter().map(|(u,)| u).collect())
}

struct Remap {
    ids: HashMap<Uuid, Uuid>,
    doc_urls: HashMap<String, String>,
    collection_urls: HashMap<String, String>,
}

impl Remap {
    fn id(&self, id: Uuid) -> Uuid {
        self.ids.get(&id).copied().unwrap_or(id)
    }
    /// Rewrites internal links whose target got a new id.
    fn text(&self, s: &str, re: &Regex) -> String {
        if self.ids.is_empty() && self.doc_urls.is_empty() && self.collection_urls.is_empty() {
            return s.to_string();
        }
        re.replace_all(s, |c: &Captures| {
            if let Some(m) = c.name("doc") {
                if let Some(n) = self.doc_urls.get(m.as_str()) {
                    return format!("{}{n}", &c["docprefix"]);
                }
            } else if let Some(m) = c.name("col") {
                if let Some(n) = self.collection_urls.get(m.as_str()) {
                    return format!("/collection/{n}");
                }
            } else if let Some(m) = c.name("file") {
                if let Some(n) = Uuid::parse_str(m.as_str()).ok().and_then(|id| self.ids.get(&id)) {
                    return format!("/files/{n}/");
                }
            }
            c[0].to_string()
        })
        .to_string()
    }
}

pub async fn restore(st: &AppState, auth: &Auth, entries: Vec<(String, Vec<u8>)>) -> ApiResult<ImportResult> {
    auth.require_admin()?;
    let team = auth.team()?;
    let manifest = entries.iter().map(|(p, _)| p.clone()).find(|p| is_backup_file(p)).unwrap_or_default();
    let (root, name) = match manifest.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), manifest.clone()),
    };
    let mut files: HashMap<String, Vec<u8>> =
        entries.into_iter().filter_map(|(p, d)| p.strip_prefix(&root).map(|r| (r.to_string(), d))).collect();
    let json = files.remove(&name).unwrap_or_default();
    let b: Backup = serde_json::from_slice(&json).map_err(|e| ApiError::bad_request(format!("Invalid backup file: {e}")))?;
    drop(json);
    if (b.format != BACKUP_FORMAT && b.format != LEGACY_BACKUP_FORMAT) || b.version > BACKUP_VERSION {
        return Err(ApiError::bad_request(format!(
            "Unsupported backup (format {} version {}); this version reads {BACKUP_FORMAT} up to version {BACKUP_VERSION}",
            b.format, b.version
        )));
    }

    // ---- users: by email among the members of this workspace
    let members: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, lower(u.email) FROM team_memberships m JOIN users u ON u.id = m.user_id WHERE m.team_id = $1",
    )
    .bind(team)
    .fetch_all(&st.db)
    .await?;
    let by_email: HashMap<String, Uuid> = members.into_iter().map(|(id, e)| (e, id)).collect();
    let users: HashMap<Uuid, Uuid> =
        b.users.iter().filter_map(|u| by_email.get(&u.email.to_lowercase()).map(|&id| (u.id, id))).collect();
    let user = |id: Uuid| users.get(&id).copied().unwrap_or(auth.user.id);

    // ---- ids: keep them when free
    let mut ids: HashMap<Uuid, Uuid> = HashMap::new();
    ids.extend(taken_ids(st, "collections", &b.collections.iter().map(|c| c.id).collect::<Vec<_>>()).await?);
    ids.extend(taken_ids(st, "documents", &b.documents.iter().map(|d| d.id).collect::<Vec<_>>()).await?);
    ids.extend(taken_ids(st, "revisions", &b.revisions.iter().map(|r| r.id).collect::<Vec<_>>()).await?);
    ids.extend(taken_ids(st, "comments", &b.comments.iter().map(|c| c.id).collect::<Vec<_>>()).await?);
    ids.extend(taken_ids(st, "attachments", &b.attachments.iter().map(|a| a.id).collect::<Vec<_>>()).await?);
    let taken_docs = taken_url_ids(st, "documents", &b.documents.iter().map(|d| d.url_id.clone()).collect::<Vec<_>>()).await?;
    let taken_cols = taken_url_ids(st, "collections", &b.collections.iter().map(|c| c.url_id.clone()).collect::<Vec<_>>()).await?;
    let remap = Remap {
        ids,
        doc_urls: b.documents.iter().filter(|d| taken_docs.contains(&d.url_id)).map(|d| (d.url_id.clone(), new_url_id())).collect(),
        collection_urls: b
            .collections
            .iter()
            .filter(|c| taken_cols.contains(&c.url_id))
            .map(|c| (c.url_id.clone(), new_url_id()))
            .collect(),
    };
    let doc_url = |u: &str| remap.doc_urls.get(u).cloned().unwrap_or_else(|| u.to_string());
    let link_re = Regex::new(
        r"(?P<docprefix>/doc/(?:[A-Za-z0-9-]*-)?)(?P<doc>[A-Za-z0-9]{10})\b|/collection/(?P<col>[A-Za-z0-9]{10})\b|/files/(?P<file>[0-9a-fA-F-]{36})/",
    )
    .unwrap();

    let mut tx = st.db.begin().await?;

    // ---- collections, after the existing ones
    let (max,): (Option<f64>,) = sqlx::query_as("SELECT max(sort_index) FROM collections WHERE team_id = $1")
        .bind(team)
        .fetch_one(&mut *tx)
        .await?;
    let mut created_collections = vec![];
    for (i, c) in b.collections.iter().enumerate() {
        let id = remap.id(c.id);
        let url_id = remap.collection_urls.get(&c.url_id).cloned().unwrap_or_else(|| c.url_id.clone());
        sqlx::query(
            "INSERT INTO collections (id, url_id, team_id, name, description, color, icon, permission, sharing, sort_index, \
             created_by, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(&url_id)
        .bind(team)
        .bind(&c.name)
        .bind(remap.text(&c.description, &link_re))
        .bind(&c.color)
        .bind(&c.icon)
        .bind(&c.permission)
        .bind(c.sharing)
        .bind(max.unwrap_or(0.0) + 1.0 + i as f64)
        .bind(c.created_by.map(user))
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&mut *tx)
        .await?;
        created_collections.push(json!({ "id": id, "urlId": url_id, "name": c.name, "created": true }));
    }
    let known_collections: HashSet<Uuid> = b.collections.iter().map(|c| c.id).collect();
    for m in &b.collection_memberships {
        if let (true, Some(&u)) = (known_collections.contains(&m.collection_id), users.get(&m.user_id)) {
            sqlx::query(
                "INSERT INTO collection_memberships (collection_id, user_id, permission) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(remap.id(m.collection_id))
            .bind(u)
            .bind(&m.permission)
            .execute(&mut *tx)
            .await?;
        }
    }

    // ---- documents, parents first
    let doc_ids: HashSet<Uuid> = b.documents.iter().map(|d| d.id).collect();
    let depth_of = {
        let parent: HashMap<Uuid, Option<Uuid>> = b.documents.iter().map(|d| (d.id, d.parent_document_id)).collect();
        move |mut id: Uuid| {
            let mut depth = 0;
            while let Some(Some(p)) = parent.get(&id) {
                depth += 1;
                id = *p;
                if depth > 1000 {
                    break;
                }
            }
            depth
        }
    };
    let mut order: Vec<&BDocument> = b.documents.iter().collect();
    order.sort_by_key(|d| depth_of(d.id));
    for d in &order {
        let collection = d.collection_id.filter(|c| known_collections.contains(c)).map(|c| remap.id(c));
        let parent = d.parent_document_id.filter(|p| doc_ids.contains(p)).map(|p| remap.id(p));
        sqlx::query(
            "INSERT INTO documents (id, url_id, team_id, collection_id, parent_document_id, title, emoji, text, template, \
             full_width, position, created_by, updated_by, created_at, updated_at, published_at, archived_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .bind(remap.id(d.id))
        .bind(doc_url(&d.url_id))
        .bind(team)
        .bind(collection)
        .bind(parent)
        .bind(&d.title)
        .bind(&d.emoji)
        .bind(remap.text(&d.text, &link_re))
        .bind(d.template)
        .bind(d.full_width)
        .bind(d.position)
        .bind(user(d.created_by))
        .bind(user(d.updated_by))
        .bind(d.created_at)
        .bind(d.updated_at)
        .bind(d.published_at)
        .bind(d.archived_at)
        .execute(&mut *tx)
        .await?;
    }
    for r in b.revisions.iter().filter(|r| doc_ids.contains(&r.document_id)) {
        sqlx::query(
            "INSERT INTO revisions (id, document_id, user_id, title, emoji, text, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(remap.id(r.id))
        .bind(remap.id(r.document_id))
        .bind(user(r.user_id))
        .bind(&r.title)
        .bind(&r.emoji)
        .bind(remap.text(&r.text, &link_re))
        .bind(r.created_at)
        .execute(&mut *tx)
        .await?;
    }
    // replies after the comments they answer
    let comment_ids: HashSet<Uuid> = b.comments.iter().map(|c| c.id).collect();
    let mut comments: Vec<&BComment> = b.comments.iter().filter(|c| doc_ids.contains(&c.document_id)).collect();
    comments.sort_by_key(|c| c.parent_comment_id.is_some());
    for c in comments {
        sqlx::query(
            "INSERT INTO comments (id, document_id, parent_comment_id, created_by, text, created_at, updated_at, resolved_at, resolved_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(remap.id(c.id))
        .bind(remap.id(c.document_id))
        .bind(c.parent_comment_id.filter(|p| comment_ids.contains(p)).map(|p| remap.id(p)))
        .bind(user(c.created_by))
        .bind(remap.text(&c.text, &link_re))
        .bind(c.created_at)
        .bind(c.updated_at)
        .bind(c.resolved_at)
        .bind(c.resolved_by.map(user))
        .execute(&mut *tx)
        .await?;
    }

    // ---- attachments
    let mut skipped = vec![];
    let mut to_write = vec![];
    for a in &b.attachments {
        let Some(data) = files.remove(&format!("attachments/{}", a.id)) else {
            skipped.push(format!("{} (file missing from the backup)", a.name));
            continue;
        };
        let id = remap.id(a.id);
        to_write.push((st.cfg.upload_dir.join(id.to_string()), data));
        sqlx::query(
            "INSERT INTO attachments (id, team_id, user_id, document_id, name, content_type, size, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(team)
        .bind(user(a.user_id))
        .bind(a.document_id.filter(|d| doc_ids.contains(d)).map(|d| remap.id(d)))
        .bind(&a.name)
        .bind(&a.content_type)
        .bind(a.size)
        .bind(a.created_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    // files after the commit, so a failed restore leaves nothing behind
    for (path, data) in to_write {
        if let Err(e) = tokio::fs::write(&path, &data).await {
            tracing::error!(path = %path.display(), "cannot write restored attachment: {e}");
        }
    }
    tracing::info!(
        collections = b.collections.len(),
        documents = b.documents.len(),
        revisions = b.revisions.len(),
        comments = b.comments.len(),
        renamed_ids = remap.ids.len(),
        "backup restored"
    );
    Ok(ImportResult {
        collections: created_collections,
        documents: b.documents.len(),
        attachments: b.attachments.len() - skipped.len(),
        skipped,
        format: "getpaper-backup",
    })
}
