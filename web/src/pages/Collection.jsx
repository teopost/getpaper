import React, { useEffect, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { Bell, BellOff, Download, MoreHorizontal, Pencil, Plus, Star, Trash2, Upload, Search as SearchIcon, Lock } from 'lucide-react';
import { api, collectionUrl, docUrl, downloadText, slugify, upload } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import PinnedDocs from '../components/PinnedDocs';
import { CollectionIcon } from '../components/icons';
import { Button, IconButton, Menu, Spinner, Tabs, Empty, ConfirmModal } from '../components/ui';
import CollectionModal from '../components/CollectionModal';
import { useToast } from '../components/Toast';
import { t } from '../i18n';
import Editor from '../editor/Editor';

export default function CollectionPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const toast = useToast();
  const { createDoc, refreshCollections, refreshAllTrees, refreshTree, isAdmin, user, collections, starredIds, toggleStar } = useApp();
  const [collection, setCollection] = useState(null);
  const [error, setError] = useState(null);
  const [tab, setTab] = useState('updated');
  const [docs, setDocs] = useState(null);
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // following the collection: notified of documents published in it
  const [subscribed, setSubscribed] = useState(null);
  const descTimer = useRef(null);

  const load = () =>
    api('collections.info', { id })
      .then((c) => {
        setCollection(c);
        setError(null);
      })
      .catch((e) => setError(e));

  useEffect(() => {
    setCollection(null);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  useEffect(() => {
    setSubscribed(null);
    api('subscriptions.info', { collectionId: id })
      .then((r) => setSubscribed(r.subscribed))
      .catch(() => setSubscribed(null));
  }, [id]);

  // keep in sync with edits made via the sidebar
  useEffect(() => {
    const c = collections.find((x) => x.id === collection?.id);
    if (c && collection && (c.name !== collection.name || c.color !== collection.color || c.icon !== collection.icon || c.permission !== collection.permission)) {
      setCollection((prev) => ({ ...prev, ...c }));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collections]);

  useEffect(() => {
    if (!collection) return;
    setDocs(null);
    const params =
      tab === 'updated'
        ? { sort: 'updatedAt' }
        : tab === 'published'
        ? { sort: 'publishedAt' }
        : tab === 'old'
        ? { sort: 'updatedAt', direction: 'ASC' }
        : { sort: 'title', direction: 'ASC' };
    api('documents.list', { collectionId: collection.id, limit: 100, ...params }).then(setDocs);
  }, [collection?.id, tab]);

  if (error) return <div className="centered-page">{error.message}</div>;
  if (!collection)
    return (
      <div className="centered-page">
        <Spinner />
      </div>
    );

  const canWrite = collection.access === 'write';
  const canDelete = canWrite && (isAdmin || collection.createdBy === user.id);

  const saveDescription = (md) => {
    clearTimeout(descTimer.current);
    descTimer.current = setTimeout(() => {
      api('collections.update', { id: collection.id, description: md }).catch((e) => toast(e.message, 'error'));
    }, 800);
  };

  const importFile = () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.zip,.tar.gz,.tgz,.md,.markdown,.txt,.html,.htm,text/markdown,text/plain';
    input.multiple = true;
    input.onchange = async () => {
      let last;
      let total = 0;
      for (const file of input.files) {
        try {
          if (/\.(zip|tar\.gz|tgz)$/i.test(file.name)) {
            const r = await upload('collections.import', { file, collectionId: collection.id, publish: 'true' });
            total += r.documents;
          } else {
            last = await upload('documents.import', { file, collectionId: collection.id, publish: 'true' });
            total += 1;
          }
        } catch (e) {
          toast(`${file.name}: ${e.message}`, 'error');
        }
      }
      await refreshTree(collection.id);
      if (total) toast(t(total === 1 ? 'Imported {count} document' : 'Imported {count} documents', { count: total }));
      if (last && total === 1) navigate(docUrl(last));
      else load();
    };
    input.click();
  };

  const exportCollection = async () => {
    const res = await api('collections.export', { id: collection.id });
    downloadText(`${slugify(res.name)}.md`, res.markdown);
  };

  return (
    <Scene
      title={collection.name}
      headerTitle={
        <span className="breadcrumb">
          <CollectionIcon collection={collection} size={16} />
          <span>{collection.name}</span>
          {!collection.permission && <Lock size={13} className="muted" title={t('Private')} />}
        </span>
      }
      actions={
        <>
          <IconButton title={t('Search in collection')} onClick={() => navigate(`/search?collectionId=${collection.id}`)}>
            <SearchIcon size={18} />
          </IconButton>
          <IconButton
            title={starredIds.has(collection.id) ? t('Unstar') : t('Star')}
            active={starredIds.has(collection.id)}
            onClick={() => toggleStar({ collectionId: collection.id })}
          >
            <Star size={18} fill={starredIds.has(collection.id) ? 'currentColor' : 'none'} />
          </IconButton>
          {canWrite && (
            <Button icon={<Plus size={16} />} onClick={() => createDoc({ collectionId: collection.id })}>
              {t('New doc')}
            </Button>
          )}
          <Menu
            align="right"
            trigger={
              <IconButton title={t('More')}>
                <MoreHorizontal size={18} />
              </IconButton>
            }
            items={[
              subscribed !== null && {
                label: subscribed ? t('Unfollow') : t('Follow'),
                icon: subscribed ? <BellOff size={16} /> : <Bell size={16} />,
                onClick: async () => {
                  try {
                    const r = await api(subscribed ? 'subscriptions.delete' : 'subscriptions.create', { collectionId: collection.id });
                    setSubscribed(r.subscribed);
                    toast(r.subscribed ? t('You will be notified of new documents in this collection') : t('You no longer follow this collection'));
                  } catch (e) {
                    toast(e.message, 'error');
                  }
                },
              },
              subscribed !== null && { separator: true },
              canWrite && { label: t('New document'), icon: <Plus size={16} />, onClick: () => createDoc({ collectionId: collection.id }) },
              canWrite && { label: t('Import…'), icon: <Upload size={16} />, onClick: importFile },
              canWrite && { label: t('Edit…'), icon: <Pencil size={16} />, onClick: () => setEditing(true) },
              { label: t('Export'), icon: <Download size={16} />, onClick: exportCollection },
              canDelete && { separator: true },
              canDelete && { label: t('Delete…'), icon: <Trash2 size={16} />, danger: true, onClick: () => setConfirmDelete(true) },
            ]}
          />
        </>
      }
    >
      <div className="collection-hero">
        <CollectionIcon collection={collection} size={40} />
        <h1 className="page-title">{collection.name}</h1>
      </div>
      <div className="collection-description">
        <Editor
          key={collection.id}
          value={collection.description}
          editable={canWrite}
          onChange={saveDescription}
          documentId={undefined}
          placeholder={canWrite ? t('Add a description…') : ''}
        />
      </div>
      <PinnedDocs key={collection.id} collectionId={collection.id} />
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'updated', label: t('Recently updated') },
          { value: 'published', label: t('Recently published') },
          { value: 'old', label: t('Least recently updated') },
          { value: 'alpha', label: 'A–Z' },
        ]}
      />
      {docs && docs.length === 0 ? (
        <Empty>
          <p>{t('Collections are a place to organize documents. Get started by creating a new one!')}</p>
          {canWrite && (
            <Button icon={<Plus size={16} />} onClick={() => createDoc({ collectionId: collection.id })}>
              {t('Create a document')}
            </Button>
          )}
        </Empty>
      ) : (
        <DocumentList docs={docs} showCollection={false} />
      )}
      {editing && (
        <CollectionModal
          collection={collection}
          onClose={() => {
            setEditing(false);
            load();
          }}
        />
      )}
      {confirmDelete && (
        <ConfirmModal
          title={t('Delete collection')}
          danger
          confirmLabel={t('I’m sure – Delete')}
          message={t('Are you sure you want to delete the {name} collection? All of its documents will be moved to the trash.', { name: collection.name })}
          onClose={() => setConfirmDelete(false)}
          onConfirm={async () => {
            await api('collections.delete', { id: collection.id });
            await refreshCollections();
            await refreshAllTrees();
            toast(t('Collection deleted'));
            navigate('/');
          }}
        />
      )}
    </Scene>
  );
}

export { collectionUrl };
