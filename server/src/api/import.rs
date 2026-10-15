//! Bulk import of an archive (ZIP) of Markdown / HTML / text files.
//!
//! Supported layouts (covers Outline, Notion, Obsidian, Wiki.js, GitBook, BookStack markdown exports):
//! - top-level folders become collections (unless a target collection is given)
//! - nested folders become nested documents; `Doc.md` + `Doc/child.md` attaches the children to `Doc`
//! - `index.md` / `README.md` inside a folder becomes the content of the folder document
//! - other files (images, PDFs, …) become attachments and relative links to them are rewritten
//! - relative links between documents and Obsidian `[[wiki links]]` are rewritten to internal links

use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read},
    sync::OnceLock,
};

use axum::{
    extract::{Multipart, State},
    Json,
};
use regex::{Captures, Regex};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::{Access, Collection},
    perm::load_collection,
    util::{new_url_id, random_color, slugify},
    AppState,
};

const DOC_EXTS: &[&str] = &["md", "markdown", "txt", "html", "htm"];
const MAX_ENTRIES: usize = 10_000;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

fn ext_of(p: &str) -> String {
    let name = p.rsplit('/').next().unwrap_or(p);
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

fn strip_ext(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && DOC_EXTS.contains(&e.to_lowercase().as_str()) => s,
        _ => name,
    }
}

/// Removes Notion style id suffixes ("Page 1a2b…32 hex") and surrounding whitespace.
fn clean_name(s: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let s = re(&R, r"\s+[0-9a-fA-F]{32}$").replace(s, "");
    let s = s.trim();
    if s.is_empty() {
        "Untitled".into()
    } else {
        s.to_string()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Joins `base_dir` and a relative `target`, resolving `.` and `..`.
fn join_path(base_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if base_dir.is_empty() { vec![] } else { base_dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn content_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

fn strip_tags(s: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"<[^>]*>").replace_all(s, "").trim().to_string()
}

/// Extract (title, text) from a file.
fn parse_doc(ext: &str, raw: &[u8], fallback: &str) -> (String, String) {
    let mut s = String::from_utf8_lossy(raw).to_string();
    if s.starts_with('\u{feff}') {
        s.remove(0);
    }
    s = s.replace("\r\n", "\n");
    let mut title: Option<String> = None;

    if ext == "html" || ext == "htm" {
        static TITLE: OnceLock<Regex> = OnceLock::new();
        static BODY: OnceLock<Regex> = OnceLock::new();
        static H1: OnceLock<Regex> = OnceLock::new();
        static SCRIPT: OnceLock<Regex> = OnceLock::new();
        if let Some(c) = re(&TITLE, r"(?is)<title[^>]*>(.*?)</title>").captures(&s) {
            title = Some(strip_tags(&c[1])).filter(|t| !t.is_empty());
        }
        let mut body = re(&BODY, r"(?is)<body[^>]*>(.*)</body>")
            .captures(&s)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| s.clone());
        body = re(&SCRIPT, r"(?is)<(script|style|head)[^>]*>.*?</(script|style|head)>").replace_all(&body, "").to_string();
        if let Some(c) = re(&H1, r"(?is)<h1[^>]*>(.*?)</h1>").captures(&body.clone()) {
            let h = strip_tags(&c[1]);
            if title.is_none() || title.as_deref() == Some(h.as_str()) {
                title = Some(h);
                body = body.replacen(&c[0], "", 1);
            }
        }
        return (title.unwrap_or_else(|| fallback.to_string()), body.trim().to_string());
    }

    // YAML front matter
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                if let Some(t) = line.strip_prefix("title:") {
                    let t = t.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !t.is_empty() {
                        title = Some(t);
                    }
                }
            }
            let after = &rest[end + 4..];
            s = after.strip_prefix('\n').unwrap_or(after).to_string();
        }
    }
    // leading "# Title"
    let trimmed = s.trim_start_matches('\n');
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let (first, remaining) = rest.split_once('\n').unwrap_or((rest, ""));
        if title.is_none() || title.as_deref() == Some(first.trim()) {
            title = Some(first.trim().to_string());
            s = remaining.trim_start_matches('\n').to_string();
        }
    }
    (title.unwrap_or_else(|| fallback.to_string()), s.trim_end().to_string())
}

struct Node {
    coll: String,
    key: Vec<String>,
    title: String,
    text: String,
    zip_path: Option<String>,
    id: Uuid,
    url_id: String,
}

impl Node {
    fn url(&self) -> String {
        format!("/doc/{}-{}", slugify(&self.title), self.url_id)
    }
}

pub struct ImportResult {
    pub collections: Vec<Value>,
    pub documents: usize,
    pub attachments: usize,
    pub skipped: Vec<String>,
}

fn read_entries(filename: &str, bytes: Vec<u8>, max_file: usize) -> ApiResult<(Vec<(String, Vec<u8>)>, Vec<String>)> {
    let mut entries = vec![];
    let mut skipped = vec![];
    if ext_of(filename) != "zip" {
        entries.push((filename.rsplit('/').next().unwrap_or(filename).to_string(), bytes));
        return Ok((entries, skipped));
    }
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| ApiError::bad_request(format!("Invalid ZIP archive: {e}")))?;
    if z.len() > MAX_ENTRIES {
        return Err(ApiError::bad_request(format!("The archive contains more than {MAX_ENTRIES} files")));
    }
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| ApiError::bad_request(format!("Invalid ZIP archive: {e}")))?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().replace('\\', "/").trim_start_matches('/').to_string();
        if name.split('/').any(|seg| seg.starts_with('.') || seg == "__MACOSX") || name.ends_with("Thumbs.db") {
            continue;
        }
        if f.size() as usize > max_file {
            skipped.push(format!("{name} (too large)"));
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| ApiError::bad_request(format!("Cannot read {name}: {e}")))?;
        entries.push((name, buf));
    }
    Ok((entries, skipped))
}

pub async fn import_archive(
    st: &AppState,
    auth: &Auth,
    filename: &str,
    bytes: Vec<u8>,
    target: Option<Collection>,
    publish: bool,
) -> ApiResult<ImportResult> {
    let team = target.as_ref().map(|c| c.team_id).unwrap_or(auth.team()?);
    let max_file = st.cfg.max_upload_mb * 1024 * 1024;
    let (mut entries, mut skipped) = read_entries(filename, bytes, max_file)?;
    if entries.is_empty() {
        return Err(ApiError::bad_request("The archive is empty"));
    }

    // Strip a single wrapper folder that directly contains no documents ("export/Coll/…").
    loop {
        let first: Vec<&str> = entries.iter().map(|(p, _)| p.split('/').next().unwrap_or("")).collect();
        let all_nested = entries.iter().all(|(p, _)| p.contains('/'));
        let same = first.iter().all(|f| *f == first[0]);
        let docs_at_root2 = entries
            .iter()
            .any(|(p, _)| p.split('/').count() == 2 && DOC_EXTS.contains(&ext_of(p).as_str()));
        let strip = all_nested && same && (target.is_some() || !docs_at_root2);
        if !strip {
            break;
        }
        for (p, _) in entries.iter_mut() {
            *p = p.split_once('/').map(|(_, r)| r.to_string()).unwrap_or_default();
        }
    }

    let default_coll = {
        let base = filename.rsplit('/').next().unwrap_or(filename);
        clean_name(base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base))
    };

    // ---- build the document tree
    let mut nodes: Vec<Node> = vec![];
    let mut index: HashMap<(String, Vec<String>), usize> = HashMap::new();
    let mut attachments: Vec<(String, Vec<u8>)> = vec![];

    for (path, data) in entries {
        let ext = ext_of(&path);
        if !DOC_EXTS.contains(&ext.as_str()) {
            attachments.push((path, data));
            continue;
        }
        let segs: Vec<&str> = path.split('/').collect();
        let (coll, rel): (String, Vec<&str>) = if target.is_some() {
            (String::new(), segs.clone())
        } else if segs.len() >= 2 {
            (clean_name(segs[0]), segs[1..].to_vec())
        } else {
            (default_coll.clone(), segs.clone())
        };
        let mut key: Vec<String> = rel.iter().map(|s| clean_name(strip_ext(s))).collect();
        let last = key.last().cloned().unwrap_or_default().to_lowercase();
        if key.len() > 1 && matches!(last.as_str(), "index" | "readme" | "_index" | "home") {
            key.pop();
        }
        let fallback = key.last().cloned().unwrap_or_else(|| "Untitled".into());
        let (title, text) = parse_doc(&ext, &data, &fallback);
        let k = (coll.clone(), key.clone());
        if let Some(&i) = index.get(&k) {
            // e.g. "Doc.md" and "Doc/index.md": merge
            let n = &mut nodes[i];
            if n.text.is_empty() {
                n.text = text;
            } else if !text.is_empty() {
                n.text = format!("{}\n\n{}", n.text, text);
            }
            if n.zip_path.is_none() {
                n.zip_path = Some(path);
                n.title = title;
            }
            continue;
        }
        index.insert(k, nodes.len());
        nodes.push(Node { coll, key, title, text, zip_path: Some(path), id: Uuid::new_v4(), url_id: new_url_id() });
    }

    // placeholders for folders without a document
    let mut i = 0;
    while i < nodes.len() {
        let (coll, key) = (nodes[i].coll.clone(), nodes[i].key.clone());
        for depth in 1..key.len() {
            let prefix = key[..depth].to_vec();
            let k = (coll.clone(), prefix.clone());
            if !index.contains_key(&k) {
                index.insert(k, nodes.len());
                nodes.push(Node {
                    coll: coll.clone(),
                    title: prefix.last().cloned().unwrap_or_default(),
                    key: prefix,
                    text: String::new(),
                    zip_path: None,
                    id: Uuid::new_v4(),
                    url_id: new_url_id(),
                });
            }
        }
        i += 1;
    }
    if nodes.is_empty() {
        return Err(ApiError::bad_request(
            "No documents found. Supported formats are .md, .markdown, .txt and .html files, optionally inside a .zip archive.",
        ));
    }

    // ---- store attachments
    let mut tx = st.db.begin().await?;
    let mut att_urls: HashMap<String, String> = HashMap::new();
    let mut att_count = 0;
    for (path, data) in attachments {
        let ext = ext_of(&path);
        if ext == "json" || ext == "xml" || ext == "yml" || ext == "yaml" {
            skipped.push(path);
            continue;
        }
        let name = path.rsplit('/').next().unwrap_or(&path);
        let safe: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' })
            .collect();
        let id = Uuid::new_v4();
        tokio::fs::write(st.cfg.upload_dir.join(id.to_string()), &data).await?;
        sqlx::query(
            "INSERT INTO attachments (id, team_id, user_id, name, content_type, size) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(team)
        .bind(auth.user.id)
        .bind(&safe)
        .bind(content_type(&ext))
        .bind(data.len() as i64)
        .execute(&mut *tx)
        .await?;
        att_urls.insert(path.clone(), format!("/files/{id}/{safe}"));
        att_count += 1;
    }

    // ---- rewrite links
    let doc_urls: HashMap<String, String> =
        nodes.iter().filter_map(|n| n.zip_path.clone().map(|p| (p, n.url()))).collect();
    let mut title_urls: HashMap<String, String> = HashMap::new();
    for n in &nodes {
        title_urls.entry(n.title.to_lowercase()).or_insert_with(|| n.url());
        if let Some(last) = n.key.last() {
            title_urls.entry(last.to_lowercase()).or_insert_with(|| n.url());
        }
    }
    // fallback: unique file names anywhere in the archive (exports often have imprecise relative links)
    let mut by_name: HashMap<String, Option<String>> = HashMap::new();
    for (p, u) in att_urls.iter().chain(doc_urls.iter()) {
        let name = p.rsplit('/').next().unwrap_or(p).to_lowercase();
        by_name.entry(name).and_modify(|e| *e = None).or_insert_with(|| Some(u.clone()));
    }
    let resolve = |base_dir: &str, target: &str| -> Option<String> {
        let t = target.trim().trim_start_matches('<').trim_end_matches('>');
        if t.is_empty() || t.starts_with('#') || t.starts_with('/') || t.contains("://") || t.starts_with("mailto:") || t.starts_with("data:") {
            return None;
        }
        let (path_part, anchor) = match t.split_once('#') {
            Some((p, a)) => (p, Some(a)),
            None => (t, None),
        };
        let path_part = path_part.split('?').next().unwrap_or(path_part);
        let full = join_path(base_dir, &percent_decode(path_part));
        let found = att_urls
            .get(&full)
            .or_else(|| doc_urls.get(&full))
            .or_else(|| doc_urls.get(&format!("{full}.md")))
            .cloned()
            .or_else(|| {
                let name = full.rsplit('/').next().unwrap_or(&full).to_lowercase();
                by_name
                    .get(&name)
                    .or_else(|| by_name.get(&format!("{name}.md")))
                    .cloned()
                    .flatten()
            })?;
        Some(match anchor {
            Some(a) if !found.starts_with("/files/") => format!("{found}#{a}"),
            _ => found,
        })
    };
    static MD_LINK: OnceLock<Regex> = OnceLock::new();
    static HTML_ATTR: OnceLock<Regex> = OnceLock::new();
    static WIKI: OnceLock<Regex> = OnceLock::new();
    let md_link = re(&MD_LINK, r#"(!?\[[^\]]*\]\()(<[^>]+>|[^)\s]+)((?:\s+"[^"]*")?\))"#);
    let html_attr = re(&HTML_ATTR, r#"\b(src|href)="([^"]+)""#);
    let wiki = re(&WIKI, r"(!?)\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]");
    for n in nodes.iter_mut() {
        let Some(zp) = &n.zip_path else { continue };
        let base_dir = zp.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();
        let t = md_link.replace_all(&n.text, |c: &Captures| match resolve(&base_dir, &c[2]) {
            Some(u) => format!("{}{}{}", &c[1], u, &c[3]),
            None => c[0].to_string(),
        });
        let t = html_attr.replace_all(&t, |c: &Captures| match resolve(&base_dir, &c[2]) {
            Some(u) => format!("{}=\"{}\"", &c[1], u),
            None => c[0].to_string(),
        });
        let t = wiki.replace_all(&t, |c: &Captures| {
            let target = c[2].trim();
            let label = c.get(3).map(|m| m.as_str().trim()).unwrap_or(target);
            if &c[1] == "!" {
                // embedded attachment: ![[image.png]]
                let found = att_urls.iter().find(|(p, _)| p.rsplit('/').next() == Some(target)).map(|(_, u)| u.clone());
                return match found {
                    Some(u) => format!("![{label}]({u})"),
                    None => c[0].to_string(),
                };
            }
            match title_urls.get(&target.to_lowercase()) {
                Some(u) => format!("[{label}]({u})"),
                None => label.to_string(),
            }
        });
        n.text = t.to_string();
    }

    // ---- create collections
    let mut coll_ids: BTreeMap<String, Uuid> = BTreeMap::new();
    let mut created_collections = vec![];
    if let Some(c) = &target {
        coll_ids.insert(String::new(), c.id);
        created_collections.push(json!({ "id": c.id, "urlId": c.url_id, "name": c.name, "created": false }));
    } else {
        let mut names: Vec<String> = nodes.iter().map(|n| n.coll.clone()).collect();
        names.sort();
        names.dedup();
        let (max,): (Option<f64>,) = sqlx::query_as("SELECT max(sort_index) FROM collections WHERE team_id = $1")
            .bind(team)
            .fetch_one(&mut *tx)
            .await?;
        for (i, name) in names.into_iter().enumerate() {
            let id = Uuid::new_v4();
            let url_id = new_url_id();
            sqlx::query(
                "INSERT INTO collections (id, url_id, team_id, name, color, permission, sort_index, created_by) \
                 VALUES ($1, $2, $3, $4, $5, 'read_write', $6, $7)",
            )
            .bind(id)
            .bind(&url_id)
            .bind(team)
            .bind(&name)
            .bind(random_color())
            .bind(max.unwrap_or(0.0) + 1.0 + i as f64)
            .bind(auth.user.id)
            .execute(&mut *tx)
            .await?;
            created_collections.push(json!({ "id": id, "urlId": url_id, "name": name, "created": true }));
            coll_ids.insert(name, id);
        }
    }

    // ---- create documents, parents first, siblings in natural order
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        let (na, nb) = (&nodes[a], &nodes[b]);
        na.key.len().cmp(&nb.key.len()).then_with(|| na.key.cmp(&nb.key))
    });
    let mut positions: HashMap<(String, Vec<String>), f64> = HashMap::new();
    for i in order {
        let n = &nodes[i];
        let parent = if n.key.len() > 1 {
            index.get(&(n.coll.clone(), n.key[..n.key.len() - 1].to_vec())).map(|&p| nodes[p].id)
        } else {
            None
        };
        let pos_key = (n.coll.clone(), n.key[..n.key.len().saturating_sub(1)].to_vec());
        let pos = positions.entry(pos_key).or_insert(0.0);
        *pos += 1.0;
        sqlx::query(
            "INSERT INTO documents (id, url_id, team_id, collection_id, parent_document_id, title, text, created_by, updated_by, published_at, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, CASE WHEN $9 THEN now() ELSE NULL END, $10)",
        )
        .bind(n.id)
        .bind(&n.url_id)
        .bind(team)
        .bind(coll_ids.get(&n.coll).copied())
        .bind(parent)
        .bind(&n.title)
        .bind(&n.text)
        .bind(auth.user.id)
        .bind(publish)
        .bind(*pos)
        .execute(&mut *tx)
        .await?;
        if publish {
            sqlx::query("INSERT INTO revisions (id, document_id, user_id, title, text) VALUES ($1, $2, $3, $4, $5)")
                .bind(Uuid::new_v4())
                .bind(n.id)
                .bind(auth.user.id)
                .bind(&n.title)
                .bind(&n.text)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    Ok(ImportResult { collections: created_collections, documents: nodes.len(), attachments: att_count, skipped })
}

pub async fn import(State(st): State<AppState>, auth: Auth, mut mp: Multipart) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut collection_id: Option<String> = None;
    let mut publish = true;
    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        match field.name().unwrap_or("") {
            "file" => {
                let name = field.file_name().unwrap_or("import.zip").to_string();
                let bytes = field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?;
                file = Some((name, bytes.to_vec()));
            }
            "collectionId" => collection_id = Some(field.text().await.unwrap_or_default()).filter(|s| !s.is_empty()),
            "publish" => publish = field.text().await.unwrap_or_default() != "false",
            _ => {}
        }
    }
    let (name, bytes) = file.ok_or_else(|| ApiError::bad_request("file is required"))?;
    let target = match collection_id {
        Some(c) => Some(load_collection(&st.db, &auth, &c, Access::Write).await?.0),
        None => None,
    };
    let r = import_archive(&st, &auth, &name, bytes, target, publish).await?;
    tracing::info!(documents = r.documents, attachments = r.attachments, "import completed");
    ok(json!({
        "collections": r.collections,
        "documents": r.documents,
        "attachments": r.attachments,
        "skipped": r.skipped.into_iter().take(100).collect::<Vec<_>>(),
    }))
}
