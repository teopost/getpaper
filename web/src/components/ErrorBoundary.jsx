import React from 'react';
import { t } from '../i18n';
import { reportError } from '../errorReport';

/** Shows a message with a reload button instead of a blank page when rendering fails. */
export default class ErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error, info) {
    console.error('GetPaper crashed', error, info?.componentStack);
    reportError('render', { message: error?.message, stack: `${error?.stack || ''}\n${info?.componentStack || ''}` });
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="centered-page error-page">
        <h1>{t('Something went wrong')}</h1>
        <p className="muted">{t('The page could not be displayed. Reloading usually fixes it, for example after an update.')}</p>
        <pre className="error-detail">{String(this.state.error?.message || this.state.error)}</pre>
        <button className="btn btn-primary" onClick={() => window.location.reload()}>
          {t('Reload')}
        </button>
      </div>
    );
  }
}
