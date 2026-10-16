import React from 'react';
import { NavLink, Link } from 'react-router-dom';
import { ChevronLeft, User, Building2, Users, UsersRound, Globe, Key, Layers, Upload, Download, SlidersHorizontal } from 'lucide-react';
import { useApp } from '../store';
import { TeamAvatar } from './ui';
import { t } from '../i18n';

function Item({ to, icon, children }) {
  return (
    <NavLink to={to} className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
      <span className="sidebar-icon">{icon}</span>
      <span className="sidebar-label">{children}</span>
    </NavLink>
  );
}

export default function SettingsSidebar() {
  const { team, isAdmin, isSuperadmin } = useApp();
  return (
    <aside className="sidebar">
      <Link to="/" className="settings-back">
        <ChevronLeft size={18} /> {t('Return to App')}
      </Link>
      <div className="sidebar-scroll">
        <div className="sidebar-section">
          <div className="sidebar-heading">{t('Account')}</div>
          <Item to="/settings/profile" icon={<User size={18} />}>
            {t('Profile')}
          </Item>
          <Item to="/settings/api-keys" icon={<Key size={18} />}>
            {t('API Keys & MCP')}
          </Item>
        </div>
        <div className="sidebar-section">
          <div className="sidebar-heading">
            <span className="settings-team">
              <TeamAvatar team={team} size={16} /> {team.name}
            </span>
          </div>
          {isAdmin && (
            <Item to="/settings/workspace" icon={<Building2 size={18} />}>
              {t('Details')}
            </Item>
          )}
          <Item to="/settings/members" icon={<Users size={18} />}>
            {t('Members')}
          </Item>
          <Item to="/settings/groups" icon={<UsersRound size={18} />}>
            {t('Groups')}
          </Item>
          <Item to="/settings/shares" icon={<Globe size={18} />}>
            {t('Shared Links')}
          </Item>
          <Item to="/settings/import" icon={<Upload size={18} />}>
            {t('Import')}
          </Item>
          <Item to="/settings/export" icon={<Download size={18} />}>
            {t('Export')}
          </Item>
        </div>
        <div className="sidebar-section">
          <div className="sidebar-heading">{t('Workspaces')}</div>
          <Item to="/settings/workspaces" icon={<Layers size={18} />}>
            {t('All workspaces')}
          </Item>
        </div>
        {isSuperadmin && (
          <div className="sidebar-section">
            <div className="sidebar-heading">{t('Installation')}</div>
            <Item to="/settings/general" icon={<SlidersHorizontal size={18} />}>
              {t('General settings')}
            </Item>
          </div>
        )}
      </div>
    </aside>
  );
}
