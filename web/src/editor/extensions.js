import { Extension, Mark, Node, getHTMLFromFragment, markInputRule, mergeAttributes } from '@tiptap/core';
import { Fragment } from '@tiptap/pm/model';
import { ReactNodeViewRenderer, ReactRenderer } from '@tiptap/react';
import Suggestion from '@tiptap/suggestion';
import { NodeSelection, Plugin, PluginKey, TextSelection } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import container from 'markdown-it-container';
import CommandList from './CommandList';
import TocView from './TocView';
import EmbedView from './EmbedView';
import { embedFor } from './embeds';

export const NOTICE_STYLES = ['info', 'warning', 'tip', 'success'];

/** Outline style notice blocks, stored in markdown as :::info … ::: */
export const Notice = Node.create({
  name: 'notice',
  group: 'block',
  content: 'block+',
  defining: true,

  addAttributes() {
    return {
      style: {
        default: 'info',
        parseHTML: (el) => el.getAttribute('data-style') || 'info',
        renderHTML: (attrs) => ({ 'data-style': attrs.style }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div.notice' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes({ class: 'notice' }, HTMLAttributes), 0];
  },

  addCommands() {
    return {
      setNotice:
        (style = 'info') =>
        ({ commands }) =>
          commands.wrapIn(this.name, { style }),
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state, node) {
          state.write(`:::${node.attrs.style || 'info'}\n`);
          state.renderContent(node);
          state.flushClose(1);
          state.write(':::');
          state.closeBlock(node);
        },
        parse: {
          setup(md) {
            NOTICE_STYLES.forEach((name) => {
              md.use(container, name, {
                render(tokens, idx) {
                  return tokens[idx].nesting === 1 ? `<div class="notice" data-style="${name}">\n` : '</div>\n';
                },
              });
            });
          },
        },
      },
    };
  },
});

const TOC_MARKER = /^\[\[toc\]\]\s*$/i;

/** Table of contents built from the document's headings, stored in markdown as a [[toc]] line */
export const Toc = Node.create({
  name: 'toc',
  group: 'block',
  atom: true,
  draggable: true,

  parseHTML() {
    return [{ tag: 'div[data-type="toc"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes({ 'data-type': 'toc' }, HTMLAttributes)];
  },

  addNodeView() {
    return ReactNodeViewRenderer(TocView);
  },

  addCommands() {
    return {
      insertToc:
        () =>
        ({ commands }) =>
          commands.insertContent({ type: this.name }),
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state, node) {
          state.write('[[toc]]');
          state.closeBlock(node);
        },
        parse: {
          setup(md) {
            md.block.ruler.before('paragraph', 'toc', (state, startLine, endLine, silent) => {
              if (state.sCount[startLine] - state.blkIndent >= 4) return false;
              const line = state.src.slice(state.bMarks[startLine] + state.tShift[startLine], state.eMarks[startLine]);
              if (!TOC_MARKER.test(line)) return false;
              if (silent) return true;
              const token = state.push('toc', 'div', 0);
              token.map = [startLine, startLine + 1];
              state.line = startLine + 1;
              return true;
            });
            md.renderer.rules.toc = () => '<div data-type="toc"></div>\n';
          },
        },
      },
    };
  },
});

// ---------------------------------------------------------------------------
// template placeholders
// ---------------------------------------------------------------------------

/**
 * Range of the placeholder that starts at $pos or contains it (only the text after $pos counts,
 * so the cursor right after a placeholder is outside it).
 */
function placeholderRange($pos, type) {
  const { parent, parentOffset } = $pos;
  const after = parent.childAfter(parentOffset);
  if (!after.node || !type.isInSet(after.node.marks)) return null;
  let start = after.index;
  let end = after.index + 1;
  let from = $pos.start() + after.offset;
  let to = from + after.node.nodeSize;
  while (start > 0 && type.isInSet(parent.child(start - 1).marks)) from -= parent.child(--start).nodeSize;
  while (end < parent.childCount && type.isInSet(parent.child(end).marks)) to += parent.child(end++).nodeSize;
  return { from, to };
}

/**
 * Placeholder in a template, as in Outline: highlighted text, stored in markdown as !!text!!, that the
 * author of a new document replaces. Outside templates a click puts the cursor at its start, typing or
 * pasting replaces it, Backspace removes it and the arrows skip it.
 * Options: template, true while editing a template (the placeholder is then ordinary text).
 */
export const TemplatePlaceholder = Mark.create({
  name: 'placeholder',
  inclusive: false,

  addOptions() {
    return { template: false };
  },

  parseHTML() {
    return [{ tag: 'span.template-placeholder' }];
  },

  renderHTML() {
    return ['span', { class: 'template-placeholder' }, 0];
  },

  addCommands() {
    return {
      togglePlaceholder:
        () =>
        ({ commands }) =>
          commands.toggleMark(this.name),
    };
  },

  addInputRules() {
    return [markInputRule({ find: /(?:^|\s)(!!(?!\s+!!)((?:[^!]+))!!(?!\s+!!))$/, type: this.type })];
  },

  addProseMirrorPlugins() {
    const { type } = this;
    const active = (view) => !this.options.template && view.editable;
    // removes the placeholder around pos (the text after it, or before it with pos - 1) and returns the transaction
    const remove = (state, $pos, text = '') => {
      const range = placeholderRange($pos, type);
      if (!range) return null;
      const tr = state.tr.removeMark(range.from, range.to, type).insertText(text, range.from, range.to);
      return tr.setSelection(TextSelection.near(tr.doc.resolve(range.from + text.length)));
    };
    const before = (state, pos) => state.doc.resolve(Math.max(0, pos - 1));

    return [
      new Plugin({
        key: new PluginKey('templatePlaceholder'),
        props: {
          handleDOMEvents: {
            paste: (view) => {
              if (!active(view)) return false;
              const { state } = view;
              const tr = remove(state, state.selection.$to) || remove(state, before(state, state.selection.from));
              if (tr) view.dispatch(tr);
              return false;
            },
            mousedown: (view, event) => {
              if (!active(view)) return false;
              const pos = view.posAtCoords({ left: event.clientX, top: event.clientY });
              if (!pos) return false;
              const { doc } = view.state;
              const range = placeholderRange(doc.resolve(pos.pos), type) || placeholderRange(before(view.state, pos.pos), type);
              if (!range) return false;
              event.preventDefault();
              view.dispatch(view.state.tr.setSelection(TextSelection.near(doc.resolve(range.from))).scrollIntoView());
              view.focus();
              return true;
            },
          },
          handleTextInput: (view, from, to, text) => {
            if (this.options.template) return false;
            const { state } = view;
            const range = placeholderRange(state.doc.resolve(from), type);
            if (!range) return false;
            const start = Math.min(from, range.from);
            const end = Math.max(to, range.to);
            const tr = state.tr.removeMark(range.from, range.to, type).insertText(text, start, end);
            view.dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(start + text.length))));
            return true;
          },
          handleKeyDown: (view, event) => {
            if (!active(view)) return false;
            const { state } = view;
            const { selection } = state;
            if (event.key === 'Backspace') {
              const tr = remove(state, before(state, selection.from));
              if (!tr) return false;
              view.dispatch(tr);
              return true;
            }
            if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
              const left = event.key === 'ArrowLeft';
              const range = placeholderRange(left ? before(state, selection.from) : selection.$from, type);
              if (!range) return false;
              view.dispatch(state.tr.setSelection(TextSelection.near(state.doc.resolve(left ? range.from : range.to))));
              return true;
            }
            return false;
          },
        },
      }),
    ];
  },

  addStorage() {
    return {
      markdown: {
        serialize: { open: '!!', close: '!!', mixable: true, expelEnclosingWhitespace: true },
        parse: {
          setup(md) {
            md.inline.ruler.before('image', 'placeholder', (state, silent) => {
              const start = state.pos;
              if (state.src.charCodeAt(start) !== 0x21 || state.src.charCodeAt(start + 1) !== 0x21) return false;
              const end = state.src.indexOf('!!', start + 2);
              if (end < 0 || end > state.posMax || end === start + 2 || /\n/.test(state.src.slice(start + 2, end))) return false;
              if (!silent) {
                state.push('placeholder_open', 'span', 1).attrs = [['class', 'template-placeholder']];
                const max = state.posMax;
                state.pos = start + 2;
                state.posMax = end;
                state.md.inline.tokenize(state);
                state.posMax = max;
                state.push('placeholder_close', 'span', -1);
              }
              state.pos = end + 2;
              return true;
            });
          },
        },
      },
    };
  },
});

// ---------------------------------------------------------------------------
// tables
// ---------------------------------------------------------------------------

const cells = (table) => {
  const out = [];
  table.forEach((row) => row.forEach((cell) => out.push(cell)));
  return out;
};

/**
 * A table fits in Markdown (GFM) when the first row is all headers, the others have none, and every
 * cell is a single paragraph without spans. Column widths set by resizing don't fit either.
 * (Same rules as tiptap-markdown, plus the widths.)
 */
function fitsMarkdown(table) {
  let ok = true;
  table.forEach((row, _, r) =>
    row.forEach((cell) => {
      const header = cell.type.name === 'tableHeader';
      if ((r === 0) !== header || cell.attrs.colspan > 1 || cell.attrs.rowspan > 1 || cell.childCount > 1) ok = false;
    })
  );
  return ok && !cells(table).some((c) => c.attrs.colwidth?.some((w) => w > 0));
}

/**
 * Markdown of a table: a GFM table, or an HTML block (which keeps the column widths in the
 * colwidth attribute of the cells) when it doesn't fit, like tiptap-markdown does for spans.
 */
export function serializeTable(state, node, parent) {
  if (!fitsMarkdown(node)) {
    const html = getHTMLFromFragment(Fragment.from(node), node.type.schema);
    const dom = new DOMParser().parseFromString(html, 'text/html');
    const table = dom.body.firstElementChild;
    // the widths are in the cells; the colgroup and inline sizes are recomputed from them
    table.querySelector('colgroup')?.remove();
    table.removeAttribute('style');
    // an HTML block must start and end on its own lines, without blank lines inside (CommonMark):
    // the only line breaks inside are in code, written as character references
    const inner = table.innerHTML.trim().replace(/\n/g, '&#10;');
    table.innerHTML = '';
    const open = table.outerHTML.slice(0, -'</table>'.length);
    state.write(parent?.type?.name === 'doc' || parent instanceof Fragment ? `${open}\n${inner}\n</table>` : html);
    state.closeBlock(node);
    return;
  }
  state.inTable = true;
  node.forEach((row, _, i) => {
    state.write('| ');
    row.forEach((cell, __, j) => {
      if (j) state.write(' | ');
      const content = cell.firstChild;
      if (content.textContent.trim()) state.renderInline(content);
    });
    state.write(' |');
    state.ensureNewLine();
    if (!i) {
      state.write(`| ${Array.from({ length: row.childCount }, () => '---').join(' | ')} |`);
      state.ensureNewLine();
    }
  });
  state.closeBlock(node);
  state.inTable = false;
}

/** Popup next to the cursor with a keyboard navigable list, for the "/" and "@" menus */
function suggestionPopup() {
  let component;
  let popup;
  const place = (clientRect) => {
    if (!popup || !clientRect) return;
    const rect = clientRect();
    if (!rect) return;
    const h = popup.offsetHeight || 300;
    let top = rect.bottom + 6;
    if (top + h > window.innerHeight - 8) top = Math.max(8, rect.top - h - 6);
    popup.style.top = `${top}px`;
    popup.style.left = `${Math.min(rect.left, window.innerWidth - 300)}px`;
  };
  return {
    onStart: (props) => {
      component = new ReactRenderer(CommandList, { props, editor: props.editor });
      popup = document.createElement('div');
      popup.className = 'slash-popup';
      document.body.appendChild(popup);
      popup.appendChild(component.element);
      requestAnimationFrame(() => place(props.clientRect));
    },
    onUpdate: (props) => {
      component?.updateProps(props);
      requestAnimationFrame(() => place(props.clientRect));
    },
    onKeyDown: (props) => {
      if (props.event.key === 'Escape') {
        popup?.remove();
        popup = null;
        return true;
      }
      return component?.ref?.onKeyDown(props) || false;
    },
    onExit: () => {
      popup?.remove();
      popup = null;
      component?.destroy();
    },
  };
}

/** "/" command menu */
export function createSlashCommand(getItems) {
  return Extension.create({
    name: 'slashCommand',
    addProseMirrorPlugins() {
      return [
        Suggestion({
          editor: this.editor,
          char: '/',
          pluginKey: new PluginKey('slashCommand'),
          allowSpaces: false,
          startOfLine: false,
          allow: ({ state, range }) => {
            const $from = state.doc.resolve(range.from);
            // not inside code
            return $from.parent.type.name !== 'codeBlock';
          },
          command: ({ editor, range, props }) => {
            // url: the link typed in the menu for an embed
            props.command({ editor, range, url: props.url });
          },
          items: ({ query }) => {
            const q = query.toLowerCase();
            return getItems()
              .filter(
                (i) =>
                  i.title.toLowerCase().includes(q) ||
                  (i.titleEn || '').toLowerCase().includes(q) ||
                  (i.keywords || '').toLowerCase().includes(q)
              )
              .slice(0, 20);
          },
          render: suggestionPopup,
        }),
      ];
    },
  });
}

// @[Label](mention://<mention id>/<user|document|collection>/<id>), the format Outline uses; the label escapes "]" and backslashes
const MENTION_MD = /^@\[((?:[^\]\\]|\\.)*)\]\(mention:\/\/([A-Za-z0-9_-]+)\/([a-z_]+)\/([0-9a-fA-F-]{36})\)/;

const escapeHtml = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Id of a new mention; crypto.randomUUID is missing on plain http */
export const newMentionId = () =>
  window.crypto?.randomUUID?.() || `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`;

/** Address of the document or collection a mention points to; none for people */
export const mentionHref = ({ type, modelId }) =>
  type === 'document' ? `/doc/${modelId}` : type === 'collection' ? `/collection/${modelId}` : null;

const mentionAttrs = ({ id, type, modelId }) => {
  const href = mentionHref({ type, modelId });
  return { class: `mention mention-${type}`, 'data-id': id, 'data-type': type, 'data-model-id': modelId, ...(href ? { href } : {}) };
};

/**
 * Mention, as in Outline. People (@Name) and groups (@Group, all of their members) are notified
 * when the document is published or the mention is added to a published document; documents and
 * collections are shown as chips that
 * open them with a click (or Enter when selected), also while editing.
 * Options: navigate(path), used to open documents and collections.
 */
export const Mention = Node.create({
  name: 'mention',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addOptions() {
    return { navigate: null };
  },

  addAttributes() {
    return {
      id: { default: null, parseHTML: (el) => el.getAttribute('data-id') },
      type: { default: 'user', parseHTML: (el) => el.getAttribute('data-type') || 'user' },
      modelId: { default: null, parseHTML: (el) => el.getAttribute('data-model-id') },
      label: {
        default: '',
        parseHTML: (el) => {
          const text = el.textContent || '';
          return mentionHref({ type: el.getAttribute('data-type') }) ? text : text.replace(/^@/, '');
        },
      },
    };
  },

  parseHTML() {
    // before the link mark, which would turn document and collection chips into plain links
    return [
      { tag: 'span.mention[data-model-id]', priority: 60 },
      { tag: 'a.mention[data-model-id]', priority: 60 },
    ];
  },

  renderHTML({ node }) {
    const { type, label } = node.attrs;
    if (!mentionHref(node.attrs)) return ['span', mentionAttrs(node.attrs), `@${label}`];
    return ['a', mentionAttrs(node.attrs), ['span', { class: 'mention-icon', contenteditable: 'false' }], label];
  },

  renderText({ node }) {
    return mentionHref(node.attrs) ? node.attrs.label : `@${node.attrs.label}`;
  },

  addKeyboardShortcuts() {
    return {
      Enter: ({ editor }) => {
        const { selection } = editor.state;
        if (!(selection instanceof NodeSelection) || selection.node.type.name !== this.name) return false;
        const href = mentionHref(selection.node.attrs);
        if (!href || !this.options.navigate) return false;
        this.options.navigate(href);
        return true;
      },
    };
  },

  addProseMirrorPlugins() {
    // every mention needs its own id (a copied mention is a new one), like Outline
    return [
      new Plugin({
        key: new PluginKey('mentionIds'),
        appendTransaction: (transactions, oldState, newState) => {
          if (!transactions.some((tr) => tr.docChanged)) return null;
          const tr = newState.tr;
          const seen = new Set();
          let modified = false;
          newState.doc.descendants((node, pos) => {
            if (node.type.name !== this.name) return;
            let { id } = node.attrs;
            if (!id || seen.has(id)) {
              id = newMentionId();
              tr.setNodeAttribute(pos, 'id', id);
              modified = true;
            }
            seen.add(id);
          });
          return modified ? tr : null;
        },
      }),
    ];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state, node) {
          const { id, type, modelId, label } = node.attrs;
          state.write(`@[${String(label).replace(/[\\\]]/g, '\\$&')}](mention://${id}/${type}/${modelId})`);
        },
        parse: {
          setup(md) {
            md.inline.ruler.before('link', 'mention', (state, silent) => {
              if (state.src.charCodeAt(state.pos) !== 0x40 /* @ */) return false;
              const m = MENTION_MD.exec(state.src.slice(state.pos));
              if (!m) return false;
              if (!silent) {
                const token = state.push('mention', 'span', 0);
                token.meta = { label: m[1].replace(/\\(.)/g, '$1'), id: m[2], type: m[3], modelId: m[4] };
              }
              state.pos += m[0].length;
              return true;
            });
            md.renderer.rules.mention = (tokens, idx) => {
              const { label, ...attrs } = tokens[idx].meta;
              const html = Object.entries(mentionAttrs(attrs))
                .map(([k, v]) => `${k}="${escapeHtml(v)}"`)
                .join(' ');
              return mentionHref(attrs) ? `<a ${html}>${escapeHtml(label)}</a>` : `<span ${html}>@${escapeHtml(label)}</span>`;
            };
          },
        },
      },
    };
  },
});

/** "@" menu: people, groups, documents and collections; getItems(query) returns a promise of menu items */
export function createMentionSuggestion(getItems) {
  return Extension.create({
    name: 'mentionSuggestion',
    addProseMirrorPlugins() {
      return [
        Suggestion({
          editor: this.editor,
          char: '@',
          pluginKey: new PluginKey('mentionSuggestion'),
          allowSpaces: true,
          startOfLine: false,
          allow: ({ state, range }) => {
            const $from = state.doc.resolve(range.from);
            if ($from.parent.type.name === 'codeBlock') return false;
            return !$from.marks().some((m) => m.type.name === 'code');
          },
          command: ({ editor, range, props }) => {
            props.command({ editor, range });
          },
          items: ({ query }) => getItems(query),
          render: suggestionPopup,
        }),
      ];
    },
  });
}

// ---------------------------------------------------------------------------
// comments on a passage of the text
// ---------------------------------------------------------------------------

/**
 * The text of the document as one string, with the document position of every character (-1 for
 * the spaces put between blocks). Anchors of comments are searched in this text.
 */
export function flatText(doc) {
  let text = '';
  const pos = [];
  doc.descendants((node, p) => {
    if (node.isText) {
      for (let i = 0; i < node.text.length; i++) pos.push(p + i);
      text += node.text;
    } else if ((node.isBlock || node.isLeaf) && text && !text.endsWith(' ')) {
      text += ' ';
      pos.push(-1);
    }
    return true;
  });
  return { text, pos };
}

const ANCHOR_CONTEXT = 40;

/** The anchor of a comment on the selection from..to: its text with some context around it */
export function anchorFor(doc, from, to) {
  const { text, pos } = flatText(doc);
  let a = pos.findIndex((p) => p >= from);
  let b = pos.length;
  for (let i = pos.length - 1; i >= 0; i--) {
    if (pos[i] !== -1 && pos[i] < to) {
      b = i + 1;
      break;
    }
  }
  if (a === -1 || b <= a) return null;
  const quoted = text.slice(a, b).trim();
  if (!quoted) return null;
  a = text.indexOf(quoted, a);
  return { text: quoted, prefix: text.slice(Math.max(0, a - ANCHOR_CONTEXT), a), suffix: text.slice(a + quoted.length, a + quoted.length + ANCHOR_CONTEXT) };
}

const commonEnd = (a, b) => {
  let n = 0;
  while (n < a.length && n < b.length && a[a.length - 1 - n] === b[b.length - 1 - n]) n++;
  return n;
};
const commonStart = (a, b) => {
  let n = 0;
  while (n < a.length && n < b.length && a[n] === b[n]) n++;
  return n;
};

/** Where the anchor is now: the occurrence of its text whose context matches best, or null */
export function locateAnchor(flat, anchor) {
  const { text, pos } = flat;
  if (!anchor?.text) return null;
  let best = -1;
  let bestScore = -1;
  for (let i = text.indexOf(anchor.text); i !== -1; i = text.indexOf(anchor.text, i + 1)) {
    const score =
      commonEnd(text.slice(Math.max(0, i - ANCHOR_CONTEXT), i), anchor.prefix || '') +
      commonStart(text.slice(i + anchor.text.length, i + anchor.text.length + ANCHOR_CONTEXT), anchor.suffix || '');
    if (score > bestScore) {
      best = i;
      bestScore = score;
    }
  }
  if (best === -1) return null;
  let first = best;
  while (first < best + anchor.text.length && pos[first] === -1) first++;
  let last = best + anchor.text.length - 1;
  while (last > first && pos[last] === -1) last--;
  if (pos[first] === -1) return null;
  return { from: pos[first], to: pos[last] + 1 };
}

export const commentAnchorsKey = new PluginKey('commentAnchors');

/**
 * Highlights the passages that comments are about. The anchors are set with
 * editor.commands.setCommentAnchors([{ id, text, prefix, suffix }], activeId); clicking a highlight
 * calls editor.storage.commentAnchors.onClick(id).
 */
export const CommentAnchors = Extension.create({
  name: 'commentAnchors',

  addStorage() {
    return { anchors: [], active: null, onClick: null };
  },

  addCommands() {
    return {
      setCommentAnchors:
        (anchors, active = null) =>
        ({ tr, dispatch }) => {
          this.storage.anchors = anchors || [];
          this.storage.active = active;
          if (dispatch) tr.setMeta(commentAnchorsKey, true).setMeta('addToHistory', false);
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    const storage = this.storage;
    const build = (doc) => {
      if (!storage.anchors.length) return DecorationSet.empty;
      const flat = flatText(doc);
      const decorations = [];
      for (const a of storage.anchors) {
        const range = locateAnchor(flat, a);
        if (!range) continue;
        const cls = `comment-anchor${a.id === storage.active ? ' active' : ''}`;
        decorations.push(Decoration.inline(range.from, range.to, { class: cls, 'data-comment-id': a.id }));
      }
      return DecorationSet.create(doc, decorations);
    };
    return [
      new Plugin({
        key: commentAnchorsKey,
        state: {
          init: (_, state) => build(state.doc),
          apply: (tr, old, _, state) => (tr.getMeta(commentAnchorsKey) ? build(state.doc) : tr.docChanged ? build(state.doc) : old),
        },
        props: {
          decorations(state) {
            return this.getState(state);
          },
          handleDOMEvents: {
            click: (view, event) => {
              const el = event.target.closest?.('[data-comment-id]');
              if (el && storage.onClick) storage.onClick(el.getAttribute('data-comment-id'));
              return false;
            },
          },
        },
      }),
    ];
  },
});

// ---------------------------------------------------------------------------
// embeds
// ---------------------------------------------------------------------------

/**
 * YouTube, Figma, Google Docs… shown in the document (see embeds.js). Stored in markdown as a
 * link whose text is its address, alone in a paragraph, as in Outline: other tools show a link.
 */
export const Embed = Node.create({
  name: 'embed',
  group: 'block',
  atom: true,
  draggable: true,

  addAttributes() {
    return {
      url: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-url') || '',
        renderHTML: (attrs) => ({ 'data-url': attrs.url }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="embed"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes({ 'data-type': 'embed' }, HTMLAttributes)];
  },

  addNodeView() {
    return ReactNodeViewRenderer(EmbedView);
  },

  addCommands() {
    return {
      insertEmbed:
        (url) =>
        ({ commands }) =>
          embedFor(url) ? commands.insertContent({ type: this.name, attrs: { url } }) : false,
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state, node) {
          const url = node.attrs.url.replace(/[()\s]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase().padStart(2, '0')}`);
          state.write(`[${url}](${url})`);
          state.closeBlock(node);
        },
        parse: {
          setup(md) {
            // a paragraph with only [url](url) of an embeddable service; bare and auto links stay links
            md.core.ruler.after('inline', 'embed', (state) => {
              const tokens = state.tokens;
              for (let i = 0; i + 2 < tokens.length; i++) {
                if (tokens[i].type !== 'paragraph_open' || tokens[i + 1].type !== 'inline' || tokens[i + 2].type !== 'paragraph_close') continue;
                const ch = (tokens[i + 1].children || []).filter((c) => !(c.type === 'text' && !c.content.trim()));
                if (ch.length !== 3 || ch[0].type !== 'link_open' || ch[1].type !== 'text' || ch[2].type !== 'link_close') continue;
                if (ch[0].markup === 'linkify' || ch[0].markup === 'autolink') continue;
                const href = ch[0].attrGet('href');
                if (!href || ch[1].content !== href || !embedFor(href)) continue;
                const token = new state.Token('embed', 'div', 0);
                token.meta = { url: href };
                token.block = true;
                tokens.splice(i, 3, token);
              }
            });
            md.renderer.rules.embed = (tokens, idx) => `<div data-type="embed" data-url="${escapeHtml(tokens[idx].meta.url)}"></div>\n`;
          },
        },
      },
    };
  },
});

