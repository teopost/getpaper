// Passkeys (WebAuthn) in the browser: the options from the server have binary fields as base64url
// strings, the browser wants ArrayBuffers, and the answer goes back as base64url again.

const toBuffer = (s) => {
  let b = s.replace(/-/g, '+').replace(/_/g, '/');
  while (b.length % 4) b += '=';
  return Uint8Array.from(atob(b), (c) => c.charCodeAt(0)).buffer;
};

const toBase64url = (buf) => {
  let s = '';
  for (const byte of new Uint8Array(buf)) s += String.fromCharCode(byte);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

/** Without null fields: the browser refuses null where it expects a list or an object */
function clean(value) {
  if (Array.isArray(value)) return value.map(clean);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== null && v !== undefined).map(([k, v]) => [k, clean(v)]));
  }
  return value;
}

const credentialList = (list) => (list || []).map((c) => ({ ...c, id: toBuffer(c.id) }));

export const passkeysSupported = () => typeof window !== 'undefined' && !!window.PublicKeyCredential && !!navigator.credentials;

/** Creates a passkey with the options of passkeys.registerStart */
export async function createPasskey(options) {
  const pk = clean(options.publicKey);
  const cred = await navigator.credentials.create({
    publicKey: {
      ...pk,
      challenge: toBuffer(pk.challenge),
      user: { ...pk.user, id: toBuffer(pk.user.id) },
      excludeCredentials: credentialList(pk.excludeCredentials),
    },
  });
  return {
    id: cred.id,
    rawId: toBase64url(cred.rawId),
    type: cred.type,
    response: {
      attestationObject: toBase64url(cred.response.attestationObject),
      clientDataJSON: toBase64url(cred.response.clientDataJSON),
      transports: cred.response.getTransports?.() || [],
    },
    extensions: cred.getClientExtensionResults?.() || {},
  };
}

/** Signs with a passkey, with the options of auth.passkeyStart */
export async function getPasskey(options) {
  const pk = clean(options.publicKey);
  const cred = await navigator.credentials.get({
    publicKey: { ...pk, challenge: toBuffer(pk.challenge), allowCredentials: credentialList(pk.allowCredentials) },
  });
  return {
    id: cred.id,
    rawId: toBase64url(cred.rawId),
    type: cred.type,
    response: {
      authenticatorData: toBase64url(cred.response.authenticatorData),
      clientDataJSON: toBase64url(cred.response.clientDataJSON),
      signature: toBase64url(cred.response.signature),
      userHandle: cred.response.userHandle ? toBase64url(cred.response.userHandle) : null,
    },
    extensions: cred.getClientExtensionResults?.() || {},
  };
}

/** A message for a failed passkey operation; null when the person cancelled it */
export function passkeyError(e) {
  if (e?.name === 'NotAllowedError' || e?.name === 'AbortError') return null;
  return e?.message || String(e);
}
