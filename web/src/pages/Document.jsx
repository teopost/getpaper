import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import {
  Archive,
  Bell,
  BellOff,
  ChevronRight,
  Copy,
  CornerDownRight,
  Download,
  History,
  LayoutTemplate,
  Maximize2,
  MessageSquare,
  MoreHorizontal,
  Plus,
  RotateCcw,
  Star,
  Trash2,
  Undo2,
  X,
  Globe,
  Smile,
  FolderInput,
  FileText,
  FileCode,
  FileType,
  Printer,
  Presentation,
  Pin,
  PinOff,
  Link2,
} from 'lucide-react';
import {
  api,
  collectionUrl,
  copyToClipboard,
  docUrl,
  downloadText,
  formatDate,
  slugify,
  timeAgo,
} from '../api';
import { useApp } from '../store';
import Editor from '../editor/Editor';
import DocumentAccess from '../components/DocumentAccess';
import { flatText, locateAnchor } from '../editor/extensions';
import Scene from '../components/Scene';
import { CollectionIcon, DocIcon } from '../components/icons';
import { Avatar, Button, ConfirmModal, Field, IconButton, Menu, Modal, Popover, Spinner, Switch } from '../components/ui';
import EmojiPicker from '../components/EmojiPicker';
import { notificationsChanged } from '../components/Notifications';
import PresentationMode from '../components/PresentationMode';
import { useToast } from '../components/Toast';
import { t } from '../i18n';
import { getProductName } from '../prefs';
import { replaceTemplateVariables } from '../templates';

// ---------------------------------------------------------------------------
// Share popover
// ---------------------------------------------------------------------------

/** onChange(share): the public link was created (share) or revoked (null) */
function SharePanel({ doc, onChange }) {
  const { instance } = useApp();
  const [share, setShare] = useState(undefined);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  useEffect(() => {
    api('shares.info', { documentId: doc.id }).then(setShare).catch(() => setShare(null));
  }, [doc.id]);
  const internalUrl = `${window.location.origin}${docUrl(doc)}`;
  const toggle = async (on) => {
    setBusy(true);
    try {
      if (on) {
        const created = await api('shares.create', { documentId: doc.id });
        setShare(created);
        onChange?.(created);
      } else {
        await api('shares.revoke', { id: share.id });
        setShare(null);
        onChange?.(null);
      }
    } catch (e) {
      toast(e.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="share-panel">
      {doc.publishedAt && (
        <>
          <DocumentAccess doc={doc} />
          <div className="share-divider" />
        </>
      )}
      {instance.allowPublicSharing === false ? (
        <div className="muted small">{t('Public links have been disabled by the administrator.')}</div>
      ) : (
      <>
      <div className="share-row">
        <Globe size={20} className="muted" />
        <div className="grow">
          <div className="share-title">{t('Publish to internet')}</div>
          <div className="muted small">
            {share ? t('Anyone with the link can view this document') : t('Only members of this workspace can view')}
          </div>
        </div>
        <Switch checked={!!share} disabled={busy || share === undefined || !doc.publishedAt} onChange={toggle} />
      </div>
      {!doc.publishedAt && <div className="muted small">{t('Publish the document before sharing it publicly.')}</div>}
      {share && (
        <div className="row gap">
          <input readOnly value={share.url} className="grow" onFocus={(e) => e.target.select()} />
          <Button
            variant="secondary"
            icon={<Copy size={14} />}
            onClick={async () => {
              await copyToClipboard(share.url);
              toast(t('Public link copied to clipboard'));
            }}
          >
            {t('Copy')}
          </Button>
        </div>
      )}
      {share && <ShareOptions share={share} onChange={setShare} />}
      </>
      )}
      <div className="share-divider" />
      <Button
        variant="secondary"
        className="btn-block"
        icon={<Copy size={14} />}
        onClick={async () => {
          await copyToClipboard(internalUrl);
          toast(t('Link copied to clipboard'));
        }}
      >
        {t('Copy internal link')}
      </Button>
    </div>
  );
}

/** Options of a public link, as in Outline */
function ShareOptions({ share, onChange }) {
  const toast = useToast();
  const set = async (field, value) => {
    try {
      onChange(await api('shares.update', { id: share.id, [field]: value }));
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const options = [
    ['includeChildDocuments', t('Include nested documents'), t('The documents inside this one can be read through the link too.')],
    ['allowIndexing', t('Search engine indexing'), t('Search engines may list the pages of the link.')],
    ['showLastUpdated', t('Show last modified'), t('Show when the document was last changed.')],
  ];
  return (
    <div className="share-options">
      {options.map(([field, label, hint]) => (
        <label key={field} className="share-option">
          <span className="grow">
            <span className="share-option-label">{label}</span>
            <span className="muted small">{hint}</span>
          </span>
          <Switch checked={!!share[field]} onChange={(v) => set(field, v)} />
        </label>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Move modal
// ---------------------------------------------------------------------------

function flatten(nodes, depth, exclude, out) {
  for (const n of nodes) {
    if (n.id === exclude) continue;
    out.push({ ...n, depth });
    flatten(n.children, depth + 1, exclude, out);
  }
  return out;
}

function MoveModal({ doc, onClose, onMoved }) {
  const { collections, trees, team, teams, switchTeam } = useApp();
  const toast = useToast();
  const [filter, setFilter] = useState('');
  // target workspace: the current one, or another where the user can write
  const [teamId, setTeamId] = useState(team.id);
  const [other, setOther] = useState(null);
  const targetTeams = teams.filter((w) => w.id === team.id || w.role !== 'viewer');
  const elsewhere = teamId !== team.id;
  useEffect(() => {
    if (!elsewhere) {
      setOther(null);
      return;
    }
    let cancelled = false;
    setOther(undefined);
    (async () => {
      try {
        const cols = (await api('collections.listForWorkspace', { teamId })).filter((c) => c.access === 'write');
        const tr = {};
        await Promise.all(cols.map(async (c) => (tr[c.id] = await api('collections.documents', { id: c.id }))));
        if (!cancelled) setOther({ collections: cols, trees: tr });
      } catch (e) {
        toast(e.message, 'error');
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId]);
  const writable = elsewhere ? other?.collections || [] : collections.filter((c) => c.access === 'write');
  const targetTrees = elsewhere ? other?.trees || {} : trees;
  const targetName = targetTeams.find((w) => w.id === teamId)?.name;
  // moving to another workspace asks for confirmation first
  const [pending, setPending] = useState(null);
  const choose = (target, label) => (elsewhere ? setPending({ target, label }) : move(target));
  const move = async (target) => {
    try {
      const moved = await api('documents.move', { id: doc.id, ...target });
      onClose();
      if (elsewhere) {
        toast(t('Document moved to {workspace}', { workspace: targetName }));
        // follow the document to its new workspace
        await switchTeam(teamId, docUrl(moved));
      } else {
        toast(t('Document moved'));
        onMoved();
      }
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const f = filter.toLowerCase();
  return (
    <Modal title={t('Move “{title}”', { title: doc.title || t('Untitled') })} onClose={onClose} width={520}>
      {targetTeams.length > 1 && (
        <div className="move-workspace">
          <Field label={t('Workspace')}>
            <select value={teamId} onChange={(e) => setTeamId(e.target.value)}>
              {targetTeams.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.id === team.id ? t('{name} (current)', { name: w.name }) : w.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
      )}
      {elsewhere && (
        <div className="notice-bar info column move-notice">
          {t('The document and its nested documents, history, comments and attachments move to {workspace}. Its public links are revoked.', {
            workspace: targetName,
          })}
        </div>
      )}
      <input autoFocus placeholder={t('Filter…')} value={filter} onChange={(e) => setFilter(e.target.value)} />
      <div className="move-list">
        {elsewhere && other === undefined && <Spinner />}
        {elsewhere && other && writable.length === 0 && <div className="muted small">{t('No collection you can write to in this workspace.')}</div>}
        {writable.map((c) => {
          const items = flatten(targetTrees[c.id] || [], 1, doc.id, []).filter((n) => !f || (n.title || '').toLowerCase().includes(f));
          if (f && !c.name.toLowerCase().includes(f) && items.length === 0) return null;
          return (
            <div key={c.id}>
              <button className="move-item" onClick={() => choose({ collectionId: c.id }, c.name)}>
                <CollectionIcon collection={c} size={16} /> <strong>{c.name}</strong>
              </button>
              {items.map((n) => (
                <button
                  key={n.id}
                  className="move-item"
                  style={{ paddingLeft: 12 + n.depth * 18 }}
                  onClick={() => choose({ parentDocumentId: n.id }, `${c.name} / ${n.title || t('Untitled')}`)}
                >
                  {n.emoji ? <span>{n.emoji}</span> : <FileText size={15} className="muted" />} {n.title || t('Untitled')}
                </button>
              ))}
            </div>
          );
        })}
      </div>
      {pending && (
        <ConfirmModal
          title={t('Move to another workspace?')}
          message={t(
            '“{title}” and its nested documents will be moved to {workspace} ({target}), with history, comments and attachments. Only members of {workspace} will be able to see them, and their public links will be revoked.',
            { title: doc.title || t('Untitled'), workspace: targetName, target: pending.label }
          )}
          confirmLabel={t('Move')}
          onConfirm={() => move(pending.target)}
          onClose={() => setPending(null)}
        />
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// History panel
// ---------------------------------------------------------------------------

function HistoryPanel({ doc, onSelect, selectedId, onClose }) {
  const [revisions, setRevisions] = useState(null);
  useEffect(() => {
    api('revisions.list', { documentId: doc.id }).then(setRevisions);
  }, [doc.id, doc.updatedAt]);
  return (
    <aside className="side-panel">
      <div className="side-panel-header">
        <h3>{t('History')}</h3>
        <IconButton title={t('Close')} onClick={onClose}>
          <X size={18} />
        </IconButton>
      </div>
      <div className="side-panel-body">
        {!revisions && <Spinner />}
        {revisions?.length === 0 && <div className="muted small">{t('No history yet. Revisions are saved once the document is published.')}</div>}
        {revisions?.map((r, i) => (
          <button
            key={r.id}
            className={`revision-item ${selectedId === r.id ? 'active' : ''}`}
            onClick={() => onSelect(i === 0 ? null : r.id)}
          >
            <Avatar name={r.userName} color={r.userColor} size={28} />
            <div>
              <div className="revision-title">{i === 0 ? t('Current version') : formatDate(r.createdAt)}</div>
              <div className="muted small">
                {r.userName} · {timeAgo(r.createdAt)}
              </div>
            </div>
          </button>
        ))}
      </div>
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Comments panel
// ---------------------------------------------------------------------------

function CommentItem({ c, replies, onReply, reload, canResolve, active, located, onJump, threadRef }) {
  const { user, isAdmin } = useApp();
  const toast = useToast();
  const act = async (method) => {
    try {
      await api(method, { id: c.id });
      reload();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return (
    <div ref={threadRef} className={`comment-thread ${c.resolvedAt ? 'resolved' : ''} ${active ? 'active' : ''}`}>
      {c.anchorText && (
        <button
          type="button"
          className={`comment-quote ${located ? '' : 'detached'}`}
          onClick={located ? onJump : undefined}
          title={located ? t('Show in the document') : t('This text is no longer in the document')}
        >
          {c.anchorText}
        </button>
      )}
      {[c, ...replies].map((x, i) => (
        <div key={x.id} className={`comment ${i > 0 ? 'reply' : ''}`}>
          <Avatar name={x.createdByName} color={x.createdByColor} size={24} />
          <div className="comment-body">
            <div className="comment-head">
              <strong>{x.createdByName}</strong> <span className="muted small">{timeAgo(x.createdAt)}</span>
            </div>
            <div className="comment-text">{x.text}</div>
            {i > 0 && (x.createdBy === user.id || isAdmin) && (
              <button
                className="link-btn small"
                onClick={async () => {
                  await api('comments.delete', { id: x.id });
                  reload();
                }}
              >
                {t('Delete')}
              </button>
            )}
          </div>
        </div>
      ))}
      <div className="comment-actions">
        {!c.resolvedAt && (
          <button className="link-btn small" onClick={() => onReply(c)}>
            {t('Reply')}
          </button>
        )}
        {(canResolve || c.createdBy === user.id) && (
          <button className="link-btn small" onClick={() => act('comments.resolve')}>
            {c.resolvedAt ? t('Reopen') : t('Resolve')}
          </button>
        )}
        {(c.createdBy === user.id || isAdmin) && (
          <button className="link-btn small danger" onClick={() => act('comments.delete')}>
            {t('Delete')}
          </button>
        )}
      </div>
    </div>
  );
}

/** Comments of the document; `comments` and `load` come from the page, which also highlights their passages */
function CommentsPanel({ doc, comments, load, canResolve, onClose, editor, active, setActive, draftAnchor, clearDraft }) {
  const [text, setText] = useState('');
  const [replyTo, setReplyTo] = useState(null);
  const [showResolved, setShowResolved] = useState(false);
  const toast = useToast();
  const inputRef = useRef(null);
  const threads = useRef({});
  useEffect(() => {
    if (draftAnchor) {
      setReplyTo(null);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [draftAnchor]);
  useEffect(() => {
    if (active) threads.current[active]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [active, comments]);
  // which passages are still in the document
  const flat = editor ? flatText(editor.state.doc) : null;
  const locate = (c) => (flat && c.anchorText ? locateAnchor(flat, { text: c.anchorText, prefix: c.anchorPrefix, suffix: c.anchorSuffix }) : null);
  const jump = (c) => {
    const range = locate(c);
    if (!range || !editor) return;
    setActive(c.id);
    editor.commands.setTextSelection(range);
    const { node } = editor.view.domAtPos(range.from);
    (node.nodeType === 1 ? node : node.parentElement)?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  };
  const post = async () => {
    if (!text.trim()) return;
    try {
      const anchor = !replyTo && draftAnchor ? draftAnchor : undefined;
      const c = await api('comments.create', { documentId: doc.id, text, parentCommentId: replyTo?.id, anchor });
      setText('');
      setReplyTo(null);
      clearDraft();
      await load();
      setActive(replyTo?.id || c.id);
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const roots = (comments || []).filter((c) => !c.parentCommentId && (showResolved || !c.resolvedAt));
  const resolvedCount = (comments || []).filter((c) => !c.parentCommentId && c.resolvedAt).length;
  return (
    <aside className="side-panel">
      <div className="side-panel-header">
        <h3>{t('Comments')}</h3>
        <IconButton title={t('Close')} onClick={onClose}>
          <X size={18} />
        </IconButton>
      </div>
      <div className="side-panel-body">
        {!comments && <Spinner />}
        {comments && roots.length === 0 && <div className="muted small">{t('No comments yet.')}</div>}
        {roots.map((c) => (
          <CommentItem
            key={c.id}
            c={c}
            replies={comments.filter((r) => r.parentCommentId === c.id)}
            onReply={(c) => {
              setReplyTo(c);
              inputRef.current?.focus();
            }}
            reload={load}
            canResolve={canResolve}
            active={active === c.id}
            located={!!locate(c)}
            onJump={() => jump(c)}
            threadRef={(el) => (threads.current[c.id] = el)}
          />
        ))}
        {resolvedCount > 0 && (
          <button className="link-btn small" onClick={() => setShowResolved((s) => !s)}>
            {t(showResolved ? 'Hide {count} resolved' : 'Show {count} resolved', { count: resolvedCount })}
          </button>
        )}
      </div>
      <div className="side-panel-footer">
        {draftAnchor && !replyTo && (
          <div className="comment-draft-quote">
            <span className="comment-quote">{draftAnchor.text}</span>
            <button className="link-btn small" onClick={clearDraft}>
              {t('cancel')}
            </button>
          </div>
        )}
        {replyTo && (
          <div className="replying muted small">
            {t('Replying to {name}', { name: replyTo.createdByName })}{' '}
            <button className="link-btn small" onClick={() => setReplyTo(null)}>
              {t('cancel')}
            </button>
          </div>
        )}
        <textarea
          ref={inputRef}
          rows={3}
          placeholder={replyTo ? t('Reply…') : t('Add a comment…')}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) post();
          }}
        />
        <div className="row space-between">
          <span className="muted small">{t('Ctrl+Enter to post')}</span>
          <Button small onClick={post} disabled={!text.trim()}>
            {t('Post')}
          </Button>
        </div>
      </div>
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Document page
// ---------------------------------------------------------------------------

/** The documents that link to this one or mention it, as in Outline */
function Backlinks({ documentId, updatedAt }) {
  const [docs, setDocs] = useState([]);
  useEffect(() => {
    let cancelled = false;
    api('documents.backlinks', { id: documentId })
      .then((d) => !cancelled && setDocs(d))
      .catch(() => !cancelled && setDocs([]));
    return () => {
      cancelled = true;
    };
  }, [documentId, updatedAt]);
  if (!docs.length) return null;
  return (
    <div className="nested-docs backlinks">
      <h3>
        <Link2 size={15} /> {t('Referenced by')}
      </h3>
      <div className="backlink-list">
        {docs.map((d) => (
          <Link key={d.id} to={docUrl(d)} className="backlink">
            <DocIcon doc={d} size={16} />
            <span className="backlink-title">{d.title || t('Untitled')}</span>
            <span className="muted small nowrap">{timeAgo(d.updatedAt)}</span>
          </Link>
        ))}
      </div>
    </div>
  );
}

export default function DocumentPage() {
  const { id } = useParams();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const toast = useToast();
  const app = useApp();
  const {
    user,
    team,
    collections,
    templates,
    isAdmin,
    refreshTree,
    refreshAllTrees,
    refreshStarred,
    starredIds,
    toggleStar: toggleStarOf,
    refreshTemplates,
    setActiveDoc,
    createDoc,
  } = app;

  const [data, setData] = useState(null);
  const [error, setError] = useState(null);
  const [title, setTitle] = useState('');
  const [saveState, setSaveState] = useState('saved');
  const [conflict, setConflict] = useState(null);
  const [panel, setPanel] = useState(null);
  // comments, loaded with the document: their passages are highlighted in the text
  const [comments, setComments] = useState(null);
  const [activeComment, setActiveComment] = useState(null);
  const [draftAnchor, setDraftAnchor] = useState(null);
  const [editorInstance, setEditorInstance] = useState(null);
  // following the document: notified when it changes
  const [subscribed, setSubscribed] = useState(null);
  // where the document is pinned, and where it can be
  const [pins, setPins] = useState(null);
  const [revision, setRevision] = useState(null);
  const [editorKey, setEditorKey] = useState(0);
  const [moving, setMoving] = useState(false);
  const [confirm, setConfirm] = useState(null);
  // editor content shown as slides (presentation mode)
  const [presenting, setPresenting] = useState(null);

  const pending = useRef({});
  const timer = useRef(null);
  const baseUpdatedAt = useRef(null);
  const docIdRef = useRef(null);
  const editorRef = useRef(null);
  const commentsDocId = data?.document?.id;
  const followable = !!data?.document?.publishedAt && !data?.document?.template && !data?.document?.deletedAt;
  useEffect(() => {
    setSubscribed(null);
    if (!commentsDocId || !followable) return;
    api('subscriptions.info', { documentId: commentsDocId })
      .then((r) => setSubscribed(r.subscribed))
      .catch(() => setSubscribed(null));
  }, [commentsDocId, followable, data?.document?.updatedAt]);
  const pinnable = followable && !data?.document?.archivedAt;
  const loadPins = useCallback(() => {
    if (!commentsDocId || !pinnable) return setPins(null);
    api('pins.info', { documentId: commentsDocId }).then(setPins, () => setPins(null));
  }, [commentsDocId, pinnable]);
  useEffect(loadPins, [loadPins]);
  // ?comment=<id> (link of a notification): open that thread
  const commentParam = params.get('comment');
  useEffect(() => {
    if (!commentParam || !comments) return;
    const c = comments.find((x) => x.id === commentParam);
    if (c) {
      setActiveComment(c.parentCommentId || c.id);
      setPanel('comments');
    }
    const next = new URLSearchParams(params);
    next.delete('comment');
    setParams(next, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commentParam, comments]);
  const loadComments = useCallback(async () => {
    if (!commentsDocId) return;
    setComments(await api('comments.list', { documentId: commentsDocId }));
  }, [commentsDocId]);
  useEffect(() => {
    setComments(null);
    setActiveComment(null);
    setDraftAnchor(null);
    loadComments().catch(() => setComments([]));
  }, [loadComments]);
  useEffect(() => {
    if (!editorInstance || editorInstance.isDestroyed) return;
    const anchors = (comments || [])
      .filter((c) => !c.parentCommentId && !c.resolvedAt && c.anchorText)
      .map((c) => ({ id: c.id, text: c.anchorText, prefix: c.anchorPrefix, suffix: c.anchorSuffix }));
    if (draftAnchor) anchors.push({ id: 'draft', ...draftAnchor });
    editorInstance.storage.commentAnchors.onClick = (id) => {
      if (id === 'draft') return;
      setActiveComment(id);
      setPanel('comments');
    };
    editorInstance.commands.setCommentAnchors(anchors, draftAnchor ? 'draft' : activeComment);
  }, [editorInstance, comments, activeComment, draftAnchor]);
  const titleRef = useRef(null);
  const dataRef = useRef(null);
  dataRef.current = data;

  const doc = data?.document;

  const flush = useCallback(async () => {
    clearTimeout(timer.current);
    const fields = pending.current;
    const docId = docIdRef.current;
    if (!docId || !Object.keys(fields).length) return;
    pending.current = {};
    setSaveState('saving');
    try {
      const d = await api('documents.update', { id: docId, ...fields, baseUpdatedAt: baseUpdatedAt.current });
      if (docIdRef.current === docId) {
        baseUpdatedAt.current = d.updatedAt;
        setData((prev) => (prev && prev.document.id === d.id ? { ...prev, document: { ...d, text: prev.document.text } } : prev));
        setSaveState(Object.keys(pending.current).length ? 'dirty' : 'saved');
      }
      if (('title' in fields || 'emoji' in fields) && d.publishedAt) {
        refreshTree(d.collectionId);
        if (d.isStarred) refreshStarred();
        if (d.template) refreshTemplates();
      }
    } catch (e) {
      if (e.status === 409) {
        setConflict(e.message);
      } else {
        toast(t('Could not save: {error}', { error: e.message }), 'error');
      }
      pending.current = { ...fields, ...pending.current };
      setSaveState('error');
    }
  }, [refreshTree, refreshStarred, refreshTemplates, toast]);

  const queueSave = useCallback(
    (fields) => {
      pending.current = { ...pending.current, ...fields };
      setSaveState('dirty');
      clearTimeout(timer.current);
      timer.current = setTimeout(flush, 700);
    },
    [flush]
  );

  // flush pending changes when leaving the page
  useEffect(() => {
    const onBeforeUnload = (e) => {
      if (Object.keys(pending.current).length) {
        flush();
        e.preventDefault();
        e.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
  }, [flush]);

  const load = useCallback(async () => {
    try {
      const d = await api('documents.info', { id });
      if (d.document.teamId !== team.id) {
        // document belongs to another of my workspaces: switch to it
        await api('teams.switch', { id: d.document.teamId });
        window.location.reload();
        return;
      }
      docIdRef.current = d.document.id;
      baseUpdatedAt.current = d.document.updatedAt;
      // opening the document reads the notifications about it
      api('notifications.markRead', { documentId: d.document.id }).then(notificationsChanged, () => {});
      setData(d);
      setTitle(d.document.title);
      setError(null);
      setConflict(null);
      setSaveState('saved');
      setActiveDoc({ collectionId: d.document.collectionId, ids: [...d.path.map((p) => p.id), d.document.id] });
      document.title = `${d.document.title || t('Untitled')} - ${getProductName()}`;
    } catch (e) {
      setError(e);
    }
  }, [id, team.id, setActiveDoc]);

  useEffect(() => {
    setData(null);
    setRevision(null);
    setPanel(null);
    load();
    return () => {
      flush();
      setActiveDoc({ collectionId: null, ids: [] });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  // canonical URL (slug follows the title)
  useEffect(() => {
    if (!doc) return;
    const canonical = docUrl({ title: doc.title, urlId: doc.urlId });
    if (window.location.pathname !== canonical) {
      window.history.replaceState(window.history.state, '', canonical + window.location.search);
    }
  }, [doc?.title, doc?.urlId]);

  const isNew = params.get('new') === '1';
  useEffect(() => {
    if (isNew && data) {
      titleRef.current?.focus();
      params.delete('new');
      setParams(params, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isNew, data]);

  useEffect(() => {
    const el = titleRef.current;
    if (el) {
      el.style.height = 'auto';
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [title, data]);

  if (error) {
    return (
      <div className="centered-page">
        <h2>{error.status === 404 ? t('Not found') : t('Error')}</h2>
        <p className="muted">{error.message}</p>
        <Link to="/" className="btn btn-secondary">
          {t('Go home')}
        </Link>
      </div>
    );
  }
  if (!data)
    return (
      <div className="centered-page">
        <Spinner />
      </div>
    );

  const canWrite = data.access === 'write';
  const editable = canWrite && !doc.archivedAt && !doc.deletedAt && !revision;
  const isDraft = !doc.publishedAt;
  const collection = data.collection ? collections.find((c) => c.id === data.collection.id) || data.collection : null;
  const writableCollections = collections.filter((c) => c.access === 'write');
  const showTemplatePicker =
    editable && !doc.template && !doc.text.trim() && !title.trim() && templates.length > 0 && editorKey === 0;

  const setEmoji = (emoji) => {
    setData((d) => ({ ...d, document: { ...d.document, emoji: emoji || null } }));
    queueSave({ emoji: emoji || '' });
  };

  const publish = async (collectionId) => {
    await flush();
    try {
      const d = await api('documents.update', { id: doc.id, publish: true, collectionId });
      toast(t('Document published'));
      await refreshAllTrees();
      baseUpdatedAt.current = d.updatedAt;
      load();
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  // the same state as the sidebar, so starring or unstarring there shows here too
  const isStarred = starredIds.has(doc.id);
  const toggleStar = () => toggleStarOf({ documentId: doc.id }, !isStarred);

  // keeps the "Public" badge and the globe in the sidebar in step with the share panel
  const onShareChange = (share) => {
    setData((d) => ({ ...d, publicSince: share ? share.createdAt : null }));
    refreshTree(doc.collectionId);
  };

  const runAction = async (fn, message) => {
    try {
      await flush();
      await fn();
      if (message) toast(message);
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  const applyTemplate = (tpl) => {
    const newTitle = title.trim() ? title : replaceTemplateVariables(tpl.title, user);
    const text = replaceTemplateVariables(tpl.text, user);
    setTitle(newTitle);
    setData((d) => ({ ...d, document: { ...d.document, text, emoji: tpl.emoji || d.document.emoji } }));
    setEditorKey((k) => k + 1);
    queueSave({ title: newTitle, text, ...(tpl.emoji ? { emoji: tpl.emoji } : {}) });
  };

  const selectRevision = async (rid) => {
    if (!rid) {
      setRevision(null);
      return;
    }
    await flush();
    setRevision(await api('revisions.info', { id: rid }));
  };

  const restoreRevision = async () => {
    try {
      await api('documents.restore', { id: doc.id, revisionId: revision.id });
      toast(t('Document restored'));
      setRevision(null);
      setEditorKey((k) => k + 1);
      await load();
      refreshTree(doc.collectionId);
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  const menuItems = [
    subscribed !== null && {
      label: subscribed ? t('Unfollow') : t('Follow'),
      icon: subscribed ? <BellOff size={16} /> : <Bell size={16} />,
      onClick: () =>
        runAction(async () => {
          const r = await api(subscribed ? 'subscriptions.delete' : 'subscriptions.create', { documentId: doc.id });
          setSubscribed(r.subscribed);
        }, subscribed ? t('You no longer follow this document') : t('You will be notified when this document changes')),
    },
    pins?.canPinHome && {
      label: pins.home ? t('Unpin from home') : t('Pin to home'),
      icon: pins.home ? <PinOff size={16} /> : <Pin size={16} />,
      onClick: () =>
        runAction(async () => {
          await api(pins.home ? 'pins.delete' : 'pins.create', pins.home ? { id: pins.home } : { documentId: doc.id });
          loadPins();
        }, pins.home ? t('Unpinned from home') : t('Pinned to home')),
    },
    pins?.canPinCollection && {
      label: pins.collection ? t('Unpin from collection') : t('Pin to collection'),
      icon: pins.collection ? <PinOff size={16} /> : <Pin size={16} />,
      onClick: () =>
        runAction(async () => {
          await api(
            pins.collection ? 'pins.delete' : 'pins.create',
            pins.collection ? { id: pins.collection } : { documentId: doc.id, collectionId: doc.collectionId }
          );
          loadPins();
        }, pins.collection ? t('Unpinned from collection') : t('Pinned to collection')),
    },
    (subscribed !== null || pins?.canPinHome || pins?.canPinCollection) && { separator: true },
    canWrite && !isDraft && !doc.template && !doc.deletedAt && {
      label: t('New nested document'),
      icon: <Plus size={16} />,
      onClick: () => createDoc({ parentDocumentId: doc.id }),
    },
    canWrite && !doc.deletedAt && {
      label: t('Duplicate'),
      icon: <Copy size={16} />,
      onClick: () =>
        runAction(async () => {
          const d = await api('documents.duplicate', { id: doc.id });
          await refreshAllTrees();
          navigate(docUrl(d));
        }, t('Document duplicated')),
    },
    canWrite && !isDraft && !doc.template && !doc.deletedAt && !doc.archivedAt && {
      label: t('Move…'),
      icon: <FolderInput size={16} />,
      onClick: () => setMoving(true),
    },
    canWrite && !doc.template && !doc.deletedAt && {
      label: t('Create template…'),
      icon: <LayoutTemplate size={16} />,
      onClick: () =>
        runAction(async () => {
          const tpl = await api('documents.create', {
            title: doc.title,
            text: editorRef.current?.storage.markdown.getMarkdown() ?? doc.text,
            emoji: doc.emoji,
            template: true,
            publish: true,
          });
          await refreshTemplates();
          navigate(docUrl(tpl));
        }, t('Template created')),
    },
    { separator: true },
    {
      label: t('History'),
      icon: <History size={16} />,
      onClick: () => setPanel(panel === 'history' ? null : 'history'),
    },
    !revision && {
      label: t('Present'),
      icon: <Presentation size={16} />,
      onClick: () => editorRef.current && setPresenting(editorRef.current.getJSON()),
    },
    ...(app.instance.allowMarkdownExport !== false
      ? [
          { separator: true },
          { heading: t('Download as') },
          {
            label: 'Markdown',
            icon: <Download size={16} />,
            onClick: () =>
              runAction(async () => {
                const md = await api('documents.export', { id: doc.id });
                downloadText(`${slugify(doc.title)}.md`, md);
              }),
          },
          ...[
            { label: 'HTML', icon: <FileCode size={16} />, run: (x) => x.downloadHtml },
            { label: 'PDF', icon: <Printer size={16} />, run: (x) => x.printPdf },
            { label: t('Word (.docx)'), icon: <FileType size={16} />, run: (x) => x.downloadDocx },
          ].map((f) => ({
            label: f.label,
            icon: f.icon,
            onClick: () =>
              runAction(async () => {
                if (!editorRef.current) return;
                // the exporters (and the Word library) are loaded only when used
                const exporters = await import('../editor/exporters');
                await f.run(exporters)(editorRef.current, { title, emoji: doc.emoji });
              }),
          })),
          canWrite && !doc.deletedAt && { separator: true },
        ]
      : []),
    canWrite && !doc.deletedAt && {
      label: t('Full width'),
      icon: <Maximize2 size={16} />,
      checked: doc.fullWidth,
      onClick: () => {
        setData((d) => ({ ...d, document: { ...d.document, fullWidth: !d.document.fullWidth } }));
        queueSave({ fullWidth: !doc.fullWidth });
      },
    },
    canWrite && !doc.deletedAt && { separator: true },
    canWrite && !isDraft && !doc.template && !doc.deletedAt && (doc.createdBy === user.id || isAdmin) && data.children.length === 0 && {
      label: t('Unpublish'),
      icon: <Undo2 size={16} />,
      onClick: () =>
        runAction(async () => {
          await api('documents.unpublish', { id: doc.id });
          await refreshAllTrees();
          load();
        }, t('Document unpublished')),
    },
    canWrite && !isDraft && !doc.archivedAt && !doc.deletedAt && {
      label: t('Archive@@action'),
      icon: <Archive size={16} />,
      onClick: () =>
        runAction(async () => {
          await api('documents.archive', { id: doc.id });
          await refreshAllTrees();
          refreshTemplates();
          load();
        }, t('Document archived')),
    },
    canWrite && !doc.deletedAt && {
      label: t('Delete…'),
      icon: <Trash2 size={16} />,
      danger: true,
      onClick: () => setConfirm('delete'),
    },
  ];

  const shownTitle = revision ? revision.title : title;
  const shownEmoji = revision ? revision.emoji : doc.emoji;

  return (
    <div className={`document-layout ${panel ? 'with-panel' : ''}`}>
      <Scene
        centered={false}
        headerTitle={
          <span className="breadcrumb">
            {collection ? (
              <Link to={collectionUrl(collection)} className="crumb">
                <CollectionIcon collection={collection} size={16} />
                <span>{collection.name}</span>
              </Link>
            ) : doc.template ? (
              <Link to="/settings/templates" className="crumb">
                <LayoutTemplate size={16} /> <span>{t('Templates')}</span>
              </Link>
            ) : (
              <Link to="/drafts" className="crumb">
                <span>{t('Drafts')}</span>
              </Link>
            )}
            {data.path.map((p) => (
              <React.Fragment key={p.id}>
                <ChevronRight size={14} className="muted crumb-sep" />
                <Link to={docUrl(p)} className="crumb">
                  {p.emoji && <span>{p.emoji}</span>}
                  <span>{p.title || t('Untitled')}</span>
                </Link>
              </React.Fragment>
            ))}
            {data.publicSince && !doc.deletedAt && (
              <Popover
                align="left"
                width={380}
                trigger={
                  <span
                    className="public-badge"
                    title={t('Public: anyone with the link can read it (since {date})', { date: formatDate(data.publicSince) })}
                  >
                    <Globe size={13} />
                    {t('Public')}
                  </span>
                }
              >
                <SharePanel doc={doc} onChange={onShareChange} />
              </Popover>
            )}
          </span>
        }
        actions={
          <>
            {canWrite && (
              <span className={`save-state ${saveState}`}>
                {saveState === 'saving' ? t('Saving…') : saveState === 'dirty' ? t('Editing') : saveState === 'error' ? t('Not saved') : t('Saved')}
              </span>
            )}
            {isDraft && !doc.template && canWrite && !doc.deletedAt && (
              doc.collectionId ? (
                <Button onClick={() => publish()}>{t('Publish')}</Button>
              ) : (
                <Menu
                  align="right"
                  width={240}
                  trigger={<Button>{t('Publish')}</Button>}
                  items={[
                    { heading: t('Publish in…') },
                    ...writableCollections.map((c) => ({
                      label: c.name,
                      icon: <CollectionIcon collection={c} size={16} />,
                      onClick: () => publish(c.id),
                    })),
                  ]}
                />
              )
            )}
            {!doc.template && !doc.deletedAt && (
              <Popover trigger={<Button variant="secondary">{t('Share')}</Button>} width={420}>
                <SharePanel doc={doc} onChange={onShareChange} />
              </Popover>
            )}
            {!isDraft && !doc.deletedAt && (
              <IconButton title={isStarred ? t('Unstar') : t('Star')} onClick={toggleStar} active={isStarred}>
                <Star size={18} fill={isStarred ? 'currentColor' : 'none'} />
              </IconButton>
            )}
            <IconButton title={t('Comments')} active={panel === 'comments'} onClick={() => setPanel(panel === 'comments' ? null : 'comments')}>
              <MessageSquare size={18} />
            </IconButton>
            <IconButton title={t('History')} active={panel === 'history'} onClick={() => setPanel(panel === 'history' ? null : 'history')}>
              <History size={18} />
            </IconButton>
            <Menu
              align="right"
              trigger={
                <IconButton title={t('More')}>
                  <MoreHorizontal size={18} />
                </IconButton>
              }
              items={menuItems}
            />
          </>
        }
      >
        {conflict && (
          <div className="notice-bar warning">
            {conflict}
            <Button
              small
              variant="secondary"
              onClick={() => {
                pending.current = {};
                setEditorKey((k) => k + 1);
                load();
              }}
            >
              {t('Reload')}
            </Button>
          </div>
        )}
        {doc.deletedAt && (
          <div className="notice-bar">
            {t('This document was deleted {time}. It will be permanently removed after 30 days.', { time: timeAgo(doc.deletedAt) })}
            {canWrite && (
              <span className="row gap">
                <Button
                  small
                  variant="secondary"
                  icon={<RotateCcw size={14} />}
                  onClick={() =>
                    runAction(async () => {
                      const params = { id: doc.id };
                      if (doc.publishedAt && !collections.some((c) => c.id === doc.collectionId)) {
                        params.collectionId = writableCollections[0]?.id;
                      }
                      await api('documents.restore', params);
                      await refreshAllTrees();
                      load();
                    }, t('Document restored'))
                  }
                >
                  {t('Restore')}
                </Button>
                <Button small variant="danger" onClick={() => setConfirm('permanent')}>
                  {t('Delete permanently')}
                </Button>
              </span>
            )}
          </div>
        )}
        {doc.archivedAt && !doc.deletedAt && (
          <div className="notice-bar">
            {t('This document was archived {time}.', { time: timeAgo(doc.archivedAt) })}
            {canWrite && (
              <Button
                small
                variant="secondary"
                icon={<RotateCcw size={14} />}
                onClick={() =>
                  runAction(async () => {
                    await api('documents.restore', { id: doc.id });
                    await refreshAllTrees();
                    load();
                  }, t('Document restored'))
                }
              >
                {t('Restore')}
              </Button>
            )}
          </div>
        )}
        {revision && (
          <div className="notice-bar">
            {t('Viewing revision from {date} by {name}.', { date: formatDate(revision.createdAt), name: revision.userName })}
            <span className="row gap">
              {canWrite && !doc.deletedAt && (
                <Button small icon={<RotateCcw size={14} />} onClick={restoreRevision}>
                  {t('Restore this version')}
                </Button>
              )}
              <Button small variant="secondary" onClick={() => setRevision(null)}>
                {t('Close')}
              </Button>
            </span>
          </div>
        )}
        {doc.template && !revision && (
          <div className="notice-bar info">
            <LayoutTemplate size={16} />
            <span className="notice-bar-text">
              {t('You’re editing a template. It will be available to everyone when creating a new document.')}{' '}
              {t('Select some text and use the placeholder button to add placeholders that can be filled out when creating new documents. {date}, {time}, {datetime} and {author} are replaced when the template is used.')}
            </span>
          </div>
        )}

        <article className={`document ${doc.fullWidth ? 'full-width' : ''}`}>
          <div className="document-header">
            {editable ? (
              <Popover
                align="left"
                width={330}
                trigger={
                  <button className={`emoji-trigger ${shownEmoji ? 'has-emoji' : ''}`} title={t('Choose an icon')}>
                    {shownEmoji || <Smile size={22} />}
                  </button>
                }
              >
                {(close) => (
                  <EmojiPicker
                    onSelect={(e) => {
                      setEmoji(e);
                      close();
                    }}
                    onRemove={
                      doc.emoji
                        ? () => {
                            setEmoji(null);
                            close();
                          }
                        : undefined
                    }
                  />
                )}
              </Popover>
            ) : (
              shownEmoji && <span className="emoji-static">{shownEmoji}</span>
            )}
            <textarea
              ref={titleRef}
              className="document-title"
              rows={1}
              value={shownTitle}
              readOnly={!editable}
              placeholder={doc.template ? t('Start your template…') : t('Untitled')}
              onChange={(e) => {
                const v = e.target.value.replace(/\n/g, '');
                setTitle(v);
                queueSave({ title: v });
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || (e.key === 'ArrowDown' && e.target.selectionStart === e.target.value.length)) {
                  e.preventDefault();
                  const ed = editorRef.current;
                  if (ed) {
                    ed.view.focus();
                    ed.commands.focus('start');
                  }
                }
              }}
            />
          </div>
          <div className="document-meta">
            {isDraft && !doc.template && <span className="badge">{t('Draft')}</span>}
            <span>
              {doc.updatedBy === user.id
                ? t(isDraft ? 'You saved {time}' : 'You updated {time}', { time: timeAgo(doc.updatedAt) })
                : t(isDraft ? '{name} saved {time}' : '{name} updated {time}', { name: doc.updatedByName, time: timeAgo(doc.updatedAt) })}
            </span>
            {collection && (
              <>
                <span>·</span>
                <span>
                  {t('in')} <Link to={collectionUrl(collection)}>{collection.name}</Link>
                </span>
              </>
            )}
          </div>

          {showTemplatePicker && (
            <div className="template-picker">
              <Menu
                width={260}
                trigger={
                  <Button variant="secondary" small icon={<LayoutTemplate size={14} />}>
                    {t('Use a template')}
                  </Button>
                }
                items={templates.map((tpl) => ({
                  label: replaceTemplateVariables(tpl.title, user) || t('Untitled'),
                  icon: <DocIcon doc={tpl} size={16} />,
                  onClick: () => applyTemplate(tpl),
                }))}
              />
            </div>
          )}

          {revision ? (
            <Editor key={`rev-${revision.id}`} value={revision.text} editable={false} documentId={doc.id} />
          ) : (
            <Editor
              key={`${doc.id}-${editorKey}`}
              value={doc.text}
              editable={editable}
              template={doc.template}
              documentId={doc.id}
              onReady={(ed) => {
                editorRef.current = ed;
                setEditorInstance(ed);
              }}
              onComment={
                doc.template || doc.deletedAt
                  ? undefined
                  : (anchor) => {
                      setDraftAnchor(anchor);
                      setActiveComment(null);
                      setPanel('comments');
                    }
              }
              onChange={(md) => queueSave({ text: md })}
            />
          )}

          {!revision && doc.publishedAt && !doc.template && <Backlinks documentId={doc.id} updatedAt={doc.updatedAt} />}

          {data.children.length > 0 && !revision && (
            <div className="nested-docs">
              <h3>{t('Nested documents')}</h3>
              <div className="nested-grid">
                {data.children.map((c) => (
                  <Link key={c.id} to={docUrl(c)} className="nested-card">
                    <DocIcon doc={c} size={16} />
                    <span>{c.title || t('Untitled')}</span>
                    <CornerDownRight size={14} className="muted" />
                  </Link>
                ))}
              </div>
            </div>
          )}
        </article>
      </Scene>

      {panel === 'history' && (
        <HistoryPanel doc={doc} selectedId={revision?.id} onSelect={selectRevision} onClose={() => { setPanel(null); setRevision(null); }} />
      )}
      {panel === 'comments' && (
        <CommentsPanel
          doc={doc}
          comments={comments}
          load={loadComments}
          canResolve={canWrite}
          editor={editorInstance}
          active={activeComment}
          setActive={setActiveComment}
          draftAnchor={draftAnchor}
          clearDraft={() => setDraftAnchor(null)}
          onClose={() => {
            setPanel(null);
            setDraftAnchor(null);
            setActiveComment(null);
          }}
        />
      )}

      {presenting && (
        <PresentationMode title={title} emoji={doc.emoji} content={presenting} onClose={() => setPresenting(null)} />
      )}
      {moving && <MoveModal doc={doc} onClose={() => setMoving(false)} onMoved={async () => { await refreshAllTrees(); load(); }} />}
      {confirm === 'delete' && (
        <ConfirmModal
          title={t('Delete document')}
          danger
          confirmLabel={t('I’m sure – Delete')}
          message={
            data.children.length
              ? t('Are you sure about that? Deleting “{title}” will also delete all of its nested documents.', { title: doc.title || t('Untitled') })
              : t('Are you sure about that? Deleting “{title}” will move it to the trash.', { title: doc.title || t('Untitled') })
          }
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            pending.current = {};
            await api('documents.delete', { id: doc.id });
            await refreshAllTrees();
            refreshStarred();
            refreshTemplates();
            toast(t('Document moved to trash'));
            navigate(collection ? collectionUrl(collection) : '/');
          }}
        />
      )}
      {confirm === 'permanent' && (
        <ConfirmModal
          title={t('Permanently delete')}
          danger
          confirmLabel={t('Delete forever')}
          message={t('This document and its nested documents will be permanently deleted. This cannot be undone.')}
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            await api('documents.delete', { id: doc.id, permanent: true });
            toast(t('Document permanently deleted'));
            navigate('/trash');
          }}
        />
      )}
    </div>
  );
}
