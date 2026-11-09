// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods; only this
// file knows the server's routes.  A tile is named by its id (the server's,
// never a position in a list).
//
//   connect({onSnapshot, onTile, onProgress, onState})  start listening; onState('live' | 'offline')
//   fetchMesh(id) -> ArrayBuffer            the tile's binary mesh
//   fetchSketch(id, sketch) -> ArrayBuffer  one stage's sketch while it builds
//   build(id)                               build a tile not built yet
//   text(id, file?) -> {file, text, writable}
//   elements() -> [element]                 the element schema
//
// The editor's calls, each -> {status, body} (status 0 when disconnected):
//   check(id, {text, file?})                what the text would build
//   checkValue(element, control, typed)     what the reader refuses in a typed value: {message, label, help} (message null when it takes it)
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

export function browserHost() {
  let token = null;
  const session = async () => {
    if (!token) token = (await (await fetch('/api/session', { cache: 'no-store' })).json()).token;
    return token;
  };
  // One editor call: JSON in, {status, body} out (or, for a draft, the
  // bytes), with the session token.
  const call = async (path, body, bytes = false) => {
    try {
      const r = await fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      if (bytes && r.ok) return { status: r.status, buffer: await r.arrayBuffer() };
      let v = null;
      try { v = await r.json(); } catch { /* not JSON */ }
      return { status: r.status, body: v };
    } catch {
      return { status: 0, body: { error: 'the editor is disconnected' } };
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
      if (!r.ok) throw new Error(`mesh: ${r.status}`);
      return r.arrayBuffer();
    },
    async fetchSketch(id, sketch) {
      const r = await fetch(tile(id, `sketch?id=${sketch}`), { cache: 'no-store' });
      if (!r.ok) throw new Error(`sketch: ${r.status}`);
      return r.arrayBuffer();
    },
    async build(id) {
      await fetch(tile(id, 'build'), { method: 'POST' }).catch(() => {}); // offline: asked again on reconnect
    },
    async text(id, file) {
      const q = file ? `?file=${encodeURIComponent(file)}` : '';
      const r = await fetch(tile(id, `text${q}`), { cache: 'no-store' });
      return r.ok ? r.json() : null;
    },
    async elements() {
      const r = await fetch('/api/elements', { cache: 'force-cache' });
      return r.ok ? r.json() : [];
    },
    check: (id, body) => call(tile(id, 'check'), body),
    checkValue: (element, control, typed) => call('/api/check-value', { element, control, typed }),
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
