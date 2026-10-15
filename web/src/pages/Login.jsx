import React, { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { api } from '../api';
import { Button, Field } from '../components/ui';
import { t } from '../i18n';
import { LogoFull } from '../components/Logo';
import { getProductName } from '../prefs';

export { LogoMark as Logo } from '../components/Logo';

export function MicrosoftButton({ redirect, invite, label = t('Continue with Microsoft') }) {
  const params = new URLSearchParams();
  if (redirect && redirect !== '/') params.set('redirect', redirect);
  if (invite) params.set('invite', invite);
  const qs = params.toString();
  return (
    <a href={`/auth/microsoft${qs ? `?${qs}` : ''}`} className="btn btn-secondary btn-block btn-microsoft">
      <svg width="18" height="18" viewBox="0 0 21 21" aria-hidden="true">
        <rect x="1" y="1" width="9" height="9" fill="#f25022" />
        <rect x="11" y="1" width="9" height="9" fill="#7fba00" />
        <rect x="1" y="11" width="9" height="9" fill="#00a4ef" />
        <rect x="11" y="11" width="9" height="9" fill="#ffb900" />
      </svg>
      {label}
    </a>
  );
}

export function OrDivider() {
  return (
    <div className="or-divider">
      <span>{t('or')}</span>
    </div>
  );
}

export function AuthShell({ title, subtitle, children }) {
  return (
    <div className="auth-page">
      <div className="auth-card">
        <LogoFull size={40} />
        <h1>{title}</h1>
        {subtitle && <p className="auth-subtitle">{subtitle}</p>}
        {children}
      </div>
    </div>
  );
}

export default function Login() {
  const [params] = useSearchParams();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState(params.get('error'));
  const [busy, setBusy] = useState(false);
  const [config, setConfig] = useState(null);
  const redirect = params.get('redirect') || '/';

  useEffect(() => {
    document.title = t('Login - {product}', { product: getProductName() });
    api('auth.config').then(setConfig).catch(() => {});
  }, []);

  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api('auth.login', { email, password });
      window.location.href = redirect.startsWith('/') ? redirect : '/';
    } catch (err) {
      setError(err.message);
      setBusy(false);
    }
  };

  if (!config) return null;

  if (config.isFirstUser && config.passwordLogin) {
    return (
      <AuthShell title={t('Welcome to {product}', { product: getProductName() })} subtitle={t('Let\'s get started by creating the first account and workspace.')}>
        {config.microsoft && <MicrosoftButton />}
        {config.microsoft && <OrDivider />}
        <Link to="/register" className="btn btn-primary btn-block">
          {t('Create account')}
        </Link>
      </AuthShell>
    );
  }

  return (
    <AuthShell title={t('Login to {product}', { product: getProductName() })}>
      {config.microsoft && <MicrosoftButton redirect={redirect} />}
      {config.microsoft && config.passwordLogin && <OrDivider />}
      {!config.passwordLogin && error && <div className="error-text">{error}</div>}
      {config.passwordLogin && (
      <form onSubmit={submit} className="form">
        <Field label={t('Email')}>
          <input type="email" autoFocus value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" />
        </Field>
        <Field label={t('Password')}>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            autoComplete="current-password"
          />
        </Field>
        {error && <div className="error-text">{error}</div>}
        <Button type="submit" disabled={busy} className="btn-block">
          {busy ? t('Signing in…') : t('Continue with email')}
        </Button>
      </form>
      )}
      {config.allowSignup && (
        <p className="auth-footer">
          {t('Don’t have an account?')}{' '}
          <Link to={`/register${redirect !== '/' ? `?redirect=${encodeURIComponent(redirect)}` : ''}`}>{t('Sign up')}</Link>
        </p>
      )}
    </AuthShell>
  );
}
