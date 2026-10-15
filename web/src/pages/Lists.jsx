import React, { useEffect, useState } from 'react';
import { RotateCcw, Trash2 } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import { Button, ConfirmModal } from '../components/ui';
import { useToast } from '../components/Toast';
import { NewDocButton } from './Home';

export function Drafts() {
  const [docs, setDocs] = useState(null);
  useEffect(() => {
    api('documents.drafts', { limit: 100 }).then(setDocs);
  }, []);
  return (
    <Scene title="Drafts" headerTitle={<span className="header-title">Drafts</span>} actions={<NewDocButton />}>
      <h1 className="page-title">Drafts</h1>
      <p className="page-subtitle muted">Documents you have started but not yet published. Only you can see them.</p>
      <DocumentList docs={docs} empty="You’ve not got any drafts at the moment." />
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
    <Scene title="Archive" headerTitle={<span className="header-title">Archive</span>}>
      <h1 className="page-title">Archive</h1>
      <p className="page-subtitle muted">Archived documents are hidden from the sidebar and search, but can be restored at any time.</p>
      <DocumentList
        docs={docs}
        empty="The document archive is empty at the moment."
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
                toast('Document restored');
                load();
                refreshAllTrees();
              } catch (err) {
                toast(err.message, 'error');
              }
            }}
          >
            Restore
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
        if (!target) throw new Error('No collection available to restore into');
        params.collectionId = target.id;
      }
      await api('documents.restore', params);
      toast('Document restored');
      load();
      refreshAllTrees();
    } catch (err) {
      toast(err.message, 'error');
    }
  };

  return (
    <Scene title="Trash" headerTitle={<span className="header-title">Trash</span>}>
      <h1 className="page-title">Trash</h1>
      <p className="page-subtitle muted">Deleted documents are kept here for 30 days before being removed permanently.</p>
      <DocumentList
        docs={docs}
        empty="Trash is empty at the moment."
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
              Restore
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
              Delete
            </Button>
          </>
        )}
      />
      {confirm && (
        <ConfirmModal
          title="Permanently delete"
          danger
          confirmLabel="Delete forever"
          message={`“${confirm.title || 'Untitled'}” and its nested documents will be permanently deleted. This cannot be undone.`}
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            await api('documents.delete', { id: confirm.id, permanent: true });
            toast('Document permanently deleted');
            load();
          }}
        />
      )}
    </Scene>
  );
}
