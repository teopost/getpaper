import React, { useEffect, useState, useSyncExternalStore } from 'react';
import { t } from '../i18n';

// Mermaid diagrams, as in Outline: a code block with language "mermaid" is drawn as a diagram.
// The library is large, so it is loaded the first time a diagram is shown.

let mermaidPromise;
function loadMermaid() {
  mermaidPromise ||= import('mermaid').then((m) => m.default);
  return mermaidPromise;
}

// ---- dark theme: the user's choice (data-theme on <html>), else the OS setting
const darkQuery = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
function isDark() {
  const theme = document.documentElement.dataset.theme;
  if (theme) return theme === 'dark';
  return !!darkQuery?.matches;
}
function subscribeTheme(callback) {
  const observer = new MutationObserver(callback);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  darkQuery?.addEventListener('change', callback);
  return () => {
    observer.disconnect();
    darkQuery?.removeEventListener('change', callback);
  };
}
function useDark() {
  return useSyncExternalStore(subscribeTheme, isDark);
}

// ---- rendering: one diagram at a time, because mermaid.initialize (theme) is global
const cache = new Map();
const MAX_CACHE = 50;
let queue = Promise.resolve();
let counter = 0;

function render(text, dark) {
  const key = `${dark ? 'dark' : 'light'}\n${text}`;
  if (cache.has(key)) return Promise.resolve(cache.get(key));
  const job = queue.then(async () => {
    const mermaid = await loadMermaid();
    mermaid.initialize({
      startOnLoad: false,
      suppressErrorRendering: true,
      securityLevel: 'strict',
      theme: dark ? 'dark' : 'default',
      darkMode: dark,
      fontFamily: getComputedStyle(document.body).fontFamily || 'inherit',
    });
    const id = `mermaid-${++counter}`;
    try {
      const { svg } = await mermaid.render(id, text);
      if (cache.size >= MAX_CACHE) cache.delete(cache.keys().next().value);
      cache.set(key, svg);
      return svg;
    } finally {
      // mermaid can leave its temporary element behind when the diagram has errors
      document.getElementById(`d${id}`)?.remove();
      document.getElementById(id)?.remove();
    }
  });
  queue = job.catch(() => {});
  return job;
}

/** The diagram drawn from `text`; `onClick` lets the editor open the source */
export default function MermaidDiagram({ text, onClick, live }) {
  const dark = useDark();
  const [svg, setSvg] = useState(() => cache.get(`${dark ? 'dark' : 'light'}\n${text}`) || '');
  const [error, setError] = useState('');
  const empty = !text.trim();

  useEffect(() => {
    if (empty) {
      setSvg('');
      setError('');
      return undefined;
    }
    let cancelled = false;
    // while the source is being edited, wait for a pause in typing
    const timer = setTimeout(
      () => {
        render(text, dark).then(
          (result) => {
            if (cancelled) return;
            setSvg(result);
            setError('');
          },
          (e) => {
            if (!cancelled) setError(String(e?.message || e));
          }
        );
      },
      live ? 400 : 0
    );
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [text, dark, empty, live]);

  return (
    <div
      className={`mermaid-diagram ${onClick ? 'clickable' : ''}`}
      contentEditable={false}
      onMouseDown={
        onClick
          ? (e) => {
              e.preventDefault();
              onClick();
            }
          : undefined
      }
    >
      {empty ? (
        <div className="mermaid-diagram-message">{t('Empty diagram')}</div>
      ) : error ? (
        <div className="mermaid-diagram-message error">{error}</div>
      ) : svg ? (
        <div className="mermaid-diagram-svg" dangerouslySetInnerHTML={{ __html: svg }} />
      ) : (
        <div className="mermaid-diagram-message">{t('Loading diagram…')}</div>
      )}
    </div>
  );
}
