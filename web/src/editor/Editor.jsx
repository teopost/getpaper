import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { BubbleMenu, EditorContent, useEditor } from '@tiptap/react';
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
  Info,
  AlertTriangle,
  Lightbulb,
  CheckCircle2,
  Braces,
  Link2,
  Underline as UnderlineIcon,
  Calendar,
  Paperclip,
  Trash2,
  Check,
  X,
} from 'lucide-react';
import { uploadFile } from '../api';
import { Notice, createSlashCommand } from './extensions';

const lowlight = createLowlight(common);

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
      window.alert(`Upload failed: ${e.message}`);
    }
  }
}

function slashItems(documentId) {
  const run = (fn) => ({ editor, range }) => fn(editor.chain().focus().deleteRange(range), editor);
  return [
    { section: 'Text', title: 'Big heading', keywords: 'h1 title', icon: <Heading1 size={16} />, shortcut: '#', command: run((c) => c.setNode('heading', { level: 1 }).run()) },
    { section: 'Text', title: 'Medium heading', keywords: 'h2 subtitle', icon: <Heading2 size={16} />, shortcut: '##', command: run((c) => c.setNode('heading', { level: 2 }).run()) },
    { section: 'Text', title: 'Small heading', keywords: 'h3', icon: <Heading3 size={16} />, shortcut: '###', command: run((c) => c.setNode('heading', { level: 3 }).run()) },
    { section: 'Lists', title: 'Todo list', keywords: 'checkbox task checklist', icon: <ListTodo size={16} />, shortcut: '[ ]', command: run((c) => c.toggleTaskList().run()) },
    { section: 'Lists', title: 'Bulleted list', keywords: 'ul unordered', icon: <List size={16} />, shortcut: '-', command: run((c) => c.toggleBulletList().run()) },
    { section: 'Lists', title: 'Ordered list', keywords: 'ol numbered', icon: <ListOrdered size={16} />, shortcut: '1.', command: run((c) => c.toggleOrderedList().run()) },
    { section: 'Blocks', title: 'Table', keywords: 'grid', icon: <TableIcon size={16} />, command: run((c) => c.insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()) },
    { section: 'Blocks', title: 'Quote', keywords: 'blockquote citation', icon: <Quote size={16} />, shortcut: '>', command: run((c) => c.toggleBlockquote().run()) },
    { section: 'Blocks', title: 'Code block', keywords: 'pre snippet', icon: <Braces size={16} />, shortcut: '```', command: run((c) => c.toggleCodeBlock().run()) },
    { section: 'Blocks', title: 'Divider', keywords: 'hr horizontal rule line', icon: <Minus size={16} />, shortcut: '---', command: run((c) => c.setHorizontalRule().run()) },
    {
      section: 'Blocks',
      title: 'Image',
      keywords: 'picture photo upload',
      icon: <ImageIcon size={16} />,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        pickFile('image/*').then((f) => f && insertFiles(editor, [f], documentId));
      },
    },
    {
      section: 'Blocks',
      title: 'File attachment',
      keywords: 'upload attachment',
      icon: <Paperclip size={16} />,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        pickFile().then((f) => f && insertFiles(editor, [f], documentId));
      },
    },
    { section: 'Notices', title: 'Info notice', keywords: 'callout', icon: <Info size={16} />, shortcut: ':::', command: run((c) => c.setNotice('info').run()) },
    { section: 'Notices', title: 'Warning notice', keywords: 'callout alert', icon: <AlertTriangle size={16} />, command: run((c) => c.setNotice('warning').run()) },
    { section: 'Notices', title: 'Tip notice', keywords: 'callout hint', icon: <Lightbulb size={16} />, command: run((c) => c.setNotice('tip').run()) },
    { section: 'Notices', title: 'Success notice', keywords: 'callout done', icon: <CheckCircle2 size={16} />, command: run((c) => c.setNotice('success').run()) },
    {
      section: 'Other',
      title: 'Current date',
      keywords: 'today time',
      icon: <Calendar size={16} />,
      command: run((c) =>
        c.insertContent(new Date().toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })).run()
      ),
    },
  ];
}

function FormattingMenu({ editor }) {
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
              placeholder="Paste or type a link…"
              onChange={(e) => setHref(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  applyLink();
                }
                if (e.key === 'Escape') setLinkMode(false);
              }}
            />
            <B title="Apply" onClick={applyLink}>
              <Check size={16} />
            </B>
            <B
              title="Remove link"
              onClick={() => {
                editor.chain().focus().extendMarkRange('link').unsetLink().run();
                setLinkMode(false);
              }}
            >
              <Trash2 size={16} />
            </B>
            <B title="Cancel" onClick={() => setLinkMode(false)}>
              <X size={16} />
            </B>
          </div>
        ) : (
          <>
            <B title="Bold" active={editor.isActive('bold')} onClick={() => editor.chain().focus().toggleBold().run()}>
              <Bold size={16} />
            </B>
            <B title="Italic" active={editor.isActive('italic')} onClick={() => editor.chain().focus().toggleItalic().run()}>
              <Italic size={16} />
            </B>
            <B title="Underline" active={editor.isActive('underline')} onClick={() => editor.chain().focus().toggleUnderline().run()}>
              <UnderlineIcon size={16} />
            </B>
            <B title="Strikethrough" active={editor.isActive('strike')} onClick={() => editor.chain().focus().toggleStrike().run()}>
              <Strikethrough size={16} />
            </B>
            <B title="Highlight" active={editor.isActive('highlight')} onClick={() => editor.chain().focus().toggleHighlight().run()}>
              <Highlighter size={16} />
            </B>
            <B title="Code" active={editor.isActive('code')} onClick={() => editor.chain().focus().toggleCode().run()}>
              <Code size={16} />
            </B>
            <span className="fmt-sep" />
            <B title="Heading" active={editor.isActive('heading', { level: 1 })} onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}>
              <Heading1 size={16} />
            </B>
            <B title="Subheading" active={editor.isActive('heading', { level: 2 })} onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}>
              <Heading2 size={16} />
            </B>
            <B title="Small heading" active={editor.isActive('heading', { level: 3 })} onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}>
              <Heading3 size={16} />
            </B>
            <B title="Quote" active={editor.isActive('blockquote')} onClick={() => editor.chain().focus().toggleBlockquote().run()}>
              <Quote size={16} />
            </B>
            <span className="fmt-sep" />
            <B title="Checklist" active={editor.isActive('taskList')} onClick={() => editor.chain().focus().toggleTaskList().run()}>
              <ListTodo size={16} />
            </B>
            <B title="Bulleted list" active={editor.isActive('bulletList')} onClick={() => editor.chain().focus().toggleBulletList().run()}>
              <List size={16} />
            </B>
            <B title="Ordered list" active={editor.isActive('orderedList')} onClick={() => editor.chain().focus().toggleOrderedList().run()}>
              <ListOrdered size={16} />
            </B>
            <span className="fmt-sep" />
            <B
              title="Link"
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
        <B title="Add row above" onClick={() => editor.chain().focus().addRowBefore().run()}>+ Row ↑</B>
        <B title="Add row below" onClick={() => editor.chain().focus().addRowAfter().run()}>+ Row ↓</B>
        <B title="Add column left" onClick={() => editor.chain().focus().addColumnBefore().run()}>+ Col ←</B>
        <B title="Add column right" onClick={() => editor.chain().focus().addColumnAfter().run()}>+ Col →</B>
        <span className="fmt-sep" />
        <B title="Delete row" onClick={() => editor.chain().focus().deleteRow().run()}>− Row</B>
        <B title="Delete column" onClick={() => editor.chain().focus().deleteColumn().run()}>− Col</B>
        <B title="Delete table" danger onClick={() => editor.chain().focus().deleteTable().run()}>
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
  placeholder = "Type '/' to insert, or start writing…",
  documentId,
  autoFocus,
  onReady,
  className = '',
}) {
  const navigate = useNavigate();
  const editorRef = useRef(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const extensions = useMemo(
    () => [
      StarterKit.configure({ codeBlock: false, heading: { levels: [1, 2, 3, 4] } }),
      Placeholder.configure({
        placeholder: ({ node }) => (node.type.name === 'heading' ? 'Heading' : placeholder),
        includeChildren: false,
      }),
      Link.configure({ openOnClick: false, autolink: true, HTMLAttributes: { rel: 'noopener noreferrer nofollow', target: null } }),
      Image.configure({ inline: false, allowBase64: false }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      Highlight,
      Underline,
      CodeBlockLowlight.configure({ lowlight }),
      Notice,
      createSlashCommand(() => slashItems(documentId)),
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
    [documentId, placeholder]
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
            if (isEditable && !(event.metaKey || event.ctrlKey)) return false;
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
      {editor && editable && <FormattingMenu editor={editor} />}
      {editor && editable && <TableMenu editor={editor} />}
      <EditorContent editor={editor} />
    </div>
  );
}
