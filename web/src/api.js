import { getLanguage, t } from './i18n';
import { reportError } from './errorReport';

export class ApiError extends Error {
  constructor(message, status, code) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

/** What the browser saw of a response, for the server log */
function describe(res, url, readError) {
  const h = (k) => res.headers.get(k);
  const timing = performance.getEntriesByName(new URL(url, window.location.href).href).pop();
  return [
    `HTTP ${res.status}`,
    `type=${h('content-type')}`,
    `length=${h('content-length')}`,
    `encoding=${h('content-encoding')}`,
    `transfer=${h('transfer-encoding')}`,
    `proxy=${h('x-ms-proxy-service-name') || h('via') || '-'}`,
    timing && `transferred=${timing.transferSize} encoded=${timing.encodedBodySize} decoded=${timing.decodedBodySize} protocol=${timing.nextHopProtocol}`,
    readError && `read error: ${readError}`,
  ]
    .filter(Boolean)
    .join(', ');
}

async function handle(res, method = '', url = '') {
  let readError = null;
  const text = await res.text().catch((e) => {
    readError = `${e?.name || 'Error'}: ${e?.message || e}`;
    return '';
  });
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    /* not JSON */
  }
  if (!res.ok) {
    throw new ApiError(json?.message || res.statusText || 'Request failed', res.status, json?.error);
  }
  // every API answer is {"ok": true, "data": …}; anything else was altered on the way (proxy, network)
  if (!json || json.ok !== true) {
    reportError('bad-response', {
      message: `${method}: ${text.length} chars received, ${describe(res, url, readError)}`,
      stack: text.slice(0, 300),
    });
    throw new ApiError(t('Unexpected response from the server, please try again.'), res.status, 'bad_response');
  }
  return json;
}

// reads are safe to repeat when an answer arrives damaged
const isRead = (method) => /\.(list|info|config)$/.test(method) || method === 'documents.search';

/** Call an RPC style endpoint, e.g. api('documents.info', { id }) */
export async function api(method, body = {}, attempt = 1) {
  const url = `/api/${method}`;
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify(body),
  });
  try {
    const json = await handle(res, method, url);
    if (attempt > 1) reportError('retry-ok', { message: `${method}: second attempt succeeded` });
    return json.data;
  } catch (e) {
    if (e.code === 'bad_response' && attempt === 1 && isRead(method)) return api(method, body, 2);
    throw e;
  }
}

export async function upload(method, fields) {
  const fd = new FormData();
  Object.entries(fields).forEach(([k, v]) => {
    if (v !== undefined && v !== null) fd.append(k, v);
  });
  const res = await fetch(`/api/${method}`, { method: 'POST', body: fd, credentials: 'same-origin' });
  const json = await handle(res, method);
  return json.data;
}

/** POST an RPC method whose response is a file, and save it. */
export async function downloadFile(method, body = {}) {
  const res = await fetch(`/api/${method}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    credentials: 'same-origin',
  });
  if (!res.ok) await handle(res);
  const name = /filename="([^"]+)"/.exec(res.headers.get('Content-Disposition') || '')?.[1] || 'export.zip';
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
  return { name, size: blob.size };
}

export async function uploadFile(file, documentId) {
  return upload('attachments.create', { file, documentId });
}

export function slugify(s) {
  return (
    (s || '')
      .toLowerCase()
      .normalize('NFD')
      .replace(/[̀-ͯ]/g, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 60) || 'untitled'
  );
}

export const docUrl = (d) => `/doc/${slugify(d.title)}-${d.urlId}`;
export const collectionUrl = (c) => `/collection/${slugify(c.name)}-${c.urlId}`;

export function timeAgo(date) {
  if (!date) return '';
  const d = new Date(date);
  const s = Math.round((Date.now() - d.getTime()) / 1000);
  if (s < 45) return t('just now');
  const rtf = new Intl.RelativeTimeFormat(getLanguage(), { numeric: 'always' });
  const m = Math.round(s / 60);
  if (m < 60) return rtf.format(-m, 'minute');
  const h = Math.round(m / 60);
  if (h < 24) return rtf.format(-h, 'hour');
  const days = Math.round(h / 24);
  if (days < 30) return rtf.format(-days, 'day');
  return d.toLocaleDateString(getLanguage(), { year: 'numeric', month: 'short', day: 'numeric' });
}

export function formatDate(date) {
  return new Date(date).toLocaleString(getLanguage(), {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function downloadText(filename, text, type = 'text/markdown') {
  const blob = new Blob([text], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    try {
      document.execCommand('copy');
    } catch {
      /* ignore */
    }
    ta.remove();
    return true;
  }
}

export function debounce(fn, ms) {
  let t;
  const wrapped = (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
  wrapped.cancel = () => clearTimeout(t);
  return wrapped;
}
