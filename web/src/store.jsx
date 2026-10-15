import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { api, docUrl } from './api';
import { applyInstance, applyPrefs, setDefaults } from './prefs';
import { useToast } from './components/Toast';

const AppContext = createContext(null);

export function useApp() {
  return useContext(AppContext);
}

export function AppProvider({ initialAuth, onOpenWorkspace, children }) {
  const [auth, setAuth] = useState(initialAuth);
  // applied while rendering so the whole tree below already uses the chosen language
  setDefaults(auth.defaults);
  applyInstance(auth.instance);
  const { language } = applyPrefs(auth.user);
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

  // switch without reloading the page: the provider is keyed by workspace, so its state starts fresh
  const switchTeam = useCallback(
    async (id, path = '/') => {
      try {
        await api('teams.switch', { id });
        await onOpenWorkspace(path);
      } catch (e) {
        toast(e.message, 'error');
      }
    },
    [onOpenWorkspace, toast]
  );

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
      isSuperadmin: !!auth.user?.isSuperadmin,
      instance: auth.instance || {},
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
      openWorkspace: onOpenWorkspace,
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
      onOpenWorkspace,
      createDoc,
    ]
  );

  return (
    <AppContext.Provider value={value}>
      {/* remount on language change so every component picks up the new strings */}
      <React.Fragment key={language}>{children}</React.Fragment>
    </AppContext.Provider>
  );
}
