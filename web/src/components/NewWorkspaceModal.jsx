import React, { useState } from 'react';
import { api } from '../api';
import { useToast } from './Toast';
import { Button, Field, Modal } from './ui';

export default function NewWorkspaceModal({ onClose }) {
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api('teams.create', { name });
      window.location.href = '/';
    } catch (err) {
      toast(err.message, 'error');
      setBusy(false);
    }
  };
  return (
    <Modal title="Create a workspace" onClose={onClose}>
      <form onSubmit={submit} className="form">
        <p className="modal-text">
          Workspaces are completely separate knowledge bases with their own members, collections and settings. You can
          switch between them at any time from the workspace menu.
        </p>
        <Field label="Workspace name">
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Acme Inc." required />
        </Field>
        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {busy ? 'Creating…' : 'Create workspace'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
