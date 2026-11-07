// The selection: what is selected and hovered, in the view and the text, and
// the pointer on the canvas.

import { $, isEntry, same, state, tile, toast, viewer } from './app.js';
import { closePopovers, pickView } from './chrome.js';
import { canLeave, dirty, doc, linesOf, loadText, markText, stackItems, texts } from './document.js';
import { controlText, tools } from './map.js';
import { renderLens } from './sheet.js';
import { stem } from './tiles.js';

// only a scatter's things are picked one by one (a dirt's clods or a grass's clumps are the entry)
const piecesOf = (e) => (stackItems()[e]?.what === 'scatter' ? state.pieces.get(e) || 0 : 0);

// ---- the selection ----------------------------------------------------------

function describe(w) {
  if (!w) return '';
  if (w.tile) return tile() ? `tile ${stem(tile())}` : 'tile';
  if (w.region != null) return `region ${w.region}`;
  if (w.line != null) return `line ${w.line}`;
  const it = stackItems()[w.entry];
  if (!it) return '';
  const n = piecesOf(w.entry);
  if (w.piece != null && n > 1) return `${it.label} · ${w.piece + 1} of ${n}`;
  return it.label;
}

export function setSel(w, { from, guarded = false } = {}) {
  if (same(w, state.sel)) return;
  const targetFile = isEntry(w) ? stackItems()[w.entry]?.file
    : w && (w.tile || w.region != null || w.line != null) ? tile()?.path : null;
  if (!guarded && targetFile && targetFile !== texts.file && dirty()) {
    canLeave().then((ok) => { if (ok) setSel(w, { from, guarded: true }); });
    return;
  }
  state.sel = w;
  $('tilebtn').classList.toggle('sel', !!w?.tile);
  viewer.setHighlight(isEntry(w) ? w : null, hovForView());
  // the text follows: the selection's own file, scrolled to it
  if (isEntry(w)) {
    const it = stackItems()[w.entry];
    if (it?.file && it.file !== texts.file && from !== 'text') loadText(it.file).then(() => { revealSel(); renderLens(); });
  } else if (w && texts.file !== tile()?.path) loadText(null); // the tile, a region, a line: the tile's own text
  markText();
  tools?.paint();
  if (from !== 'text') revealSel();
  renderLens();
}
export function revealSel() { const l = linesOf(state.sel); if (l) texts.reveal(l[0]); }

export function setHov(w) {
  if (same(w, state.hov)) return;
  state.hov = w;
  viewer.setHighlight(isEntry(state.sel) ? state.sel : null, hovForView());
  markText();
}
export const hovForView = () => (isEntry(state.hov) && !same(state.hov, state.sel) ? state.hov : null);

/** A click in the view on `hit`: select it, or climb when it is already selected. */
function clickHit(hit) {
  if (!hit) return setSel(null);
  const s = state.sel;
  const many = piecesOf(hit.entry) > 1;
  const inOf = (e) => (stackItems()[e] ? controlText(stackItems()[e], 'in') : null);
  if (s?.region != null && inOf(hit.entry) === s.region) return setSel({ tile: true }, { from: 'view' }); // the region -> the tile
  if (!isEntry(s) || s.entry !== hit.entry) return setSel({ entry: hit.entry, piece: many ? hit.piece : null }, { from: 'view' });
  if (s.piece != null) return setSel({ entry: hit.entry, piece: null }, { from: 'view' }); // the piece -> its entry
  const inr = inOf(s.entry);
  if (inr && doc.map?.regions.some((r) => r.name === inr)) return setSel({ region: inr }, { from: 'view' }); // the entry -> its region
  return setSel({ tile: true }, { from: 'view' }); // the entry -> the tile
}

/** Fly to the selection (f). */
export function frameSel() {
  const s = state.sel;
  if (!isEntry(s)) return pickView(state.view);
  const b = viewer.centreOf(s.entry, s.piece);
  if (!b) return;
  const pose = viewer.pose();
  const size = Math.max(8, Math.hypot(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]));
  pose.target.set((b.lo[0] + b.hi[0]) / 2, (b.lo[1] + b.hi[1]) / 2, (b.lo[2] + b.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 2.4 });
}

/** Explain the gate's paint by taking the camera to its most important face. */
function showWorst() {
  const f = viewer.worstFlagged();
  if (!f) return;
  if (f.entry != null) setSel({ entry: f.entry, piece: f.piece }, { from: 'view' });
  const pose = viewer.pose();
  const size = Math.max(4, Math.hypot(f.hi[0] - f.lo[0], f.hi[1] - f.lo[1], f.hi[2] - f.lo[2]));
  pose.target.set((f.lo[0] + f.hi[0]) / 2, (f.lo[1] + f.hi[1]) / 2, (f.lo[2] + f.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 3.2 });
  closePopovers();
  toast(f.kind === 'red'
    ? `Showing the largest face past ${state.steepestDeg}°`
    : `Showing the largest face in the ${state.overhangDeg}–${state.steepestDeg}° band`);
}

// hover and click in the view: a click is a press and release that hardly moved
export const canvas = $('canvas');
let down = null, hoverRaf = 0, lastMove = null;

/** Wire this part of the page (once, at load). */
export function init() {
  $('show-worst').onclick = showWorst;
  canvas.addEventListener('pointerdown', (e) => { down = { x: e.clientX, y: e.clientY, t: performance.now(), b: e.button }; $('hint').style.display = 'none'; });
  canvas.addEventListener('pointerup', (e) => {
    const d = down; down = null;
    if (!d || d.b !== 0 || Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4 || performance.now() - d.t > 600) return;
    const r = canvas.getBoundingClientRect();
    clickHit(viewer.pick(e.clientX - r.left, e.clientY - r.top));
  });
  canvas.addEventListener('pointermove', (e) => {
    if (e.buttons || e.pointerType === 'touch') return;
    lastMove = e;
    if (hoverRaf) return;
    hoverRaf = requestAnimationFrame(() => {
      hoverRaf = 0;
      const r = canvas.getBoundingClientRect();
      const x = lastMove.clientX - r.left, y = lastMove.clientY - r.top;
      const hit = viewer.pick(x, y);
      const many = hit && piecesOf(hit.entry) > 1;
      // hover shows what a click would select
      let w = hit && { entry: hit.entry, piece: many ? hit.piece : null };
      if (w && isEntry(state.sel) && state.sel.entry === w.entry) w = state.sel;
      setHov(w);
      const hint = $('hint');
      if (w) {
        hint.textContent = describe(w);
        hint.style.display = 'block';
        const left = Math.min(x + 14, r.width - hint.offsetWidth - 8);
        hint.style.left = `${Math.max(8, left)}px`;
        hint.style.top = `${Math.max(8, y - 30)}px`;
      } else hint.style.display = 'none';
    });
  });
  canvas.addEventListener('pointerleave', () => { setHov(null); $('hint').style.display = 'none'; });
  canvas.addEventListener('dblclick', () => frameSel());
}
