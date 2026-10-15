import React from 'react';
import { NavLink, Link } from 'react-router-dom';
import { ChevronLeft, User, Building2, Users, Globe, Key, Layers, Upload } from 'lucide-react';
import { useApp } from '../store';
import { TeamAvatar } from './ui';

function Item({ to, icon, children }) {
  return (
    <NavLink to={to} className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
      <span className="sidebar-icon">{icon}</span>
      <span className="sidebar-label">{children}</span>
    </NavLink>
  );
}

export default function SettingsSidebar() {
  const { team, isAdmin } = useApp();
  return (
    <aside className="sidebar">
      <Link to="/" className="settings-back">
        <ChevronLeft size={18} /> Return to App
      </Link>
      <div className="sidebar-scroll">
        <div className="sidebar-section">
          <div className="sidebar-heading">Account</div>
          <Item to="/settings/profile" icon={<User size={18} />}>
            Profile
          </Item>
          <Item to="/settings/api-keys" icon={<Key size={18} />}>
            API Keys & MCP
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
              Details
            </Item>
          )}
          <Item to="/settings/members" icon={<Users size={18} />}>
            Members
          </Item>
          <Item to="/settings/shares" icon={<Globe size={18} />}>
            Shared Links
          </Item>
          <Item to="/settings/import" icon={<Upload size={18} />}>
            Import
          </Item>
        </div>
        <div className="sidebar-section">
          <div className="sidebar-heading">Workspaces</div>
          <Item to="/settings/workspaces" icon={<Layers size={18} />}>
            All workspaces
          </Item>
        </div>
      </div>
    </aside>
  );
}
