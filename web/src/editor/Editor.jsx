import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { BubbleMenu, EditorContent, ReactNodeViewRenderer, useEditor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Link from '@tiptap/extension-link';
import Image from '@tiptap/extension-image';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Table from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableCell from '@tiptap/extension-table-cell';
import TableHeader from '@tiptap/extension-table-header';
import Highlight from '@tiptap/extension-highlight';
import Underline from '@tiptap/extension-underline';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { common, createLowlight } from 'lowlight';
import apache from 'highlight.js/lib/languages/apache';
import dockerfile from 'highlight.js/lib/languages/dockerfile';
import dos from 'highlight.js/lib/languages/dos';
import groovy from 'highlight.js/lib/languages/groovy';
import nginx from 'highlight.js/lib/languages/nginx';
import powershell from 'highlight.js/lib/languages/powershell';
import properties from 'highlight.js/lib/languages/properties';
import { Markdown } from 'tiptap-markdown';
import {
  Bold,
  Italic,
  Strikethrough,
  Code,
  Highlighter,
  Heading1,
  Heading2,
  Heading3,
  Quote,
  List,
  ListOrdered,
  ListTodo,
  Table as TableIcon,
  Image as ImageIcon,
  Minus,
  ListTree,
  Info,
  AlertTriangle,
  Lightbulb,
  CheckCircle2,
  Braces,
  Workflow,
  Link2,
  Underline as UnderlineIcon,
  Calendar,
  Paperclip,
  Trash2,
  Check,
  X,
  Users,
  TextCursorInput,
} from 'lucide-react';
import { api, uploadFile } from '../api';
import { t, getLanguage } from '../i18n';
import { Avatar } from '../components/ui';
import { CollectionIcon, DocIcon } from '../components/icons';
import { Mention, Notice, TemplatePlaceholder, Toc, createMentionSuggestion, createSlashCommand, newMentionId, serializeTable } from './extensions';
import CodeBlockView from './CodeBlockView';

const lowlight = createLowlight(common);
// languages common in ops documentation that are not in lowlight's default set
lowlight.register({ apache, dockerfile, dos, groovy, nginx, powershell, properties });
// Mermaid sources stay plain text (otherwise lowlight would guess a language and color them)
lowlight.register({ mermaid: () => ({ name: 'Mermaid', aliases: ['mermaidjs'], contains: [] }) });

const isMermaidBlock = (node) => node.type.name === 'codeBlock' && ['mermaid', 'mermaidjs'].includes(node.attrs.language);

/** Code block with language picker, copy button and line wrapping toggle; "mermaid" blocks are drawn as diagrams */
const CodeBlock = CodeBlockLowlight.extend({
  addNodeView() {
    return ReactNodeViewRenderer(CodeBlockView);
  },
  addKeyboardShortcuts() {
    return {
      ...this.parent?.(),
      // Escape leaves the source of a diagram and selects the block, as in Outline
      Escape: ({ editor }) => {
        const { $from } = editor.state.selection;
        if (!isMermaidBlock($from.parent)) return false;
        return editor.commands.setNodeSelection($from.before());
      },
    };
  },
});

/**
 * Tables with resizable columns, as in Outline. Markdown has no column widths, so resized tables are
 * saved as HTML blocks (see serializeTable); the others stay Markdown tables.
 */
const ResizableTable = Table.extend({
  addStorage() {
    return { markdown: { serialize: serializeTable, parse: {} } };
  },
});

function pickFile(accept) {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    if (accept) input.accept = accept;
    input.onchange = () => resolve(input.files?.[0] || null);
    input.click();
  });
}

async function insertFiles(editor, files, documentId, pos) {
  for (const file of files) {
    try {
      const res = await uploadFile(file, documentId);
      const chain = editor.chain().focus();
      if (pos != null) chain.setTextSelection(pos);
      if (file.type.startsWith('image/')) {
        chain.setImage({ src: res.url, alt: file.name }).run();
      } else {
        chain
          .insertContent([
            { type: 'text', text: `📎 ${file.name}`, marks: [{ type: 'link', attrs: { href: res.url } }] },
            { type: 'text', text: ' ' },
          ])
          .run();
      }
    } catch (e) {
      window.alert(t('Upload failed: {error}', { error: e.message }));
    }
  }
}

function slashItems(documentId) {
  const run = (fn) => ({ editor, range }) => fn(editor.chain().focus().deleteRange(range), editor);
  return [
    { section: t('Text'), title: t('Big heading'), titleEn: 'Big heading', keywords: 'h1 title', icon: <Heading1 size={16} />, shortcut: '#', command: run((c) => c.setNode('heading', { level: 1 }).run()) },
    { section: t('Text'), title: t('Medium heading'), titleEn: 'Medium heading', keywords: 'h2 subtitle', icon: <Heading2 size={16} />, shortcut: '##', command: run((c) => c.setNode('heading', { level: 2 }).run()) },
    { section: t('Text'), title: t('Small heading'), titleEn: 'Small heading', keywords: 'h3', icon: <Heading3 size={16} />, shortcut: '###', command: run((c) => c.setNode('heading', { level: 3 }).run()) },
    { section: t('Lists'), title: t('Todo list'), titleEn: 'Todo list', keywords: 'checkbox task checklist', icon: <ListTodo size={16} />, shortcut: '[ ]', command: run((c) => c.toggleTaskList().run()) },
    { section: t('Lists'), title: t('Bulleted list'), titleEn: 'Bulleted list', keywords: 'ul unordered', icon: <List size={16} />, shortcut: '-', command: run((c) => c.toggleBulletList().run()) },
    { section: t('Lists'), title: t('Ordered list'), titleEn: 'Ordered list', keywords: 'ol numbered', icon: <ListOrdered size={16} />, shortcut: '1.', command: run((c) => c.toggleOrderedList().run()) },
    { section: t('Blocks'), title: t('Table'), titleEn: 'Table', keywords: 'grid', icon: <TableIcon size={16} />, command: run((c) => c.insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()) },
    { section: t('Blocks'), title: t('Quote'), titleEn: 'Quote', keywords: 'blockquote citation', icon: <Quote size={16} />, shortcut: '>', command: run((c) => c.toggleBlockquote().run()) },
    { section: t('Blocks'), title: t('Code block'), titleEn: 'Code block', keywords: 'pre snippet', icon: <Braces size={16} />, shortcut: '```', command: run((c) => c.toggleCodeBlock().run()) },
    { section: t('Blocks'), title: t('Mermaid diagram'), titleEn: 'Mermaid diagram', keywords: 'diagram chart graph flowchart sequence gantt mermaidjs', icon: <Workflow size={16} />, command: run((c) => c.setCodeBlock({ language: 'mermaid' }).run()) },
    { section: t('Blocks'), title: t('Table of contents'), titleEn: 'Table of contents', keywords: 'toc index outline headings', icon: <ListTree size={16} />, command: run((c) => c.insertToc().run()) },
    { section: t('Blocks'), title: t('Divider'), titleEn: 'Divider', keywords: 'hr horizontal rule line', icon: <Minus size={16} />, shortcut: '---', command: run((c) => c.setHorizontalRule().run()) },
    {
      section: t('Blocks'),
      title: t('Image'), titleEn: 'Image',
      keywords: 'picture photo upload',
      icon: <ImageIcon size={16} />,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        pickFile('image/*').then((f) => f && insertFiles(editor, [f], documentId));
      },
    },
    {
      section: t('Blocks'),
      title: t('File attachment'), titleEn: 'File attachment',
      keywords: 'upload attachment',
      icon: <Paperclip size={16} />,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        pickFile().then((f) => f && insertFiles(editor, [f], documentId));
      },
    },
    { section: t('Notices'), title: t('Info notice'), titleEn: 'Info notice', keywords: 'callout', icon: <Info size={16} />, shortcut: ':::', command: run((c) => c.setNotice('info').run()) },
    { section: t('Notices'), title: t('Warning notice'), titleEn: 'Warning notice', keywords: 'callout alert', icon: <AlertTriangle size={16} />, command: run((c) => c.setNotice('warning').run()) },
    { section: t('Notices'), title: t('Tip notice'), titleEn: 'Tip notice', keywords: 'callout hint', icon: <Lightbulb size={16} />, command: run((c) => c.setNotice('tip').run()) },
    { section: t('Notices'), title: t('Success notice'), titleEn: 'Success notice', keywords: 'callout done', icon: <CheckCircle2 size={16} />, command: run((c) => c.setNotice('success').run()) },
    {
      section: t('Other'),
      title: t('Current date'), titleEn: 'Current date',
      keywords: 'today time',
      icon: <Calendar size={16} />,
      command: run((c) =>
        c.insertContent(new Date().toLocaleDateString(getLanguage(), { year: 'numeric', month: 'long', day: 'numeric' })).run()
      ),
    },
  ];
}

/** Inserts a mention, followed by a space */
const insertMention = (type, modelId, label) => ({ editor, range }) =>
  editor
    .chain()
    .focus()
    .insertContentAt(range, [
      { type: 'mention', attrs: { id: newMentionId(), type, modelId, label } },
      { type: 'text', text: ' ' },
    ])
    .run();

/**
 * Items of the "@" menu, as in Outline: people and groups (notified), then documents and collections.
 * Without a query: everyone and the recently viewed documents.
 */
async function mentionItems(query) {
  const q = query.trim();
  const lower = q.toLowerCase();
  const [users, groups, docs, collections] = await Promise.all([
    api('users.list', { query: q }).catch(() => []),
    api('groups.list', { query: q }).catch(() => []),
    (q ? api('documents.search', { query: q, limit: 5 }).then((r) => r.map((x) => x.document)) : api('documents.viewed', { limit: 5 })).catch(
      () => []
    ),
    api('collections.list').catch(() => []),
  ]);
  return [
    ...users.slice(0, 6).map((u) => ({
      key: `user-${u.id}`,
      section: t('People'),
      title: u.name,
      icon: <Avatar name={u.name} color={u.color} size={18} />,
      command: insertMention('user', u.id, u.name),
    })),
    ...groups
      .filter((g) => !g.disableMentions)
      .slice(0, 4)
      .map((g) => ({
        key: `group-${g.id}`,
        section: t('Groups'),
        title: g.name,
        icon: <Users size={16} />,
        command: insertMention('group', g.id, g.name),
      })),
    ...docs.slice(0, 5).map((d) => ({
      key: `doc-${d.id}`,
      section: t('Documents'),
      title: d.title || t('Untitled'),
      icon: <DocIcon doc={d} />,
      command: insertMention('document', d.id, d.title || t('Untitled')),
    })),
    ...collections
      .filter((c) => c.name.toLowerCase().includes(lower))
      .slice(0, 3)
      .map((c) => ({
        key: `collection-${c.id}`,
        section: t('Collections'),
        title: c.name,
        icon: <CollectionIcon collection={c} size={16} />,
        command: insertMention('collection', c.id, c.name),
      })),
  ];
}

function FormattingMenu({ editor, template }) {
  const [linkMode, setLinkMode] = useState(false);
  const [href, setHref] = useState('');
  const inputRef = useRef(null);

  useEffect(() => {
    if (linkMode) setTimeout(() => inputRef.current?.focus(), 0);
  }, [linkMode]);

  const B = ({ active, onClick, title, children }) => (
    <button
      className={`fmt-btn ${active ? 'active' : ''}`}
      title={title}
      onMouseDown={(e) => {
        e.preventDefault();
        onClick();
      }}
    >
      {children}
    </button>
  );

  const applyLink = () => {
    const url = href.trim();
    if (!url) editor.chain().focus().extendMarkRange('link').unsetLink().run();
    else {
      const full = /^(https?:|mailto:|\/|#)/.test(url) ? url : `https://${url}`;
      editor.chain().focus().extendMarkRange('link').setLink({ href: full }).run();
    }
    setLinkMode(false);
  };

  return (
    <BubbleMenu
      editor={editor}
      tippyOptions={{ duration: 100, maxWidth: 'none', onHidden: () => setLinkMode(false) }}
      shouldShow={({ editor, state }) => {
        const { from, to, empty } = state.selection;
        if (empty || !editor.isEditable) return false;
        if (editor.isActive('codeBlock') || editor.isActive('image')) return false;
        return state.doc.textBetween(from, to, ' ').length > 0;
      }}
    >
      <div className="formatting-menu">
        {linkMode ? (
          <div className="link-editor">
            <input
              ref={inputRef}
              value={href}
              placeholder={t('Paste or type a link…')}
              onChange={(e) => setHref(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  applyLink();
                }
                if (e.key === 'Escape') setLinkMode(false);
              }}
            />
            <B title={t('Apply')} onClick={applyLink}>
              <Check size={16} />
            </B>
            <B
              title={t('Remove link')}
              onClick={() => {
                editor.chain().focus().extendMarkRange('link').unsetLink().run();
                setLinkMode(false);
              }}
            >
              <Trash2 size={16} />
            </B>
            <B title={t('Cancel')} onClick={() => setLinkMode(false)}>
              <X size={16} />
            </B>
          </div>
        ) : (
          <>
            <B title={t('Bold')} active={editor.isActive('bold')} onClick={() => editor.chain().focus().toggleBold().run()}>
              <Bold size={16} />
            </B>
            <B title={t('Italic')} active={editor.isActive('italic')} onClick={() => editor.chain().focus().toggleItalic().run()}>
              <Italic size={16} />
            </B>
            <B title={t('Underline')} active={editor.isActive('underline')} onClick={() => editor.chain().focus().toggleUnderline().run()}>
              <UnderlineIcon size={16} />
            </B>
            <B title={t('Strikethrough')} active={editor.isActive('strike')} onClick={() => editor.chain().focus().toggleStrike().run()}>
              <Strikethrough size={16} />
            </B>
            <B title={t('Highlight')} active={editor.isActive('highlight')} onClick={() => editor.chain().focus().toggleHighlight().run()}>
              <Highlighter size={16} />
            </B>
            <B title={t('Code')} active={editor.isActive('code')} onClick={() => editor.chain().focus().toggleCode().run()}>
              <Code size={16} />
            </B>
            {template && (
              <B title={t('Placeholder')} active={editor.isActive('placeholder')} onClick={() => editor.chain().focus().togglePlaceholder().run()}>
                <TextCursorInput size={16} />
              </B>
            )}
            <span className="fmt-sep" />
            <B title={t('Heading')} active={editor.isActive('heading', { level: 1 })} onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}>
              <Heading1 size={16} />
            </B>
            <B title={t('Subheading')} active={editor.isActive('heading', { level: 2 })} onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}>
              <Heading2 size={16} />
            </B>
            <B title={t('Small heading')} active={editor.isActive('heading', { level: 3 })} onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}>
              <Heading3 size={16} />
            </B>
            <B title={t('Quote')} active={editor.isActive('blockquote')} onClick={() => editor.chain().focus().toggleBlockquote().run()}>
              <Quote size={16} />
            </B>
            <span className="fmt-sep" />
            <B title={t('Checklist')} active={editor.isActive('taskList')} onClick={() => editor.chain().focus().toggleTaskList().run()}>
              <ListTodo size={16} />
            </B>
            <B title={t('Bulleted list')} active={editor.isActive('bulletList')} onClick={() => editor.chain().focus().toggleBulletList().run()}>
              <List size={16} />
            </B>
            <B title={t('Ordered list')} active={editor.isActive('orderedList')} onClick={() => editor.chain().focus().toggleOrderedList().run()}>
              <ListOrdered size={16} />
            </B>
            <span className="fmt-sep" />
            <B
              title={t('Link')}
              active={editor.isActive('link')}
              onClick={() => {
                setHref(editor.getAttributes('link').href || '');
                setLinkMode(true);
              }}
            >
              <Link2 size={16} />
            </B>
          </>
        )}
      </div>
    </BubbleMenu>
  );
}

function TableMenu({ editor }) {
  const B = ({ onClick, children, title, danger }) => (
    <button
      className={`fmt-btn text ${danger ? 'danger' : ''}`}
      title={title}
      onMouseDown={(e) => {
        e.preventDefault();
        onClick();
      }}
    >
      {children}
    </button>
  );
  return (
    <BubbleMenu
      editor={editor}
      pluginKey="tableMenu"
      tippyOptions={{ duration: 100, placement: 'top', maxWidth: 'none' }}
      shouldShow={({ editor, state }) => editor.isEditable && editor.isActive('table') && state.selection.empty}
    >
      <div className="formatting-menu">
        <B title={t('Add row above')} onClick={() => editor.chain().focus().addRowBefore().run()}>{t('+ Row ↑')}</B>
        <B title={t('Add row below')} onClick={() => editor.chain().focus().addRowAfter().run()}>{t('+ Row ↓')}</B>
        <B title={t('Add column left')} onClick={() => editor.chain().focus().addColumnBefore().run()}>{t('+ Col ←')}</B>
        <B title={t('Add column right')} onClick={() => editor.chain().focus().addColumnAfter().run()}>{t('+ Col →')}</B>
        <span className="fmt-sep" />
        <B title={t('Delete row')} onClick={() => editor.chain().focus().deleteRow().run()}>{t('− Row')}</B>
        <B title={t('Delete column')} onClick={() => editor.chain().focus().deleteColumn().run()}>{t('− Col')}</B>
        <B title={t('Delete table')} danger onClick={() => editor.chain().focus().deleteTable().run()}>
          <Trash2 size={14} />
        </B>
      </div>
    </BubbleMenu>
  );
}

/**
 * Markdown-backed rich text editor.
 * value: markdown string (initial value, the editor is uncontrolled afterwards)
 */
export default function Editor({
  value,
  onChange,
  editable = true,
  placeholder = t("Type '/' to insert, '@' to mention, or start writing…"),
  documentId,
  template = false,
  autoFocus,
  onReady,
  className = '',
}) {
  const navigate = useNavigate();
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const editorRef = useRef(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const extensions = useMemo(
    () => [
      StarterKit.configure({ codeBlock: false, heading: { levels: [1, 2, 3, 4] } }),
      Placeholder.configure({
        placeholder: ({ node }) => (node.type.name === 'heading' ? t('Heading') : placeholder),
        includeChildren: false,
      }),
      Link.configure({ openOnClick: false, autolink: true, HTMLAttributes: { rel: 'noopener noreferrer nofollow', target: null } }),
      Image.configure({ inline: false, allowBase64: false }),
      TaskList,
      TaskItem.configure({ nested: true }),
      ResizableTable.configure({ resizable: true, cellMinWidth: 60, lastColumnResizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      Highlight,
      Underline,
      CodeBlock.configure({ lowlight }),
      Notice,
      Toc,
      TemplatePlaceholder.configure({ template }),
      Mention.configure({ navigate: (path) => navigateRef.current(path) }),
      createSlashCommand(() => slashItems(documentId)),
      createMentionSuggestion(mentionItems),
      Markdown.configure({
        html: true,
        tightLists: true,
        linkify: true,
        breaks: false,
        transformPastedText: true,
        transformCopiedText: false,
      }),
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [documentId, placeholder, template]
  );

  const editor = useEditor(
    {
      extensions,
      content: value || '',
      editable,
      autofocus: autoFocus ? 'end' : false,
      onUpdate: ({ editor }) => {
        onChangeRef.current?.(editor.storage.markdown.getMarkdown());
      },
      editorProps: {
        attributes: { class: 'ProseMirror-doc', spellcheck: 'true' },
        handlePaste: (view, event) => {
          const files = Array.from(event.clipboardData?.files || []);
          if (!files.length || !editorRef.current?.isEditable) return false;
          event.preventDefault();
          insertFiles(editorRef.current, files, documentId);
          return true;
        },
        handleDrop: (view, event, slice, moved) => {
          if (moved) return false;
          const files = Array.from(event.dataTransfer?.files || []);
          if (!files.length || !editorRef.current?.isEditable) return false;
          event.preventDefault();
          const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
          insertFiles(editorRef.current, files, documentId, pos);
          return true;
        },
        handleDOMEvents: {
          click: (view, event) => {
            const a = event.target.closest?.('a[href]');
            if (!a) return false;
            const isEditable = editorRef.current?.isEditable;
            // links need Ctrl/Cmd while editing; mentions of documents and collections open with a click, as in Outline
            if (isEditable && !a.classList.contains('mention') && !(event.metaKey || event.ctrlKey)) return false;
            event.preventDefault();
            const href = a.getAttribute('href');
            if (href.startsWith('/') && !href.startsWith('/files/')) navigate(href);
            else window.open(href, '_blank', 'noopener');
            return true;
          },
        },
      },
    },
    [extensions]
  );

  useEffect(() => {
    editorRef.current = editor;
    if (editor) onReady?.(editor);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor]);

  useEffect(() => {
    if (editor && editor.isEditable !== editable) editor.setEditable(editable);
  }, [editor, editable]);

  return (
    <div className={`editor ${editable ? 'is-editable' : 'is-readonly'} ${className}`}>
      {editor && editable && <FormattingMenu editor={editor} template={template} />}
      {editor && editable && <TableMenu editor={editor} />}
      <EditorContent editor={editor} />
    </div>
  );
}
