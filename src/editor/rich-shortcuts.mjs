// The toolbar and keyboard use the same commands and shortcut descriptions.
export const formattingActions = {
  bold: { shortcut: 'Mod-b', command: chain => chain.toggleBold() },
  italic: { shortcut: 'Mod-i', command: chain => chain.toggleItalic() },
  underline: { shortcut: 'Mod-u', command: chain => chain.toggleUnderline() },
  strike: { shortcut: 'Mod-Shift-s', command: chain => chain.toggleStrike() },
  highlight: { shortcut: 'Mod-Shift-h', command: chain => chain.toggleHighlight() },
  link: { shortcut: 'Mod-k' },
  unlink: { shortcut: 'Mod-Shift-k', command: chain => chain.extendMarkRange('link').unsetLink() },
  code: { shortcut: 'Mod-e', command: chain => chain.toggleCode() },
  codeBlock: { shortcut: 'Mod-Alt-c', command: chain => chain.toggleCodeBlock() },
  quote: { shortcut: 'Mod-Shift-b', command: chain => chain.toggleBlockquote() },
  bulletList: { shortcut: 'Mod-Shift-8', command: chain => chain.toggleBulletList() },
  orderedList: { shortcut: 'Mod-Shift-7', command: chain => chain.toggleOrderedList() },
  taskList: { shortcut: 'Mod-Shift-9', command: chain => chain.toggleTaskList() },
  paragraph: { shortcut: 'Mod-Alt-0', command: chain => chain.setParagraph() },
  ...Object.fromEntries([1, 2, 3, 4, 5, 6].map(level => [`heading${level}`, {
    shortcut: `Mod-Alt-${level}`, command: chain => chain.toggleHeading({ level })
  }])),
  clear: { shortcut: 'Mod-\\', command: chain => chain.unsetAllMarks().clearNodes() },
  copy: { shortcut: 'Mod-c' },
  applyLink: { shortcut: 'Enter' }
};

export function shortcutHint(shortcut, mac = /Mac|iPhone|iPad/.test(globalThis.navigator?.platform || '')) {
  if (!shortcut) return {};
  const keys = shortcut.split('-');
  return {
    label: keys.map(key => ({ Mod: mac ? '⌘' : 'Ctrl', Shift: mac ? '⇧' : 'Shift', Alt: mac ? '⌥' : 'Alt' })[key] || (key.length === 1 ? key.toUpperCase() : key)).join(mac ? '' : '+'),
    aria: keys.map(key => ({ Mod: mac ? 'Meta' : 'Control', Alt: 'Alt' })[key] || (key.length === 1 ? key.toUpperCase() : key)).join('+')
  };
}
