// Sends uncaught errors to the server log (POST /api/client.error), at most a few per page load.
let sent = 0;

export function reportError(kind, error) {
  if (sent >= 20) return;
  sent += 1;
  try {
    const body = JSON.stringify({
      kind,
      message: String(error?.message || error || ''),
      stack: String(error?.stack || ''),
      url: window.location.pathname + window.location.search,
    });
    fetch('/api/client.error', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body, keepalive: true }).catch(() => {});
  } catch {
    /* never let reporting break the page */
  }
}

export function installErrorReporting() {
  window.addEventListener('error', (e) => reportError('error', e.error || e.message));
  window.addEventListener('unhandledrejection', (e) => reportError('promise', e.reason));
}
