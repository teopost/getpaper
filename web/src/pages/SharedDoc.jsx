import React, { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { api, timeAgo } from '../api';
import Editor from '../editor/Editor';
import { Spinner } from '../components/ui';
import { Logo } from './Login';
import { t } from '../i18n';
import { getProductName } from '../prefs';

export default function SharedDoc() {
  const { shareId } = useParams();
  const [data, setData] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    // public links must not be indexed nor leak through the Referer header
    const metas = [
      ['robots', 'noindex, nofollow'],
      ['referrer', 'no-referrer'],
    ].map(([name, content]) => {
      const m = document.createElement('meta');
      m.name = name;
      m.content = content;
      document.head.appendChild(m);
      return m;
    });
    return () => metas.forEach((m) => m.remove());
  }, []);

  useEffect(() => {
    api('documents.info', { shareId })
      .then((d) => {
        setData(d);
        document.title = d.document.title || t('Untitled');
      })
      .catch((e) => setError(e.message));
  }, [shareId]);

  if (error)
    return (
      <div className="centered-page">
        <h2>{t('Not found')}</h2>
        <p className="muted">{error}</p>
      </div>
    );
  if (!data)
    return (
      <div className="centered-page">
        <Spinner />
      </div>
    );
  const doc = data.document;
  return (
    <div className="shared-page">
      <header className="shared-header">
        <span className="shared-brand">
          <Logo size={22} /> {data.team.name}
        </span>
      </header>
      <article className={`document ${doc.fullWidth ? 'full-width' : ''}`}>
        <h1 className="document-title-static">
          {doc.emoji && <span className="title-emoji">{doc.emoji}</span>}
          {doc.title || t('Untitled')}
        </h1>
        <div className="document-meta">{t('Last updated {time}', { time: timeAgo(doc.updatedAt) })}</div>
        <Editor value={doc.text} editable={false} documentId={doc.id} />
      </article>
      <footer className="shared-footer">
        {t('Powered by')} <strong>{getProductName()}</strong>
      </footer>
    </div>
  );
}
