// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods.
//
//   connect({onSnapshot, onTile, onProgress, onState})  start listening; onState('live' | 'offline')
//   fetchMesh(index, draft?) -> ArrayBuffer the tile's binary mesh, or its last draft
//   fetchSketch(index, id) -> ArrayBuffer   one stage's sketch while it builds
//   build(index)                            build a tile not built yet
//   text(index, file?) -> {file, text, writable}
//   elements() -> [element]                 the element schema
//   post(path, body) -> {status, body}      an editor call (check, edit, save, mine)

export function browserHost() {
  let token = null;
  const session = async () => {
    if (!token) token = (await (await fetch('/api/session', { cache: 'no-store' })).json()).token;
    return token;
  };
  return {
    // One loop: the stream is live once its snapshot is in; when it drops
    // or is refused, it is closed and opened again a second later, for as
    // long as the page is open.  The server back may be a new one, with a
    // new token: it is fetched afresh.
    connect({ onSnapshot, onTile, onProgress, onState }) {
      const open = () => {
        const es = new EventSource('/api/events');
        es.onmessage = (e) => {
          const m = JSON.parse(e.data);
          if (m.type === 'snapshot') { onState('live'); onSnapshot(m); }
          else if (m.type === 'tile') onTile(m.tile);
          else if (m.type === 'progress') onProgress(m.index, m.progress);
        };
        es.onerror = () => { es.close(); token = null; onState('offline'); setTimeout(open, 1000); };
      };
      open();
    },
    async fetchMesh(index, draft = false) {
      const r = await fetch(`/api/tiles/${index}/mesh${draft ? '?draft=1' : ''}`, { cache: 'no-store' });
      if (!r.ok) throw new Error(`mesh: ${r.status}`);
      return r.arrayBuffer();
    },
    async fetchSketch(index, id) {
      const r = await fetch(`/api/tiles/${index}/sketch?id=${id}`, { cache: 'no-store' });
      if (!r.ok) throw new Error(`sketch: ${r.status}`);
      return r.arrayBuffer();
    },
    async build(index) {
      await fetch(`/api/tiles/${index}/build`, { method: 'POST' }).catch(() => {}); // offline: asked again on reconnect
    },
    async text(index, file) {
      const q = file ? `?file=${encodeURIComponent(file)}` : '';
      const r = await fetch(`/api/tiles/${index}/text${q}`, { cache: 'no-store' });
      return r.ok ? r.json() : null;
    },
    async elements() {
      const r = await fetch('/api/elements', { cache: 'force-cache' });
      return r.ok ? r.json() : [];
    },
    async post(path, body) {
      try {
        const send = async () => fetch(path, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
          body: JSON.stringify(body),
        });
        const r = await send();
        let v = null;
        try { v = await r.json(); } catch { /* not JSON */ }
        return { status: r.status, body: v };
      } catch {
        return { status: 0, body: { error: 'the editor is disconnected' } };
      }
    },
  };
}
