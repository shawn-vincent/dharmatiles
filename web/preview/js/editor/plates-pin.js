// Pinning a piece on a plate (docs/maps.html, "Pins"): in the plates' view, looking down on the bed, press a
// piece and drag it; where you let go it is pinned there.  A pin is a line in the map's `plates` block (the
// piece's id, its plate and where it stands on the bed); the packer keeps a pinned piece where it was dropped and
// packs the rest around it, and says so when a pin cannot hold.  The dragging is the map's (scene-drag.js): the same
// press, the same snap to a place, one change of the text with Undo.  A pinned piece wears a small pin pill.

import { $, host, toast, viewer } from './app.js';
import { failed, showError } from '../problems.js';
import { say } from './inventory.js';
import { isPlatesView, tile, view } from './stores.js';

const plan = () => tile()?.plan?.plates ?? null;
const topDown = () => viewer.camera.position.clone().sub(viewer.controls.target).normalize().z > 0.93;
const editable = () => { const t = tile(); return !!t && !t.path.startsWith('lib:') && (t.plan?.writable ?? true); };

/** The piece of the plate in view that triangle owner `n` is: its layout row. */
const rowOf = (n) => plan()?.layout?.[plan().view]?.pieces?.[n] ?? null;

/** A call that edits the list: the answer's body, or null after the refusal is shown. */
async function call(label, run) {
  try {
    const r = await run();
    if (r.status !== 200) {
      if (r.status !== 0 && !failed(label, r)) toast(`${label}: ${r.body?.error ?? 'refused'}`, null, { ms: 9000 });
      return null;
    }
    if (r.body.ok === false) { say(r.body, label); return null; }
    return r.body;
  } catch (e) { showError(label, e); return null; }
}

/** Undo of a pin change: written back only if the file still holds what the change wrote (`b.after`). */
async function undo(b) {
  await call('Could not undo', () => host.listsRestore({ list: b.list, text: b.before, expect: b.after }));
}

/** Pin the piece (list index `index`) at `x`, `y` (the footprint's north-west corner from the bed's, mm) on plate `plate` (from 1). */
export async function pin(index, plate, x, y, turn, name) {
  const b = await call('Could not pin the piece', () => host.mapsPin({ list: tile().id, index, plate, x, y, turn }));
  if (b) toast(`Pinned ${name} on plate ${plate}`, () => undo(b));
  return b;
}

/** Take the pin of a piece away. */
export async function unpin(index, name) {
  const b = await call('Could not take the pin away', () => host.mapsUnpin({ list: tile().id, index }));
  if (b) toast(`Took the pin off ${name}`, () => undo(b));
  return b;
}

/** Take every pin away. */
export async function unpinAll() {
  const b = await call('Could not take the pins away', () => host.mapsUnpinAll({ list: tile().id }));
  if (b) toast('Took every pin away', () => undo(b));
  return b;
}

let drag = null;

function onDown(e) {
  if (!isPlatesView() || e.button !== 0 || e.shiftKey || e.pointerType === 'touch' || !editable() || !topDown()) return;
  const p = plan();
  if (!p?.planned) return;
  const r = $('canvas').getBoundingClientRect();
  const h = viewer.pick(e.clientX - r.left, e.clientY - r.top);
  if (!h || h.entry < 0) return;
  const row = rowOf(h.entry);
  if (!row || row.piece == null) return;
  const pt = viewer.pointAt(e.clientX - r.left, e.clientY - r.top, 0);
  if (!pt) return;
  // a piece that stands for several copies of one line cannot be told apart in the document
  const line = (p.pieces ?? []).find((q) => q.index === row.piece);
  if (line && line.count > 1) { toast(`${row.name} is ${line.count} copies of one line: give each copy its own piece line to pin it`, null, { ms: 8000 }); return; }
  viewer.controls.enabled = false;
  drag = { row, line, x: e.clientX, y: e.clientY, offX: pt.p[0] - row.x, offY: pt.p[1] - row.y, moved: false, to: null };
}

function onMove(e) {
  if (!drag) return;
  if (!drag.moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 4) return;
  drag.moved = true;
  const r = $('canvas').getBoundingClientRect();
  const pt = viewer.pointAt(e.clientX - r.left, e.clientY - r.top, 0);
  if (!pt) return;
  const snap = (v) => Math.round(v * 2) / 2;
  drag.to = { x: snap(pt.p[0] - drag.offX), y: snap(pt.p[1] - drag.offY) };
  viewer.setGhostRect({ x0: drag.to.x, y0: drag.to.y, x1: drag.to.x + drag.row.w, y1: drag.to.y + drag.row.d });
}

async function onUp() {
  const d = drag;
  drag = null;
  viewer.controls.enabled = true;
  if (!d) return;
  if (!d.moved || !d.to) { viewer.setGhostRect(null); return; }
  const p = plan();
  const bed = p.bed;
  try {
    await pin(d.row.piece, p.view + 1, d.to.x, bed[1] - d.to.y - d.row.d, d.row.turned, d.row.name);
  } finally {
    // the plate is laid out again with the pin: until it comes, the ghost shows where it was dropped
    setTimeout(() => viewer.setGhostRect(null), 1500);
  }
}

// ---- the pin pills ----------------------------------------------------------------------------------------

const tags = new Map();

function drawTags() {
  const box = $('ppins');
  const p = isPlatesView() ? plan() : null;
  const rows = (p?.planned && p.layout?.[p.view]?.pieces) || [];
  const live = new Set();
  rows.forEach((row, n) => {
    if (!row.pinned) return;
    const s = viewer.project([row.x + row.w / 2, row.y + row.d / 2, 18]);
    let e = tags.get(n);
    if (!e) { e = document.createElement('span'); e.className = 'mp-tag have'; e.textContent = 'pinned'; box.append(e); tags.set(n, e); }
    e.style.transform = `translate(${s.x.toFixed(1)}px, ${s.y.toFixed(1)}px) translate(-50%, -100%)`;
    live.add(n);
  });
  for (const [n, e] of tags) if (!live.has(n)) { e.remove(); tags.delete(n); }
}

/** Wire this part of the page (once, at load). */
export function init() {
  $('view').addEventListener('pointerdown', onDown, true);
  addEventListener('pointermove', onMove);
  addEventListener('pointerup', onUp);
  addEventListener('pointercancel', onUp);
  viewer.onRenderHook(drawTags);
  view.subscribe(['dmode', 'tileId'], () => { viewer.setGhostRect(null); viewer.draw(); });
}
