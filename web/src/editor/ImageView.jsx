import React, { useRef, useState } from 'react';
import Image from '@tiptap/extension-image';
import { NodeViewWrapper, ReactNodeViewRenderer } from '@tiptap/react';
import { defaultMarkdownSerializer } from '@tiptap/pm/markdown';

const MIN_WIDTH = 40;
/** distance (px) within which the width snaps to a target */
const SNAP_DISTANCE = 10;
/** fractions of the page width the images snap to, besides the widths of the other images */
const SNAP_FRACTIONS = [0.25, 1 / 3, 0.5, 2 / 3, 0.75];

/** The target closest to `width` within SNAP_DISTANCE, or null */
function snapWidth(width, targets) {
  let best = null;
  for (const t of targets) {
    if (Math.abs(t - width) <= SNAP_DISTANCE && (best == null || Math.abs(t - width) < Math.abs(best - width))) best = t;
  }
  return best;
}
const ALIGNS = ['left', 'center', 'right'];

const escapeAttr = (s) => String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/**
 * Markdown has no image size nor alignment: a resized or aligned image is saved as an HTML block
 * (`<img src="…" width="300" data-align="center">` on its own line), the others stay `![alt](src)`.
 */
function serializeImage(state, node, parent, index) {
  const { src, alt, title, width, align } = node.attrs;
  if (!width && !align) {
    defaultMarkdownSerializer.nodes.image(state, node, parent, index);
    // a block of its own, also when another image follows
    state.closeBlock(node);
    return;
  }
  const attrs = [['src', src], ['alt', alt], ['title', title], ['width', width], ['data-align', align]]
    .filter(([, v]) => v != null && v !== '')
    .map(([k, v]) => `${k}="${escapeAttr(v)}"`)
    .join(' ');
  state.write(`<img ${attrs}>`);
  state.closeBlock(node);
}

/** The image under the pointer: handles on its sides change the width, keeping the proportions */
function ImageView({ node, editor, selected, updateAttributes }) {
  const { src, alt, title, width, align } = node.attrs;
  const imgRef = useRef(null);
  const [dragWidth, setDragWidth] = useState(null);
  const [snapped, setSnapped] = useState(false);
  const editable = editor.isEditable;

  const startResize = (side) => (e) => {
    e.preventDefault();
    e.stopPropagation();
    const img = imgRef.current;
    const box = img?.closest('.image-block');
    if (!img || !box) return;
    const startX = e.clientX;
    const startWidth = img.getBoundingClientRect().width;
    const maxWidth = box.getBoundingClientRect().width;
    // a centred image grows on both sides
    const factor = (side === 'left' ? -1 : 1) * (align === 'center' ? 2 : 1);
    // the widths of the other images of the document and some fractions of the page; Ctrl (or Cmd) held
    // while dragging turns snapping off
    const others = [...editor.view.dom.querySelectorAll('.image-frame img')]
      .filter((i) => i !== img)
      .map((i) => Math.round(i.getBoundingClientRect().width))
      .filter((w) => w >= MIN_WIDTH);
    const targets = [...new Set([...others, ...SNAP_FRACTIONS.map((f) => Math.round(maxWidth * f)), Math.round(maxWidth)])];
    let current = startWidth;
    const move = (ev) => {
      const free = Math.round(Math.min(maxWidth, Math.max(MIN_WIDTH, startWidth + (ev.clientX - startX) * factor)));
      const snap = ev.ctrlKey || ev.metaKey ? null : snapWidth(free, targets);
      current = snap ?? free;
      setSnapped(snap != null);
      setDragWidth(current);
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setDragWidth(null);
      setSnapped(false);
      // as wide as the page: no fixed width, it follows the page
      updateAttributes({ width: current >= maxWidth - 1 ? null : current });
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const shown = dragWidth ?? width;
  return (
    <NodeViewWrapper className={`image-block align-${align || 'left'}`} data-drag-handle>
      <span className={`image-frame ${selected ? 'selected' : ''} ${dragWidth != null ? 'resizing' : ''}`}>
        <img ref={imgRef} src={src} alt={alt || ''} title={title || undefined} style={shown ? { width: `${shown}px` } : undefined} draggable={false} />
        {editable && selected && (
          <>
            <span className="image-handle left" contentEditable={false} onPointerDown={startResize('left')} />
            <span className="image-handle right" contentEditable={false} onPointerDown={startResize('right')} />
          </>
        )}
        {dragWidth != null && (
          <span className={`image-size ${snapped ? 'snapped' : ''}`} contentEditable={false}>
            {dragWidth} px
          </span>
        )}
      </span>
    </NodeViewWrapper>
  );
}

/** Images that can be resized and aligned left, centre or right */
export const ImageBlock = Image.extend({
  draggable: true,
  addAttributes() {
    return {
      ...this.parent?.(),
      width: {
        default: null,
        parseHTML: (el) => {
          const w = parseInt(el.getAttribute('width') || el.style.width, 10);
          return Number.isFinite(w) && w > 0 ? w : null;
        },
        renderHTML: (attrs) => (attrs.width ? { width: attrs.width } : {}),
      },
      align: {
        default: null,
        parseHTML: (el) => (ALIGNS.includes(el.getAttribute('data-align')) ? el.getAttribute('data-align') : null),
        renderHTML: (attrs) => (attrs.align ? { 'data-align': attrs.align } : {}),
      },
    };
  },
  addStorage() {
    return { markdown: { serialize: serializeImage, parse: {} } };
  },
  addNodeView() {
    return ReactNodeViewRenderer(ImageView);
  },
});
