const documentExtension = /\.(?:md|markdown|html?|txt|pdf)$/i;
const controls = /[\u0000-\u001f\u007f]/;
const uuid = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i;
const boardId = /^b_[a-f0-9]{1,32}$/;
const noteId = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i;
function clean(value) {
  if (typeof value !== 'string' || value.length > 8192 || controls.test(value)) throw new Error('Invalid link.');
  return value.trim().replace(/^(["'])(.*)\1$/, '$2');
}
function documentPath(value) {
  if (!(value.startsWith('/') && !value.startsWith('//') || value.startsWith('~/')) || !documentExtension.test(value) || controls.test(value)) throw new Error('Use a local Markdown, text, HTML, or PDF document path.');
  return value;
}
export function fileLink(value) {
  const path = documentPath(clean(value));
  return path.startsWith('~/') ? path : `file://${path.split('/').map(encodeURIComponent).join('/')}`;
}
export function taskLink(value) {
  const text = clean(value);
  const match = /^T([1-9]\d{0,14})$/.exec(text);
  if (!match) throw new Error('Use a task reference such as T42.');
  return `cxtasks://task/T${match[1]}`;
}
function boardLink(id) { return { kind: 'board', href: `cxboards://b/${id}`, id }; }
// Only opening actions are accepted. A pasted URI cannot create/overwrite an
// Obsidian note, run a CXTasks command, or launch an arbitrary local file.
export function parseAppLink(value) {
  const text = clean(value);
  if (text.startsWith('/') || text.startsWith('~/')) {
    const path = documentPath(text);
    return { kind: 'document', path, href: fileLink(path) };
  }
  try { const href = taskLink(text); return { kind: 'task', href, reference: href.split('/').at(-1) }; } catch {}
  let url;
  try { url = new URL(text); } catch { throw new Error('Use a web link, document path, or task reference such as T42.'); }
  if (url.username || url.password) throw new Error('Links containing credentials are not supported.');
  if (url.protocol === 'txt:') {
    const id = url.pathname.replace(/^\/+|\/+$/g, '');
    if (url.hostname !== 'note' || !noteId.test(id)) throw new Error('Invalid txt note link.');
    return { kind: 'txt', href: `txt://note/${id.toLowerCase()}`, id: id.toLowerCase() };
  }
  if (url.protocol === 'margin:') return { kind: 'margin', href: text };
  if (url.protocol === 'file:') {
    if (url.hostname && url.hostname !== 'localhost' || url.search || url.hash) throw new Error('Use a local file link.');
    const path = documentPath(decodeURIComponent(url.pathname));
    return { kind: 'document', path, href: fileLink(path) };
  }
  if (url.protocol === 'cxtasks:') {
    const reference = url.pathname.slice(1).replace(/\/$/, '');
    if (url.hostname !== 'task' || url.port || !(/^T[1-9]\d{0,14}$/i.test(reference) || uuid.test(reference))) throw new Error('Invalid CXTasks task link.');
    const normalized = /^t/i.test(reference) ? 'T' + reference.slice(1) : reference;
    return { kind: 'task', href: `cxtasks://task/${normalized}`, reference: normalized };
  }
  if (url.protocol === 'cxboards:') {
    const id = url.pathname.slice(1).replace(/\/$/, '');
    if (url.hostname !== 'b' || url.port || !boardId.test(id)) throw new Error('Invalid CXBoards board link.');
    return boardLink(id);
  }
  if (url.protocol === 'obsidian:') {
    if (url.hostname !== 'open' || !['', '/'].includes(url.pathname) || url.port || url.hash || [...url.searchParams.keys()].some(key => !['path', 'vault', 'file'].includes(key))) throw new Error('Only Obsidian open links are supported.');
    const path = url.searchParams.get('path');
    const file = url.searchParams.get('file');
    const vault = url.searchParams.get('vault');
    if (path) documentPath(path);
    else if (!file || !vault || controls.test(file + vault)) throw new Error('Use an Obsidian link with a document path or vault and file.');
    return { kind: 'obsidian', href: url.href, ...(path ? { path } : {}) };
  }
  if (['http:', 'https:'].includes(url.protocol)) {
    // Onyx's copied reader URL carries the same local path as Copy Path.
    if (['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) && url.pathname === '/view' && url.searchParams.has('src')) {
      const path = documentPath(url.searchParams.get('src'));
      return { kind: 'document', path, href: fileLink(path) };
    }
    // CXBoards' "Copy link to board" gives its local server address.
    const board = /^\/b\/([^/]+)\/?$/.exec(url.pathname);
    if (['127.0.0.1', 'localhost'].includes(url.hostname) && url.port === '4870' && board && boardId.test(board[1])) return boardLink(board[1]);
    return { kind: 'web', href: text };
  }
  if (url.protocol === 'mailto:') return { kind: 'web', href: text };
  throw new Error('This link type is not supported.');
}
export function safeAppHref(value) {
  try { return parseAppLink(value).href; } catch { return ''; }
}
export function editableLink(value) {
  try { return parseAppLink(value).href; }
  catch (error) {
    const text = clean(value);
    if (/^(?:\d+|task\s*#?\s*\d+|#\d+|t\d+)$/i.test(text)) throw new Error('Use an uppercase T followed by a number, such as T42.');
    if (/^[a-z][a-z0-9+.-]*:/i.test(text) || text.startsWith('/') || text.startsWith('~/')) throw error;
    return parseAppLink('https://' + text).href;
  }
}
