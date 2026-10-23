//! Wiki.js 2.x content → the folder-of-Markdown layout understood by the generic importer.
//!
//! Two layouts are recognised:
//! - Administration → Utilities → Export: `pages.json.gz` (every page with its source, title, dates,
//!   publishing state and locale) next to an `assets/` folder; the other files of the export (users,
//!   groups, comments, history, navigation, settings) are not imported;
//! - the files of the Local File System or Git storage, or a disk backup `wiki-*.tar.gz`: one
//!   `<path>.md` / `<path>.html` per page with the metadata in a front matter (`---` or `<!-- -->`
//!   with `title`, `published`, `date`, `editor`, `dateCreated`), and the assets in their folders.
//!
//! Pages in a locale other than the main one go under a `<locale>/` folder, as in Wiki.js storage.
//! A top-level page with the same path as a folder (`guide` and `guide/…`) names the collection made
//! from that folder and becomes one of its documents. Links in Wiki.js are usually absolute
//! (`/guide/page`, `/en/guide/page`, `/folder/image.png`): links to pages and assets of the archive
//! are rewritten as relative links, which the generic importer resolves. HTML pages are converted to
//! Markdown; AsciiDoc pages of the export are imported from their rendered HTML. Wiki.js Markdown
//! extras: a blockquote followed by `{.is-info}` (success, warning, danger) becomes a notice,
//! `=300x` image sizes and `{.class}` attributes are removed.

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::OnceLock,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use regex::{Captures, Regex};
use serde_json::Value;

use super::import::{join_path, percent_decode, DocMeta};
use crate::{
    error::{ApiError, ApiResult},
    html_md, terminal_svg,
};

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    /// Administration → Utilities → Export
    Export,
    /// Local File System / Git storage files
    Storage,
}

impl Layout {
    pub fn format(self) -> &'static str {
        match self {
            Layout::Export => "wikijs-export",
            Layout::Storage => "wikijs-storage",
        }
    }
}

fn file_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn ext_of(p: &str) -> String {
    file_name(p).rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// Backup archives that the disk storage keeps inside its own folder
fn in_backup_folder(p: &str) -> bool {
    p.split('/').any(|s| s == "_daily" || s == "_manual")
}

fn text_of(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.strip_prefix('\u{feff}').unwrap_or(&s).replace("\r\n", "\n")
}

/// The front matter Wiki.js writes in storage files: `---` (Markdown) or `<!-- -->` (HTML) around
/// `key: value` lines. Returns the values and the content after it.
fn front_matter(text: &str, html: bool) -> Option<(HashMap<String, String>, String)> {
    let (open, close) = if html { ("<!--\n", "\n-->") } else { ("---\n", "\n---") };
    let rest = text.strip_prefix(open)?;
    let end = rest.find(close)?;
    let mut values = HashMap::new();
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
            values.insert(k.trim().to_string(), v.to_string());
        }
    }
    let body = rest[end + close.len()..].trim_start_matches('\n').to_string();
    Some((values, body))
}

/// Storage files are recognised by the keys Wiki.js always writes
fn is_storage_page(path: &str, raw: &[u8]) -> bool {
    let html = match ext_of(path).as_str() {
        "md" | "markdown" => false,
        "html" | "htm" => true,
        _ => return false,
    };
    let head = &raw[..raw.len().min(4096)];
    front_matter(&text_of(head), html)
        .is_some_and(|(v, _)| v.contains_key("title") && v.contains_key("editor") && v.contains_key("dateCreated"))
}

/// The Wiki.js layout of the archive, if it is one.
pub fn detect(entries: &[(String, Vec<u8>)]) -> Option<Layout> {
    if entries.iter().any(|(p, _)| file_name(p) == "pages.json.gz") {
        Some(Layout::Export)
    } else if entries.iter().any(|(p, d)| !in_backup_folder(p) && is_storage_page(p, d)) {
        Some(Layout::Storage)
    } else {
        None
    }
}

struct Page {
    /// path in the wiki, with the `<locale>/` prefix for the other locales
    path: String,
    title: Option<String>,
    /// Markdown, or HTML when `html`
    content: String,
    html: bool,
    published: bool,
    created: Option<DateTime<Utc>>,
    updated: Option<DateTime<Utc>>,
}

fn date(s: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s?.trim()).ok().map(|d| d.with_timezone(&Utc))
}

/// `isPublished` is a boolean with PostgreSQL, 0 / 1 with SQLite and MySQL
fn flag(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|n| n != 0.0),
        Value::String(s) => Some(!matches!(s.trim(), "false" | "0" | "")),
        _ => None,
    }
}

fn locale_like(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"(?i)^[a-z]{2}(-[a-z]{2})?$").is_match(s)
}

/// The array of pages of `pages.json.gz`. Wiki.js writes the pages in batches and leaves out the comma between
/// the last page of a batch and the first of the next one (`}{`): the pages are read one by one, skipping the
/// commas that are there.
fn parse_pages(json: &str) -> serde_json::Result<Vec<Value>> {
    if let Ok(pages) = serde_json::from_str(json) {
        return Ok(pages);
    }
    let body = json.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']'));
    let Some(mut rest) = body else { return serde_json::from_str(json) };
    let mut pages = vec![];
    loop {
        rest = rest.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        if rest.is_empty() {
            return Ok(pages);
        }
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        match stream.next() {
            Some(page) => pages.push(page?),
            None => return Ok(pages),
        }
        rest = &rest[stream.byte_offset()..];
    }
}

/// Wiki.js reads the pages in batches of 10 without an order, so the same page can be exported more than once (and
/// others not at all): one page per `id`, the most recently updated one, in the order of its first appearance.
fn dedup_pages(pages: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    let mut seen: HashMap<i64, usize> = HashMap::new();
    for p in pages {
        let Some(id) = p["id"].as_i64() else {
            out.push(p);
            continue;
        };
        match seen.get(&id) {
            Some(&i) => {
                if date(p["updatedAt"].as_str()) > date(out[i]["updatedAt"].as_str()) {
                    out[i] = p;
                }
            }
            None => {
                seen.insert(id, out.len());
                out.push(p);
            }
        }
    }
    out
}

/// Pages and assets of an Administration → Utilities → Export folder
fn read_export(entries: Vec<(String, Vec<u8>)>, skipped: &mut Vec<String>) -> ApiResult<(Vec<Page>, Vec<(String, Vec<u8>)>)> {
    let json_path = entries.iter().map(|(p, _)| p).find(|p| file_name(p) == "pages.json.gz").cloned().unwrap_or_default();
    let base = json_path.strip_suffix("pages.json.gz").unwrap_or("").to_string();
    let mut pages_raw: Vec<Value> = vec![];
    let mut assets = vec![];
    for (path, data) in entries {
        if path == json_path {
            let mut json = String::new();
            flate2::read::GzDecoder::new(&data[..])
                .read_to_string(&mut json)
                .map_err(|e| ApiError::bad_request(format!("Cannot read pages.json.gz: {e}")))?;
            pages_raw = parse_pages(&json).map_err(|e| ApiError::bad_request(format!("Cannot read pages.json.gz: {e}")))?;
        } else if let Some(asset) = path.strip_prefix(&format!("{base}assets/")) {
            assets.push((asset.to_string(), data));
        } else {
            // users, groups, comments, history, navigation, settings
            skipped.push(path);
        }
    }
    let pages_raw = dedup_pages(pages_raw);
    let locale = |p: &Value| p["localeCode"].as_str().unwrap_or("").to_string();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in &pages_raw {
        *counts.entry(locale(p)).or_default() += 1;
    }
    let main = counts.into_iter().max_by_key(|(l, n)| (*n, std::cmp::Reverse(l.clone()))).map(|(l, _)| l).unwrap_or_default();
    let mut pages = vec![];
    for p in &pages_raw {
        let Some(path) = p["path"].as_str().map(|s| s.trim_matches('/')).filter(|s| !s.is_empty()) else { continue };
        let loc = locale(p);
        let path = if loc.is_empty() || loc == main { path.to_string() } else { format!("{loc}/{path}") };
        let str_of = |k: &str| p[k].as_str().unwrap_or("").to_string();
        // AsciiDoc and other editors: the HTML that Wiki.js rendered
        let (content, html) = match p["contentType"].as_str().unwrap_or("markdown") {
            "markdown" => (str_of("content"), false),
            "html" => (str_of("content"), true),
            _ => (str_of("render"), true),
        };
        pages.push(Page {
            path,
            title: p["title"].as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            content,
            html,
            published: flag(&p["isPublished"]).unwrap_or(true),
            created: date(p["createdAt"].as_str()),
            updated: date(p["updatedAt"].as_str()),
        });
    }
    Ok((pages, assets))
}

/// Pages and assets of the storage files
fn read_storage(entries: Vec<(String, Vec<u8>)>, skipped: &mut Vec<String>) -> (Vec<Page>, Vec<(String, Vec<u8>)>) {
    let mut pages = vec![];
    let mut assets = vec![];
    for (path, data) in entries {
        if in_backup_folder(&path) {
            skipped.push(path);
            continue;
        }
        let ext = ext_of(&path);
        let html = match ext.as_str() {
            "md" | "markdown" => false,
            "html" | "htm" => true,
            "adoc" => {
                skipped.push(format!("{path} (AsciiDoc: import the Wiki.js export from Administration → Utilities instead)"));
                continue;
            }
            _ => {
                assets.push((path, data));
                continue;
            }
        };
        let wiki_path = path[..path.len() - ext.len() - 1].to_string();
        let text = text_of(&data);
        let page = match front_matter(&text, html) {
            Some((v, body)) => Page {
                path: wiki_path,
                title: v.get("title").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                content: body,
                html,
                // "true" / "false" with PostgreSQL, 1 / 0 with SQLite and MySQL
                published: v.get("published").map_or(true, |s| !matches!(s.trim(), "false" | "0")),
                created: date(v.get("dateCreated").map(String::as_str)),
                updated: date(v.get("date").map(String::as_str)),
            },
            None => Page { path: wiki_path, title: None, content: text, html, published: true, created: None, updated: None },
        };
        pages.push(page);
    }
    // the root of the wiki is the folder of the home page: the files may be inside a wrapper folder
    let root = pages
        .iter()
        .filter(|p| file_name(&p.path) == "home" && p.title.is_some())
        .map(|p| p.path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default())
        .min_by_key(|d| if d.is_empty() { 0 } else { d.matches('/').count() + 1 })
        .map(|d| match d.rsplit_once('/') {
            // only the home page of another locale: its parent
            Some((parent, last)) if locale_like(last) => parent.to_string(),
            None if locale_like(&d) => String::new(),
            _ => d,
        })
        .filter(|d| !d.is_empty());
    if let Some(root) = root {
        let prefix = format!("{root}/");
        for p in pages.iter_mut() {
            if let Some(rest) = p.path.strip_prefix(&prefix) {
                p.path = rest.to_string();
            }
        }
        for (path, _) in assets.iter_mut() {
            if let Some(rest) = path.strip_prefix(&prefix) {
                *path = rest.to_string();
            }
        }
    }
    (pages, assets)
}

/// `to` (a path in the archive) relative to the folder `from_dir`, escaped for a Markdown link
fn relative(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from.iter().zip(&to_parts).take_while(|(a, b)| a == b).count().min(to_parts.len() - 1);
    let mut out: Vec<&str> = vec![".."; from.len() - common];
    out.extend(&to_parts[common..]);
    out.join("/").replace('%', "%25").replace(' ', "%20").replace('(', "%28").replace(')', "%29")
}

/// Removes the Wiki.js Markdown extras and turns `{.is-info}` blockquotes into notices.
fn convert_markdown(s: &str) -> String {
    static NOTICE: OnceLock<Regex> = OnceLock::new();
    static ATTR_LINE: OnceLock<Regex> = OnceLock::new();
    static ATTR_END: OnceLock<Regex> = OnceLock::new();
    static ATTR_INLINE: OnceLock<Regex> = OnceLock::new();
    static IMSIZE: OnceLock<Regex> = OnceLock::new();
    let notice = re(&NOTICE, r"^\s*\{\.is-(info|success|warning|danger)\}\s*$");
    let attr_line = re(&ATTR_LINE, r"^\s*\{[.#][^{}]*\}\s*$");
    let attr_end = re(&ATTR_END, r"\s*\{[.#][^{}]*\}\s*$");
    let attr_inline = re(&ATTR_INLINE, r"\)\{[.#][^{}]*\}");
    let imsize = re(&IMSIZE, r#"(!\[(?:\\.|[^\]\\])*\]\([^)\s]+(?:\s+"[^"]*")?)\s+=\d*x\d*\)"#);

    let lines: Vec<&str> = s.lines().collect();
    let mut out: Vec<String> = vec![];
    let mut fence: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let marker = html_md::code_fence(line);
        if let Some(f) = &fence {
            if marker.is_some_and(|m| m.starts_with(f.as_str())) {
                fence = None;
            }
            out.push(line.to_string());
            i += 1;
            continue;
        }
        if marker.is_some() {
            fence = marker;
            out.push(line.to_string());
            i += 1;
            continue;
        }
        if line.starts_with('>') {
            let start = i;
            while i < lines.len() && lines[i].starts_with('>') {
                i += 1;
            }
            let quoted = &lines[start..i];
            if let Some(c) = lines.get(i).and_then(|l| notice.captures(l)) {
                let style = if &c[1] == "danger" { "warning" } else { &c[1] };
                out.push(format!(":::{style}"));
                for l in quoted {
                    let l = &l[1..];
                    out.push(convert_inline(l.strip_prefix(' ').unwrap_or(l), attr_end, attr_inline, imsize));
                }
                out.push(":::".into());
                i += 1;
            } else {
                out.extend(quoted.iter().map(|l| convert_inline(l, attr_end, attr_inline, imsize)));
            }
            continue;
        }
        if !attr_line.is_match(line) {
            out.push(convert_inline(line, attr_end, attr_inline, imsize));
        }
        i += 1;
    }
    out.join("\n")
}

fn convert_inline(line: &str, attr_end: &Regex, attr_inline: &Regex, imsize: &Regex) -> String {
    let l = imsize.replace_all(line, "$1)");
    let l = attr_inline.replace_all(&l, ")");
    attr_end.replace(&l, "").to_string()
}

/// Diagrams (a ```` ```diagram ```` block with the SVG in base64) and images embedded as `data:` URIs become
/// files next to the page, `<stem>-diagram-N.svg` and `<stem>-image-N.png`: kept in the text they make it too
/// large for search (a few MB of base64) and the editor can't show the diagrams. Terminal screens written as
/// HTML (a `<pre>` with coloured spans, often in an inline-block `<div>` after a `<style>` loading fonts) become
/// `<stem>-screen-N.svg` images that look like them.
fn extract_embedded(markdown: &str, archive_dir: &str, stem: &str, used: &mut HashSet<String>) -> (String, Vec<(String, Vec<u8>)>) {
    static DIAGRAM: OnceLock<Regex> = OnceLock::new();
    static DATA_IMAGE: OnceLock<Regex> = OnceLock::new();
    static SCREEN: OnceLock<Regex> = OnceLock::new();
    let diagram = re(&DIAGRAM, r"(?m)^```diagram[ \t]*\n([A-Za-z0-9+/=\s]+?)\n```[ \t]*$");
    let data_image = re(&DATA_IMAGE, r"!\[((?:\\.|[^\]\\])*)\]\(<?data:image/(png|jpe?g|gif|webp|svg\+xml);base64,([A-Za-z0-9+/=\s]+)>?\)");
    let screen = re(
        &SCREEN,
        r#"(?s)(?:<style>[^<]*</style>\s*)?(?:<div [^>]*inline-block[^>]*>\s*(<pre\s+style="[^"]*"\s*>.*?</pre>)\s*</div>|(<pre\s+style="[^"]*"\s*>.*?</pre>))"#,
    );
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let mut files = vec![];
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut file = |kind: &str, ext: &str, data: Vec<u8>| -> String {
        let n = counts.entry(kind.to_string()).or_default();
        let name = loop {
            *n += 1;
            let name = format!("{stem}-{kind}-{n}.{ext}");
            if used.insert(join_path(archive_dir, &name).to_lowercase()) {
                break name;
            }
        };
        files.push((join_path(archive_dir, &name), data));
        name
    };
    let decode = |b64: &str| STANDARD.decode(b64.chars().filter(|c| !c.is_whitespace()).collect::<String>()).ok();
    let text = diagram.replace_all(markdown, |c: &Captures| match decode(&c[1]) {
        Some(svg) => format!("![]({})", file("diagram", "svg", svg)),
        None => c[0].to_string(),
    });
    let text = data_image.replace_all(&text, |c: &Captures| match decode(&c[3]) {
        Some(img) => {
            let ext = match &c[2] {
                "jpg" | "jpeg" => "jpg",
                "svg+xml" => "svg",
                e => e,
            };
            format!("![{}]({})", &c[1], file("image", ext, img))
        }
        None => c[0].to_string(),
    });
    let text = screen.replace_all(&text, |c: &Captures| {
        let pre = c.get(1).or_else(|| c.get(2)).map_or("", |m| m.as_str());
        match terminal_svg::render(pre) {
            Some(svg) => format!("![]({})", file("screen", "svg", svg.into_bytes())),
            None => c[0].to_string(),
        }
    });
    (text.into_owned(), files)
}

/// Rewrites the Wiki.js content to the generic layout. `into_collection`: everything goes into one
/// existing collection, so top-level folders don't become collections.
pub fn convert(
    entries: Vec<(String, Vec<u8>)>,
    layout: Layout,
    into_collection: bool,
) -> ApiResult<(Vec<(String, Vec<u8>)>, HashMap<String, DocMeta>, Vec<String>)> {
    let mut skipped = vec![];
    let (pages, assets) = match layout {
        Layout::Export => read_export(entries, &mut skipped)?,
        Layout::Storage => read_storage(entries, &mut skipped),
    };

    // ---- where each page goes: `guide` + `guide/…` → folder named after the page title
    let paths: HashSet<&str> = pages.iter().map(|p| p.path.as_str()).collect();
    let mut folder_names: HashMap<String, String> = HashMap::new();
    if !into_collection {
        let folders: HashSet<&str> = pages.iter().filter_map(|p| p.path.split_once('/').map(|(f, _)| f)).collect();
        let mut used: HashSet<String> = folders.iter().map(|f| f.to_lowercase()).collect();
        for p in &pages {
            if p.path.contains('/') || !folders.contains(p.path.as_str()) {
                continue;
            }
            let Some(title) = &p.title else { continue };
            let name = title.replace('/', "-");
            if name.to_lowercase() != p.path.to_lowercase() && !used.insert(name.to_lowercase()) {
                continue;
            }
            folder_names.insert(p.path.clone(), name);
        }
    }
    let new_path = |wiki_path: &str| -> String {
        if let Some(name) = folder_names.get(wiki_path) {
            return format!("{name}/{wiki_path}.md");
        }
        match wiki_path.split_once('/') {
            Some((f, rest)) if folder_names.contains_key(f) => format!("{}/{rest}.md", folder_names[f]),
            _ => format!("{wiki_path}.md"),
        }
    };
    let archive_paths: HashMap<&str, String> = pages.iter().map(|p| (p.path.as_str(), new_path(&p.path))).collect();
    let asset_paths: HashSet<&str> = assets.iter().map(|(p, _)| p.as_str()).collect();
    // storage files may be inside a wrapper folder: a unique match on the end of the path
    let mut by_suffix: HashMap<String, Option<String>> = HashMap::new();
    for (wiki, archive) in archive_paths.iter().map(|(w, a)| (w.to_string(), a.clone())).chain(asset_paths.iter().map(|a| (a.to_string(), a.to_string()))) {
        let segs: Vec<&str> = wiki.split('/').collect();
        for i in 1..segs.len() {
            by_suffix.entry(segs[i..].join("/")).and_modify(|e| *e = None).or_insert_with(|| Some(archive.clone()));
        }
    }
    let home = paths.contains("home").then(|| archive_paths["home"].clone());

    // a link target in the wiki → its path in the archive
    let resolve = |page_dir: &str, target: &str| -> Option<String> {
        let t = target.trim();
        if t.is_empty() || t.starts_with('#') || t.contains("://") || t.starts_with("mailto:") || t.starts_with("data:") || t.starts_with("//") {
            return None;
        }
        let (path_part, anchor) = match t.split_once('#') {
            Some((p, a)) => (p, Some(a)),
            None => (t, None),
        };
        let path_part = percent_decode(path_part.split('?').next().unwrap_or(path_part));
        let wiki = if let Some(abs) = path_part.strip_prefix('/') { abs.trim_end_matches('/').to_string() } else { join_path(page_dir, &path_part) };
        let mut candidates = vec![wiki.clone()];
        if let Some((first, rest)) = wiki.split_once('/') {
            if locale_like(first) {
                candidates.push(rest.to_string());
            }
        }
        let found = candidates.iter().find_map(|c| {
            if c.is_empty() || locale_like(c) {
                return home.clone();
            }
            archive_paths
                .get(c.as_str())
                .cloned()
                .or_else(|| asset_paths.contains(c.as_str()).then(|| c.clone()))
                .or_else(|| by_suffix.get(c).cloned().flatten())
        })?;
        Some(match anchor {
            Some(a) if found.ends_with(".md") => format!("{found}#{a}"),
            _ => found,
        })
    };

    static MD_LINK: OnceLock<Regex> = OnceLock::new();
    static MD_REF: OnceLock<Regex> = OnceLock::new();
    static HTML_ATTR: OnceLock<Regex> = OnceLock::new();
    let md_link = re(&MD_LINK, r#"(!?\[(?:\\.|[^\]\\])*\]\()(<[^>]+>|[^)\s]+)"#);
    let md_ref = re(&MD_REF, r"(?m)^( {0,3}\[[^\]]+\]:\s*)(\S+)");
    let html_attr = re(&HTML_ATTR, r#"\b(src|href)="([^"]+)""#);
    static MISSING_PAGE: OnceLock<Regex> = OnceLock::new();
    let missing_page = re(&MISSING_PAGE, r#"(!?)\[((?:\\.|[^\]\\])*)\]\((/[^)\s]*)\)"#);

    let mut out = vec![];
    let mut meta = HashMap::new();
    let mut used: HashSet<String> =
        archive_paths.values().cloned().chain(asset_paths.iter().map(|a| a.to_string())).map(|p| p.to_lowercase()).collect();
    for p in &pages {
        let archive = archive_paths[p.path.as_str()].clone();
        let archive_dir = archive.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let page_dir = p.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let link = |target: &str| resolve(page_dir, target.trim_start_matches('<').trim_end_matches('>')).map(|to| relative(archive_dir, &to));
        let content = html_attr.replace_all(&p.content, |c: &Captures| match link(&c[2]) {
            Some(u) => format!("{}=\"{}\"", &c[1], u),
            None => c[0].to_string(),
        });
        let markdown = if p.html { html_md::convert(&content) } else { convert_markdown(&content) };
        let markdown = md_link.replace_all(&markdown, |c: &Captures| match link(&c[2]) {
            Some(u) => format!("{}{}", &c[1], u),
            None => c[0].to_string(),
        });
        let markdown = md_ref.replace_all(&markdown, |c: &Captures| match link(&c[2]) {
            Some(u) => format!("{}{}", &c[1], u),
            None => c[0].to_string(),
        });
        // links to pages of the wiki that are not in the archive: their text, instead of a link that leads nowhere
        let markdown = missing_page.replace_all(&markdown, |c: &Captures| {
            let target = c[3].split(['#', '?']).next().unwrap_or("");
            if c[1].is_empty() && !target.starts_with("//") && !file_name(target).contains('.') {
                c[2].to_string()
            } else {
                c[0].to_string()
            }
        });
        let stem = file_name(&archive).trim_end_matches(".md");
        let (markdown, files) = extract_embedded(&markdown, archive_dir, stem, &mut used);
        out.extend(files);
        let title = p.title.clone().unwrap_or_else(|| file_name(&p.path).to_string());
        let text = format!("---\ntitle: {}\n---\n\n{}\n", title.replace('\n', " "), markdown.trim());
        // the page that names a collection comes first in it
        let order = folder_names.contains_key(&p.path).then_some(0);
        meta.insert(archive.clone(), DocMeta { order, created_at: p.created, updated_at: p.updated, published: Some(p.published) });
        out.push((archive, text.into_bytes()));
    }
    out.extend(assets);
    Ok((out, meta, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn gz(s: &str) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        e.write_all(s.as_bytes()).unwrap();
        e.finish().unwrap()
    }

    fn doc<'a>(out: &'a [(String, Vec<u8>)], path: &str) -> String {
        String::from_utf8(out.iter().find(|(p, _)| p == path).unwrap_or_else(|| panic!("{path} missing")).1.clone()).unwrap()
    }

    #[test]
    fn export_without_commas_between_batches() {
        let json = "[\n{\"path\": \"a\", \"title\": \"A\", \"content\": \"x\"},\n{\"path\": \"b\", \"title\": \"B\", \"content\": \"}{\"}{\n\"path\": \"c\", \"title\": \"C\", \"content\": \"z\"}\n]\n";
        let pages = parse_pages(json).unwrap();
        assert_eq!(pages.iter().map(|p| p["path"].as_str().unwrap()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(pages[1]["content"], "}{");
        assert!(parse_pages("[{\"path\": \"a\"} x]").is_err());
        let entries = vec![("pages.json.gz".to_string(), gz(json))];
        let (out, _, _) = convert(entries, Layout::Export, false).unwrap();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn embedded_diagrams_and_images() {
        let svg = STANDARD.encode("<svg/>");
        let png = STANDARD.encode("png");
        let content = format!("Intro\n\n```diagram\n{svg}\n```\n\n![shot](data:image/png;base64,{png}) and ![x](data:image/jpeg;base64,{png})\n\n```diagram\n{svg}\n```\n\n```js\nlet a = 1;\n```");
        let json = serde_json::json!([{"path": "net/Radius", "title": "Radius", "content": content}]).to_string();
        let (out, _, _) = convert(vec![("pages.json.gz".to_string(), gz(&json))], Layout::Export, false).unwrap();
        let page = doc(&out, "net/Radius.md");
        assert!(page.contains("![](Radius-diagram-1.svg)") && page.contains("![](Radius-diagram-2.svg)"), "{page}");
        assert!(page.contains("![shot](Radius-image-1.png) and ![x](Radius-image-2.jpg)"), "{page}");
        assert!(page.contains("```js\nlet a = 1;\n```"), "{page}");
        assert!(!page.contains("base64"), "{page}");
        assert_eq!(doc(&out, "net/Radius-diagram-1.svg"), "<svg/>");
        assert_eq!(doc(&out, "net/Radius-image-2.jpg"), "png");
    }

    #[test]
    fn terminal_screens_become_images() {
        let content = "# Menu\n\n<style>@import url('https://fonts.googleapis.com/css2?family=Fira+Code');</style><div style=\"overflow:auto;display:inline-block;\"><pre style=\"color:#00cc00;background:#000000;\"><span style=\"display:block\"><span style=\"color:#ffffff\">10.</span> Configurazione</span></pre></div>\n\nAfter\n\n<pre style=\"background:#000\">ok</pre>";
        let json = serde_json::json!([{"path": "m", "title": "M", "content": content}]).to_string();
        let (out, _, _) = convert(vec![("pages.json.gz".to_string(), gz(&json))], Layout::Export, false).unwrap();
        let page = doc(&out, "m.md");
        assert!(page.contains("# Menu\n\n![](m-screen-1.svg)\n\nAfter\n\n![](m-screen-2.svg)"), "{page}");
        assert!(!page.contains("<style>") && !page.contains("</div>"), "{page}");
        assert!(doc(&out, "m-screen-1.svg").contains(">10.</text>"));
    }

    #[test]
    fn links_to_missing_pages_become_text() {
        let json = serde_json::json!([
            {"path": "a", "title": "A", "content": "- [B](/it/b)\n- [Missing](/it/missing#x)\n- ![img](/img/x.png)\n- [file](/f/doc.pdf)\n- [web](https://x.it/p)"},
            {"path": "b", "title": "B", "content": "b"}
        ])
        .to_string();
        let (out, _, _) = convert(vec![("pages.json.gz".to_string(), gz(&json))], Layout::Export, false).unwrap();
        let a = doc(&out, "a.md");
        assert!(a.contains("- [B](b.md)\n- Missing\n- ![img](/img/x.png)\n- [file](/f/doc.pdf)\n- [web](https://x.it/p)"), "{a}");
    }

    #[test]
    fn export_with_repeated_pages() {
        let json = r#"[
{"id": 1, "path": "a", "title": "A", "content": "old", "updatedAt": "2024-01-01T00:00:00.000Z"},
{"id": 2, "path": "b", "title": "B", "content": "b"},
{"id": 1, "path": "a", "title": "A", "content": "new", "updatedAt": "2024-02-01T00:00:00.000Z"},
{"id": 2, "path": "b", "title": "B", "content": "b"}
]"#;
        let pages = dedup_pages(parse_pages(json).unwrap());
        assert_eq!(pages.iter().map(|p| p["content"].as_str().unwrap()).collect::<Vec<_>>(), ["new", "b"]);
        let (out, _, _) = convert(vec![("pages.json.gz".to_string(), gz(json))], Layout::Export, false).unwrap();
        assert_eq!(out.len(), 2);
        assert!(doc(&out, "a.md").contains("new"));
    }

    #[test]
    fn storage_files() {
        let entries = vec![
            ("wiki/guide.md".to_string(), b"---\ntitle: User Guide\ndescription: \npublished: true\ndate: 2024-03-02T10:00:00.000Z\ntags: a, b\neditor: markdown\ndateCreated: 2023-01-05T08:30:00.000Z\n---\n\nSee [install](/guide/install#setup) and ![logo](/img/logo.png =200x).\n".to_vec()),
            ("wiki/guide/install.md".to_string(), b"---\ntitle: Install: step by step\ndescription: \npublished: 0\ndate: 2024-03-02T10:00:00.000Z\ntags: \neditor: markdown\ndateCreated: 2023-01-05T08:30:00.000Z\n---\n\n> Be careful\n> really\n{.is-danger}\n\n## Steps {#steps}\n\n- one\n{.links-list}\n\n```js\n{.is-info}\n```\n\nBack to [the guide](/en/guide) or [home](/).\n".to_vec()),
            ("wiki/home.html".to_string(), b"<!--\ntitle: Home\ndescription: \npublished: true\ndate: 2024-03-02T10:00:00.000Z\ntags: \neditor: ckeditor\ndateCreated: 2023-01-05T08:30:00.000Z\n-->\n\n<p>Read the <a href=\"/guide\">guide</a></p><blockquote class=\"is-info\"><p>Tip</p></blockquote>".to_vec()),
            ("wiki/img/logo.png".to_string(), b"png".to_vec()),
            ("wiki/_daily/wiki-01.tar.gz".to_string(), b"x".to_vec()),
        ];
        assert_eq!(detect(&entries), Some(Layout::Storage));
        let (out, meta, skipped) = convert(entries, Layout::Storage, false).unwrap();
        assert_eq!(skipped, vec!["wiki/_daily/wiki-01.tar.gz"]);
        // the top-level page names the folder and goes into it
        let guide = doc(&out, "User Guide/guide.md");
        assert!(guide.starts_with("---\ntitle: User Guide\n---\n"), "{guide}");
        assert!(guide.contains("[install](install.md#setup)"), "{guide}");
        assert!(guide.contains("![logo](../img/logo.png)"), "{guide}");
        let install = doc(&out, "User Guide/install.md");
        assert!(install.contains("title: Install: step by step"), "{install}");
        assert!(install.contains(":::warning\nBe careful\nreally\n:::"), "{install}");
        assert!(install.contains("## Steps\n"), "{install}");
        assert!(!install.contains("{.links-list}"), "{install}");
        assert!(install.contains("```js\n{.is-info}\n```"), "code blocks are kept: {install}");
        assert!(install.contains("[the guide](guide.md)"), "{install}");
        assert!(install.contains("[home](../home.md)"), "{install}");
        let home = doc(&out, "home.md");
        assert!(home.contains("[guide](User%20Guide/guide.md)"), "{home}");
        assert!(home.contains(":::info\nTip\n:::"), "{home}");
        assert_eq!(meta["User Guide/guide.md"].order, Some(0));
        assert_eq!(meta["User Guide/install.md"].order, None);
        let m = &meta["User Guide/install.md"];
        assert_eq!(m.published, Some(false));
        assert_eq!(m.created_at.unwrap().to_rfc3339(), "2023-01-05T08:30:00+00:00");
        assert_eq!(m.updated_at.unwrap().to_rfc3339(), "2024-03-02T10:00:00+00:00");
        assert!(out.iter().any(|(p, _)| p == "img/logo.png"));
    }

    #[test]
    fn export_folder() {
        let pages = r#"[
          {"path": "home", "localeCode": "en", "title": "Home", "contentType": "markdown", "content": "Go to [Docs](/docs) and [Doc FR](/fr/docs).\n\n![d](/diagrams/a%20b.png)", "isPublished": true, "createdAt": "2022-01-01T00:00:00.000Z", "updatedAt": "2022-02-01T00:00:00.000Z"},
          {"path": "docs", "localeCode": "en", "title": "Docs", "contentType": "html", "content": "<h2>Intro</h2><p>Hi</p>", "isPublished": true},
          {"path": "docs/api", "localeCode": "en", "title": "API", "contentType": "asciidoc", "content": "= API", "render": "<p>Rendered <a href=\"/en/docs\">docs</a></p>", "isPublished": true},
          {"path": "docs", "localeCode": "fr", "title": "Documentation", "contentType": "markdown", "content": "Bonjour", "isPublished": 0}
        ]"#;
        let entries = vec![
            ("export/pages.json.gz".to_string(), gz(pages)),
            ("export/assets/diagrams/a b.png".to_string(), b"png".to_vec()),
            ("export/users.json.gz".to_string(), b"x".to_vec()),
        ];
        assert_eq!(detect(&entries), Some(Layout::Export));
        let (out, meta, skipped) = convert(entries, Layout::Export, false).unwrap();
        assert_eq!(skipped, vec!["export/users.json.gz"]);
        let home = doc(&out, "home.md");
        assert!(home.contains("[Docs](Docs/docs.md)"), "{home}");
        assert!(home.contains("[Doc FR](fr/docs.md)"), "{home}");
        assert!(home.contains("![d](diagrams/a%20b.png)"), "{home}");
        assert!(doc(&out, "Docs/docs.md").contains("## Intro\n\nHi"));
        let api = doc(&out, "Docs/api.md");
        assert!(api.contains("Rendered [docs](docs.md)"), "{api}");
        assert!(doc(&out, "fr/docs.md").contains("title: Documentation"));
        assert!(out.iter().any(|(p, _)| p == "diagrams/a b.png"));
        assert_eq!(meta["home.md"].updated_at.unwrap().to_rfc3339(), "2022-02-01T00:00:00+00:00");
        assert_eq!(meta["home.md"].published, Some(true));
        // SQLite and MySQL export the flag as a number
        assert_eq!(meta["fr/docs.md"].published, Some(false));
        assert_eq!(meta.len(), 4);
    }

    #[test]
    fn into_one_collection_keeps_the_paths() {
        let entries = vec![
            ("guide.md".to_string(), b"---\ntitle: Guide\neditor: markdown\ndateCreated: 2023-01-05T08:30:00.000Z\n---\n\nx".to_vec()),
            ("guide/a.md".to_string(), b"---\ntitle: A\neditor: markdown\ndateCreated: 2023-01-05T08:30:00.000Z\n---\n\n[up](/guide)".to_vec()),
        ];
        let (out, _, _) = convert(entries, Layout::Storage, true).unwrap();
        assert!(doc(&out, "guide/a.md").contains("[up](../guide.md)"));
    }

    #[test]
    fn plain_markdown_is_not_wikijs() {
        let entries = vec![("a.md".to_string(), b"---\ntitle: A\n---\n\nx".to_vec())];
        assert_eq!(detect(&entries), None);
    }
}
