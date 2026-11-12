// The selection: what is selected and hovered, in the view and the text, and
// the pointer on the canvas.  It keeps `view.sel` and `view.hov` (one shape:
// {kind, id, piece?}; an entry's id is its key) and reflects them in the 3D
// view's tint, the text pane's marks and the title's button; the sheet and
// the map subscribe to the same keys themselves.
//
// An entry is selected by key, and each part resolves it against the stack
// it needs: the view (hover, click, tint, pieces, framing) against the stack
// of the mesh on screen, which may be older than the text; the text and the
// sheet (lines, files, labels) against the stack of the text now.  So an
// insert or a move in the source cannot point the selection at another entry.

import { byteToIndex } from '../text.js';
import { entryKey, meshIndex, meshStack } from '../payload.js';
import { cloudAtCaret, cloudOnLine, cloudsIn } from '../cloud.js';
import { $, pickView, toast, viewer } from './app.js';
import { texts, canLeave } from './document.js';
import {
  controlText, dirty, regionOf, doc, entrySel, isEntry, isMark, isPlace, itemOf, mapNode, markSel, same, server, stackItems, stem,
  tile, tileSel, view,
} from './stores.js';

/** The entry `key` as the mesh on screen has it (`{key, label, element, what}`), or null: the view's stack, not the text's. */
const meshOf = (key) => {
  const i = meshIndex(viewer.payload, key);
  return i < 0 ? null : meshStack(viewer.payload)[i];
};

// only a scatter's things are picked one by one (a dirt's clods or a grass's clumps are the entry)
const piecesOf = (key) => (meshOf(key)?.what === 'scatter' ? view.get().pieces.get(key) || 0 : 0);

/** What the pointer found in the view, `{entry, piece}` against the mesh's stack, as `{key, piece}` (null: nothing owned). */
function hitKey(hit) {
  const key = hit ? entryKey(viewer.payload, hit.entry) : null;
  return key == null ? null : { key, piece: hit.piece };
}

// ---- the selection ----------------------------------------------------------

function describe(w) {
  if (!w) return '';
  if (w.kind === 'tile') return tile() ? `tile ${stem(tile())}` : 'tile';
  if (isMark(w)) return `${w.kind} ${w.id}`;
  // the text's stack names it; the mesh's still does when the text has moved on
  const it = itemOf(w) ?? meshOf(w.id);
  if (!it) return '';
  const n = piecesOf(w.id);
  if (w.piece != null && n > 1) return `${it.label} · ${w.piece + 1} of ${n}`;
  return it.label;
}

/** Select `w` (or nothing); the text follows to the selection's own file. */
export function setSel(w, { from, guarded = false } = {}) {
  if (same(w, view.get().sel)) return;
  const targetFile = isEntry(w) ? itemOf(w)?.file : w ? tile()?.path : null;
  if (!guarded && targetFile && targetFile !== texts.file && dirty()) {
    canLeave().then((ok) => { if (ok) setSel(w, { from, guarded: true }); });
    return;
  }
  const patch = { sel: w };
  if (isEntry(w)) {
    // the text follows: the selection's own file (unless it was the text that asked)
    if (targetFile && from !== 'text') patch.textFile = targetFile === tile()?.path ? null : targetFile;
  } else if (w) patch.textFile = null; // the tile, a region, a line: the tile's own text
  view.set(patch);
  if (from !== 'text') revealSel();
}
function revealSel() { const l = linesOf(view.get().sel); if (l) texts.reveal(l[0]); }

function setHov(w) { if (!same(w, view.get().hov)) view.set({ hov: w }); }

// ---- the selection in the view, the text and the title ------------------------

/** The view's tint for a selection: its place in the mesh on screen (none when that mesh has no such entry). */
function tint(w) {
  const entry = isEntry(w) ? meshIndex(viewer.payload, w.id) : -1;
  return entry < 0 ? null : { entry, piece: w.piece };
}
function paintHighlight() {
  const { sel, hov } = view.get();
  viewer.setHighlight(tint(sel), isEntry(hov) && !same(hov, sel) ? tint(hov) : null);
}

/** The lines [first, last] (from 1) that byte span [offset, length] of the text covers. */
function spanLines([o, len]) {
  const t = doc.get().text || '';
  const a = byteToIndex(t, o), b = byteToIndex(t, o + Math.max(len, 1) - 1);
  return [t.slice(0, a).split('\n').length, t.slice(0, b).split('\n').length];
}

/** The lines of the file shown that a selection is written on, or null. */
export function linesOf(w) {
  if (!w) return null;
  const d = doc.get(), own = d.file === tile()?.path;
  if (w.kind === 'tile') return own ? [1, (d.text || '').split('\n').length] : null;
  if (isPlace(w)) {
    const n = mapNode(w);
    return n?.span && own ? spanLines(n.span) : null;
  }
  const it = itemOf(w);
  return it?.lines && it.file === d.file ? it.lines : null;
}

/** The innermost entry written on line `n` of the file shown, else the tile (its own file) or nothing. */
function entryAtLine(line, hover = false) {
  let best = null, width = Infinity;
  for (const it of stackItems()) {
    if (!it.key || !it.lines || it.file !== texts.file) continue;
    const [a, b] = it.lines;
    if (line >= a && line <= b && b - a < width) { best = it.key; width = b - a; }
  }
  if (best != null) return entrySel(best);
  if (texts.file !== tile()?.path) return null;
  const map = doc.get().map;
  // a point is only hovered: the caret on one selects the tile
  for (const [kind, list] of [['region', map?.regions], ['line', map?.lines], ...(hover ? [['point', map?.points]] : [])]) {
    for (const node of list || []) {
      const l = node.span && spanLines(node.span);
      if (l && line >= l[0] && line <= l[1]) return markSel(kind, node.name);
    }
  }
  return tileSel();
}

function markText() {
  const t = tile(), d = doc.get(), { sel, hov, tileId } = view.get();
  const live = d.checked === d.text && d.tileId === tileId;
  const errs = (live ? d.errors : t?.errors || []).filter((e) => (!e.file || e.file === texts.file) && e.line)
    .map((e) => ({ line: e.line, message: `${e.message}${e.help ? `. ${e.help}` : ''}` }));
  texts.mark(sel?.kind === 'tile' ? null : linesOf(sel), linesOf(hov), errs);
}

// ---- the selected cloud ----------------------------------------------------

/** The clouds written in the file shown. */
const shownClouds = () => cloudsIn(texts.text, stackItems(), texts.file);
/** The cloud the caret is in, as its stack entry. */
function caretCloud() {
  const c = texts.text ? cloudAtCaret(shownClouds(), texts.caret().index) : null;
  view.set({ cloudText: c ? c.key : null });
}
function paintCloud() {
  const v = view.get();
  viewer.setCloud(v.cloudSheet ?? v.cloudText, v.cloudSheetHov ?? v.cloudHov);
}

/** A node a gesture just wrote, once it is in a clean check, becomes the selection. */
function selectWritten() {
  const d = doc.get();
  if (d.selectAt == null || !d.ok) return;
  doc.set({ selectAt: null });
  const it = stackItems().find((x) => x.key && x.span && x.span[0] === d.selectAt && x.file === d.file);
  if (it) setSel(entrySel(it.key), { from: 'view' });
}

/** A click in the view on `hit`: select it, or climb when it is already selected. */
function clickHit(found) {
  const hit = hitKey(found);
  if (!hit) return setSel(null);
  const s = view.get().sel;
  const many = piecesOf(hit.key) > 1;
  const inOf = (key) => { const it = itemOf(entrySel(key)); return it ? regionOf(it) : null; };
  if (s?.kind === 'region' && inOf(hit.key) === s.id) return setSel(tileSel(), { from: 'view' }); // the region -> the tile
  if (!isEntry(s) || s.id !== hit.key) return setSel(entrySel(hit.key, many ? hit.piece : null), { from: 'view' });
  if (s.piece != null) return setSel(entrySel(hit.key), { from: 'view' }); // the piece -> its entry
  const inr = inOf(s.id);
  if (inr && doc.get().map?.regions.some((r) => r.name === inr)) return setSel(markSel('region', inr), { from: 'view' }); // the entry -> its region
  return setSel(tileSel(), { from: 'view' }); // the entry -> the tile
}

/** Fly to the selection (f). */
export function frameSel() {
  const { sel, mode } = view.get();
  if (!isEntry(sel)) return pickView(mode);
  const at = meshIndex(viewer.payload, sel.id);
  const b = at < 0 ? null : viewer.centreOf(at, sel.piece);
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
  if (f.key != null) setSel(entrySel(f.key, f.piece), { from: 'view' });
  const pose = viewer.pose();
  const size = Math.max(4, Math.hypot(f.hi[0] - f.lo[0], f.hi[1] - f.lo[1], f.hi[2] - f.lo[2]));
  pose.target.set((f.lo[0] + f.hi[0]) / 2, (f.lo[1] + f.hi[1]) / 2, (f.lo[2] + f.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 3.2 });
  const { steepestDeg, overhangDeg } = server.get();
  toast(f.kind === 'red'
    ? `Showing the largest face past ${steepestDeg}°`
    : `Showing the largest face in the ${overhangDeg}–${steepestDeg}° band`);
}

// hover and click in the view: a click is a press and release that hardly moved
const canvas = $('canvas');
let down = null, hoverRaf = 0, lastMove = null;

/** Wire this part of the page (once, at load). */
export function init() {
  // the pane tells which line the caret or the pointer is on
  const caret = texts.cb.onCaretLine;
  texts.cb.onCaretLine = (n) => { caret?.(n); setSel(entryAtLine(n), { from: 'text' }); };
  texts.cb.onCaret = caretCloud; // on every move of the caret and every edit, not only a click or a key
  texts.cb.onHoverLine = (n) => {
    setHov(n == null ? null : entryAtLine(n, true));
    view.set({ cloudHov: n == null ? null : cloudOnLine(texts.text, shownClouds(), n)?.key ?? null });
  };
  view.subscribe(['cloudText', 'cloudSheet', 'cloudHov', 'cloudSheetHov'], paintCloud);
  // another tile, or a stack checked again (its entries may have moved): ask the caret again
  view.subscribe(['tileId'], () => view.set({ cloudText: null, cloudSheet: null, cloudHov: null, cloudSheetHov: null }));
  doc.subscribe(['stack', 'file', 'tileId'], caretCloud);

  view.subscribe(['sel', 'hov', 'shown', 'draft', 'tileId'], paintHighlight);
  view.subscribe(['sel', 'tileId'], () => $('tilebtn').classList.toggle('sel', view.get().sel?.kind === 'tile'));
  view.subscribe(['sel', 'hov', 'tileId'], markText);
  doc.subscribe(['checked', 'errors', 'file', 'map', 'stack', 'tileId'], markText);
  server.subscribe(['tiles'], markText);
  doc.subscribe(['stack', 'checked'], selectWritten);
  // a file just shown: scroll it to the selection's lines
  doc.subscribe(['file'], revealSel);
  $('show-worst').onclick = showWorst;
  $('tilebtn').onclick = () => setSel(tileSel());
  addEventListener('keydown', (e) => {
    const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable;
    if (e.key === 'f' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey && view.get().sel) frameSel();
  });
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
      const hit = hitKey(viewer.pick(x, y));
      const many = hit && piecesOf(hit.key) > 1;
      // hover shows what a click would select
      let w = hit && entrySel(hit.key, many ? hit.piece : null);
      const sel = view.get().sel;
      if (w && isEntry(sel) && sel.id === w.id) w = sel;
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
