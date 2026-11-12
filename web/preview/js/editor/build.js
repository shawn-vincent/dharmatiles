// The build as it goes: the last mesh fades and each stage's sketch stands in
// until the finished mesh lands, and the camera follows what is known of the
// tile's size meanwhile.  Follows the server store (the tile's status and
// sketches) and the view store (the mesh shown).

import { parsePayload } from '../payload.js';
import { isOffline } from '../host.js';
import { showError } from '../problems.js';
import { host, viewer } from './app.js';
import { server, stem, tile, view } from './stores.js';

// ---- the build as it goes ----------------------------------------------
// While the tile builds, the mesh shown is the last build's: it fades. Each
// stage's sketch (what it has made, or a rough stand-in) replaces it as the
// build sends them, faded too, until the finished mesh lands. A draft
// being dragged is shown instead of either.

const sketchCache = new Map(); // epoch:sketch id -> Promise of its payload, or null
let sketchGen = 0;
let asked = null; // the tile the program was last asked to build, with the run it was asked in

/** The version of the tile's mesh the view holds, or null. */
const shownVersion = (t) => {
  const s = view.get().shown;
  return s && s.id === t.id && s.epoch === server.get().epoch ? s.version : null;
};

let marksTold = null; // `tile|run|on`: what the program was last told of the debug layer

/**
 * Tell the program whether the debug layer is on for the tile shown: its
 * marks are collected only while it is.  Turned on, the tile is built
 * again with them; turned off, the program is told so and nothing is
 * rebuilt.
 */
function syncMarks() {
  const t = tile(), { live, epoch } = server.get(), on = view.get().debug;
  if (!live) marksTold = null;
  if (!t || !live) return;
  const key = `${t.id}|${epoch}|${on}`;
  if (marksTold === key) return;
  // a program that was never told it is on has nothing to switch off
  const was = marksTold?.startsWith(`${t.id}|${epoch}|`) && marksTold.endsWith('|true');
  marksTold = key;
  if (on || was) host.build(t.id, { debug: on }).catch((e) => showError('Could not tell the program about the debug layer', e));
}

function showBuild() {
  const t = tile(), { live, epoch } = server.get();
  if (!live) asked = null;
  if (!t) return;
  // the tile shown is built: asking again is harmless, and covers a server
  // that restarted or a request lost while it was down
  if (t.status === 'idle' && live && asked !== `${t.id}|${epoch}`) { asked = `${t.id}|${epoch}`; host.build(t.id); }
  const shown = shownVersion(t);
  const current = t.status === 'ready' && t.mesh_version === shown;
  // what is shown is not the tile as it is: a build is on its way, or the server is away
  viewer.setStale(shown != null && (!live || (!view.get().draft && !current)));
  syncSketches(t);
}

async function syncSketches(t) {
  // a finished mesh on its way: the sketches stay until it lands
  if (t.status === 'ready' && t.mesh_version != null && shownVersion(t) !== t.mesh_version) return;
  const gen = ++sketchGen, { epoch } = server.get();
  const list = view.get().draft ? [] : (t.sketches || []);
  const key = (k) => `${epoch}:${k.id}`;
  for (const k of list) {
    if (!sketchCache.has(key(k))) {
      // a failed fetch is not kept: the next sync asks again
      sketchCache.set(key(k), host.fetchSketch(t.id, k.id).then((buf) => (buf ? parsePayload(buf) : null)).catch((e) => {
        sketchCache.delete(key(k));
        if (!isOffline(e)) showError(`Could not load a sketch of ${stem(t)}`, e); // offline: asked again on the next sync
        return null;
      }));
    }
  }
  const got = await Promise.all(list.map(async (k) => ({ id: k.id, rough: k.rough, payload: await sketchCache.get(key(k)) })));
  if (gen !== sketchGen || t.id !== view.get().tileId) return; // a newer list came
  const keep = new Set(list.map(key));
  for (const k of sketchCache.keys()) if (!keep.has(k)) sketchCache.delete(k);
  viewer.setSketches(got.filter((k) => k.payload));
  followBuild();
}

// ---- the camera before a tile's mesh is in --------------------------------
// The view starts on a 1×1 tile, and while a tile builds it follows what is
// known of its size: the plan's footprint, then each sketch the build sends.
// The finished mesh frames it; moving the view yourself stops the following.

const ONE_SQUARE = { lo: [0, 0, 0], hi: [35, 35, 8] };
let following = null; // {id, b}: the tile followed and the bounds last framed

/** Follow tile `id`'s build (no mesh to frame yet and no view of yours kept for it). */
export function beginFollow(id) { following = { id, b: null }; }
/** Stop following; true if it was following tile `id`. */
export function endFollow(id) {
  const was = following?.id === id;
  if (was || id == null) following = null;
  return was;
}

export function followBuild() {
  const t = tile();
  if (!following || !t || following.id !== t.id) return;
  const [w, h] = t.plan?.size_mm || [35, 35];
  const b = { lo: [0, 0, 0], hi: [w, h, 8] };
  for (const k of viewer.sketchBounds?.() || []) {
    for (let a = 0; a < 3; a++) { b.lo[a] = Math.min(b.lo[a], k.lo[a]); b.hi[a] = Math.max(b.hi[a], k.hi[a]); }
  }
  // a small change is not worth a camera move
  const size = (x) => Math.hypot(...x.hi.map((v, a) => v - x.lo[a]));
  const was = following.b;
  if (was && Math.abs(size(b) - size(was)) < 0.08 * size(was)
    && b.lo.every((v, a) => Math.abs(v - was.lo[a]) < 2) && b.hi.every((v, a) => Math.abs(v - was.hi[a]) < 2)) return;
  following.b = b;
  viewer.setView(view.get().mode, true, b);
}

/** Wire this part of the page (once, at load). */
export function init() {
  viewer.setView(view.get().mode, false, ONE_SQUARE);
  // a finished mesh landing ends the sketches
  view.subscribe(['shown'], () => { sketchCache.clear(); viewer.setSketches([]); });
  view.subscribe(['tileId', 'shown', 'draft'], showBuild);
  server.subscribe(['tiles', 'live', 'epoch'], showBuild);
  view.subscribe(['debug', 'tileId'], syncMarks);
  server.subscribe(['live', 'epoch'], syncMarks);
  const user = viewer.onUserViewChange;
  viewer.onUserViewChange = () => { following = null; user?.(); };
}
