import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Trash2 } from 'lucide-react';
import { api, collectionUrl } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import { Button, Field, Modal, Switch, Avatar } from './ui';
import { COLLECTION_ICONS, COLORS } from './icons';

function MembersEditor({ collection }) {
  const { auth } = useApp();
  const [members, setMembers] = useState([]);
  const [all, setAll] = useState([]);
  const [selected, setSelected] = useState('');
  const toast = useToast();
  const load = () => api('collections.memberships', { id: collection.id }).then(setMembers);
  useEffect(() => {
    load();
    api('users.list').then(setAll);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collection.id]);
  const add = async () => {
    if (!selected) return;
    try {
      await api('collections.addUser', { id: collection.id, userId: selected, permission: 'read_write' });
      setSelected('');
      load();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const candidates = all.filter((u) => !members.some((m) => m.id === u.id));
  return (
    <div className="members-editor">
      <div className="field-label">Members with access</div>
      {members.map((m) => (
        <div key={m.id} className="member-row">
          <Avatar name={m.name} color={m.color} size={24} />
          <span className="member-name">
            {m.name} {m.id === auth.user.id && <span className="muted">(you)</span>}
          </span>
          <select
            value={m.permission}
            onChange={async (e) => {
              await api('collections.addUser', { id: collection.id, userId: m.id, permission: e.target.value });
              load();
            }}
          >
            <option value="read_write">Can edit</option>
            <option value="read">View only</option>
          </select>
          <button
            className="icon-btn"
            title="Remove"
            onClick={async () => {
              await api('collections.removeUser', { id: collection.id, userId: m.id });
              load();
            }}
          >
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      {members.length === 0 && <div className="muted small">No individual members yet.</div>}
      {candidates.length > 0 && (
        <div className="row gap">
          <select value={selected} onChange={(e) => setSelected(e.target.value)} className="grow">
            <option value="">Add a member…</option>
            {candidates.map((u) => (
              <option key={u.id} value={u.id}>
                {u.name} ({u.email})
              </option>
            ))}
          </select>
          <Button variant="secondary" onClick={add} disabled={!selected}>
            Add
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
        toast('Collection updated');
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
    <Modal title={collection ? 'Edit collection' : 'New collection'} onClose={onClose} width={540}>
      <form onSubmit={submit} className="form">
        <p className="modal-text">
          Collections are used to group documents and choose permissions.
        </p>
        <Field label="Name">
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Engineering" required />
        </Field>
        <Field label="Icon">
          <div className="icon-picker">
            {Object.entries(COLLECTION_ICONS).map(([key, Icon]) => (
              <button
                type="button"
                key={key}
                className={`icon-choice ${icon === key ? 'selected' : ''}`}
                onClick={() => setIcon(key)}
                title={key}
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
        <Field label="Description">
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What is this collection about? (optional)"
          />
        </Field>
        <Field
          label="Permission"
          hint={
            permission === 'private'
              ? 'Only members added to this collection and workspace admins can see it.'
              : permission === 'read'
              ? 'Everyone in the workspace can view, only members added explicitly can edit.'
              : 'Everyone in the workspace can view and edit documents.'
          }
        >
          <select value={permission} onChange={(e) => setPermission(e.target.value)}>
            <option value="read_write">Workspace members can view and edit</option>
            <option value="read">Workspace members can view</option>
            <option value="private">Private (no default access)</option>
          </select>
        </Field>
        {collection && (
          <div className="field row space-between">
            <div>
              <div className="field-label">Public document sharing</div>
              <div className="field-hint">Allow documents in this collection to be shared publicly on the internet.</div>
            </div>
            <Switch checked={sharing} onChange={setSharing} />
          </div>
        )}
        {collection && (permission !== 'read_write') && <MembersEditor collection={collection} />}
        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {collection ? 'Save' : busy ? 'Creating…' : 'Create'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
