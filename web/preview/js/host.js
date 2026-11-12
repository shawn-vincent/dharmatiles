// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods; only this
// file knows the server's routes.  A tile is named by its id (the server's,
// never a position in a list).
//
//   connect({onSnapshot, onTile, onProgress, onMessage, onState})  start listening; onState('live' | 'offline'); onMessage(m) for each message of the stream
//   messagesText(id) -> string              the tile's open messages as text, the program's one renderer (Copy all)
//   reportMessage(body)                     a failure of the page's own, to join the stream: {tile?, what, text, help?, causes?, backtrace?}
//   fetchMesh(id) -> ArrayBuffer            the tile's binary mesh
//   fetchSketch(id, sketch) -> ArrayBuffer  one stage's sketch while it builds (null once the build has moved on)
//   build(id, {debug?})                     build a tile not built yet; `debug` says whether the debug layer is on for it (its marks are collected only then)
//   text(id, file?) -> {file, text, writable}
//   elements() -> [element]                 the element schema
//   language() -> {docs, nodes, units, functions, names}   the tile's nodes with their controls, and the language's words
//   names(text) -> {ok, defs, regions, lines, points}      the names the text makes, by the program's parser
//
// The editor's calls, each -> {status, body} (status 0 when disconnected):
//   weightings(query)                       the weighting functions over a range, or one value, with the program's curves (a read; no token)
//   check(id, {text, file?})                what the text would build
//   offers(id, {text, at, file?})            what the Add menu may offer at place `at` of the stack: {ok, offers: [{name, family, about, node, ok, why}]}
//   checkValue(element, control, typed)  what the reader refuses in a typed value: {message, label, help} (message null when it takes it)
//   draft(id, {text, file?, debug?})        a quick build of the text (with the debug layer's marks when `debug`): {status, buffer} (the mesh), 409 when a newer draft took its place
//   save(id, {text, base, file?})           write the text to its file
//   rename(id, name)                        rename a tile's file: the new tile's {path, id}
//   mine(id)                                copy a library tile to My tiles: {path, id}
//   deleteTile(id)                          delete a tile's file from a folder the preview lists (Undo of that copy)
//   newTile(name, size, into?)              a new tile in My tiles (or in the folder `into` {root, dir}): {path, id}
//   addPlace()                              show the program's folder chooser; the folder chosen is a place from now on: {added, root, label, snapshot}, or {cancelled: true}
//   removePlace(root)                       take an added place off the list (its folder is left alone)
//   newFolder(root, dir, name) / renameFolder(root, dir, name) / trashFolder(root, dir)   a folder of place `root`, named by its path below it
//   moveTile(id, {root, dir})               a tile into a folder   duplicateTile(id)   a copy beside it   trashTile(id)   to the Trash
//   undoTrash(n)                            put back what a trash answered `undo: n` for
//   (each of these answers with the new list, `snapshot`, to take at once)
//   edit(body)                              a text edit (a control set, a node inserted or removed)
//   setTarget({system?, printer?})          choose the build target (base system, printer profile) for every build
//   exportTile(id, body)                    write STL / 3MF ({where: true} asks where it would go)
//   reveal(path)                            show a written file in the file manager
//   platesShow(id, plate)                   the plate of a print list the view shows (from 0)
//   platesSet(id, key, value)               set a word of the list's `plates` line (printer, mode, group, gap, margin, turn); value null removes it
//   platesExport(id)                        build every piece in full and write the plates: {ok, files, format, plates, failures}
//
// The event stream: a snapshot, then one event per change, each with a
// revision.  When the stream drops it is opened again with the epoch (which
// run of the program) and the last revision seen: the program answers with
// what changed since, or a fresh snapshot.

import { serverErrorOf } from './server-error.js';

// fetch rejects with a TypeError when it cannot reach the program (down, or
// the connection dropped): that is a lost connection, shown by the chip, and
// the page repeats what it needs on reconnect.  Only that rejection, from
// fetch itself, is made an `Offline`; any other error, a TypeError from the
// page's own code included, is a failure and goes to the error panel.
export class Offline extends Error {
  constructor(cause) {
    super('the editor is disconnected', { cause });
    this.name = 'Offline';
  }
}
export const isOffline = (e) => e instanceof Offline;
const offline = isOffline;
/** `fetch`, whose network failure is an `Offline` and nothing else is. */
async function netFetch(...args) {
  try {
    return await fetch(...args);
  } catch (e) {
    if (e instanceof TypeError) throw new Offline(e);
    throw e;
  }
}

// What a failed reply says, as the page's own error for what it was doing
// (`what`), holding the program's ServerError as its cause: one error with
// both stacks.  A reply that is not the program's JSON has its words only.
const replyError = async (r, what) => {
  const raw = await r.text();
  let body = null;
  try { body = JSON.parse(raw); } catch { /* not JSON: serverErrorOf makes the error from the words */ }
  const se = serverErrorOf(r.status, body, raw);
  return new Error(`${what}: ${se.message}`, { cause: se });
};

// One reply to an editor call: {status, body}, and for a failure the
// program's ServerError as `error` (with body.error its words, for the
// places that say them beside what was asked).  Only a success may be
// something other than JSON.
const reply = async (r, path) => {
  const raw = await r.text();
  let body = null;
  try { body = JSON.parse(raw); } catch (e) {
    if (r.ok) throw new Error(`${path}: the program answered ${r.status} with something that is not JSON`, { cause: e });
  }
  const error = serverErrorOf(r.status, body, raw);
  if (!error) return { status: r.status, body };
  return { status: r.status, body: { ...(body && typeof body === 'object' ? body : {}), error: error.message }, error };
};

export function browserHost() {
  let token = null;
  const session = async () => {
    if (!token) {
      const r = await netFetch('/api/session', { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'session');
      token = (await r.json()).token;
    }
    return token;
  };
  // One editor call: JSON in, {status, body} out (or, for a draft, the
  // bytes), with the session token.  A reply that is not JSON has its words
  // as the body's error.  Only a lost connection is status 0.
  const call = async (path, body, bytes = false) => {
    try {
      const r = await netFetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      if (bytes && r.ok) return { status: r.status, buffer: await r.arrayBuffer() };
      return await reply(r, path);
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  // One read of the program with a query: {status, body}, as a call is.
  const read = async (path, query) => {
    try {
      const r = await netFetch(`${path}?${new URLSearchParams(query)}`, { cache: 'no-store' });
      return await reply(r, path);
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  const tile = (id, what) => `/api/tiles/${encodeURIComponent(id)}/${what}`;
  return {
    // One loop: the stream is live once its first message is in; when it
    // drops or is refused, it is closed and opened again a second later, for
    // as long as the page is open.  The server back may be a new one, with a
    // new token: it is fetched afresh.
    connect({ onSnapshot, onTile, onProgress, onMessage, onState }) {
      let epoch = null, rev = null;
      const open = () => {
        const q = epoch != null && rev != null ? `?epoch=${epoch}&since=${rev}` : '';
        const es = new EventSource(`/api/events${q}`);
        es.onmessage = (e) => {
          const m = JSON.parse(e.data);
          if (m.type !== 'progress' && m.type !== 'message' && m.rev != null) rev = m.rev;
          if (m.type === 'snapshot') { epoch = m.epoch; onState('live'); onSnapshot(m); }
          else if (m.type === 'resumed') onState('live');
          else if (m.type === 'tile') onTile(m.tile);
          else if (m.type === 'progress') onProgress(m.id, m.progress);
          else if (m.type === 'message') onMessage(m.message);
        };
        es.onerror = () => { es.close(); token = null; onState('offline'); setTimeout(open, 1000); };
      };
      open();
    },
    async fetchMesh(id) {
      const r = await netFetch(tile(id, 'mesh'), { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'mesh');
      return r.arrayBuffer();
    },
    async fetchSketch(id, sketch) {
      const r = await netFetch(tile(id, `sketch?id=${sketch}`), { cache: 'no-store' });
      if (r.status === 404) return null; // the build moved on: its sketches are gone
      if (!r.ok) throw await replyError(r, 'sketch');
      return r.arrayBuffer();
    },
    async build(id, { debug } = {}) {
      let r;
      const q = typeof debug === 'boolean' ? `?debug=${debug ? 1 : 0}` : '';
      try { r = await netFetch(tile(id, `build${q}`), { method: 'POST' }); } catch (e) {
        if (offline(e)) return; // offline: asked again on reconnect
        throw e;
      }
      if (r.status >= 500) throw await replyError(r, 'build');
    },
    async text(id, file) {
      const q = file ? `?file=${encodeURIComponent(file)}` : '';
      const r = await netFetch(tile(id, `text${q}`), { cache: 'no-store' });
      if (r.status === 404) return null; // no such tile or file (any more)
      if (!r.ok) throw await replyError(r, 'text');
      return r.json();
    },
    async about() {
      const r = await netFetch('/api/about', { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'about');
      return r.json();
    },
    async elements() {
      const r = await netFetch('/api/elements', { cache: 'force-cache' });
      if (!r.ok) throw await replyError(r, 'elements');
      return r.json();
    },
    async language() {
      const r = await netFetch('/api/language', { cache: 'force-cache' });
      if (!r.ok) throw await replyError(r, 'language');
      return r.json();
    },
    names: (text) => call('/api/names', { text }),
    check: (id, body) => call(tile(id, 'check'), body),
    offers: (id, body) => call(tile(id, 'offers'), body),
    weightings: (query) => read('/api/weightings', query),
    checkValue: (element, control, typed) => call('/api/check-value', { element, control, typed }),
    draft: (id, body) => call(tile(id, 'draft'), body, true),
    save: (id, body) => call(tile(id, 'save'), body),
    rename: (id, name) => call(tile(id, 'rename'), { name }),
    mine: (id) => call(tile(id, 'mine'), {}),
    deleteTile: (id) => call(tile(id, 'delete'), {}),
    reportMessage: (body) => call('/api/messages', body),
    async messagesText(id) {
      const r = await netFetch(`/api/messages?${new URLSearchParams({ tile: id, state: 'open', format: 'text' })}`, { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'messages');
      return r.text();
    },
    newTile: (name, size, into) => call('/api/new', { name, size, ...(into ? { root: into.root, dir: into.dir } : {}) }),
    addPlace: () => call('/api/places/add', {}),
    removePlace: (root) => call('/api/places/remove', { root }),
    newFolder: (root, dir, name) => call('/api/folders/new', { root, dir, name }),
    renameFolder: (root, dir, name) => call('/api/folders/rename', { root, dir, name }),
    trashFolder: (root, dir) => call('/api/folders/trash', { root, dir }),
    moveTile: (id, into) => call(tile(id, 'move'), { root: into.root, dir: into.dir }),
    duplicateTile: (id) => call(tile(id, 'duplicate'), {}),
    trashTile: (id) => call(tile(id, 'trash'), {}),
    undoTrash: (undo) => call('/api/trash/undo', { undo }),
    edit: (body) => call('/api/edit', body),
    setTarget: (body) => call('/api/target', body),
    exportTile: (id, body) => call(tile(id, 'export'), body),
    reveal: (path) => call('/api/reveal', { path }),
    platesShow: (id, plate) => call('/api/plates/show', { tile: id, plate }),
    platesSet: (id, key, value) => call('/api/plates/set', { tile: id, key, value }),
    platesExport: (id) => call('/api/plates/export', { tile: id }),
  };
}
