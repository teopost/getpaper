import { Extension, Node, mergeAttributes } from '@tiptap/core';
import { ReactRenderer } from '@tiptap/react';
import Suggestion from '@tiptap/suggestion';
import { PluginKey } from '@tiptap/pm/state';
import container from 'markdown-it-container';
import CommandList from './CommandList';

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
                  (i.keywords || '').toLowerCase().includes(q)
              )
              .slice(0, 20);
          },
          render: () => {
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
          },
        }),
      ];
    },
  });
}
