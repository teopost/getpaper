import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ArrowLeft, ArrowRight, Maximize, Minimize, X } from 'lucide-react';
import Editor from '../editor/Editor';
import { t } from '../i18n';

const IDLE_MS = 3000;
// activity that shows the controls again; the keyboard is left out so they stay hidden while presenting
const IDLE_EVENTS = ['click', 'mousemove', 'mousedown', 'touchstart', 'touchmove'];

const isEmpty = (nodes) => nodes.every((n) => n.type === 'paragraph' && !n.content?.length);

/**
 * Slides of a document, as in Outline: a title slide, then a new slide at every big or medium
 * heading (h1, h2) and at every divider (the divider itself is not shown).
 */
function splitIntoSlides(doc, title, emoji) {
  const slides = [{ type: 'title', title, emoji }];
  let current = [];
  for (const node of doc.content || []) {
    if (node.type === 'horizontalRule') {
      if (current.length) slides.push({ type: 'content', content: current });
      current = [];
      continue;
    }
    if (node.type === 'heading' && node.attrs?.level <= 2 && current.length) {
      slides.push({ type: 'content', content: current });
      current = [];
    }
    current.push(node);
  }
  if (current.length) slides.push({ type: 'content', content: current });
  const hasContent = slides.some((s) => s.type === 'content' && !isEmpty(s.content));
  return hasContent ? slides : [slides[0], { type: 'instructions' }];
}

function useIdle(ms) {
  const [idle, setIdle] = useState(false);
  useEffect(() => {
    let timer;
    const wake = () => {
      setIdle(false);
      clearTimeout(timer);
      timer = setTimeout(() => setIdle(true), ms);
    };
    wake();
    IDLE_EVENTS.forEach((e) => window.addEventListener(e, wake, { passive: true }));
    return () => {
      clearTimeout(timer);
      IDLE_EVENTS.forEach((e) => window.removeEventListener(e, wake));
    };
  }, [ms]);
  return idle;
}

/**
 * Full screen presentation of a document (content: the editor JSON). Arrows, space and Page Up/Down
 * move between slides, Home/End jump to the first/last one, F toggles full screen, Esc closes.
 */
export default function PresentationMode({ title, emoji, content, onClose }) {
  const slides = useMemo(() => splitIntoSlides(content, title || t('Untitled'), emoji), [content, title, emoji]);
  const [current, setCurrent] = useState(0);
  const [fullscreen, setFullscreen] = useState(false);
  const containerRef = useRef(null);
  const slideRef = useRef(null);
  const idle = useIdle(IDLE_MS);
  const total = slides.length;
  const canFullscreen = !!document.documentElement.requestFullscreen;

  const next = useCallback(() => setCurrent((c) => Math.min(c + 1, total - 1)), [total]);
  const prev = useCallback(() => setCurrent((c) => Math.max(c - 1, 0)), []);
  const toggleFullscreen = useCallback(() => {
    if (!canFullscreen) return;
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else containerRef.current?.requestFullscreen().catch(() => {});
  }, [canFullscreen]);

  useEffect(() => {
    const onKey = (e) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const actions = {
        Escape: onClose,
        ArrowRight: next,
        ArrowDown: next,
        PageDown: next,
        ' ': next,
        ArrowLeft: prev,
        ArrowUp: prev,
        PageUp: prev,
        Home: () => setCurrent(0),
        End: () => setCurrent(total - 1),
        f: toggleFullscreen,
      };
      const action = actions[e.key];
      if (!action) return;
      // Esc in full screen only leaves full screen (the browser does it)
      if (e.key === 'Escape' && document.fullscreenElement) return;
      e.preventDefault();
      e.stopPropagation();
      action();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [next, prev, total, toggleFullscreen, onClose]);

  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const onChange = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', onChange);
    return () => {
      document.body.style.overflow = previous;
      document.removeEventListener('fullscreenchange', onChange);
      if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    };
  }, []);

  // scale every slide to fill the screen: measured once at its natural size, then on resize
  useEffect(() => {
    const el = slideRef.current;
    const container = containerRef.current;
    if (!el || !container) return undefined;
    let natural = { width: 0, height: 0 };
    const apply = () => {
      if (!natural.width || !natural.height) {
        el.style.transform = 'scale(1)';
        return;
      }
      const scale = Math.min((container.clientWidth - 160) / natural.width, (container.clientHeight - 160) / natural.height, 1.5);
      el.style.transform = `scale(${Math.max(scale, 0.5)})`;
    };
    el.style.transform = 'none';
    const frame = requestAnimationFrame(() => {
      natural = { width: el.scrollWidth, height: el.scrollHeight };
      apply();
    });
    window.addEventListener('resize', apply);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('resize', apply);
    };
  }, [current]);

  const slide = slides[current];
  const slideDoc = useMemo(() => (slide.type === 'content' ? { type: 'doc', content: slide.content } : null), [slide]);

  return createPortal(
    <div ref={containerRef} className={`presentation ${idle ? 'idle' : ''}`}>
      <div className="presentation-bar">
        <div className="presentation-nav">
          <button className="presentation-btn" title={t('Previous slide')} onClick={prev} disabled={current === 0}>
            <ArrowLeft size={18} />
          </button>
          <span className="presentation-counter">
            {current + 1} / {total}
          </span>
          <button className="presentation-btn" title={t('Next slide')} onClick={next} disabled={current === total - 1}>
            <ArrowRight size={18} />
          </button>
        </div>
        <div className="presentation-nav">
          {canFullscreen && (
            <button className="presentation-btn" title={t('Toggle full screen')} onClick={toggleFullscreen}>
              {fullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
            </button>
          )}
          <button className="presentation-btn" title={t('Close')} onClick={onClose}>
            <X size={18} />
          </button>
        </div>
      </div>
      <div
        className="presentation-area"
        onClick={(e) => {
          if (!e.target.closest('a, button, input')) next();
        }}
      >
        <div ref={slideRef} className="presentation-slide">
          {slide.type === 'title' && (
            <div className="presentation-title">
              {slide.emoji && <div className="presentation-emoji">{slide.emoji}</div>}
              <h1>{slide.title}</h1>
            </div>
          )}
          {slide.type === 'instructions' && (
            <div className="presentation-instructions">
              <h2>{t('Create your presentation')}</h2>
              <p className="muted">{t('Add content to your document, then use headings or dividers to separate it into slides.')}</p>
            </div>
          )}
          {slideDoc && <Editor key={current} value={slideDoc} editable={false} placeholder="" />}
        </div>
      </div>
    </div>,
    document.body
  );
}
