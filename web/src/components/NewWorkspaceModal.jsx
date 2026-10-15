import React, { useState } from 'react';
import { api } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import { Button, Field, Modal } from './ui';
import { t } from '../i18n';

export default function NewWorkspaceModal({ onClose }) {
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  // outside the app (no workspace yet) there is nothing to rebuild: reload
  const openWorkspace = useApp()?.openWorkspace;
  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api('teams.create', { name });
      if (openWorkspace) {
        onClose();
        await openWorkspace('/');
      } else window.location.href = '/';
    } catch (err) {
      toast(err.message, 'error');
      setBusy(false);
    }
  };
  return (
    <Modal title={t('Create a workspace')} onClose={onClose}>
      <form onSubmit={submit} className="form">
        <p className="modal-text">
          {t(
            'Workspaces are completely separate knowledge bases with their own members, collections and settings. You can switch between them at any time from the workspace menu.'
          )}
        </p>
        <Field label={t('Workspace name')}>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t('e.g. Acme Inc.')} required />
        </Field>
        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {busy ? t('Creating…') : t('Create workspace')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
