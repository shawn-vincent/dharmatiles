// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods.
//
//   connect({onSnapshot, onTile, onProgress, onState})  start listening
//   fetchMesh(index, draft?) -> ArrayBuffer the tile's binary mesh, or its last draft
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
    connect({ onSnapshot, onTile, onProgress, onState }) {
      const es = new EventSource('/api/events');
      es.onopen = () => onState('live');
      es.onerror = () => onState('reconnecting…');
      es.onmessage = (e) => {
        const m = JSON.parse(e.data);
        if (m.type === 'snapshot') onSnapshot(m);
        else if (m.type === 'tile') onTile(m.tile);
        else if (m.type === 'progress') onProgress(m.index, m.progress);
      };
    },
    async fetchMesh(index, draft = false) {
      const r = await fetch(`/api/tiles/${index}/mesh${draft ? '?draft=1' : ''}`, { cache: 'no-store' });
      if (!r.ok) throw new Error(`mesh: ${r.status}`);
      return r.arrayBuffer();
    },
    async build(index) {
      await fetch(`/api/tiles/${index}/build`, { method: 'POST' });
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
      const send = async () => fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      let r = await send();
      if (r.status === 403 && token) { token = null; r = await send(); } // the program restarted
      let v = null;
      try { v = await r.json(); } catch { /* not JSON */ }
      return { status: r.status, body: v };
    },
  };
}
