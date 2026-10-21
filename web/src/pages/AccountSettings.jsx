import React, { useEffect, useState } from 'react';
import { Copy, Fingerprint, MoreHorizontal, Plus, RefreshCw, Send, Trash2, Webhook } from 'lucide-react';
import { api, copyToClipboard, formatDate, timeAgo } from '../api';
import { useApp } from '../store';
import { Button, ConfirmModal, Empty, Field, Menu, Modal, Spinner, Switch } from '../components/ui';
import { useToast } from '../components/Toast';
import { createPasskey, passkeyError, passkeysSupported } from '../passkeys';
import { t } from '../i18n';

// ---------------------------------------------------------------------------
// Passkeys (in the profile)
// ---------------------------------------------------------------------------

/** A name for a new passkey, from the browser and the system */
function deviceName() {
  const ua = navigator.userAgent;
  const os = [
    [/iPhone|iPad/, 'iOS'],
    [/Android/, 'Android'],
    [/Mac OS X/, 'macOS'],
    [/Windows/, 'Windows'],
    [/Linux/, 'Linux'],
  ].find(([re]) => re.test(ua))?.[1];
  const browser = [
    [/Edg\//, 'Edge'],
    [/Firefox\//, 'Firefox'],
    [/Chrome\//, 'Chrome'],
    [/Safari\//, 'Safari'],
  ].find(([re]) => re.test(ua))?.[1];
  return [browser, os].filter(Boolean).join(' - ') || t('Passkey');
}

export function Passkeys() {
  const [keys, setKeys] = useState(null);
  const [adding, setAdding] = useState(null);
  const [removing, setRemoving] = useState(null);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const load = () => api('passkeys.list').then(setKeys).catch(() => setKeys([]));
  useEffect(() => {
    load();
  }, []);
  const add = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      const { challengeId, options } = await api('passkeys.registerStart');
      const credential = await createPasskey(options);
      await api('passkeys.registerFinish', { challengeId, name: adding, credential });
      toast(t('Passkey added'));
      setAdding(null);
      load();
    } catch (err) {
      const message = err.status ? err.message : passkeyError(err);
      if (message) toast(message, 'error');
    }
    setBusy(false);
  };
  return (
    <>
      <h2 className="section-title">{t('Passkeys')}</h2>
      <p className="muted">{t('Sign in with your fingerprint, face or device PIN instead of typing a password.')}</p>
      {!passkeysSupported() && <p className="warning-text">{t('This browser doesn’t support passkeys.')}</p>}
      {keys === null && <Spinner />}
      {keys?.length > 0 && (
        <div className="passkey-list">
          {keys.map((k) => (
            <div key={k.id} className="passkey-row">
              <Fingerprint size={18} className="muted" />
              <div className="grow">
                <div className="passkey-name">{k.name}</div>
                <div className="muted small">
                  {t('Added {date}', { date: formatDate(k.createdAt) })}
                  {' - '}
                  {k.lastUsedAt ? t('last used {time}', { time: timeAgo(k.lastUsedAt) }) : t('never used')}
                </div>
              </div>
              <Button variant="secondary" small icon={<Trash2 size={14} />} onClick={() => setRemoving(k)}>
                {t('Remove')}
              </Button>
            </div>
          ))}
        </div>
      )}
      {passkeysSupported() && (
        <div>
          <Button variant="secondary" icon={<Plus size={14} />} onClick={() => setAdding(deviceName())}>
            {t('Add a passkey')}
          </Button>
        </div>
      )}
      {adding !== null && (
        <Modal title={t('Add a passkey')} onClose={() => !busy && setAdding(null)}>
          <form className="form" onSubmit={add}>
            <Field label={t('Name')} hint={t('To recognise it later, for example the device it is on.')}>
              <input autoFocus value={adding} onChange={(e) => setAdding(e.target.value)} required maxLength={100} />
            </Field>
            <div className="modal-actions">
              <Button type="button" variant="secondary" onClick={() => setAdding(null)} disabled={busy}>
                {t('Cancel')}
              </Button>
              <Button type="submit" disabled={busy}>
                {busy ? t('Waiting for the passkey…') : t('Continue')}
              </Button>
            </div>
          </form>
        </Modal>
      )}
      {removing && (
        <ConfirmModal
          title={t('Remove passkey')}
          message={t('“{name}” will no longer sign you in. Remove it from the device too, if you don’t need it.', { name: removing.name })}
          confirmLabel={t('Remove')}
          danger
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await api('passkeys.delete', { id: removing.id });
            setRemoving(null);
            load();
          }}
        />
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Notifications (Settings → Notifications)
// ---------------------------------------------------------------------------

// Titles are kept in English here and translated with t() at render time.
const NOTIFICATION_EVENTS = [
  ['mentioned_in_document', 'Mentions', 'Someone mentions you, or a group you belong to, in a document.'],
  ['document_published', 'Documents published', 'A document is published in a collection you follow.'],
  ['document_updated', 'Documents updated', 'A document you follow changes. You follow the documents you publish or edit.'],
  ['document_shared', 'Documents shared with you', 'Someone gives you access to a document.'],
  ['comment_created', 'Comments', 'Someone comments on a document you follow, or replies in a thread you took part in.'],
  ['comment_resolved', 'Resolved threads', 'Someone resolves a thread you took part in.'],
  ['collection_added', 'Access to collections', 'Someone adds you to a collection.'],
  ['invite_accepted', 'Accepted invitations', 'Someone you invited joins the workspace.'],
];

export function NotificationSettings() {
  const { user, auth, refreshAuth } = useApp();
  const toast = useToast();
  const settings = user.notificationSettings || {};
  const save = async (event, value) => {
    try {
      await api('users.update', { notificationSettings: { [event]: value } });
      await refreshAuth();
      toast(t('Preferences saved'));
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  return (
    <>
      <h1 className="page-title">{t('Notifications')}</h1>
      <p className="page-subtitle muted">
        {t('Notifications always appear in the sidebar. Choose which ones also reach you by email.')}
      </p>
      {auth.email === false && <p className="warning-text">{t('Email is not configured on this server: no email is sent for now.')}</p>}
      <div className="form settings-form">
        {NOTIFICATION_EVENTS.map(([event, label, hint]) => (
          <div key={event} className="field row space-between">
            <div>
              <div className="field-label">{t(label)}</div>
              <div className="field-hint">{t(hint)}</div>
            </div>
            <Switch checked={settings[event] !== false} onChange={(v) => save(event, v)} />
          </div>
        ))}
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------
// Webhooks (Settings → Webhooks, admins)
// ---------------------------------------------------------------------------

const WEBHOOK_GROUPS = {
  documents: 'Documents',
  collections: 'Collections',
  comments: 'Comments',
  shares: 'Public links',
  users: 'Members',
  groups: 'Groups',
  invites: 'Invitations',
  api_keys: 'API keys',
  teams: 'Workspace',
  data: 'Import',
};

function WebhookModal({ webhook, groups, onClose, onSaved }) {
  const toast = useToast();
  const [name, setName] = useState(webhook?.name || '');
  const [url, setUrl] = useState(webhook?.url || '');
  const [all, setAll] = useState(!webhook || webhook.events.length === 0);
  const [events, setEvents] = useState(webhook?.events || []);
  const [busy, setBusy] = useState(false);
  const toggle = (g) => setEvents((ev) => (ev.includes(g) ? ev.filter((e) => e !== g) : [...ev, g]));
  const submit = async (e) => {
    e.preventDefault();
    if (!all && events.length === 0) {
      toast(t('Choose at least one kind of event.'), 'error');
      return;
    }
    setBusy(true);
    try {
      const body = { name, url, events: all ? [] : events };
      const saved = await api(webhook ? 'webhooks.update' : 'webhooks.create', webhook ? { id: webhook.id, ...body } : body);
      onSaved(saved);
    } catch (err) {
      toast(err.message, 'error');
      setBusy(false);
    }
  };
  return (
    <Modal title={webhook ? t('Edit webhook') : t('New webhook')} onClose={onClose} width={520}>
      <form className="form" onSubmit={submit}>
        <Field label={t('Name')}>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} required maxLength={100} />
        </Field>
        <Field label={t('URL')} hint={t('GetPaper sends a POST request with a JSON body to this address.')}>
          <input type="url" value={url} onChange={(e) => setUrl(e.target.value)} required placeholder="https://example.com/hooks/getpaper" />
        </Field>
        <Field label={t('Events')}>
          <label className="checkbox-row">
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            {t('All events')}
          </label>
          {!all && (
            <div className="webhook-groups">
              {groups.map((g) => (
                <label key={g} className="checkbox-row">
                  <input type="checkbox" checked={events.includes(g)} onChange={() => toggle(g)} />
                  {t(WEBHOOK_GROUPS[g] || g)}
                  <code className="muted small">{g}.*</code>
                </label>
              ))}
            </div>
          )}
        </Field>
        <div className="modal-actions">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button type="submit" disabled={busy}>
            {webhook ? t('Save') : t('Create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

function Deliveries({ webhook, onClose }) {
  const [rows, setRows] = useState(null);
  useEffect(() => {
    api('webhooks.deliveries', { id: webhook.id }).then(setRows).catch(() => setRows([]));
  }, [webhook.id]);
  return (
    <Modal title={t('Deliveries of {name}', { name: webhook.name })} onClose={onClose} width={640}>
      {rows === null && <Spinner />}
      {rows?.length === 0 && <Empty>{t('Nothing sent in the last 7 days.')}</Empty>}
      {rows?.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t('Event')}</th>
                <th>{t('Result')}</th>
                <th>{t('Attempts')}</th>
                <th>{t('When')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((d) => (
                <tr key={d.id}>
                  <td>
                    <code>{d.event}</code>
                  </td>
                  <td className={d.error ? 'error-text' : 'muted'}>{d.error || `HTTP ${d.status}`}</td>
                  <td className="muted">{d.attempts}</td>
                  <td className="muted nowrap">{timeAgo(d.createdAt)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  );
}

export function Webhooks() {
  const [data, setData] = useState(null);
  const [editing, setEditing] = useState(null);
  const [deliveries, setDeliveries] = useState(null);
  const [removing, setRemoving] = useState(null);
  const toast = useToast();
  const load = () => api('webhooks.list').then(setData).catch((e) => toast(e.message, 'error'));
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const update = async (w, body, message) => {
    try {
      await api('webhooks.update', { id: w.id, ...body });
      if (message) toast(message);
      load();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const test = async (w) => {
    try {
      const r = await api('webhooks.test', { id: w.id });
      if (r.error) toast(t('The test failed: {error}', { error: r.error }), 'error');
      else toast(t('Test sent: HTTP {status}', { status: r.status }));
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return (
    <>
      <h1 className="page-title">{t('Webhooks')}</h1>
      <p className="page-subtitle muted">
        {t('Tell other systems what happens in this workspace: for every event chosen, GetPaper sends an HTTP request signed with the secret of the webhook.')}
      </p>
      <div>
        <Button icon={<Plus size={14} />} onClick={() => setEditing({})}>
          {t('New webhook')}
        </Button>
      </div>
      {data === null && <Spinner />}
      {data?.webhooks.length === 0 && <Empty>{t('No webhooks yet.')}</Empty>}
      {data?.webhooks.length > 0 && (
        <div className="webhook-list">
          {data.webhooks.map((w) => (
            <div key={w.id} className={`webhook-card ${w.enabled ? '' : 'disabled'}`}>
              <div className="webhook-head">
                <Webhook size={18} className="muted" />
                <div className="grow webhook-title">
                  <strong>{w.name}</strong>
                  <span className="muted small webhook-url">{w.url}</span>
                </div>
                <Switch checked={w.enabled} onChange={(v) => update(w, { enabled: v }, v ? t('Webhook turned on') : t('Webhook turned off'))} />
                <Menu
                  align="right"
                  trigger={
                    <button className="icon-btn" title={t('More')}>
                      <MoreHorizontal size={16} />
                    </button>
                  }
                  items={[
                    { label: t('Edit'), onClick: () => setEditing(w) },
                    { label: t('Send a test'), icon: <Send size={14} />, onClick: () => test(w) },
                    { label: t('Deliveries'), onClick: () => setDeliveries(w) },
                    { label: t('New secret'), icon: <RefreshCw size={14} />, onClick: () => update(w, { rotateSecret: true }, t('New secret created')) },
                    { separator: true },
                    { label: t('Delete'), icon: <Trash2 size={14} />, danger: true, onClick: () => setRemoving(w) },
                  ]}
                />
              </div>
              <div className="webhook-meta muted small">
                {w.events.length === 0 ? t('All events') : w.events.map((e) => t(WEBHOOK_GROUPS[e] || e)).join(', ')}
                {' - '}
                {w.lastDeliveryAt
                  ? w.lastError
                    ? t('last call failed {time}: {error}', { time: timeAgo(w.lastDeliveryAt), error: w.lastError })
                    : t('last call {time}', { time: timeAgo(w.lastDeliveryAt) })
                  : t('never called')}
              </div>
              {!w.enabled && w.failures >= 25 && (
                <div className="warning-text small">{t('Turned off after 25 failed deliveries in a row. Fix the receiver, then turn it on again.')}</div>
              )}
              <div className="row gap webhook-secret">
                <span className="muted small">{t('Signing secret')}</span>
                <code className="grow">{w.secret}</code>
                <Button
                  variant="secondary"
                  small
                  icon={<Copy size={14} />}
                  onClick={async () => {
                    await copyToClipboard(w.secret);
                    toast(t('Secret copied'));
                  }}
                >
                  {t('Copy')}
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
      <h2 className="section-title">{t('How calls are signed')}</h2>
      <p className="muted">
        {t('Every request has the header X-GetPaper-Signature: t=<time>,s=<signature>, where the signature is the HMAC-SHA256, in hexadecimal, of "<time>.<body>" with the secret. X-GetPaper-Event names the event.')}
      </p>
      {editing && (
        <WebhookModal
          webhook={editing.id ? editing : null}
          groups={data?.groups || Object.keys(WEBHOOK_GROUPS)}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            toast(t('Webhook saved'));
            load();
          }}
        />
      )}
      {deliveries && <Deliveries webhook={deliveries} onClose={() => setDeliveries(null)} />}
      {removing && (
        <ConfirmModal
          title={t('Delete webhook')}
          message={t('“{name}” will no longer receive events.', { name: removing.name })}
          confirmLabel={t('Delete')}
          danger
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await api('webhooks.delete', { id: removing.id });
            setRemoving(null);
            load();
          }}
        />
      )}
    </>
  );
}
