import React, { useEffect, useState } from 'react';
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom';
import {
  Archive,
  ChevronDown,
  ChevronRight,
  Home,
  LayoutTemplate,
  LogOut,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Settings,
  Star,
  Trash2,
  UserPlus,
  Check,
  Globe,
} from 'lucide-react';
import { api, collectionUrl, docUrl, formatDate } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import { CollectionIcon, DocIcon } from './icons';
import { Menu, TeamAvatar, Avatar } from './ui';
import CollectionModal from './CollectionModal';
import NewWorkspaceModal from './NewWorkspaceModal';
import InviteModal from './InviteModal';
import Notifications from './Notifications';
import { t } from '../i18n';

const DND_TYPE = 'application/x-getpaper-doc';

function usePersistentSet(key) {
  const [set, setSet] = useState(() => {
    try {
      return new Set(JSON.parse(localStorage.getItem(key) || '[]'));
    } catch {
      return new Set();
    }
  });
  /** toggles id, or sets it (and the other ids, e.g. a whole branch) to value */
  const toggle = (id, value, others = []) => {
    setSet((s) => {
      const n = new Set(s);
      const v = value ?? !n.has(id);
      for (const i of [id, ...others]) {
        if (v) n.add(i);
        else n.delete(i);
      }
      try {
        localStorage.setItem(key, JSON.stringify([...n]));
      } catch {
        /* ignore */
      }
      return n;
    });
  };
  return [set, toggle];
}

/**
 * Collections opened or closed by hand, remembered in this browser: { id: true | false }. The
 * others follow Outline: only the collection being viewed is open. Collections closed with the
 * previous sidebar (which started with everything open) stay closed.
 */
function useCollectionState() {
  const KEY = 'sidebar-collections-open';
  const [state, setState] = useState(() => {
    try {
      const saved = localStorage.getItem(KEY);
      if (saved) return JSON.parse(saved);
      const closed = JSON.parse(localStorage.getItem('sidebar-collapsed-collections') || '[]');
      return Object.fromEntries(closed.map((id) => [id, false]));
    } catch {
      return {};
    }
  });
  const set = (id, open) => {
    setState((s) => {
      const n = { ...s, [id]: open };
      try {
        localStorage.setItem(KEY, JSON.stringify(n));
      } catch {
        /* ignore */
      }
      return n;
    });
  };
  return [state, set];
}

/** ids of the documents with children in a branch of the tree (for Alt+click) */
function branchIds(nodes, out = []) {
  for (const n of nodes) {
    if (n.children.length) {
      out.push(n.id);
      branchIds(n.children, out);
    }
  }
  return out;
}

function TeamSwitcher() {
  const { team, teams, user, switchTeam } = useApp();
  const navigate = useNavigate();
  const [showNew, setShowNew] = useState(false);
  const [showInvite, setShowInvite] = useState(false);

  const logout = async () => {
    await api('auth.logout');
    window.location.href = '/login';
  };

  const items = [
    { heading: t('Workspaces') },
    ...teams.map((tm) => ({
      label: tm.name,
      icon: <TeamAvatar team={tm} size={20} />,
      checked: tm.id === team.id,
      onClick: () => tm.id !== team.id && switchTeam(tm.id),
    })),
    { label: t('New workspace…'), icon: <Plus size={16} />, onClick: () => setShowNew(true) },
    { separator: true },
    { label: t('Settings'), icon: <Settings size={16} />, onClick: () => navigate('/settings/workspace') },
    { label: t('Invite people…'), icon: <UserPlus size={16} />, onClick: () => setShowInvite(true) },
    { separator: true },
    {
      render: () => (
        <div className="menu-user">
          <Avatar name={user.name} color={user.color} size={24} />
          <div>
            <div className="menu-user-name">{user.name}</div>
            <div className="menu-user-email">{user.email}</div>
          </div>
        </div>
      ),
    },
    { label: t('Profile'), icon: <Pencil size={16} />, onClick: () => navigate('/settings/profile') },
    { label: t('Log out'), icon: <LogOut size={16} />, onClick: logout },
  ];

  return (
    <>
      <Menu
        width={260}
        className="team-switcher-wrap"
        trigger={
          <div className="team-switcher">
            <TeamAvatar team={team} size={28} />
            <span className="team-name">{team.name}</span>
            <ChevronDown size={16} className="muted" />
          </div>
        }
        items={items}
      />
      {showNew && <NewWorkspaceModal onClose={() => setShowNew(false)} />}
      {showInvite && <InviteModal onClose={() => setShowInvite(false)} />}
    </>
  );
}

/** Label of a tree row; the full text appears as a tooltip when it doesn't fit */
function TruncatedLabel({ text }) {
  return (
    <span
      className="sidebar-label"
      onMouseEnter={(e) => {
        const el = e.currentTarget;
        if (el.scrollWidth > el.clientWidth) el.title = text;
        else el.removeAttribute('title');
      }}
    >
      {text}
    </span>
  );
}

function SidebarLink({ to, icon, children, end, badge }) {
  return (
    <NavLink to={to} end={end} className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
      <span className="sidebar-icon">{icon}</span>
      <span className="sidebar-label">{children}</span>
      {badge ? <span className="sidebar-badge">{badge}</span> : null}
    </NavLink>
  );
}

function DropZone({ onDropDoc, children, className = '', allowInside = true, edges = true }) {
  const [state, setState] = useState(null);
  const compute = (e) => {
    const r = e.currentTarget.getBoundingClientRect();
    const y = e.clientY - r.top;
    if (edges && y < r.height * 0.25) return 'before';
    if (edges && y > r.height * 0.75) return 'after';
    return allowInside ? 'inside' : y < r.height / 2 ? 'before' : 'after';
  };
  return (
    <div
      className={`${className} ${state ? `drop-${state}` : ''}`}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(DND_TYPE)) return;
        e.preventDefault();
        e.stopPropagation();
        setState(compute(e));
      }}
      onDragLeave={() => setState(null)}
      onDrop={(e) => {
        const raw = e.dataTransfer.getData(DND_TYPE);
        setState(null);
        if (!raw) return;
        e.preventDefault();
        e.stopPropagation();
        const data = JSON.parse(raw);
        let where = compute(e);
        if (data.type === 'collection') {
          const r = e.currentTarget.getBoundingClientRect();
          where = e.clientY - r.top < r.height / 2 ? 'before' : 'after';
        }
        onDropDoc(data, where);
      }}
    >
      {children}
    </div>
  );
}

function DocNode({ node, collection, parentId, index, depth, expanded, toggleExpanded, onMove }) {
  const { activeDoc, createDoc } = useApp();
  const location = useLocation();
  const isActive = location.pathname.endsWith(`-${node.urlId}`);
  const hasChildren = node.children.length > 0;
  const isOpen = expanded.has(node.id);
  const canWrite = collection.access === 'write';

  useEffect(() => {
    if (activeDoc.ids.includes(node.id) && hasChildren && !isOpen && !isActive) toggleExpanded(node.id, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDoc]);

  const handleDrop = (data, where) => {
    if (data.id === node.id || data.type === 'collection') return;
    if (where === 'inside') onMove(data, { parentDocumentId: node.id });
    else {
      let idx = where === 'before' ? index : index + 1;
      if (data.parentId === parentId && data.collectionId === collection.id && data.index < idx) idx -= 1;
      onMove(data, { collectionId: collection.id, parentDocumentId: parentId, index: idx });
    }
  };

  return (
    <div className="doc-node">
      <DropZone onDropDoc={canWrite ? handleDrop : () => {}}>
        <Link
          to={docUrl(node)}
          className={`sidebar-link doc-link ${isActive ? 'active' : ''}`}
          style={{ paddingLeft: 12 + depth * 16 }}
          draggable={canWrite}
          onDragStart={(e) => {
            e.dataTransfer.setData(
              DND_TYPE,
              JSON.stringify({ id: node.id, parentId, index, collectionId: collection.id })
            );
            e.dataTransfer.effectAllowed = 'move';
          }}
        >
          <span
            className={`disclosure ${hasChildren ? '' : 'invisible'}`}
            title={hasChildren ? t('Alt+click (Option on Mac) to expand or collapse all') : undefined}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              // Alt+click: the whole branch, as in Outline
              if (e.altKey) toggleExpanded(node.id, !isOpen, branchIds(node.children));
              else toggleExpanded(node.id);
            }}
          >
            {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </span>
          {node.emoji && <span className="sidebar-emoji">{node.emoji}</span>}
          <TruncatedLabel text={node.title || t('Untitled')} />
          {node.sharedAt && (
            <span
              className="sidebar-public"
              title={t('Public: anyone with the link can read it (since {date})', { date: formatDate(node.sharedAt) })}
            >
              <Globe size={14} />
            </span>
          )}
          {canWrite && (
            <span className="sidebar-actions">
              <span
                className="sidebar-action"
                title={t('New nested document')}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  toggleExpanded(node.id, true);
                  createDoc({ parentDocumentId: node.id });
                }}
              >
                <Plus size={14} />
              </span>
            </span>
          )}
        </Link>
      </DropZone>
      {isOpen &&
        node.children.map((c, i) => (
          <DocNode
            key={c.id}
            node={c}
            collection={collection}
            parentId={node.id}
            index={i}
            depth={depth + 1}
            expanded={expanded}
            toggleExpanded={toggleExpanded}
            onMove={onMove}
          />
        ))}
    </div>
  );
}

function CollectionNode({ collection, index, expanded, toggleExpanded, openState, setOpen, onMove, onMoveCollection }) {
  const { trees, activeDoc, createDoc, refreshCollections, refreshAllTrees, isAdmin, user } = useApp();
  const location = useLocation();
  const navigate = useNavigate();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const nodes = trees[collection.id] || [];
  const canWrite = collection.access === 'write';
  const isActive = location.pathname.endsWith(`-${collection.urlId}`);
  // the collection being viewed (its page or one of its documents)
  const isCurrent = isActive || activeDoc.collectionId === collection.id;
  const isOpen = openState[collection.id] ?? isCurrent;

  // opening a document or the page of a collection closed by hand opens it again, as in Outline
  useEffect(() => {
    if (isCurrent && openState[collection.id] === false) setOpen(collection.id, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isCurrent, activeDoc]);

  const remove = async () => {
    if (!window.confirm(t('Delete the "{name}" collection and all of its documents?', { name: collection.name }))) return;
    try {
      await api('collections.delete', { id: collection.id });
      await refreshCollections();
      await refreshAllTrees();
      toast(t('Collection deleted'));
      if (isActive) navigate('/');
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  return (
    <div className="collection-node">
      <DropZone
        onDropDoc={(data, where) => {
          if (data.type === 'collection') {
            onMoveCollection(data, where === 'after' ? index + 1 : index);
          } else if (canWrite) {
            onMove(data, { collectionId: collection.id, parentDocumentId: null, index: where === 'before' ? 0 : undefined });
          }
        }}
        edges={false}
      >
        <Link
          to={collectionUrl(collection)}
          className={`sidebar-link collection-link ${isActive ? 'active' : ''}`}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData(DND_TYPE, JSON.stringify({ type: 'collection', id: collection.id, index }));
          }}
        >
          <span
            className="disclosure"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setOpen(collection.id, !isOpen);
              // Alt+click: also every document in it
              if (e.altKey) {
                const ids = branchIds(nodes);
                if (ids.length) toggleExpanded(ids[0], !isOpen, ids.slice(1));
              }
            }}
            title={t('Alt+click (Option on Mac) to expand or collapse all')}
          >
            {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </span>
          <span className="sidebar-icon">
            <CollectionIcon collection={collection} size={16} />
          </span>
          <TruncatedLabel text={collection.name} />
          <span className="sidebar-actions">
            {canWrite && (
              <span
                className="sidebar-action"
                title={t('New document')}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setOpen(collection.id, true);
                  createDoc({ collectionId: collection.id });
                }}
              >
                <Plus size={14} />
              </span>
            )}
            {canWrite && (
              <Menu
                trigger={
                  <span className="sidebar-action" title={t('More')}>
                    <MoreHorizontal size={14} />
                  </span>
                }
                items={[
                  { label: t('New document'), icon: <Plus size={16} />, onClick: () => createDoc({ collectionId: collection.id }) },
                  { label: t('Edit…'), icon: <Pencil size={16} />, onClick: () => setEditing(true) },
                  (isAdmin || collection.createdBy === user.id) && { separator: true },
                  (isAdmin || collection.createdBy === user.id) && {
                    label: t('Delete…'),
                    icon: <Trash2 size={16} />,
                    danger: true,
                    onClick: remove,
                  },
                ]}
              />
            )}
          </span>
        </Link>
      </DropZone>
      {isOpen && (
        <div className="collection-children">
          {nodes.map((n, i) => (
            <DocNode
              key={n.id}
              node={n}
              collection={collection}
              parentId={null}
              index={i}
              depth={1}
              expanded={expanded}
              toggleExpanded={toggleExpanded}
              onMove={onMove}
            />
          ))}
          {nodes.length === 0 && <div className="sidebar-empty">{t('Empty')}</div>}
        </div>
      )}
      {editing && <CollectionModal collection={collection} onClose={() => setEditing(false)} />}
    </div>
  );
}

export default function Sidebar() {
  const { collections, starred, canWrite, refreshAllTrees, refreshCollections } = useApp();
  const [expanded, toggleExpanded] = usePersistentSet('sidebar-expanded');
  const [openState, setOpen] = useCollectionState();
  const [creating, setCreating] = useState(false);
  const [draftCount, setDraftCount] = useState(0);
  const location = useLocation();
  const toast = useToast();

  useEffect(() => {
    api('documents.drafts', { limit: 100 })
      .then((d) => setDraftCount(d.length))
      .catch(() => {});
  }, [location.pathname]);

  const onMove = async (data, target) => {
    if (data.type === 'collection') return;
    try {
      await api('documents.move', { id: data.id, ...target });
      await refreshAllTrees();
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  const onMoveCollection = async (data, index) => {
    let idx = index;
    if (data.index < idx) idx -= 1;
    try {
      await api('collections.move', { id: data.id, index: Math.max(0, idx) });
      await refreshCollections();
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  return (
    <aside className="sidebar">
      <TeamSwitcher />
      <div className="sidebar-scroll">
        <nav className="sidebar-section">
          <SidebarLink to="/" end icon={<Home size={18} />}>
            {t('Home')}
          </SidebarLink>
          <SidebarLink to="/search" icon={<Search size={18} />}>
            {t('Search')}
          </SidebarLink>
          <Notifications />
          {canWrite && (
            <SidebarLink to="/drafts" icon={<Pencil size={18} />} badge={draftCount || null}>
              {t('Drafts')}
            </SidebarLink>
          )}
        </nav>

        {starred.length > 0 && (
          <div className="sidebar-section">
            <div className="sidebar-heading">{t('Starred')}</div>
            {starred.map((d) => (
              <NavLink key={d.id} to={docUrl(d)} className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
                <span className="sidebar-icon">
                  {d.emoji ? <span className="sidebar-emoji">{d.emoji}</span> : <Star size={16} />}
                </span>
                <TruncatedLabel text={d.title || t('Untitled')} />
              </NavLink>
            ))}
          </div>
        )}

        <div className="sidebar-section">
          <div className="sidebar-heading">
            <span>{t('Collections')}</span>
            {canWrite && (
              <button className="sidebar-heading-action" title={t('New collection')} onClick={() => setCreating(true)}>
                <Plus size={14} />
              </button>
            )}
          </div>
          {collections.map((c, i) => (
            <CollectionNode
              key={c.id}
              collection={c}
              index={i}
              expanded={expanded}
              toggleExpanded={toggleExpanded}
              openState={openState}
              setOpen={setOpen}
              onMove={onMove}
              onMoveCollection={onMoveCollection}
            />
          ))}
          {canWrite && (
            <button className="sidebar-link muted-link" onClick={() => setCreating(true)}>
              <span className="sidebar-icon">
                <Plus size={16} />
              </span>
              <span className="sidebar-label">{t('New collection…')}</span>
            </button>
          )}
        </div>

        <div className="sidebar-section sidebar-bottom">
          <SidebarLink to="/templates" icon={<LayoutTemplate size={18} />}>
            {t('Templates')}
          </SidebarLink>
          <SidebarLink to="/archive" icon={<Archive size={18} />}>
            {t('Archive')}
          </SidebarLink>
          <SidebarLink to="/trash" icon={<Trash2 size={18} />}>
            {t('Trash')}
          </SidebarLink>
        </div>
      </div>
      {creating && <CollectionModal onClose={() => setCreating(false)} />}
    </aside>
  );
}

