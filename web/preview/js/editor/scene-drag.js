// Moving pieces with the pointer, and putting a tile from the catalog down by dropping it (docs/maps.html).
//
// In a view looking down on the map a press on a piece takes it (and the rest of the selection, with Shift)
// and a drag moves it, snapping to the squares of its own system; the piece follows the pointer at its level
// and lands where the pointer lets go (a place that is taken is the nearest free one, said so).  In any
// other view a drag orbits, as it always does, and a click still selects.  Moving is one change of the map's
// text, with Undo, like every other.  A tile card dragged from the catalog and dropped on the map is added
// where it is dropped.

import * as M from '../map-model.js';
import { $, viewer } from './app.js';
import { server, view } from './stores.js';
import { evaluate, live, mapMode, pieces, sceneEdit, sceneNow, select } from './scene.js';
import { addTile, editable, moveTo } from './scene-edit.js';

/** Is the camera looking (nearly) straight down? */
const topDown = () => viewer.camera.position.clone().sub(viewer.controls.target).normalize().z > 0.93;

/** The map point (mm, y down) under the pointer on the plane `z`, or null. */
function mapPoint(e, z) {
  const r = $('canvas').getBoundingClientRect();
  const h = viewer.pointAt(e.clientX - r.left, e.clientY - r.top, z);
  return h ? [h.p[0], -h.p[1]] : null;
}

let drag = null;

function onDown(e) {
  if (!mapMode() || e.button !== 0 || e.shiftKey || e.pointerType === 'touch') return;
  if (!editable() || !topDown()) return;
  const r = $('canvas').getBoundingClientRect();
  const hit = viewer.scenePick(e.clientX - r.left, e.clientY - r.top);
  if (hit < 0) return;
  const sel = view.get().msel.includes(hit) ? view.get().msel : [hit];
  if (!view.get().msel.includes(hit)) select([hit]);
  const ps = sel.map((i) => pieces().find((p) => p.index === i)).filter((p) => p?.at);
  if (!ps.length) return;
  const z = ps[0].bottom_mm || 0;
  const pt = mapPoint(e, z);
  if (!pt) return;
  // the orbit does not take this press: the controls are off until it is let go
  viewer.controls.enabled = false;
  sceneEdit.dragged = false;
  drag = { ps, z, x: e.clientX, y: e.clientY, from: pt, offs: ps.map((p) => [pt[0] - p.at[0], pt[1] - p.at[1]]), moved: false, to: null };
}

let raf = 0;
function onMove(e) {
  if (!drag) return;
  if (!drag.moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 4) return;
  drag.moved = true;
  sceneEdit.dragged = true;
  const pt = mapPoint(e, drag.z);
  if (!pt) return;
  drag.to = drag.ps.map((p, i) => {
    const s = M.snapSquares(p, pt[0] - drag.offs[i][0], pt[1] - drag.offs[i][1]);
    return { p, x: s.x, y: s.y };
  });
  for (const m of drag.to) live.set(m.p.index, [m.x * m.p.sq, m.y * m.p.sq]);
  if (!raf) raf = requestAnimationFrame(() => { raf = 0; evaluate(true); });
}

async function onUp() {
  const d = drag;
  drag = null;
  viewer.controls.enabled = true;
  if (!d) return;
  if (!d.moved || !d.to) { live.clear(); setTimeout(() => { sceneEdit.dragged = false; }, 0); return; }
  const changed = d.to.filter((m) => m.p.at && (Math.abs(m.x * m.p.sq - m.p.at[0]) > 0.01 || Math.abs(m.y * m.p.sq - m.p.at[1]) > 0.01));
  try {
    if (changed.length) await moveTo(changed);
  } finally {
    live.clear();
    evaluate(true);
    setTimeout(() => { sceneEdit.dragged = false; }, 0);
  }
}

// ---- dropping a tile from the catalog -------------------------------------------------------------------------

export const TILE_MIME = 'application/x-dharmatiles-tile';

function onDragOver(e) {
  if (!mapMode() || !e.dataTransfer?.types.includes(TILE_MIME)) return;
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
}

async function onDrop(e) {
  if (!mapMode() || !e.dataTransfer?.types.includes(TILE_MIME)) return;
  e.preventDefault();
  const id = e.dataTransfer.getData(TILE_MIME);
  const t = server.get().tiles.find((x) => x.id === id);
  if (!t) return;
  const pt = mapPoint(e, 0);
  const sc = sceneNow()?.system === 'ol' ? 25.4 : 35;
  await addTile(t, pt ? { x: Math.round(pt[0] / sc - 0.5), y: Math.round(pt[1] / sc - 0.5) } : null);
}

/** Wire this part of the page (once, at load). */
export function init() {
  const v = $('view');
  v.addEventListener('pointerdown', onDown, true);
  addEventListener('pointermove', onMove);
  addEventListener('pointerup', onUp);
  addEventListener('pointercancel', onUp);
  const c = $('canvas');
  c.addEventListener('dragover', onDragOver);
  c.addEventListener('drop', onDrop);
}
