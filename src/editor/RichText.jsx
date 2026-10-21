// Margin's RichText (renderer/RichText.jsx) cut down to a full-window note:
// no attachments or reading view, the editor is always live.
import React, { forwardRef, useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import { BubbleMenu } from '@tiptap/react/menus';
import { Bold, Italic, Highlighter, Strikethrough, Underline, Link2, Code2, Braces, Eraser, Copy, ListTodo, List, ListOrdered, Type, Quote, Check, X } from 'lucide-react';
import { richExtensions } from './rich-extensions.mjs';
import { formattingActions, shortcutHint } from './rich-shortcuts.mjs';
import { editableLink } from './app-links.mjs';
import './rich-text.css';

function Tool({ label, action, active = false, children, onClick, type = 'button', ...props }) {
  const hint = shortcutHint(formattingActions[action]?.shortcut);
  const description = hint.label ? `${label} (${hint.label})` : label;
  const id = useId();
  const button = useRef(null);
  const tooltip = useRef(null);
  const [show, setShow] = useState(false);
  const [position, setPosition] = useState(null);
  const hide = () => { setShow(false); setPosition(null); };
  useLayoutEffect(() => {
    if (!show) return;
    const anchor = button.current.getBoundingClientRect();
    const tip = tooltip.current.getBoundingClientRect();
    setPosition({ left: Math.max(8, Math.min(anchor.left + (anchor.width - tip.width) / 2, innerWidth - tip.width - 8)), top: anchor.bottom + tip.height + 14 <= innerHeight ? anchor.bottom + 6 : Math.max(8, anchor.top - tip.height - 6) });
  }, [show, description]);
  useEffect(() => {
    if (!show) return;
    document.addEventListener('scroll', hide, true);
    document.addEventListener('keydown', hide, true);
    document.addEventListener('pointerdown', hide, true);
    window.addEventListener('resize', hide);
    return () => {
      document.removeEventListener('scroll', hide, true);
      document.removeEventListener('keydown', hide, true);
      document.removeEventListener('pointerdown', hide, true);
      window.removeEventListener('resize', hide);
    };
  }, [show]);
  return <>
    <button ref={button} type={type} aria-label={label} aria-description={description} aria-keyshortcuts={hint.aria} aria-describedby={show ? id : undefined} aria-pressed={type === 'button' ? active : undefined} className={active ? 'selected' : ''}
      onMouseEnter={() => setShow(true)} onMouseLeave={hide} onFocus={() => setShow(true)} onBlur={hide}
      onMouseDown={e => e.preventDefault()} onClick={e => { hide(); onClick?.(e); }} {...props}>{children}</button>
    {show && createPortal(<span ref={tooltip} id={id} role="tooltip" className="format-tooltip" style={{ ...position, visibility: position ? 'visible' : 'hidden' }}>{label}{hint.label && <kbd>{hint.label}</kbd>}</span>, document.body)}
  </>;
}
const RichText = forwardRef(function RichText({ body, visible, onChange, onOpenLink, copy, placeholder }, ref) {
  const initialBody = useRef(body);
  const container = useRef(null);
  const editLink = useRef(null);
  const openLinkRef = useRef(onOpenLink); openLinkRef.current = onOpenLink;
  const extensions = useMemo(() => richExtensions(placeholder, () => { editLink.current?.(); return true; }, href => openLinkRef.current?.(href)), [placeholder]);
  const editorProps = useMemo(() => ({ attributes: { class: 'rich-body markdown', role: 'textbox', 'aria-label': 'Note body', 'aria-multiline': 'true', spellcheck: 'true' } }), []);
  const [scrollTarget, setScrollTarget] = useState(null);
  const options = useMemo(() => ({ strategy: 'fixed', placement: 'top', offset: 8, flip: { padding: 12 }, shift: { padding: 12 }, scrollTarget: scrollTarget || window }), [scrollTarget]);
  const appendTo = useCallback(() => document.querySelector('.app-shell') || document.body, []);
  const changeRef = useRef(onChange); changeRef.current = onChange;
  const visibleRef = useRef(visible); visibleRef.current = visible;
  const [menu, setMenu] = useState(null);
  const [linkRequest, setLinkRequest] = useState(0);
  const menuRef = useRef(menu); menuRef.current = menu;
  const linkInput = useRef(null);
  const shouldShow = useCallback(({ editor: ed, view, state }) => visibleRef.current && ed.isEditable && (!state.selection.empty || menuRef.current === 'link') && (view.hasFocus() || Boolean(document.activeElement?.closest('.format-bubble'))), []);
  const [link, setLink] = useState('');
  const [linkError, setLinkError] = useState('');
  const editor = useEditor({
    extensions, content: initialBody.current, contentType: 'markdown', editorProps,
    onUpdate: ({ editor }) => changeRef.current(editor.getMarkdown())
  });
  const flags = useEditorState({ editor, selector: ({ editor: ed }) => ed ? {
    bold: ed.isActive('bold'), italic: ed.isActive('italic'), highlight: ed.isActive('highlight'), strike: ed.isActive('strike'), underline: ed.isActive('underline'),
    link: ed.isActive('link'), code: ed.isActive('code'), codeBlock: ed.isActive('codeBlock'), heading: ed.isActive('heading'), quote: ed.isActive('blockquote'),
    list: ed.isActive('taskList') || ed.isActive('bulletList') || ed.isActive('orderedList')
  } : {} });
  useEffect(() => {
    if (!editor) return;
    const scroller = container.current?.closest('.note-scroll');
    setScrollTarget(scroller);
    const reposition = () => editor.commands.setMeta('txt-formatting', 'updatePosition');
    scroller?.addEventListener('scroll', reposition);
    return () => scroller?.removeEventListener('scroll', reposition);
  }, [editor]);
  useEffect(() => { if (!visible) { setMenu(null); editor?.commands.setMeta('txt-formatting', 'hide'); } }, [visible, editor]);
  useEffect(() => {
    if (!editor) return;
    if (visible && menu === 'link') {
      // Let any pending editor focus finish before handing focus to the URL.
      const frame = requestAnimationFrame(() => {
        if (editor.isDestroyed) return;
        editor.commands.setMeta('txt-formatting', 'show');
        editor.commands.setMeta('txt-formatting', 'updatePosition');
        linkInput.current?.focus();
        linkInput.current?.select();
      });
      return () => cancelAnimationFrame(frame);
    } else if (editor.state.selection.empty) editor.commands.setMeta('txt-formatting', 'hide');
    editor.commands.setMeta('txt-formatting', 'updatePosition');
  }, [menu, linkRequest, visible, editor]);
  useEffect(() => { editor?.commands.setMeta('txt-formatting', 'updatePosition'); }, [linkError, editor]);
  useImperativeHandle(ref, () => ({
    focus: (where = 'end') => editor?.commands.focus(where),
    isEmpty: () => editor?.isEmpty ?? true,
  }), [editor]);
  const run = command => { command(editor.chain().focus()).run(); setMenu(null); };
  const toggleMenu = value => { setMenu(old => old === value ? null : value); };
  const openLink = () => { if (!editor) return; setLink(editor.getAttributes('link').href || ''); setLinkError(''); setMenu('link'); setLinkRequest(value => value + 1); };
  editLink.current = openLink;
  if (!editor) return null;
  const applyLink = e => {
    e.preventDefault();
    if (!link.trim()) { run(chain => chain.extendMarkRange('link').unsetLink()); return; }
    let href;
    try { href = editableLink(link); }
    catch (error) { setLinkError(error.message); return; }
    if (editor.state.selection.empty && !editor.isActive('link')) {
      run(chain => chain.insertContent({ type: 'text', text: link.trim(), marks: [{ type: 'link', attrs: { href } }] }).unsetMark('link'));
    } else run(chain => chain.extendMarkRange('link').setLink({ href }));
  };
  const actions = [
    ['Bold', Bold, 'bold'], ['Italic', Italic, 'italic'],
    ['Highlight', Highlighter, 'highlight'], ['Strikethrough', Strikethrough, 'strike'],
    ['Underline', Underline, 'underline']
  ];
  return <div ref={container} className="rich-editor-content" hidden={!visible}>
    <EditorContent editor={editor} />
    {scrollTarget && <BubbleMenu editor={editor} pluginKey="txt-formatting" updateDelay={50} appendTo={appendTo}
      shouldShow={shouldShow}
      options={options}
      className="format-bubble" role="toolbar" aria-label="Text formatting">
      <div className="bubble-tools">
        <Tool label="Copy selected text" action="copy" onClick={() => copy(editor.state.doc.textBetween(editor.state.selection.from, editor.state.selection.to, '\n'))}><Copy size={14} /></Tool>
        <i className="tool-separator" />
        <Tool label="Lists" active={flags.list} onClick={() => toggleMenu('lists')}><ListTodo size={15} /></Tool>
        <Tool label="Text style" active={flags.heading} onClick={() => toggleMenu('style')}><Type size={15} /></Tool>
        <Tool label="Blockquote" action="quote" active={flags.quote} onClick={() => run(formattingActions.quote.command)}><Quote size={15} /></Tool>
        <i className="tool-separator" />
        {actions.map(([label, Icon, flag]) => <Tool key={label} label={label} action={flag} active={flags[flag]} onClick={() => run(formattingActions[flag].command)}><Icon size={14} /></Tool>)}
        <Tool label="Edit link" action="link" active={flags.link} onClick={openLink}><Link2 size={15} /></Tool>
        <i className="tool-separator" />
        <Tool label="Inline code" action="code" active={flags.code} onClick={() => run(formattingActions.code.command)}><Code2 size={15} /></Tool>
        <Tool label="Code block" action="codeBlock" active={flags.codeBlock} onClick={() => run(formattingActions.codeBlock.command)}><Braces size={14} /></Tool>
        <Tool label="Clear formatting" action="clear" onClick={() => run(formattingActions.clear.command)}><Eraser size={15} /></Tool>
      </div>
      {menu === 'lists' && <div className="bubble-submenu" role="group" aria-label="List styles">
        <Tool label="Bullet list" action="bulletList" active={editor.isActive('bulletList')} onClick={() => run(formattingActions.bulletList.command)}><List size={15} />Bullet list</Tool>
        <Tool label="Numbered list" action="orderedList" active={editor.isActive('orderedList')} onClick={() => run(formattingActions.orderedList.command)}><ListOrdered size={15} />Numbered list</Tool>
        <Tool label="Checklist" action="taskList" active={editor.isActive('taskList')} onClick={() => run(formattingActions.taskList.command)}><ListTodo size={15} />Checklist</Tool>
      </div>}
      {menu === 'style' && <div className="bubble-submenu text-styles" role="group" aria-label="Text styles">
        <Tool label="Paragraph" action="paragraph" active={editor.isActive('paragraph')} onClick={() => run(formattingActions.paragraph.command)}>Text</Tool>
        {[1,2,3,4,5,6].map(level => <Tool key={level} label={`Heading ${level}`} action={`heading${level}`} active={editor.isActive('heading', {level})} onClick={() => run(formattingActions[`heading${level}`].command)}>H{level}</Tool>)}
      </div>}
      {menu === 'link' && <form className="bubble-link" onSubmit={applyLink}>
        <input ref={linkInput} aria-label="Link URL" placeholder="URL, document path, T42, or txt:// link" value={link} onChange={e => setLink(e.target.value)} onKeyDown={e => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === 'Enter') { e.stopPropagation(); applyLink(e); }
          else if (e.key === 'Escape' || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k')) {
            e.preventDefault(); e.stopPropagation();
            if (e.key !== 'Escape' && e.shiftKey) run(formattingActions.unlink.command);
            else { setMenu(null); editor.commands.focus(); }
          }
        }} />
        <Tool type="submit" label="Apply link" action="applyLink"><Check size={15} /></Tool>
        {flags.link && <Tool label="Remove link" action="unlink" onClick={() => run(formattingActions.unlink.command)}><X size={15} /></Tool>}
        {linkError && <small role="alert">{linkError}</small>}
      </form>}
    </BubbleMenu>}
  </div>;
});
export default RichText;
