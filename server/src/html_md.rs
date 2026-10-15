//! HTML → Markdown conversion for imports (generic HTML files and Confluence exports).
//!
//! Produces the Markdown dialect the editor reads: GFM tables and task lists, fenced code with
//! language, `:::info` / `:::warning` / `:::tip` / `:::success` notices. Confluence macros found in
//! HTML space exports (information panels, code panels, expand, status, emoticons, mentions) are
//! mapped to the closest equivalent; layout wrappers are flattened and navigation chrome is dropped.
//!
//! `convert_storage` reads Confluence *storage format* (page bodies in XML space exports): `ac:` macros
//! and `ri:` resource identifiers. Links to other pages and to attachments are emitted with the
//! placeholder URLs `cfl-page:<space>/<title>#<anchor>` and `cfl-att:<page title>/<file name>`
//! (each part percent-encoded, empty page title = the current page), resolved by the caller.

use std::sync::OnceLock;

use regex::{Captures, Regex};
use scraper::{ElementRef, Html, Node};

/// Converts an HTML document or fragment. Only the `<body>` is used when there is one.
pub fn convert(html: &str) -> String {
    let doc = Html::parse_document(html);
    let root = doc
        .root_element()
        .children()
        .filter_map(ElementRef::wrap)
        .find(|e| e.value().name() == "body")
        .unwrap_or_else(|| doc.root_element());
    convert_element(root)
}

/// The fence (``` or ~~~, any length) when `line` opens or closes a fenced code block, also inside
/// list items ("1. ```bash") and quotes.
pub fn code_fence(line: &str) -> Option<String> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let item = ITEM.get_or_init(|| Regex::new(r"^\s*(?:(?:[-*+]|\d+[.)])\s+|>\s?)*").unwrap());
    let rest = &line[item.find(line).map(|m| m.end()).unwrap_or(0)..];
    let fence: String = rest.chars().take_while(|c| *c == '`' || *c == '~').collect();
    (fence.len() >= 3 && fence.chars().all(|c| c == fence.chars().next().unwrap())).then_some(fence)
}

/// Converts a Confluence storage format body.
pub fn convert_storage(xhtml: &str) -> String {
    static CDATA: OnceLock<Regex> = OnceLock::new();
    static SELF_CLOSING: OnceLock<Regex> = OnceLock::new();
    // CDATA (code bodies) would be dropped by the HTML parser: turn it into escaped text
    let s = CDATA
        .get_or_init(|| Regex::new(r"(?s)<!\[CDATA\[(.*?)\]\]>").unwrap())
        .replace_all(xhtml, |c: &Captures| c[1].replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"));
    // XML self-closing tags (<ri:page … />, <p />) are not self-closing in HTML
    let s = SELF_CLOSING
        .get_or_init(|| Regex::new(r"<([A-Za-z][\w:-]*)(\s[^<>]*?)?\s*/>").unwrap())
        .replace_all(&s, |c: &Captures| {
            let tag = &c[1];
            let attrs = c.get(2).map(|m| m.as_str()).unwrap_or("");
            if VOID_TAGS.contains(&tag.to_lowercase().as_str()) {
                format!("<{tag}{attrs}>")
            } else {
                format!("<{tag}{attrs}></{tag}>")
            }
        });
    convert(&format!("<html><body>{s}</body></html>"))
}

const VOID_TAGS: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// Converts the content of an element (the element's own tag is not rendered).
pub fn convert_element(el: ElementRef) -> String {
    let out = join_blocks(&blocks(el));
    tidy(&out)
}

fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// element classification
// ---------------------------------------------------------------------------

const BLOCK_TAGS: &[&str] = &[
    "address", "article", "aside", "blockquote", "center", "dd", "details", "dir", "div", "dl", "dt", "fieldset",
    "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "li", "main", "menu",
    "nav", "ol", "p", "pre", "section", "summary", "table", "ul",
];

const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "head", "title", "meta", "link", "iframe", "object", "embed", "svg",
    "button", "input", "select", "textarea",
];

/// Confluence chrome and macros without a useful Markdown equivalent
const SKIP_CLASSES: &[&str] = &["toc-macro", "client-side-toc-macro", "hidden", "recently-updated", "plugin_pagetree"];

fn has_class(el: ElementRef, class: &str) -> bool {
    el.value().classes().any(|c| c == class)
}

fn is_block(el: ElementRef) -> bool {
    BLOCK_TAGS.contains(&el.value().name())
}

fn skip(el: ElementRef) -> bool {
    SKIP_TAGS.contains(&el.value().name())
        || SKIP_CLASSES.iter().any(|c| has_class(el, c))
        || el.value().attr("style").is_some_and(|s| s.replace(' ', "").contains("display:none"))
}

/// Text of a macro title, which is rendered in bold by the caller.
fn plain_text(el: ElementRef) -> String {
    escape_text(collapse(&el.text().collect::<String>()).trim())
}

fn child_with_class<'a>(el: ElementRef<'a>, class: &str) -> Option<ElementRef<'a>> {
    el.descendants().filter_map(ElementRef::wrap).find(|e| has_class(*e, class))
}

// ---------------------------------------------------------------------------
// blocks
// ---------------------------------------------------------------------------

/// Renders the children of `el` as a list of Markdown blocks.
fn blocks(el: ElementRef) -> Vec<String> {
    let mut out = vec![];
    let mut inline_buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<String>| {
        let p = buf.trim();
        if !p.is_empty() && p != "\\" {
            out.push(p.trim_end_matches('\\').trim_end().to_string());
        }
        buf.clear();
    };
    for child in el.children() {
        match child.value() {
            Node::Text(t) => inline_buf.push_str(&escape_text(&collapse(t))),
            Node::Element(_) => {
                let c = ElementRef::wrap(child).unwrap();
                if skip(c) {
                    continue;
                }
                if is_block(c) || is_block_macro(c) {
                    flush(&mut inline_buf, &mut out);
                    out.extend(block(c));
                } else {
                    inline_buf.push_str(&inline(c));
                }
            }
            _ => {}
        }
    }
    flush(&mut inline_buf, &mut out);
    out
}

fn is_block_macro(el: ElementRef) -> bool {
    has_class(el, "confluence-information-macro")
        || has_class(el, "expand-container")
        || (el.value().name() == "div" && has_class(el, "code"))
        || is_storage_block(el)
}

fn join_blocks(blocks: &[String]) -> String {
    blocks.iter().filter(|b| !b.trim().is_empty()).cloned().collect::<Vec<_>>().join("\n\n")
}

fn block(el: ElementRef) -> Vec<String> {
    let name = el.value().name();
    if is_storage(el) {
        return storage_block(el);
    }

    // ---- Confluence macros
    if has_class(el, "confluence-information-macro") {
        let style = if has_class(el, "confluence-information-macro-tip") {
            "tip"
        } else if has_class(el, "confluence-information-macro-note") || has_class(el, "confluence-information-macro-warning") {
            "warning"
        } else {
            "info"
        };
        let title = el
            .children()
            .filter_map(ElementRef::wrap)
            .find(|c| has_class(*c, "title"))
            .map(plain_text)
            .filter(|t| !t.is_empty());
        let body = child_with_class(el, "confluence-information-macro-body").unwrap_or(el);
        let mut parts = vec![];
        if let Some(t) = title {
            parts.push(format!("**{t}**"));
        }
        parts.extend(blocks(body));
        return vec![format!(":::{style}\n{}\n:::", join_blocks(&parts))];
    }
    if has_class(el, "expand-container") {
        let title = child_with_class(el, "expand-control-text").map(plain_text);
        let body = child_with_class(el, "expand-content").unwrap_or(el);
        let mut parts = vec![];
        if let Some(t) = title.filter(|t| !t.is_empty()) {
            parts.push(format!("**{t}**"));
        }
        parts.extend(blocks(body));
        return parts;
    }
    if has_class(el, "code") || has_class(el, "preformatted") {
        if let Some(pre) = el.descendants().filter_map(ElementRef::wrap).find(|e| e.value().name() == "pre") {
            let title = child_with_class(el, "codeHeader").map(plain_text);
            let mut out = vec![];
            if let Some(t) = title.filter(|t| !t.is_empty()) {
                out.push(format!("**{t}**"));
            }
            out.push(code_block(pre));
            return out;
        }
    }

    match name {
        "p" | "dt" | "summary" | "figcaption" => {
            // a <p> can (invalidly) contain block content in some exports
            if el.children().filter_map(ElementRef::wrap).any(|c| (is_block(c) || is_block_macro(c)) && !skip(c)) {
                return blocks(el);
            }
            let s = inline_content(el).trim().to_string();
            let s = s.trim_end_matches('\\').trim_end().to_string();
            if s.is_empty() {
                vec![]
            } else {
                vec![s]
            }
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level: usize = name[1..].parse().unwrap_or(1);
            let s = inline_content(el).replace("\\\n", " ").trim().to_string();
            if s.is_empty() {
                vec![]
            } else {
                vec![format!("{} {s}", "#".repeat(level))]
            }
        }
        "hr" => vec!["---".into()],
        "pre" => vec![code_block(el)],
        "blockquote" => {
            let inner = join_blocks(&blocks(el));
            if inner.is_empty() {
                return vec![];
            }
            vec![inner.lines().map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") }).collect::<Vec<_>>().join("\n")]
        }
        "ul" | "ol" | "menu" | "dir" => vec![list(el)],
        "table" => vec![table(el)],
        "li" => vec![join_blocks(&blocks(el))],
        _ => blocks(el),
    }
}

fn list(el: ElementRef) -> String {
    let ordered = el.value().name() == "ol";
    let tasks = has_class(el, "inline-task-list");
    let mut n: usize = el.value().attr("start").and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut items = vec![];
    for li in el.children().filter_map(ElementRef::wrap) {
        if skip(li) {
            continue;
        }
        let parts = if li.value().name() == "li" { blocks(li) } else { block(li) };
        let marker = if tasks {
            format!("- [{}] ", if has_class(li, "checked") { "x" } else { " " })
        } else if ordered {
            let m = format!("{n}. ");
            n += 1;
            m
        } else {
            "- ".to_string()
        };
        let indent = " ".repeat(marker.len());
        let mut text = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                // nested lists stay tight, other blocks get a blank line
                let nested_list = part.starts_with("- ") || part.chars().next().is_some_and(|c| c.is_ascii_digit());
                text.push_str(if nested_list { "\n" } else { "\n\n" });
            }
            text.push_str(part);
        }
        let mut lines = text.lines();
        let first = lines.next().unwrap_or("");
        let mut item = format!("{marker}{first}");
        for l in lines {
            item.push('\n');
            if !l.is_empty() {
                item.push_str(&indent);
                item.push_str(l);
            }
        }
        items.push(item.trim_end().to_string());
    }
    items.join("\n")
}

/// Confluence language names → highlight.js names
fn normalize_lang(lang: &str) -> String {
    match lang.trim().to_lowercase().as_str() {
        "plain" | "text" | "none" | "" => String::new(),
        "js" | "jscript" => "javascript".into(),
        "py" => "python".into(),
        "ps" | "powershell" => "powershell".into(),
        "shell" | "bash" | "sh" | "b" => "bash".into(),
        "csharp" | "c#" => "csharp".into(),
        "xml" | "html" => "xml".into(),
        l => l.to_string(),
    }
}

fn code_language(pre: ElementRef) -> String {
    // Confluence: data-syntaxhighlighter-params="brush: java; gutter: false"
    if let Some(p) = pre.value().attr("data-syntaxhighlighter-params") {
        for kv in p.split(';') {
            if let Some((k, v)) = kv.split_once(':') {
                if k.trim() == "brush" {
                    return normalize_lang(v);
                }
            }
        }
    }
    let code = pre.children().filter_map(ElementRef::wrap).find(|c| c.value().name() == "code");
    for e in [Some(pre), code].into_iter().flatten() {
        for c in e.value().classes() {
            if let Some(l) = c.strip_prefix("language-").or_else(|| c.strip_prefix("lang-")) {
                return l.to_string();
            }
        }
        if let Some(l) = e.value().attr("data-language") {
            return l.to_string();
        }
    }
    String::new()
}

fn raw_text(el: ElementRef, out: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if e.name() == "br" => out.push('\n'),
            Node::Element(_) => raw_text(ElementRef::wrap(child).unwrap(), out),
            _ => {}
        }
    }
}

fn code_block(pre: ElementRef) -> String {
    let mut text = String::new();
    raw_text(pre, &mut text);
    fenced(&text, &code_language(pre))
}

fn fenced(text: &str, lang: &str) -> String {
    let text = text.replace('\u{a0}', " ");
    let text = text.trim_matches('\n').trim_end();
    let mut fence = "```".to_string();
    while text.contains(&fence) {
        fence.push('`');
    }
    format!("{fence}{lang}\n{text}\n{fence}")
}

fn table(el: ElementRef) -> String {
    let mut rows: Vec<(Vec<String>, bool)> = vec![];
    fn collect<'a>(el: ElementRef<'a>, rows: &mut Vec<ElementRef<'a>>) {
        for c in el.children().filter_map(ElementRef::wrap) {
            match c.value().name() {
                "tr" => rows.push(c),
                "thead" | "tbody" | "tfoot" => collect(c, rows),
                _ => {}
            }
        }
    }
    let mut trs = vec![];
    collect(el, &mut trs);
    for tr in trs {
        let mut cells = vec![];
        let mut header = true;
        for cell in tr.children().filter_map(ElementRef::wrap) {
            let tag = cell.value().name();
            if tag != "td" && tag != "th" {
                continue;
            }
            header &= tag == "th";
            let text = blocks(cell)
                .iter()
                .map(|b| b.replace("\\\n", " ").replace('\n', " "))
                .collect::<Vec<_>>()
                .join(" ")
                .replace('|', "\\|");
            cells.push(text.trim().to_string());
            let span: usize = cell.value().attr("colspan").and_then(|s| s.parse().ok()).unwrap_or(1);
            for _ in 1..span.min(50) {
                cells.push(String::new());
            }
        }
        if !cells.is_empty() {
            rows.push((cells, header));
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    let cols = rows.iter().map(|r| r.0.len()).max().unwrap_or(1);
    let fmt = |cells: &[String]| {
        let mut c: Vec<String> = cells.to_vec();
        c.resize(cols, String::new());
        format!("| {} |", c.join(" | "))
    };
    // GFM needs a header row: use the first row
    let mut out = vec![fmt(&rows[0].0), format!("|{}", " --- |".repeat(cols))];
    for (cells, _) in &rows[1..] {
        out.push(fmt(cells));
    }
    out.join("\n")
}

// ---------------------------------------------------------------------------
// inline
// ---------------------------------------------------------------------------

fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    // characters that only matter at the start of a line
    if let Some(rest) = out.strip_prefix('#') {
        return format!("\\#{rest}");
    }
    out
}

fn inline_content(el: ElementRef) -> String {
    let mut out = String::new();
    for child in el.children() {
        match child.value() {
            Node::Text(t) => out.push_str(&escape_text(&collapse(t))),
            Node::Element(_) => {
                let c = ElementRef::wrap(child).unwrap();
                if !skip(c) {
                    out.push_str(&inline(c));
                }
            }
            _ => {}
        }
    }
    out
}

/// Wraps `inner` in a marker, keeping surrounding spaces outside ("a ** b**" is not bold).
fn wrap(inner: &str, marker: &str) -> String {
    let core = inner.trim();
    if core.is_empty() {
        return inner.to_string();
    }
    let lead = if inner.starts_with(' ') { " " } else { "" };
    let trail = if inner.ends_with(' ') { " " } else { "" };
    format!("{lead}{marker}{core}{marker}{trail}")
}

fn link_target(url: &str) -> String {
    let url = url.trim();
    if url.contains(' ') || url.contains(')') {
        format!("<{url}>")
    } else {
        url.to_string()
    }
}

fn emoticon(el: ElementRef) -> Option<String> {
    if let Some(e) = el.value().attr("data-emoji-fallback").filter(|e| !e.starts_with(':')) {
        return Some(e.to_string());
    }
    let names = [
        ("tick", "✅"), ("cross", "❌"), ("warning", "⚠️"), ("information", "ℹ️"), ("smile", "🙂"), ("sad", "🙁"),
        ("cheeky", "😛"), ("laugh", "😀"), ("wink", "😉"), ("thumbs-up", "👍"), ("thumbs-down", "👎"), ("plus", "➕"),
        ("minus", "➖"), ("question", "❓"), ("light-on", "💡"), ("light-off", "💡"), ("yellow-star", "⭐"),
        ("red-star", "⭐"), ("green-star", "⭐"), ("blue-star", "⭐"), ("heart", "❤️"), ("broken-heart", "💔"),
    ];
    let key = el.value().classes().find_map(|c| c.strip_prefix("emoticon-")).map(|s| s.to_string()).or_else(|| {
        el.value().attr("src").and_then(|s| s.rsplit('/').next()).map(|f| f.split('.').next().unwrap_or("").to_string())
    })?;
    names.iter().find(|(n, _)| *n == key).map(|(_, e)| e.to_string()).or_else(|| el.value().attr("alt").map(|a| a.to_string()))
}

fn inline(el: ElementRef) -> String {
    let name = el.value().name();
    if is_storage(el) {
        return storage_inline(el);
    }
    match name {
        "br" => "\\\n".into(),
        "strong" | "b" => wrap(&inline_content(el), "**"),
        "em" | "i" | "cite" | "dfn" => wrap(&inline_content(el), "*"),
        "s" | "del" | "strike" => wrap(&inline_content(el), "~~"),
        "u" | "ins" => {
            let inner = inline_content(el);
            if inner.trim().is_empty() {
                inner
            } else {
                format!("<u>{}</u>", inner.trim())
            }
        }
        "code" | "kbd" | "samp" | "tt" => {
            let mut text = String::new();
            raw_text(el, &mut text);
            let text = collapse(&text);
            let core = text.trim();
            if core.is_empty() {
                return text;
            }
            let fence = if core.contains('`') { "``" } else { "`" };
            format!("{fence}{core}{fence}")
        }
        "img" => {
            if has_class(el, "emoticon") || el.value().attr("src").is_some_and(|s| s.contains("/emoticons/")) {
                return emoticon(el).unwrap_or_default();
            }
            let src = el.value().attr("src").or_else(|| el.value().attr("data-image-src")).unwrap_or("");
            // Confluence theme icons (bullets, macro placeholders)
            if src.is_empty() || src.starts_with("images/icons/") || src.contains("/images/icons/") {
                return String::new();
            }
            let alt = el.value().attr("alt").unwrap_or("").replace(['[', ']'], "");
            format!("![{}]({})", alt.trim(), link_target(src))
        }
        "a" => {
            let text = inline_content(el);
            let href = el.value().attr("href").unwrap_or("").trim();
            // mentions and links without a usable target keep only their text
            if href.is_empty() || href.starts_with('#') || href.starts_with("javascript:") || has_class(el, "confluence-userlink") {
                return text;
            }
            let label = text.trim();
            if label.is_empty() {
                // e.g. a link around an image only
                return String::new();
            }
            if label.starts_with("![") {
                return text;
            }
            let lead = if text.starts_with(' ') { " " } else { "" };
            let trail = if text.ends_with(' ') { " " } else { "" };
            format!("{lead}[{label}]({}){trail}", link_target(href))
        }
        "span" if has_class(el, "status-macro") => wrap(&inline_content(el).to_uppercase(), "**"),
        _ if is_block(el) => {
            // block inside inline context (e.g. <div> in a <span>): keep the text on one line
            let b = join_blocks(&blocks(el));
            format!(" {} ", b.replace('\n', " "))
        }
        _ => inline_content(el),
    }
}

// ---------------------------------------------------------------------------
// Confluence storage format (ac:/ri: elements)
// ---------------------------------------------------------------------------

/// Macros rendered inline; every other structured macro is a block
const INLINE_MACROS: &[&str] = &["status", "jira", "anchor", "view-file", "viewpdf", "viewdoc", "viewxls", "viewppt", "multimedia"];

/// Dynamic macros (generated lists, search boxes) with nothing to keep
const DYNAMIC_MACROS: &[&str] = &[
    "toc", "toc-zone", "children", "pagetree", "pagetreesearch", "contentbylabel", "content-report-table", "livesearch",
    "recently-updated", "attachments", "blog-posts", "create-from-template", "create-space-button", "profile",
    "listlabels", "popular-labels", "related-labels", "space-details", "tasks-report-macro", "detailssummary",
    "recently-updated-dashboard", "favpages", "network", "index", "excerpt-include-placeholder",
];

fn is_storage(el: ElementRef) -> bool {
    let n = el.value().name();
    n.starts_with("ac:") || n.starts_with("ri:") || n == "time"
}

fn ac_attr<'a>(el: ElementRef<'a>, name: &str) -> &'a str {
    el.value().attr(name).unwrap_or("")
}

fn is_storage_block(el: ElementRef) -> bool {
    match el.value().name() {
        "ac:structured-macro" | "ac:macro" => !INLINE_MACROS.contains(&ac_attr(el, "ac:name")),
        "ac:layout" | "ac:layout-section" | "ac:layout-cell" | "ac:task-list" | "ac:adf-extension" | "ac:rich-text-body" => true,
        _ => false,
    }
}

fn child_named<'a>(el: ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    el.children().filter_map(ElementRef::wrap).find(|c| c.value().name() == name)
}

fn descendant_named<'a>(el: ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    el.descendants().filter_map(ElementRef::wrap).find(|c| c.value().name() == name)
}

fn macro_param<'a>(el: ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    el.children()
        .filter_map(ElementRef::wrap)
        .find(|c| c.value().name() == "ac:parameter" && ac_attr(*c, "ac:name") == name)
}

fn macro_param_text(el: ElementRef, name: &str) -> String {
    macro_param(el, name).map(|p| collapse(&p.text().collect::<String>()).trim().to_string()).unwrap_or_default()
}

/// Percent-encodes the characters that would break a placeholder URL or a Markdown link.
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' | '/' | '#' | ' ' | '(' | ')' | '<' | '>' | '[' | ']' | '"' | '\\' | '\n' | '\t' => {
                let mut b = [0; 4];
                for byte in c.encode_utf8(&mut b).bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Placeholder URL for an `ri:attachment` (optionally on another page).
fn attachment_url(att: ElementRef) -> Option<(String, String)> {
    let file = ac_attr(att, "ri:filename");
    if file.is_empty() {
        return None;
    }
    let page = descendant_named(att, "ri:page").map(|p| ac_attr(p, "ri:content-title")).unwrap_or("");
    Some((format!("cfl-att:{}/{}", enc(page), enc(file)), file.to_string()))
}

/// Placeholder URL for an `ri:page` / `ri:blog-post` reference.
fn page_url(page: ElementRef, anchor: &str) -> Option<(String, String)> {
    let title = ac_attr(page, "ri:content-title");
    if title.is_empty() {
        return None;
    }
    let space = ac_attr(page, "ri:space-key");
    let anchor = if anchor.is_empty() { String::new() } else { format!("#{}", enc(anchor)) };
    Some((format!("cfl-page:{}/{}{anchor}", enc(space), enc(title)), title.to_string()))
}

fn notice(style: &str, title: &str, body: Vec<String>) -> Vec<String> {
    let mut parts = vec![];
    if !title.is_empty() {
        parts.push(format!("**{}**", escape_text(title)));
    }
    parts.extend(body);
    if parts.is_empty() {
        return vec![];
    }
    vec![format!(":::{style}\n{}\n:::", join_blocks(&parts))]
}

fn storage_block(el: ElementRef) -> Vec<String> {
    match el.value().name() {
        "ac:structured-macro" | "ac:macro" => {
            let name = ac_attr(el, "ac:name").to_lowercase();
            if INLINE_MACROS.contains(&name.as_str()) {
                let s = storage_inline(el);
                return if s.trim().is_empty() { vec![] } else { vec![s.trim().to_string()] };
            }
            if DYNAMIC_MACROS.contains(&name.as_str()) {
                return vec![];
            }
            let rich = child_named(el, "ac:rich-text-body");
            let title = macro_param_text(el, "title");
            match name.as_str() {
                "code" | "noformat" => {
                    let mut text = String::new();
                    if let Some(b) = child_named(el, "ac:plain-text-body") {
                        raw_text(b, &mut text);
                    }
                    if text.trim().is_empty() {
                        return vec![];
                    }
                    let mut out = vec![];
                    if !title.is_empty() {
                        out.push(format!("**{}**", escape_text(&title)));
                    }
                    out.push(fenced(&text, &normalize_lang(&macro_param_text(el, "language"))));
                    out
                }
                "info" | "note" | "warning" | "tip" => {
                    let style = match name.as_str() {
                        "tip" => "tip",
                        "info" => "info",
                        _ => "warning",
                    };
                    notice(style, &title, rich.map(blocks).unwrap_or_default())
                }
                "include" | "excerpt-include" => {
                    let link = descendant_named(el, "ri:page").and_then(|p| page_url(p, ""));
                    match link {
                        Some((url, t)) => vec![format!("[{}]({url})", escape_text(&t))],
                        None => vec![],
                    }
                }
                _ => {
                    // expand, panel, details, excerpt, section/column, … : keep the content
                    let mut out = vec![];
                    if !title.is_empty() {
                        out.push(format!("**{}**", escape_text(&title)));
                    }
                    if let Some(b) = rich {
                        out.extend(blocks(b));
                    } else if let Some(b) = child_named(el, "ac:plain-text-body") {
                        let mut text = String::new();
                        raw_text(b, &mut text);
                        if !text.trim().is_empty() {
                            out.push(fenced(&text, ""));
                        }
                    }
                    out
                }
            }
        }
        "ac:task-list" => {
            let mut items = vec![];
            for task in el.children().filter_map(ElementRef::wrap).filter(|c| c.value().name() == "ac:task") {
                let done = child_named(task, "ac:task-status").is_some_and(|s| s.text().collect::<String>().trim() == "complete");
                let body = child_named(task, "ac:task-body")
                    .map(|b| join_blocks(&blocks(b)).replace("\n\n", " ").replace('\n', " "))
                    .unwrap_or_default();
                items.push(format!("- [{}] {}", if done { "x" } else { " " }, body.trim()));
            }
            vec![items.join("\n")]
        }
        "ac:adf-extension" => {
            // newer Cloud panels: prefer the ADF node, fall back to the rendered HTML
            if let Some(node) = child_named(el, "ac:adf-node").filter(|n| ac_attr(*n, "type") == "panel") {
                let panel_type = node
                    .children()
                    .filter_map(ElementRef::wrap)
                    .find(|a| a.value().name() == "ac:adf-attribute" && ac_attr(*a, "key") == "panel-type")
                    .map(|a| a.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                let style = match panel_type.as_str() {
                    "warning" | "error" => "warning",
                    "success" => "success",
                    "tip" => "tip",
                    _ => "info",
                };
                let content = child_named(node, "ac:adf-content").map(blocks).unwrap_or_default();
                return notice(style, "", content);
            }
            child_named(el, "ac:adf-fallback").map(blocks).unwrap_or_default()
        }
        "ac:layout" | "ac:layout-section" | "ac:layout-cell" | "ac:rich-text-body" => blocks(el),
        _ => {
            let s = storage_inline(el);
            if s.trim().is_empty() {
                vec![]
            } else {
                vec![s.trim().to_string()]
            }
        }
    }
}

fn emoticon_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "tick" => "✅",
        "cross" => "❌",
        "warning" => "⚠️",
        "information" => "ℹ️",
        "smile" => "🙂",
        "sad" => "🙁",
        "cheeky" => "😛",
        "laugh" => "😀",
        "wink" => "😉",
        "thumbs-up" => "👍",
        "thumbs-down" => "👎",
        "plus" => "➕",
        "minus" => "➖",
        "question" => "❓",
        "light-on" | "light-off" => "💡",
        "yellow-star" | "red-star" | "green-star" | "blue-star" => "⭐",
        "heart" => "❤️",
        "broken-heart" => "💔",
        _ => return None,
    })
}

fn storage_inline(el: ElementRef) -> String {
    match el.value().name() {
        "ac:structured-macro" | "ac:macro" => match ac_attr(el, "ac:name").to_lowercase().as_str() {
            "status" => {
                let t = macro_param_text(el, "title");
                if t.is_empty() {
                    String::new()
                } else {
                    format!("**{}**", escape_text(&t.to_uppercase()))
                }
            }
            "jira" => escape_text(&macro_param_text(el, "key")),
            "anchor" => String::new(),
            "view-file" | "viewpdf" | "viewdoc" | "viewxls" | "viewppt" | "multimedia" => {
                match macro_param(el, "name").and_then(|p| descendant_named(p, "ri:attachment")).and_then(attachment_url) {
                    Some((url, file)) => format!("[{}]({url})", escape_text(&file)),
                    None => String::new(),
                }
            }
            _ => {
                // block macro inside inline content
                let b = join_blocks(&storage_block(el));
                format!(" {} ", b.replace('\n', " "))
            }
        },
        "ac:image" => {
            let alt = ac_attr(el, "ac:alt").replace(['[', ']'], "");
            if let Some((url, _)) = descendant_named(el, "ri:attachment").and_then(attachment_url) {
                return format!("![{alt}]({url})");
            }
            match descendant_named(el, "ri:url").map(|u| ac_attr(u, "ri:value")).filter(|u| !u.is_empty()) {
                Some(u) => format!("![{alt}]({})", link_target(u)),
                None => String::new(),
            }
        }
        "ac:link" => {
            let anchor = ac_attr(el, "ac:anchor");
            let target = descendant_named(el, "ri:page")
                .or_else(|| descendant_named(el, "ri:blog-post"))
                .and_then(|p| page_url(p, anchor))
                .or_else(|| descendant_named(el, "ri:attachment").and_then(attachment_url));
            let label = if let Some(b) = descendant_named(el, "ac:link-body") {
                inline_content(b).trim().to_string()
            } else if let Some(b) = descendant_named(el, "ac:plain-text-link-body") {
                let mut t = String::new();
                raw_text(b, &mut t);
                escape_text(collapse(&t).trim())
            } else {
                String::new()
            };
            match target {
                Some((url, default)) => {
                    let label = if label.is_empty() { escape_text(&default) } else { label };
                    format!("[{label}]({url})")
                }
                // mentions (already replaced by their text) and in-page anchors
                None if label.is_empty() => {
                    let t = collapse(&el.text().collect::<String>()).trim().to_string();
                    if t.is_empty() {
                        escape_text(anchor)
                    } else {
                        escape_text(&t)
                    }
                }
                None => label,
            }
        }
        "ac:emoticon" => {
            let fallback = ac_attr(el, "ac:emoji-fallback");
            if !fallback.is_empty() && !fallback.starts_with(':') {
                return fallback.to_string();
            }
            emoticon_name(ac_attr(el, "ac:name")).map(|e| e.to_string()).unwrap_or_default()
        }
        "time" => {
            let d = ac_attr(el, "datetime");
            if d.is_empty() {
                inline_content(el)
            } else {
                d.to_string()
            }
        }
        "ac:placeholder" | "ac:parameter" | "ac:task-id" | "ac:plain-text-body" => String::new(),
        n if n.starts_with("ri:") => String::new(),
        _ if is_storage_block(el) => {
            let b = join_blocks(&storage_block(el));
            format!(" {} ", b.replace('\n', " "))
        }
        // ac:inline-comment-marker, ac:link-body outside links, …
        _ => inline_content(el),
    }
}

#[cfg(test)]
mod tests {
    use super::convert;

    #[test]
    fn basics() {
        let md = convert("<p>Hello <strong>bold </strong>and <a href=\"x.html\">link</a></p><ul><li>a<ul><li>b</li></ul></li><li>c</li></ul>");
        assert_eq!(md, "Hello **bold** and [link](x.html)\n\n- a\n  - b\n- c");
    }

    #[test]
    fn confluence_macros() {
        let html = r#"<div class="confluence-information-macro confluence-information-macro-note"><p class="title">Careful</p>
            <span class="aui-icon confluence-information-macro-icon"></span>
            <div class="confluence-information-macro-body"><p>Body text</p></div></div>
            <div class="code panel pdl"><div class="codeContent panelContent pdl">
            <pre class="syntaxhighlighter-pre" data-syntaxhighlighter-params="brush: java; gutter: false">int a = 1;
int b = 2;</pre></div></div>
            <ul class="inline-task-list"><li class="checked"><span>done</span></li><li><span>todo</span></li></ul>"#;
        let md = convert(html);
        assert_eq!(
            md,
            ":::warning\n**Careful**\n\nBody text\n:::\n\n```java\nint a = 1;\nint b = 2;\n```\n\n- [x] done\n- [ ] todo"
        );
    }

    #[test]
    fn tables() {
        let md = convert("<table><tbody><tr><th>A</th><th>B</th></tr><tr><td><p>1</p><p>x|y</p></td><td>2</td></tr></tbody></table>");
        assert_eq!(md, "| A | B |\n| --- | --- |\n| 1 x\\|y | 2 |");
    }

    #[test]
    fn storage_format() {
        let xhtml = r#"<h2>Setup</h2><ac:structured-macro ac:name="code"><ac:parameter ac:name="language">b</ac:parameter><ac:plain-text-body><![CDATA[echo "<timestamp>" && ls]]></ac:plain-text-body></ac:structured-macro>
<p>See <ac:link><ri:page ri:content-title="Other Page" /><ac:link-body>this</ac:link-body></ac:link> and <ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">ABC-1</ac:parameter></ac:structured-macro>
<ac:structured-macro ac:name="status"><ac:parameter ac:name="title">aperto</ac:parameter></ac:structured-macro></p><p />
<ac:structured-macro ac:name="note"><ac:parameter ac:name="title">Attenzione</ac:parameter><ac:rich-text-body><p>Testo</p></ac:rich-text-body></ac:structured-macro>
<p><ac:image ac:alt="x"><ri:attachment ri:filename="image 1.png" /></ac:image></p>
<ac:task-list><ac:task><ac:task-id>1</ac:task-id><ac:task-status>complete</ac:task-status><ac:task-body>fatto</ac:task-body></ac:task></ac:task-list>
<ac:structured-macro ac:name="toc" />"#;
        let md = super::convert_storage(xhtml);
        assert_eq!(
            md,
            "## Setup\n\n```bash\necho \"<timestamp>\" && ls\n```\n\nSee [this](cfl-page:/Other%20Page) and ABC-1 **APERTO**\n\n:::warning\n**Attenzione**\n\nTesto\n:::\n\n![x](cfl-att:/image%201.png)\n\n- [x] fatto"
        );
    }
}
