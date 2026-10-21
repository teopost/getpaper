import React, { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { PinOff } from 'lucide-react';
import { api, docUrl, timeAgo } from '../api';
import { DocIcon } from './icons';
import { useToast } from './Toast';
import { t } from '../i18n';

/** Plain text of the beginning of a document, for its card */
function excerpt(markdown) {
  return (markdown || '')
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/@\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_`~|:-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 200);
}

/**
 * Documents pinned at the top of the home page (no collectionId) or of a collection, as in Outline.
 * Who can pin them reorders them by dragging and unpins them from the card.
 */
export default function PinnedDocs({ collectionId }) {
  const [pins, setPins] = useState([]);
  const [canPin, setCanPin] = useState(false);
  const [dragging, setDragging] = useState(null);
  const [over, setOver] = useState(null);
  const toast = useToast();

  const load = useCallback(() => {
    api('pins.list', collectionId ? { collectionId } : {})
      .then((r) => {
        setPins(r.pins);
        setCanPin(r.canPin);
      })
      .catch(() => setPins([]));
  }, [collectionId]);
  useEffect(load, [load]);

  if (!pins.length) return null;

  const drop = async (index) => {
    const id = dragging;
    setDragging(null);
    setOver(null);
    const from = pins.findIndex((p) => p.id === id);
    if (from < 0 || from === index) return;
    const next = [...pins];
    const [moved] = next.splice(from, 1);
    next.splice(index, 0, moved);
    setPins(next);
    try {
      await api('pins.update', { id, index });
    } catch (e) {
      toast(e.message, 'error');
      load();
    }
  };

  const unpin = async (e, pin) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await api('pins.delete', { id: pin.id });
      setPins((p) => p.filter((x) => x.id !== pin.id));
    } catch (err) {
      toast(err.message, 'error');
    }
  };

  return (
    <section className="pinned" aria-label={t('Pinned')}>
      <div className="pinned-grid">
        {pins.map((p, i) => (
          <Link
            key={p.id}
            to={docUrl(p.document)}
            className={`pinned-card ${dragging === p.id ? 'dragging' : ''} ${over === i && dragging !== p.id ? 'drop-target' : ''}`}
            draggable={canPin}
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = 'move';
              setDragging(p.id);
            }}
            onDragEnd={() => {
              setDragging(null);
              setOver(null);
            }}
            onDragOver={(e) => {
              if (!dragging) return;
              e.preventDefault();
              setOver(i);
            }}
            onDrop={(e) => {
              e.preventDefault();
              drop(i);
            }}
          >
            <span className="pinned-card-title">
              <DocIcon doc={p.document} size={16} />
              <span>{p.document.title || t('Untitled')}</span>
            </span>
            <span className="pinned-card-text">{excerpt(p.document.text)}</span>
            <span className="pinned-card-meta">{t('Updated {time}', { time: timeAgo(p.document.updatedAt) })}</span>
            {canPin && (
              <button className="icon-btn pinned-unpin" title={t('Unpin')} onClick={(e) => unpin(e, p)}>
                <PinOff size={14} />
              </button>
            )}
          </Link>
        ))}
      </div>
    </section>
  );
}
