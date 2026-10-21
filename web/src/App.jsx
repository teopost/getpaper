import React, { useCallback, useEffect, useState } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import { api } from './api';
import { AppProvider } from './store';
import { ToastProvider } from './components/Toast';
import Layout from './components/Layout';
import Login from './pages/Login';
import Register from './pages/Register';
import Invite from './pages/Invite';
import SharedDoc from './pages/SharedDoc';
import Home from './pages/Home';
import Search from './pages/Search';
import CollectionPage from './pages/Collection';
import DocumentPage from './pages/Document';
import { Drafts, Archive, Trash } from './pages/Lists';
import Templates from './pages/Templates';
import Settings from './pages/Settings';
import NoWorkspace from './pages/NoWorkspace';
import { Spinner } from './components/ui';
import { applyInstance, applyPrefs, setDefaults } from './prefs';
import { t } from './i18n';

function AuthenticatedApp() {
  const [auth, setAuth] = useState(null);
  const [error, setError] = useState(null);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    // ?workspace=<id> (link in the "you were added" email): open that workspace instead of the last one used
    const params = new URLSearchParams(location.search);
    const wanted = params.get('workspace');
    const switched = wanted ? api('teams.switch', { id: wanted }).catch((e) => (e.status === 401 ? Promise.reject(e) : null)) : Promise.resolve();
    if (wanted) {
      params.delete('workspace');
      const rest = params.toString();
      navigate(location.pathname + (rest ? `?${rest}` : '') + location.hash, { replace: true });
    }
    switched
      .then(() => api('auth.info'))
      .then(setAuth)
      .catch((e) => {
        if (e.status === 401) {
          const redirect = location.pathname + location.search;
          navigate(`/login${redirect && redirect !== '/' ? `?redirect=${encodeURIComponent(redirect)}` : ''}`, {
            replace: true,
          });
        } else setError(e);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // after switching or creating a workspace: reload the session data and rebuild the app in place
  const openWorkspace = useCallback(
    async (path = '/') => {
      try {
        const next = await api('auth.info');
        setAuth(next);
        navigate(path);
      } catch {
        // the server is already on the new workspace: load it from scratch rather than show stale data
        window.location.href = path;
      }
    },
    [navigate]
  );

  if (error) return <div className="centered-page">{t('Something went wrong: {message}', { message: error.message })}</div>;
  if (!auth) return <div className="centered-page"><Spinner /></div>;
  if (!auth.team) {
    setDefaults(auth.defaults);
    applyInstance(auth.instance);
    applyPrefs(auth.user);
    return <NoWorkspace auth={auth} />;
  }

  return (
    <AppProvider key={auth.team.id} initialAuth={auth} onOpenWorkspace={openWorkspace}>
      <Layout>
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/home" element={<Navigate to="/" replace />} />
          <Route path="/search" element={<Search />} />
          <Route path="/drafts" element={<Drafts />} />
          <Route path="/archive" element={<Archive />} />
          <Route path="/trash" element={<Trash />} />
          <Route path="/templates" element={<Templates />} />
          <Route path="/collection/:id" element={<CollectionPage />} />
          <Route path="/doc/:id" element={<DocumentPage />} />
          <Route path="/settings" element={<Navigate to="/settings/profile" replace />} />
          <Route path="/settings/:section" element={<Settings />} />
          <Route path="*" element={<div className="centered-page">{t('Page not found')}</div>} />
        </Routes>
      </Layout>
    </AppProvider>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route path="/register" element={<Register />} />
        <Route path="/invite/:token" element={<Invite />} />
        <Route path="/s/:shareId" element={<SharedDoc />} />
        <Route path="/*" element={<AuthenticatedApp />} />
      </Routes>
    </ToastProvider>
  );
}
