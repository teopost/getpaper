import React, { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Search as SearchIcon } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import { DocumentListItem } from '../components/DocumentList';
import { Empty, Spinner, Switch } from '../components/ui';
import { t } from '../i18n';

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/** Turn a ts_headline snippet (markdown with ⟪ ⟫ markers) into safe HTML. */
function contextToHtml(ctx) {
  const cleaned = ctx
    // table of contents marker, also when the search highlighted "toc" in it
    .replace(/\[\[⟪?toc⟫?\]\]/gi, ' ')
    // mentions of documents and collections show their title (people and groups keep the @)
    .replace(/@\[((?:[^\]\\]|\\.)*)\]\(mention:\/\/[^/)]+\/(?:document|collection)\/[^)]*\)/g, '$1')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#*_`>|]+/g, ' ')
    .replace(/:::\w*/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return escapeHtml(cleaned).replace(/⟪/g, '<mark>').replace(/⟫/g, '</mark>');
}

export default function Search() {
  const { collections, team, teams } = useApp();
  const [params, setParams] = useSearchParams();
  const [query, setQuery] = useState(params.get('q') || '');
  const [collectionId, setCollectionId] = useState(params.get('collectionId') || '');
  const [allWorkspaces, setAllWorkspaces] = useState(params.get('all') === '1');
  const multipleTeams = teams.length > 1;
  const [results, setResults] = useState(null);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const q = query.trim();
    const timer = setTimeout(() => {
      const next = {};
      if (q) next.q = q;
      if (allWorkspaces) next.all = '1';
      else if (collectionId) next.collectionId = collectionId;
      setParams(next, { replace: true });
      if (!q) {
        setResults(null);
        return;
      }
      setLoading(true);
      api('documents.search', {
        query: q,
        // collections belong to the current workspace, so the filter only applies there
        collectionId: (!allWorkspaces && collectionId) || undefined,
        allWorkspaces,
        limit: 50,
      })
        .then(setResults)
        .catch(() => setResults([]))
        .finally(() => setLoading(false));
    }, 250);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, collectionId, allWorkspaces]);

  const searchHelp = t(
    'Search titles and content across all collections you have access to. Use quotes for exact phrases and {code} to exclude terms.'
  ).split('{code}');

  return (
    <Scene title={t('Search')} headerTitle={<span className="header-title">{t('Search')}</span>}>
      <div className="search-input-wrap">
        <SearchIcon size={22} className="muted" />
        <input
          ref={inputRef}
          className="search-input"
          placeholder={t('Search…')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {loading && <Spinner size={18} />}
      </div>
      <div className="search-filters">
        {multipleTeams && (
          <label className="search-scope">
            <Switch checked={allWorkspaces} onChange={setAllWorkspaces} />
            {t('Search all workspaces')}
          </label>
        )}
        {!allWorkspaces && (
          <select value={collectionId} onChange={(e) => setCollectionId(e.target.value)}>
            <option value="">{t('Any collection')}</option>
            {collections.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        )}
      </div>
      {results && results.length === 0 && !loading && <Empty>{t('No documents found for your search filters.')}</Empty>}
      {results && results.length > 0 && (
        <div className="doc-list">
          <div className="search-count muted small">
            {t(results.length === 1 ? '{count} result' : '{count} results', { count: results.length })}
          </div>
          {results.map((r) => (
            <DocumentListItem
              key={r.document.id}
              doc={r.document}
              context={contextToHtml(r.context)}
              workspace={r.document.teamId !== team.id ? teams.find((w) => w.id === r.document.teamId) : undefined}
              collectionName={r.collectionName}
            />
          ))}
        </div>
      )}
      {!results && (
        <Empty>
          {searchHelp[0]}
          <code>-word</code>
          {searchHelp[1]}
        </Empty>
      )}
    </Scene>
  );
}
