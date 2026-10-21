import React, { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { FileText, Globe } from 'lucide-react';
import { api, timeAgo } from '../api';
import { useApp } from '../store';
import { t } from '../i18n';

const DELAY = 450;
const cache = new Map();

/** "/doc/some-title-AbC123" → "AbC123" */
const urlIdOf = (href) => /^\/doc\/(?:.*-)?([A-Za-z0-9]{10})(?:[#?].*)?$/.exec(href)?.[1];

function excerpt(markdown) {
  return markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/@\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_`~|:-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 180);
}

async function load(href, externalAllowed) {
  if (cache.has(href)) return cache.get(href);
  let preview = null;
  const urlId = href.startsWith('/') ? urlIdOf(href) : null;
  if (urlId) {
    const r = await api('documents.info', { id: urlId, preview: true }).catch(() => null);
    const d = r?.document;
    if (d) preview = { internal: true, title: `${d.emoji ? `${d.emoji} ` : ''}${d.title || t('Untitled')}`, description: excerpt(d.text || ''), meta: t('Updated {time}', { time: timeAgo(d.updatedAt) }) };
  } else if (externalAllowed && /^https?:\/\//.test(href)) {
    const r = await api('urls.unfurl', { url: href }).catch(() => null);
    if (r?.title) preview = { internal: false, title: r.title, description: r.description, meta: r.siteName };
  }
  cache.set(href, preview);
  return preview;
}

/**
 * Preview of the link under the pointer, as in Outline: documents of the workspace always,
 * external pages only when a superadmin turned link previews on.
 */
export default function LinkPreview({ containerRef }) {
  const app = useApp();
  const externalAllowed = !!app?.instance?.linkPreviews;
  const [card, setCard] = useState(null);
  const timer = useRef(null);
  const current = useRef(null);

  useEffect(() => {
    const root = containerRef.current;
    if (!root || !app) return undefined;
    const hide = () => {
      clearTimeout(timer.current);
      current.current = null;
      setCard(null);
    };
    const over = (e) => {
      const a = e.target.closest?.('a[href]');
      if (!a || !root.contains(a) || a === current.current) return;
      const href = a.getAttribute('href');
      if (!href || href.startsWith('/files/') || href.startsWith('#') || href.startsWith('mailto:')) return;
      current.current = a;
      clearTimeout(timer.current);
      timer.current = setTimeout(async () => {
        const preview = await load(href, externalAllowed);
        if (current.current !== a || !preview) return;
        const r = a.getBoundingClientRect();
        setCard({ ...preview, top: r.bottom + 6, left: Math.min(r.left, window.innerWidth - 340) });
      }, DELAY);
    };
    const out = (e) => {
      const a = e.target.closest?.('a[href]');
      if (a && a === current.current && !a.contains(e.relatedTarget)) hide();
    };
    root.addEventListener('mouseover', over);
    root.addEventListener('mouseout', out);
    window.addEventListener('scroll', hide, true);
    return () => {
      clearTimeout(timer.current);
      root.removeEventListener('mouseover', over);
      root.removeEventListener('mouseout', out);
      window.removeEventListener('scroll', hide, true);
    };
  }, [containerRef, app, externalAllowed]);

  if (!card) return null;
  return createPortal(
    <div className="link-preview" style={{ top: card.top, left: Math.max(8, card.left) }} role="tooltip">
      <div className="link-preview-title">
        {card.internal ? <FileText size={14} /> : <Globe size={14} />}
        <span>{card.title}</span>
      </div>
      {card.description && <div className="link-preview-text">{card.description}</div>}
      {card.meta && <div className="link-preview-meta">{card.meta}</div>}
    </div>,
    document.body
  );
}
