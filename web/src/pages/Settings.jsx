import React, { useEffect, useState } from 'react';
import { Link, Navigate, useParams } from 'react-router-dom';
import { Copy, Plus, Trash2, UserPlus, Check, Download, Mail, Send, ImageUp } from 'lucide-react';
import { api, collectionUrl, copyToClipboard, docUrl, formatDate, downloadFile, timeAgo, upload, uploadFile } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import { Avatar, Button, ConfirmModal, Empty, Field, Modal, Spinner, Switch, TeamAvatar } from '../components/ui';
import { useToast } from '../components/Toast';
import { COLORS } from '../components/icons';
import InviteModal from '../components/InviteModal';
import Groups from './Groups';
import NewWorkspaceModal from '../components/NewWorkspaceModal';
import { LANGUAGES, t } from '../i18n';
import { DEFAULT_PRODUCT_NAME, getDefaults, getProductName } from '../prefs';

function ColorPicker({ value, onChange }) {
  return (
    <div className="color-picker">
      {COLORS.map((c) => (
        <button
          type="button"
          key={c}
          className={`color-choice ${value === c ? 'selected' : ''}`}
          style={{ background: c }}
          onClick={() => onChange(c)}
        />
      ))}
    </div>
  );
}

const THEMES = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];
const CONTENT_WIDTHS = [
  { value: 'normal', label: 'Normal' },
  { value: 'wide', label: 'Wide' },
  { value: 'full', label: 'Full width' },
];

/** Select for a personal preference; the empty value means "use the installation default". */
function PreferenceSelect({ value, options, fallback, onChange }) {
  const def = options.find((o) => o.value === fallback);
  return (
    <select value={value || ''} onChange={(e) => onChange(e.target.value)}>
      <option value="">{t('Default ({value})', { value: def ? t(def.label) : fallback })}</option>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {t(o.label)}
        </option>
      ))}
    </select>
  );
}

/** Language, appearance and document width. Saved on change and applied immediately. */
function Preferences() {
  const { user, refreshAuth } = useApp();
  const toast = useToast();
  const defaults = getDefaults();
  const save = async (field, value) => {
    try {
      await api('users.update', { [field]: value });
      await refreshAuth();
      toast(t('Preferences saved'));
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  return (
    <>
      <h2 className="section-title">{t('Preferences')}</h2>
      <div className="form settings-form">
        <Field label={t('Language')}>
          <PreferenceSelect value={user.language} options={LANGUAGES} fallback={defaults.language} onChange={(v) => save('language', v)} />
        </Field>
        <Field label={t('Appearance')}>
          <PreferenceSelect value={user.theme} options={THEMES} fallback={defaults.theme} onChange={(v) => save('theme', v)} />
        </Field>
        <Field
          label={t('Document width')}
          hint={t('Width of the text column. Documents set to full width always use the whole page.')}
        >
          <PreferenceSelect
            value={user.contentWidth}
            options={CONTENT_WIDTHS}
            fallback={defaults.contentWidth}
            onChange={(v) => save('contentWidth', v)}
          />
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">{t('Email me when someone mentions me')}</div>
            <div className="field-hint">{t('Mentions always appear in the notifications in the sidebar.')}</div>
          </div>
          <Switch checked={user.emailOnMention} onChange={(v) => save('emailOnMention', v)} />
        </div>
      </div>
    </>
  );
}

/** Renders a translated string, replacing {placeholders} with React nodes (e.g. <strong>, <code>). */
function rich(text, nodes) {
  return text.split(/(\{\w+\})/).map((part, i) => {
    const m = part.match(/^\{(\w+)\}$/);
    return <React.Fragment key={i}>{m && m[1] in nodes ? nodes[m[1]] : part}</React.Fragment>;
  });
}

function Profile() {
  const { user, auth, refreshAuth } = useApp();
  const hasPassword = auth.hasPassword;
  const toast = useToast();
  const [name, setName] = useState(user.name);
  const [color, setColor] = useState(user.color);
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  return (
    <>
      <h1 className="page-title">{t('Profile')}</h1>
      <p className="page-subtitle muted">{t('Manage how you appear to other members of the workspace.')}</p>
      <form
        className="form settings-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api('users.update', { name, color });
            await refreshAuth();
            toast(t('Profile saved'));
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        <div className="row gap">
          <Avatar name={name} color={color} size={56} />
          <ColorPicker value={color} onChange={setColor} />
        </div>
        <Field label={t('Name')}>
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label={t('Email')}>
          <input value={user.email} disabled />
        </Field>
        <div>
          <Button type="submit">{t('Save')}</Button>
        </div>
      </form>

      <Preferences />

      {auth.passwordLogin !== false && (
        <>
          <h2 className="section-title">{t('Password')}</h2>
          {!hasPassword && (
            <p className="muted">{t('You sign in with Microsoft. You can optionally set a password as well.')}</p>
          )}
          <form
            className="form settings-form"
            onSubmit={async (e) => {
              e.preventDefault();
              try {
                await api('users.changePassword', { currentPassword: current, newPassword: next });
                setCurrent('');
                setNext('');
                await refreshAuth();
                toast(hasPassword ? t('Password changed') : t('Password set'));
              } catch (err) {
                toast(err.message, 'error');
              }
            }}
          >
            {hasPassword && (
              <Field label={t('Current password')}>
                <input type="password" value={current} onChange={(e) => setCurrent(e.target.value)} required autoComplete="current-password" />
              </Field>
            )}
            <Field label={t('New password')} hint={t('At least 8 characters.')}>
              <input type="password" value={next} onChange={(e) => setNext(e.target.value)} required minLength={8} autoComplete="new-password" />
            </Field>
            <div>
              <Button type="submit" variant="secondary">
                {hasPassword ? t('Change password') : t('Set password')}
              </Button>
            </div>
          </form>
        </>
      )}
    </>
  );
}

const LOGO_SIZE = 256;

/** The image as a square PNG logo: the whole image fitted in the square, transparent around it */
async function squareLogo(file) {
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    await new Promise((resolve, reject) => {
      img.onload = resolve;
      img.onerror = () => reject(new Error(t('This file is not an image')));
      img.src = url;
    });
    const w = img.naturalWidth || LOGO_SIZE;
    const h = img.naturalHeight || LOGO_SIZE;
    const scale = Math.min(LOGO_SIZE / w, LOGO_SIZE / h);
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = LOGO_SIZE;
    const ctx = canvas.getContext('2d');
    ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(img, (LOGO_SIZE - w * scale) / 2, (LOGO_SIZE - h * scale) / 2, w * scale, h * scale);
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
    return new File([blob], 'workspace-logo.png', { type: 'image/png' });
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** Logo of the workspace, as in Outline: uploaded and saved at once, shown in place of the initial */
function WorkspaceLogo({ team, name, color, onChange }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const inputRef = React.useRef(null);
  const save = async (avatarUrl, message) => {
    setBusy(true);
    try {
      await api('teams.update', { avatarUrl });
      await onChange();
      toast(message);
    } catch (err) {
      toast(err.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="row gap">
      <TeamAvatar team={{ name, color, avatarUrl: team.avatarUrl }} size={56} />
      <div className="workspace-logo-actions">
        <div className="row gap-sm">
          <Button type="button" variant="secondary" small icon={<ImageUp size={14} />} disabled={busy} onClick={() => inputRef.current?.click()}>
            {team.avatarUrl ? t('Change logo…') : t('Upload logo…')}
          </Button>
          {team.avatarUrl && (
            <Button type="button" variant="secondary" small disabled={busy} onClick={() => save('', t('Logo removed'))}>
              {t('Remove')}
            </Button>
          )}
        </div>
        <div className="field-hint">{t('Square images work best; others are fitted in a square. Without a logo the initial on the colour below is shown.')}</div>
        <input
          ref={inputRef}
          type="file"
          accept="image/*"
          hidden
          onChange={async (e) => {
            const file = e.target.files?.[0];
            e.target.value = '';
            if (!file) return;
            try {
              const res = await uploadFile(await squareLogo(file));
              await save(res.url, t('Logo updated'));
            } catch (err) {
              toast(err.message, 'error');
            }
          }}
        />
      </div>
    </div>
  );
}

function Workspace() {
  const { team, isAdmin, refreshAuth } = useApp();
  const toast = useToast();
  const [name, setName] = useState(team.name);
  const [color, setColor] = useState(team.color);
  const [sharing, setSharing] = useState(team.sharing);
  const [defaultRole, setDefaultRole] = useState(team.defaultUserRole);
  const [allowedDomains, setAllowedDomains] = useState(team.allowedDomains || '');
  const [deleting, setDeleting] = useState(false);
  const [confirmName, setConfirmName] = useState('');
  if (!isAdmin) return <Navigate to="/settings/members" replace />;
  return (
    <>
      <h1 className="page-title">{t('Details')}</h1>
      <p className="page-subtitle muted">{t('These settings affect the way that your workspace appears to everyone on the team.')}</p>
      <form
        className="form settings-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api('teams.update', { name, color, sharing, defaultUserRole: defaultRole, allowedDomains });
            await refreshAuth();
            toast(t('Settings saved'));
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        <WorkspaceLogo team={team} name={name} color={color} onChange={refreshAuth} />
        {!team.avatarUrl && <ColorPicker value={color} onChange={setColor} />}
        <Field label={t('Name')}>
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">{t('Public document sharing')}</div>
            <div className="field-hint">{t('When enabled, documents can be shared publicly on the internet by any member.')}</div>
          </div>
          <Switch checked={sharing} onChange={setSharing} />
        </div>
        <Field
          label={t('Allowed domains')}
          hint={t(
            'People signing in with Microsoft whose email belongs to one of these domains join this workspace automatically, with the default role. Separate multiple domains with commas.'
          )}
        >
          <input value={allowedDomains} onChange={(e) => setAllowedDomains(e.target.value)} placeholder={t('e.g. {example}', { example: 'example.com, example.org' })} />
        </Field>
        <Field label={t('Default role')} hint={t('The role new members get when joining automatically.')}>
          <select value={defaultRole} onChange={(e) => setDefaultRole(e.target.value)}>
            <option value="member">{t('Editor')}</option>
            <option value="viewer">{t('Viewer')}</option>
            <option value="admin">{t('Admin')}</option>
          </select>
        </Field>
        <div>
          <Button type="submit">{t('Save')}</Button>
        </div>
      </form>

      <h2 className="section-title danger-text">{t('Danger zone')}</h2>
      <p className="muted">
        {t(
          'Deleting the workspace permanently removes all of its collections, documents and memberships. Your account and other workspaces are not affected.'
        )}
      </p>
      <Button variant="danger" onClick={() => setDeleting(true)}>
        {t('Delete workspace…')}
      </Button>
      {deleting && (
        <Modal title={t('Delete workspace')} onClose={() => setDeleting(false)}>
          <form
            className="form"
            onSubmit={async (e) => {
              e.preventDefault();
              try {
                await api('teams.delete', { id: team.id, name: confirmName });
                window.location.href = '/';
              } catch (err) {
                toast(err.message, 'error');
              }
            }}
          >
            <p className="modal-text">
              {rich(t('Type {name} to confirm. This action cannot be undone.'), { name: <strong>{team.name}</strong> })}
            </p>
            <input autoFocus value={confirmName} onChange={(e) => setConfirmName(e.target.value)} />
            <div className="modal-actions">
              <Button variant="secondary" type="button" onClick={() => setDeleting(false)}>
                {t('Cancel')}
              </Button>
              <Button variant="danger" type="submit" disabled={confirmName.trim() !== team.name.trim()}>
                {t('Delete workspace')}
              </Button>
            </div>
          </form>
        </Modal>
      )}
    </>
  );
}

// English labels; translated with t() where rendered.
const ROLE_LABEL = { admin: 'Admin', member: 'Editor', viewer: 'Viewer' };

function Members() {
  const { user, isAdmin, canWrite, auth } = useApp();
  const toast = useToast();
  const [members, setMembers] = useState(null);
  const [invites, setInvites] = useState([]);
  const [sendingAll, setSendingAll] = useState(false);
  const [confirmSendAll, setConfirmSendAll] = useState(false);
  const sendInvite = async (i) => {
    try {
      await api('invites.send', { id: i.id });
      toast(t('Invitation email sent to {email}', { email: i.email }));
      api('invites.list').then(setInvites);
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  const sendAll = async () => {
    setConfirmSendAll(false);
    setSendingAll(true);
    let sent = 0;
    const failed = [];
    for (const i of invites) {
      try {
        await api('invites.send', { id: i.id });
        sent++;
      } catch (err) {
        failed.push(`${i.email}: ${err.message}`);
      }
    }
    setSendingAll(false);
    api('invites.list').then(setInvites);
    if (failed.length) toast(t('{sent} emails sent, {failed} failed: {errors}', { sent, failed: failed.length, errors: failed.join('; ') }), 'error');
    else toast(t('{sent} emails sent', { sent }));
  };
  const [query, setQuery] = useState('');
  const [inviting, setInviting] = useState(false);
  const [removing, setRemoving] = useState(null);

  const load = () => {
    api('users.list', { query })
      .then(setMembers)
      .catch((err) => toast(err.message, 'error'));
    if (canWrite)
      api('invites.list')
        .then((list) => setInvites(Array.isArray(list) ? list : []))
        .catch((err) => toast(err.message, 'error'));
  };
  useEffect(() => {
    const timer = setTimeout(load, 200);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  return (
    <>
      <div className="row space-between">
        <h1 className="page-title">{t('Members')}</h1>
        {canWrite && (
          <Button icon={<UserPlus size={16} />} onClick={() => setInviting(true)}>
            {t('Invite people…')}
          </Button>
        )}
      </div>
      <p className="page-subtitle muted">
        {t(
          'Everyone that has signed into this workspace. Admins can manage roles; editors can create and edit documents; viewers have read-only access.'
        )}
      </p>
      <input placeholder={t('Filter…')} value={query} onChange={(e) => setQuery(e.target.value)} className="filter-input" />
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              <th>{t('Name')}</th>
              <th>{t('Email')}</th>
              <th>{t('Last active')}</th>
              <th>{t('Role')}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {members?.map((m) => (
              <tr key={m.id}>
                <td>
                  <span className="row gap-sm">
                    <Avatar name={m.name} color={m.color} size={28} />
                    {m.name} {m.id === user.id && <span className="muted">{t('(you)')}</span>}
                    {m.isSuperadmin && (
                      <span className="badge badge-accent" title={t('Manages the general settings of the installation')}>
                        {t('Superadmin')}
                      </span>
                    )}
                  </span>
                </td>
                <td className="muted">{m.email}</td>
                <td className="muted">{m.lastActiveAt ? timeAgo(m.lastActiveAt) : t('Never')}</td>
                <td>
                  {isAdmin ? (
                    <select
                      value={m.role}
                      onChange={async (e) => {
                        try {
                          await api('users.updateRole', { id: m.id, role: e.target.value });
                          toast(t('Role updated'));
                          load();
                        } catch (err) {
                          toast(err.message, 'error');
                        }
                      }}
                    >
                      <option value="admin">{t('Admin')}</option>
                      <option value="member">{t('Editor')}</option>
                      <option value="viewer">{t('Viewer')}</option>
                    </select>
                  ) : (
                    <span className="badge">{t(ROLE_LABEL[m.role])}</span>
                  )}
                </td>
                <td className="right">
                  {(isAdmin || m.id === user.id) && (
                    <Button variant="secondary" small onClick={() => setRemoving(m)}>
                      {m.id === user.id ? t('Leave') : t('Remove')}
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {invites.length > 0 && (
        <>
          <div className="row space-between">
            <h2 className="section-title">{t('Pending invites')}</h2>
            {auth.email && (
              <Button variant="secondary" small icon={<Mail size={14} />} disabled={sendingAll} onClick={() => setConfirmSendAll(true)}>
                {sendingAll ? t('Sending…') : t('Email all pending invites')}
              </Button>
            )}
          </div>
          <div className="table-wrap">
            <table className="table">
              <tbody>
                {invites.map((i) => (
                  <tr key={i.id}>
                    <td>{i.email}</td>
                    <td className="muted">{t(ROLE_LABEL[i.role])}</td>
                    <td className="muted">
                      {t('Invited {time}', { time: timeAgo(i.createdAt) })}
                      <div className="small">
                        {i.emailSentAt ? t('Email sent {time}', { time: timeAgo(i.emailSentAt) }) : t('Email not sent yet')}
                      </div>
                    </td>
                    <td className="right">
                      <span className="row gap-sm justify-end">
                        {auth.email && (
                          <Button variant="secondary" small icon={<Mail size={14} />} onClick={() => sendInvite(i)}>
                            {i.emailSentAt ? t('Send again') : t('Send email')}
                          </Button>
                        )}
                        <Button
                          variant="secondary"
                          small
                          icon={<Copy size={14} />}
                          onClick={async () => {
                            const { link } = await api('invites.link', { id: i.id });
                            await copyToClipboard(link);
                            toast(t('Invite link copied'));
                          }}
                        >
                          {t('Copy link')}
                        </Button>
                        <Button
                          variant="secondary"
                          small
                          onClick={async () => {
                            await api('invites.revoke', { id: i.id });
                            load();
                          }}
                        >
                          {t('Revoke')}
                        </Button>
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      {inviting && <InviteModal onClose={() => setInviting(false)} onInvited={load} />}
      {confirmSendAll && (
        <ConfirmModal
          title={t('Email all pending invites')}
          confirmLabel={t('Send {count} emails', { count: invites.length })}
          message={t('Each of the {count} people with a pending invite receives the invitation email, also those who already got it.', {
            count: invites.length,
          })}
          onConfirm={sendAll}
          onClose={() => setConfirmSendAll(false)}
        />
      )}
      {removing && (
        <ConfirmModal
          title={removing.id === user.id ? t('Leave workspace') : t('Remove member')}
          danger
          confirmLabel={removing.id === user.id ? t('Leave') : t('Remove')}
          message={
            removing.id === user.id
              ? t('You will lose access to this workspace and its documents.')
              : t('{name} will lose access to this workspace. Their documents are kept.', { name: removing.name })
          }
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            try {
              await api('users.remove', { id: removing.id });
              if (removing.id === user.id) window.location.href = '/';
              else load();
            } catch (err) {
              toast(err.message, 'error');
            }
          }}
        />
      )}
    </>
  );
}

function Shares() {
  const [shares, setShares] = useState(null);
  const toast = useToast();
  const load = () => api('shares.list').then(setShares);
  useEffect(() => {
    load();
  }, []);
  return (
    <>
      <h1 className="page-title">{t('Shared Links')}</h1>
      <p className="page-subtitle muted">
        {t('Documents that have been shared publicly are listed below. Anyone with the link can access them.')}
      </p>
      {shares && shares.length === 0 && <Empty>{t('No documents have been shared yet.')}</Empty>}
      {shares && shares.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t('Document')}</th>
                <th>{t('Shared by')}</th>
                <th>{t('Views')}</th>
                <th>{t('Last accessed')}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {shares.map((s) => (
                <tr key={s.id}>
                  <td>
                    <Link to={docUrl({ title: s.documentTitle, urlId: s.documentUrlId })}>{s.documentTitle || t('Untitled')}</Link>
                  </td>
                  <td className="muted">{s.createdByName}</td>
                  <td className="muted">{s.views}</td>
                  <td className="muted">{s.lastAccessedAt ? timeAgo(s.lastAccessedAt) : t('Never')}</td>
                  <td className="right">
                    <span className="row gap-sm justify-end">
                      <Button
                        variant="secondary"
                        small
                        icon={<Copy size={14} />}
                        onClick={async () => {
                          await copyToClipboard(s.url);
                          toast(t('Link copied'));
                        }}
                      >
                        {t('Copy')}
                      </Button>
                      <Button
                        variant="secondary"
                        small
                        onClick={async () => {
                          await api('shares.revoke', { id: s.id });
                          toast(t('Share revoked'));
                          load();
                        }}
                      >
                        {t('Revoke')}
                      </Button>
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

const EXPIRY_OPTIONS = [
  { value: '', label: 'No expiry' },
  { value: '30', label: '30 days' },
  { value: '90', label: '90 days' },
  { value: '180', label: '180 days' },
  { value: '365', label: '1 year' },
];

/** Form to create an API key with its limits: access, workspaces, collections, expiry */
function NewApiKeyModal({ onClose, onCreated }) {
  const { team } = useApp();
  const toast = useToast();
  const [workspaces, setWorkspaces] = useState(null);
  const [name, setName] = useState('');
  const [access, setAccess] = useState('read');
  const [scope, setScope] = useState('current'); // current | all | some
  const [chosen, setChosen] = useState([team.id]);
  const [limitCollections, setLimitCollections] = useState(false);
  const [collections, setCollections] = useState([]);
  const [expiry, setExpiry] = useState('90');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api('apiKeys.collections', {})
      .then(setWorkspaces)
      .catch((e) => toast(e.message, 'error'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectedTeams = scope === 'all' ? (workspaces || []).map((w) => w.id) : scope === 'current' ? [team.id] : chosen;
  const toggle = (list, setList, id) => setList(list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      const visible = new Set((workspaces || []).filter((w) => selectedTeams.includes(w.id)).flatMap((w) => w.collections.map((c) => c.id)));
      const created = await api('apiKeys.create', {
        name,
        access,
        allWorkspaces: scope === 'all',
        workspaceIds: scope === 'all' ? undefined : selectedTeams,
        collectionIds: limitCollections ? collections.filter((c) => visible.has(c)) : undefined,
        expiresInDays: expiry ? Number(expiry) : undefined,
      });
      onCreated(created);
    } catch (err) {
      toast(err.message, 'error');
      setBusy(false);
    }
  };

  return (
    <Modal title={t('New API key')} onClose={onClose} width={600}>
      <form className="form" onSubmit={submit}>
        <Field label={t('Name')}>
          <input autoFocus required value={name} placeholder={t('Key name, e.g. Claude Code')} onChange={(e) => setName(e.target.value)} />
        </Field>

        <Field group label={t('Access')} hint={t('A key never has more permissions than your account, and can’t administer workspaces.')}>
          <div className="radio-group">
            <label>
              <input type="radio" checked={access === 'read'} onChange={() => setAccess('read')} />
              <span>
                {t('Read only')}
                <span className="option-hint">{t('search and read documents')}</span>
              </span>
            </label>
            <label>
              <input type="radio" checked={access === 'write'} onChange={() => setAccess('write')} />
              <span>
                {t('Read and write')}
                <span className="option-hint">{t('also create, edit and comment, as your role allows')}</span>
              </span>
            </label>
          </div>
        </Field>

        <Field group label={t('Workspaces')}>
          <div className="radio-group">
            <label>
              <input type="radio" checked={scope === 'current'} onChange={() => setScope('current')} /> {t('Only {name}', { name: team.name })}
            </label>
            <label>
              <input type="radio" checked={scope === 'all'} onChange={() => setScope('all')} /> {t('All my workspaces, also future ones')}
            </label>
            <label>
              <input type="radio" checked={scope === 'some'} onChange={() => setScope('some')} /> {t('Choose workspaces')}
            </label>
            {scope === 'some' && (
              <div className="checkbox-list">
                {(workspaces || []).map((w) => (
                  <label key={w.id}>
                    <input type="checkbox" checked={chosen.includes(w.id)} onChange={() => toggle(chosen, setChosen, w.id)} /> {w.name}
                  </label>
                ))}
              </div>
            )}
          </div>
        </Field>

        <Field group label={t('Collections')}>
          <div className="radio-group">
            <label>
              <input type="radio" checked={!limitCollections} onChange={() => setLimitCollections(false)} /> {t('All collections I can access')}
            </label>
            <label>
              <input type="radio" checked={limitCollections} onChange={() => setLimitCollections(true)} /> {t('Only some collections')}
            </label>
            {limitCollections && (
              <div className="checkbox-list">
                {(workspaces || [])
                  .filter((w) => selectedTeams.includes(w.id))
                  .map((w) => (
                    <div key={w.id}>
                      {selectedTeams.length > 1 && <div className="muted small checkbox-group-title">{w.name}</div>}
                      {w.collections.map((c) => (
                        <label key={c.id}>
                          <input type="checkbox" checked={collections.includes(c.id)} onChange={() => toggle(collections, setCollections, c.id)} />{' '}
                          {c.name}
                        </label>
                      ))}
                      {w.collections.length === 0 && <div className="muted small">{t('No collections')}</div>}
                    </div>
                  ))}
              </div>
            )}
          </div>
        </Field>

        <Field label={t('Expiry')}>
          <select value={expiry} onChange={(e) => setExpiry(e.target.value)}>
            {EXPIRY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {t(o.label)}
              </option>
            ))}
          </select>
        </Field>

        <div className="modal-actions">
          <Button variant="secondary" type="button" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button
            type="submit"
            disabled={busy || !name.trim() || (scope === 'some' && chosen.length === 0) || (limitCollections && collections.length === 0)}
          >
            {t('Create key')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

function keyScope(k) {
  const workspaces = k.allWorkspaces ? t('All workspaces') : k.workspaces.map((w) => w.name).join(', ');
  const collections = k.collections
    ? t(k.collections.length === 1 ? '{count} collection' : '{count} collections', { count: k.collections.length })
    : t('All collections');
  return { workspaces, collections, collectionNames: k.collections?.map((c) => c.name).join(', ') };
}

function ApiKeys() {
  const [keys, setKeys] = useState(null);
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState(null);
  const toast = useToast();
  const load = () => api('apiKeys.list').then(setKeys).catch((e) => toast(e.message, 'error'));
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <>
      <div className="row space-between">
        <h1 className="page-title">{t('API Keys')}</h1>
        <Button icon={<Plus size={16} />} onClick={() => setCreating(true)}>
          {t('New API key')}
        </Button>
      </div>
      <p className="page-subtitle muted">
        {t(
          'Personal keys for the API and the MCP server. Each key can be read-only, valid in one or more workspaces, limited to some collections and set to expire. A key never has more permissions than your account.'
        )}
      </p>
      {created && (
        <div className="notice-bar info column">
          <div>
            {rich(t('Copy your new key {name} now, you won’t be able to see it again:'), { name: <strong>{created.name}</strong> })}
          </div>
          <div className="row gap">
            <input readOnly value={created.secret} className="grow mono" onFocus={(e) => e.target.select()} />
            <Button
              variant="secondary"
              icon={<Copy size={14} />}
              onClick={async () => {
                await copyToClipboard(created.secret);
                toast(t('API key copied'));
              }}
            >
              {t('Copy')}
            </Button>
          </div>
          <pre className="code-sample">{`curl -X POST ${window.location.origin}/api/documents.list \\
  -H "Authorization: Bearer ${created.secret}" \\
  -H "Content-Type: application/json" -d '{}'`}</pre>
        </div>
      )}
      <div className="table-wrap">
        <table className="table">
          <tbody>
            {keys?.map((k) => {
              const scope = keyScope(k);
              return (
                <tr key={k.id}>
                  <td>
                    <strong>{k.name}</strong> <span className="muted mono">…{k.last4}</span>
                    <div className="small key-badges">
                      <span className="badge">{k.access === 'read' ? t('Read only') : t('Read and write')}</span>
                      {k.expired ? (
                        <span className="badge badge-danger">{t('Expired')}</span>
                      ) : k.expiresAt ? (
                        <span className="muted">{t('Expires {date}', { date: formatDate(k.expiresAt) })}</span>
                      ) : (
                        <span className="muted">{t('No expiry')}</span>
                      )}
                    </div>
                  </td>
                  <td className="muted small">
                    <div>{scope.workspaces}</div>
                    <div title={scope.collectionNames || ''}>{scope.collections}</div>
                  </td>
                  <td className="muted small">
                    <div>{t('Created {time}', { time: timeAgo(k.createdAt) })}</div>
                    <div>{k.lastUsedAt ? t('Used {time}', { time: timeAgo(k.lastUsedAt) }) : t('Never used')}</div>
                  </td>
                  <td className="right">
                    <Button
                      variant="secondary"
                      small
                      icon={<Trash2 size={14} />}
                      onClick={async () => {
                        await api('apiKeys.delete', { id: k.id });
                        load();
                      }}
                    >
                      {t('Revoke')}
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {keys && keys.length === 0 && <Empty>{t('You haven’t created any API keys yet.')}</Empty>}
      {creating && (
        <NewApiKeyModal
          onClose={() => setCreating(false)}
          onCreated={(c) => {
            setCreating(false);
            setCreated(c);
            load();
          }}
        />
      )}
      <McpSection />
    </>
  );
}

function Workspaces() {
  const { teams, team, switchTeam } = useApp();
  const [creating, setCreating] = useState(false);
  return (
    <>
      <div className="row space-between">
        <h1 className="page-title">{t('Workspaces')}</h1>
        <Button icon={<Plus size={16} />} onClick={() => setCreating(true)}>
          {t('New workspace')}
        </Button>
      </div>
      <p className="page-subtitle muted">
        {t(
          'Each workspace is a completely separate knowledge base with its own members, collections, permissions and settings. Use the workspace switcher at the top of the sidebar to move between them.'
        )}
      </p>
      <div className="workspace-grid">
        {teams.map((w) => (
          <div key={w.id} className={`workspace-card ${w.id === team.id ? 'current' : ''}`}>
            <TeamAvatar team={w} size={40} />
            <div className="grow">
              <div className="workspace-name">{w.name}</div>
              <div className="muted small">
                {t(ROLE_LABEL[w.role])} ·{' '}
                {t(w.memberCount === 1 ? '{count} member' : '{count} members', { count: w.memberCount })}
              </div>
            </div>
            {w.id === team.id ? (
              <span className="badge success">
                <Check size={12} /> {t('Current')}
              </span>
            ) : (
              <Button variant="secondary" small onClick={() => switchTeam(w.id)}>
                {t('Switch')}
              </Button>
            )}
          </div>
        ))}
      </div>
      {creating && <NewWorkspaceModal onClose={() => setCreating(false)} />}
    </>
  );
}

/** Import sources: the server recognises Confluence exports by their content; the others are folders of Markdown. */
const IMPORT_SOURCES = [
  {
    value: 'auto',
    label: 'Automatic detection',
    steps: [
      'Upload a Markdown, HTML or text file, or a ZIP archive containing them.',
      'Confluence exports (XML or HTML) are recognised automatically; any other ZIP is imported as folders of Markdown files.',
      'Choose a specific source to see how to export from that tool.',
    ],
  },
  {
    value: 'confluence',
    label: 'Confluence',
    steps: [
      'In Confluence open the space, then Space settings → Export space (Server/Data Center: Space tools → Content Tools → Export).',
      'Choose XML (recommended: it keeps creation and modification dates) or HTML and export the whole space.',
      'Upload the ZIP as it is, without extracting it.',
      'Each space becomes a collection with its page tree, attachments and links between pages. Drafts, deleted pages, comments, version history and authors are not imported.',
    ],
  },
  {
    value: 'getpaper',
    label: '{product} backup',
    steps: [
      'In the source instance open Settings → Export and download the backup (workspace admins only).',
      'Upload the ZIP here as it is: collections, documents, drafts, history, comments and attachments are restored with their dates.',
      'Authors are matched by email with the members of this workspace; content of anyone else is attributed to you. Nothing already in this workspace is changed.',
    ],
  },
  {
    value: 'outline',
    label: 'Outline',
    steps: [
      'In Outline open Settings → Export, choose Markdown and export all collections.',
      'Upload the ZIP you receive by email or download: every collection becomes a collection here, with nested documents and attachments.',
    ],
  },
  {
    value: 'notion',
    label: 'Notion',
    steps: [
      'In Notion open Settings → Workspace → Export all workspace content (or ••• → Export on a page), choose Markdown & CSV and include subpages and files.',
      'Upload the ZIP: page IDs are removed from titles and links between pages are rewritten. Databases are imported as their pages.',
    ],
  },
  {
    value: 'obsidian',
    label: 'Obsidian',
    steps: [
      'Compress your vault folder into a ZIP and upload it; hidden folders such as .obsidian are ignored.',
      'Folders become collections and nested documents; {wikiLinks}, embedded images and callouts are converted.',
    ],
  },
  {
    value: 'markdown',
    label: 'Markdown / other',
    steps: [
      'Wiki.js, GitBook, BookStack and other tools: export as Markdown and upload the ZIP, or upload single .md, .html or .txt files.',
      'Top-level folders become collections, nested folders become nested documents.',
      'A document {file} and a folder {folder} are merged; {index} or {readme} inside a folder becomes the folder’s content.',
      'Images and other files are uploaded as attachments; relative links and {wikiLinks} are rewritten.',
    ],
  },
];

const IMPORT_FORMATS = {
  'confluence-xml': 'Confluence export (XML)',
  'confluence-html': 'Confluence export (HTML)',
  markdown: 'Markdown files',
  'getpaper-backup': '{product} backup',
};

/** Server errors with a code the client can translate */
const IMPORT_ERRORS = {
  not_confluence_export:
    'This archive is not a Confluence export: it contains neither entities.xml (XML export) nor Confluence HTML pages. Export the space as XML or HTML and upload the ZIP as it is.',
  confluence_export: 'This archive is a Confluence export. Choose Confluence or automatic detection as the source.',
  getpaper_backup: 'This archive is a {product} backup. Choose {product} backup or automatic detection as the source.',
  not_getpaper_backup: 'This archive is not a {product} backup: getpaper-backup.json is missing.',
};

function ImportData() {
  const { collections, refreshAllTrees } = useApp();
  const toast = useToast();
  const [source, setSource] = useState('auto');
  const [file, setFile] = useState(null);
  const [target, setTarget] = useState('');
  const [publish, setPublish] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState(null);
  const writable = collections.filter((c) => c.access === 'write');
  const current = IMPORT_SOURCES.find((s) => s.value === source);

  const submit = async (e) => {
    e.preventDefault();
    if (!file) return;
    setBusy(true);
    setResult(null);
    try {
      const r = await upload('collections.import', {
        file,
        source,
        collectionId: target || undefined,
        publish: publish ? 'true' : 'false',
      });
      setResult(r);
      await refreshAllTrees();
      toast(t(r.documents === 1 ? 'Imported {count} document' : 'Imported {count} documents', { count: r.documents }));
    } catch (err) {
      toast(IMPORT_ERRORS[err.code] ? t(IMPORT_ERRORS[err.code], { product: getProductName() }) : err.message, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h1 className="page-title">{t('Import')}</h1>
      <p className="page-subtitle muted">{t('Import documents from another wiki, or from Markdown, HTML and text files.')}</p>
      <form className="form settings-form" onSubmit={submit}>
        <Field label={t('Source')}>
          <select value={source} onChange={(e) => setSource(e.target.value)}>
            {IMPORT_SOURCES.map((s) => (
              <option key={s.value} value={s.value}>
                {t(s.label, { product: getProductName() })}
              </option>
            ))}
          </select>
        </Field>
        <ol className="import-help muted">
          {current.steps.map((step) => (
            <li key={step}>
              {rich(t(step), {
                file: <code>Page.md</code>,
                folder: <code>Page/</code>,
                index: <code>index.md</code>,
                readme: <code>README.md</code>,
                wikiLinks: <code>[[wiki links]]</code>,
              })}
            </li>
          ))}
        </ol>
        <Field label={t('File')}>
          <input
            type="file"
            accept=".zip,.md,.markdown,.txt,.html,.htm"
            onChange={(e) => setFile(e.target.files?.[0] || null)}
          />
        </Field>
        <Field label={t('Destination')}>
          <select value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">
              {source === 'confluence'
                ? t('New collections (one per space)')
                : source === 'getpaper'
                  ? t('The collections of the backup')
                  : t('New collections (one per top-level folder)')}
            </option>
            {writable.map((c) => (
              <option key={c.id} value={c.id}>
                {t('Into “{name}”', { name: c.name })}
              </option>
            ))}
          </select>
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">{t('Publish imported documents')}</div>
            <div className="field-hint">{t('When disabled, documents are imported as your private drafts.')}</div>
          </div>
          <Switch checked={publish} onChange={setPublish} />
        </div>
        <div>
          <Button type="submit" disabled={!file || busy}>
            {busy ? t('Importing…') : t('Import')}
          </Button>
        </div>
      </form>
      {result && (
        <div className="notice-bar info column">
          {result.format && (
            <div className="small">{t('Recognised format: {format}', { format: t(IMPORT_FORMATS[result.format] || result.format, { product: getProductName() }) })}</div>
          )}
          <div>
            {rich(t('Imported {documents} documents and {attachments} attachments into:'), {
              documents: <strong>{result.documents}</strong>,
              attachments: <strong>{result.attachments}</strong>,
            })}
          </div>
          <ul className="import-result">
            {result.collections.map((c) => (
              <li key={c.id}>
                <Link to={collectionUrl(c)}>{c.name}</Link> {c.created ? t('(new collection)') : ''}
              </li>
            ))}
          </ul>
          {result.skipped.length > 0 && (
            <div className="small">{t('Skipped: {files}', { files: result.skipped.join(', ') })}</div>
          )}
        </div>
      )}
    </>
  );
}

function formatSize(bytes) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function ExportCard({ title, description, items, button, format, disabled, note }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      const r = await downloadFile('workspace.export', { format });
      toast(t('Downloaded {name} ({size})', { name: r.name, size: formatSize(r.size) }));
    } catch (err) {
      toast(err.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="export-card">
      <h2 className="section-title">{title}</h2>
      <p className="muted">{description}</p>
      <ul className="import-help muted">
        {items.map((i) => (
          <li key={i}>{rich(i, { file: <code>getpaper-backup.json</code>, wikiLinks: <code>[[wiki links]]</code>, callout: <code>{'> [!info]'}</code> })}</li>
        ))}
      </ul>
      {note && <p className="muted small">{note}</p>}
      <Button onClick={run} disabled={busy || disabled}>
        <Download size={16} /> {busy ? t('Preparing the archive…') : button}
      </Button>
    </div>
  );
}

function ExportData() {
  const { isAdmin, team, instance } = useApp();
  const backupOff = instance.allowBackupExport === false;
  const markdownOff = instance.allowMarkdownExport === false;
  return (
    <>
      <h1 className="page-title">{t('Export')}</h1>
      <p className="page-subtitle muted">{t('Download the content of the “{name}” workspace.', { name: team.name })}</p>
      <ExportCard
        title={t('Backup for {product}', { product: getProductName() })}
        description={t('A complete copy of the workspace, to restore it in another {product} instance from Settings → Import.', { product: getProductName() })}
        items={[
          t('Collections with icons, colours and permissions, documents with their hierarchy, drafts, templates and archived documents.'),
          t('Creation and modification dates, version history, comments and members of private collections.'),
          t('All attachments. The data is in {file}, the files in the attachments folder.'),
          t('When restoring, authors are matched by email with the members of the target workspace; content of anyone else is attributed to whoever restores it.'),
        ]}
        note={
          backupOff
            ? t('This export has been disabled by the administrator.')
            : isAdmin
              ? null
              : t('Only workspace admins can download a backup.')
        }
        disabled={!isAdmin || backupOff}
        button={t('Download backup')}
        format="backup"
      />
      <ExportCard
        title={t('Obsidian vault (Markdown)')}
        description={t('The published documents you can read, as Markdown files ready to be opened as an Obsidian vault.')}
        items={[
          t('A folder for each collection; nested documents are a file plus a folder with the same name.'),
          t('Links between documents become {wikiLinks}, images and files are in the _attachments folder.'),
          t('Notices become Obsidian callouts ({callout}); creation and modification dates are in the front matter.'),
          t('Drafts, templates, archived documents, comments and history are not included.'),
        ]}
        note={markdownOff ? t('This export has been disabled by the administrator.') : null}
        disabled={markdownOff}
        button={t('Download vault')}
        format="obsidian"
      />
    </>
  );
}

const PRESET_COLORS = ['#c8102e', '#e4002b', '#d97706', '#15803d', '#0f766e', '#1d4ed8', '#4338ca', '#111827'];

/** Optional colour: picker, hex field, presets and reset. `null` = default look. */
function ColorSetting({ label, hint, value, onChange }) {
  const [text, setText] = useState(value || '');
  useEffect(() => setText(value || ''), [value]);
  const commit = (v) => {
    const c = v.trim().toLowerCase();
    if (c === (value || '')) return;
    if (c === '') onChange(null);
    else if (/^#[0-9a-f]{6}$/.test(c)) onChange(c);
  };
  return (
    <Field label={label} hint={hint}>
      <div className="color-setting">
        <input type="color" value={value || '#ffffff'} onChange={(e) => onChange(e.target.value)} aria-label={label} />
        <input
          className="color-hex"
          value={text}
          placeholder={t('Default')}
          onChange={(e) => setText(e.target.value)}
          onBlur={(e) => commit(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              commit(e.currentTarget.value);
            }
          }}
        />
        <div className="color-presets">
          {PRESET_COLORS.map((c) => (
            <button
              type="button"
              key={c}
              className={`color-choice ${value === c ? 'selected' : ''}`}
              style={{ background: c }}
              title={c}
              onClick={() => onChange(c)}
            />
          ))}
        </div>
        {value && (
          <Button variant="secondary" onClick={() => onChange(null)}>
            {t('Reset')}
          </Button>
        )}
      </div>
    </Field>
  );
}

function ToggleSetting({ label, hint, checked, onChange }) {
  return (
    <div className="field row space-between">
      <div>
        <div className="field-label">{label}</div>
        {hint && <div className="field-hint">{hint}</div>}
      </div>
      <Switch checked={checked} onChange={onChange} />
    </div>
  );
}

/** Text setting saved when the field loses focus or on Enter; empty = default (shown as placeholder) */
function TextSetting({ label, hint, value, placeholder, maxLength, onChange }) {
  const [text, setText] = useState(value || '');
  useEffect(() => setText(value || ''), [value]);
  const commit = () => {
    const v = text.trim();
    if (v !== (value || '')) onChange(v || null);
  };
  return (
    <Field label={label} hint={hint}>
      <input
        value={text}
        placeholder={placeholder}
        maxLength={maxLength}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
          }
        }}
      />
    </Field>
  );
}

/** Installation-wide settings, superadmins only */
function EmailCheck({ enabled, email }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async (checkOnly) => {
    setBusy(true);
    try {
      const r = await api('instance.testEmail', { checkOnly });
      toast(checkOnly ? t('Connected to {host}:{port}', { host: r.host, port: r.port }) : t('Test email sent to {email}', { email: r.sentTo }));
    } catch (e) {
      toast(e.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  if (!enabled) {
    return (
      <p className="muted">
        {t('Email is not configured. Set SMTP_HOST, SMTP_PORT, SMTP_USERNAME, SMTP_PASSWORD and SMTP_FROM_EMAIL in the server configuration to send invitation emails.')}
      </p>
    );
  }
  return (
    <>
      <p className="muted">{t('Invitation emails are sent with the mail server set in the server configuration.')}</p>
      <div className="row gap">
        <Button variant="secondary" disabled={busy} onClick={() => run(true)}>
          {t('Check connection')}
        </Button>
        <Button variant="secondary" icon={<Send size={14} />} disabled={busy} onClick={() => run(false)}>
          {t('Send a test email to {email}', { email })}
        </Button>
      </div>
    </>
  );
}

function GeneralSettings() {
  const { isSuperadmin, refreshAuth, user, auth } = useApp();
  const toast = useToast();
  const [data, setData] = useState(null);
  const [email, setEmail] = useState('');

  useEffect(() => {
    if (isSuperadmin) api('instance.info').then(setData).catch((e) => toast(e.message, 'error'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isSuperadmin]);

  if (!isSuperadmin) return <Empty>{t('Only superadmins can change the general settings.')}</Empty>;
  if (!data) return <Spinner />;
  const s = data.settings;
  const env = data.environmentDefaults;

  const update = async (patch) => {
    try {
      setData(await api('instance.update', patch));
      await refreshAuth();
      toast(t('Settings saved'));
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const changeSuperadmins = async (method, body) => {
    try {
      setData(await api(method, body));
      setEmail('');
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const choice = (options, current, fallback, field) => (
    <select value={current || ''} onChange={(e) => update({ [field]: e.target.value || null })}>
      <option value="">
        {t('Server default ({value})', { value: t(options.find((o) => o.value === fallback)?.label || fallback) })}
      </option>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {t(o.label)}
        </option>
      ))}
    </select>
  );

  return (
    <>
      <h1 className="page-title">{t('General settings')}</h1>
      <p className="page-subtitle muted">{t('These settings apply to every workspace of this installation.')}</p>

      <h2 className="section-title">{t('Branding')}</h2>
      <div className="form settings-form">
        <TextSetting
          label={t('Product name')}
          hint={t('Shown in the browser tab, on the sign-in page, next to the logo and in emails. Leave empty to use {name}.', {
            name: DEFAULT_PRODUCT_NAME,
          })}
          value={s.productName}
          placeholder={DEFAULT_PRODUCT_NAME}
          maxLength={40}
          onChange={(v) => update({ productName: v })}
        />
        <ColorSetting
          label={t('Header colour')}
          hint={t('Background of the top bar. The text colour is chosen automatically for contrast.')}
          value={s.headerColor}
          onChange={(v) => update({ headerColor: v })}
        />
        <ColorSetting
          label={t('Accent colour')}
          hint={t('Buttons, links and selections.')}
          value={s.accentColor}
          onChange={(v) => update({ accentColor: v })}
        />
      </div>

      <h2 className="section-title">{t('Data protection')}</h2>
      <div className="form settings-form">
        <ToggleSetting
          label={t('Workspace backups')}
          hint={t('Workspace admins can download a full backup from Settings → Export.')}
          checked={s.allowBackupExport}
          onChange={(v) => update({ allowBackupExport: v })}
        />
        <ToggleSetting
          label={t('Markdown export')}
          hint={t('Members can download the Obsidian vault and single documents as Markdown.')}
          checked={s.allowMarkdownExport}
          onChange={(v) => update({ allowMarkdownExport: v })}
        />
        <ToggleSetting
          label={t('Public links')}
          hint={t('Documents can be published to the internet. When disabled, existing links stop working until it is enabled again.')}
          checked={s.allowPublicSharing}
          onChange={(v) => update({ allowPublicSharing: v })}
        />
      </div>

      <h2 className="section-title">{t('Email')}</h2>
      <EmailCheck enabled={!!auth.email} email={user.email} />

      <h2 className="section-title">{t('Defaults for users')}</h2>
      <p className="muted">{t('Used when a user has not chosen their own preference in their profile.')}</p>
      <div className="form settings-form">
        <Field label={t('Language')}>{choice(LANGUAGES, s.defaultLanguage, env.language, 'defaultLanguage')}</Field>
        <Field label={t('Appearance')}>{choice(THEMES, s.defaultTheme, env.theme, 'defaultTheme')}</Field>
        <Field label={t('Document width')}>
          {choice(CONTENT_WIDTHS, s.defaultContentWidth, env.contentWidth, 'defaultContentWidth')}
        </Field>
      </div>

      <h2 className="section-title">{t('Superadmins')}</h2>
      <p className="muted">
        {t('Superadmins manage these settings. They don’t get access to workspaces they are not members of.')}
      </p>
      <div className="superadmin-list">
        {data.superadmins.map((u) => (
          <div key={u.id} className="row space-between superadmin-row">
            <div className="row gap">
              <Avatar name={u.name} color={u.color} size={28} />
              <div>
                <div>
                  {u.name} {u.id === user.id && <span className="muted">({t('you')})</span>}
                </div>
                <div className="muted small">{u.email}</div>
              </div>
            </div>
            {u.fromEnvironment ? (
              <span className="muted small" title={t('Set in the server configuration')}>
                SUPERADMIN_EMAILS
              </span>
            ) : (
              data.superadmins.length > 1 && (
                <Button variant="secondary" onClick={() => changeSuperadmins('instance.removeSuperadmin', { id: u.id })}>
                  {t('Remove')}
                </Button>
              )
            )}
          </div>
        ))}
      </div>
      <form
        className="row gap superadmin-add"
        onSubmit={(e) => {
          e.preventDefault();
          if (email.trim()) changeSuperadmins('instance.addSuperadmin', { email });
        }}
      >
        <input
          type="email"
          className="grow"
          placeholder={t('Email of an existing account')}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Button type="submit" disabled={!email.trim()}>
          {t('Add superadmin')}
        </Button>
      </form>
    </>
  );
}

function McpSection() {
  const toast = useToast();
  const endpoint = `${window.location.origin}/mcp`;
  const cmd = `claude mcp add --transport http getpaper ${endpoint} --header "Authorization: Bearer <API_KEY>"`;
  const json = `{
  "mcpServers": {
    "getpaper": {
      "type": "http",
      "url": "${endpoint}",
      "headers": { "Authorization": "Bearer <API_KEY>" }
    }
  }
}`;
  return (
    <>
      <h2 className="section-title">{t('MCP server')}</h2>
      <p className="muted">
        {rich(
          t(
            'Connect an LLM client (Claude Code, Claude Desktop, Cursor, …) through the Model Context Protocol. The assistant works within the limits of the API key: read-only or read/write, its workspaces and collections. Create an API key above and use it in place of {placeholder}.'
          ),
          { placeholder: <code>&lt;API_KEY&gt;</code> }
        )}
      </p>
      <Field label={t('Endpoint (Streamable HTTP)')}>
        <div className="row gap">
          <input readOnly value={endpoint} className="grow mono" />
          <Button
            variant="secondary"
            icon={<Copy size={14} />}
            onClick={async () => {
              await copyToClipboard(endpoint);
              toast(t('Copied'));
            }}
          >
            {t('Copy')}
          </Button>
        </div>
      </Field>
      <p className="muted small">
        {rich(
          t(
            'A key valid in several workspaces needs a single configuration: the assistant lists them with {listTool} and chooses one with the {param} argument of every tool; the search can cover all of them.'
          ),
          { listTool: <code>list_workspaces</code>, param: <code>workspace</code> }
        )}
      </p>
      <div className="field-label">Claude Code</div>
      <pre className="code-sample bordered">{cmd}</pre>
      <div className="field-label">{t('Other clients ({file})', { file: 'mcp.json' })}</div>
      <pre className="code-sample bordered">{json}</pre>
    </>
  );
}

// Titles are kept in English here and translated with t() at render time.
const SECTIONS = {
  profile: { title: 'Profile', component: Profile },
  workspace: { title: 'Details', component: Workspace },
  members: { title: 'Members', component: Members },
  groups: { title: 'Groups', component: Groups },
  shares: { title: 'Shared Links', component: Shares },
  'api-keys': { title: 'API Keys', component: ApiKeys },
  workspaces: { title: 'Workspaces', component: Workspaces },
  import: { title: 'Import', component: ImportData },
  export: { title: 'Export', component: ExportData },
  general: { title: 'General settings', component: GeneralSettings },
};

export default function Settings() {
  const { section } = useParams();
  const s = SECTIONS[section];
  if (!s) return <Navigate to="/settings/profile" replace />;
  const C = s.component;
  return (
    <Scene title={t(s.title)} headerTitle={<span className="header-title">{t('Settings')}</span>}>
      <C />
    </Scene>
  );
}
