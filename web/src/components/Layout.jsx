import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Menu as MenuIcon } from 'lucide-react';
import Sidebar from './Sidebar';
import SettingsSidebar from './SettingsSidebar';
import { t } from '../i18n';

const SIDEBAR_DEFAULT = 260;
const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 600;
const WIDTH_KEY = 'sidebar-width';

function clampWidth(w) {
  const max = Math.min(SIDEBAR_MAX, Math.round(window.innerWidth * 0.6));
  return Math.round(Math.min(Math.max(w, SIDEBAR_MIN), Math.max(max, SIDEBAR_MIN)));
}

/** Sidebar width, changed by dragging its edge and remembered in this browser. */
function useSidebarWidth() {
  const [width, setWidth] = useState(() => {
    try {
      const saved = parseInt(localStorage.getItem(WIDTH_KEY), 10);
      return saved ? clampWidth(saved) : SIDEBAR_DEFAULT;
    } catch {
      return SIDEBAR_DEFAULT;
    }
  });
  const save = useCallback((w) => {
    try {
      localStorage.setItem(WIDTH_KEY, String(w));
    } catch {
      /* storage unavailable: the width lasts for this page only */
    }
  }, []);
  return [width, setWidth, save];
}

function SidebarResizer({ width, setWidth, save }) {
  const dragging = useRef(false);

  const onPointerDown = (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragging.current = true;
    document.body.classList.add('resizing-sidebar');
  };
  const onPointerMove = (e) => {
    // the sidebar starts at the left edge of the window
    if (dragging.current) setWidth(clampWidth(e.clientX));
  };
  const onPointerUp = (e) => {
    if (!dragging.current) return;
    dragging.current = false;
    document.body.classList.remove('resizing-sidebar');
    const w = clampWidth(e.clientX);
    setWidth(w);
    save(w);
  };
  const onKeyDown = (e) => {
    const step = e.shiftKey ? 64 : 16;
    let w = null;
    if (e.key === 'ArrowLeft') w = width - step;
    else if (e.key === 'ArrowRight') w = width + step;
    else if (e.key === 'Home') w = SIDEBAR_DEFAULT;
    if (w === null) return;
    e.preventDefault();
    w = clampWidth(w);
    setWidth(w);
    save(w);
  };
  const reset = () => {
    setWidth(SIDEBAR_DEFAULT);
    save(SIDEBAR_DEFAULT);
  };

  return (
    <div
      className="sidebar-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label={t('Resize sidebar')}
      aria-valuenow={width}
      aria-valuemin={SIDEBAR_MIN}
      aria-valuemax={SIDEBAR_MAX}
      title={t('Drag to resize, double-click to reset')}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={reset}
      onKeyDown={onKeyDown}
    />
  );
}

export default function Layout({ children }) {
  const location = useLocation();
  const navigate = useNavigate();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [sidebarWidth, setSidebarWidth, saveSidebarWidth] = useSidebarWidth();
  const inSettings = location.pathname.startsWith('/settings');

  useEffect(() => setMobileOpen(false), [location.pathname]);

  useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        navigate('/search');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [navigate]);

  return (
    <div className={`layout ${mobileOpen ? 'mobile-open' : ''}`} style={{ '--sidebar-width': `${sidebarWidth}px` }}>
      <button className="mobile-menu-btn" onClick={() => setMobileOpen((o) => !o)} aria-label={t('Menu')}>
        <MenuIcon size={20} />
      </button>
      {inSettings ? <SettingsSidebar /> : <Sidebar />}
      <SidebarResizer width={sidebarWidth} setWidth={setSidebarWidth} save={saveSidebarWidth} />
      <main className="main">{children}</main>
    </div>
  );
}
