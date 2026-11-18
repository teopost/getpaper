import React, { useState } from 'react';
import { api } from '../api';
import { Button, Field } from '../components/ui';
import { AuthShell } from './Login';
import { t } from '../i18n';

export default function NoWorkspace({ auth }) {
  const [name, setName] = useState('');
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api('teams.create', { name });
      window.location.href = '/';
    } catch (err) {
      setError(err.message);
      setBusy(false);
    }
  };
  const logout = async () => {
    await api('auth.logout');
    window.location.href = '/login';
  };
  // people who signed in only to read a page shared with the organization
  if (auth.canCreateWorkspace === false)
    return (
      <AuthShell
        title={t('No workspace')}
        subtitle={t('Hi {name}, you are not a member of any workspace. The pages shared with you open from the link you received.', {
          name: auth.user.name,
        })}
      >
        <Button variant="secondary" type="button" className="btn-block" onClick={logout}>
          {t('Log out')}
        </Button>
      </AuthShell>
    );
  return (
    <AuthShell
      title={t('No workspace')}
      subtitle={t('Hi {name}, you are not a member of any workspace yet. Create a new one, or ask someone to invite you.', { name: auth.user.name })}
    >
      <form className="form" onSubmit={submit}>
        <Field label={t('Workspace name')}>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        {error && <div className="error-text">{error}</div>}
        <Button type="submit" className="btn-block" disabled={busy}>
          {t('Create workspace')}
        </Button>
        <Button
          variant="secondary"
          type="button"
          className="btn-block"
          onClick={async () => {
            await api('auth.logout');
            window.location.href = '/login';
          }}
        >
          {t('Log out')}
        </Button>
      </form>
    </AuthShell>
  );
}
