import React, { useEffect, useMemo, useRef, useState } from 'react';
import { FileText, Plus, RotateCcw, Sparkles } from 'lucide-react';
import { relativeTime } from './titles.mjs';
import { recentRows } from './recents.mjs';

const KEYWORD_DELAY = 80;
// Meaning search may have to wake the model (a few seconds), so it waits for
// a pause in typing and never blocks the keyword results above it.
const SEMANTIC_DELAY = 400;

export default function Switcher({ notes, closed = [], openIds, api, onPick, onCreate, onClose }) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const [keyword, setKeyword] = useState(null);
  const [semantic, setSemantic] = useState({ query: '', hits: [], pending: false });
  const [status, setStatus] = useState(null);
  const list = useRef(null);
  const seq = useRef(0);
  const byId = useMemo(() => new Map(notes.map(n => [n.id, n])), [notes]);

  useEffect(() => { api.embedStatus().then(setStatus).catch(() => {}); }, []);
  useEffect(() => {
    const q = query.trim();
    const mine = ++seq.current;
    setIndex(0);
    if (!q) { setKeyword(null); setSemantic({ query: '', hits: [], pending: false }); return; }
    setSemantic(s => ({ ...s, pending: true }));
    const k = setTimeout(() => api.searchKeyword(q).then(hits => { if (seq.current === mine) setKeyword(hits); }).catch(() => {}), KEYWORD_DELAY);
    const m = setTimeout(() => api.searchSemantic(q)
      .then(hits => { if (seq.current === mine) setSemantic({ query: q, hits, pending: false }); })
      .catch(() => { if (seq.current === mine) setSemantic({ query: q, hits: [], pending: false }); })
      .finally(() => api.embedStatus().then(setStatus).catch(() => {})), SEMANTIC_DELAY);
    return () => { clearTimeout(k); clearTimeout(m); };
  }, [query]);

  const rows = useMemo(() => {
    const q = query.trim();
    if (!q) {
      // Empty: notes closed lately, then the rest by last edit.
      const recents = recentRows(notes, closed, openIds);
      return [...recents.closed.map(r => ({ note: r.note, closedAt: r.closedAt })), ...recents.recent.map(note => ({ note, recent: true }))];
    }
    const found = (keyword || []).map(h => byId.get(h.id) && { note: byId.get(h.id), snippet: h.snippet }).filter(Boolean);
    const seen = new Set(found.map(r => r.note.id));
    const related = semantic.hits.filter(h => !seen.has(h.id) && byId.get(h.id)).map(h => ({ note: byId.get(h.id), snippet: h.snippet, related: true }));
    // The create row sits above Related, so late-arriving related notes never move it.
    return [...found, { create: q }, ...related];
  }, [query, keyword, semantic, notes, byId, closed, openIds]);

  useEffect(() => { list.current?.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest' }); }, [index]);
  const choose = (row, mode) => row.create ? onCreate(row.create) : onPick(row.note.id, mode);
  const onKey = e => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Escape') { e.preventDefault(); onClose(); }
    else if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) { e.preventDefault(); setIndex(i => Math.min(i + 1, rows.length - 1)); }
    else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) { e.preventDefault(); setIndex(i => Math.max(i - 1, 0)); }
    else if (e.key === 'Enter' && rows[index]) { e.preventDefault(); choose(rows[index], e.altKey ? 'window' : e.metaKey ? 'tab' : 'here'); }
  };
  const firstRelated = rows.findIndex(r => r.related);
  const firstClosed = rows.findIndex(r => r.closedAt);
  const firstRecent = firstClosed < 0 ? -1 : rows.findIndex(r => r.recent);
  const meaning = !status ? '' : status.available || !status.error
    ? `Meaning search · ${status.indexed}/${status.total} notes`
    : 'Meaning search off: no embedding model running';

  return <div className="switcher-scrim" onMouseDown={e => { if (e.target === e.currentTarget) onClose(); }}>
    <div className="switcher" role="dialog" aria-label="Open note">
      <input autoFocus className="switcher-input" placeholder="Find a note, or name a new one" value={query} onChange={e => setQuery(e.target.value)} onKeyDown={onKey} spellCheck={false} />
      <ul ref={list} className="switcher-list" role="listbox">
        {rows.map((row, i) => <React.Fragment key={row.create ? 'create' : row.note.id}>
          {i === firstRelated && <li className="switcher-section" role="presentation">Related</li>}
          {i === firstClosed && <li className="switcher-section" role="presentation">Recently closed</li>}
          {i === firstRecent && <li className="switcher-section" role="presentation">Recent</li>}
          {row.create
            ? <li role="option" aria-selected={i === index} className="switcher-row create" onMouseEnter={() => setIndex(i)} onClick={() => choose(row)}>
                <Plus size={14} /><span className="row-title">New note “{row.create}”</span>
              </li>
            : <li role="option" aria-selected={i === index} className="switcher-row" onMouseEnter={() => setIndex(i)} onClick={e => choose(row, e.altKey ? 'window' : e.metaKey ? 'tab' : 'here')}>
                {row.related ? <Sparkles size={14} /> : row.closedAt ? <RotateCcw size={14} /> : <FileText size={14} />}
                <span className="row-text">
                  <span className="row-title">{row.note.title}{openIds.has(row.note.id) && <em> · open</em>}</span>
                  {(row.snippet || row.note.preview) && <span className="row-preview">{row.snippet || row.note.preview}</span>}
                </span>
                <span className="row-meta">{row.note.kind === 'txt' && <b>txt</b>}{row.closedAt ? `closed ${relativeTime(row.closedAt)}` : relativeTime(row.note.modified)}</span>
              </li>}
        </React.Fragment>)}
        {query.trim() && semantic.pending && <li className="switcher-section" role="presentation">Looking for related notes…</li>}
        {!rows.length && <li className="switcher-empty">No notes yet.</li>}
      </ul>
      <footer className="switcher-hints"><span><kbd>↵</kbd> open here</span><span><kbd>⌘↵</kbd> new tab</span><span><kbd>⌥↵</kbd> new window</span>{firstClosed >= 0 && <span><kbd>⌘⇧T</kbd> reopen</span>}<span className="hint-status" title={status?.error || ''}>{meaning}</span></footer>
    </div>
  </div>;
}
