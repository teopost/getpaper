import React, { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Search as SearchIcon } from 'lucide-react';
import { api } from '../api';
import { useApp } from '../store';
import Scene from '../components/Scene';
import { DocumentListItem } from '../components/DocumentList';
import { Empty, Spinner } from '../components/ui';

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/** Turn a ts_headline snippet (markdown with ⟪ ⟫ markers) into safe HTML. */
function contextToHtml(ctx) {
  const cleaned = ctx
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#*_`>|]+/g, ' ')
    .replace(/:::\w*/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return escapeHtml(cleaned).replace(/⟪/g, '<mark>').replace(/⟫/g, '</mark>');
}

export default function Search() {
  const { collections } = useApp();
  const [params, setParams] = useSearchParams();
  const [query, setQuery] = useState(params.get('q') || '');
  const [collectionId, setCollectionId] = useState(params.get('collectionId') || '');
  const [results, setResults] = useState(null);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const q = query.trim();
    const t = setTimeout(() => {
      const next = {};
      if (q) next.q = q;
      if (collectionId) next.collectionId = collectionId;
      setParams(next, { replace: true });
      if (!q) {
        setResults(null);
        return;
      }
      setLoading(true);
      api('documents.search', { query: q, collectionId: collectionId || undefined, limit: 50 })
        .then(setResults)
        .catch(() => setResults([]))
        .finally(() => setLoading(false));
    }, 250);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, collectionId]);

  return (
    <Scene title="Search" headerTitle={<span className="header-title">Search</span>}>
      <div className="search-input-wrap">
        <SearchIcon size={22} className="muted" />
        <input
          ref={inputRef}
          className="search-input"
          placeholder="Search…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {loading && <Spinner size={18} />}
      </div>
      <div className="search-filters">
        <select value={collectionId} onChange={(e) => setCollectionId(e.target.value)}>
          <option value="">Any collection</option>
          {collections.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </div>
      {results && results.length === 0 && !loading && <Empty>No documents found for your search filters.</Empty>}
      {results && results.length > 0 && (
        <div className="doc-list">
          <div className="search-count muted small">
            {results.length} result{results.length === 1 ? '' : 's'}
          </div>
          {results.map((r) => (
            <DocumentListItem key={r.document.id} doc={r.document} context={contextToHtml(r.context)} />
          ))}
        </div>
      )}
      {!results && (
        <Empty>
          Search titles and content across all collections you have access to. Use quotes for exact phrases and{' '}
          <code>-word</code> to exclude terms.
        </Empty>
      )}
    </Scene>
  );
}
