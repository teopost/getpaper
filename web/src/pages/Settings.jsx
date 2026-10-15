import React, { useEffect, useState } from 'react';
import { Link, Navigate, useParams } from 'react-router-dom';
import { Copy, Plus, Trash2, UserPlus, Check } from 'lucide-react';
import { api, collectionUrl, copyToClipboard, docUrl, timeAgo, upload } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import { Avatar, Button, ConfirmModal, Empty, Field, Modal, Switch, TeamAvatar } from '../components/ui';
import { useToast } from '../components/Toast';
import { COLORS } from '../components/icons';
import InviteModal from '../components/InviteModal';
import NewWorkspaceModal from '../components/NewWorkspaceModal';

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
      <h1 className="page-title">Profile</h1>
      <p className="page-subtitle muted">Manage how you appear to other members of the workspace.</p>
      <form
        className="form settings-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api('users.update', { name, color });
            await refreshAuth();
            toast('Profile saved');
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        <div className="row gap">
          <Avatar name={name} color={color} size={56} />
          <ColorPicker value={color} onChange={setColor} />
        </div>
        <Field label="Name">
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label="Email">
          <input value={user.email} disabled />
        </Field>
        <div>
          <Button type="submit">Save</Button>
        </div>
      </form>

      <h2 className="section-title">Password</h2>
      {!hasPassword && (
        <p className="muted">You sign in with Microsoft. You can optionally set a password as well.</p>
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
            toast(hasPassword ? 'Password changed' : 'Password set');
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        {hasPassword && (
          <Field label="Current password">
            <input type="password" value={current} onChange={(e) => setCurrent(e.target.value)} required autoComplete="current-password" />
          </Field>
        )}
        <Field label="New password" hint="At least 8 characters.">
          <input type="password" value={next} onChange={(e) => setNext(e.target.value)} required minLength={8} autoComplete="new-password" />
        </Field>
        <div>
          <Button type="submit" variant="secondary">
            {hasPassword ? 'Change password' : 'Set password'}
          </Button>
        </div>
      </form>
    </>
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
      <h1 className="page-title">Details</h1>
      <p className="page-subtitle muted">These settings affect the way that your workspace appears to everyone on the team.</p>
      <form
        className="form settings-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api('teams.update', { name, color, sharing, defaultUserRole: defaultRole, allowedDomains });
            await refreshAuth();
            toast('Settings saved');
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        <div className="row gap">
          <TeamAvatar team={{ name, color }} size={56} />
          <ColorPicker value={color} onChange={setColor} />
        </div>
        <Field label="Name">
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">Public document sharing</div>
            <div className="field-hint">When enabled, documents can be shared publicly on the internet by any member.</div>
          </div>
          <Switch checked={sharing} onChange={setSharing} />
        </div>
        <Field
          label="Allowed domains"
          hint="People signing in with Microsoft whose email belongs to one of these domains join this workspace automatically, with the default role. Separate multiple domains with commas."
        >
          <input value={allowedDomains} onChange={(e) => setAllowedDomains(e.target.value)} placeholder="e.g. horsa.it, vidata.it" />
        </Field>
        <Field label="Default role" hint="The role new members get when joining automatically.">
          <select value={defaultRole} onChange={(e) => setDefaultRole(e.target.value)}>
            <option value="member">Editor</option>
            <option value="viewer">Viewer</option>
            <option value="admin">Admin</option>
          </select>
        </Field>
        <div>
          <Button type="submit">Save</Button>
        </div>
      </form>

      <h2 className="section-title danger-text">Danger zone</h2>
      <p className="muted">
        Deleting the workspace permanently removes all of its collections, documents and memberships. Your account and
        other workspaces are not affected.
      </p>
      <Button variant="danger" onClick={() => setDeleting(true)}>
        Delete workspace…
      </Button>
      {deleting && (
        <Modal title="Delete workspace" onClose={() => setDeleting(false)}>
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
              Type <strong>{team.name}</strong> to confirm. This action cannot be undone.
            </p>
            <input autoFocus value={confirmName} onChange={(e) => setConfirmName(e.target.value)} />
            <div className="modal-actions">
              <Button variant="secondary" type="button" onClick={() => setDeleting(false)}>
                Cancel
              </Button>
              <Button variant="danger" type="submit" disabled={confirmName.trim() !== team.name.trim()}>
                Delete workspace
              </Button>
            </div>
          </form>
        </Modal>
      )}
    </>
  );
}

const ROLE_LABEL = { admin: 'Admin', member: 'Editor', viewer: 'Viewer' };

function Members() {
  const { user, isAdmin, canWrite } = useApp();
  const toast = useToast();
  const [members, setMembers] = useState(null);
  const [invites, setInvites] = useState([]);
  const [query, setQuery] = useState('');
  const [inviting, setInviting] = useState(false);
  const [removing, setRemoving] = useState(null);

  const load = () => {
    api('users.list', { query }).then(setMembers);
    if (canWrite) api('invites.list').then(setInvites);
  };
  useEffect(() => {
    const t = setTimeout(load, 200);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  return (
    <>
      <div className="row space-between">
        <h1 className="page-title">Members</h1>
        {canWrite && (
          <Button icon={<UserPlus size={16} />} onClick={() => setInviting(true)}>
            Invite people…
          </Button>
        )}
      </div>
      <p className="page-subtitle muted">
        Everyone that has signed into this workspace. Admins can manage roles; editors can create and edit documents;
        viewers have read-only access.
      </p>
      <input placeholder="Filter…" value={query} onChange={(e) => setQuery(e.target.value)} className="filter-input" />
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Email</th>
              <th>Last active</th>
              <th>Role</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {members?.map((m) => (
              <tr key={m.id}>
                <td>
                  <span className="row gap-sm">
                    <Avatar name={m.name} color={m.color} size={28} />
                    {m.name} {m.id === user.id && <span className="muted">(you)</span>}
                  </span>
                </td>
                <td className="muted">{m.email}</td>
                <td className="muted">{m.lastActiveAt ? timeAgo(m.lastActiveAt) : 'Never'}</td>
                <td>
                  {isAdmin ? (
                    <select
                      value={m.role}
                      onChange={async (e) => {
                        try {
                          await api('users.updateRole', { id: m.id, role: e.target.value });
                          toast('Role updated');
                          load();
                        } catch (err) {
                          toast(err.message, 'error');
                        }
                      }}
                    >
                      <option value="admin">Admin</option>
                      <option value="member">Editor</option>
                      <option value="viewer">Viewer</option>
                    </select>
                  ) : (
                    <span className="badge">{ROLE_LABEL[m.role]}</span>
                  )}
                </td>
                <td className="right">
                  {(isAdmin || m.id === user.id) && (
                    <Button variant="secondary" small onClick={() => setRemoving(m)}>
                      {m.id === user.id ? 'Leave' : 'Remove'}
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
          <h2 className="section-title">Pending invites</h2>
          <div className="table-wrap">
            <table className="table">
              <tbody>
                {invites.map((i) => (
                  <tr key={i.id}>
                    <td>{i.email}</td>
                    <td className="muted">{ROLE_LABEL[i.role]}</td>
                    <td className="muted">Invited {timeAgo(i.createdAt)}</td>
                    <td className="right">
                      <span className="row gap-sm justify-end">
                        <Button
                          variant="secondary"
                          small
                          icon={<Copy size={14} />}
                          onClick={async () => {
                            await copyToClipboard(i.link);
                            toast('Invite link copied');
                          }}
                        >
                          Copy link
                        </Button>
                        <Button
                          variant="secondary"
                          small
                          onClick={async () => {
                            await api('invites.revoke', { id: i.id });
                            load();
                          }}
                        >
                          Revoke
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
      {removing && (
        <ConfirmModal
          title={removing.id === user.id ? 'Leave workspace' : 'Remove member'}
          danger
          confirmLabel={removing.id === user.id ? 'Leave' : 'Remove'}
          message={
            removing.id === user.id
              ? 'You will lose access to this workspace and its documents.'
              : `${removing.name} will lose access to this workspace. Their documents are kept.`
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
      <h1 className="page-title">Shared Links</h1>
      <p className="page-subtitle muted">Documents that have been shared publicly are listed below. Anyone with the link can access them.</p>
      {shares && shares.length === 0 && <Empty>No documents have been shared yet.</Empty>}
      {shares && shares.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Document</th>
                <th>Shared by</th>
                <th>Views</th>
                <th>Last accessed</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {shares.map((s) => (
                <tr key={s.id}>
                  <td>
                    <Link to={docUrl({ title: s.documentTitle, urlId: s.documentUrlId })}>{s.documentTitle || 'Untitled'}</Link>
                  </td>
                  <td className="muted">{s.createdByName}</td>
                  <td className="muted">{s.views}</td>
                  <td className="muted">{s.lastAccessedAt ? timeAgo(s.lastAccessedAt) : 'Never'}</td>
                  <td className="right">
                    <span className="row gap-sm justify-end">
                      <Button
                        variant="secondary"
                        small
                        icon={<Copy size={14} />}
                        onClick={async () => {
                          await copyToClipboard(s.url);
                          toast('Link copied');
                        }}
                      >
                        Copy
                      </Button>
                      <Button
                        variant="secondary"
                        small
                        onClick={async () => {
                          await api('shares.revoke', { id: s.id });
                          toast('Share revoked');
                          load();
                        }}
                      >
                        Revoke
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

function ApiKeys() {
  const [keys, setKeys] = useState(null);
  const [name, setName] = useState('');
  const [created, setCreated] = useState(null);
  const toast = useToast();
  const load = () => api('apiKeys.list').then(setKeys);
  useEffect(() => {
    load();
  }, []);
  return (
    <>
      <h1 className="page-title">API Keys</h1>
      <p className="page-subtitle muted">
        Create personal API keys to authenticate with the API and programmatically control your workspace’s data. API keys
        have the same permissions as your user account and are scoped to the current workspace.
      </p>
      <form
        className="row gap"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            setCreated(await api('apiKeys.create', { name }));
            setName('');
            load();
          } catch (err) {
            toast(err.message, 'error');
          }
        }}
      >
        <input placeholder="Key name, e.g. Development" value={name} onChange={(e) => setName(e.target.value)} className="grow" required />
        <Button type="submit" icon={<Plus size={16} />}>
          New API key
        </Button>
      </form>
      {created && (
        <div className="notice-bar info column">
          <div>
            Copy your new key <strong>{created.name}</strong> now, you won’t be able to see it again:
          </div>
          <div className="row gap">
            <input readOnly value={created.secret} className="grow mono" onFocus={(e) => e.target.select()} />
            <Button
              variant="secondary"
              icon={<Copy size={14} />}
              onClick={async () => {
                await copyToClipboard(created.secret);
                toast('API key copied');
              }}
            >
              Copy
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
            {keys?.map((k) => (
              <tr key={k.id}>
                <td>
                  <strong>{k.name}</strong> <span className="muted mono">…{k.last4}</span>
                </td>
                <td className="muted">Created {timeAgo(k.createdAt)}</td>
                <td className="muted">{k.lastUsedAt ? `Used ${timeAgo(k.lastUsedAt)}` : 'Never used'}</td>
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
                    Revoke
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {keys && keys.length === 0 && <Empty>You haven’t created any API keys yet.</Empty>}
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
        <h1 className="page-title">Workspaces</h1>
        <Button icon={<Plus size={16} />} onClick={() => setCreating(true)}>
          New workspace
        </Button>
      </div>
      <p className="page-subtitle muted">
        Each workspace is a completely separate knowledge base with its own members, collections, permissions and
        settings. Use the workspace switcher at the top of the sidebar to move between them.
      </p>
      <div className="workspace-grid">
        {teams.map((t) => (
          <div key={t.id} className={`workspace-card ${t.id === team.id ? 'current' : ''}`}>
            <TeamAvatar team={t} size={40} />
            <div className="grow">
              <div className="workspace-name">{t.name}</div>
              <div className="muted small">
                {ROLE_LABEL[t.role]} · {t.memberCount} member{t.memberCount === 1 ? '' : 's'}
              </div>
            </div>
            {t.id === team.id ? (
              <span className="badge success">
                <Check size={12} /> Current
              </span>
            ) : (
              <Button variant="secondary" small onClick={() => switchTeam(t.id)}>
                Switch
              </Button>
            )}
          </div>
        ))}
      </div>
      {creating && <NewWorkspaceModal onClose={() => setCreating(false)} />}
    </>
  );
}

function ImportData() {
  const { collections, refreshAllTrees } = useApp();
  const toast = useToast();
  const [file, setFile] = useState(null);
  const [target, setTarget] = useState('');
  const [publish, setPublish] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState(null);
  const writable = collections.filter((c) => c.access === 'write');

  const submit = async (e) => {
    e.preventDefault();
    if (!file) return;
    setBusy(true);
    setResult(null);
    try {
      const r = await upload('collections.import', {
        file,
        collectionId: target || undefined,
        publish: publish ? 'true' : 'false',
      });
      setResult(r);
      await refreshAllTrees();
      toast(`Imported ${r.documents} documents`);
    } catch (err) {
      toast(err.message, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h1 className="page-title">Import</h1>
      <p className="page-subtitle muted">
        Import Markdown, HTML or text files, or a ZIP archive containing them. Exports from Outline, Notion, Obsidian,
        Wiki.js, GitBook and BookStack (Markdown format) are supported.
      </p>
      <ul className="import-help muted">
        <li>Top-level folders become collections, nested folders become nested documents.</li>
        <li>
          A document <code>Page.md</code> and a folder <code>Page/</code> are merged; <code>index.md</code> or{' '}
          <code>README.md</code> inside a folder becomes the folder&apos;s content.
        </li>
        <li>Images and other files are uploaded as attachments; relative links and <code>[[wiki links]]</code> are rewritten.</li>
      </ul>
      <form className="form settings-form" onSubmit={submit}>
        <Field label="File">
          <input
            type="file"
            accept=".zip,.md,.markdown,.txt,.html,.htm"
            onChange={(e) => setFile(e.target.files?.[0] || null)}
          />
        </Field>
        <Field label="Destination">
          <select value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">New collections (one per top-level folder)</option>
            {writable.map((c) => (
              <option key={c.id} value={c.id}>
                Into “{c.name}”
              </option>
            ))}
          </select>
        </Field>
        <div className="field row space-between">
          <div>
            <div className="field-label">Publish imported documents</div>
            <div className="field-hint">When disabled, documents are imported as your private drafts.</div>
          </div>
          <Switch checked={publish} onChange={setPublish} />
        </div>
        <div>
          <Button type="submit" disabled={!file || busy}>
            {busy ? 'Importing…' : 'Import'}
          </Button>
        </div>
      </form>
      {result && (
        <div className="notice-bar info column">
          <div>
            Imported <strong>{result.documents}</strong> documents and <strong>{result.attachments}</strong> attachments into:
          </div>
          <ul className="import-result">
            {result.collections.map((c) => (
              <li key={c.id}>
                <Link to={collectionUrl(c)}>{c.name}</Link> {c.created ? '(new collection)' : ''}
              </li>
            ))}
          </ul>
          {result.skipped.length > 0 && <div className="small">Skipped: {result.skipped.join(', ')}</div>}
        </div>
      )}
    </>
  );
}

function McpSection() {
  const toast = useToast();
  const endpoint = `${window.location.origin}/mcp`;
  const cmd = `claude mcp add --transport http horsa-codex ${endpoint} --header "Authorization: Bearer <API_KEY>"`;
  const json = `{
  "mcpServers": {
    "horsa-codex": {
      "type": "http",
      "url": "${endpoint}",
      "headers": { "Authorization": "Bearer <API_KEY>" }
    }
  }
}`;
  return (
    <>
      <h2 className="section-title">MCP server</h2>
      <p className="muted">
        Connect an LLM client (Claude Code, Claude Desktop, Cursor, …) to this workspace through the Model Context
        Protocol. The assistant can search, read, write, organise, comment and share documents with the permissions of
        the API key owner. Create an API key above and use it in place of <code>&lt;API_KEY&gt;</code>.
      </p>
      <Field label="Endpoint (Streamable HTTP)">
        <div className="row gap">
          <input readOnly value={endpoint} className="grow mono" />
          <Button
            variant="secondary"
            icon={<Copy size={14} />}
            onClick={async () => {
              await copyToClipboard(endpoint);
              toast('Copied');
            }}
          >
            Copy
          </Button>
        </div>
      </Field>
      <div className="field-label">Claude Code</div>
      <pre className="code-sample bordered">{cmd}</pre>
      <div className="field-label">Other clients (mcp.json)</div>
      <pre className="code-sample bordered">{json}</pre>
    </>
  );
}

const SECTIONS = {
  profile: { title: 'Profile', component: Profile },
  workspace: { title: 'Details', component: Workspace },
  members: { title: 'Members', component: Members },
  shares: { title: 'Shared Links', component: Shares },
  'api-keys': { title: 'API Keys', component: ApiKeys },
  workspaces: { title: 'Workspaces', component: Workspaces },
  import: { title: 'Import', component: ImportData },
};

export default function Settings() {
  const { section } = useParams();
  const s = SECTIONS[section];
  if (!s) return <Navigate to="/settings/profile" replace />;
  const C = s.component;
  return (
    <Scene title={s.title} headerTitle={<span className="header-title">Settings</span>}>
      <C />
    </Scene>
  );
}
