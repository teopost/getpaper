import React from 'react';
import { NodeViewWrapper } from '@tiptap/react';
import { ExternalLink, Link2 } from 'lucide-react';
import { embedFor } from './embeds';
import { t } from '../i18n';

/** A YouTube video, a Figma file, a Google document… shown in the document */
export default function EmbedView({ node, editor, getPos, selected }) {
  const { url } = node.attrs;
  const embed = embedFor(url);
  const toLink = () => {
    const pos = getPos();
    editor
      .chain()
      .focus()
      .insertContentAt({ from: pos, to: pos + node.nodeSize }, { type: 'paragraph', content: [{ type: 'text', text: url, marks: [{ type: 'link', attrs: { href: url } }] }] })
      .run();
  };
  return (
    <NodeViewWrapper className={`embed ${selected ? 'selected' : ''}`} data-drag-handle>
      {embed ? (
        <div className="embed-frame" style={embed.ratio ? { aspectRatio: String(embed.ratio) } : { height: embed.height || 400 }}>
          <iframe
            src={embed.src}
            title={embed.name}
            loading="lazy"
            referrerPolicy="strict-origin-when-cross-origin"
            sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-presentation allow-forms"
            allow="fullscreen; picture-in-picture; encrypted-media; clipboard-write"
            allowFullScreen
          />
        </div>
      ) : (
        <div className="embed-missing muted small">{t('This link can no longer be embedded.')}</div>
      )}
      <div className="embed-caption" contentEditable={false}>
        <span className="muted small">{embed?.name}</span>
        <a href={url} target="_blank" rel="noopener noreferrer" className="small">
          <ExternalLink size={12} /> {t('Open')}
        </a>
        {editor.isEditable && (
          <button type="button" className="link-btn small" onClick={toLink}>
            <Link2 size={12} /> {t('Show as link')}
          </button>
        )}
      </div>
    </NodeViewWrapper>
  );
}
