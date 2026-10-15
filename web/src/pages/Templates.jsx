import React, { useEffect } from 'react';
import { Plus } from 'lucide-react';
import { useApp } from '../store';
import Scene from '../components/Scene';
import DocumentList from '../components/DocumentList';
import { Button } from '../components/ui';

export default function Templates() {
  const { templates, refreshTemplates, createDoc, canWrite } = useApp();
  useEffect(() => {
    refreshTemplates();
  }, [refreshTemplates]);
  return (
    <Scene
      title="Templates"
      headerTitle={<span className="header-title">Templates</span>}
      actions={
        canWrite && (
          <Button icon={<Plus size={16} />} onClick={() => createDoc({ template: true })}>
            New template
          </Button>
        )
      }
    >
      <h1 className="page-title">Templates</h1>
      <p className="page-subtitle muted">
        Templates help your team create consistent and accurate documentation. They are available to everyone in the
        workspace when creating a new document.
      </p>
      <DocumentList docs={templates} empty="There are no templates just yet." />
    </Scene>
  );
}
