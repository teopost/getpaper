import React, { useEffect, useMemo, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { api, timeAgo } from '../api';
import Editor from '../editor/Editor';
import { Spinner } from '../components/ui';
import { Logo } from './Login';
import { t } from '../i18n';
import { getProductName } from '../prefs';

/** "/doc/some-title-AbC123defg" → "AbC123defg" */
const urlIdOf = (href) => /^\/doc\/(?:.*-)?([A-Za-z0-9]{10})(?:[#?].*)?$/.exec(href || '')?.[1];

function addMeta(name, content) {
  const m = document.createElement('meta');
  m.name = name;
  m.content = content;
  document.head.appendChild(m);
  return m;
}

/** The documents of the link, nested as in the workspace */
function SharedTree({ nodes, parentId, shareId, currentId, depth = 0 }) {
  const children = nodes.filter((n) => (n.parentDocumentId || null) === parentId);
  if (!children.length) return null;
  return (
    <ul className="shared-tree-list">
      {children.map((n) => {
        const root = depth === 0 && !parentId;
        const to = root ? `/s/${shareId}` : `/s/${shareId}/doc/${n.urlId}`;
        return (
          <li key={n.id}>
            <Link to={to} className={`shared-tree-link ${n.id === currentId ? 'active' : ''}`} style={{ paddingLeft: 8 + depth * 14 }}>
              {n.emoji && <span className="shared-tree-emoji">{n.emoji}</span>}
              <span className="shared-tree-title">{n.title || t('Untitled')}</span>
            </Link>
            <SharedTree nodes={nodes} parentId={n.id} shareId={shareId} currentId={currentId} depth={depth + 1} />
          </li>
        );
      })}
    </ul>
  );
}

export default function SharedDoc() {
  const { shareId, docId } = useParams();
  const navigate = useNavigate();
  const [data, setData] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    // the address of the link must not leak through the Referer header
    const m = addMeta('referrer', 'no-referrer');
    return () => m.remove();
  }, []);

  useEffect(() => {
    setError(null);
    api('documents.info', { shareId, id: docId })
      .then((d) => {
        setData(d);
        document.title = d.document.title || t('Untitled');
        window.scrollTo(0, 0);
      })
      .catch((e) => setError(e.message));
  }, [shareId, docId]);

  // public links stay out of search engines unless the link allows it
  const indexable = !!data?.share?.allowIndexing;
  useEffect(() => {
    if (!data || indexable) return undefined;
    const m = addMeta('robots', 'noindex, nofollow');
    return () => m.remove();
  }, [data, indexable]);

  const tree = useMemo(() => data?.tree || [], [data]);
  const inTree = useMemo(() => new Map(tree.map((n) => [n.urlId, n])), [tree]);

  if (error)
    return (
      <div className="centered-page">
        <h2>{t('Not found')}</h2>
        <p className="muted">{error}</p>
      </div>
    );
  if (!data)
    return (
      <div className="centered-page">
        <Spinner />
      </div>
    );
  const doc = data.document;
  const share = data.share;

  // links to documents of the link stay inside it
  const onClick = (e) => {
    const a = e.target.closest?.('a[href]');
    const id = a && urlIdOf(a.getAttribute('href'));
    const node = id && inTree.get(id);
    if (!node || e.metaKey || e.ctrlKey || e.shiftKey) return;
    // before the editor, which would open the workspace page of the document
    e.preventDefault();
    e.stopPropagation();
    navigate(node.id === share.rootId ? `/s/${shareId}` : `/s/${shareId}/doc/${node.urlId}`);
  };

  return (
    <div className="shared-page">
      <header className="shared-header">
        <span className="shared-brand">
          <Logo size={22} /> {data.team.name}
        </span>
      </header>
      <div className={`shared-body ${tree.length > 1 ? 'with-tree' : ''}`}>
        {tree.length > 1 && (
          <nav className="shared-tree" aria-label={t('Documents')}>
            <SharedTree nodes={tree} parentId={null} shareId={shareId} currentId={doc.id} />
          </nav>
        )}
        <article className={`document ${doc.fullWidth ? 'full-width' : ''}`} onClickCapture={onClick}>
          <h1 className="document-title-static">
            {doc.emoji && <span className="title-emoji">{doc.emoji}</span>}
            {doc.title || t('Untitled')}
          </h1>
          {share.showLastUpdated && doc.updatedAt && (
            <div className="document-meta">{t('Last updated {time}', { time: timeAgo(doc.updatedAt) })}</div>
          )}
          <Editor key={doc.id} value={doc.text} editable={false} documentId={doc.id} />
        </article>
      </div>
      <footer className="shared-footer">
        {t('Powered by')} <strong>{getProductName()}</strong>
      </footer>
    </div>
  );
}
