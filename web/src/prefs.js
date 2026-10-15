// Personal preferences (language, theme, document width) with installation-wide defaults
// from DEFAULT_LANGUAGE / DEFAULT_THEME / DEFAULT_CONTENT_WIDTH (returned by auth.config and auth.info).
import { setLanguage } from './i18n';

const WIDTHS = { normal: '46em', wide: '64em', full: 'none' };

let defaults = { language: 'en', theme: 'system', contentWidth: 'normal' };

export function setDefaults(d) {
  if (d) defaults = { ...defaults, ...d };
}

export function getDefaults() {
  return defaults;
}

export function resolvePrefs(user) {
  return {
    language: user?.language || defaults.language,
    theme: user?.theme || defaults.theme,
    contentWidth: user?.contentWidth || defaults.contentWidth,
  };
}

export function applyPrefs(user) {
  const p = resolvePrefs(user);
  setLanguage(p.language);
  const root = document.documentElement;
  if (p.theme === 'light' || p.theme === 'dark') root.dataset.theme = p.theme;
  else delete root.dataset.theme;
  root.style.setProperty('--doc-width', WIDTHS[p.contentWidth] || WIDTHS.normal);
  return p;
}

// ---- installation settings (general settings, managed by superadmins)

let instance = { allowBackupExport: true, allowMarkdownExport: true, allowPublicSharing: true };

export function getInstance() {
  return instance;
}

/** Black or white, whichever reads better on `hex` */
function contrastText(hex) {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.4 ? '#111319' : '#ffffff';
}

/** Stores the settings and applies the branding colours (CSS variables on <html>). */
export function applyInstance(next) {
  if (next) instance = { ...instance, ...next };
  const root = document.documentElement;
  const set = (name, value) => (value ? root.style.setProperty(name, value) : root.style.removeProperty(name));
  const header = instance.headerColor;
  set('--header-bg', header);
  set('--header-text', header && contrastText(header));
  set('--logo-color', header);
  if (header) root.dataset.brandedHeader = '';
  else delete root.dataset.brandedHeader;
  const accent = instance.accentColor;
  set('--accent', accent);
  set('--accent-hover', accent && `color-mix(in srgb, ${accent} 85%, black)`);
  set('--accent-text', accent && contrastText(accent));
}
