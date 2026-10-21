import React, { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../api';
import { Button, Field } from '../components/ui';
import { t } from '../i18n';
import { getProductName } from '../prefs';
import { AuthShell, OrDivider, SsoButtons, hasSso } from './Login';

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
      <Field label={t('Full name')}>
        <input autoFocus value={name} onChange={(e) => setName(e.target.value)} required autoComplete="name" />
      </Field>
      <Field label={t('Email')}>
        <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" />
      </Field>
      <Field label={t('Password')} hint={t('At least 8 characters.')}>
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
        <Field label={t('Workspace name')} hint={t('You can create more workspaces later.')}>
          <input value={workspace} onChange={(e) => setWorkspace(e.target.value)} placeholder={t('e.g. Acme Inc.')} />
        </Field>
      )}
      {error && <div className="error-text">{error}</div>}
      <Button type="submit" disabled={busy} className="btn-block">
        {busy ? t('Creating account…') : inviteToken ? (teamName ? t('Join {name}', { name: teamName }) : t('Join workspace')) : t('Create account')}
      </Button>
    </form>
  );
}

export default function Register() {
  const [config, setConfig] = useState(null);
  useEffect(() => {
    document.title = t('Sign up - {product}', { product: getProductName() });
    api('auth.config').then(setConfig).catch(() => setConfig({ allowSignup: false }));
  }, []);
  if (!config) return null;
  if (!config.passwordLogin && hasSso(config)) {
    return (
      <AuthShell
        title={t('Create an account')}
        subtitle={
          config.microsoft && !config.providers?.length
            ? t('Accounts are created automatically the first time you sign in with Microsoft.')
            : t('Accounts are created the first time you sign in with single sign-on.')
        }
      >
        <SsoButtons config={config} />
      </AuthShell>
    );
  }
  if (!config.allowSignup) {
    return (
      <AuthShell title={t('Sign ups are disabled')} subtitle={t('Ask a workspace admin to send you an invite link.')}>
        <Link to="/login" className="btn btn-secondary btn-block">
          {t('Back to login')}
        </Link>
      </AuthShell>
    );
  }
  return (
    <AuthShell
      title={config.isFirstUser ? t('Create the first account') : t('Create an account')}
      subtitle={t('A new workspace will be created for you.')}
    >
      {hasSso(config) && (
        <>
          <SsoButtons config={config} verb="signup" />
          <OrDivider />
        </>
      )}
      <RegisterForm />
      <p className="auth-footer">
        {t('Already have an account?')} <Link to="/login">{t('Log in')}</Link>
      </p>
    </AuthShell>
  );
}
