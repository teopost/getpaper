import React, { useState } from 'react';
import { Copy } from 'lucide-react';
import { api, copyToClipboard } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import { Button, Field, Modal } from './ui';
import { t } from '../i18n';

export default function InviteModal({ onClose, onInvited }) {
  const { team, isAdmin, auth } = useApp();
  const emailEnabled = !!auth.email;
  const [emails, setEmails] = useState('');
  const [role, setRole] = useState('member');
  const [sendEmail, setSendEmail] = useState(true);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState(null);
  const toast = useToast();

  const submit = async (e) => {
    e.preventDefault();
    const list = emails
      .split(/[\s,;]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (!list.length) return;
    setBusy(true);
    const out = [];
    for (const email of list) {
      try {
        const r = await api('invites.create', { email, role, sendEmail: emailEnabled && sendEmail });
        out.push({ email, ...r });
      } catch (err) {
        out.push({ email, error: err.message });
      }
    }
    setResults(out);
    setBusy(false);
    onInvited?.();
  };

  return (
    <Modal title={t('Invite people to {name}', { name: team.name })} onClose={onClose} width={560}>
      {!results ? (
        <form onSubmit={submit} className="form">
          <p className="modal-text">
            {emailEnabled
              ? t('Invite members to join your workspace. Existing users are added immediately; new users receive an invitation email with a personal link.')
              : t(
                  'Invite members to join your workspace. Existing users are added immediately; new users receive a personal invite link that you can share with them.'
                )}
          </p>
          <Field label={t('Email addresses')} hint={t('Separate multiple addresses with commas or new lines.')}>
            <textarea
              autoFocus
              rows={3}
              value={emails}
              onChange={(e) => setEmails(e.target.value)}
              placeholder="name@example.com"
            />
          </Field>
          <Field label={t('Role')}>
            <select value={role} onChange={(e) => setRole(e.target.value)}>
              {isAdmin && <option value="admin">{t('Admin')}</option>}
              <option value="member">{t('Editor')}</option>
              <option value="viewer">{t('Viewer')}</option>
            </select>
          </Field>
          {emailEnabled && (
            <label className="row gap-sm checkbox-row">
              <input type="checkbox" checked={sendEmail} onChange={(e) => setSendEmail(e.target.checked)} />
              {t('Send the invitation by email')}
            </label>
          )}
          <div className="modal-actions">
            <Button variant="secondary" type="button" onClick={onClose}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={busy || !emails.trim()}>
              {busy ? t('Sending…') : t('Send invites')}
            </Button>
          </div>
        </form>
      ) : (
        <div className="form">
          {results.map((r) => (
            <div key={r.email} className="invite-result">
              <div className="invite-email">{r.email}</div>
              {r.error ? (
                <div className="error-text">{r.error}</div>
              ) : r.added ? (
                <div className="muted small">
                  {r.user
                    ? t('{name} already has an account: added to the workspace right away, no invite needed.', { name: r.user.name })
                    : t('Added to the workspace.')}{' '}
                  {r.emailSent && t('An email was sent to let them know.')}
                  {r.emailError && <span className="error-text"> {t('Email not sent: {error}', { error: r.emailError })}</span>}
                </div>
              ) : r.alreadyMember ? (
                <div className="muted small">
                  {r.user ? t('{name} is already a member of this workspace.', { name: r.user.name }) : t('Already a member.')}
                </div>
              ) : (
                <>
                {r.emailSent && <div className="success-text small">{t('Invitation email sent.')}</div>}
                {r.emailError && <div className="error-text small">{t('Email not sent: {error}', { error: r.emailError })}</div>}
                <div className="row gap">
                  <input readOnly value={r.link} onFocus={(e) => e.target.select()} className="grow" />
                  <Button
                    variant="secondary"
                    icon={<Copy size={14} />}
                    onClick={async () => {
                      await copyToClipboard(r.link);
                      toast(t('Invite link copied'));
                    }}
                  >
                    {t('Copy')}
                  </Button>
                </div>
                </>
              )}
            </div>
          ))}
          <div className="modal-actions">
            <Button onClick={onClose}>{t('Done')}</Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
