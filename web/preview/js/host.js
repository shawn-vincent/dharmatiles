// The browser host: how the viewer talks to `dharmatiles preview`.
// The Tauri app supplies another object with the same four methods.
//
//   connect({onSnapshot, onTile, onProgress, onState})  start listening
//   fetchMesh(index) -> ArrayBuffer         the tile's binary mesh
//   build(index)                            build a tile not built yet

export function browserHost() {
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
    async fetchMesh(index) {
      const r = await fetch(`/api/tiles/${index}/mesh`, { cache: 'no-store' });
      if (!r.ok) throw new Error(`mesh: ${r.status}`);
      return r.arrayBuffer();
    },
    async build(index) {
      await fetch(`/api/tiles/${index}/build`, { method: 'POST' });
    },
  };
}
