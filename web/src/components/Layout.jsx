import React, { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Menu as MenuIcon } from 'lucide-react';
import Sidebar from './Sidebar';
import SettingsSidebar from './SettingsSidebar';

export default function Layout({ children }) {
  const location = useLocation();
  const navigate = useNavigate();
  const [mobileOpen, setMobileOpen] = useState(false);
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
    <div className={`layout ${mobileOpen ? 'mobile-open' : ''}`}>
      <button className="mobile-menu-btn" onClick={() => setMobileOpen((o) => !o)} aria-label="Menu">
        <MenuIcon size={20} />
      </button>
      {inSettings ? <SettingsSidebar /> : <Sidebar />}
      <main className="main">{children}</main>
    </div>
  );
}
