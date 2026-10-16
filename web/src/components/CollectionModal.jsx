import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Trash2, Users } from 'lucide-react';
import { api, collectionUrl } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import PeopleSearch from './PeopleSearch';
import { Button, Field, Modal, Switch, Avatar } from './ui';
import { COLLECTION_ICONS, COLORS } from './icons';
import { t } from '../i18n';

// English labels for the icon picker tooltips (translated where rendered).
const ICON_LABELS = {
  collection: 'Collection',
  book: 'Book',
  home: 'House',
  star: 'Five-pointed star',
  code: 'Code',
  briefcase: 'Briefcase',
  cloud: 'Cloud',
  coffee: 'Coffee',
  database: 'Database',
  flag: 'Flag',
  globe: 'Globe',
  heart: 'Heart',
  lightbulb: 'Lightbulb',
  rocket: 'Rocket',
  settings: 'Gear',
  users: 'Users',
  tool: 'Tool',
  calendar: 'Calendar',
  camera: 'Camera',
  leaf: 'Leaf',
  palette: 'Palette',
  shield: 'Shield',
  zap: 'Lightning',
  academic: 'Education',
  music: 'Music',
  map: 'Map',
  package: 'Package',
  lock: 'Lock',
};

function MembersEditor({ collection }) {
  const { auth } = useApp();
  const [members, setMembers] = useState([]);
  const toast = useToast();
  const load = () => api('collections.memberships', { id: collection.id }).then(setMembers);
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collection.id]);
  const add = async (u) => {
    try {
      await api('collections.addUser', { id: collection.id, userId: u.id, permission: 'read_write' });
      await load();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return (
    <div className="members-editor">
      <div className="field-label">{t('Members with access')}</div>
      {members.map((m) => (
        <div key={m.id} className="member-row">
          <Avatar name={m.name} color={m.color} size={24} />
          <span className="member-name">
            {m.name} {m.id === auth.user.id && <span className="muted">{t('(you)')}</span>}
          </span>
          <select
            value={m.permission}
            onChange={async (e) => {
              await api('collections.addUser', { id: collection.id, userId: m.id, permission: e.target.value });
              load();
            }}
          >
            <option value="read_write">{t('Can edit')}</option>
            <option value="read">{t('View only')}</option>
          </select>
          <button
            className="icon-btn"
            title={t('Remove')}
            onClick={async () => {
              await api('collections.removeUser', { id: collection.id, userId: m.id });
              load();
            }}
          >
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      {members.length === 0 && <div className="muted small">{t('No individual members yet.')}</div>}
      <PeopleSearch exclude={members.map((m) => m.id)} onAdd={add} />
    </div>
  );
}

/** Groups with access to the collection: every member of a group gets its permission */
function GroupsEditor({ collection }) {
  const [groups, setGroups] = useState([]);
  const [all, setAll] = useState([]);
  const [selected, setSelected] = useState('');
  const toast = useToast();
  const load = () => api('collections.groupMemberships', { id: collection.id }).then(setGroups);
  useEffect(() => {
    load();
    api('groups.list').then(setAll);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collection.id]);
  const run = async (method, body) => {
    try {
      await api(method, { id: collection.id, ...body });
      await load();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const candidates = all.filter((g) => !groups.some((m) => m.id === g.id));
  if (!groups.length && !all.length) return null;
  return (
    <div className="members-editor">
      <div className="field-label">{t('Groups with access')}</div>
      {groups.map((g) => (
        <div key={g.id} className="member-row">
          <Users size={18} className="muted" />
          <span className="member-name">
            {g.name}{' '}
            <span className="muted small">{t(g.memberCount === 1 ? '{count} member' : '{count} members', { count: g.memberCount })}</span>
          </span>
          <select value={g.permission} onChange={(e) => run('collections.addGroup', { groupId: g.id, permission: e.target.value })}>
            <option value="read_write">{t('Can edit')}</option>
            <option value="read">{t('View only')}</option>
          </select>
          <button type="button" className="icon-btn" title={t('Remove')} onClick={() => run('collections.removeGroup', { groupId: g.id })}>
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      {groups.length === 0 && <div className="muted small">{t('No groups yet.')}</div>}
      {candidates.length > 0 && (
        <div className="row gap">
          <select value={selected} onChange={(e) => setSelected(e.target.value)} className="grow">
            <option value="">{t('Add a group…')}</option>
            {candidates.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
          <Button
            type="button"
            variant="secondary"
            disabled={!selected}
            onClick={async () => {
              await run('collections.addGroup', { groupId: selected, permission: 'read_write' });
              setSelected('');
            }}
          >
            {t('Add')}
          </Button>
        </div>
      )}
    </div>
  );
}

export default function CollectionModal({ collection, onClose }) {
  const { refreshCollections, refreshTree } = useApp();
  const navigate = useNavigate();
  const toast = useToast();
  const [name, setName] = useState(collection?.name || '');
  const [description, setDescription] = useState(collection?.description || '');
  const [color, setColor] = useState(collection?.color || COLORS[1]);
  const [icon, setIcon] = useState(collection?.icon || 'collection');
  const [permission, setPermission] = useState(
    collection ? collection.permission || 'private' : 'read_write'
  );
  const [sharing, setSharing] = useState(collection?.sharing ?? true);
  const [busy, setBusy] = useState(false);

  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      if (collection) {
        await api('collections.update', { id: collection.id, name, description, color, icon, permission, sharing });
        await refreshCollections();
        toast(t('Collection updated'));
        onClose();
      } else {
        const c = await api('collections.create', { name, description, color, icon, permission });
        await refreshCollections();
        await refreshTree(c.id);
        onClose();
        navigate(collectionUrl(c));
      }
    } catch (err) {
      toast(err.message, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={collection ? t('Edit collection') : t('New collection')} onClose={onClose} width={540}>
      <form onSubmit={submit} className="form">
        <p className="modal-text">
          {t('Collections are used to group documents and choose permissions.')}
        </p>
        <Field label={t('Name')}>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t('e.g. Engineering')} required />
        </Field>
        <Field label={t('Icon')}>
          <div className="icon-picker">
            {Object.entries(COLLECTION_ICONS).map(([key, Icon]) => (
              <button
                type="button"
                key={key}
                className={`icon-choice ${icon === key ? 'selected' : ''}`}
                onClick={() => setIcon(key)}
                title={t(ICON_LABELS[key] || key)}
              >
                <Icon size={18} color={color} />
              </button>
            ))}
          </div>
          <div className="color-picker">
            {COLORS.map((c) => (
              <button
                type="button"
                key={c}
                className={`color-choice ${color === c ? 'selected' : ''}`}
                style={{ background: c }}
                onClick={() => setColor(c)}
              />
            ))}
          </div>
        </Field>
        <Field label={t('Description')}>
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t('What is this collection about? (optional)')}
          />
        </Field>
        <Field
          label={t('Permission')}
          hint={
            permission === 'private'
              ? t('Only members added to this collection and workspace admins can see it.')
              : permission === 'read'
              ? t('Everyone in the workspace can view, only members added explicitly can edit.')
              : t('Everyone in the workspace can view and edit documents.')
          }
        >
          <select value={permission} onChange={(e) => setPermission(e.target.value)}>
            <option value="read_write">{t('Workspace members can view and edit')}</option>
            <option value="read">{t('Workspace members can view')}</option>
            <option value="private">{t('Private (no default access)')}</option>
          </select>
        </Field>
        {collection && (
          <div className="field row space-between">
            <div>
              <div className="field-label">{t('Public document sharing')}</div>
              <div className="field-hint">{t('Allow documents in this collection to be shared publicly on the internet.')}</div>
            </div>
            <Switch checked={sharing} onChange={setSharing} />
          </div>
        )}
        {collection && (permission !== 'read_write') && <MembersEditor collection={collection} />}
        {collection && permission !== 'read_write' && <GroupsEditor collection={collection} />}
        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {collection ? t('Save') : busy ? t('Creating…') : t('Create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
