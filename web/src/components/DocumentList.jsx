import React from 'react';
import { Link } from 'react-router-dom';
import { Star } from 'lucide-react';
import { docUrl, timeAgo } from '../api';
import { useApp } from '../store';
import { CollectionIcon, DocIcon } from './icons';
import { Empty, TeamAvatar } from './ui';
import { t } from '../i18n';

export function docMeta(doc, user, collections, { showCollection = true } = {}) {
  const you = doc.updatedBy === user.id;
  let action, date;
  if (doc.deletedAt) [action, date] = ['deleted', doc.deletedAt];
  else if (doc.archivedAt) [action, date] = ['archived', doc.archivedAt];
  else if (!doc.publishedAt) [action, date] = ['saved', doc.updatedAt];
  else if (doc.createdAt === doc.updatedAt) [action, date] = ['created', doc.createdAt];
  else [action, date] = ['updated', doc.updatedAt];
  const vars = { name: doc.updatedByName, time: timeAgo(date) };
  const text = {
    deleted: you ? t('You deleted {time}', vars) : t('{name} deleted {time}', vars),
    archived: you ? t('You archived {time}', vars) : t('{name} archived {time}', vars),
    saved: you ? t('You saved {time}', vars) : t('{name} saved {time}', vars),
    created: you ? t('You created {time}', vars) : t('{name} created {time}', vars),
    updated: you ? t('You updated {time}', vars) : t('{name} updated {time}', vars),
  }[action];
  const collection = showCollection && collections.find((c) => c.id === doc.collectionId);
  return { text, collection };
}

/**
 * `workspace` is set for search results from another workspace: the item shows the workspace and
 * collection name and switches workspace before opening the document.
 */
export function DocumentListItem({ doc, actions, context, showCollection = true, onStarChange, workspace, collectionName }) {
  const { user, collections, starredIds, toggleStar: toggleStarOf, switchTeam } = useApp();
  const meta = docMeta(doc, user, collections, { showCollection: showCollection && !workspace });
  // documents of another workspace are never starred here
  const isStarred = !workspace && starredIds.has(doc.id);
  const toggleStar = async (e) => {
    e.preventDefault();
    e.stopPropagation();
    await toggleStarOf({ documentId: doc.id }, !isStarred);
    onStarChange?.();
  };
  return (
    <Link
      to={docUrl(doc)}
      className="doc-list-item"
      onClick={
        workspace &&
        ((e) => {
          e.preventDefault();
          switchTeam(workspace.id, docUrl(doc));
        })
      }
    >
      <div className="doc-list-main">
        <div className="doc-list-title">
          <DocIcon doc={doc} size={18} />
          <span className="title-text">{doc.title || t('Untitled')}</span>
          {!doc.publishedAt && !doc.template && <span className="badge">{t('Draft')}</span>}
          {doc.template && <span className="badge">{t('Template')}</span>}
          {!doc.deletedAt && doc.publishedAt && !workspace && (
            <button className={`star-btn ${isStarred ? 'starred' : ''}`} onClick={toggleStar} title={isStarred ? t('Unstar') : t('Star')}>
              <Star size={15} fill={isStarred ? 'currentColor' : 'none'} />
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
              {` · ${t('in')} `}
              <span className="meta-collection">
                <CollectionIcon collection={meta.collection} size={12} /> {meta.collection.name}
              </span>
            </>
          )}
          {workspace && (
            <>
              {` · ${t('in')} `}
              <span className="meta-collection">
                <TeamAvatar team={workspace} size={14} /> {workspace.name}
                {collectionName && ` / ${collectionName}`}
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

export default function DocumentList({ docs, empty = t('No documents yet'), renderActions, showCollection, onStarChange }) {
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
