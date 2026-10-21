//! Workspace export.
//!
//! - `backup`: the whole workspace for another GetPaper instance (admins only): a ZIP with
//!   `getpaper-backup.json` (collections, documents with drafts / templates / archived ones,
//!   revisions, comments, private collection members, the users they refer to) and the attachment
//!   files as `attachments/<id>`. Restored by `backup_restore` through the importer.
//! - `obsidian`: a Markdown vault of the published documents the user can read: a folder per
//!   collection, nested documents as `Page.md` + `Page/`, `[[wiki links]]`, attachments in
//!   `_attachments/`, notices as callouts and dates in the front matter.

use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Write},
    sync::OnceLock,
};

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use super::import_confluence::{safe_name, tree_paths};
use crate::{
    audit,
    auth::Auth,
    error::{ApiError, ApiResult},
    perm::readable_collection_ids,
    util::slugify,
    AppState,
};

pub const BACKUP_FILE: &str = "getpaper-backup.json";
pub const BACKUP_FORMAT: &str = "getpaper-backup";
/// backups made before the product was renamed (Horsa Codex) are still restored
pub const LEGACY_BACKUP_FILE: &str = "horsa-codex-backup.json";
pub const LEGACY_BACKUP_FORMAT: &str = "horsa-codex-backup";
pub const BACKUP_VERSION: u32 = 1;

#[derive(Deserialize)]
pub struct ExportReq {
    format: String,
}

pub async fn export(State(st): State<AppState>, auth: Auth, Json(b): Json<ExportReq>) -> ApiResult<Response> {
    let team = auth.team()?;
    let (team_name,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(team).fetch_one(&st.db).await?;
    let settings = st.settings();
    let disabled = || ApiError::new(StatusCode::FORBIDDEN, "export_disabled", "This export has been disabled by the administrator");
    let (files, suffix) = match b.format.as_str() {
        "backup" => {
            auth.require_admin()?;
            if !settings.allow_backup_export {
                return Err(disabled());
            }
            (backup(&st, team, &team_name).await?, "backup")
        }
        "obsidian" => {
            if !settings.allow_markdown_export {
                return Err(disabled());
            }
            (obsidian(&st, &auth).await?, "obsidian")
        }
        _ => return Err(ApiError::bad_request("format must be backup or obsidian")),
    };
    let zip = tokio::task::spawn_blocking(move || write_zip(files))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    let filename = format!("{}-{}-{suffix}.zip", slugify(&team_name), Utc::now().format("%Y-%m-%d"));
    tracing::info!(format = %b.format, bytes = zip.len(), "workspace exported");
    audit::record(&st, &auth, "workspace.export", None, serde_json::json!({ "format": b.format, "bytes": zip.len() })).await;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
        ],
        zip,
    )
        .into_response())
}

fn write_zip(files: Vec<(String, Vec<u8>)>) -> ApiResult<Vec<u8>> {
    let err = |e: zip::result::ZipError| ApiError::internal(format!("Cannot create the archive: {e}"));
    let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        // images and office files are already compressed
        let method = match name.rsplit('.').next().map(|e| e.to_lowercase()).as_deref() {
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "zip" | "docx" | "xlsx" | "pptx" | "pdf" | "mp4") => CompressionMethod::Stored,
            _ => CompressionMethod::Deflated,
        };
        let opts = SimpleFileOptions::default().compression_method(method).large_file(data.len() > u32::MAX as usize / 2);
        zw.start_file(name, opts).map_err(err)?;
        zw.write_all(&data).map_err(|e| ApiError::internal(e.to_string()))?;
    }
    Ok(zw.finish().map_err(err)?.into_inner())
}

async fn json_rows(st: &AppState, sql: &str, team: Uuid) -> ApiResult<Value> {
    let (v,): (Value,) = sqlx::query_as(&format!("SELECT coalesce(json_agg(t), '[]'::json) FROM ({sql}) t"))
        .bind(team)
        .fetch_one(&st.db)
        .await?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// backup
// ---------------------------------------------------------------------------

async fn backup(st: &AppState, team: Uuid, team_name: &str) -> ApiResult<Vec<(String, Vec<u8>)>> {
    const COLLECTIONS: &str = "SELECT id, url_id AS \"urlId\", name, description, color, icon, permission, sharing, \
        sort_index AS \"sortIndex\", created_by AS \"createdBy\", created_at AS \"createdAt\", updated_at AS \"updatedAt\" \
        FROM collections WHERE team_id = $1 AND deleted_at IS NULL ORDER BY sort_index";
    // documents in the trash, or in a deleted collection, are left out
    const DOCS: &str = "SELECT d.id FROM documents d LEFT JOIN collections c ON c.id = d.collection_id \
        WHERE d.team_id = $1 AND d.deleted_at IS NULL AND (d.collection_id IS NULL OR c.deleted_at IS NULL)";
    let collections = json_rows(st, COLLECTIONS, team).await?;
    let documents = json_rows(
        st,
        &format!(
            "SELECT id, url_id AS \"urlId\", collection_id AS \"collectionId\", parent_document_id AS \"parentDocumentId\", \
             title, emoji, text, template, full_width AS \"fullWidth\", position, created_by AS \"createdBy\", \
             updated_by AS \"updatedBy\", created_at AS \"createdAt\", updated_at AS \"updatedAt\", \
             published_at AS \"publishedAt\", archived_at AS \"archivedAt\" \
             FROM documents WHERE id IN ({DOCS}) ORDER BY created_at"
        ),
        team,
    )
    .await?;
    let revisions = json_rows(
        st,
        &format!(
            "SELECT id, document_id AS \"documentId\", user_id AS \"userId\", title, emoji, text, created_at AS \"createdAt\" \
             FROM revisions WHERE document_id IN ({DOCS}) ORDER BY created_at"
        ),
        team,
    )
    .await?;
    let comments = json_rows(
        st,
        &format!(
            "SELECT id, document_id AS \"documentId\", parent_comment_id AS \"parentCommentId\", created_by AS \"createdBy\", \
             text, created_at AS \"createdAt\", updated_at AS \"updatedAt\", resolved_at AS \"resolvedAt\", \
             resolved_by AS \"resolvedBy\" FROM comments WHERE document_id IN ({DOCS}) ORDER BY created_at"
        ),
        team,
    )
    .await?;
    let memberships = json_rows(
        st,
        "SELECT m.collection_id AS \"collectionId\", m.user_id AS \"userId\", m.permission FROM collection_memberships m \
         JOIN collections c ON c.id = m.collection_id WHERE c.team_id = $1 AND c.deleted_at IS NULL",
        team,
    )
    .await?;
    let attachments = json_rows(
        st,
        "SELECT id, document_id AS \"documentId\", user_id AS \"userId\", name, content_type AS \"contentType\", size, \
         created_at AS \"createdAt\" FROM attachments WHERE team_id = $1 ORDER BY created_at",
        team,
    )
    .await?;
    // users referred to by anything above: matched by email when restoring
    let users = json_rows(
        st,
        "SELECT id, name, email FROM users WHERE id IN ( \
           SELECT created_by FROM collections WHERE team_id = $1 \
           UNION SELECT created_by FROM documents WHERE team_id = $1 UNION SELECT updated_by FROM documents WHERE team_id = $1 \
           UNION SELECT r.user_id FROM revisions r JOIN documents d ON d.id = r.document_id WHERE d.team_id = $1 \
           UNION SELECT c.created_by FROM comments c JOIN documents d ON d.id = c.document_id WHERE d.team_id = $1 \
           UNION SELECT c.resolved_by FROM comments c JOIN documents d ON d.id = c.document_id WHERE d.team_id = $1 \
           UNION SELECT user_id FROM attachments WHERE team_id = $1 \
           UNION SELECT m.user_id FROM collection_memberships m JOIN collections c ON c.id = m.collection_id WHERE c.team_id = $1)",
        team,
    )
    .await?;

    let mut files = vec![];
    let mut missing = 0;
    for a in attachments.as_array().into_iter().flatten() {
        let Some(id) = a.get("id").and_then(|v| v.as_str()) else { continue };
        match tokio::fs::read(st.cfg.upload_dir.join(id)).await {
            Ok(data) => files.push((format!("attachments/{id}"), data)),
            Err(_) => missing += 1,
        }
    }
    if missing > 0 {
        tracing::warn!(missing, "attachment files not found while exporting");
    }
    let count = |v: &Value| v.as_array().map(|a| a.len()).unwrap_or(0);
    let manifest = json!({
        "format": BACKUP_FORMAT,
        "version": BACKUP_VERSION,
        "app": format!("getpaper {}", env!("CARGO_PKG_VERSION")),
        "exportedAt": Utc::now(),
        "workspace": { "name": team_name },
        "counts": {
            "collections": count(&collections), "documents": count(&documents), "revisions": count(&revisions),
            "comments": count(&comments), "attachments": files.len(),
        },
        "users": users,
        "collections": collections,
        "collectionMemberships": memberships,
        "documents": documents,
        "revisions": revisions,
        "comments": comments,
        "attachments": attachments,
    });
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| ApiError::internal(e.to_string()))?;
    files.insert(0, (BACKUP_FILE.to_string(), json));
    Ok(files)
}

// ---------------------------------------------------------------------------
// Obsidian vault
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct VaultDoc {
    id: Uuid,
    url_id: String,
    collection_id: Uuid,
    parent_document_id: Option<Uuid>,
    title: String,
    text: String,
    position: f64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Obsidian file names can't contain these either
fn vault_name(s: &str) -> String {
    safe_name(&s.replace(['^', '[', ']'], "-"))
}

/// `:::info … :::` notices → Obsidian callouts (`> [!info]`)
fn notices_to_callouts(text: &str) -> String {
    let mut out = vec![];
    let mut in_notice = false;
    let mut fence: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = crate::html_md::code_fence(line);
        if let Some(f) = &fence {
            if marker.is_some_and(|m| m.starts_with(f.as_str())) {
                fence = None;
            }
        } else if marker.is_some() {
            fence = marker;
        } else if !in_notice {
            if let Some(kind) = trimmed.strip_prefix(":::").map(str::trim).filter(|k| ["info", "warning", "tip", "success"].contains(k)) {
                out.push(format!("> [!{kind}]"));
                in_notice = true;
                continue;
            }
        } else if trimmed == ":::" {
            in_notice = false;
            continue;
        }
        if in_notice {
            out.push(if line.is_empty() { ">".to_string() } else { format!("> {line}") });
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

async fn obsidian(st: &AppState, auth: &Auth) -> ApiResult<Vec<(String, Vec<u8>)>> {
    let team = auth.team()?;
    let ids = readable_collection_ids(&st.db, auth).await?;
    let collections: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM collections WHERE id = ANY($1) AND deleted_at IS NULL ORDER BY sort_index")
            .bind(&ids)
            .fetch_all(&st.db)
            .await?;
    let docs: Vec<VaultDoc> = sqlx::query_as(
        "SELECT id, url_id, collection_id, parent_document_id, title, text, position, created_at, updated_at FROM documents \
         WHERE team_id = $1 AND collection_id = ANY($2) AND deleted_at IS NULL AND archived_at IS NULL \
         AND published_at IS NOT NULL AND template = false",
    )
    .bind(team)
    .bind(&ids)
    .fetch_all(&st.db)
    .await?;

    // ---- paths: <Collection>/<Parent>/<Title>.md
    let mut path_of: HashMap<Uuid, String> = HashMap::new();
    let mut used_dirs = HashSet::new();
    for (cid, cname) in &collections {
        let mut dir = vault_name(cname);
        while !used_dirs.insert(dir.to_lowercase()) {
            dir.push('_');
        }
        let members: Vec<&VaultDoc> = docs.iter().filter(|d| d.collection_id == *cid).collect();
        let local: HashMap<Uuid, usize> = members.iter().enumerate().map(|(i, d)| (d.id, i)).collect();
        let titles: Vec<String> =
            members.iter().map(|d| if d.title.trim().is_empty() { "Untitled".into() } else { vault_name(&d.title) }).collect();
        let parents: Vec<Option<usize>> = members.iter().map(|d| d.parent_document_id.and_then(|p| local.get(&p).copied())).collect();
        // positions are fractional and per parent: rank them
        let mut by_pos: Vec<usize> = (0..members.len()).collect();
        by_pos.sort_by(|&a, &b| members[a].position.total_cmp(&members[b].position));
        let mut positions = vec![None; members.len()];
        for (rank, &i) in by_pos.iter().enumerate() {
            positions[i] = Some(rank as i64);
        }
        for (i, p) in tree_paths(&dir, &titles, &parents, &positions).into_iter().enumerate() {
            path_of.insert(members[i].id, p);
        }
    }
    let by_url_id: HashMap<&str, Uuid> = docs.iter().map(|d| (d.url_id.as_str(), d.id)).collect();

    // ---- attachments referenced by the exported documents
    static FILE_REF: OnceLock<Regex> = OnceLock::new();
    let file_ref = FILE_REF.get_or_init(|| Regex::new(r"/files/([0-9a-fA-F-]{36})/").unwrap());
    let mut att_ids: HashSet<Uuid> = HashSet::new();
    for d in &docs {
        for c in file_ref.captures_iter(&d.text) {
            if let Ok(id) = Uuid::parse_str(&c[1]) {
                att_ids.insert(id);
            }
        }
    }
    let att_ids: Vec<Uuid> = att_ids.into_iter().collect();
    let atts: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM attachments WHERE id = ANY($1) AND team_id = $2 ORDER BY created_at")
        .bind(&att_ids)
        .bind(team)
        .fetch_all(&st.db)
        .await?;
    let mut files = vec![];
    let mut att_path: HashMap<Uuid, String> = HashMap::new();
    let mut used_names = HashSet::new();
    for (id, name) in atts {
        let Ok(data) = tokio::fs::read(st.cfg.upload_dir.join(id.to_string())).await else { continue };
        let clean = vault_name(&name);
        let (stem, ext) = clean.rsplit_once('.').map(|(s, e)| (s.to_string(), format!(".{e}"))).unwrap_or((clean.clone(), String::new()));
        let mut file = clean.clone();
        let mut n = 2;
        while !used_names.insert(file.to_lowercase()) {
            file = format!("{stem} {n}{ext}");
            n += 1;
        }
        let path = format!("_attachments/{file}");
        att_path.insert(id, path.clone());
        files.push((path, data));
    }

    // ---- documents
    // links written by the editor: [label](/doc/slug-urlId#anchor), ![alt](/files/id/name), with or without the public URL
    let base = regex::escape(st.cfg.url.trim_end_matches('/'));
    let link = Regex::new(&format!(
        r#"(!?)\[((?:\\.|[^\]\\])*)\]\(<?(?:{base})?/(doc|files)/([^)\s>]+)>?(?:\s+"[^"]*")?\)"#
    ))
    .unwrap();
    for d in &docs {
        let Some(path) = path_of.get(&d.id) else { continue };
        // tables saved as HTML become Markdown tables first, so the links in them are rewritten too
        let source = crate::html_md::html_tables_to_gfm(&d.text);
        let text = link.replace_all(&source, |c: &Captures| {
            let (bang, label, kind, rest) = (&c[1], &c[2], &c[3], &c[4]);
            let alias = |l: &str| if l.is_empty() { String::new() } else { format!("|{l}") };
            if kind == "files" {
                let id = rest.split('/').next().and_then(|s| Uuid::parse_str(s).ok());
                return match id.and_then(|id| att_path.get(&id)) {
                    Some(p) if !bang.is_empty() => format!("![[{p}]]"),
                    Some(p) => format!("[[{p}{}]]", alias(label)),
                    None => c[0].to_string(),
                };
            }
            let (target, anchor) = rest.split_once('#').map(|(a, b)| (a, format!("#{b}"))).unwrap_or((rest, String::new()));
            let url_id = target.rsplit('-').next().unwrap_or(target);
            match by_url_id.get(url_id).and_then(|id| path_of.get(id)) {
                Some(p) => format!("[[{}{anchor}{}]]", p.trim_end_matches(".md"), alias(label)),
                // documents not in the vault (drafts, other collections): keep the text
                None => label.to_string(),
            }
        });
        // wiki link aliases inside table rows need an escaped pipe
        let text: String = text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with('|') {
                    static ALIAS: OnceLock<Regex> = OnceLock::new();
                    ALIAS
                        .get_or_init(|| Regex::new(r"\[\[([^\]|]+)\|([^\]]*)\]\]").unwrap())
                        .replace_all(l, "[[$1\\|$2]]")
                        .to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = notices_to_callouts(&text);
        let file_title = path.rsplit('/').next().unwrap_or("").trim_end_matches(".md");
        let mut fm = vec![];
        if file_title != d.title.trim() && !d.title.trim().is_empty() {
            let t = serde_json::to_string(d.title.trim()).unwrap_or_default();
            fm.push(format!("title: {t}"));
            fm.push(format!("aliases:\n  - {t}"));
        }
        fm.push(format!("created: {}", d.created_at.format("%Y-%m-%dT%H:%M:%S")));
        fm.push(format!("updated: {}", d.updated_at.format("%Y-%m-%dT%H:%M:%S")));
        let body = format!("---\n{}\n---\n\n{}\n", fm.join("\n"), text.trim());
        files.push((path.clone(), body.into_bytes()));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    #[test]
    fn callouts() {
        let md = "Intro\n\n:::warning\n**Title**\n\nBody\n:::\n\n```\n:::info\n```";
        assert_eq!(super::notices_to_callouts(md), "Intro\n\n> [!warning]\n> **Title**\n>\n> Body\n\n```\n:::info\n```");
        // fences inside list items
        let md = "1. ```bash\n   ls\n   ```\n\n:::tip\nok\n:::";
        assert_eq!(super::notices_to_callouts(md), "1. ```bash\n   ls\n   ```\n\n> [!tip]\n> ok");
    }
}
