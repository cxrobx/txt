import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { Plus, X, Search } from 'lucide-react';
import RichText from './editor/RichText.jsx';
import { parseAppLink } from './editor/app-links.mjs';
import Switcher from './Switcher.jsx';
import { derivedTitle } from './titles.mjs';

const appWindow = getCurrentWebviewWindow();
const api = {
  list: () => invoke('list_notes'),
  read: id => invoke('read_note', { id }),
  create: name => invoke('create_note', { name }),
  save: (id, body, title, expectedModified) => invoke('save_note', { id, body, title, expectedModified }),
  trash: id => invoke('trash_note', { id }),
  openWindow: note => invoke('open_window', { note }),
  openExternal: target => invoke('open_external', { target }),
  copy: text => invoke('copy_text', { text }),
  reveal: id => invoke('reveal', { id }),
  searchKeyword: query => invoke('search_keyword', { query }),
  searchSemantic: query => invoke('search_semantic', { query }),
  embedStatus: () => invoke('embed_status'),
  flushEmbeddings: () => invoke('flush_embeddings'),
};
const SAVE_DELAY = 600;
let nextKey = 1;

function tabFromDoc(doc) {
  return { key: nextKey++, id: doc.id, kind: doc.kind, title: doc.title, titleSource: doc.title_source, body: doc.body, modified: doc.modified, loadKey: 0, status: 'saved' };
}
function draftTab(name) {
  return { key: nextKey++, id: null, name: name || null, kind: 'md', title: name || '', titleSource: name ? 'manual' : 'derived', body: '', modified: null, loadKey: 0, status: 'saved' };
}
const isBlankDraft = tab => tab && !tab.id && !tab.body.trim() && !tab.name;
export const tabLabel = tab => tab.titleSource === 'derived' ? derivedTitle(tab.body) : (tab.title || 'Untitled');

export default function App() {
  const [tabs, setTabsState] = useState([]);
  const tabsRef = useRef([]);
  const [activeKey, setActiveKey] = useState(null);
  const activeRef = useRef(null); activeRef.current = activeKey;
  const [notes, setNotes] = useState([]);
  const [switcher, setSwitcher] = useState(false);
  const [toast, setToast] = useState(null);
  const pending = useRef({});
  const timers = useRef({});
  const inflight = useRef({});
  const editors = useRef({});

  const setTabs = next => { tabsRef.current = typeof next === 'function' ? next(tabsRef.current) : next; setTabsState(tabsRef.current); };
  const update = (key, patch) => setTabs(all => all.map(t => t.key === key ? { ...t, ...patch } : t));
  const tabByKey = key => tabsRef.current.find(t => t.key === key);
  const say = useCallback(message => { setToast(message); clearTimeout(say.timer); say.timer = setTimeout(() => setToast(null), 2600); }, []);
  const refreshNotes = useCallback(() => api.list().then(setNotes).catch(() => {}), []);

  // ── Saving ────────────────────────────────────────────────────────────────
  // One pending write per tab; edits during a write queue behind it.
  const flush = useCallback(async key => {
    clearTimeout(timers.current[key]);
    while (inflight.current[key]) await inflight.current[key];
    const work = pending.current[key];
    if (!work) return;
    delete pending.current[key];
    const run = (async () => {
      const tab = tabByKey(key);
      if (!tab) return;
      let { id, modified } = tab;
      try {
        if (!id) {
          // A window that's opened and closed again leaves no empty file.
          if (!work.body.trim() && !work.title?.trim()) return;
          const doc = await api.create(tab.name);
          id = doc.id; modified = doc.modified;
          update(key, { id, kind: doc.kind, modified, name: null });
        }
        const meta = await api.save(id, work.body, work.title ?? null, work.force ? null : modified);
        update(key, { modified: meta.modified, title: meta.title, titleSource: meta.title_source, status: pending.current[key] ? 'dirty' : 'saved', error: null });
      } catch (error) {
        pending.current[key] ??= work;
        update(key, String(error).startsWith('conflict') ? { status: 'conflict' } : { status: 'error', error: String(error) });
      }
    })();
    inflight.current[key] = run;
    try { await run; } finally { delete inflight.current[key]; }
  }, []);
  const queue = useCallback((key, patch) => {
    const tab = tabByKey(key);
    if (!tab) return;
    pending.current[key] = { body: tab.body, ...pending.current[key], ...patch };
    if (tab.status === 'conflict') return;
    update(key, { status: 'dirty' });
    clearTimeout(timers.current[key]);
    timers.current[key] = setTimeout(() => flush(key), SAVE_DELAY);
  }, [flush]);
  const flushAll = useCallback(() => Promise.all(tabsRef.current.map(t => flush(t.key))), [flush]);

  const onBody = useCallback((key, body) => { update(key, { body }); queue(key, { body }); }, [queue]);
  const onTitle = (key, title) => {
    const tab = tabByKey(key);
    update(key, { title, titleSource: title.trim() ? 'manual' : 'derived' });
    queue(key, { body: tab.body, title });
  };

  // ── Tabs ──────────────────────────────────────────────────────────────────
  const activate = key => { setActiveKey(key); requestAnimationFrame(() => editors.current[key]?.focus?.('end')); };
  const newTab = useCallback((name = null) => {
    const tab = draftTab(name);
    setTabs(all => [...all, tab]);
    activate(tab.key);
  }, []);
  // `mode`: 'here' swaps the note shown in this tab, 'tab' opens a new tab,
  // 'window' a new window. A note already open in this window is just focused.
  const openNote = useCallback(async (id, mode = 'here') => {
    if (mode === 'window') return api.openWindow(id);
    const open = tabsRef.current.find(t => t.id === id);
    if (open) return activate(open.key);
    let doc;
    try { doc = await api.read(id); } catch (error) { return say(String(error)); }
    const tab = tabFromDoc(doc);
    const current = tabByKey(activeRef.current);
    if (mode === 'here' && current) {
      await flush(current.key);
      setTabs(all => all.map(t => t.key === current.key ? tab : t));
    } else if (isBlankDraft(current)) {
      setTabs(all => all.map(t => t.key === current.key ? tab : t));
    } else {
      setTabs(all => [...all, tab]);
    }
    activate(tab.key);
  }, [flush, say]);
  const closeTab = useCallback(async key => {
    await flush(key);
    const all = tabsRef.current;
    const index = all.findIndex(t => t.key === key);
    const rest = all.filter(t => t.key !== key);
    if (!rest.length) return appWindow.close();
    setTabs(rest);
    if (activeRef.current === key) activate(rest[Math.min(index, rest.length - 1)].key);
  }, [flush]);
  const cycle = step => {
    const all = tabsRef.current;
    const index = all.findIndex(t => t.key === activeRef.current);
    if (all.length > 1) activate(all[(index + step + all.length) % all.length].key);
  };

  const ensureSaved = async key => {
    await flush(key);
    const tab = tabByKey(key);
    if (!tab?.id) say('Type something first — an empty note isn’t saved.');
    return tab?.id ? tab : null;
  };
  const resolveConflict = async (key, keepMine) => {
    const tab = tabByKey(key);
    if (keepMine) {
      update(key, { status: 'dirty' });
      pending.current[key] = { body: tab.body, ...pending.current[key], force: true };
      return flush(key);
    }
    delete pending.current[key];
    const doc = await api.read(tab.id);
    update(key, { body: doc.body, title: doc.title, titleSource: doc.title_source, modified: doc.modified, loadKey: tab.loadKey + 1, status: 'saved' });
  };

  const openLink = useCallback(href => {
    let link;
    try { link = parseAppLink(href); } catch (error) { return say(error.message); }
    if (link.kind === 'txt') return openNote(link.id, 'tab');
    api.openExternal(link.path || link.href).catch(error => say(String(error)));
  }, [openNote, say]);

  // ── Window wiring ─────────────────────────────────────────────────────────
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    if (params.get('note')) openNote(params.get('note'), 'tab').then(() => { if (!tabsRef.current.length) newTab(); });
    else newTab(params.get('name'));
    refreshNotes();
  }, []);

  useEffect(() => {
    const active = tabs.find(t => t.key === activeKey);
    appWindow.setTitle(active ? tabLabel(active) : 'txt').catch(() => {});
  }, [tabs, activeKey]);

  const handlers = useRef({});
  handlers.current = {
    'new-tab': () => newTab(),
    switch: () => { refreshNotes(); setSwitcher(true); },
    'close-tab': () => activeRef.current && closeTab(activeRef.current),
    'close-window': () => appWindow.close(),
    'next-tab': () => cycle(1),
    'prev-tab': () => cycle(-1),
    'copy-link': async () => { const tab = await ensureSaved(activeRef.current); if (tab) { await api.copy(`txt://note/${tab.id}`); say('Link copied'); } },
    reveal: async () => { const tab = await ensureSaved(activeRef.current); if (tab) api.reveal(tab.id); },
    trash: async () => {
      const key = activeRef.current, tab = tabByKey(key);
      if (!tab) return;
      clearTimeout(timers.current[key]); delete pending.current[key];
      if (tab.id) { try { await api.trash(tab.id); say('Moved to Trash'); } catch (error) { return say(String(error)); } }
      closeTab(key);
    },
  };
  useEffect(() => {
    const offMenu = listen('menu', event => handlers.current[event.payload]?.(), { target: { kind: 'WebviewWindow', label: appWindow.label } });
    const offChanged = listen('notes-changed', async ({ payload }) => {
      refreshNotes();
      if (payload.by === appWindow.label) return;
      for (const tab of tabsRef.current.filter(t => t.id === payload.id && t.status === 'saved' && !pending.current[t.key])) {
        try {
          const doc = await api.read(tab.id);
          if (doc.modified === tab.modified) continue;
          // An auto title changes only the front matter: keep the editor (and
          // its caret and undo history) when the text is what's on screen.
          const current = tabByKey(tab.key);
          if (doc.body === current.body) update(tab.key, { title: doc.title, titleSource: doc.title_source, modified: doc.modified });
          else update(tab.key, { body: doc.body, title: doc.title, titleSource: doc.title_source, modified: doc.modified, loadKey: current.loadKey + 1 });
        } catch { update(tab.key, { status: 'error', error: 'This note was moved or deleted.' }); }
      }
    });
    const offClose = appWindow.onCloseRequested(async () => { await flushAll(); await api.flushEmbeddings().catch(() => {}); });
    const onKey = e => {
      if (e.metaKey && /^[1-9]$/.test(e.key)) {
        const all = tabsRef.current, target = e.key === '9' ? all.at(-1) : all[Number(e.key) - 1];
        if (target) { e.preventDefault(); activate(target.key); }
      }
    };
    const onBlur = () => flushAll();
    window.addEventListener('keydown', onKey);
    window.addEventListener('blur', onBlur);
    window.addEventListener('focus', refreshNotes);
    return () => {
      offMenu.then(f => f()); offChanged.then(f => f()); offClose.then(f => f());
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', onBlur);
      window.removeEventListener('focus', refreshNotes);
    };
  }, [flushAll, refreshNotes]);

  const openIds = useMemo(() => new Set(tabs.map(t => t.id).filter(Boolean)), [tabs]);
  const active = tabs.find(t => t.key === activeKey);

  return <div className="app-shell">
    <header className="tab-bar" data-tauri-drag-region>
      <div className="tabs" role="tablist" data-tauri-drag-region>
        {tabs.map(tab => <div key={tab.key} role="tab" aria-selected={tab.key === activeKey} className={`tab${tab.key === activeKey ? ' active' : ''}`}
          onMouseDown={e => { if (e.button === 1) { e.preventDefault(); closeTab(tab.key); } }} onClick={() => activate(tab.key)} title={tabLabel(tab)}>
          <span className="tab-title">{tabLabel(tab)}</span>
          {tab.status !== 'saved' && <i className={`tab-dot ${tab.status}`} aria-label={tab.status} />}
          <button className="tab-close" aria-label="Close tab" onClick={e => { e.stopPropagation(); closeTab(tab.key); }}><X size={12} /></button>
        </div>)}
        <button className="bar-button" aria-label="New tab (⌘T)" title="New tab (⌘T)" onClick={() => newTab()}><Plus size={15} /></button>
      </div>
      <button className="bar-button switch-button" aria-label="Open note (⌘P)" title="Open note (⌘P)" onClick={() => handlers.current.switch()}><Search size={14} /><kbd>⌘P</kbd></button>
    </header>

    <main className="note-scroll">
      {tabs.map(tab => <article key={tab.key} className="note" hidden={tab.key !== activeKey}>
        {tab.status === 'conflict' && <div className="banner" role="alert">
          This note changed in another window or app.
          <button onClick={() => resolveConflict(tab.key, false)}>Load theirs</button>
          <button onClick={() => resolveConflict(tab.key, true)}>Keep mine</button>
        </div>}
        {tab.status === 'error' && <div className="banner" role="alert">{tab.error}</div>}
        <input className="note-title" value={tab.titleSource === 'derived' ? '' : tab.title} placeholder="Title"
          aria-label="Title" spellCheck={false} onChange={e => onTitle(tab.key, e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); editors.current[tab.key]?.focus('start'); } }} />
        {tab.kind === 'txt'
          ? <PlainText key={`${tab.key}:${tab.loadKey}`} ref={el => { editors.current[tab.key] = el; }} body={tab.body} onChange={body => onBody(tab.key, body)} />
          : <RichText key={`${tab.key}:${tab.loadKey}`} ref={el => { editors.current[tab.key] = el; }} body={tab.body} visible={tab.key === activeKey}
              placeholder="Write something…" onChange={body => onBody(tab.key, body)} onOpenLink={openLink} copy={text => api.copy(text)} />}
      </article>)}
    </main>

    <footer className="status-bar">
      {active && <>
        <span>{{ saved: active.id ? 'Saved' : 'New note', dirty: 'Edited', conflict: 'Not saved', error: 'Not saved' }[active.status]}</span>
        <span>{active.kind === 'txt' ? 'Plain text' : 'Markdown'}</span>
      </>}
    </footer>

    {switcher && <Switcher notes={notes} openIds={openIds} api={api} onClose={() => { setSwitcher(false); requestAnimationFrame(() => editors.current[activeRef.current]?.focus?.('end')); }}
      onPick={(id, mode) => { setSwitcher(false); openNote(id, mode); }}
      onCreate={name => { setSwitcher(false); newTab(name); }} />}
    {toast && <div className="toast" role="status">{toast}</div>}
  </div>;
}

// `.txt` notes stay plain text: no Markdown parsing, so saving never rewrites
// a character you didn't type.
const PlainText = React.forwardRef(function PlainText({ body, onChange }, ref) {
  const area = useRef(null);
  const fit = () => { const el = area.current; if (el) { el.style.height = 'auto'; el.style.height = el.scrollHeight + 'px'; } };
  useEffect(fit, []);
  React.useImperativeHandle(ref, () => ({
    focus: where => { const el = area.current; el?.focus(); if (el && where === 'start') el.setSelectionRange(0, 0); else if (el) el.setSelectionRange(el.value.length, el.value.length); },
  }), []);
  return <textarea ref={area} className="plain-body" defaultValue={body} spellCheck aria-label="Note body" placeholder="Write something…"
    onChange={e => { fit(); onChange(e.target.value); }} />;
});
