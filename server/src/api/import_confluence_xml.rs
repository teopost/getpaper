//! Confluence XML space export → the folder-of-Markdown layout understood by the generic importer.
//!
//! The XML export (Cloud and Server/Data Center) contains `exportDescriptor.properties`,
//! `entities.xml` (a Hibernate dump of every object of the space) and the attachment files as
//! `attachments/<page id>/<attachment id>/<version>`.
//!
//! Only current pages are imported (drafts, deleted pages and historical versions are skipped), with
//! their hierarchy (`parent`), sibling order (`position`) and creation / modification dates. Page
//! bodies are in storage format and converted by `html_md::convert_storage`; links to pages and
//! attachments are rewritten to the new paths. Every space becomes a collection. Attachments not
//! shown in the page content are listed at the end of the page.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDateTime, Utc};
use quick_xml::{events::Event, Reader};
use regex::{Captures, Regex};

use super::{
    import::{percent_decode, DocMeta},
    import_confluence::{safe_name, tree_paths},
};
use crate::{
    error::{ApiError, ApiResult},
    html_md,
};

/// True when the archive is a Confluence XML export.
pub fn detect(entries: &[(String, Vec<u8>)]) -> bool {
    let has = |name: &str| entries.iter().any(|(p, _)| p == name || p.ends_with(&format!("/{name}")));
    has("entities.xml") && has("exportDescriptor.properties")
}

#[derive(Debug, Clone)]
enum Prop {
    Text(String),
    Ref(String),
}

#[derive(Default)]
struct Obj {
    props: HashMap<String, Prop>,
}

impl Obj {
    fn text(&self, k: &str) -> Option<&str> {
        match self.props.get(k) {
            Some(Prop::Text(t)) if !t.is_empty() => Some(t),
            _ => None,
        }
    }
    fn reference(&self, k: &str) -> Option<&str> {
        match self.props.get(k) {
            Some(Prop::Ref(id)) if !id.is_empty() => Some(id),
            _ => None,
        }
    }
    fn date(&self, k: &str) -> Option<DateTime<Utc>> {
        // exports are written in UTC (timezoneId in exportDescriptor.properties)
        NaiveDateTime::parse_from_str(self.text(k)?, "%Y-%m-%d %H:%M:%S%.f").ok().map(|d| d.and_utc())
    }
}

const CLASSES: &[&str] = &["Space", "Page", "BodyContent", "Attachment", "ConfluenceUserImpl"];

/// Reads the objects of the classes we need: (class, id) → properties. Collections are skipped.
fn parse_entities(xml: &[u8]) -> ApiResult<Objects> {
    let bad = |e: String| ApiError::bad_request(format!("Invalid entities.xml: {e}"));
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut objs = HashMap::new();
    // state inside the current <object>
    let mut obj: Option<(String, String, Obj)> = None;
    let mut depth = 0usize; // element depth inside the object
    let mut in_id = false; // reading the object's own <id>
    let mut prop: Option<(String, bool, String)> = None; // (name, is reference, value)
    let mut skip_depth: Option<usize> = None; // inside a <collection>
    loop {
        let ev = reader.read_event_into(&mut buf).map_err(|e| bad(e.to_string()))?;
        match ev {
            Event::Start(e) => {
                let name = e.name().as_ref().to_vec();
                if obj.is_none() {
                    if name == b"object" {
                        let class = e
                            .try_get_attribute("class")
                            .map_err(|e| bad(e.to_string()))?
                            .map(|a| String::from_utf8_lossy(&a.value).to_string())
                            .unwrap_or_default();
                        if CLASSES.contains(&class.as_str()) {
                            obj = Some((class, String::new(), Obj::default()));
                            depth = 0;
                        } else {
                            reader.read_to_end_into(e.name(), &mut Vec::new()).map_err(|e| bad(e.to_string()))?;
                        }
                    }
                } else {
                    depth += 1;
                    if skip_depth.is_some() {
                        // inside a collection
                    } else if depth == 1 && name == b"id" {
                        in_id = true;
                    } else if depth == 1 && name == b"property" {
                        let pname = e
                            .try_get_attribute("name")
                            .map_err(|e| bad(e.to_string()))?
                            .map(|a| String::from_utf8_lossy(&a.value).to_string())
                            .unwrap_or_default();
                        prop = Some((pname, false, String::new()));
                    } else if depth == 2 && name == b"id" {
                        if let Some(p) = prop.as_mut() {
                            p.1 = true;
                        }
                    } else if depth == 1 && name == b"collection" {
                        skip_depth = Some(depth);
                    }
                }
            }
            Event::End(_) if obj.is_some() => {
                if depth == 0 {
                    let (class, id, o) = obj.take().unwrap();
                    objs.insert((class, id), o);
                    continue;
                }
                if skip_depth == Some(depth) {
                    skip_depth = None;
                } else if skip_depth.is_none() && depth == 1 {
                    if in_id {
                        in_id = false;
                    } else if let Some((name, is_ref, value)) = prop.take() {
                        let value = value.trim().to_string();
                        let p = if is_ref { Prop::Ref(value) } else { Prop::Text(value) };
                        obj.as_mut().unwrap().2.props.insert(name, p);
                    }
                }
                depth -= 1;
            }
            Event::Text(t) if obj.is_some() && skip_depth.is_none() => {
                let s = t.unescape().map_err(|e| bad(e.to_string()))?;
                if in_id {
                    obj.as_mut().unwrap().1.push_str(s.trim());
                } else if let Some(p) = prop.as_mut() {
                    p.2.push_str(&s);
                }
            }
            Event::CData(c) if obj.is_some() && skip_depth.is_none() => {
                let s = String::from_utf8_lossy(&c);
                if in_id {
                    obj.as_mut().unwrap().1.push_str(s.trim());
                } else if let Some(p) = prop.as_mut() {
                    p.2.push_str(&s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(objs)
}

type Objects = HashMap<(String, String), Obj>;

fn objects_of<'a>(objs: &'a Objects, class: &'a str) -> impl Iterator<Item = (&'a str, &'a Obj)> + 'a {
    objs.iter().filter(move |((c, _), _)| c == class).map(|((_, id), o)| (id.as_str(), o))
}

/// "@name" for a user: the part of the account name before "@", nothing for opaque account ids.
fn user_label(name: &str) -> Option<String> {
    let n = name.split('@').next().unwrap_or(name).trim();
    if n.is_empty() || n.contains(':') || (n.len() >= 20 && n.chars().all(|c| c.is_ascii_hexdigit())) {
        None
    } else {
        Some(format!("@{n}"))
    }
}

struct Page {
    id: String,
    title: String,
    parent: Option<String>,
    position: Option<i64>,
    space: String,
    created: Option<DateTime<Utc>>,
    updated: Option<DateTime<Utc>>,
    body: String,
    new_path: String,
}

pub fn convert(entries: Vec<(String, Vec<u8>)>) -> ApiResult<(Vec<(String, Vec<u8>)>, HashMap<String, DocMeta>)> {
    // the export may sit inside a wrapper folder
    let root = entries
        .iter()
        .find(|(p, _)| p == "entities.xml" || p.ends_with("/entities.xml"))
        .map(|(p, _)| p.trim_end_matches("entities.xml").to_string())
        .unwrap_or_default();
    let mut files: HashMap<String, Vec<u8>> =
        entries.into_iter().filter_map(|(p, d)| p.strip_prefix(&root).map(|r| (r.to_string(), d))).collect();
    let xml = files.remove("entities.xml").unwrap_or_default();
    let objs = parse_entities(&xml)?;
    drop(xml);

    let of = |class| objects_of(&objs, class);

    // ---- spaces, users
    let spaces: HashMap<&str, (String, String)> = of("Space")
        .map(|(id, o)| (id, (o.text("name").unwrap_or("Confluence").to_string(), o.text("key").unwrap_or("").to_string())))
        .collect();
    let default_space = spaces.keys().min().map(|s| s.to_string()).unwrap_or_default();
    let mut users: HashMap<String, String> = HashMap::new();
    for (id, o) in of("ConfluenceUserImpl") {
        if let Some(label) = o.text("name").and_then(user_label) {
            users.insert(id.to_string(), label.clone());
            if let Some(acc) = o.text("atlassianAccountId") {
                users.insert(acc.to_string(), label);
            }
        }
    }

    // ---- current pages and their storage format bodies
    let mut bodies: HashMap<&str, &str> = HashMap::new();
    for (_, o) in of("BodyContent") {
        if let (Some(page), Some("2")) = (o.reference("content"), o.text("bodyType")) {
            bodies.insert(page, o.text("body").unwrap_or(""));
        }
    }
    let mut pages: Vec<Page> = of("Page")
        .filter(|(_, o)| o.text("contentStatus") == Some("current") && o.reference("originalVersion").is_none())
        .map(|(id, o)| Page {
            id: id.to_string(),
            title: o.text("title").unwrap_or("Untitled").to_string(),
            parent: o.reference("parent").map(|s| s.to_string()),
            position: o.text("position").and_then(|p| p.parse().ok()),
            space: o.reference("space").map(|s| s.to_string()).unwrap_or_default(),
            created: o.date("creationDate"),
            updated: o.date("lastModificationDate"),
            body: bodies.get(id).map(|b| b.to_string()).unwrap_or_default(),
            new_path: String::new(),
        })
        .collect();
    pages.sort_by(|a, b| a.id.cmp(&b.id));
    let by_id: HashMap<String, usize> = pages.iter().enumerate().map(|(i, p)| (p.id.clone(), i)).collect();
    // pages without a space reference inherit it from their ancestors
    for i in 0..pages.len() {
        if pages[i].space.is_empty() {
            let mut cur = pages[i].parent.clone();
            let mut hops = 0;
            while let Some(p) = cur.and_then(|p| by_id.get(&p).copied()) {
                if !pages[p].space.is_empty() || hops > 100 {
                    pages[i].space = pages[p].space.clone();
                    break;
                }
                cur = pages[p].parent.clone();
                hops += 1;
            }
            if pages[i].space.is_empty() {
                pages[i].space = default_space.clone();
            }
        }
    }

    // ---- attachments: current versions of page attachments
    struct Att {
        page: String,
        title: String,
        file: String,
    }
    let mut atts: Vec<Att> = vec![];
    for (id, o) in of("Attachment") {
        if o.text("contentStatus") != Some("current") || o.reference("originalVersion").is_some() {
            continue;
        }
        let Some(page) = o.reference("containerContent").filter(|p| by_id.contains_key(*p)) else { continue };
        let version = o.text("version").unwrap_or("1");
        let mut file = format!("attachments/{page}/{id}/{version}");
        if !files.contains_key(&file) {
            // fall back to the highest version present
            let prefix = format!("attachments/{page}/{id}/");
            match files.keys().filter(|k| k.starts_with(&prefix)).max_by_key(|k| k[prefix.len()..].parse::<u32>().unwrap_or(0)) {
                Some(k) => file = k.clone(),
                None => continue,
            }
        }
        atts.push(Att { page: page.to_string(), title: o.text("title").unwrap_or("file").to_string(), file });
    }
    atts.sort_by(|a, b| (&a.page, &a.title).cmp(&(&b.page, &b.title)));

    // ---- new paths, per space
    let mut meta: HashMap<String, DocMeta> = HashMap::new();
    let mut used_dirs: HashSet<String> = HashSet::new();
    let mut space_dirs: HashMap<String, String> = HashMap::new();
    let mut space_ids: Vec<String> = pages.iter().map(|p| p.space.clone()).collect();
    space_ids.sort();
    space_ids.dedup();
    for sid in &space_ids {
        let name = spaces.get(sid.as_str()).map(|s| s.0.clone()).unwrap_or_else(|| "Confluence".into());
        let mut dir = safe_name(&name);
        while !used_dirs.insert(dir.to_lowercase()) {
            dir.push('_');
        }
        space_dirs.insert(sid.clone(), dir.clone());
        let members: Vec<usize> = (0..pages.len()).filter(|&i| &pages[i].space == sid).collect();
        let local: HashMap<&str, usize> = members.iter().enumerate().map(|(k, &i)| (pages[i].id.as_str(), k)).collect();
        let titles: Vec<String> = members.iter().map(|&i| pages[i].title.clone()).collect();
        let parents: Vec<Option<usize>> =
            members.iter().map(|&i| pages[i].parent.as_deref().and_then(|p| local.get(p).copied())).collect();
        let positions: Vec<Option<i64>> = members.iter().map(|&i| pages[i].position).collect();
        for (k, path) in tree_paths(&dir, &titles, &parents, &positions).into_iter().enumerate() {
            let p = &mut pages[members[k]];
            meta.insert(path.clone(), DocMeta { order: p.position, created_at: p.created, updated_at: p.updated, ..Default::default() });
            p.new_path = path;
        }
    }
    // (space key lowercased, title) → page index, for links by title
    let mut by_title: HashMap<(String, String), usize> = HashMap::new();
    for (i, p) in pages.iter().enumerate() {
        let key = spaces.get(p.space.as_str()).map(|s| s.1.to_lowercase()).unwrap_or_default();
        by_title.entry((key, p.title.clone())).or_insert(i);
    }
    let att_path = |a: &Att| {
        let dir = by_id.get(&a.page).map(|&i| space_dirs[&pages[i].space].clone()).unwrap_or_default();
        format!("{dir}/attachments/{}/{}", a.page, safe_name(&a.title))
    };
    let mut atts_by_page: HashMap<&str, Vec<usize>> = HashMap::new();
    for (k, a) in atts.iter().enumerate() {
        atts_by_page.entry(a.page.as_str()).or_default().push(k);
    }

    // ---- convert pages
    let user_re = Regex::new(r#"<ri:user\s+[^>]*?ri:(?:userkey|account-id)="([^"]+)"[^>]*?/?>(?:\s*</ri:user>)?"#).unwrap();
    let md_link = Regex::new(r#"(!?)\[((?:\\.|[^\]\\])*)\]\((cfl-(?:att|page):[^)\s]*)\)"#).unwrap();
    let mut out: Vec<(String, Vec<u8>)> = vec![];
    let mut used_atts: HashSet<usize> = HashSet::new();
    for p in &pages {
        let space_key = spaces.get(p.space.as_str()).map(|s| s.1.to_lowercase()).unwrap_or_default();
        // bodies are stored in a CDATA section, so the export writes the inner CDATA terminators as "]] >"
        let body = p.body.replace("]] >", "]]>");
        let body = user_re.replace_all(&body, |c: &Captures| users.get(&c[1]).cloned().unwrap_or_else(|| "@user".into()));
        let md = html_md::convert_storage(&body);
        let up = "../".repeat(p.new_path.matches('/').count());
        let mut shown: HashSet<usize> = HashSet::new();
        let md = md_link.replace_all(&md, |c: &Captures| {
            let (bang, label, url) = (&c[1], &c[2], &c[3]);
            let target = if let Some(rest) = url.strip_prefix("cfl-att:") {
                let (page_title, file) = rest.split_once('/').unwrap_or(("", rest));
                let (page_title, file) = (percent_decode(page_title), percent_decode(file));
                let page = if page_title.is_empty() {
                    Some(p.id.as_str())
                } else {
                    by_title.get(&(space_key.clone(), page_title)).map(|&i| pages[i].id.as_str())
                };
                page.and_then(|pid| atts_by_page.get(pid))
                    .and_then(|list| list.iter().find(|&&k| atts[k].title == file))
                    .map(|&k| {
                        shown.insert(k);
                        format!("{up}{}", att_path(&atts[k]))
                    })
            } else {
                let rest = url.strip_prefix("cfl-page:").unwrap_or("");
                let (rest, anchor) = rest.split_once('#').map(|(a, b)| (a, Some(b))).unwrap_or((rest, None));
                let (space, title) = rest.split_once('/').unwrap_or(("", rest));
                let space = if space.is_empty() { space_key.clone() } else { percent_decode(space).to_lowercase() };
                by_title.get(&(space, percent_decode(title))).map(|&i| {
                    let a = anchor.map(|a| format!("#{a}")).unwrap_or_default();
                    format!("{up}{}{a}", pages[i].new_path)
                })
            };
            match target {
                Some(t) if t.contains(' ') => format!("{bang}[{label}](<{t}>)"),
                Some(t) => format!("{bang}[{label}]({t})"),
                // pages of other spaces, missing attachments: keep the text
                None if bang.is_empty() => label.to_string(),
                None => String::new(),
            }
        });
        let mut body = md.trim().to_string();
        let hidden: Vec<usize> = atts_by_page
            .get(p.id.as_str())
            .map(|list| list.iter().copied().filter(|k| !shown.contains(k)).collect())
            .unwrap_or_default();
        let extra: Vec<String> = hidden
            .iter()
            .map(|&k| format!("- [{}](<{up}{}>)", atts[k].title.replace(['[', ']'], ""), att_path(&atts[k])))
            .collect();
        shown.extend(hidden);
        if !extra.is_empty() {
            body.push_str("\n\n## Attachments\n\n");
            body.push_str(&extra.join("\n"));
        }
        used_atts.extend(shown);
        out.push((p.new_path.clone(), format!("# {}\n\n{}\n", p.title, body).into_bytes()));
    }

    // ---- attachment files under their new names
    for k in used_atts {
        let a = &atts[k];
        if let Some(data) = files.remove(&a.file) {
            out.push((att_path(a), data));
        }
    }
    Ok((out, meta))
}
