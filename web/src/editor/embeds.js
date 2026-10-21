// Services whose links can be shown inside a document, as in Outline. Only these hosts are
// embedded, always through their own https embed page; any other link stays a link.

const param = (url, name) => {
  try {
    return new URL(url).searchParams.get(name);
  } catch {
    return null;
  }
};

/** YouTube start time from t=90, t=1m30s or start=90 */
function youtubeStart(url) {
  const v = param(url, 't') || param(url, 'start');
  if (!v) return '';
  const m = /^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s?)?$/.exec(v);
  if (!m) return '';
  const s = (Number(m[1] || 0) * 60 + Number(m[2] || 0)) * 60 + Number(m[3] || 0);
  return s ? `?start=${s}` : '';
}

export const EMBEDS = [
  {
    name: 'YouTube',
    match: /^https?:\/\/(?:www\.|m\.)?(?:youtube\.com\/(?:watch\?(?:.*&)?v=|embed\/|shorts\/|live\/)|youtu\.be\/)([\w-]{11})/,
    src: (m, url) => `https://www.youtube-nocookie.com/embed/${m[1]}${youtubeStart(url)}`,
    ratio: 16 / 9,
  },
  {
    name: 'Vimeo',
    match: /^https?:\/\/(?:www\.)?vimeo\.com\/(?:channels\/[\w-]+\/|groups\/[\w-]+\/videos\/)?(\d+)(?:\/([\da-f]+))?/,
    src: (m) => `https://player.vimeo.com/video/${m[1]}${m[2] ? `?h=${m[2]}` : ''}`,
    ratio: 16 / 9,
  },
  {
    name: 'Loom',
    match: /^https?:\/\/(?:www\.)?loom\.com\/(?:share|embed)\/([\da-f]{20,})/,
    src: (m) => `https://www.loom.com/embed/${m[1]}`,
    ratio: 16 / 9,
  },
  {
    name: 'Figma',
    match: /^https:\/\/(?:www\.)?figma\.com\/(?:file|design|proto|board|slides)\/[\w-]+/,
    src: (m, url) => `https://www.figma.com/embed?embed_host=getpaper&url=${encodeURIComponent(url)}`,
    height: 450,
  },
  {
    name: 'Google Docs',
    match: /^https:\/\/docs\.google\.com\/(document)\/d\/([\w-]+)/,
    src: (m) => `https://docs.google.com/${m[1]}/d/${m[2]}/preview`,
    height: 520,
  },
  {
    name: 'Google Sheets',
    match: /^https:\/\/docs\.google\.com\/(spreadsheets)\/d\/([\w-]+)/,
    src: (m) => `https://docs.google.com/${m[1]}/d/${m[2]}/preview`,
    height: 520,
  },
  {
    name: 'Google Slides',
    match: /^https:\/\/docs\.google\.com\/(presentation)\/d\/([\w-]+)/,
    src: (m) => `https://docs.google.com/${m[1]}/d/${m[2]}/preview`,
    ratio: 16 / 9,
  },
  {
    name: 'Google Forms',
    match: /^https:\/\/docs\.google\.com\/forms\/d\/e\/([\w-]+)/,
    src: (m) => `https://docs.google.com/forms/d/e/${m[1]}/viewform?embedded=true`,
    height: 600,
  },
  {
    name: 'Google Drive',
    match: /^https:\/\/drive\.google\.com\/file\/d\/([\w-]+)/,
    src: (m) => `https://drive.google.com/file/d/${m[1]}/preview`,
    height: 480,
  },
  {
    name: 'Miro',
    match: /^https:\/\/miro\.com\/app\/board\/([\w=-]+)/,
    src: (m) => `https://miro.com/app/live-embed/${m[1]}/`,
    height: 480,
  },
  {
    name: 'Spotify',
    match: /^https:\/\/open\.spotify\.com\/(?:intl-[\w-]+\/)?(track|album|playlist|episode|show|artist)\/(\w+)/,
    src: (m) => `https://open.spotify.com/embed/${m[1]}/${m[2]}`,
    height: (m) => (m[1] === 'track' || m[1] === 'episode' ? 152 : 352),
  },
  {
    name: 'CodePen',
    match: /^https:\/\/codepen\.io\/([\w-]+)\/(?:pen|full|details)\/(\w+)/,
    src: (m) => `https://codepen.io/${m[1]}/embed/${m[2]}?default-tab=result`,
    height: 420,
  },
];

/** How to show `url` inside a document, or null when it is not an embeddable link; `only`: name of one service */
export function embedFor(url, only) {
  if (typeof url !== 'string') return null;
  for (const e of EMBEDS) {
    if (only && e.name !== only) continue;
    const m = e.match.exec(url.trim());
    if (m) {
      return {
        name: e.name,
        src: e.src(m, url.trim()),
        ratio: e.ratio,
        height: typeof e.height === 'function' ? e.height(m) : e.height,
      };
    }
  }
  return null;
}
