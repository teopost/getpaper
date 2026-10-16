import { Extension, Node, mergeAttributes } from '@tiptap/core';
import { ReactNodeViewRenderer, ReactRenderer } from '@tiptap/react';
import Suggestion from '@tiptap/suggestion';
import { NodeSelection, Plugin, PluginKey } from '@tiptap/pm/state';
import container from 'markdown-it-container';
import CommandList from './CommandList';
import TocView from './TocView';

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
            props.command({ editor, range });
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
