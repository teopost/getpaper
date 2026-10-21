import React, { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Fingerprint, KeyRound, Mail } from 'lucide-react';
import { api } from '../api';
import { passkeyError, passkeysSupported, getPasskey } from '../passkeys';
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

function ProviderIcon({ id }) {
  if (id === 'google')
    return (
      <svg width="18" height="18" viewBox="0 0 48 48" aria-hidden="true">
        <path fill="#EA4335" d="M24 9.5c3.5 0 6.6 1.2 9.1 3.6l6.8-6.8C35.8 2.4 30.3 0 24 0 14.6 0 6.6 5.4 2.6 13.3l7.9 6.1C12.4 13.7 17.7 9.5 24 9.5z" />
        <path fill="#4285F4" d="M46.5 24.5c0-1.6-.1-3.1-.4-4.5H24v9h12.7c-.6 3-2.3 5.5-4.8 7.2l7.7 6c4.5-4.2 6.9-10.3 6.9-17.7z" />
        <path fill="#FBBC05" d="M10.5 28.6c-.5-1.4-.8-3-.8-4.6s.3-3.2.8-4.6l-7.9-6.1C.9 16.6 0 20.2 0 24s.9 7.4 2.6 10.7l7.9-6.1z" />
        <path fill="#34A853" d="M24 48c6.5 0 11.9-2.1 15.9-5.8l-7.7-6c-2.1 1.4-4.9 2.3-8.2 2.3-6.3 0-11.6-4.2-13.5-10l-7.9 6.1C6.6 42.6 14.6 48 24 48z" />
      </svg>
    );
  if (id === 'slack')
    return (
      <svg width="18" height="18" viewBox="0 0 54 54" aria-hidden="true">
        <path fill="#36C5F0" d="M19.7 0a5.4 5.4 0 0 0 0 10.8h5.4V5.4A5.4 5.4 0 0 0 19.7 0zm0 14.4H5.4a5.4 5.4 0 0 0 0 10.8h14.3a5.4 5.4 0 0 0 0-10.8z" />
        <path fill="#2EB67D" d="M54 19.8a5.4 5.4 0 0 0-10.8 0v5.4h5.4a5.4 5.4 0 0 0 5.4-5.4zm-14.4 0V5.4a5.4 5.4 0 0 0-10.8 0v14.4a5.4 5.4 0 0 0 10.8 0z" />
        <path fill="#ECB22E" d="M34.2 54a5.4 5.4 0 0 0 0-10.8h-5.4v5.4a5.4 5.4 0 0 0 5.4 5.4zm0-14.4h14.4a5.4 5.4 0 0 0 0-10.8H34.2a5.4 5.4 0 0 0 0 10.8z" />
        <path fill="#E01E5A" d="M0 34.2a5.4 5.4 0 0 0 10.8 0v-5.4H5.4A5.4 5.4 0 0 0 0 34.2zm14.4 0v14.4a5.4 5.4 0 0 0 10.8 0V34.2a5.4 5.4 0 0 0-10.8 0z" />
      </svg>
    );
  return <KeyRound size={18} />;
}

/** True when the installation has a single sign-on provider */
export const hasSso = (config) => !!config.microsoft || (config.providers || []).length > 0;

const SSO_LABELS = {
  continue: [() => t('Continue with Microsoft'), (name) => t('Continue with {provider}', { provider: name })],
  join: [() => t('Join with Microsoft'), (name) => t('Join with {provider}', { provider: name })],
  signup: [() => t('Sign up with Microsoft'), (name) => t('Sign up with {provider}', { provider: name })],
};

/** The buttons of the single sign-on providers: Microsoft, Google, Slack, any OpenID Connect provider */
export function SsoButtons({ config, redirect, invite, verb = 'continue' }) {
  const params = new URLSearchParams();
  if (redirect && redirect !== '/') params.set('redirect', redirect);
  if (invite) params.set('invite', invite);
  const qs = params.toString();
  const [ms, other] = SSO_LABELS[verb];
  return (
    <>
      {config.microsoft && <MicrosoftButton redirect={redirect} invite={invite} label={ms()} />}
      {(config.providers || []).map((p) => (
        <a key={p.id} href={`/auth/${p.id}${qs ? `?${qs}` : ''}`} className="btn btn-secondary btn-block btn-microsoft">
          <ProviderIcon id={p.id} />
          {other(p.name)}
        </a>
      ))}
    </>
  );
}

function PasskeyButton({ redirect, onError }) {
  const [busy, setBusy] = useState(false);
  if (!passkeysSupported()) return null;
  const signIn = async () => {
    setBusy(true);
    onError(null);
    try {
      const { challengeId, options } = await api('auth.passkeyStart');
      const credential = await getPasskey(options);
      await api('auth.passkeyFinish', { challengeId, credential });
      window.location.href = redirect.startsWith('/') ? redirect : '/';
    } catch (e) {
      onError(e.status ? e.message : passkeyError(e));
      setBusy(false);
    }
  };
  return (
    <button type="button" className="btn btn-secondary btn-block btn-microsoft" onClick={signIn} disabled={busy}>
      <Fingerprint size={18} />
      {busy ? t('Waiting for the passkey…') : t('Sign in with a passkey')}
    </button>
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
  const [linkSent, setLinkSent] = useState(null);
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

  const sendLink = async () => {
    if (!email.includes('@')) {
      setError(t('Enter your email address first.'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api('auth.emailLink', { email, redirect });
      setLinkSent(email);
    } catch (err) {
      setError(err.message);
    }
    setBusy(false);
  };

  if (config.isFirstUser && config.passwordLogin) {
    return (
      <AuthShell title={t('Welcome to {product}', { product: getProductName() })} subtitle={t('Let\'s get started by creating the first account and workspace.')}>
        {hasSso(config) && <SsoButtons config={config} />}
        {hasSso(config) && <OrDivider />}
        <Link to="/register" className="btn btn-primary btn-block">
          {t('Create account')}
        </Link>
      </AuthShell>
    );
  }

  if (linkSent) {
    return (
      <AuthShell title={t('Check your email')} subtitle={t('If there is an account for {email}, we sent it a sign-in link. It works for 15 minutes.', { email: linkSent })}>
        <button type="button" className="btn btn-secondary btn-block" onClick={() => setLinkSent(null)}>
          {t('Back to login')}
        </button>
      </AuthShell>
    );
  }

  const others = hasSso(config) || config.passkeys;
  const forms = config.passwordLogin || config.emailLogin;
  return (
    <AuthShell title={t('Login to {product}', { product: getProductName() })}>
      <SsoButtons config={config} redirect={redirect} />
      {config.passkeys && <PasskeyButton redirect={redirect} onError={setError} />}
      {others && forms && <OrDivider />}
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
        {config.emailLogin && (
          <button type="button" className="link-button auth-link-button" onClick={sendLink} disabled={busy}>
            {t('Email me a sign-in link instead')}
          </button>
        )}
      </form>
      )}
      {!config.passwordLogin && config.emailLogin && (
        <form
          className="form"
          onSubmit={(e) => {
            e.preventDefault();
            sendLink();
          }}
        >
          <Field label={t('Email')}>
            <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoComplete="email" />
          </Field>
          <Button type="submit" disabled={busy} className="btn-block" icon={<Mail size={16} />}>
            {t('Email me a sign-in link')}
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

/** /login/email?token=… - the page of the sign-in link: one click signs in */
export function EmailSignIn() {
  const [params] = useSearchParams();
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const token = params.get('token') || '';
  useEffect(() => {
    document.title = t('Login - {product}', { product: getProductName() });
  }, []);
  const signIn = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api('auth.emailCallback', { token });
      window.location.href = r.redirect && r.redirect.startsWith('/') ? r.redirect : '/';
    } catch (e) {
      setError(e.message);
      setBusy(false);
    }
  };
  return (
    <AuthShell title={t('Sign in to {product}', { product: getProductName() })} subtitle={t('Use the button to finish signing in.')}>
      {error && <div className="error-text">{error}</div>}
      <Button className="btn-block" onClick={signIn} disabled={busy || !token}>
        {busy ? t('Signing in…') : t('Sign in')}
      </Button>
      <p className="auth-footer">
        <Link to="/login">{t('Back to login')}</Link>
      </p>
    </AuthShell>
  );
}
