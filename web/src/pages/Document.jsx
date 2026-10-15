import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import {
  Archive,
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
import Scene from '../components/Scene';
import { CollectionIcon, DocIcon } from '../components/icons';
import { Avatar, Button, ConfirmModal, IconButton, Menu, Modal, Popover, Spinner, Switch } from '../components/ui';
import EmojiPicker from '../components/EmojiPicker';
import { useToast } from '../components/Toast';
import { t } from '../i18n';

// ---------------------------------------------------------------------------
// Share popover
// ---------------------------------------------------------------------------

function SharePanel({ doc }) {
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
      if (on) setShare(await api('shares.create', { documentId: doc.id }));
      else {
        await api('shares.revoke', { id: share.id });
        setShare(null);
      }
    } catch (e) {
      toast(e.message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="share-panel">
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
  const { collections, trees } = useApp();
  const toast = useToast();
  const [filter, setFilter] = useState('');
  const writable = collections.filter((c) => c.access === 'write');
  const move = async (target) => {
    try {
      await api('documents.move', { id: doc.id, ...target });
      toast(t('Document moved'));
      onMoved();
      onClose();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const f = filter.toLowerCase();
  return (
    <Modal title={t('Move “{title}”', { title: doc.title || t('Untitled') })} onClose={onClose} width={520}>
      <input autoFocus placeholder={t('Filter…')} value={filter} onChange={(e) => setFilter(e.target.value)} />
      <div className="move-list">
        {writable.map((c) => {
          const items = flatten(trees[c.id] || [], 1, doc.id, []).filter((n) => !f || (n.title || '').toLowerCase().includes(f));
          if (f && !c.name.toLowerCase().includes(f) && items.length === 0) return null;
          return (
            <div key={c.id}>
              <button className="move-item" onClick={() => move({ collectionId: c.id })}>
                <CollectionIcon collection={c} size={16} /> <strong>{c.name}</strong>
              </button>
              {items.map((n) => (
                <button
                  key={n.id}
                  className="move-item"
                  style={{ paddingLeft: 12 + n.depth * 18 }}
                  onClick={() => move({ parentDocumentId: n.id })}
                >
                  {n.emoji ? <span>{n.emoji}</span> : <FileText size={15} className="muted" />} {n.title || t('Untitled')}
                </button>
              ))}
            </div>
          );
        })}
      </div>
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

function CommentItem({ c, replies, onReply, reload, canResolve }) {
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
    <div className={`comment-thread ${c.resolvedAt ? 'resolved' : ''}`}>
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

function CommentsPanel({ doc, canResolve, onClose }) {
  const [comments, setComments] = useState(null);
  const [text, setText] = useState('');
  const [replyTo, setReplyTo] = useState(null);
  const [showResolved, setShowResolved] = useState(false);
  const toast = useToast();
  const inputRef = useRef(null);
  const load = useCallback(() => api('comments.list', { documentId: doc.id }).then(setComments), [doc.id]);
  useEffect(() => {
    load();
  }, [load]);
  const post = async () => {
    if (!text.trim()) return;
    try {
      await api('comments.create', { documentId: doc.id, text, parentCommentId: replyTo?.id });
      setText('');
      setReplyTo(null);
      load();
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
          />
        ))}
        {resolvedCount > 0 && (
          <button className="link-btn small" onClick={() => setShowResolved((s) => !s)}>
            {t(showResolved ? 'Hide {count} resolved' : 'Show {count} resolved', { count: resolvedCount })}
          </button>
        )}
      </div>
      <div className="side-panel-footer">
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
  const [revision, setRevision] = useState(null);
  const [editorKey, setEditorKey] = useState(0);
  const [moving, setMoving] = useState(false);
  const [confirm, setConfirm] = useState(null);

  const pending = useRef({});
  const timer = useRef(null);
  const baseUpdatedAt = useRef(null);
  const docIdRef = useRef(null);
  const editorRef = useRef(null);
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
      setData(d);
      setTitle(d.document.title);
      setError(null);
      setConflict(null);
      setSaveState('saved');
      setActiveDoc({ collectionId: d.document.collectionId, ids: [...d.path.map((p) => p.id), d.document.id] });
      document.title = `${d.document.title || t('Untitled')} - GetPaper`;
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

  const toggleStar = async () => {
    await api(doc.isStarred ? 'stars.delete' : 'stars.create', { documentId: doc.id });
    setData((d) => ({ ...d, document: { ...d.document, isStarred: !d.document.isStarred } }));
    refreshStarred();
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
    const newTitle = title.trim() ? title : tpl.title;
    setTitle(newTitle);
    setData((d) => ({ ...d, document: { ...d.document, text: tpl.text, emoji: tpl.emoji || d.document.emoji } }));
    setEditorKey((k) => k + 1);
    queueSave({ title: newTitle, text: tpl.text, ...(tpl.emoji ? { emoji: tpl.emoji } : {}) });
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
    app.instance.allowMarkdownExport !== false && {
      label: t('Download'),
      icon: <Download size={16} />,
      onClick: () =>
        runAction(async () => {
          const md = await api('documents.export', { id: doc.id });
          downloadText(`${slugify(doc.title)}.md`, md);
        }),
    },
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
              <Link to="/templates" className="crumb">
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
              <Popover trigger={<Button variant="secondary">{t('Share')}</Button>} width={380}>
                <SharePanel doc={doc} />
              </Popover>
            )}
            {!isDraft && !doc.deletedAt && (
              <IconButton title={doc.isStarred ? t('Unstar') : t('Star')} onClick={toggleStar} active={doc.isStarred}>
                <Star size={18} fill={doc.isStarred ? 'currentColor' : 'none'} />
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
            <LayoutTemplate size={16} /> {t('You’re editing a template. It will be available to everyone when creating a new document.')}
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
                  label: tpl.title || t('Untitled'),
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
              documentId={doc.id}
              onReady={(ed) => (editorRef.current = ed)}
              onChange={(md) => queueSave({ text: md })}
            />
          )}

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
      {panel === 'comments' && <CommentsPanel doc={doc} canResolve={canWrite} onClose={() => setPanel(null)} />}

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
