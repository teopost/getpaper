import React, { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import { Button, Tabs, Menu } from '../components/ui';
import { CollectionIcon } from '../components/icons';

export function NewDocButton({ collectionId }) {
  const { collections, createDoc, canWrite } = useApp();
  if (!canWrite) return null;
  const writable = collections.filter((c) => c.access === 'write');
  if (collectionId) {
    return (
      <Button icon={<Plus size={16} />} onClick={() => createDoc({ collectionId })}>
        New doc
      </Button>
    );
  }
  return (
    <Menu
      align="right"
      width={240}
      trigger={<Button icon={<Plus size={16} />}>New doc</Button>}
      items={[
        { heading: 'New document in…' },
        ...writable.map((c) => ({
          label: c.name,
          icon: <CollectionIcon collection={c} size={16} />,
          onClick: () => createDoc({ collectionId: c.id }),
        })),
        { separator: true },
        { label: 'New draft (no collection)', onClick: () => createDoc({}) },
      ]}
    />
  );
}

export default function Home() {
  const { user, team } = useApp();
  const [tab, setTab] = useState('viewed');
  const [docs, setDocs] = useState(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    setDocs(null);
    const req =
      tab === 'viewed'
        ? api('documents.viewed', { limit: 25 })
        : tab === 'updated'
        ? api('documents.list', { sort: 'updatedAt', limit: 25 })
        : api('documents.list', { userId: user.id, sort: 'updatedAt', limit: 25 });
    req.then(setDocs).catch(() => setDocs([]));
  }, [tab, user.id, version]);

  return (
    <Scene title="Home" headerTitle={<span className="header-title">Home</span>} actions={<NewDocButton />}>
      <h1 className="page-title">Home</h1>
      <p className="page-subtitle muted">Welcome back to {team.name}.</p>
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'viewed', label: 'Recently viewed' },
          { value: 'updated', label: 'Recently updated' },
          { value: 'mine', label: 'Created by me' },
        ]}
      />
      <DocumentList
        docs={docs}
        onStarChange={() => setVersion((v) => v + 1)}
        empty={
          tab === 'viewed'
            ? 'Documents you’ve recently viewed will be here for easy access.'
            : 'No documents yet. Create one to get started!'
        }
      />
    </Scene>
  );
}
