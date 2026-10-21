// Margin's editor extensions (renderer/rich-extensions.mjs), minus attachments,
// plus txt:// links and ⌘-click to open a link from the always-editing view.
import { Extension, InputRule } from '@tiptap/core';
import { Plugin } from '@tiptap/pm/state';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from '@tiptap/markdown';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Highlight from '@tiptap/extension-highlight';
import Underline from '@tiptap/extension-underline';
import Placeholder from '@tiptap/extension-placeholder';
import { TableKit } from '@tiptap/extension-table';
import Image from '@tiptap/extension-image';
import { safeAppHref } from './app-links.mjs';
import { formattingActions } from './rich-shortcuts.mjs';

const FormattingShortcuts = Extension.create({
  name: 'txtFormattingShortcuts',
  priority: 1000,
  addOptions() { return { editLink: () => false }; },
  addKeyboardShortcuts() {
    const shortcuts = {
      ...Object.fromEntries(Object.values(formattingActions).filter(action => action.command).map(({ shortcut, command }) => [shortcut, () => command(this.editor.chain()).run()])),
      [formattingActions.link.shortcut]: () => this.options.editLink()
    };
    // Handle capital letter events too, including Shift combinations and Caps Lock.
    return Object.fromEntries(Object.entries(shortcuts).flatMap(([key, command]) => [[key, command], [key.replace(/[a-z]$/, letter => letter.toUpperCase()), command]]));
  }
});

// These two inline HTML tags travel with Markdown and stay readable in any
// other editor. No private editor JSON is needed to preserve formatting.
const MarkdownUnderline = Underline.extend({
  renderMarkdown: (node, helpers) => `<u>${helpers.renderChildren(node)}</u>`
});
const MarkdownHighlight = Highlight.extend({
  renderMarkdown: (node, helpers) => `<mark>${helpers.renderChildren(node)}</mark>`
});
// Keep image Markdown intact without fetching anything while editing.
const QuietImage = Image.extend({
  renderHTML: ({ node }) => node.attrs.src?.startsWith('data:image/')
    ? ['img', { src: node.attrs.src, alt: node.attrs.alt || '', class: 'note-inline-image' }]
    : ['span', { class: 'remote-image', 'data-remote-src': node.attrs.src }, `${node.attrs.alt || 'Image'} · ${node.attrs.src}`]
});
// The bullet shortcut fires as soon as "- " is typed. Completing "[ ] "
// converts that item into a task instead of nesting a checklist inside a bullet.
const LiveTaskItem = TaskItem.extend({
  addInputRules() {
    return [new InputRule({
      find: /^\s*(?:[-*+]\s)?\[([ xX])?\]\s$/,
      handler: ({ range, match, chain }) => {
        const commands = chain().deleteRange(range);
        if (this.editor.isActive('bulletList') || this.editor.isActive('orderedList')) commands.liftListItem('listItem');
        if (!this.editor.isActive('taskList')) commands.toggleTaskList();
        commands.updateAttributes('taskItem', { checked: match[1]?.toLowerCase() === 'x' }).run();
      }
    })];
  }
});
// Plain Markdown pasted from an assistant or text file should render in the
// same editing surface. Rich HTML pastes keep the editor's normal handling.
const MarkdownPaste = Extension.create({
  name: 'markdownPaste',
  addProseMirrorPlugins() {
    const editor = this.editor;
    return [new Plugin({ props: { handlePaste(view, event) {
      const text = event.clipboardData?.getData('text/plain');
      if (!text || event.clipboardData.getData('text/html') || editor.isActive('codeBlock') || editor.isActive('code')) return false;
      if (!/(?:^|\n)\s*(?:#{1,6} |[-*+] |\d+\. |>|```|~~~|\|)|\*\*[^*]+\*\*|__[^_]+__|~~[^~]+~~|\[[^\]]+\]\([^)]+\)/.test(text)) return false;
      return editor.commands.insertContent(text, { contentType: 'markdown' });
    } } })];
  }
});
// The note is always editable, so a plain click places the caret and ⌘-click
// follows the link.
const LinkClick = Extension.create({
  name: 'txtLinkClick',
  addOptions() { return { openLink: null }; },
  addProseMirrorPlugins() {
    const openLink = this.options.openLink;
    return [new Plugin({ props: { handleClick(view, pos, event) {
      if (!(event.metaKey || event.ctrlKey) || !openLink) return false;
      const anchor = event.target.closest?.('a[href]');
      if (!anchor) return false;
      event.preventDefault();
      openLink(anchor.getAttribute('href'));
      return true;
    } } })];
  }
});
export function richExtensions(placeholder, editLink, openLink) {
  return [
    StarterKit.configure({
      underline: false, trailingNode: false,
      link: { openOnClick: false, autolink: true, markdownLinks: true, defaultProtocol: 'https', protocols: ['http', 'https', 'mailto', 'txt', 'margin', 'file', 'obsidian', 'cxtasks', 'cxboards'], isAllowedUri: value => Boolean(safeAppHref(value)) },
      undoRedo: { depth: 200, newGroupDelay: 500 }
    }),
    FormattingShortcuts.configure({ editLink }), LinkClick.configure({ openLink }), MarkdownPaste, MarkdownUnderline, MarkdownHighlight, TaskList, LiveTaskItem.configure({ nested: true, HTMLAttributes: { 'data-type': 'taskItem' } }),
    TableKit.configure({ table: { resizable: false } }), QuietImage,
    Placeholder.configure({ placeholder }), Markdown.configure({ markedOptions: { gfm: true } })
  ];
}
