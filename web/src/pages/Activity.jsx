import React, { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Download } from 'lucide-react';
import { api, collectionUrl, docUrl, downloadText, formatDate, timeAgo } from '../api';
import { useApp } from '../store';
import { Avatar, Button, Empty, Spinner, Tabs } from '../components/ui';
import { useToast } from '../components/Toast';
import { t } from '../i18n';

const PERIODS = [
  { value: 7, label: 'Last 7 days' },
  { value: 30, label: 'Last 30 days' },
  { value: 90, label: 'Last 90 days' },
  { value: 365, label: 'Last 12 months' },
  { value: 0, label: 'All time' },
];

const CATEGORIES = [
  { value: '', label: 'All events' },
  { value: 'auth', label: 'Sign-ins' },
  { value: 'members', label: 'Members and groups' },
  { value: 'content', label: 'Documents and collections' },
  { value: 'sharing', label: 'Public links' },
  { value: 'data', label: 'Import and export' },
  { value: 'settings', label: 'Settings and API keys' },
];

/** What an event means, in words */
const EVENTS = {
  'auth.signin': 'Signed in',
  'auth.signin_failed': 'Failed sign-in',
  'auth.signout': 'Signed out',
  'users.create': 'Created an account',
  'users.join': 'Joined the workspace',
  'users.leave': 'Left the workspace',
  'users.remove': 'Removed a member',
  'users.role': 'Changed the role of a member',
  'users.password': 'Changed their password',
  'users.password_set': 'Set a password',
  'users.suspend': 'Suspended a member',
  'users.activate': 'Reactivated a member',
  'users.suspend_account': 'Suspended an account',
  'users.activate_account': 'Reactivated an account',
  'documents.add_user': 'Shared a document with someone',
  'documents.remove_user': 'Removed someone from a document',
  'documents.add_group': 'Shared a document with a group',
  'documents.remove_group': 'Removed a group from a document',
  'invites.create': 'Invited someone',
  'invites.revoke': 'Revoked an invite',
  'groups.create': 'Created a group',
  'groups.update': 'Changed a group',
  'groups.delete': 'Deleted a group',
  'groups.add_user': 'Added someone to a group',
  'groups.remove_user': 'Removed someone from a group',
  'collections.create': 'Created a collection',
  'collections.update': 'Changed a collection',
  'collections.permission': 'Changed the access to a collection',
  'collections.delete': 'Deleted a collection',
  'collections.add_user': 'Gave someone access to a collection',
  'collections.remove_user': 'Removed someone from a collection',
  'collections.add_group': 'Gave a group access to a collection',
  'collections.remove_group': 'Removed a group from a collection',
  'collections.export': 'Exported a collection',
  'documents.publish': 'Published a document',
  'documents.unpublish': 'Unpublished a document',
  'documents.move': 'Moved a document',
  'documents.archive': 'Archived a document',
  'documents.restore': 'Restored a document',
  'documents.delete': 'Deleted a document',
  'documents.export': 'Downloaded a document',
  'shares.create': 'Shared a document publicly',
  'shares.revoke': 'Revoked a public link',
  'api_keys.create': 'Created an API key',
  'api_keys.delete': 'Deleted an API key',
  'data.import': 'Imported documents',
  'workspace.export': 'Exported the workspace',
  'teams.create': 'Created a workspace',
  'teams.update': 'Changed the workspace settings',
  'teams.delete': 'Deleted a workspace',
  'instance.update': 'Changed the general settings',
  'instance.superadmin_add': 'Added a superadmin',
  'instance.superadmin_remove': 'Removed a superadmin',
};

const eventLabel = (name) => (EVENTS[name] ? t(EVENTS[name]) : name);

const KEYS = {
  role: 'Role',
  permission: 'Access',
  sharing: 'Public links',
  name: 'Name',
  email: 'Email',
  method: 'Method',
  reason: 'Reason',
  nested: 'Nested documents',
  permanent: 'Permanently',
  format: 'Format',
  documents: 'Documents',
  attachments: 'Attachments',
  user: 'Person',
  group: 'Group',
  collection: 'Collection',
  collections: 'Collections',
  from: 'From',
  to: 'To',
  file: 'File',
  added: 'Existing account',
  access: 'Access',
  via: 'Via',
  invite: 'Invite',
  renamedFrom: 'Previous name',
  allWorkspaces: 'All workspaces',
  limitedToCollections: 'Limited to some collections',
  expiresAt: 'Expires',
  defaultUserRole: 'Default role',
  allowedDomains: 'Allowed domains',
  logo: 'Logo',
  bytes: 'Size (bytes)',
  // general settings
  auditRetentionDays: 'Audit log (days)',
  activityShowViews: 'Documents viewed per person',
  allowBackupExport: 'Workspace backups',
  allowMarkdownExport: 'Markdown export',
  allowPublicSharing: 'Public links',
  productName: 'Product name',
  headerColor: 'Header colour',
  accentColor: 'Accent colour',
  defaultLanguage: 'Default language',
  defaultTheme: 'Default theme',
  defaultContentWidth: 'Default document width',
};

function value(key, v) {
  if (v === null || v === undefined || v === '') return key === 'permission' ? t('Private') : '-';
  if (v === true) return t('Yes');
  if (v === false) return t('No');
  if (key === 'expiresAt') return formatDate(v);
  if (key === 'role' || key === 'defaultUserRole') return t({ admin: 'Admin', member: 'Editor', viewer: 'Viewer' }[v] || String(v));
  if (key === 'permission' || key === 'access') {
    return { read: t('Read only'), read_write: t('Can edit'), write: t('Read and write') }[v] || String(v);
  }
  if (Array.isArray(v)) return v.join(', ');
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

/** The details of an event in one line: "Role: Editor → Admin, …" */
function details(e) {
  return Object.entries(e.data || {})
    .filter(([k, v]) => !(k === 'new' && v === false) && k !== 'template' && !(k === 'nested' && !v))
    .map(([k, v]) => {
      const label = KEYS[k] ? t(KEYS[k]) : k;
      if (v && typeof v === 'object' && !Array.isArray(v) && 'from' in v && 'to' in v) {
        return `${label}: ${value(k, v.from)} → ${value(k, v.to)}`;
      }
      if (k === 'new') return t('New document');
      return `${label}: ${value(k, v)}`;
    })
    .join(', ');
}

/** "Chrome on Windows" from the user agent */
function browser(ua) {
  if (!ua) return '';
  const name = [
    [/Edg\//, 'Edge'],
    [/OPR\//, 'Opera'],
    [/Firefox\//, 'Firefox'],
    [/Chrome\//, 'Chrome'],
    [/Safari\//, 'Safari'],
    [/^curl\//, 'curl'],
  ].find(([r]) => r.test(ua))?.[1];
  const os = [
    [/Windows/, 'Windows'],
    [/iPhone|iPad/, 'iOS'],
    [/Mac OS X/, 'macOS'],
    [/Android/, 'Android'],
    [/Linux/, 'Linux'],
  ].find(([r]) => r.test(ua))?.[1];
  if (name && os) return t('{browser} on {os}', { browser: name, os });
  return name || os || ua.slice(0, 40);
}

/** The request for a filter: the period in days becomes its start, computed when the request is made */
function request({ days, ...rest }) {
  return { ...rest, from: days ? new Date(Date.now() - days * 86400000).toISOString() : undefined };
}

function Person({ name, email, color }) {
  return (
    <span className="row gap-sm activity-person">
      <Avatar name={name || email} color={color} size={22} />
      <span className="activity-person-text">
        <span>{name || email || t('Unknown')}</span>
        {email && name && <span className="muted small">{email}</span>}
      </span>
    </span>
  );
}

function When({ at }) {
  return (
    <span title={formatDate(at)} className="nowrap">
      {timeAgo(at)}
    </span>
  );
}

function Select({ value, onChange, options }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.translate === false ? o.label : t(o.label)}
        </option>
      ))}
    </select>
  );
}

/** Loads a page of a list and appends the next ones with "Load more" */
function usePaged(method, filter) {
  const [state, setState] = useState({ items: null, hasMore: false, info: {} });
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const key = JSON.stringify(filter);
  const load = async (offset = 0) => {
    setBusy(true);
    try {
      const r = await api(method, { ...request(filter), limit: 100, offset });
      setState((s) => ({ items: offset ? [...(s.items || []), ...r.items] : r.items, hasMore: r.hasMore, info: r }));
    } catch (e) {
      toast(e.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    setState({ items: null, hasMore: false, info: {} });
    load(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [method, key]);
  return { ...state, busy, more: () => load(state.items?.length || 0) };
}

function LoadMore({ list }) {
  if (!list.hasMore) return null;
  return (
    <div className="activity-more">
      <Button variant="secondary" onClick={list.more} disabled={list.busy}>
        {list.busy ? t('Loading…') : t('Load more')}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// people
// ---------------------------------------------------------------------------

function People({ days, onPick }) {
  const list = usePaged('activity.people', { days });
  if (!list.items) return <Spinner />;
  const showViews = list.info.showViews;
  return (
    <>
      <div className="table-wrap">
        <table className="table activity-table">
          <thead>
            <tr>
              <th>{t('Person')}</th>
              <th>{t('Last active')}</th>
              <th>{t('Last sign-in')}</th>
              <th className="num">{t('Sign-ins')}</th>
              <th className="num">{t('Published')}</th>
              <th className="num">{t('Edited')}</th>
              {showViews && <th className="num">{t('Viewed')}</th>}
            </tr>
          </thead>
          <tbody>
            {list.items.map((p) => (
              <tr key={p.id}>
                <td>
                  <button className="link-button" onClick={() => onPick(p.id)} title={t('See the changes of this person')}>
                    <Person {...p} />
                  </button>
                  <span className="badge activity-role">{t({ admin: 'Admin', member: 'Editor', viewer: 'Viewer' }[p.role] || p.role)}</span>
                </td>
                <td className="muted">{p.lastActiveAt ? <When at={p.lastActiveAt} /> : t('Never')}</td>
                <td className="muted">{p.lastSignInAt ? <When at={p.lastSignInAt} /> : '-'}</td>
                <td className="num">{p.signIns}</td>
                <td className="num">{p.documentsPublished}</td>
                <td className="num" title={t('{count} editing sessions', { count: p.edits })}>
                  {p.documentsEdited}
                </td>
                {showViews && <td className="num">{p.documentsViewed}</td>}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="muted small activity-note">
        {t('Published, edited and viewed count documents in the period; sign-ins are counted since the audit log is on.')}
        {!showViews && ' ' + t('The number of documents viewed by each person is hidden: a superadmin can show it in the general settings.')}
      </p>
    </>
  );
}

// ---------------------------------------------------------------------------
// changes
// ---------------------------------------------------------------------------

function Changes({ days, userId, collectionId }) {
  const list = usePaged('activity.changes', { days, userId: userId || undefined, collectionId: collectionId || undefined });
  if (!list.items) return <Spinner />;
  if (!list.items.length) return <Empty>{t('No changes to documents in this period.')}</Empty>;
  return (
    <>
      <div className="table-wrap">
        <table className="table activity-table">
          <thead>
            <tr>
              <th>{t('When')}</th>
              <th>{t('Person')}</th>
              <th>{t('Document')}</th>
              <th>{t('Collection')}</th>
            </tr>
          </thead>
          <tbody>
            {list.items.map((c) => (
              <tr key={c.id}>
                <td className="muted">
                  <When at={c.createdAt} />
                </td>
                <td>
                  <Person name={c.userName} color={c.userColor} />
                </td>
                <td>
                  {c.deleted ? (
                    <span className="muted">{c.title || t('Untitled')}</span>
                  ) : (
                    <Link to={docUrl({ title: c.title, urlId: c.urlId })}>
                      {c.emoji ? `${c.emoji} ` : ''}
                      {c.title || t('Untitled')}
                    </Link>
                  )}{' '}
                  {c.isFirst && <span className="badge">{t('New')}</span>}
                  {c.deleted && <span className="badge">{t('Deleted')}</span>}
                </td>
                <td className="muted">{c.collectionName || '-'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <LoadMore list={list} />
    </>
  );
}

// ---------------------------------------------------------------------------
// audit log
// ---------------------------------------------------------------------------

function Target({ e }) {
  if (!e.targetName) return '-';
  if (e.targetUrlId && e.targetType === 'document') return <Link to={docUrl({ title: e.targetName, urlId: e.targetUrlId })}>{e.targetName}</Link>;
  if (e.targetUrlId && e.targetType === 'collection') return <Link to={collectionUrl({ name: e.targetName, urlId: e.targetUrlId })}>{e.targetName}</Link>;
  return e.targetName;
}

const csvCell = (v) => {
  const s = v === null || v === undefined ? '' : String(v);
  return /[",\n;]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

/** The events of the workspace (activity.events) or of the installation (instance.events) */
export function EventLog({ method, userId, days, showTeam }) {
  const [category, setCategory] = useState('');
  const [query, setQuery] = useState('');
  const [q, setQ] = useState('');
  const [exporting, setExporting] = useState(false);
  const toast = useToast();
  const filter = { days, userId: userId || undefined, category: category || undefined, q: q || undefined };
  const list = usePaged(method, filter);

  // the whole filtered log (at most 20 000 events) as CSV
  const exportCsv = async () => {
    setExporting(true);
    try {
      const rows = [];
      for (let offset = 0; offset < 20000; offset += 1000) {
        const r = await api(method, { ...request(filter), limit: 1000, offset });
        rows.push(...r.items);
        if (!r.hasMore) break;
      }
      const header = ['time', 'workspace', 'event', 'description', 'person', 'email', 'target type', 'target', 'details', 'ip', 'via', 'user agent'];
      const lines = rows.map((e) =>
        [e.createdAt, e.teamName, e.name, eventLabel(e.name), e.actorName, e.actorEmail, e.targetType, e.targetName, details(e), e.ip, e.via, e.userAgent]
          .map(csvCell)
          .join(',')
      );
      downloadText(`audit-log-${new Date().toISOString().slice(0, 10)}.csv`, [header.join(','), ...lines].join('\n'), 'text/csv');
    } catch (e) {
      toast(e.message, 'error');
    } finally {
      setExporting(false);
    }
  };

  const retention = list.info.auditRetentionDays;
  return (
    <>
      <div className="activity-filters">
        <Select value={category} onChange={setCategory} options={CATEGORIES} />
        <form
          className="grow"
          onSubmit={(e) => {
            e.preventDefault();
            setQ(query.trim());
          }}
        >
          <input
            type="search"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              if (!e.target.value) setQ('');
            }}
            placeholder={t('Search name, email, document or IP address')}
          />
        </form>
        <Button variant="secondary" icon={<Download size={16} />} onClick={exportCsv} disabled={exporting || !list.items?.length}>
          {exporting ? t('Exporting…') : t('Export CSV')}
        </Button>
      </div>
      {retention === 0 && (
        <p className="activity-note warning-text">{t('The audit log is off: a superadmin can turn it on in the general settings.')}</p>
      )}
      {!list.items ? (
        <Spinner />
      ) : !list.items.length ? (
        <Empty>{t('No events in this period.')}</Empty>
      ) : (
        <div className="table-wrap">
          <table className="table activity-table">
            <thead>
              <tr>
                <th>{t('When')}</th>
                {showTeam && <th>{t('Workspace')}</th>}
                <th>{t('Person')}</th>
                <th>{t('Action')}</th>
                <th>{t('On')}</th>
                <th>{t('Details')}</th>
                <th>{t('From')}</th>
              </tr>
            </thead>
            <tbody>
              {list.items.map((e) => (
                <tr key={e.id} className={e.name === 'auth.signin_failed' ? 'activity-failed' : ''}>
                  <td className="muted">
                    <When at={e.createdAt} />
                  </td>
                  {showTeam && <td className="muted">{e.teamName || '-'}</td>}
                  <td>
                    <Person name={e.actorName} email={e.actorEmail} />
                  </td>
                  <td>{eventLabel(e.name)}</td>
                  <td>
                    <Target e={e} />
                  </td>
                  <td className="muted small activity-details">{details(e)}</td>
                  <td className="muted small" title={e.userAgent || ''}>
                    <div className="nowrap">{e.ip || '-'}</div>
                    <div className="nowrap">
                      {browser(e.userAgent)}
                      {e.via && e.via !== 'web' ? ` · ${e.via === 'mcp' ? 'MCP' : t('API key')}` : ''}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {list.items && <LoadMore list={list} />}
      {retention > 0 && <p className="muted small activity-note">{t('Events are kept for {days} days.', { days: retention })}</p>}
    </>
  );
}

// ---------------------------------------------------------------------------
// Settings → Activity
// ---------------------------------------------------------------------------

export default function Activity() {
  const { isAdmin, collections } = useApp();
  const [tab, setTab] = useState('people');
  const [days, setDays] = useState(30);
  const [userId, setUserId] = useState('');
  const [collectionId, setCollectionId] = useState('');
  const [members, setMembers] = useState([]);

  useEffect(() => {
    if (isAdmin) api('users.list').then(setMembers).catch(() => {});
  }, [isAdmin]);

  if (!isAdmin) return <Empty>{t('Only workspace admins can see the activity.')}</Empty>;

  const people = [{ value: '', label: t('Everyone'), translate: false }, ...members.map((m) => ({ value: m.id, label: m.name, translate: false }))];
  return (
    <>
      <h1 className="page-title">{t('Activity')}</h1>
      <p className="page-subtitle muted">
        {t('What people do in this workspace: who is active, which documents they changed, sign-ins and changes to members, access and settings.')}
      </p>
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'people', label: t('People') },
          { value: 'changes', label: t('Changes to documents') },
          { value: 'log', label: t('Audit log') },
        ]}
      />
      <div className="activity-filters">
        <Select value={days} onChange={(v) => setDays(Number(v))} options={PERIODS} />
        {tab !== 'people' && <Select value={userId} onChange={setUserId} options={people} />}
        {tab === 'changes' && (
          <Select
            value={collectionId}
            onChange={setCollectionId}
            options={[{ value: '', label: t('All collections'), translate: false }, ...collections.map((c) => ({ value: c.id, label: c.name, translate: false }))]}
          />
        )}
      </div>
      {tab === 'people' && (
        <People
          days={days}
          onPick={(id) => {
            setUserId(id);
            setTab('changes');
          }}
        />
      )}
      {tab === 'changes' && <Changes days={days} userId={userId} collectionId={collectionId} />}
      {tab === 'log' && <EventLog method="activity.events" userId={userId} days={days} />}
    </>
  );
}

/** General settings → Audit log: the events of the installation */
export function InstanceAudit() {
  const { isSuperadmin } = useApp();
  const [days, setDays] = useState(30);
  if (!isSuperadmin) return <Empty>{t('Only superadmins can see the audit log of the installation.')}</Empty>;
  return (
    <>
      <h1 className="page-title">{t('Audit log')}</h1>
      <p className="page-subtitle muted">
        {t('Sign-ins and failed sign-ins, new accounts, general settings, superadmins and workspaces created or deleted. What happens inside a workspace is in its Settings → Activity, for its admins.')}
      </p>
      <div className="activity-filters">
        <Select value={days} onChange={(v) => setDays(Number(v))} options={PERIODS} />
      </div>
      <EventLog method="instance.events" days={days} />
    </>
  );
}
