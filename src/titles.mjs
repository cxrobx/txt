// Mirrors notes.rs `first_line_title`, so a tab's label matches what the file
// list will say once the note is saved.
const prefixes = ['#', '-', '*', '+', '>', '[ ]', '[x]', '[X]'];

export function cleanLine(line) {
  let s = line.trim();
  for (const prefix of prefixes) while (s.startsWith(prefix)) s = s.slice(prefix.length).trimStart();
  return s.replaceAll('**', '').replaceAll('__', '').replaceAll('`', '');
}

export function derivedTitle(body, fallback = 'Untitled') {
  const line = (body || '').split('\n').map(cleanLine).find(Boolean);
  if (!line) return fallback;
  return line.length > 60 ? line.slice(0, 60).trimEnd() + '…' : line;
}

export function relativeTime(ms, now = Date.now()) {
  const minutes = Math.round((now - ms) / 60000);
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d`;
  return new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric', ...(days > 330 ? { year: 'numeric' } : {}) });
}
