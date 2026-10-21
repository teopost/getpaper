import React, { useEffect, useState } from 'react';
import { Search } from 'lucide-react';
import { api } from '../api';
import { Avatar, Button } from './ui';
import { t } from '../i18n';

const SHOWN = 8;

/**
 * Finds members of the workspace by name or email and adds them with a button, as in Outline's
 * "Add people" dialog: a long list of people doesn't fit in a select.
 * exclude: ids already added; onAdd(user) may return a promise.
 */
export default function PeopleSearch({ exclude = [], onAdd, autoFocus }) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState(null);
  const [adding, setAdding] = useState(null);

  useEffect(() => {
    const timer = setTimeout(
      () =>
        api('users.list', { query: query.trim() })
          .then(setResults)
          .catch(() => setResults([])),
      200
    );
    return () => clearTimeout(timer);
  }, [query]);

  // suspended people can't be given access to anything
  const candidates = (results || []).filter((u) => !exclude.includes(u.id) && !u.suspendedAt && !u.accountSuspendedAt);
  const shown = candidates.slice(0, SHOWN);

  const add = async (u) => {
    setAdding(u.id);
    try {
      await onAdd(u);
    } finally {
      setAdding(null);
    }
  };

  return (
    <div className="people-search">
      <div className="people-search-input">
        <Search size={16} className="muted" />
        <input
          autoFocus={autoFocus}
          value={query}
          placeholder={t('Search people by name or email…')}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            // Enter adds the first match; never submits a surrounding form
            if (e.key === 'Enter') {
              e.preventDefault();
              if (shown[0] && query.trim()) add(shown[0]);
            }
          }}
        />
      </div>
      {results && (
        <div className="people-search-results">
          {shown.map((u) => (
            <div key={u.id} className="member-row">
              <Avatar name={u.name} color={u.color} size={24} />
              <span className="member-name">
                {u.name}
                <div className="muted small">{u.email}</div>
              </span>
              <Button type="button" variant="secondary" small disabled={adding === u.id} onClick={() => add(u)}>
                {t('Add')}
              </Button>
            </div>
          ))}
          {shown.length === 0 && <div className="muted small">{query.trim() ? t('No people found.') : t('Everyone is already added.')}</div>}
          {candidates.length > shown.length && (
            <div className="muted small">
              {t('Showing {shown} of {total}: type to narrow the search.', { shown: shown.length, total: candidates.length })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
