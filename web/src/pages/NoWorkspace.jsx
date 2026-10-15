import React, { useState } from 'react';
import { api } from '../api';
import { Button, Field } from '../components/ui';
import { AuthShell } from './Login';

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
  return (
    <AuthShell
      title="No workspace"
      subtitle={`Hi ${auth.user.name}, you are not a member of any workspace yet. Create a new one, or ask someone to invite you.`}
    >
      <form className="form" onSubmit={submit}>
        <Field label="Workspace name">
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        {error && <div className="error-text">{error}</div>}
        <Button type="submit" className="btn-block" disabled={busy}>
          Create workspace
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
          Log out
        </Button>
      </form>
    </AuthShell>
  );
}
