import React, { useEffect, useState } from 'react';
import { NodeViewWrapper } from '@tiptap/react';
import { t } from '../i18n';

function collectHeadings(doc) {
  const headings = [];
  doc.descendants((node, pos) => {
    if (node.type.name === 'heading') {
      const text = node.textContent.trim();
      if (text) headings.push({ level: node.attrs.level, text, pos });
    }
    // headings only live at the top level or inside container blocks, never inside text
    return !node.isTextblock;
  });
  return headings;
}

/** Table of contents block: a live list of the document's headings, clicking one scrolls to it. */
export default function TocView({ editor, selected }) {
  const [headings, setHeadings] = useState(() => collectHeadings(editor.state.doc));

  useEffect(() => {
    const refresh = () => setHeadings(collectHeadings(editor.state.doc));
    editor.on('update', refresh);
    return () => editor.off('update', refresh);
  }, [editor]);

  const minLevel = Math.min(...headings.map((h) => h.level));

  const go = (e, pos) => {
    e.preventDefault();
    const dom = editor.view.nodeDOM(pos);
    if (dom instanceof HTMLElement) dom.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  return (
    <NodeViewWrapper className={`toc ${selected ? 'is-selected' : ''}`} contentEditable={false}>
      <div className="toc-title">{t('Contents')}</div>
      {headings.length ? (
        <ul>
          {headings.map((h) => (
            <li key={h.pos} style={{ paddingLeft: `${(h.level - minLevel) * 1.25}em` }}>
              {/* a button, not a link: the editor opens every a[href] it sees clicked */}
              <button type="button" onClick={(e) => go(e, h.pos)}>
                {h.text}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <div className="toc-empty">{t('Add headings to the document to build the table of contents.')}</div>
      )}
    </NodeViewWrapper>
  );
}
