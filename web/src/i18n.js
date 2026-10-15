// Minimal i18n: the English text is the key, so untranslated strings fall back to English.
// Usage: t('Saved'), t('Moved to {name}', { name }), plural: t(n === 1 ? '{count} document' : '{count} documents', { count: n })
// Same English text with different meanings: add a context after "@@", e.g. t('Archive@@action'); English shows "Archive".
import it from './locales/it';

const DICTIONARIES = { it };
export const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'it', label: 'Italiano' },
];

let language = 'en';

export function setLanguage(lang) {
  language = DICTIONARIES[lang] || lang === 'en' ? lang : 'en';
  document.documentElement.lang = language;
}

export function getLanguage() {
  return language;
}

export function t(key, vars) {
  const s = DICTIONARIES[language]?.[key] ?? key.split('@@')[0];
  if (!vars) return s;
  return s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
}
