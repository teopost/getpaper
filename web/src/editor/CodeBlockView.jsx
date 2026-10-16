import React, { useEffect, useState, useSyncExternalStore } from 'react';
import { NodeViewContent, NodeViewWrapper } from '@tiptap/react';
import { Check, Code, Copy, WrapText } from 'lucide-react';
import { copyToClipboard } from '../api';
import { t } from '../i18n';
import MermaidDiagram from './MermaidDiagram';

/** Languages offered in the picker (highlight.js names), in display order after "Plain text". */
export const CODE_LANGUAGES = [
  ['bash', 'Bash'],
  ['shell', 'Shell session'],
  ['powershell', 'PowerShell'],
  ['dos', 'Batch (cmd)'],
  ['sql', 'SQL'],
  ['yaml', 'YAML'],
  ['json', 'JSON'],
  ['xml', 'XML / HTML'],
  ['ini', 'INI / TOML'],
  ['properties', 'Properties'],
  ['dockerfile', 'Dockerfile'],
  ['nginx', 'Nginx'],
  ['apache', 'Apache'],
  ['makefile', 'Makefile'],
  ['diff', 'Diff'],
  ['markdown', 'Markdown'],
  ['javascript', 'JavaScript'],
  ['typescript', 'TypeScript'],
  ['css', 'CSS'],
  ['scss', 'SCSS'],
  ['less', 'Less'],
  ['graphql', 'GraphQL'],
  ['python', 'Python'],
  ['java', 'Java'],
  ['groovy', 'Groovy'],
  ['kotlin', 'Kotlin'],
  ['csharp', 'C#'],
  ['vbnet', 'VB.NET'],
  ['c', 'C'],
  ['cpp', 'C++'],
  ['go', 'Go'],
  ['rust', 'Rust'],
  ['php', 'PHP'],
  ['ruby', 'Ruby'],
  ['perl', 'Perl'],
  ['lua', 'Lua'],
  ['r', 'R'],
  ['swift', 'Swift'],
  ['objectivec', 'Objective-C'],
  ['arduino', 'Arduino'],
  ['wasm', 'WebAssembly'],
  ['mermaid', 'Mermaid diagram'],
];

const LABELS = Object.fromEntries(CODE_LANGUAGES);
// names found in imported Markdown that mean the same language
const ALIASES = { sh: 'bash', zsh: 'bash', console: 'shell', ps: 'powershell', ps1: 'powershell', yml: 'yaml', html: 'xml',
  js: 'javascript', ts: 'typescript', py: 'python', cs: 'csharp', bat: 'dos', cmd: 'dos', toml: 'ini', docker: 'dockerfile',
  mermaidjs: 'mermaid' };

// ---- line wrapping: one setting for every code block, remembered in this browser
const WRAP_KEY = 'code-wrap';
const listeners = new Set();
function readWrap() {
  try {
    return localStorage.getItem(WRAP_KEY) === '1';
  } catch {
    return false;
  }
}
let wrap = readWrap();
function setWrap(v) {
  wrap = v;
  try {
    localStorage.setItem(WRAP_KEY, v ? '1' : '0');
  } catch {
    /* storage unavailable: the setting lasts for this page only */
  }
  listeners.forEach((l) => l());
}
function useWrap() {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => wrap
  );
}

/** True while the selection is inside the code block at `getPos` */
function useCursorInside(editor, getPos, enabled) {
  const [inside, setInside] = useState(false);
  useEffect(() => {
    if (!enabled) return undefined;
    const update = () => {
      const pos = typeof getPos === 'function' ? getPos() : null;
      const node = pos == null ? null : editor.state.doc.nodeAt(pos);
      const { from, to } = editor.state.selection;
      setInside(!!node && editor.isEditable && from > pos && to < pos + node.nodeSize);
    };
    update();
    editor.on('selectionUpdate', update);
    editor.on('update', update);
    return () => {
      editor.off('selectionUpdate', update);
      editor.off('update', update);
    };
  }, [editor, getPos, enabled]);
  return enabled && inside;
}

export default function CodeBlockView({ node, updateAttributes, editor, getPos }) {
  const wrapped = useWrap();
  const [copied, setCopied] = useState(false);
  const raw = node.attrs.language || '';
  const language = ALIASES[raw] || raw;
  const label = language ? LABELS[language] || language : t('Plain text');
  const mermaid = language === 'mermaid';
  // Mermaid blocks show the diagram; the source appears while the cursor is in the block (or it is empty)
  const cursorInside = useCursorInside(editor, getPos, mermaid);
  const editingDiagram = mermaid && editor.isEditable && (cursorInside || !node.textContent.trim());

  const editSource = () => {
    const pos = getPos();
    editor.chain().focus().setTextSelection(pos + node.nodeSize - 1).run();
  };
  // leave the source by selecting the whole block, like Escape does
  const closeSource = () => editor.chain().focus().setNodeSelection(getPos()).run();

  const copy = async () => {
    await copyToClipboard(node.textContent);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <NodeViewWrapper
      className={`code-block ${wrapped ? 'wrap' : ''} ${mermaid ? 'mermaid' : ''} ${mermaid && !editingDiagram ? 'collapsed' : ''}`}
    >
      <div className="code-block-toolbar" contentEditable={false}>
        {editor.isEditable ? (
          <select
            className="code-block-language"
            value={language}
            onChange={(e) => updateAttributes({ language: e.target.value || null })}
            title={t('Language')}
          >
            <option value="">{t('Plain text')}</option>
            {/* keep a language the picker doesn't list (e.g. from an import) */}
            {language && !LABELS[language] && <option value={language}>{language}</option>}
            {CODE_LANGUAGES.map(([value, name]) => (
              <option key={value} value={value}>
                {name}
              </option>
            ))}
          </select>
        ) : (
          <span className="code-block-language-label">{label}</span>
        )}
        {mermaid && editor.isEditable && (
          <button
            type="button"
            className={`code-block-btn ${editingDiagram ? 'active' : ''}`}
            onClick={editingDiagram ? closeSource : editSource}
            title={editingDiagram ? t('Hide diagram source') : t('Edit diagram')}
          >
            <Code size={14} />
          </button>
        )}
        <button
          type="button"
          className={`code-block-btn ${wrapped ? 'active' : ''}`}
          onClick={() => setWrap(!wrapped)}
          title={wrapped ? t('Disable line wrapping') : t('Wrap long lines')}
        >
          <WrapText size={14} />
        </button>
        <button type="button" className="code-block-btn" onClick={copy} title={t('Copy')}>
          {copied ? <Check size={14} /> : <Copy size={14} />}
        </button>
      </div>
      <pre>
        <NodeViewContent as="code" className={language ? `language-${language}` : undefined} />
      </pre>
      {mermaid && (
        <MermaidDiagram
          text={node.textContent}
          live={editingDiagram}
          onClick={editor.isEditable && !editingDiagram ? editSource : undefined}
        />
      )}
    </NodeViewWrapper>
  );
}
