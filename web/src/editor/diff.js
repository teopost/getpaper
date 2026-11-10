// Differences between two versions of a document, shown as decorations on the newer one: added text
// is highlighted, removed text is shown struck through where it was. Used by the history.
//
// The documents are compared in two steps, so that rewritten paragraphs don't match on scattered
// common words: first the blocks (paragraphs, headings, code blocks, images…), then the words of
// the blocks that were edited.

import { Extension, createDocument } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

// above this many edits a diff is not worth computing: the versions are just different
const MAX_EDITS = 2000;

/**
 * Myers' diff of two arrays of strings. Returns the operations in order:
 * ['=', i, j] (a[i] equals b[j]), ['-', i] (a[i] removed), ['+', j] (b[j] added); null when the
 * arrays differ in more than MAX_EDITS places.
 */
export function diffSequences(a, b) {
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const ops = [];
  for (let i = 0; i < start; i++) ops.push(['=', i, i]);
  const middle = myers(a, b, start, endA, start, endB);
  if (!middle) return null;
  ops.push(...middle);
  for (let i = endA, j = endB; i < a.length; i++, j++) ops.push(['=', i, j]);
  return ops;
}

function myers(a, b, a0, a1, b0, b1) {
  const n = a1 - a0;
  const m = b1 - b0;
  if (!n) return Array.from({ length: m }, (_, k) => ['+', b0 + k]);
  if (!m) return Array.from({ length: n }, (_, k) => ['-', a0 + k]);
  const max = Math.min(n + m, MAX_EDITS);
  const offset = max + 1;
  const v = new Int32Array(2 * max + 3);
  const trace = [];
  let found = -1;
  for (let d = 0; d <= max && found < 0; d++) {
    // the frontier before this step, only the diagonals it can reach
    trace.push(v.slice(offset - d - 1, offset + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1]) ? v[offset + k + 1] : v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[a0 + x] === b[b0 + y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        found = d;
        break;
      }
    }
  }
  if (found < 0) return null;
  // walk back through the saved frontiers
  const ops = [];
  let x = n;
  let y = m;
  for (let d = found; d > 0; d--) {
    const prev = trace[d];
    const at = (diag) => prev[diag + d + 1];
    const k = x - y;
    const down = k === -d || (k !== d && at(k - 1) < at(k + 1));
    const pk = down ? k + 1 : k - 1;
    const px = at(pk);
    const py = px - pk;
    while (x > px + (down ? 0 : 1) && y > py + (down ? 1 : 0)) {
      x--;
      y--;
      ops.push(['=', a0 + x, b0 + y]);
    }
    if (down) {
      y--;
      ops.push(['+', b0 + y]);
    } else {
      x--;
      ops.push(['-', a0 + x]);
    }
  }
  while (x > 0 && y > 0) {
    x--;
    y--;
    ops.push(['=', a0 + x, b0 + y]);
  }
  return ops.reverse();
}

// ---------------------------------------------------------------------------
// documents as blocks and words
// ---------------------------------------------------------------------------

const markKey = (marks) =>
  marks
    .map((m) => (m.type.name === 'link' ? `link:${m.attrs.href}` : m.type.name))
    .sort()
    .join(',');

const attrsKey = (node) => {
  const { id, ...attrs } = node.attrs || {};
  return Object.keys(attrs).length ? JSON.stringify(attrs) : '';
};

/** The words, spaces and punctuation of a textblock, with their positions in the document. */
function wordsOf(node, pos) {
  const words = [];
  node.forEach((child, offset) => {
    const from = pos + 1 + offset;
    if (child.isText) {
      const marks = markKey(child.marks);
      const re = /\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]/gu;
      let m;
      while ((m = re.exec(child.text))) {
        words.push({ key: `${m[0]}\u0000${marks}`, text: m[0], from: from + m.index, to: from + m.index + m[0].length });
      }
    } else {
      words.push({ key: `\u0001${child.type.name}${attrsKey(child)}`, text: child.textContent || '', from, to: from + child.nodeSize, atom: child });
    }
  });
  return words;
}

/** Paragraphs, headings, code blocks and leaf blocks (images, rules, embeds) in document order. */
function blocksOf(doc) {
  const blocks = [];
  doc.descendants((node, pos) => {
    if (node.isTextblock) {
      const words = wordsOf(node, pos);
      blocks.push({ node, pos, words, key: `${node.type.name}${attrsKey(node)}\u0002${words.map((w) => w.key).join('\u0003')}` });
      return false;
    }
    if (node.isBlock && node.isLeaf) {
      blocks.push({ node, pos, words: [], key: `${node.type.name}${attrsKey(node)}` });
      return false;
    }
    return true;
  });
  return blocks;
}

// share of the words two blocks have in common, to pair an edited block with its old version
function similarity(a, b) {
  const wordSet = (block) => new Set(block.words.filter((w) => /[\p{L}\p{N}]/u.test(w.text)).map((w) => w.text.toLowerCase()));
  const sa = wordSet(a);
  const sb = wordSet(b);
  if (!sa.size && !sb.size) return a.node.type === b.node.type ? 1 : 0;
  let common = 0;
  for (const w of sa) if (sb.has(w)) common++;
  return common / Math.max(sa.size, sb.size);
}

// ---------------------------------------------------------------------------
// decorations
// ---------------------------------------------------------------------------

function removedBlocksWidget(blocks) {
  return () => {
    const box = document.createElement('div');
    box.className = 'diff-removed-block';
    box.contentEditable = 'false';
    for (const b of blocks) {
      if (b.node.type.name === 'image' && b.node.attrs.src) {
        const img = document.createElement('img');
        img.src = b.node.attrs.src;
        img.alt = b.node.attrs.alt || '';
        box.appendChild(img);
        continue;
      }
      const p = document.createElement(b.node.type.name === 'codeBlock' ? 'pre' : 'p');
      p.textContent = b.node.textContent || (b.node.isLeaf ? `[${b.node.type.name}]` : '');
      box.appendChild(p);
    }
    return box;
  };
}

function removedTextWidget(text) {
  return () => {
    const span = document.createElement('span');
    span.className = 'diff-removed';
    span.textContent = text;
    return span;
  };
}

const visible = (b) => b.node.isLeaf || b.node.textContent.trim();

/**
 * Decorations that show on `doc` what changed from `oldDoc`, and how many blocks changed.
 * Returns null when the versions are too different to compare.
 */
export function diffDecorations(oldDoc, doc) {
  const oldBlocks = blocksOf(oldDoc);
  const newBlocks = blocksOf(doc);
  const ops = diffSequences(
    oldBlocks.map((b) => b.key),
    newBlocks.map((b) => b.key)
  );
  if (!ops) return null;
  const decorations = [];
  let changes = 0;

  // the changes come in hunks of removed and added blocks between unchanged ones
  let removed = [];
  let added = [];
  const flush = (nextPos) => {
    if (!removed.length && !added.length) return;
    // pair each added block with the most similar removed one that keeps the order
    const pairs = new Map();
    let last = -1;
    for (let j = 0; j < added.length; j++) {
      let best = -1;
      let bestScore = 0.4;
      for (let i = last + 1; i < removed.length; i++) {
        const s = similarity(removed[i], added[j]);
        if (s > bestScore) {
          best = i;
          bestScore = s;
        }
      }
      if (best >= 0) {
        pairs.set(j, best);
        last = best;
      }
    }
    const pairedOld = new Set(pairs.values());
    // removed blocks are shown above the blocks that replaced them
    let pending = [];
    let i = 0;
    const showRemoved = (upTo, pos) => {
      for (; i < upTo; i++) if (!pairedOld.has(i) && visible(removed[i])) pending.push(removed[i]);
      if (pending.length) {
        decorations.push(Decoration.widget(pos, removedBlocksWidget(pending), { side: -1, key: `del-${pos}-${changes}` }));
        changes++;
        pending = [];
      }
    };
    added.forEach((b, j) => {
      if (pairs.has(j)) {
        showRemoved(pairs.get(j), b.pos);
        i = pairs.get(j) + 1;
        changes += diffWords(removed[pairs.get(j)], b, decorations);
      } else {
        // the removed blocks before the next pair go above it, as a replacement
        let next = removed.length;
        for (let k = j + 1; k < added.length; k++) if (pairs.has(k)) {
          next = pairs.get(k);
          break;
        }
        showRemoved(next, b.pos);
        if (visible(b)) {
          decorations.push(Decoration.node(b.pos, b.pos + b.node.nodeSize, { class: 'diff-added-block' }));
          changes++;
        }
      }
    });
    showRemoved(removed.length, nextPos);
    removed = [];
    added = [];
  };
  for (const op of ops) {
    if (op[0] === '-') removed.push(oldBlocks[op[1]]);
    else if (op[0] === '+') added.push(newBlocks[op[1]]);
    else flush(newBlocks[op[2]].pos);
  }
  flush(doc.content.size);
  return { decorations: DecorationSet.create(doc, decorations), changes };
}

// word level changes inside a block that was edited; returns 1 when something is shown
function diffWords(oldBlock, block, decorations) {
  const ops = diffSequences(
    oldBlock.words.map((w) => w.key),
    block.words.map((w) => w.key)
  );
  const end = block.pos + block.node.nodeSize - 1;
  if (!ops) {
    decorations.push(Decoration.node(block.pos, block.pos + block.node.nodeSize, { class: 'diff-added-block' }));
    return 1;
  }
  let shown = 0;
  let removedText = '';
  let addedFrom = null;
  let addedTo = null;
  const flushAdded = () => {
    if (addedFrom !== null) decorations.push(Decoration.inline(addedFrom, addedTo, { class: 'diff-added' }));
    addedFrom = addedTo = null;
  };
  const flushRemoved = (pos) => {
    if (removedText) decorations.push(Decoration.widget(pos, removedTextWidget(removedText), { side: -1, key: `delw-${pos}-${removedText}` }));
    removedText = '';
  };
  for (const op of ops) {
    if (op[0] === '-') {
      removedText += oldBlock.words[op[1]].text;
      shown = 1;
    } else if (op[0] === '+') {
      const w = block.words[op[1]];
      if (addedFrom === null) addedFrom = w.from;
      addedTo = w.to;
      shown = 1;
    } else {
      const w = block.words[op[2]];
      flushRemoved(addedFrom ?? w.from);
      flushAdded();
    }
  }
  flushRemoved(addedFrom ?? end);
  flushAdded();
  // a heading that became a paragraph, a different code language…
  if (oldBlock.node.type !== block.node.type || attrsKey(oldBlock.node) !== attrsKey(block.node)) {
    decorations.push(Decoration.node(block.pos, block.pos + block.node.nodeSize, { class: 'diff-changed-block' }));
    shown = 1;
  }
  return shown;
}

// ---------------------------------------------------------------------------
// extension
// ---------------------------------------------------------------------------

export const revisionDiffKey = new PluginKey('revisionDiff');

/**
 * editor.commands.setRevisionDiff(markdown) shows what changed from that older version of the
 * document; null hides the changes. editor.storage.revisionDiff.changes counts the changed places
 * (null when the versions are too different to compare).
 */
export const RevisionDiff = Extension.create({
  name: 'revisionDiff',

  addStorage() {
    return { changes: 0 };
  },

  addCommands() {
    return {
      setRevisionDiff:
        (markdown) =>
        ({ editor, tr, dispatch }) => {
          let set = DecorationSet.empty;
          this.storage.changes = 0;
          if (markdown != null) {
            const html = editor.storage.markdown.parser.parse(markdown);
            const oldDoc = createDocument(html, editor.schema);
            const result = diffDecorations(oldDoc, tr.doc);
            if (result) set = result.decorations;
            this.storage.changes = result ? result.changes : null;
          }
          if (dispatch) tr.setMeta(revisionDiffKey, set).setMeta('addToHistory', false);
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: revisionDiffKey,
        state: {
          init: () => DecorationSet.empty,
          apply: (tr, old) => {
            const set = tr.getMeta(revisionDiffKey);
            if (set) return set;
            return tr.docChanged ? old.map(tr.mapping, tr.doc) : old;
          },
        },
        props: {
          decorations(state) {
            return this.getState(state);
          },
        },
      }),
    ];
  },
});
