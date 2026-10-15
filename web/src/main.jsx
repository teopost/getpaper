import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import './styles.css';
import './editor/editor.css';

import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import ErrorBoundary from './components/ErrorBoundary';
import { installErrorReporting } from './errorReport';

installErrorReporting();
import { api } from './api';
import { applyInstance, applyPrefs, setDefaults } from './prefs';

// installation defaults (language, theme) are needed before the first render, also on public pages
api('auth.config')
  .then((c) => {
    setDefaults(c.defaults);
    applyInstance(c.instance);
  })
  .catch(() => {})
  .finally(() => {
    applyPrefs(null);
    ReactDOM.createRoot(document.getElementById('root')).render(
      <ErrorBoundary>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </ErrorBoundary>
    );
  });
