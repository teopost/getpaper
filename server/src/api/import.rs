//! Bulk import of an archive (ZIP, or tar.gz) of Markdown / HTML / text files.
//!
//! Confluence space exports (`import_confluence`, `import_confluence_xml`) and Wiki.js exports and
//! storage files (`import_wikijs`) are recognised and rewritten to this layout first.
//! HTML files are converted to Markdown.
//!
//! Supported layouts (covers Outline, Notion, Obsidian, GitBook, BookStack markdown exports):
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
    http::StatusCode,
    Json,
};
use regex::{Captures, Regex};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{backup_restore, import_confluence, import_confluence_xml, import_wikijs};
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

pub(crate) fn percent_decode(s: &str) -> String {
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
pub(crate) fn join_path(base_dir: &str, target: &str) -> String {
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
        return (title.unwrap_or_else(|| fallback.to_string()), crate::html_md::convert(&body));
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
    (title.unwrap_or_else(|| fallback.to_string()), callouts_to_notices(s.trim_end()))
}

/// Obsidian callouts (`> [!warning] Title` + quoted lines) → notices (`:::warning`)
fn callouts_to_notices(s: &str) -> String {
    static START: OnceLock<Regex> = OnceLock::new();
    let start = re(&START, r"^>\s?\[!(\w+)\][+-]?\s*(.*)$");
    let mut out: Vec<String> = vec![];
    let mut in_callout = false;
    let mut fence: Option<String> = None;
    for line in s.lines() {
        if in_callout {
            if let Some(rest) = line.strip_prefix('>') {
                out.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
                continue;
            }
            out.push(":::".into());
            in_callout = false;
        }
        let marker = crate::html_md::code_fence(line);
        if let Some(f) = &fence {
            if marker.is_some_and(|m| m.starts_with(f.as_str())) {
                fence = None;
            }
        } else if marker.is_some() {
            fence = marker;
        } else if let Some(c) = start.captures(line) {
            let style = match c[1].to_lowercase().as_str() {
                "tip" | "hint" | "important" => "tip",
                "success" | "check" | "done" => "success",
                "warning" | "caution" | "attention" | "danger" | "error" | "bug" | "failure" | "fail" | "missing" => "warning",
                _ => "info",
            };
            out.push(format!(":::{style}"));
            if !c[2].trim().is_empty() {
                out.push(format!("**{}**", c[2].trim()));
                out.push(String::new());
            }
            in_callout = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_callout {
        out.push(":::".into());
    }
    out.join("\n")
}

struct Node {
    coll: String,
    key: Vec<String>,
    title: String,
    text: String,
    zip_path: Option<String>,
    id: Uuid,
    url_id: String,
    meta: DocMeta,
}

/// Per-document details some sources provide, keyed by path in the (rewritten) archive.
#[derive(Clone, Default)]
pub struct DocMeta {
    /// position among siblings; documents without one follow, sorted by name
    pub order: Option<i64>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// false: imported as a draft even when publishing (unpublished in the source)
    pub published: Option<bool>,
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
    /// format that was recognised: "confluence-xml", "confluence-html", "wikijs-export", "wikijs-storage" or "markdown"
    pub format: &'static str,
}

/// Where the archive comes from, as chosen by the user. Confluence and Wiki.js exports are recognised
/// by their content; every other tool exports folders of Markdown files handled by the generic importer.
#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    Auto,
    Confluence,
    WikiJs,
    Backup,
    Markdown,
}

impl Source {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "" | "auto" => Source::Auto,
            "confluence" => Source::Confluence,
            "wikijs" | "wiki.js" => Source::WikiJs,
            "getpaper" | "horsa" | "backup" => Source::Backup,
            // outline, notion, obsidian, markdown, …
            _ => Source::Markdown,
        }
    }
}

/// Files of an archive that are not imported: hidden files and folders, macOS and Windows metadata
fn ignored(name: &str) -> bool {
    name.split('/').any(|seg| seg.starts_with('.') || seg == "__MACOSX") || name.ends_with("Thumbs.db")
}

/// Exports that keep everything in one large file: the Confluence XML export, our backup, Wiki.js
fn single_file_export(name: &str) -> bool {
    name.ends_with("entities.xml") || super::backup_restore::is_backup_file(name) || name.ends_with("pages.json.gz")
}

fn is_tar(filename: &str) -> bool {
    let f = filename.to_lowercase();
    f.ends_with(".tar.gz") || f.ends_with(".tgz") || f.ends_with(".tar")
}

/// Files of a tar archive, gzipped or not (Wiki.js disk backups are `wiki-*.tar.gz`)
fn read_tar(filename: &str, bytes: Vec<u8>, max_file: usize) -> ApiResult<(Vec<(String, Vec<u8>)>, Vec<String>)> {
    let invalid = |e: std::io::Error| ApiError::bad_request(format!("Invalid tar archive: {e}"));
    let reader: Box<dyn Read> =
        if filename.to_lowercase().ends_with(".tar") { Box::new(Cursor::new(bytes)) } else { Box::new(flate2::read::GzDecoder::new(Cursor::new(bytes))) };
    let mut archive = tar::Archive::new(reader);
    let mut entries = vec![];
    let mut skipped = vec![];
    for f in archive.entries().map_err(invalid)? {
        let mut f = f.map_err(invalid)?;
        if !f.header().entry_type().is_file() {
            continue;
        }
        let name = f.path().map_err(invalid)?.to_string_lossy().replace('\\', "/");
        let name = name.trim_start_matches("./").trim_start_matches('/').to_string();
        if name.is_empty() || ignored(&name) {
            continue;
        }
        if entries.len() + skipped.len() >= MAX_ENTRIES {
            return Err(ApiError::bad_request(format!("The archive contains more than {MAX_ENTRIES} files")));
        }
        let size = f.header().size().unwrap_or(0) as usize;
        if size > max_file && !single_file_export(&name) {
            skipped.push(format!("{name} (too large)"));
            continue;
        }
        let mut buf = Vec::with_capacity(size);
        f.read_to_end(&mut buf).map_err(|e| ApiError::bad_request(format!("Cannot read {name}: {e}")))?;
        entries.push((name, buf));
    }
    Ok((entries, skipped))
}

fn read_entries(filename: &str, bytes: Vec<u8>, max_file: usize) -> ApiResult<(Vec<(String, Vec<u8>)>, Vec<String>)> {
    if is_tar(filename) {
        return read_tar(filename, bytes, max_file);
    }
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
        if ignored(&name) {
            continue;
        }
        if f.size() as usize > max_file && !single_file_export(&name) {
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
    source: Source,
) -> ApiResult<ImportResult> {
    let team = target.as_ref().map(|c| c.team_id).unwrap_or(auth.team()?);
    let max_file = st.cfg.max_upload_mb * 1024 * 1024;
    let (mut entries, mut skipped) = read_entries(filename, bytes, max_file)?;
    if entries.is_empty() {
        return Err(ApiError::bad_request("The archive is empty"));
    }
    // a backup of this product is restored as it is, not imported as files
    let is_backup = backup_restore::detect(&entries);
    match (source, is_backup) {
        (Source::Auto | Source::Backup, true) => return backup_restore::restore(st, auth, entries).await,
        (_, true) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "getpaper_backup",
                "This archive is a GetPaper backup. Choose GetPaper backup or automatic detection as the source.",
            ))
        }
        (Source::Backup, false) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "not_getpaper_backup",
                "This archive is not a GetPaper backup: getpaper-backup.json is missing.",
            ))
        }
        _ => {}
    }
    let mut meta: HashMap<String, DocMeta> = HashMap::new();
    // Wiki.js content chosen as "Markdown / other" is imported as plain Markdown files
    let wikijs = if matches!(source, Source::Auto | Source::WikiJs) { import_wikijs::detect(&entries) } else { None };
    let format = if import_confluence_xml::detect(&entries) {
        "confluence-xml"
    } else if import_confluence::detect(&entries) {
        "confluence-html"
    } else if let Some(layout) = wikijs {
        layout.format()
    } else {
        "markdown"
    };
    // the codes let the web client show a translated message
    match (source, format) {
        (Source::Confluence, "markdown") => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "not_confluence_export",
                "This archive is not a Confluence export: it contains neither entities.xml (XML export) nor Confluence HTML pages. \
                 Export the space as XML or HTML and upload the ZIP as it is.",
            ))
        }
        (Source::WikiJs, "markdown") => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "not_wikijs_export",
                "This archive is not a Wiki.js export: it contains neither pages.json.gz (Administration → Utilities → Export) \
                 nor Wiki.js page files with their metadata (Local File System or Git storage).",
            ))
        }
        (Source::Markdown | Source::WikiJs, "confluence-xml" | "confluence-html") => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "confluence_export",
                "This archive is a Confluence export. Choose Confluence or automatic detection as the source.",
            ))
        }
        _ => {}
    }
    if format == "confluence-xml" {
        (entries, meta) = import_confluence_xml::convert(entries)?;
    } else if format == "confluence-html" {
        (entries, meta) = import_confluence::convert(entries);
    } else if let Some(layout) = wikijs {
        let extra;
        (entries, meta, extra) = import_wikijs::convert(entries, layout, target.is_some())?;
        skipped.extend(extra);
    }
    tracing::info!(format, files = entries.len(), "importing");

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
        meta = meta.into_iter().map(|(p, v)| (p.split_once('/').map(|(_, r)| r.to_string()).unwrap_or_default(), v)).collect();
    }

    let default_coll = {
        let base = filename.rsplit('/').next().unwrap_or(filename);
        let base = base.strip_suffix(".gz").unwrap_or(base);
        clean_name(base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base))
    };

    // ---- build the document tree
    let mut nodes: Vec<Node> = vec![];
    let mut index: HashMap<(String, Vec<String>), usize> = HashMap::new();
    let mut attachments: Vec<(String, Vec<u8>)> = vec![];

    // archives rewritten from a structured export list their documents in `meta`; any other file is an attachment
    let structured = !meta.is_empty();
    for (path, data) in entries {
        let ext = ext_of(&path);
        let is_doc = if structured { meta.contains_key(&path) } else { DOC_EXTS.contains(&ext.as_str()) };
        if !is_doc {
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
        let meta = meta.get(&path).cloned().unwrap_or_default();
        nodes.push(Node { coll, key, title, text, zip_path: Some(path), id: Uuid::new_v4(), url_id: new_url_id(), meta });
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
                    meta: DocMeta::default(),
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
        // metadata files of Markdown exports; real attachments in structured exports
        if !structured && matches!(ext.as_str(), "json" | "xml" | "yml" | "yaml") {
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
    let md_link = re(&MD_LINK, r#"(!?\[(?:\\.|[^\]\\])*\]\()(<[^>]+>|[^)\s]+)((?:\s+"[^"]*")?\))"#);
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
        let (pa, pb) = (&na.key[..na.key.len().saturating_sub(1)], &nb.key[..nb.key.len().saturating_sub(1)]);
        na.key
            .len()
            .cmp(&nb.key.len())
            .then_with(|| (&na.coll, pa).cmp(&(&nb.coll, pb)))
            .then_with(|| na.meta.order.unwrap_or(i64::MAX).cmp(&nb.meta.order.unwrap_or(i64::MAX)))
            .then_with(|| na.key.cmp(&nb.key))
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
        let published = publish && n.meta.published.unwrap_or(true);
        sqlx::query(
            "INSERT INTO documents (id, url_id, team_id, collection_id, parent_document_id, title, text, created_by, updated_by, \
             published_at, position, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, CASE WHEN $9 THEN COALESCE($12, now()) ELSE NULL END, $10, \
             COALESCE($11, now()), COALESCE($12, now()))",
        )
        .bind(n.id)
        .bind(&n.url_id)
        .bind(team)
        .bind(coll_ids.get(&n.coll).copied())
        .bind(parent)
        .bind(&n.title)
        .bind(&n.text)
        .bind(auth.user.id)
        .bind(published)
        .bind(*pos)
        .bind(n.meta.created_at)
        .bind(n.meta.updated_at)
        .execute(&mut *tx)
        .await?;
        if published {
            sqlx::query(
                "INSERT INTO revisions (id, document_id, user_id, title, text, created_at) VALUES ($1, $2, $3, $4, $5, COALESCE($6, now()))",
            )
                .bind(Uuid::new_v4())
                .bind(n.id)
                .bind(auth.user.id)
                .bind(&n.title)
                .bind(&n.text)
                .bind(n.meta.updated_at)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    Ok(ImportResult { collections: created_collections, documents: nodes.len(), attachments: att_count, skipped, format })
}

pub async fn import(State(st): State<AppState>, auth: Auth, mut mp: Multipart) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut collection_id: Option<String> = None;
    let mut publish = true;
    let mut source = Source::Auto;
    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        match field.name().unwrap_or("") {
            "source" => source = Source::parse(&field.text().await.unwrap_or_default()),
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
    let r = import_archive(&st, &auth, &name, bytes, target, publish, source).await?;
    tracing::info!(documents = r.documents, attachments = r.attachments, "import completed");
    let data = json!({ "file": name, "format": r.format, "documents": r.documents, "attachments": r.attachments, "collections": r.collections.iter().filter_map(|c| c["name"].as_str()).collect::<Vec<_>>() });
    crate::audit::record(&st, &auth, "data.import", None, data).await;
    ok(json!({
        "format": r.format,
        "collections": r.collections,
        "documents": r.documents,
        "attachments": r.attachments,
        "skipped": r.skipped.into_iter().take(100).collect::<Vec<_>>(),
    }))
}
