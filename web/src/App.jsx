import React, { useEffect, useState } from 'react';
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

function AuthenticatedApp() {
  const [auth, setAuth] = useState(null);
  const [error, setError] = useState(null);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    api('auth.info')
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

  if (error) return <div className="centered-page">Something went wrong: {error.message}</div>;
  if (!auth) return <div className="centered-page"><Spinner /></div>;
  if (!auth.team) return <NoWorkspace auth={auth} />;

  return (
    <AppProvider initialAuth={auth}>
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
          <Route path="*" element={<div className="centered-page">Page not found</div>} />
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
