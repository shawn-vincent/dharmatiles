// Live updates: the event stream from the server, and waiting for a tile to
// arrive.

import { $, host, state, tile, viewer } from './app.js';
import { showBuild, sketchCache } from './build.js';
import { browsing } from './chrome.js';
import { askDraft, changed, dirty, doc, draftArrived, drafts } from './document.js';
import { renderTitle } from './export.js';
import { followBuild, loadMesh, select } from './open.js';
import { inRoot, renderList } from './tiles.js';
import { renderAll, showProgress } from './title.js';

// ---- live updates ----------------------------------------------------

/** A plan that differs from the one the view has. */
const planChanged = (p) => !!p && JSON.stringify(p) !== JSON.stringify(viewer.plan);

// A tile the server just made (new, renamed, copied) joins the list with
// its next message: wait for it there.
const awaited = new Map(); // tile index -> resolvers waiting for it
/** Resolves true once tile `i` is on the list (false if the server changed meanwhile). */
export function whenTile(i) {
  if (state.tiles[i]) return Promise.resolve(true);
  return new Promise((res) => awaited.set(i, [...(awaited.get(i) || []), res]));
}
function tilesArrived() {
  for (const [i, rs] of awaited) if (state.tiles[i]) { awaited.delete(i); rs.forEach((r) => r(true)); }
}

/**
 * After the stream dropped, the server may be a new one, numbering its
 * meshes, sketches and drafts from the start again: forget what was
 * keyed by its numbers, and find the tile shown again by its path.
 */
function afterGap(m) {
  const path = tile()?.path;
  state.loaded.clear();
  sketchCache.clear();
  Object.assign(drafts, { sent: 0, waiting: false, again: false, shown: false });
  // indices waited for were the old server's
  for (const rs of awaited.values()) rs.forEach((r) => r(false));
  awaited.clear();
  if (path == null) return;
  const i = m.tiles.findIndex((t) => t.path === path && !t.gone);
  // the text pane's document follows its tile to its new number, unsaved edits and all
  if (doc.tile === state.selected) doc.tile = i;
  state.selected = i;
  state.textFor = null;
}

/** Wire this part of the page (once, at load). */
export function init() {
  host.connect({
    onState: (s) => {
      state.live = s === 'live';
      state.ever ||= state.live;
      if (!state.live) state.gap = true;
      renderTitle();
      showBuild(tile());
    },
    onSnapshot: (m) => {
      const gapped = state.gap;
      state.gap = false;
      if (gapped) afterGap(m);
      state.tiles = m.tiles; state.roots = m.roots || [];
      tilesArrived();
      // an edit the server never got: checked, drafted and saved now
      if (gapped && tile() && dirty()) { changed(); askDraft(); }
      state.overhangDeg = m.overhang_deg; state.steepestDeg = m.steepest_deg ?? m.overhang_deg + 10; state.system = m.system;
      $('lg-orange').textContent = `${state.overhangDeg}–${state.steepestDeg}°: allowed within budget`;
      $('lg-red').textContent = `Over ${state.steepestDeg}°: small patches allowed, more fails`;
      const cur = state.tiles[state.selected];
      if (cur && !cur.gone) {
        // a rescan or a reconnect: keep the view, catch up on a newer mesh
        renderAll();
        if (planChanged(cur.plan)) viewer.setPlan(cur.plan);
        if (cur.mesh_version != null && state.loaded.get(cur.index) !== cur.mesh_version) loadMesh(cur.index, cur.mesh_version);
        showBuild(cur);
        return;
      }
      if (state.root >= 0) {
        // the tile shown was deleted, or none was: the first one left in this root
        const first = inRoot(state.root)[0];
        select(first ? first.index : -1);
        return;
      }
      // the first snapshot: the tile in the address, else the first root with tiles
      const want = decodeURIComponent((location.hash.match(/tile=([^&]+)/) || [])[1] || '');
      const wantRoot = (location.hash.match(/root=(\d+)/) || [])[1];
      const wanted = want ? state.tiles.findIndex((t) => t.path === want && !t.gone) : -1;
      if (wanted >= 0) return select(wanted);
      const roots = state.roots.length ? state.roots.map((_, i) => i) : [0];
      state.root = wantRoot != null && state.roots[wantRoot] ? Number(wantRoot)
        : (roots.find((r) => inRoot(r).length) ?? roots[0]);
      const first = inRoot(state.root)[0];
      select(first ? first.index : -1);
    },
    onProgress: (i, p) => {
      const t = state.tiles[i];
      if (!t) return;
      t.progress = p;
      if (i === state.selected && t.status === 'building') showProgress(p);
    },
    onTile: (t) => {
      const before = state.tiles[t.index];
      state.tiles[t.index] = t;
      tilesArrived();
      if (t.draft_id != null && t.draft_id !== before?.draft_id) draftArrived(t);
      if (t.index !== state.selected) { if (browsing()) renderList(); return; }
      if (planChanged(t.plan)) { viewer.setPlan(t.plan); followBuild(); }
      renderAll();
      showBuild(t);
      // the new mesh swaps in place: the camera is kept
      if (t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) loadMesh(t.index, t.mesh_version);
    },
  });
}
