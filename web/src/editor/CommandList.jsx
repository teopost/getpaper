import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { t } from '../i18n';

const CommandList = forwardRef(function CommandList({ items, command }, ref) {
  const [selected, setSelected] = useState(0);
  const listRef = useRef(null);

  useEffect(() => setSelected(0), [items]);

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-index="${selected}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const select = (i) => {
    const item = items[i];
    if (item) command(item);
  };

  useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
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

  if (!items.length) return <div className="command-list empty-commands">{t('No results')}</div>;

  return (
    <div className="command-list" ref={listRef}>
      {items.map((item, i) => (
        <React.Fragment key={item.title}>
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
