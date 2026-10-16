// Export of a document from the editor content: standalone HTML (images embedded), PDF (the
// browser's print dialog on that HTML, "Save as PDF") and Word (.docx, built with the docx library,
// loaded only when needed). Markdown comes from the server (documents.export).
import { downloadText, slugify } from '../api';

const absolute = (href) => (href && href.startsWith('/') ? `${window.location.origin}${href}` : href);

function blobToDataUrl(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });
}

const EXPORT_CSS = `
:root { color-scheme: light; }
body { margin: 0; background: #fff; color: #111319; font: 16px/1.6 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; }
main { max-width: 760px; margin: 0 auto; padding: 48px 24px 64px; }
h1, h2, h3, h4 { line-height: 1.25; margin: 1.6em 0 0.5em; }
h1.doc-title { font-size: 2.2em; margin-top: 0; }
h1 { font-size: 1.75em; } h2 { font-size: 1.4em; } h3 { font-size: 1.15em; } h4 { font-size: 1em; }
p { margin: 0 0 0.8em; }
a { color: #0366d6; }
img { max-width: 100%; height: auto; }
hr { border: 0; border-top: 1px solid #dae1e9; margin: 2em 0; }
blockquote { margin: 1em 0; padding: 0 0 0 1em; border-left: 3px solid #dae1e9; color: #4e5c6e; }
code { font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace; font-size: 0.9em; background: #f4f7fa; padding: 0.1em 0.3em; border-radius: 3px; }
pre { background: #f4f7fa; padding: 12px 16px; border-radius: 6px; overflow-x: auto; white-space: pre-wrap; }
pre code { background: none; padding: 0; font-size: 0.85em; }
mark { background: #fdea9b; }
table { border-collapse: collapse; width: 100%; margin: 1em 0; }
th, td { border: 1px solid #dae1e9; padding: 6px 10px; text-align: left; vertical-align: top; }
th { background: #f4f7fa; }
td p, th p { margin: 0; }
ul[data-type="taskList"] { list-style: none; padding-left: 0.2em; }
ul[data-type="taskList"] li { display: flex; gap: 0.5em; align-items: flex-start; }
ul[data-type="taskList"] li > label { margin-top: 0.2em; }
ul[data-type="taskList"] li > div { flex: 1; }
ul[data-type="taskList"] li[data-checked="true"] > div { color: #8796a5; text-decoration: line-through; }
.notice { margin: 1em 0; padding: 10px 16px; border-radius: 6px; border-left: 4px solid #2f80ed; background: #e8f1fd; }
.notice[data-style="warning"] { border-color: #f08a24; background: #fff4e5; }
.notice[data-style="tip"] { border-color: #b8860b; background: #fff9db; }
.notice[data-style="success"] { border-color: #2fae64; background: #e7f8ee; }
.notice > :last-child { margin-bottom: 0; }
.mention { font-weight: 500; }
.mention-user, .mention-group { color: #0366d6; }
nav.toc { margin: 1em 0; padding: 12px 16px; border: 1px solid #dae1e9; border-radius: 6px; }
nav.toc ul { list-style: none; margin: 0; padding-left: 0; }
nav.toc li { margin: 0.2em 0; }
nav.toc .toc-2 { padding-left: 1em; } nav.toc .toc-3 { padding-left: 2em; } nav.toc .toc-4 { padding-left: 3em; }
@page { margin: 18mm 16mm; }
@media print {
  main { max-width: none; padding: 0; }
  h1, h2, h3, h4 { break-after: avoid; }
  pre, table, img, blockquote, .notice { break-inside: avoid; }
  a { color: inherit; }
}
`;

const escapeHtml = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Standalone HTML page of the document: title, content, table of contents and images inline */
export async function buildHtml(editor, { title, emoji }) {
  const dom = new DOMParser().parseFromString(`<main>${editor.getHTML()}</main>`, 'text/html');
  const main = dom.querySelector('main');

  // headings get anchors for the table of contents
  const used = new Set();
  const headings = [...main.querySelectorAll('h1, h2, h3, h4')].map((h) => {
    let id = slugify(h.textContent);
    for (let i = 2; used.has(id); i++) id = `${slugify(h.textContent)}-${i}`;
    used.add(id);
    h.id = id;
    return { level: Number(h.tagName[1]), text: h.textContent, id };
  });
  main.querySelectorAll('div[data-type="toc"]').forEach((el) => {
    const nav = dom.createElement('nav');
    nav.className = 'toc';
    nav.innerHTML = `<ul>${headings
      .map((h) => `<li class="toc-${h.level}"><a href="#${h.id}">${escapeHtml(h.text)}</a></li>`)
      .join('')}</ul>`;
    el.replaceWith(nav);
  });

  main.querySelectorAll('.mention-icon').forEach((el) => el.remove());
  main.querySelectorAll('a[href]').forEach((a) => a.setAttribute('href', absolute(a.getAttribute('href'))));
  main.querySelectorAll('input[type="checkbox"]').forEach((i) => i.setAttribute('disabled', ''));
  await Promise.all(
    [...main.querySelectorAll('img[src]')].map(async (img) => {
      const src = img.getAttribute('src');
      try {
        const res = await fetch(src, { credentials: 'same-origin' });
        if (!res.ok) throw new Error(res.statusText);
        img.setAttribute('src', await blobToDataUrl(await res.blob()));
      } catch {
        img.setAttribute('src', absolute(src));
      }
    })
  );

  const heading = `${emoji ? `${emoji} ` : ''}${title || ''}`.trim();
  return `<!doctype html>
<html lang="${document.documentElement.lang || 'en'}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title || 'Untitled')}</title>
<style>${EXPORT_CSS}</style>
</head>
<body>
<main>
${heading ? `<h1 class="doc-title">${escapeHtml(heading)}</h1>\n` : ''}${main.innerHTML}
</main>
</body>
</html>
`;
}

export async function downloadHtml(editor, doc) {
  downloadText(`${slugify(doc.title)}.html`, await buildHtml(editor, doc), 'text/html');
}

/**
 * PDF through the browser's print dialog ("Save as PDF"): text stays selectable and links work.
 * The suggested file name is the document title.
 */
export async function printPdf(editor, doc) {
  const html = await buildHtml(editor, doc);
  const frame = document.createElement('iframe');
  frame.setAttribute('aria-hidden', 'true');
  frame.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;visibility:hidden';
  document.body.appendChild(frame);
  await new Promise((resolve) => {
    frame.onload = resolve;
    frame.srcdoc = html;
  });
  // let the embedded images decode before printing
  await Promise.all([...frame.contentDocument.images].map((i) => (i.complete ? null : i.decode().catch(() => null))));
  const previousTitle = document.title;
  document.title = doc.title || previousTitle;
  frame.contentWindow.focus();
  frame.contentWindow.print();
  document.title = previousTitle;
  setTimeout(() => frame.remove(), 60 * 1000);
}

// ---------------------------------------------------------------------------
// Word
// ---------------------------------------------------------------------------

const NOTICE_COLORS = {
  info: { fill: 'E8F1FD', border: '2F80ED' },
  warning: { fill: 'FFF4E5', border: 'F08A24' },
  tip: { fill: 'FFF9DB', border: 'B8860B' },
  success: { fill: 'E7F8EE', border: '2FAE64' },
};
const MAX_IMAGE_WIDTH = 600; // px, about the width of an A4 page between margins

/** Images as PNG (Word can't show every web format) with their size; null when unavailable */
async function loadImage(src) {
  try {
    const res = await fetch(src, { credentials: 'same-origin' });
    if (!res.ok) return null;
    const url = URL.createObjectURL(await res.blob());
    try {
      const img = new Image();
      await new Promise((resolve, reject) => {
        img.onload = resolve;
        img.onerror = reject;
        img.src = url;
      });
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth || 1;
      canvas.height = img.naturalHeight || 1;
      canvas.getContext('2d').drawImage(img, 0, 0);
      const png = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
      return { data: await png.arrayBuffer(), width: canvas.width, height: canvas.height };
    } finally {
      URL.revokeObjectURL(url);
    }
  } catch {
    return null;
  }
}

function collectImages(node, out = new Set()) {
  if (node.type === 'image' && node.attrs?.src) out.add(node.attrs.src);
  (node.content || []).forEach((c) => collectImages(c, out));
  return out;
}

const plainText = (node) => (node.type === 'text' ? node.text : (node.content || []).map(plainText).join(''));

/** Word document of the editor content */
export async function downloadDocx(editor, doc) {
  const d = await import('docx');
  const {
    AlignmentType,
    BorderStyle,
    Document,
    ExternalHyperlink,
    HeadingLevel,
    ImageRun,
    LevelFormat,
    Packer,
    Paragraph,
    ShadingType,
    Table,
    TableCell,
    TableRow,
    TextRun,
    WidthType,
  } = d;
  const json = editor.getJSON();
  const images = new Map();
  await Promise.all([...collectImages(json)].map(async (src) => images.set(src, await loadImage(src))));

  const HEADINGS = [HeadingLevel.HEADING_1, HeadingLevel.HEADING_1, HeadingLevel.HEADING_2, HeadingLevel.HEADING_3, HeadingLevel.HEADING_4];
  const MONO = 'Consolas';
  let orderedInstance = 0;

  const run = (text, marks = [], extra = {}) => {
    const has = (name) => marks.some((m) => m.type === name);
    return new TextRun({
      text,
      bold: has('bold') || undefined,
      italics: has('italic') || undefined,
      underline: has('underline') ? {} : undefined,
      strike: has('strike') || undefined,
      highlight: has('highlight') ? 'yellow' : undefined,
      ...(has('code') ? { font: MONO, shading: { type: ShadingType.CLEAR, fill: 'F2F4F7', color: 'auto' } } : {}),
      ...extra,
    });
  };

  const inline = (nodes = []) =>
    nodes.flatMap((n) => {
      if (n.type === 'text') {
        const link = n.marks?.find((m) => m.type === 'link');
        if (link) return new ExternalHyperlink({ link: absolute(link.attrs.href), children: [run(n.text, n.marks, { style: 'Hyperlink' })] });
        return run(n.text, n.marks);
      }
      if (n.type === 'hardBreak') return new TextRun({ break: 1 });
      if (n.type === 'mention') {
        const { type, modelId, label } = n.attrs;
        if (type === 'document' || type === 'collection') {
          const href = absolute(`/${type === 'document' ? 'doc' : 'collection'}/${modelId}`);
          return new ExternalHyperlink({ link: href, children: [run(label, [], { style: 'Hyperlink' })] });
        }
        return run(`@${label}`, [], { bold: true, color: '0366D6' });
      }
      if (n.type === 'image') return imageRun(n);
      return [];
    });

  function imageRun(n) {
    const img = images.get(n.attrs.src);
    if (!img) return n.attrs.src ? [new ExternalHyperlink({ link: absolute(n.attrs.src), children: [run(n.attrs.alt || n.attrs.src, [], { style: 'Hyperlink' })] })] : [];
    const scale = Math.min(1, MAX_IMAGE_WIDTH / img.width);
    return new ImageRun({
      type: 'png',
      data: img.data,
      transformation: { width: Math.round(img.width * scale), height: Math.round(img.height * scale) },
      altText: n.attrs.alt ? { name: n.attrs.alt, description: n.attrs.alt, title: n.attrs.alt } : undefined,
    });
  }

  /** ctx: indent (twips), shading and border of the enclosing quote or notice */
  const paragraph = (children, ctx, extra = {}) =>
    new Paragraph({
      children,
      indent: ctx.indent ? { left: ctx.indent } : undefined,
      shading: ctx.fill ? { type: ShadingType.CLEAR, fill: ctx.fill, color: 'auto' } : undefined,
      border: ctx.border ? { left: { style: BorderStyle.SINGLE, size: 18, color: ctx.border, space: 8 } } : undefined,
      ...extra,
    });

  const tocParagraphs = () =>
    (json.content || [])
      .filter((n) => n.type === 'heading' && n.attrs.level <= 4)
      .map((h) => new Paragraph({ children: [new TextRun(plainText(h))], indent: { left: 360 * (h.attrs.level - 1) }, spacing: { after: 40 } }));

  function list(node, ctx, level) {
    const ordered = node.type === 'orderedList';
    const instance = ordered ? ++orderedInstance : 0;
    return (node.content || []).flatMap((item) => {
      const checked = item.type === 'taskItem' ? item.attrs?.checked : null;
      let first = true;
      return (item.content || []).flatMap((child) => {
        if (['bulletList', 'orderedList', 'taskList'].includes(child.type)) return list(child, ctx, level + 1);
        if (child.type !== 'paragraph' || !first) return blocks([child], { ...ctx, indent: (ctx.indent || 0) + 720 * (level + 1) });
        first = false;
        const children = inline(child.content);
        if (checked !== null) {
          return paragraph([new TextRun(checked ? '☑ ' : '☐ '), ...children], { ...ctx, indent: (ctx.indent || 0) + 360 * level + 120 });
        }
        return paragraph(children, ctx, ordered ? { numbering: { reference: 'ordered', level: Math.min(level, 8), instance } } : { bullet: { level: Math.min(level, 8) } });
      });
    });
  }

  function blocks(nodes = [], ctx = {}) {
    return nodes.flatMap((n) => {
      switch (n.type) {
        case 'paragraph':
          return paragraph(inline(n.content), ctx);
        case 'heading':
          return paragraph(inline(n.content), ctx, { heading: HEADINGS[n.attrs.level] || HeadingLevel.HEADING_4 });
        case 'bulletList':
        case 'orderedList':
        case 'taskList':
          return list(n, ctx, 0);
        case 'blockquote':
          return blocks(n.content, { ...ctx, border: 'B0BAC5', indent: (ctx.indent || 0) + 240 });
        case 'notice': {
          const c = NOTICE_COLORS[n.attrs.style] || NOTICE_COLORS.info;
          return blocks(n.content, { ...ctx, fill: c.fill, border: c.border });
        }
        case 'codeBlock':
          return plainText(n)
            .split('\n')
            .map((line) =>
              paragraph([new TextRun({ text: line || ' ', font: MONO, size: 19 })], { ...ctx, fill: ctx.fill || 'F4F7FA' }, { spacing: { after: 0, line: 260 } })
            );
        case 'horizontalRule':
          return new Paragraph({ children: [], border: { bottom: { style: BorderStyle.SINGLE, size: 6, color: 'DAE1E9', space: 1 } } });
        case 'image':
          return paragraph([].concat(imageRun(n)), ctx);
        case 'toc':
          return tocParagraphs();
        case 'table':
          return new Table({
            width: { size: 100, type: WidthType.PERCENTAGE },
            rows: (n.content || []).map(
              (row, r) =>
                new TableRow({
                  tableHeader: r === 0 && row.content?.some((c) => c.type === 'tableHeader'),
                  children: (row.content || []).map((cell) => {
                    const content = blocks(cell.content, {});
                    return new TableCell({
                      columnSpan: cell.attrs?.colspan > 1 ? cell.attrs.colspan : undefined,
                      rowSpan: cell.attrs?.rowspan > 1 ? cell.attrs.rowspan : undefined,
                      shading: cell.type === 'tableHeader' ? { type: ShadingType.CLEAR, fill: 'F4F7FA', color: 'auto' } : undefined,
                      children: content.length ? content : [new Paragraph('')],
                    });
                  }),
                })
            ),
          });
        default:
          return n.content ? blocks(n.content, ctx) : [];
      }
    });
  }

  const title = `${doc.emoji ? `${doc.emoji} ` : ''}${doc.title || ''}`.trim();
  const document_ = new Document({
    title: doc.title || undefined,
    styles: { default: { document: { run: { font: 'Calibri', size: 22 } } } },
    numbering: {
      config: [
        {
          reference: 'ordered',
          levels: Array.from({ length: 9 }, (_, level) => ({
            level,
            format: [LevelFormat.DECIMAL, LevelFormat.LOWER_LETTER, LevelFormat.LOWER_ROMAN][level % 3],
            text: `%${level + 1}.`,
            alignment: AlignmentType.START,
            style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
          })),
        },
      ],
    },
    sections: [{ children: [...(title ? [new Paragraph({ text: title, heading: HeadingLevel.TITLE })] : []), ...blocks(json.content)] }],
  });
  const blob = await Packer.toBlob(document_);
  const url = URL.createObjectURL(blob);
  const a = window.document.createElement('a');
  a.href = url;
  a.download = `${slugify(doc.title)}.docx`;
  window.document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
