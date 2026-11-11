// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods; only this
// file knows the server's routes.  A tile is named by its id (the server's,
// never a position in a list).
//
//   connect({onSnapshot, onTile, onProgress, onState})  start listening; onState('live' | 'offline')
//   fetchMesh(id) -> ArrayBuffer            the tile's binary mesh
//   fetchSketch(id, sketch) -> ArrayBuffer  one stage's sketch while it builds (null once the build has moved on)
//   build(id)                               build a tile not built yet
//   text(id, file?) -> {file, text, writable}
//   elements() -> [element]                 the element schema
//
// The editor's calls, each -> {status, body} (status 0 when disconnected):
//   weightings(query)                       the weighting functions over a range, or one value, with the program's curves (a read; no token)
//   check(id, {text, file?})                what the text would build
//   checkValue(element, control, typed, format)  what the reader refuses in a typed value (read as the document's format): {message, label, help} (message null when it takes it)
//   draft(id, {text, file?})                a quick build of the text: {status, buffer} (the mesh), 409 when a newer draft took its place
//   save(id, {text, base, file?})           write the text to its file
//   rename(id, name)                        rename a tile's file: the new tile's {path, id}
//   mine(id)                                copy a library tile to My tiles: {path, id}
//   newTile(name, size)                     a new tile in My tiles: {path, id}
//   edit(body)                              a text edit (a control set, a node inserted or removed)
//   setTarget({system?, printer?})          choose the build target (base system, printer profile) for every build
//   exportTile(id, body)                    write STL / 3MF ({where: true} asks where it would go)
//   reveal(path)                            show a written file in the file manager
//
// The event stream: a snapshot, then one event per change, each with a
// revision.  When the stream drops it is opened again with the epoch (which
// run of the program) and the last revision seen: the program answers with
// what changed since, or a fresh snapshot.

// fetch rejects with a TypeError when it cannot reach the program (down, or
// the connection dropped): that is a lost connection, shown by the chip, and
// the page repeats what it needs on reconnect.  Any other failure is not.
export const isOffline = (e) => e instanceof TypeError;
const offline = isOffline;

// What an error reply says: the program's `{error}`, or its plain words.
const reasonOf = async (r) => {
  const raw = await r.text();
  try { return JSON.parse(raw).error || raw; } catch { return raw || `the program answered ${r.status}`; }
};

export function browserHost() {
  let token = null;
  const session = async () => {
    if (!token) {
      const r = await fetch('/api/session', { cache: 'no-store' });
      if (!r.ok) throw new Error(`session: ${await reasonOf(r)}`);
      token = (await r.json()).token;
    }
    return token;
  };
  // One editor call: JSON in, {status, body} out (or, for a draft, the
  // bytes), with the session token.  A reply that is not JSON has its words
  // as the body's error.  Only a lost connection is status 0.
  const call = async (path, body, bytes = false) => {
    try {
      const r = await fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      if (bytes && r.ok) return { status: r.status, buffer: await r.arrayBuffer() };
      const raw = await r.text();
      try { return { status: r.status, body: JSON.parse(raw) }; } catch (e) {
        if (r.ok) throw new Error(`${path}: the program answered ${r.status} with something that is not JSON`, { cause: e });
        return { status: r.status, body: { error: raw || `the program answered ${r.status}` } };
      }
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  // One read of the program with a query: {status, body}, as a call is.
  const read = async (path, query) => {
    try {
      const r = await fetch(`${path}?${new URLSearchParams(query)}`, { cache: 'no-store' });
      const raw = await r.text();
      try { return { status: r.status, body: JSON.parse(raw) }; } catch (e) {
        if (r.ok) throw new Error(`${path}: the program answered ${r.status} with something that is not JSON`, { cause: e });
        return { status: r.status, body: { error: raw || `the program answered ${r.status}` } };
      }
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
    connect({ onSnapshot, onTile, onProgress, onState }) {
      let epoch = null, rev = null;
      const open = () => {
        const q = epoch != null && rev != null ? `?epoch=${epoch}&since=${rev}` : '';
        const es = new EventSource(`/api/events${q}`);
        es.onmessage = (e) => {
          const m = JSON.parse(e.data);
          if (m.type !== 'progress' && m.rev != null) rev = m.rev;
          if (m.type === 'snapshot') { epoch = m.epoch; onState('live'); onSnapshot(m); }
          else if (m.type === 'resumed') onState('live');
          else if (m.type === 'tile') onTile(m.tile);
          else if (m.type === 'progress') onProgress(m.id, m.progress);
        };
        es.onerror = () => { es.close(); token = null; onState('offline'); setTimeout(open, 1000); };
      };
      open();
    },
    async fetchMesh(id) {
      const r = await fetch(tile(id, 'mesh'), { cache: 'no-store' });
      if (!r.ok) throw new Error(`mesh: ${await reasonOf(r)}`);
      return r.arrayBuffer();
    },
    async fetchSketch(id, sketch) {
      const r = await fetch(tile(id, `sketch?id=${sketch}`), { cache: 'no-store' });
      if (r.status === 404) return null; // the build moved on: its sketches are gone
      if (!r.ok) throw new Error(`sketch: ${await reasonOf(r)}`);
      return r.arrayBuffer();
    },
    async build(id) {
      let r;
      try { r = await fetch(tile(id, 'build'), { method: 'POST' }); } catch (e) {
        if (offline(e)) return; // offline: asked again on reconnect
        throw e;
      }
      if (r.status >= 500) throw new Error(`build: ${await reasonOf(r)}`);
    },
    async text(id, file) {
      const q = file ? `?file=${encodeURIComponent(file)}` : '';
      const r = await fetch(tile(id, `text${q}`), { cache: 'no-store' });
      if (r.status === 404) return null; // no such tile or file (any more)
      if (!r.ok) throw new Error(`text: ${await reasonOf(r)}`);
      return r.json();
    },
    async about() {
      const r = await fetch('/api/about', { cache: 'no-store' });
      if (!r.ok) throw new Error(`about: ${await reasonOf(r)}`);
      return r.json();
    },
    async elements() {
      const r = await fetch('/api/elements', { cache: 'force-cache' });
      if (!r.ok) throw new Error(`elements: ${await reasonOf(r)}`);
      return r.json();
    },
    check: (id, body) => call(tile(id, 'check'), body),
    weightings: (query) => read('/api/weightings', query),
    checkValue: (element, control, typed, format = 0) => call('/api/check-value', { element, control, typed, format }),
    draft: (id, body) => call(tile(id, 'draft'), body, true),
    save: (id, body) => call(tile(id, 'save'), body),
    rename: (id, name) => call(tile(id, 'rename'), { name }),
    mine: (id) => call(tile(id, 'mine'), {}),
    newTile: (name, size) => call('/api/new', { name, size }),
    edit: (body) => call('/api/edit', body),
    setTarget: (body) => call('/api/target', body),
    exportTile: (id, body) => call(tile(id, 'export'), body),
    reveal: (path) => call('/api/reveal', { path }),
  };
}
