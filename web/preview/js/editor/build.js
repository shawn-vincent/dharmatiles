// The build as it goes: the last mesh fades and each stage's sketch stands in
// until the finished mesh lands.

import { parsePayload } from '../payload.js';
import { host, state, viewer } from './app.js';
import { drafts } from './document.js';
import { followBuild } from './open.js';

// ---- the build as it goes ----------------------------------------------
// While the tile builds, the mesh shown is the last build's: it fades. Each
// stage's sketch (what it has made, or a rough stand-in) replaces it as the
// build sends them, faded too, until the finished mesh lands. A draft
// being dragged is shown instead of either.

export const sketchCache = new Map(); // sketch id -> Promise of its payload, or null
let sketchGen = 0;
export function showBuild(t) {
  if (!t || t.index !== state.selected) return;
  // the tile shown is built: asking again is harmless, and covers a server
  // that restarted or a request lost while it was down
  if (t.status === 'idle' && state.live) host.build(t.index);
  const shown = state.loaded.get(t.index);
  const current = t.status === 'ready' && t.mesh_version === shown;
  // what is shown is not the tile as it is: a build is on its way, or the server is away
  viewer.setStale(shown != null && (!state.live || (!drafts.shown && !current)));
  syncSketches(t);
}
async function syncSketches(t) {
  // a finished mesh on its way: the sketches stay until it lands
  if (t.status === 'ready' && t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) return;
  const gen = ++sketchGen;
  const list = drafts.shown ? [] : (t.sketches || []);
  for (const k of list) {
    if (!sketchCache.has(k.id)) {
      // a failed fetch is not kept: the next sync asks again
      sketchCache.set(k.id, host.fetchSketch(t.index, k.id).then(parsePayload).catch(() => { sketchCache.delete(k.id); return null; }));
    }
  }
  const got = await Promise.all(list.map(async (k) => ({ id: k.id, rough: k.rough, payload: await sketchCache.get(k.id) })));
  if (gen !== sketchGen || t.index !== state.selected) return; // a newer list came
  const ids = new Set(list.map((k) => k.id));
  for (const id of sketchCache.keys()) if (!ids.has(id)) sketchCache.delete(id);
  viewer.setSketches(got.filter((k) => k.payload));
  followBuild();
}
