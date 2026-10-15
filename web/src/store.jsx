import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { api, docUrl } from './api';
import { useToast } from './components/Toast';

const AppContext = createContext(null);

export function useApp() {
  return useContext(AppContext);
}

export function AppProvider({ initialAuth, children }) {
  const [auth, setAuth] = useState(initialAuth);
  const [collections, setCollections] = useState([]);
  const [trees, setTrees] = useState({});
  const [starred, setStarred] = useState([]);
  const [templates, setTemplates] = useState([]);
  const [activeDoc, setActiveDoc] = useState({ collectionId: null, ids: [] });
  const navigate = useNavigate();
  const toast = useToast();

  const refreshAuth = useCallback(async () => {
    setAuth(await api('auth.info'));
  }, []);

  const refreshCollections = useCallback(async () => {
    const cols = await api('collections.list');
    setCollections(cols);
    return cols;
  }, []);

  const refreshTree = useCallback(async (collectionId) => {
    if (!collectionId) return;
    try {
      const nodes = await api('collections.documents', { id: collectionId });
      setTrees((t) => ({ ...t, [collectionId]: nodes }));
    } catch {
      /* collection may have been removed */
    }
  }, []);

  const refreshAllTrees = useCallback(async () => {
    const cols = await refreshCollections();
    await Promise.all(cols.map((c) => refreshTree(c.id)));
  }, [refreshCollections, refreshTree]);

  const refreshStarred = useCallback(async () => {
    setStarred(await api('stars.list'));
  }, []);

  const refreshTemplates = useCallback(async () => {
    setTemplates(await api('documents.templates'));
  }, []);

  useEffect(() => {
    refreshAllTrees();
    refreshStarred();
    refreshTemplates();
  }, [refreshAllTrees, refreshStarred, refreshTemplates]);

  const switchTeam = useCallback(async (id) => {
    await api('teams.switch', { id });
    window.location.href = '/';
  }, []);

  const createDoc = useCallback(
    async ({ collectionId, parentDocumentId, template } = {}) => {
      try {
        const doc = await api('documents.create', {
          collectionId,
          parentDocumentId,
          template: !!template,
          publish: !!template,
          title: '',
          text: '',
        });
        navigate(docUrl(doc) + '?new=1');
        return doc;
      } catch (e) {
        toast(e.message, 'error');
      }
    },
    [navigate, toast]
  );

  const value = useMemo(
    () => ({
      auth,
      user: auth.user,
      team: auth.team,
      teams: auth.teams,
      role: auth.role,
      isAdmin: auth.role === 'admin',
      canWrite: auth.role !== 'viewer',
      refreshAuth,
      collections,
      refreshCollections,
      trees,
      refreshTree,
      refreshAllTrees,
      starred,
      refreshStarred,
      templates,
      refreshTemplates,
      activeDoc,
      setActiveDoc,
      switchTeam,
      createDoc,
    }),
    [
      auth,
      refreshAuth,
      collections,
      refreshCollections,
      trees,
      refreshTree,
      refreshAllTrees,
      starred,
      refreshStarred,
      templates,
      refreshTemplates,
      activeDoc,
      switchTeam,
      createDoc,
    ]
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}
