import React from 'react';
import { Link } from 'react-router-dom';
import { Star } from 'lucide-react';
import { api, docUrl, timeAgo } from '../api';
import { useApp } from '../store';
import { CollectionIcon, DocIcon } from './icons';
import { Empty } from './ui';

export function docMeta(doc, user, collections, { showCollection = true } = {}) {
  const who = doc.updatedBy === user.id ? 'You' : doc.updatedByName;
  let verb;
  if (doc.deletedAt) verb = `deleted ${timeAgo(doc.deletedAt)}`;
  else if (doc.archivedAt) verb = `archived ${timeAgo(doc.archivedAt)}`;
  else if (!doc.publishedAt) verb = `saved ${timeAgo(doc.updatedAt)}`;
  else if (doc.createdAt === doc.updatedAt) verb = `created ${timeAgo(doc.createdAt)}`;
  else verb = `updated ${timeAgo(doc.updatedAt)}`;
  const collection = showCollection && collections.find((c) => c.id === doc.collectionId);
  return { text: `${who} ${verb}`, collection };
}

export function DocumentListItem({ doc, actions, context, showCollection = true, onStarChange }) {
  const { user, collections, refreshStarred } = useApp();
  const meta = docMeta(doc, user, collections, { showCollection });
  const toggleStar = async (e) => {
    e.preventDefault();
    e.stopPropagation();
    await api(doc.isStarred ? 'stars.delete' : 'stars.create', { documentId: doc.id });
    refreshStarred();
    onStarChange?.();
  };
  return (
    <Link to={docUrl(doc)} className="doc-list-item">
      <div className="doc-list-main">
        <div className="doc-list-title">
          <DocIcon doc={doc} size={18} />
          <span className="title-text">{doc.title || 'Untitled'}</span>
          {!doc.publishedAt && !doc.template && <span className="badge">Draft</span>}
          {doc.template && <span className="badge">Template</span>}
          {!doc.deletedAt && doc.publishedAt && (
            <button className={`star-btn ${doc.isStarred ? 'starred' : ''}`} onClick={toggleStar} title="Star">
              <Star size={15} fill={doc.isStarred ? 'currentColor' : 'none'} />
            </button>
          )}
        </div>
        {context && (
          <div className="doc-list-context" dangerouslySetInnerHTML={{ __html: context }} />
        )}
        <div className="doc-list-meta">
          {meta.text}
          {meta.collection && (
            <>
              {' · in '}
              <span className="meta-collection">
                <CollectionIcon collection={meta.collection} size={12} /> {meta.collection.name}
              </span>
            </>
          )}
        </div>
      </div>
      {actions && (
        <div className="doc-list-actions" onClick={(e) => e.preventDefault()}>
          {actions}
        </div>
      )}
    </Link>
  );
}

export default function DocumentList({ docs, empty = 'No documents yet', renderActions, showCollection, onStarChange }) {
  if (!docs) return null;
  if (docs.length === 0) return <Empty>{empty}</Empty>;
  return (
    <div className="doc-list">
      {docs.map((d) => (
        <DocumentListItem
          key={d.id}
          doc={d}
          actions={renderActions?.(d)}
          showCollection={showCollection}
          onStarChange={onStarChange}
        />
      ))}
    </div>
  );
}
