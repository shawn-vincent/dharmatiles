// Opening a tile: choosing it, the camera before its mesh is in, and loading
// the mesh.

import { NO_ENTRY, parsePayload } from '../payload.js';
import { $, host, isEntry, state, tile, viewer } from './app.js';
import { showBuild, sketchCache } from './build.js';
import { pickView, showActiveView } from './chrome.js';
import { canLeave, clearDocumentHistory, closeComp, drafts, stackItems, texts } from './document.js';
import { surfaceHeights } from './map.js';
import { lens, renderLens } from './sheet.js';
import { renderAll } from './title.js';

// ---- choosing a tile ---------------------------------------------------
// ---- the camera before a tile's mesh is in --------------------------------
// The view starts on a 1×1 tile, and while a tile builds it follows what is
// known of its size: the plan's footprint, then each sketch the build sends.
// The finished mesh frames it; moving the view yourself stops the following.

const ONE_SQUARE = { lo: [0, 0, 0], hi: [35, 35, 8] };
let following = null; // {i, b}: the tile followed and the bounds last framed
export function followBuild() {
  const t = tile();
  if (!following || !t || following.i !== t.index) return;
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
  viewer.setView(state.view, true, b);
}

export async function select(i, guarded = false) {
  if (!guarded && i !== state.selected && !(await canLeave())) return false;
  closeComp(); lens.hide();
  if (state.selected >= 0 && viewer.payload) {
    state.cameras.set(tile().path, { camera: viewer.getCamera(), activeView: state.activeView });
  }
  if (i !== state.selected) {
    state.sel = null; state.hov = null; state.activeView = null;
    $('tilebtn').classList.remove('sel');
    showActiveView(null);
  }
  state.selected = i;
  viewer.clear();
  viewer.setPlan(null);
  state.loaded.delete(i);
  const t = state.tiles[i];
  if (!t || t.gone) {
    state.selected = -1;
    history.replaceState(null, '', `#root=${state.root}`);
    texts.clear(); state.textFor = null; clearDocumentHistory();
    renderAll();
    return true;
  }
  state.root = t.root;
  history.replaceState(null, '', `#tile=${encodeURIComponent(t.path)}`);
  // no mesh to frame yet and no view of yours kept: follow the build
  following = t.mesh_version == null && !state.cameras.get(t.path) ? { i, b: null } : null;
  if (t.plan) viewer.setPlan(t.plan);
  followBuild();
  if (t.mesh_version != null) loadMesh(i, t.mesh_version);
  renderAll();
  // a build already running: what it has sent so far, not only what comes next
  showBuild(t);
  return true;
}

export function countPieces(p) {
  state.pieces = new Map();
  for (let t = 0; t < p.triangles; t++) {
    const e = p.entry[t];
    if (e !== NO_ENTRY) state.pieces.set(e, Math.max(state.pieces.get(e) || 0, p.piece[t] + 1));
  }
}

let loadToken = 0;
export async function loadMesh(i, version) {
  const token = ++loadToken;
  try {
    const buf = await host.fetchMesh(i);
    if (token !== loadToken || i !== state.selected) return;
    const first = !viewer.payload;
    const p = parsePayload(buf);
    viewer.setMesh(p);
    countPieces(p);
    surfaceHeights(p);
    drafts.shown = false;
    state.loaded.set(i, version);
    sketchCache.clear(); viewer.setSketches([]);
    showBuild(state.tiles[i]);
    if (first) {
      const saved = state.cameras.get(state.tiles[i].path);
      if (saved) {
        viewer.setCamera(saved.camera);
        state.activeView = saved.activeView;
        showActiveView(state.activeView);
      } else pickView(state.view, following?.i === i);
    }
    if (following?.i === i) following = null;
    // a selection the new build no longer has goes
    if (isEntry(state.sel) && state.sel.entry >= stackItems().length) state.sel = null;
    renderLens();
    renderAll();
  } catch (e) {
    console.error(e);
  }
}

/** Wire this part of the page (once, at load). */
export function init() {
  viewer.setView(state.view, false, ONE_SQUARE);
  {
    const user = viewer.onUserViewChange;
    viewer.onUserViewChange = () => { following = null; user?.(); };
  }
}
