//! A `<pre>` with coloured text, as wikis use for screens of a terminal (IBM i 5250, shells), drawn as an SVG
//! image that looks like it: the editor keeps neither the colours nor the lines made of `display:block` spans.
//!
//! Lines are the `display:block` spans of the `<pre>`, or its newlines; every span can set `color`,
//! `background`/`background-color` and `text-decoration: underline`. Characters are placed on a fixed grid, so the
//! columns stay aligned whatever monospace font the browser has.

use scraper::{ElementRef, Html, Node};

#[derive(Clone, Default)]
struct Style {
    color: Option<String>,
    background: Option<String>,
    underline: bool,
}

struct Segment {
    text: String,
    style: Style,
}

#[derive(Default)]
struct Line {
    background: Option<String>,
    segments: Vec<Segment>,
}

impl Line {
    fn columns(&self) -> usize {
        self.segments.iter().map(|s| s.text.chars().count()).sum()
    }
}

/// A CSS colour that can go into an SVG attribute as it is; `None` for `transparent` and anything else
fn colour(v: &str) -> Option<String> {
    let v = v.trim();
    let ok = !v.is_empty()
        && v.len() <= 40
        && !v.eq_ignore_ascii_case("transparent")
        && !v.eq_ignore_ascii_case("inherit")
        && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | ' ' | '%'));
    ok.then(|| v.to_string())
}

/// Declarations of a `style` attribute, with lowercase names
fn declarations(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect()
}

struct Screen {
    lines: Vec<Line>,
    current: Line,
    /// lines are the `display:block` spans: the newlines between them are only there for the source
    block_lines: bool,
}

impl Screen {
    fn end_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.current));
    }

    fn text(&mut self, text: &str, style: &Style) {
        if self.block_lines && text.trim().is_empty() && text.contains('\n') {
            return;
        }
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.end_line();
            }
            let part = part.replace(['\u{a0}', '\t'], " ").replace('\r', "");
            if !part.is_empty() {
                self.current.segments.push(Segment { text: part, style: style.clone() });
            }
        }
    }

    fn walk(&mut self, el: ElementRef, style: &Style) {
        for child in el.children() {
            match child.value() {
                Node::Text(t) => self.text(t, style),
                Node::Element(e) if e.name() == "br" => self.end_line(),
                Node::Element(e) => {
                    let Some(child_el) = ElementRef::wrap(child) else { continue };
                    let mut s = style.clone();
                    let mut block = false;
                    for (k, v) in declarations(e.attr("style").unwrap_or("")) {
                        match k.as_str() {
                            "color" => s.color = colour(&v).or(s.color),
                            "background" | "background-color" => s.background = colour(&v),
                            "text-decoration" | "text-decoration-line" => s.underline = v.contains("underline"),
                            "display" => block = v == "block",
                            _ => {}
                        }
                    }
                    if block {
                        if !self.current.segments.is_empty() {
                            self.end_line();
                        }
                        // the background of the line, not of each piece of text
                        self.current.background = s.background.take();
                        self.walk(child_el, &s);
                        self.end_line();
                    } else {
                        self.walk(child_el, &s);
                    }
                }
                _ => {}
            }
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The SVG of a `<pre>…</pre>` element, or `None` when it has no text
pub fn render(pre_html: &str) -> Option<String> {
    let doc = Html::parse_fragment(pre_html);
    let pre = doc.root_element().descendants().filter_map(ElementRef::wrap).find(|e| e.value().name() == "pre")?;
    let mut base = Style { color: Some("#00cc00".into()), ..Style::default() };
    let mut background = None;
    let mut font_size: f64 = 16.0;
    for (k, v) in declarations(pre.value().attr("style").unwrap_or("")) {
        match k.as_str() {
            "color" => base.color = colour(&v).or(base.color),
            "background" | "background-color" => background = colour(&v),
            "font-size" => {
                if let Some(px) = v.strip_suffix("px").and_then(|n| n.trim().parse::<f64>().ok()) {
                    font_size = px.clamp(8.0, 32.0);
                }
            }
            _ => {}
        }
    }
    let block_lines = pre.descendants().filter_map(ElementRef::wrap).any(|e| {
        declarations(e.value().attr("style").unwrap_or("")).iter().any(|(k, v)| k == "display" && v == "block")
    });
    let mut screen = Screen { lines: vec![], current: Line::default(), block_lines };
    screen.walk(pre, &base);
    if !screen.current.segments.is_empty() {
        screen.end_line();
    }
    let mut lines = screen.lines;
    while lines.last().is_some_and(|l| l.columns() == 0 && l.background.is_none()) {
        lines.pop();
    }
    let cols = lines.iter().map(Line::columns).max().unwrap_or(0);
    if lines.iter().all(|l| l.segments.iter().all(|s| s.text.trim().is_empty())) {
        return None;
    }

    let (pad, cw, lh) = (8.0, font_size * 0.6, font_size * 1.2);
    let width = pad * 2.0 + cols as f64 * cw;
    let height = pad * 2.0 + lines.len() as f64 * lh;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.2} {height:.2}\" \
         font-family=\"'Source Code Pro', 'DejaVu Sans Mono', Consolas, 'Courier New', monospace\" font-size=\"{font_size}\">\n"
    );
    if let Some(bg) = &background {
        svg += &format!("<rect width=\"100%\" height=\"100%\" rx=\"4\" fill=\"{bg}\"/>\n");
    }
    // backgrounds of lines: one rectangle for consecutive lines with the same one, without seams between them
    let mut i = 0;
    while i < lines.len() {
        let Some(bg) = &lines[i].background else {
            i += 1;
            continue;
        };
        let start = i;
        while i < lines.len() && lines[i].background.as_ref() == Some(bg) {
            i += 1;
        }
        svg += &format!(
            "<rect x=\"{pad}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{bg}\"/>\n",
            pad + start as f64 * lh,
            cols as f64 * cw,
            (i - start) as f64 * lh
        );
    }
    for (i, line) in lines.iter().enumerate() {
        let top = pad + i as f64 * lh;
        let baseline = top + (lh - font_size) / 2.0 + font_size * 0.8;
        let mut col = 0usize;
        for seg in &line.segments {
            let n = seg.text.chars().count();
            let x = pad + col as f64 * cw;
            let w = n as f64 * cw;
            if let Some(bg) = &seg.style.background {
                svg += &format!("<rect x=\"{x:.2}\" y=\"{top:.2}\" width=\"{w:.2}\" height=\"{lh:.2}\" fill=\"{bg}\"/>\n");
            }
            let fill = seg.style.color.as_deref().unwrap_or("#00cc00");
            if !seg.text.trim().is_empty() {
                svg += &format!(
                    "<text x=\"{x:.2}\" y=\"{baseline:.2}\" fill=\"{fill}\" textLength=\"{w:.2}\" lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\">{}</text>\n",
                    escape(&seg.text)
                );
            }
            if seg.style.underline {
                svg += &format!(
                    "<rect x=\"{x:.2}\" y=\"{:.2}\" width=\"{w:.2}\" height=\"{:.2}\" fill=\"{fill}\"/>\n",
                    baseline + font_size * 0.12,
                    (font_size / 16.0).max(1.0)
                );
            }
            col += n;
        }
    }
    svg += "</svg>\n";
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_with_block_lines() {
        let html = r#"<pre style="color:#00cc00;font-size:10px;background:#000000;"><span style="display:block"><span style="color:#00cc00"> Menu </span><span style="color:rgb(255, 85, 85);text-decoration:underline">A&gt;B</span></span>
<span style="background:#000000;display:block;"><span style="color:#000;background:#00cc00">X</span></span></pre>"#;
        let svg = render(html).unwrap();
        // two lines of 9 columns, 6 px each, padding 8
        assert!(svg.contains("width=\"70\" height=\"40\""), "{svg}");
        assert!(svg.contains("fill=\"#000000\"/>"), "{svg}");
        assert!(svg.contains(">A&gt;B</text>"), "{svg}");
        assert!(svg.contains("fill=\"rgb(255, 85, 85)\""), "{svg}");
        assert!(svg.contains(">X</text>"), "{svg}");
        assert_eq!(svg.matches("<text").count(), 3, "{svg}");
    }

    #[test]
    fn newlines_and_unsafe_colours() {
        let svg = render("<pre style=\"color:red\" onload=\"x\">a\nbb<br>c</pre>").unwrap();
        assert_eq!(svg.matches("<text").count(), 3, "{svg}");
        assert!(svg.contains("fill=\"red\""), "{svg}");
        let svg = render("<pre><span style=\"color:url(javascript:x)\">a</span></pre>").unwrap();
        assert!(!svg.contains("javascript"), "{svg}");
        assert!(render("<pre> \n </pre>").is_none());
    }
}
