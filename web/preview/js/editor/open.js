// Opening a tile: choosing it, and keeping the 3D view on it: its plan, its
// mesh (or a draft in its place), its camera.  Follows the server store (the
// tile's builds) and the view store (which tile).

import { meshIndex, parsePayload, piecesByKey } from '../payload.js';
import { isOffline } from '../host.js';
import { showError } from '../problems.js';
import { host, pickView, viewer } from './app.js';
import { beginFollow, endFollow, followBuild } from './build.js';
import { canLeave, supersedeDrafts } from './document.js';
import { inRoot, isEntry, server, stackIndex, stem, tile, tileById, view } from './stores.js';

// ---- choosing a tile ---------------------------------------------------

const cameras = new Map(); // tile path -> its saved camera and active preset
let at = null;             // the tile the view is set up for
let started = false;       // the first list of tiles has been looked at

/** Show tile `id` (null: none); false if unsaved changes were kept instead. */
export async function select(id, guarded = false) {
  const cur = view.get();
  if (!guarded && id !== cur.tileId && !(await canLeave())) return false;
  const t = tileById(id);
  if (cur.tileId != null && viewer.payload && tile()) {
    cameras.set(tile().path, { camera: viewer.getCamera(), activeView: view.get().activeView });
  }
  const to = t ? t.id : null;
  view.set(to === cur.tileId ? {} : { tileId: to, sel: null, hov: null, activeView: null, textFile: null });
  return true;
}

/** A plan that differs from the one the view has. */
const planChanged = (p) => !!p && JSON.stringify(p) !== JSON.stringify(viewer.plan);

/** The view starts afresh on tile `t` (or on none). */
function reset(t) {
  viewer.clear();
  viewer.setPlan(null);
  view.set({ shown: null, draft: null });
  endFollow(null);
  if (!t) {
    history.replaceState(null, '', `#root=${view.get().root}`);
    return;
  }
  view.set({ root: t.root });
  history.replaceState(null, '', `#tile=${encodeURIComponent(t.path)}`);
  // no mesh to frame yet and no view of yours kept: follow the build
  if (t.mesh_version == null && !cameras.get(t.path)) beginFollow(t.id);
  if (t.plan) viewer.setPlan(t.plan);
  followBuild();
}

/** The first list: the tile in the address, else the first root with tiles. */
function pickFirst() {
  const { tiles, roots } = server.get();
  const want = decodeURIComponent((location.hash.match(/tile=([^&]+)/) || [])[1] || '');
  const wantRoot = (location.hash.match(/root=(\d+)/) || [])[1];
  const wanted = want ? tiles.find((t) => t.path === want) : null;
  if (wanted) return select(wanted.id, true);
  const all = roots.length ? roots.map((_, i) => i) : [0];
  const root = wantRoot != null && roots[wantRoot] ? Number(wantRoot) : (all.find((r) => inRoot(r).length) ?? all[0]);
  view.set({ root });
  return select(inRoot(root)[0]?.id ?? null, true);
}

/** The tile shown, kept in step with what the server says of it. */
function follow() {
  const { ready, epoch } = server.get(), { tileId } = view.get();
  if (!ready) return;
  if (!started) { started = true; pickFirst(); return; }
  const t = tile();
  const first = inRoot(view.get().root)[0];
  // the tile shown was deleted, or none was and one has come: the first one left in its root
  if ((tileId != null && !t) || (tileId == null && first)) { select(first?.id ?? null, true); return; }
  if (tileId !== at) { at = tileId; reset(t); }
  if (!t) return;
  // a rescan or a reconnect: keep the view, catch up on a newer plan or mesh
  if (planChanged(t.plan)) { viewer.setPlan(t.plan); followBuild(); }
  const shown = view.get().shown;
  if (t.mesh_version != null && !(shown && shown.id === t.id && shown.epoch === epoch && shown.version === t.mesh_version)) loadMesh(t);
}

let loadToken = 0;
async function loadMesh(t) {
  const token = ++loadToken, { epoch } = server.get(), version = t.mesh_version;
  try {
    const buf = await host.fetchMesh(t.id);
    if (token !== loadToken || t.id !== view.get().tileId) return;
    const first = !viewer.payload;
    const p = parsePayload(buf);
    viewer.setMesh(p); // the new mesh swaps in place: the camera is kept
    supersedeDrafts();
    view.set({ pieces: piecesByKey(p), shown: { id: t.id, epoch, version }, draft: null });
    if (first) {
      const saved = cameras.get(t.path);
      if (saved) {
        viewer.setCamera(saved.camera);
        view.set({ activeView: saved.activeView });
      } else pickView(view.get().mode, endFollow(t.id));
    }
    endFollow(t.id);
    // a selection that neither the text nor the new build has goes
    const sel = view.get().sel;
    if (isEntry(sel) && stackIndex(sel.id) < 0 && meshIndex(p, sel.id) < 0) view.set({ sel: null });
  } catch (e) {
    if (!isOffline(e)) showError(`Could not show ${stem(t)}`, e); // offline: the chip says so, and the mesh is fetched again
  }
}

/** A draft of the text the page holds stands in for the build's mesh. */
function showDraft() {
  const d = view.get().draft;
  if (!d || d.tileId !== view.get().tileId) return;
  viewer.setMesh(d.payload);
  view.set({ pieces: piecesByKey(d.payload) });
}

/** Wire this part of the page (once, at load). */
export function init() {
  view.subscribe(['tileId'], follow);
  server.subscribe(['tiles', 'ready', 'epoch'], follow);
  view.subscribe(['draft'], showDraft);
}
