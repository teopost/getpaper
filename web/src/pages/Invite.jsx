import React, { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { api } from '../api';
import { Button, Spinner } from '../components/ui';
import { t } from '../i18n';
import { AuthShell, OrDivider, SsoButtons, hasSso } from './Login';
import { RegisterForm } from './Register';

export default function Invite() {
  const { token } = useParams();
  const [invite, setInvite] = useState(null);
  const [me, setMe] = useState(undefined);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const [config, setConfig] = useState(null);

  useEffect(() => {
    api('auth.config').then(setConfig).catch(() => setConfig({ passwordLogin: true }));
    api('invites.info', { token }).then(setInvite).catch((e) => setError(e.message));
    api('auth.info')
      .then(setMe)
      .catch(() => setMe(null));
  }, [token]);

  if (error) {
    return (
      <AuthShell title={t('Invite not valid')} subtitle={error}>
        <Link to="/" className="btn btn-secondary btn-block">
          {t('Go home')}
        </Link>
      </AuthShell>
    );
  }
  if (!invite || me === undefined || !config) {
    return (
      <div className="centered-page">
        <Spinner />
      </div>
    );
  }

  const accept = async () => {
    setBusy(true);
    try {
      await api('invites.accept', { token });
      window.location.href = '/';
    } catch (e) {
      setError(e.message);
    }
  };

  return (
    <AuthShell
      title={t('Join {name}', { name: invite.teamName })}
      subtitle={t('{inviter} invited you to collaborate in the {name} workspace.', { inviter: invite.inviterName, name: invite.teamName })}
    >
      {me ? (
        <>
          <p className="auth-subtitle">
            {t('Signed in as')} <strong>{me.user.email}</strong>
          </p>
          <Button className="btn-block" onClick={accept} disabled={busy}>
            {busy ? t('Joining…') : t('Join {name}', { name: invite.teamName })}
          </Button>
        </>
      ) : (
        <>
          <SsoButtons config={config} invite={token} verb="join" />
          {hasSso(config) && config.passwordLogin && <OrDivider />}
          {config.passwordLogin && (
            <>
              <RegisterForm inviteToken={token} defaultEmail={invite.email} teamName={invite.teamName} />
              <p className="auth-footer">
                {t('Already have an account?')}{' '}
                <Link to={`/login?redirect=${encodeURIComponent(`/invite/${token}`)}`}>{t('Log in')}</Link>
              </p>
            </>
          )}
        </>
      )}
    </AuthShell>
  );
}
