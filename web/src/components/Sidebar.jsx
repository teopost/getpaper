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
// a star dragged within the Starred section
const STAR_DND = 'application/x-getpaper-star';

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

  const logout = async () => {
    await api('auth.logout');
    window.location.href = '/login';
  };

  const items = [
    { heading: t('Workspaces') },
    {
      render: (close) => (
        <WorkspaceList
          teams={teams}
          current={team.id}
          onPick={(tm) => {
            close();
            if (tm.id !== team.id) switchTeam(tm.id);
          }}
        />
      ),
    },
    { label: t('New workspace…'), icon: <Plus size={16} />, onClick: () => setShowNew(true) },
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
    </>
  );
}

// from this many workspaces the menu shows a search field above the list
const WORKSPACE_SEARCH_FROM = 8;

const fold = (s) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();

/** Workspaces in the workspace menu: a list that scrolls on its own, with a search field when there are many */
function WorkspaceList({ teams, current, onPick }) {
  const [query, setQuery] = useState('');
  const q = fold(query.trim());
  const shown = q ? teams.filter((tm) => fold(tm.name).includes(q)) : teams;
  return (
    <>
      {teams.length >= WORKSPACE_SEARCH_FROM && (
        <input
          className="menu-search"
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && shown[0]) {
              e.preventDefault();
              onPick(shown[0]);
            }
          }}
          placeholder={t('Find a workspace…')}
          aria-label={t('Find a workspace…')}
        />
      )}
      <div className="menu-scroll">
        {shown.map((tm) => (
          <button key={tm.id} className="menu-item" onClick={() => onPick(tm)}>
            <span className="menu-icon">
              <TeamAvatar team={tm} size={20} />
            </span>
            <span className="menu-label" title={tm.name}>
              {tm.name}
            </span>
            {tm.id === current && <span className="menu-check">✓</span>}
          </button>
        ))}
        {!shown.length && <div className="menu-empty">{t('No workspaces found')}</div>}
      </div>
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

/** Menu item that stars or unstars a document ({ documentId }) or a collection ({ collectionId }) */
function starItem(target, starredIds, toggleStar) {
  const starred = starredIds.has(target.documentId || target.collectionId);
  return {
    label: starred ? t('Unstar') : t('Star'),
    icon: <Star size={16} fill={starred ? 'currentColor' : 'none'} />,
    onClick: () => toggleStar(target, !starred),
  };
}

function DocNode({ node, collection, parentId, index, depth, expanded, toggleExpanded, onMove }) {
  const { activeDoc, createDoc, starredIds, toggleStar } = useApp();
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
          <span className="sidebar-actions">
            <Menu
              trigger={
                <span className="sidebar-action" title={t('More')}>
                  <MoreHorizontal size={14} />
                </span>
              }
              items={[
                starItem({ documentId: node.id }, starredIds, toggleStar),
                canWrite && {
                  label: t('New nested document'),
                  icon: <Plus size={16} />,
                  onClick: () => {
                    toggleExpanded(node.id, true);
                    createDoc({ parentDocumentId: node.id });
                  },
                },
              ]}
            />
            {canWrite && (
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
            )}
          </span>
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
  const { trees, activeDoc, createDoc, refreshCollections, refreshAllTrees, isAdmin, user, starredIds, toggleStar } = useApp();
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
            <Menu
              trigger={
                <span className="sidebar-action" title={t('More')}>
                  <MoreHorizontal size={14} />
                </span>
              }
              items={[
                canWrite && { label: t('New document'), icon: <Plus size={16} />, onClick: () => createDoc({ collectionId: collection.id }) },
                canWrite && { label: t('Edit…'), icon: <Pencil size={16} />, onClick: () => setEditing(true) },
                starItem({ collectionId: collection.id }, starredIds, toggleStar),
                canWrite && (isAdmin || collection.createdBy === user.id) && { separator: true },
                canWrite &&
                  (isAdmin || collection.createdBy === user.id) && {
                    label: t('Delete…'),
                    icon: <Trash2 size={16} />,
                    danger: true,
                    onClick: remove,
                  },
              ]}
            />
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

function findNode(nodes, id) {
  for (const n of nodes) {
    if (n.id === id) return n;
    const found = findNode(n.children, id);
    if (found) return found;
  }
  return null;
}

/** before or after the row under the pointer */
const halfOf = (e) => {
  const r = e.currentTarget.getBoundingClientRect();
  return e.clientY - r.top < r.height / 2 ? 'before' : 'after';
};

const isStarDrag = (e) => e.dataTransfer.types.includes(STAR_DND) || e.dataTransfer.types.includes(DND_TYPE);

/** A starred document or collection: opens like in the tree, drags to change the order */
function StarredItem({ star, index, expanded, toggleExpanded, onMove, onDropAt }) {
  const { collections, trees, toggleStar } = useApp();
  const location = useLocation();
  const [drop, setDrop] = useState(null);
  const doc = star.document;
  const collection = collections.find((c) => c.id === (star.collectionId || doc?.collectionId));
  if (star.collectionId && !collection) return null;
  const node = doc && collection ? findNode(trees[collection.id] || [], doc.id) : null;
  const children = star.collectionId ? trees[collection.id] || [] : node?.children || [];
  // opened separately from the same document or collection in the tree
  const key = `star:${star.id}`;
  const isOpen = expanded.has(key);
  const target = star.collectionId ? collection : doc;
  const isActive = location.pathname.endsWith(`-${target.urlId}`);

  return (
    <div className="doc-node">
      <div
        className={drop ? `drop-${drop}` : ''}
        onDragOver={(e) => {
          if (!isStarDrag(e)) return;
          e.preventDefault();
          e.stopPropagation();
          setDrop(halfOf(e));
        }}
        onDragLeave={() => setDrop(null)}
        onDrop={(e) => {
          setDrop(null);
          if (!isStarDrag(e)) return;
          e.preventDefault();
          e.stopPropagation();
          onDropAt(e, halfOf(e) === 'before' ? index : index + 1);
        }}
      >
        <Link
          to={star.collectionId ? collectionUrl(collection) : docUrl(doc)}
          className={`sidebar-link doc-link ${isActive ? 'active' : ''}`}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData(STAR_DND, JSON.stringify({ id: star.id, index }));
            e.dataTransfer.effectAllowed = 'move';
          }}
        >
          <span
            className={`disclosure ${children.length ? '' : 'invisible'}`}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              toggleExpanded(key);
            }}
          >
            {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </span>
          <span className="sidebar-icon">
            {star.collectionId ? <CollectionIcon collection={collection} size={16} /> : <DocIcon doc={doc} size={16} />}
          </span>
          <TruncatedLabel text={star.collectionId ? collection.name : doc.title || t('Untitled')} />
          <span className="sidebar-actions">
            <span
              className="sidebar-action"
              title={t('Unstar')}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                toggleStar(star.collectionId ? { collectionId: star.collectionId } : { documentId: doc.id }, false);
              }}
            >
              <Star size={14} fill="currentColor" />
            </span>
          </span>
        </Link>
      </div>
      {isOpen &&
        collection &&
        children.map((c, i) => (
          <DocNode
            key={c.id}
            node={c}
            collection={collection}
            parentId={star.collectionId ? null : doc.id}
            index={i}
            depth={1}
            expanded={expanded}
            toggleExpanded={toggleExpanded}
            onMove={onMove}
          />
        ))}
    </div>
  );
}

/**
 * Starred documents and collections, as in Outline: the heading folds the section, items open
 * like in the tree and are dragged to change the order; a document or collection dragged here from
 * the tree is starred where it is dropped.
 */
function StarredSection({ expanded, toggleExpanded, onMove }) {
  const { starred, toggleStar, refreshStarred } = useApp();
  const toast = useToast();
  const [folded, setFolded] = useState(() => {
    try {
      return localStorage.getItem('sidebar-starred-folded') === '1';
    } catch {
      return false;
    }
  });
  // while something is dragged from the tree the section shows, also when nothing is starred yet
  const [dragging, setDragging] = useState(false);
  const [over, setOver] = useState(false);

  useEffect(() => {
    const start = (e) => e.dataTransfer?.types.includes(DND_TYPE) && setDragging(true);
    const end = () => {
      setDragging(false);
      setOver(false);
    };
    document.addEventListener('dragstart', start);
    document.addEventListener('dragend', end);
    document.addEventListener('drop', end);
    return () => {
      document.removeEventListener('dragstart', start);
      document.removeEventListener('dragend', end);
      document.removeEventListener('drop', end);
    };
  }, []);

  const toggleFolded = () => {
    setFolded((f) => {
      try {
        localStorage.setItem('sidebar-starred-folded', f ? '0' : '1');
      } catch {
        /* ignore */
      }
      return !f;
    });
  };

  const dropAt = async (e, index) => {
    const star = e.dataTransfer.getData(STAR_DND);
    if (star) {
      const { id, index: from } = JSON.parse(star);
      const to = from < index ? index - 1 : index;
      if (to === from) return;
      try {
        await api('stars.update', { id, index: to });
        await refreshStarred();
      } catch (err) {
        toast(err.message, 'error');
      }
      return;
    }
    const raw = e.dataTransfer.getData(DND_TYPE);
    if (!raw) return;
    const data = JSON.parse(raw);
    toggleStar(data.type === 'collection' ? { collectionId: data.id } : { documentId: data.id }, true, index);
  };

  if (!starred.length && !dragging) return null;

  return (
    <div
      className={`sidebar-section starred-section ${over ? 'drop-target' : ''}`}
      onDragOver={(e) => {
        if (!isStarDrag(e)) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget) && setOver(false)}
      onDrop={(e) => {
        setOver(false);
        if (!isStarDrag(e)) return;
        e.preventDefault();
        dropAt(e, starred.length);
      }}
    >
      <button className="sidebar-heading sidebar-heading-toggle" onClick={toggleFolded} aria-expanded={!folded}>
        <span>{t('Starred')}</span>
        {folded ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
      </button>
      {!folded &&
        starred.map((s, i) => (
          <StarredItem
            key={s.id}
            star={s}
            index={i}
            expanded={expanded}
            toggleExpanded={toggleExpanded}
            onMove={onMove}
            onDropAt={dropAt}
          />
        ))}
      {(folded || !starred.length) && dragging && <div className="sidebar-empty starred-drop">{t('Drop here to star')}</div>}
    </div>
  );
}

export default function Sidebar() {
  const { collections, canWrite, refreshAllTrees, refreshCollections } = useApp();
  const [expanded, toggleExpanded] = usePersistentSet('sidebar-expanded');
  const [openState, setOpen] = useCollectionState();
  const [creating, setCreating] = useState(false);
  const [showInvite, setShowInvite] = useState(false);
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

        <StarredSection expanded={expanded} toggleExpanded={toggleExpanded} onMove={onMove} />

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
          <SidebarLink to="/settings/workspace" icon={<Settings size={18} />}>
            {t('Settings')}
          </SidebarLink>
          {canWrite && (
            <button className="sidebar-link" onClick={() => setShowInvite(true)}>
              <span className="sidebar-icon">
                <UserPlus size={18} />
              </span>
              <span className="sidebar-label">{t('Invite people…')}</span>
            </button>
          )}
        </div>
      </div>
      {creating && <CollectionModal onClose={() => setCreating(false)} />}
      {showInvite && <InviteModal onClose={() => setShowInvite(false)} />}
    </aside>
  );
}

