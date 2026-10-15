//! Confluence HTML space export → the folder-of-Markdown layout understood by the generic importer.
//!
//! A space export (Space tools → Content tools → Export → HTML, Cloud or Server/Data Center) is a
//! folder per space with one HTML file per page (`Page-Title_123456.html` or `123456.html`),
//! `index.html` (space overview with the page tree), `attachments/<pageId>/…` and theme files
//! (`styles/`, `images/icons/`).
//!
//! Every space becomes a collection. The page hierarchy comes from each page's breadcrumbs and
//! sibling order from the page tree in `index.html`. Pages are converted to Markdown and written as
//! `<Space>/<Parent>/<Page>.md`; links between pages and to attachments are rewritten to point at
//! the new paths, so the generic importer turns them into internal links. Theme files and
//! attachments no page refers to are dropped.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use regex::{Captures, Regex};
use scraper::{Html, Selector};

use super::import::DocMeta;
use crate::html_md;

fn sel(cell: &'static OnceLock<Selector>, s: &str) -> &'static Selector {
    cell.get_or_init(|| Selector::parse(s).unwrap())
}

fn is_html(p: &str) -> bool {
    let l = p.to_lowercase();
    l.ends_with(".html") || l.ends_with(".htm")
}

fn is_page(data: &[u8]) -> bool {
    let s = String::from_utf8_lossy(&data[..data.len().min(64 * 1024)]);
    s.contains("id=\"main-content\"") && (s.contains("wiki-content") || s.contains("id=\"breadcrumbs\""))
}

/// True when the archive looks like a Confluence HTML export.
pub fn detect(entries: &[(String, Vec<u8>)]) -> bool {
    entries.iter().filter(|(p, _)| is_html(p)).take(50).any(|(_, d)| is_page(d))
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn file_of(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// File and folder names can't contain path separators and some characters break links.
pub(super) fn safe_name(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '%') { '-' } else { c })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(120).collect();
    let s = s.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if s.is_empty() {
        "Untitled".into()
    } else {
        s
    }
}

/// `<space>/<ancestors…>/<title>.md` for every page, with unique names among siblings.
pub(super) fn tree_paths(space_dir: &str, titles: &[String], parents: &[Option<usize>], positions: &[Option<i64>]) -> Vec<String> {
    let mut seg_of: Vec<String> = vec![String::new(); titles.len()];
    let mut sibling_names: HashMap<Option<usize>, HashSet<String>> = HashMap::new();
    let mut idx: Vec<usize> = (0..titles.len()).collect();
    idx.sort_by_key(|&i| (positions[i].unwrap_or(i64::MAX), titles[i].clone()));
    for &i in &idx {
        let names = sibling_names.entry(parents[i]).or_default();
        let base = safe_name(&titles[i]);
        let mut name = base.clone();
        let mut n = 2;
        while !names.insert(name.to_lowercase()) {
            name = format!("{base} ({n})");
            n += 1;
        }
        seg_of[i] = name;
    }
    (0..titles.len())
        .map(|i| {
            let mut segs = vec![seg_of[i].clone()];
            let mut seen = HashSet::from([i]);
            let mut cur = parents[i];
            while let Some(j) = cur {
                if !seen.insert(j) {
                    break;
                }
                segs.push(seg_of[j].clone());
                cur = parents[j];
            }
            segs.push(space_dir.to_string());
            segs.reverse();
            format!("{}.md", segs.join("/"))
        })
        .collect()
}

fn text_of(doc: &Html, s: &Selector) -> Option<String> {
    doc.select(s)
        .next()
        .map(|e| e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
}

struct Page {
    /// original path in the archive
    path: String,
    title: String,
    /// file name of the parent page (same folder), from the breadcrumbs
    parent: Option<String>,
    markdown: String,
    /// attachment links from the page's "Attachments" section: (label, href)
    attachments: Vec<(String, String)>,
    /// new path, e.g. "Space/Parent/Page.md"
    new_path: String,
}

/// Rewrites the archive. Returns the new entries and the sibling order of the generated documents
/// (position in the space's page tree, keyed by new path).
pub fn convert(entries: Vec<(String, Vec<u8>)>) -> (Vec<(String, Vec<u8>)>, HashMap<String, DocMeta>) {
    static TITLE_TEXT: OnceLock<Selector> = OnceLock::new();
    static TITLE: OnceLock<Selector> = OnceLock::new();
    static CRUMBS: OnceLock<Selector> = OnceLock::new();
    static MAIN: OnceLock<Selector> = OnceLock::new();
    static SECTION_LINKS: OnceLock<Selector> = OnceLock::new();
    static LINKS: OnceLock<Selector> = OnceLock::new();

    // ---- classify
    let mut pages_by_dir: HashMap<String, Vec<(String, Vec<u8>)>> = HashMap::new();
    let mut indexes: HashMap<String, Vec<u8>> = HashMap::new();
    let mut others: HashMap<String, Vec<u8>> = HashMap::new();
    let mut passthrough = vec![];
    for (path, data) in entries {
        if is_html(&path) && file_of(&path).eq_ignore_ascii_case("index.html") {
            indexes.insert(dir_of(&path).to_string(), data);
        } else if is_html(&path) && is_page(&data) {
            pages_by_dir.entry(dir_of(&path).to_string()).or_default().push((path, data));
        } else if is_html(&path) || path.to_lowercase().ends_with(".css") || path.contains("images/icons/") {
            // theme files and the export's other HTML (e.g. user profiles)
        } else if path.to_lowercase().ends_with(".md") || path.to_lowercase().ends_with(".txt") {
            passthrough.push((path, data));
        } else {
            others.insert(path, data);
        }
    }

    let mut out: Vec<(String, Vec<u8>)> = passthrough;
    let mut order: HashMap<String, DocMeta> = HashMap::new();
    let mut used_attachments: HashSet<String> = HashSet::new();
    let mut used_space_names: HashSet<String> = HashSet::new();
    let mut new_attachment_paths: HashMap<String, String> = HashMap::new();
    let mut converted: Vec<Page> = vec![];

    let mut dirs: Vec<String> = pages_by_dir.keys().cloned().collect();
    dirs.sort();
    for dir in dirs {
        let raw_pages = pages_by_dir.remove(&dir).unwrap_or_default();
        let page_files: HashSet<String> = raw_pages.iter().map(|(p, _)| file_of(p).to_string()).collect();
        let index_doc = indexes.get(&dir).map(|d| Html::parse_document(&String::from_utf8_lossy(d)));

        // ---- space name: breadcrumb root, else the index title, else the folder name
        let mut space_name: Option<String> = None;
        let mut pages: Vec<Page> = vec![];
        for (path, data) in &raw_pages {
            let html = String::from_utf8_lossy(data);
            let doc = Html::parse_document(&html);
            let crumbs: Vec<(String, String)> = doc
                .select(sel(&CRUMBS, "#breadcrumbs a[href]"))
                .map(|a| {
                    let href = a.value().attr("href").unwrap_or("").split('#').next().unwrap_or("").to_string();
                    (href, a.text().collect::<String>().trim().to_string())
                })
                .collect();
            if space_name.is_none() {
                space_name = crumbs.first().map(|c| c.1.clone()).filter(|n| !n.is_empty());
            }
            let me = file_of(path);
            let parent = crumbs.iter().rev().map(|c| file_of(&c.0).to_string()).find(|f| f != me && page_files.contains(f));
            let full_title = text_of(&doc, sel(&TITLE_TEXT, "#title-text"))
                .or_else(|| text_of(&doc, sel(&TITLE, "title")))
                .unwrap_or_else(|| me.trim_end_matches(".html").to_string());
            let markdown = doc.select(sel(&MAIN, "#main-content")).next().map(html_md::convert_element).unwrap_or_default();
            let attachments = doc
                .select(sel(&SECTION_LINKS, ".pageSection a[href]"))
                .filter_map(|a| {
                    let href = a.value().attr("href")?;
                    href.starts_with("attachments/")
                        .then(|| (a.text().collect::<String>().trim().to_string(), href.to_string()))
                })
                .collect();
            pages.push(Page {
                path: path.clone(),
                title: full_title,
                parent,
                markdown,
                attachments,
                new_path: String::new(),
            });
        }
        let index_title = index_doc.as_ref().and_then(|d| text_of(d, sel(&TITLE, "title")));
        let space = space_name
            .or(index_title)
            .unwrap_or_else(|| if dir.is_empty() { "Confluence".into() } else { file_of(&dir).to_string() });
        // titles are "Space : Page"
        let prefix = format!("{space} : ");
        for p in pages.iter_mut() {
            if let Some(t) = p.title.strip_prefix(&prefix) {
                p.title = t.trim().to_string();
            }
        }
        let mut space_dir = safe_name(&space);
        while !used_space_names.insert(space_dir.to_lowercase()) {
            space_dir.push('_');
        }

        // ---- sibling order from the index page tree
        let mut tree_pos: HashMap<String, usize> = HashMap::new();
        if let Some(d) = &index_doc {
            for a in d.select(sel(&LINKS, "a[href]")) {
                let f = file_of(a.value().attr("href").unwrap_or("")).to_string();
                if page_files.contains(&f) && !tree_pos.contains_key(&f) {
                    tree_pos.insert(f, tree_pos.len());
                }
            }
        }

        // ---- new paths following the hierarchy
        let by_file: HashMap<String, usize> = pages.iter().enumerate().map(|(i, p)| (file_of(&p.path).to_string(), i)).collect();
        let titles: Vec<String> = pages.iter().map(|p| p.title.clone()).collect();
        let parents: Vec<Option<usize>> = pages.iter().map(|p| p.parent.as_ref().and_then(|f| by_file.get(f).copied())).collect();
        let positions: Vec<Option<i64>> = pages.iter().map(|p| tree_pos.get(file_of(&p.path)).map(|&i| i as i64)).collect();
        for (i, path) in tree_paths(&space_dir, &titles, &parents, &positions).into_iter().enumerate() {
            order.insert(path.clone(), DocMeta { order: positions[i], ..Default::default() });
            pages[i].new_path = path;
        }

        // attachments of this space keep their relative path under the space folder
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        for p in others.keys() {
            if let Some(rel) = p.strip_prefix(&prefix) {
                new_attachment_paths.entry(p.clone()).or_insert_with(|| format!("{space_dir}/{rel}"));
            }
        }
        converted.extend(pages);
    }

    // ---- rewrite links, now that every page has its new path
    let page_paths: HashMap<String, String> = converted.iter().map(|p| (p.path.clone(), p.new_path.clone())).collect();
    static MD_LINK: OnceLock<Regex> = OnceLock::new();
    let md_link = MD_LINK.get_or_init(|| Regex::new(r#"(!?\[(?:\\.|[^\]\\])*\]\()(<[^>]+>|[^)\s]+)(\))"#).unwrap());
    for p in &converted {
        let old_dir = dir_of(&p.path);
        let depth = p.new_path.matches('/').count();
        let up = "../".repeat(depth);
        let rewrite = |target: &str, used: &mut HashSet<String>| -> Option<String> {
            let t = target.trim_start_matches('<').trim_end_matches('>');
            if t.contains("://") || t.starts_with('/') || t.starts_with('#') || t.starts_with("mailto:") {
                return None;
            }
            let (path, anchor) = match t.split_once('#') {
                Some((a, b)) => (a, format!("#{b}")),
                None => (t, String::new()),
            };
            let path = path.split('?').next().unwrap_or(path);
            let full = if old_dir.is_empty() { path.to_string() } else { format!("{old_dir}/{path}") };
            let full = super::import::join_path("", &super::import::percent_decode(&full));
            if let Some(np) = page_paths.get(&full) {
                return Some(format!("{up}{np}{anchor}"));
            }
            if let Some(np) = new_attachment_paths.get(&full) {
                used.insert(full);
                return Some(format!("{up}{np}"));
            }
            None
        };
        let mut used = HashSet::new();
        let mut body = md_link
            .replace_all(&p.markdown, |c: &Captures| match rewrite(&c[2], &mut used) {
                Some(u) if u.contains(' ') => format!("{}<{}>{}", &c[1], u, &c[3]),
                Some(u) => format!("{}{}{}", &c[1], u, &c[3]),
                None => c[0].to_string(),
            })
            .to_string();
        // files listed in the page's "Attachments" section but not shown in the content
        let mut extra = vec![];
        for (label, href) in &p.attachments {
            let full = super::import::join_path("", &super::import::percent_decode(&format!("{old_dir}/{href}")));
            if used.contains(&full) {
                continue;
            }
            if let Some(u) = rewrite(href, &mut used) {
                let label = if label.is_empty() { file_of(href).to_string() } else { label.replace(['[', ']'], "") };
                let target = if u.contains(' ') { format!("<{u}>") } else { u };
                extra.push(format!("- [{label}]({target})"));
            }
        }
        if !extra.is_empty() {
            body.push_str("\n\n## Attachments\n\n");
            body.push_str(&extra.join("\n"));
        }
        used_attachments.extend(used);
        out.push((p.new_path.clone(), format!("# {}\n\n{}\n", p.title, body.trim()).into_bytes()));
    }

    // ---- only attachments some page refers to (drops theme images and unused files)
    for (path, data) in others {
        if used_attachments.contains(&path) {
            if let Some(np) = new_attachment_paths.get(&path) {
                out.push((np.clone(), data));
            }
        }
    }
    (out, order)
}
