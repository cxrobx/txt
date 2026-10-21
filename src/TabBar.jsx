import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, Plus, X, Check } from 'lucide-react';

// Onyx's tab bar (ask-widget src/onyx/tabs_ui.py), minus its auto-hide and
// split view: here it is the window's title bar, always showing.
// - Pills are 180 px and give way down to 88 px. Past that, pills spill into
//   the ⌄ list, furthest from the active tab first (fit).
// - Hovering a pill whose title is cut widens it toward its full title (360 px
//   at most) while the others give way in proportion to what each has above
//   88 px; a title still cut scrolls to its end and back. The strip's width is
//   held from the first real move until the fall-back has finished, and only a
//   mousemove at a new spot picks a pill, so a pill sliding under a still
//   pointer is never chosen.
// - Drag a pill 4 px to reorder; the others slide aside. Middle-click closes.
// Widths and `hidden` are set on the DOM directly, never through React props,
// so a re-render doesn't undo them.
const TAB_MIN = 88, TAB_MAX = 360;
const GROW = [200, 'cubic-bezier(.2,.8,.2,1)'], FALL = [320, 'cubic-bezier(.4,0,.2,1)'];
const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export default function TabBar({ tabs, activeKey, label, onActivate, onClose, onNew, onMove, onCloseOthers, children }) {
  const strip = useRef(null);
  const listButton = useRef(null);
  const [menu, setMenu] = useState(false);
  const [more, setMore] = useState(0);
  const s = useRef({ hover: null, rest: null, pt: null, release: 0, drag: null, dropped: false }).current;

  const pills = () => [...(strip.current?.children || [])].filter(p => p.classList.contains('tab'));
  const shown = () => pills().filter(p => !p.hidden);

  // ── Fit ───────────────────────────────────────────────────────────────────
  const fit = () => {
    const el = strip.current; if (!el) return;
    const ps = pills(), at = ps.findIndex(p => p.getAttribute('aria-selected') === 'true');
    let lo = 0, hi = ps.length - 1;
    for (const p of ps) p.hidden = false;
    while (el.scrollWidth > el.clientWidth + 1 && lo < hi) { if (at - lo > hi - at) ps[lo++].hidden = true; else ps[hi--].hidden = true; }
    setMore(ps.filter(p => p.hidden).length);
  };

  // ── Hover-widen ───────────────────────────────────────────────────────────
  const hold = () => {
    clearTimeout(s.release); if (s.rest) return;
    const ps = shown(), ws = ps.map(p => p.getBoundingClientRect().width);
    strip.current.style.width = getComputedStyle(strip.current).width;
    strip.current.classList.add('tabs-held');
    s.rest = new Map(ps.map((p, i) => [p, ws[i]]));
    ps.forEach((p, i) => { p.style.width = ws[i] + 'px'; });
  };
  const free = () => {
    clearTimeout(s.release); if (!s.rest) return;
    for (const p of s.rest.keys()) { p._grow?.cancel(); p._grow = null; p.style.width = ''; }
    strip.current?.classList.remove('tabs-held');
    if (strip.current) strip.current.style.width = '';
    s.rest = null;
  };
  const chrome = p => p.getBoundingClientRect().width - p.querySelector('.tab-title').getBoundingClientRect().width;
  const widths = p => {
    const text = p.querySelector('.tab-text'), rest = s.rest.get(p);
    if (!text || rest == null) return null;
    const natural = text.getBoundingClientRect().width + chrome(p) + 2;
    if (natural <= rest + 1) return null;
    let total = 0, slack = 0;
    for (const [q, w] of s.rest) { total += w; if (q !== p) slack += w - TAB_MIN; }
    const want = Math.min(TAB_MAX, natural, total - (s.rest.size - 1) * TAB_MIN);
    if (want <= rest + 1) return null;
    const give = want - rest;
    return new Map([...s.rest].map(([q, w]) => [q, q === p ? want : w - give * (w - TAB_MIN) / slack]));
  };
  const tween = (ws, [ms, ease]) => {
    const ps = [...s.rest.keys()], from = ps.map(p => p.getBoundingClientRect().width);
    if (still()) ms = 0;
    ps.forEach((p, i) => {
      const to = ws ? ws.get(p) : s.rest.get(p);
      p._grow?.cancel(); p.style.width = to + 'px';
      p._grow = ms && Math.abs(from[i] - to) > .5 ? p.animate([{ width: from[i] + 'px' }, { width: to + 'px' }], { duration: ms, easing: ease }) : null;
    });
  };
  const roll = (p, w, delay) => {
    const title = p.querySelector('.tab-title'), text = p.querySelector('.tab-text');
    const over = text.getBoundingClientRect().width - (w - chrome(p));
    clearTimeout(p._roll);
    if (over <= 1 || still()) { title.classList.remove('tab-rolling'); return; }
    const go = () => {
      if (!p.classList.contains('tab-wide')) return;
      title.style.setProperty('--roll', (-over - 4) + 'px');
      title.style.setProperty('--roll-dur', Math.max(2.5, over / 30 + 1.5).toFixed(1) + 's');
      title.classList.add('tab-rolling');
    };
    if (delay) p._roll = setTimeout(go, delay); else go();
  };
  const unwide = (p, instant) => {
    if (!p.classList.contains('tab-wide')) return;
    p.classList.remove('tab-wide'); p.title = p.getAttribute('aria-label') || ''; clearTimeout(p._roll);
    const title = p.querySelector('.tab-title'), text = p.querySelector('.tab-text');
    if (!title.classList.contains('tab-rolling')) return;
    const at = getComputedStyle(text).transform;
    title.classList.remove('tab-rolling');
    if (instant || still() || at === 'none') return;
    title.classList.add('tab-unrolling');
    text.animate([{ transform: at }, { transform: 'none' }], { duration: FALL[0], easing: FALL[1] }).onfinish = () => title.classList.remove('tab-unrolling');
  };
  const pick = (p, instant) => {
    hold(); s.hover = p;
    const ws = widths(p);
    for (const q of s.rest.keys()) if (q !== p || !ws) unwide(q, instant);
    tween(ws, instant ? [0] : ws ? GROW : FALL);
    if (!ws) return;
    const was = p.classList.contains('tab-wide');
    p.classList.add('tab-wide'); p.removeAttribute('title');
    roll(p, ws.get(p), was ? 0 : GROW[0] + 20);
  };
  const leave = () => {
    if (s.drag?.on) return;
    s.hover = null; s.pt = null; if (!s.rest) return;
    for (const q of s.rest.keys()) unwide(q);
    tween(null, FALL);
    clearTimeout(s.release); s.release = setTimeout(() => { if (!s.hover) free(); }, FALL[0] + 40);
  };
  const settle = () => { for (const q of strip.current?.querySelectorAll('.tab.tab-wide') || []) unwide(q, true); free(); s.hover = null; };

  // Tabs added, closed or renamed: back to rest, then fit again. The pill
  // under a still pointer keeps its place, measured again.
  useLayoutEffect(() => {
    const keep = s.hover && s.hover.isConnected ? s.hover : null;
    free(); fit();
    if (keep && !keep.hidden) pick(keep, true); else s.hover = null;
  }, [tabs.map(t => `${t.key}:${label(t)}`).join('|'), activeKey]);
  useEffect(() => {
    const ro = new ResizeObserver(() => { settle(); fit(); });
    ro.observe(strip.current.closest(".tab-bar"));
    return () => ro.disconnect();
  }, []);

  // ── Drag to reorder ───────────────────────────────────────────────────────
  const dragStart = () => {
    const d = s.drag, vis = shown();
    d.vis = vis; d.at = vis.indexOf(d.p); d.to = d.at;
    d.rects = vis.map(q => q.getBoundingClientRect());
    d.step = d.rects[d.at].width + parseFloat(getComputedStyle(strip.current).columnGap || '0');
    for (const q of s.rest ? s.rest.keys() : []) unwide(q, true);
    d.on = true; strip.current.classList.add('tabs-dragging'); d.p.classList.add('tab-drag');
    try { strip.current.setPointerCapture(d.pointer); } catch {}
  };
  const onPointerDown = e => {
    if (e.button !== 0 || s.drag?.on || e.target.closest('.tab-x')) return;
    const p = e.target.closest('.tab');
    if (p && !p.hidden) s.drag = { p, key: +p.dataset.key, pointer: e.pointerId, x0: e.clientX, y0: e.clientY, on: false };
  };
  const onPointerMove = e => {
    const d = s.drag; if (!d || e.pointerId !== d.pointer) return;
    if (!d.on) { if (Math.abs(e.clientX - d.x0) < 4) return; dragStart(); }
    const r = d.rects, me = r[d.at];
    const dx = Math.min(Math.max(e.clientX - d.x0, r[0].left - me.left), r[r.length - 1].right - me.right), mid = me.left + me.width / 2 + dx;
    d.p.style.transform = `translateX(${dx}px)`;
    let to = d.at;
    r.forEach((q, i) => { const c = q.left + q.width / 2; if (i > d.at && mid >= c) to = i; if (i < d.at && mid <= c && to >= d.at) to = i; });
    d.to = to;
    d.vis.forEach((q, i) => { if (q === d.p) return; const sh = i > d.at && i <= to ? -d.step : i < d.at && i >= to ? d.step : 0; q.style.transform = sh ? `translateX(${sh}px)` : ''; });
  };
  const dragEnd = e => {
    const d = s.drag; if (!d || (e && e.pointerId !== d.pointer)) return;
    s.drag = null; if (!d.on) return;
    const drop = e?.type === 'pointerup', from = d.p.getBoundingClientRect().left;
    try { strip.current.releasePointerCapture(d.pointer); } catch {}
    strip.current.classList.remove('tabs-dragging');
    for (const q of d.vis) q.style.transform = '';
    d.p.classList.remove('tab-drag');
    if (drop) { s.dropped = true; setTimeout(() => { s.dropped = false; }); }
    if (drop && d.to !== d.at) onMove(d.key, +d.vis[d.to].dataset.key, d.to > d.at);
    // The pill glides the last of the way into its place.
    requestAnimationFrame(() => {
      const p = strip.current?.querySelector(`[data-key="${d.key}"]`), gap = p ? from - p.getBoundingClientRect().left : 0;
      if (p && Math.abs(gap) > .5 && !still()) p.animate([{ transform: `translateX(${gap}px)` }, { transform: 'none' }], { duration: 150, easing: GROW[1] });
    });
  };

  useEffect(() => {
    if (!menu) return;
    const off = e => { if (!e.target.closest?.('.tab-menu, .tab-list-button')) setMenu(false); };
    const esc = e => { if (e.key === 'Escape') { e.preventDefault(); setMenu(false); } };
    window.addEventListener('mousedown', off); window.addEventListener('keydown', esc, true);
    return () => { window.removeEventListener('mousedown', off); window.removeEventListener('keydown', esc, true); };
  }, [menu]);

  const listLabel = more ? `All tabs (${more} more)` : 'All tabs';
  return <header className="tab-bar" data-tauri-drag-region>
    <div className="tab-group">
      <div ref={strip} className="tab-strip" role="tablist"
        onMouseMove={e => {
          if (s.drag?.on) return;
          if (s.pt && s.pt.x === e.clientX && s.pt.y === e.clientY) return;
          s.pt = { x: e.clientX, y: e.clientY };
          const p = e.target.closest('.tab');
          if (p && p !== s.hover && !p.hidden) pick(p);
        }}
        onMouseLeave={leave}
        onPointerDown={onPointerDown} onPointerMove={onPointerMove}
        onPointerUp={dragEnd} onPointerCancel={dragEnd} onLostPointerCapture={dragEnd}
        onMouseDown={e => { if (e.button === 1) e.preventDefault(); }}>
        {tabs.map(tab => {
          const on = tab.key === activeKey, l = label(tab);
          return <div key={tab.key} data-key={tab.key} role="tab" aria-selected={on} aria-label={l} title={l} className="tab"
            onClick={e => { if (s.dropped) return; if (!e.target.closest('.tab-x')) onActivate(tab.key); }}
            onAuxClick={e => { if (e.button === 1) { e.preventDefault(); onClose(tab.key); } }}>
            <span className="tab-title"><span className="tab-text">{l}</span></span>
            {tab.status !== 'saved' && <i className={`tab-dot ${tab.status}`} aria-label={tab.status} />}
            <button className="tab-x" tabIndex={-1} aria-label={`Close ${l}`} title="Close tab" onClick={e => { e.stopPropagation(); onClose(tab.key); }}><X /></button>
          </div>;
        })}
      </div>
      <button className="tab-btn" aria-label="New tab (⌘T)" title="New tab (⌘T)" onClick={onNew}><Plus /></button>
      <button ref={listButton} className="tab-btn tab-list-button" aria-label={listLabel} title={listLabel} aria-expanded={menu} onClick={() => setMenu(m => !m)}>
        <ChevronDown />{more > 0 && <span className="tab-more">{more}</span>}
      </button>
    </div>
    {children}
    {menu && <ul className="tab-menu" role="menu" style={{ left: Math.min(listButton.current.getBoundingClientRect().left, innerWidth - 268) }}>
      {tabs.map(tab => <li key={tab.key} role="menuitemradio" aria-checked={tab.key === activeKey} onClick={() => { setMenu(false); onActivate(tab.key); }}>
        <span className="tab-menu-check">{tab.key === activeKey && <Check />}</span><span className="tab-menu-label">{label(tab)}</span>
      </li>)}
      <li className="tab-menu-sep" role="separator" />
      <li role="menuitem" aria-disabled={tabs.length < 2} onClick={() => { if (tabs.length > 1) { setMenu(false); onCloseOthers(); } }}>
        <span className="tab-menu-check" /><span className="tab-menu-label">Close Other Tabs</span>
      </li>
    </ul>}
  </header>;
}
