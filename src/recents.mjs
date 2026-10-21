// What the empty ⌘P switcher lists: notes closed lately
// (newest first, only ones that still exist and aren't open in this window),
// then everything else by last edit.
export function recentRows(notes, closed, openIds = new Set(), { closedLimit = 10, recentLimit = 60 } = {}) {
  const byId = new Map(notes.map(n => [n.id, n]));
  const shut = [];
  for (const c of closed) {
    const note = byId.get(c.id);
    if (note && !openIds.has(c.id) && shut.length < closedLimit) shut.push({ note, closedAt: c.closed_at * 1000 });
  }
  const skip = new Set(shut.map(r => r.note.id));
  const recent = notes.filter(n => !skip.has(n.id)).sort((a, b) => b.modified - a.modified).slice(0, recentLimit);
  return { closed: shut, recent };
}
