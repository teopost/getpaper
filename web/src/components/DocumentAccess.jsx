import React, { useEffect, useState } from 'react';
import { Trash2, Users } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import { Avatar, Button } from './ui';
import { useToast } from './Toast';
import PeopleSearch from './PeopleSearch';
import { t } from '../i18n';

function PermissionSelect({ value, onChange, disabled }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled}>
      <option value="read_write">{t('Can edit')}</option>
      <option value="read">{t('View only')}</option>
    </select>
  );
}

/**
 * People and groups a document is shared with, beyond its collection, as in Outline. The access
 * also applies to nested documents; access given on a parent is shown and changed there.
 */
export default function DocumentAccess({ doc }) {
  const { auth } = useApp();
  const toast = useToast();
  const [data, setData] = useState(null);
  const [groups, setGroups] = useState([]);
  const [group, setGroup] = useState('');
  const [adding, setAdding] = useState(false);

  const load = () =>
    api('documents.memberships', { id: doc.id })
      .then(setData)
      .catch(() => setData({ users: [], groups: [], canManage: false }));
  useEffect(() => {
    load();
    api('groups.list').then(setGroups).catch(() => setGroups([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.id]);

  const run = async (method, body, message) => {
    try {
      await api(method, { id: doc.id, ...body });
      await load();
      if (message) toast(message);
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  if (!data) return null;
  const { canManage } = data;
  const direct = (rows) => rows.filter((r) => !r.inheritedFromId);
  const candidates = groups.filter((g) => !direct(data.groups).some((m) => m.id === g.id));
  if (!canManage && !data.users.length && !data.groups.length) return null;

  const inherited = (r) =>
    r.inheritedFromId && (
      <span className="muted small doc-access-inherited">{t('From “{title}”', { title: r.inheritedFromTitle || t('Untitled') })}</span>
    );

  return (
    <div className="doc-access">
      <div className="share-title">{t('People with access')}</div>
      <div className="muted small">{t('Besides the members of the collection. Nested documents are shared too.')}</div>
      {data.users.map((u) => (
        <div key={`u-${u.id}-${u.inheritedFromId || ''}`} className="member-row">
          <Avatar name={u.name} color={u.color} size={24} />
          <span className="member-name">
            {u.name} {u.id === auth.user.id && <span className="muted">{t('(you)')}</span>}
            {inherited(u)}
          </span>
          <PermissionSelect
            value={u.permission}
            disabled={!canManage || !!u.inheritedFromId}
            onChange={(permission) => run('documents.addUser', { userId: u.id, permission })}
          />
          {canManage && !u.inheritedFromId && (
            <button className="icon-btn" title={t('Remove')} onClick={() => run('documents.removeUser', { userId: u.id })}>
              <Trash2 size={14} />
            </button>
          )}
        </div>
      ))}
      {data.groups.map((g) => (
        <div key={`g-${g.id}-${g.inheritedFromId || ''}`} className="member-row">
          <Users size={18} className="muted" />
          <span className="member-name">
            {g.name}{' '}
            <span className="muted small">{t(g.memberCount === 1 ? '{count} member' : '{count} members', { count: g.memberCount })}</span>
            {inherited(g)}
          </span>
          <PermissionSelect
            value={g.permission}
            disabled={!canManage || !!g.inheritedFromId}
            onChange={(permission) => run('documents.addGroup', { groupId: g.id, permission })}
          />
          {canManage && !g.inheritedFromId && (
            <button className="icon-btn" title={t('Remove')} onClick={() => run('documents.removeGroup', { groupId: g.id })}>
              <Trash2 size={14} />
            </button>
          )}
        </div>
      ))}
      {canManage &&
        (adding ? (
          <>
            <PeopleSearch
              autoFocus
              exclude={direct(data.users).map((u) => u.id)}
              onAdd={(u) => run('documents.addUser', { userId: u.id, permission: 'read' }, t('Shared with {name}', { name: u.name }))}
            />
            {candidates.length > 0 && (
              <div className="row gap">
                <select value={group} onChange={(e) => setGroup(e.target.value)} className="grow">
                  <option value="">{t('Add a group…')}</option>
                  {candidates.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
                <Button
                  variant="secondary"
                  disabled={!group}
                  onClick={async () => {
                    await run('documents.addGroup', { groupId: group, permission: 'read' });
                    setGroup('');
                  }}
                >
                  {t('Add')}
                </Button>
              </div>
            )}
          </>
        ) : (
          <Button variant="secondary" className="btn-block" onClick={() => setAdding(true)}>
            {t('Share with people or groups…')}
          </Button>
        ))}
    </div>
  );
}
