import { getLanguage } from './i18n';

/**
 * Replaces the template variables with the current date and time and the author's name, as in Outline:
 * {date}, {time}, {datetime} and {author}. Dates and times follow the interface language.
 * The server does the same for documents created from a template (documents::replace_template_variables).
 */
export function replaceTemplateVariables(text, user) {
  if (!text) return text;
  const lang = getLanguage();
  const now = new Date();
  const date = { year: 'numeric', month: 'long', day: 'numeric' };
  const time = { hour: 'numeric', minute: 'numeric' };
  return text
    .replace(/{date}/g, now.toLocaleDateString(lang, date))
    .replace(/{time}/g, now.toLocaleTimeString(lang, time))
    .replace(/{datetime}/g, now.toLocaleString(lang, { ...date, ...time }))
    .replace(/{author}/g, () => user?.name || '');
}
