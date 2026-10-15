import React, { useState } from 'react';
import { Copy } from 'lucide-react';
import { api, copyToClipboard } from '../api';
import { useApp } from '../store';
import { useToast } from './Toast';
import { Button, Field, Modal } from './ui';

export default function InviteModal({ onClose, onInvited }) {
  const { team, isAdmin } = useApp();
  const [emails, setEmails] = useState('');
  const [role, setRole] = useState('member');
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
        const r = await api('invites.create', { email, role });
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
    <Modal title={`Invite people to ${team.name}`} onClose={onClose} width={560}>
      {!results ? (
        <form onSubmit={submit} className="form">
          <p className="modal-text">
            Invite members to join your workspace. Existing users are added immediately; new users receive a personal
            invite link that you can share with them.
          </p>
          <Field label="Email addresses" hint="Separate multiple addresses with commas or new lines.">
            <textarea
              autoFocus
              rows={3}
              value={emails}
              onChange={(e) => setEmails(e.target.value)}
              placeholder="name@example.com"
            />
          </Field>
          <Field label="Role">
            <select value={role} onChange={(e) => setRole(e.target.value)}>
              {isAdmin && <option value="admin">Admin</option>}
              <option value="member">Editor</option>
              <option value="viewer">Viewer</option>
            </select>
          </Field>
          <div className="modal-actions">
            <Button variant="secondary" type="button" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !emails.trim()}>
              {busy ? 'Sending…' : 'Send invites'}
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
                <div className="muted small">Added to the workspace.</div>
              ) : r.alreadyMember ? (
                <div className="muted small">Already a member.</div>
              ) : (
                <div className="row gap">
                  <input readOnly value={r.link} onFocus={(e) => e.target.select()} className="grow" />
                  <Button
                    variant="secondary"
                    icon={<Copy size={14} />}
                    onClick={async () => {
                      await copyToClipboard(r.link);
                      toast('Invite link copied');
                    }}
                  >
                    Copy
                  </Button>
                </div>
              )}
            </div>
          ))}
          <div className="modal-actions">
            <Button onClick={onClose}>Done</Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
