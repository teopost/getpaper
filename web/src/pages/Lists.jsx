import React, { useEffect, useState } from 'react';
import { RotateCcw, Trash2 } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import { Button, ConfirmModal } from '../components/ui';
import { useToast } from '../components/Toast';
import { t } from '../i18n';
import { NewDocButton } from './Home';

export function Drafts() {
  const [docs, setDocs] = useState(null);
  useEffect(() => {
    api('documents.drafts', { limit: 100 }).then(setDocs);
  }, []);
  return (
    <Scene title={t('Drafts')} headerTitle={<span className="header-title">{t('Drafts')}</span>} actions={<NewDocButton />}>
      <h1 className="page-title">{t('Drafts')}</h1>
      <p className="page-subtitle muted">{t('Documents you have started but not yet published. Only you can see them.')}</p>
      <DocumentList docs={docs} empty={t('You’ve not got any drafts at the moment.')} />
    </Scene>
  );
}

export function Archive() {
  const [docs, setDocs] = useState(null);
  const { refreshAllTrees } = useApp();
  const toast = useToast();
  const load = () => api('documents.archived', { limit: 100 }).then(setDocs);
  useEffect(() => {
    load();
  }, []);
  return (
    <Scene title={t('Archive')} headerTitle={<span className="header-title">{t('Archive')}</span>}>
      <h1 className="page-title">{t('Archive')}</h1>
      <p className="page-subtitle muted">{t('Archived documents are hidden from the sidebar and search, but can be restored at any time.')}</p>
      <DocumentList
        docs={docs}
        empty={t('The document archive is empty at the moment.')}
        renderActions={(d) => (
          <Button
            variant="secondary"
            small
            icon={<RotateCcw size={14} />}
            onClick={async (e) => {
              e.preventDefault();
              e.stopPropagation();
              try {
                await api('documents.restore', { id: d.id });
                toast(t('Document restored'));
                load();
                refreshAllTrees();
              } catch (err) {
                toast(err.message, 'error');
              }
            }}
          >
            {t('Restore')}
          </Button>
        )}
      />
    </Scene>
  );
}

export function Trash() {
  const [docs, setDocs] = useState(null);
  const [confirm, setConfirm] = useState(null);
  const { refreshAllTrees, collections } = useApp();
  const toast = useToast();
  const load = () => api('documents.deleted', { limit: 100 }).then(setDocs);
  useEffect(() => {
    load();
  }, []);

  const restore = async (d) => {
    try {
      const params = { id: d.id };
      const alive = collections.some((c) => c.id === d.collectionId);
      if (d.publishedAt && !alive) {
        const target = collections.find((c) => c.access === 'write');
        if (!target) throw new Error(t('No collection available to restore into'));
        params.collectionId = target.id;
      }
      await api('documents.restore', params);
      toast(t('Document restored'));
      load();
      refreshAllTrees();
    } catch (err) {
      toast(err.message, 'error');
    }
  };

  return (
    <Scene title={t('Trash')} headerTitle={<span className="header-title">{t('Trash')}</span>}>
      <h1 className="page-title">{t('Trash')}</h1>
      <p className="page-subtitle muted">{t('Deleted documents are kept here for 30 days before being removed permanently.')}</p>
      <DocumentList
        docs={docs}
        empty={t('Trash is empty at the moment.')}
        renderActions={(d) => (
          <>
            <Button
              variant="secondary"
              small
              icon={<RotateCcw size={14} />}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                restore(d);
              }}
            >
              {t('Restore')}
            </Button>
            <Button
              variant="danger"
              small
              icon={<Trash2 size={14} />}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setConfirm(d);
              }}
            >
              {t('Delete')}
            </Button>
          </>
        )}
      />
      {confirm && (
        <ConfirmModal
          title={t('Permanently delete')}
          danger
          confirmLabel={t('Delete forever')}
          message={t('“{title}” and its nested documents will be permanently deleted. This cannot be undone.', { title: confirm.title || t('Untitled') })}
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            await api('documents.delete', { id: confirm.id, permanent: true });
            toast(t('Document permanently deleted'));
            load();
          }}
        />
      )}
    </Scene>
  );
}
