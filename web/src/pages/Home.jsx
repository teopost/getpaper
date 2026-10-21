import React, { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import PinnedDocs from '../components/PinnedDocs';
import { Button, Tabs, Menu } from '../components/ui';
import { CollectionIcon } from '../components/icons';
import { t } from '../i18n';

export function NewDocButton({ collectionId }) {
  const { collections, createDoc, canWrite } = useApp();
  if (!canWrite) return null;
  const writable = collections.filter((c) => c.access === 'write');
  if (collectionId) {
    return (
      <Button icon={<Plus size={16} />} onClick={() => createDoc({ collectionId })}>
        {t('New doc')}
      </Button>
    );
  }
  return (
    <Menu
      align="right"
      width={240}
      trigger={<Button icon={<Plus size={16} />}>{t('New doc')}</Button>}
      items={[
        { heading: t('New document in…') },
        ...writable.map((c) => ({
          label: c.name,
          icon: <CollectionIcon collection={c} size={16} />,
          onClick: () => createDoc({ collectionId: c.id }),
        })),
        { separator: true },
        { label: t('New draft (no collection)'), onClick: () => createDoc({}) },
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
    <Scene title={t('Home')} headerTitle={<span className="header-title">{t('Home')}</span>} actions={<NewDocButton />}>
      <h1 className="page-title">{t('Home')}</h1>
      <p className="page-subtitle muted">{t('Welcome back to {name}.', { name: team.name })}</p>
      <PinnedDocs />
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'viewed', label: t('Recently viewed') },
          { value: 'updated', label: t('Recently updated') },
          { value: 'mine', label: t('Created by me') },
        ]}
      />
      <DocumentList
        docs={docs}
        onStarChange={() => setVersion((v) => v + 1)}
        empty={
          tab === 'viewed'
            ? t('Documents you’ve recently viewed will be here for easy access.')
            : t('No documents yet. Create one to get started!')
        }
      />
    </Scene>
  );
}
