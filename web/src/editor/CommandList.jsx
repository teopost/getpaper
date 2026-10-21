import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { embedFor } from './embeds';
import { t } from '../i18n';

/**
 * The link of a service to embed, asked inside the "/" menu as in Outline: Enter or pasting a
 * link of that service inserts it, Escape closes the menu and goes back to the text.
 */
function EmbedLinkInput({ item, editor, range, onDone }) {
  const [error, setError] = useState(null);
  const inputRef = useRef(null);
  // inserted or closed: later events (the blur when the menu disappears) must not touch the document
  const finished = useRef(false);
  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  const service = item.embed.service;
  const insert = (value) => {
    const url = value.trim();
    if (!url) return false;
    if (!embedFor(url, service)) {
      setError(service ? t('This is not a {service} link.', { service }) : t('This link can’t be embedded.'));
      return false;
    }
    if (finished.current) return false;
    finished.current = true;
    onDone(url);
    return true;
  };
  const close = () => {
    if (finished.current || editor.isDestroyed) return;
    finished.current = true;
    // removing "/query" ends the suggestion, which closes the menu; only if it is still there
    const { doc } = editor.state;
    const stillThere = range.to <= doc.content.size && doc.textBetween(range.from, range.to).startsWith('/');
    if (stillThere) editor.chain().focus().deleteRange(range).run();
    else editor.commands.focus();
  };
  return (
    <div className="command-list embed-link">
      <input
        ref={inputRef}
        type="url"
        className="embed-link-input"
        placeholder={service ? t('Paste a {service} link…', { service }) : t('Paste a link…')}
        aria-label={item.title}
        onChange={() => setError(null)}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === 'Enter') {
            e.preventDefault();
            insert(e.currentTarget.value);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            close();
          }
        }}
        onPaste={(e) => {
          const pasted = e.clipboardData.getData('text/plain');
          if (embedFor(pasted.trim(), service)) {
            e.preventDefault();
            insert(pasted);
          }
        }}
        onBlur={(e) => {
          // clicking back in the document closes the menu
          if (!e.relatedTarget?.closest?.('.slash-popup')) setTimeout(close, 0);
        }}
      />
      {error ? (
        <div className="embed-link-error">{error}</div>
      ) : (
        <div className="embed-link-hint">{t('Enter to embed, Esc to cancel')}</div>
      )}
    </div>
  );
}

const CommandList = forwardRef(function CommandList({ items, command, editor, range }, ref) {
  const [selected, setSelected] = useState(0);
  // a service to embed was chosen: the menu asks for its link
  const [linkItem, setLinkItem] = useState(null);
  const listRef = useRef(null);

  useEffect(() => setSelected(0), [items]);

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-index="${selected}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const select = (i) => {
    const item = items[i];
    if (!item) return;
    if (item.embed) setLinkItem(item);
    else command(item);
  };

  useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
      if (linkItem) return false;
      if (event.key === 'ArrowUp') {
        setSelected((s) => (s + items.length - 1) % Math.max(items.length, 1));
        return true;
      }
      if (event.key === 'ArrowDown') {
        setSelected((s) => (s + 1) % Math.max(items.length, 1));
        return true;
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        if (!items.length) return false;
        select(selected);
        return true;
      }
      return false;
    },
  }));

  if (linkItem) {
    return <EmbedLinkInput item={linkItem} editor={editor} range={range} onDone={(url) => command({ ...linkItem, url })} />;
  }

  if (!items.length) return <div className="command-list empty-commands">{t('No results')}</div>;

  return (
    <div className="command-list" ref={listRef}>
      {items.map((item, i) => (
        <React.Fragment key={item.key || item.title}>
          {item.section && (i === 0 || items[i - 1].section !== item.section) && (
            <div className="command-section">{item.section}</div>
          )}
          <button
            data-index={i}
            className={`command-item ${i === selected ? 'selected' : ''}`}
            onMouseEnter={() => setSelected(i)}
            onMouseDown={(e) => {
              e.preventDefault();
              select(i);
            }}
          >
            <span className="command-icon">{item.icon}</span>
            <span className="command-title">{item.title}</span>
            {item.shortcut && <span className="command-shortcut">{item.shortcut}</span>}
          </button>
        </React.Fragment>
      ))}
    </div>
  );
});

export default CommandList;
