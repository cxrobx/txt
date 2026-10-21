import React, { useEffect, useMemo, useRef, useState } from 'react';
import { FileText, Plus } from 'lucide-react';
import { relativeTime } from './titles.mjs';

// Every word must appear in the title or text; title hits rank first.
export function filterNotes(notes, query) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return notes;
  return notes
    .map(note => {
      const title = note.title.toLowerCase(), text = `${title} ${note.preview.toLowerCase()} ${note.file.toLowerCase()}`;
      if (!words.every(w => text.includes(w))) return null;
      return { note, score: words.filter(w => title.includes(w)).length * 2 + (title.startsWith(words[0]) ? 1 : 0) };
    })
    .filter(Boolean)
    .sort((a, b) => b.score - a.score || b.note.modified - a.note.modified)
    .map(r => r.note);
}

export default function Switcher({ notes, openIds, onPick, onCreate, onClose }) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const list = useRef(null);
  const results = useMemo(() => filterNotes(notes, query).slice(0, 60), [notes, query]);
  const rows = query.trim() ? [...results, { create: query.trim() }] : results;
  useEffect(() => setIndex(0), [query]);
  useEffect(() => { list.current?.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest' }); }, [index]);
  const choose = (row, mode) => row.create ? onCreate(row.create) : onPick(row.id, mode);
  const onKey = e => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Escape') { e.preventDefault(); onClose(); }
    else if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) { e.preventDefault(); setIndex(i => Math.min(i + 1, rows.length - 1)); }
    else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) { e.preventDefault(); setIndex(i => Math.max(i - 1, 0)); }
    else if (e.key === 'Enter' && rows[index]) { e.preventDefault(); choose(rows[index], e.altKey ? 'window' : e.metaKey ? 'tab' : 'here'); }
  };
  return <div className="switcher-scrim" onMouseDown={e => { if (e.target === e.currentTarget) onClose(); }}>
    <div className="switcher" role="dialog" aria-label="Open note">
      <input autoFocus className="switcher-input" placeholder="Find a note, or name a new one" value={query} onChange={e => setQuery(e.target.value)} onKeyDown={onKey} spellCheck={false} />
      <ul ref={list} className="switcher-list" role="listbox">
        {rows.map((row, i) => row.create
          ? <li key="create" role="option" aria-selected={i === index} className="switcher-row create" onMouseEnter={() => setIndex(i)} onClick={() => choose(row)}>
              <Plus size={14} /><span className="row-title">New note “{row.create}”</span>
            </li>
          : <li key={row.id} role="option" aria-selected={i === index} className="switcher-row" onMouseEnter={() => setIndex(i)} onClick={e => choose(row, e.altKey ? 'window' : e.metaKey ? 'tab' : 'here')}>
              <FileText size={14} />
              <span className="row-text">
                <span className="row-title">{row.title}{openIds.has(row.id) && <em> · open</em>}</span>
                {row.preview && <span className="row-preview">{row.preview}</span>}
              </span>
              <span className="row-meta">{row.kind === 'txt' && <b>txt</b>}{relativeTime(row.modified)}</span>
            </li>)}
        {!rows.length && <li className="switcher-empty">No notes yet.</li>}
      </ul>
      <footer className="switcher-hints"><span><kbd>↵</kbd> open here</span><span><kbd>⌘↵</kbd> new tab</span><span><kbd>⌥↵</kbd> new window</span></footer>
    </div>
  </div>;
}
