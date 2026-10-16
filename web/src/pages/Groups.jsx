import React, { useEffect, useState } from 'react';
import { Plus, Trash2, Users } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import { Avatar, Button, ConfirmModal, Empty, Field, Modal, Switch } from '../components/ui';
import { useToast } from '../components/Toast';
import PeopleSearch from '../components/PeopleSearch';
import { t } from '../i18n';

/** Create (no group) or edit a group */
function GroupModal({ group, onClose, onSaved }) {
  const toast = useToast();
  const [name, setName] = useState(group?.name || '');
  const [description, setDescription] = useState(group?.description || '');
  const [mentions, setMentions] = useState(!group?.disableMentions);
  const [busy, setBusy] = useState(false);
  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      const body = { name, description, disableMentions: !mentions };
      const saved = group ? await api('groups.update', { id: group.id, ...body }) : await api('groups.create', body);
      toast(group ? t('Group updated') : t('Group created'));
      onSaved(saved, !group);
      onClose();
    } catch (err) {
      toast(err.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title={group ? t('Edit group') : t('New group')} onClose={onClose}>
      <form onSubmit={submit} className="form">
        <p className="modal-text">
          {t('Groups bring people together: give a group access to a collection, or mention it with @ to notify all of its members.')}
        </p>
        <Field label={t('Name')}>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t('e.g. Engineering')} maxLength={255} required />
        </Field>
        <Field label={t('Description')}>
          <textarea rows={2} value={description} onChange={(e) => setDescription(e.target.value)} maxLength={1000} placeholder={t('What is this group for? (optional)')} />
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">{t('Allow mentions')}</div>
            <div className="field-hint">{t('Members can mention the group with @ in documents to notify everyone in it.')}</div>
          </div>
          <Switch checked={mentions} onChange={setMentions} />
        </div>
        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {group ? t('Save') : busy ? t('Creating…') : t('Create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

/** Members of a group; those who can update the group add, remove and promote them */
function GroupMembersModal({ group, onClose, onChanged }) {
  const { user } = useApp();
  const toast = useToast();
  const [members, setMembers] = useState(null);
  const load = () =>
    api('groups.memberships', { id: group.id })
      .then(setMembers)
      .catch((err) => toast(err.message, 'error'));
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [group.id]);
  const run = async (method, body) => {
    try {
      await api(method, { id: group.id, ...body });
      await load();
      onChanged();
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  return (
    <Modal title={t('Members of {name}', { name: group.name })} onClose={onClose} width={540}>
      <div className="members-editor">
        {group.canUpdate && members && (
          <>
            <div className="field-label">{t('Add people')}</div>
            <PeopleSearch autoFocus exclude={members.map((m) => m.id)} onAdd={(u) => run('groups.addUser', { userId: u.id })} />
          </>
        )}
        {members && <div className="field-label">{t(members.length === 1 ? '{count} member' : '{count} members', { count: members.length })}</div>}
        <div className="member-list">
          {members?.map((m) => (
            <div key={m.id} className="member-row">
              <Avatar name={m.name} color={m.color} size={24} />
              <span className="member-name">
                {m.name} {m.id === user.id && <span className="muted">{t('(you)')}</span>}
                <div className="muted small">{m.email}</div>
              </span>
              {group.canUpdate ? (
                <>
                  <select value={m.permission} onChange={(e) => run('groups.addUser', { userId: m.id, permission: e.target.value })}>
                    <option value="member">{t('Member')}</option>
                    <option value="admin">{t('Group admin')}</option>
                  </select>
                  <button className="icon-btn" title={t('Remove')} onClick={() => run('groups.removeUser', { userId: m.id })}>
                    <Trash2 size={14} />
                  </button>
                </>
              ) : (
                <span className="badge">{m.permission === 'admin' ? t('Group admin') : t('Member')}</span>
              )}
            </div>
          ))}
          {members?.length === 0 && <div className="muted small">{t('This group has no members yet.')}</div>}
        </div>
      </div>
      <div className="modal-actions">
        <Button onClick={onClose}>{t('Done')}</Button>
      </div>
    </Modal>
  );
}

/** Settings → Groups */
export default function Groups() {
  const { isAdmin } = useApp();
  const toast = useToast();
  const [groups, setGroups] = useState(null);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState(null); // {} for a new group
  const [members, setMembers] = useState(null);
  const [deleting, setDeleting] = useState(null);

  const load = () =>
    api('groups.list', { query })
      .then(setGroups)
      .catch((err) => toast(err.message, 'error'));
  useEffect(() => {
    const timer = setTimeout(load, 200);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  return (
    <>
      <div className="row space-between">
        <h1 className="page-title">{t('Groups')}</h1>
        {isAdmin && (
          <Button icon={<Plus size={16} />} onClick={() => setEditing({})}>
            {t('New group…')}
          </Button>
        )}
      </div>
      <p className="page-subtitle muted">
        {t(
          'Groups can be given access to collections and mentioned with @ in documents to notify all of their members. Admins create groups; workspace admins and group admins manage their members.'
        )}
      </p>
      <input placeholder={t('Filter…')} value={query} onChange={(e) => setQuery(e.target.value)} className="filter-input" />
      {groups?.length === 0 ? (
        <Empty>{query ? t('No groups match the filter.') : t('No groups yet.')}</Empty>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t('Name')}</th>
                <th>{t('Members')}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {groups?.map((g) => (
                <tr key={g.id}>
                  <td>
                    <span className="row gap-sm">
                      <Users size={18} className="muted" />
                      <span>
                        {g.name}
                        {g.myPermission && <span className="muted"> {g.myPermission === 'admin' ? t('(group admin)') : t('(member)')}</span>}
                        {g.disableMentions && <> <span className="badge">{t('Mentions off')}</span></>}
                        {g.description && <div className="muted small">{g.description}</div>}
                      </span>
                    </span>
                  </td>
                  <td className="muted">{t(g.memberCount === 1 ? '{count} member' : '{count} members', { count: g.memberCount })}</td>
                  <td className="right">
                    <span className="row gap-sm justify-end">
                      <Button variant="secondary" small onClick={() => setMembers(g)}>
                        {g.canUpdate ? t('Manage members') : t('Members')}
                      </Button>
                      {g.canUpdate && (
                        <Button variant="secondary" small onClick={() => setEditing(g)}>
                          {t('Edit')}
                        </Button>
                      )}
                      {isAdmin && (
                        <Button variant="secondary" small onClick={() => setDeleting(g)}>
                          {t('Delete')}
                        </Button>
                      )}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing && (
        <GroupModal
          group={editing.id ? editing : null}
          onClose={() => setEditing(null)}
          onSaved={(g, created) => {
            load();
            if (created) setMembers(g);
          }}
        />
      )}
      {members && <GroupMembersModal group={members} onClose={() => setMembers(null)} onChanged={load} />}
      {deleting && (
        <ConfirmModal
          title={t('Delete group')}
          danger
          confirmLabel={t('Delete')}
          message={t('The members of {name} lose the access given through the group. People and documents are kept.', { name: deleting.name })}
          onClose={() => setDeleting(null)}
          onConfirm={async () => {
            try {
              await api('groups.delete', { id: deleting.id });
              toast(t('Group deleted'));
              load();
            } catch (err) {
              toast(err.message, 'error');
            }
          }}
        />
      )}
    </>
  );
}
