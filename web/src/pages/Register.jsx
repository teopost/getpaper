import React, { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../api';
import { Button, Field } from '../components/ui';
import { AuthShell, MicrosoftButton, OrDivider } from './Login';

export function RegisterForm({ inviteToken, defaultEmail = '', teamName }) {
  const [name, setName] = useState('');
  const [email, setEmail] = useState(defaultEmail);
  const [password, setPassword] = useState('');
  const [workspace, setWorkspace] = useState('');
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api('auth.register', {
        name,
        email,
        password,
        teamName: inviteToken ? undefined : workspace,
        inviteToken,
      });
      window.location.href = '/';
    } catch (err) {
      setError(err.message);
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="form">
      <Field label="Full name">
        <input autoFocus value={name} onChange={(e) => setName(e.target.value)} required autoComplete="name" />
      </Field>
      <Field label="Email">
        <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" />
      </Field>
      <Field label="Password" hint="At least 8 characters.">
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
          autoComplete="new-password"
        />
      </Field>
      {!inviteToken && (
        <Field label="Workspace name" hint="You can create more workspaces later.">
          <input value={workspace} onChange={(e) => setWorkspace(e.target.value)} placeholder="e.g. Acme Inc." />
        </Field>
      )}
      {error && <div className="error-text">{error}</div>}
      <Button type="submit" disabled={busy} className="btn-block">
        {busy ? 'Creating account…' : inviteToken ? `Join ${teamName || 'workspace'}` : 'Create account'}
      </Button>
    </form>
  );
}

export default function Register() {
  const [config, setConfig] = useState(null);
  useEffect(() => {
    document.title = 'Sign up - Horsa Codex';
    api('auth.config').then(setConfig).catch(() => setConfig({ allowSignup: false }));
  }, []);
  if (!config) return null;
  if (!config.passwordLogin && config.microsoft) {
    return (
      <AuthShell title="Create an account" subtitle="Accounts are created automatically the first time you sign in with Microsoft.">
        <MicrosoftButton />
      </AuthShell>
    );
  }
  if (!config.allowSignup) {
    return (
      <AuthShell title="Sign ups are disabled" subtitle="Ask a workspace admin to send you an invite link.">
        <Link to="/login" className="btn btn-secondary btn-block">
          Back to login
        </Link>
      </AuthShell>
    );
  }
  return (
    <AuthShell
      title={config.isFirstUser ? 'Create the first account' : 'Create an account'}
      subtitle="A new workspace will be created for you."
    >
      {config.microsoft && (
        <>
          <MicrosoftButton label="Sign up with Microsoft" />
          <OrDivider />
        </>
      )}
      <RegisterForm />
      <p className="auth-footer">
        Already have an account? <Link to="/login">Log in</Link>
      </p>
    </AuthShell>
  );
}
