// @ts-check
// The map over the view, the bar and its tools.

import { valueOf } from '../lens.js';
import { Overlay } from '../overlay.js';
import { Tools } from '../place.js';
import { byteToIndex } from '../text.js';
import { failed } from '../problems.js';
import { token } from '../tokens.js';
import { $, host, makeMine, viewer } from './app.js';
import { editCall, askDraft, edited, openFile, spansUsable, steps, texts } from './document.js';
import { idAtByte } from '../rows.js';
import { meshStack } from '../payload.js';
import { doc, elementNamed, isEntry, itemOf, mapNode, regionOf, same, server, stackItems, tile, view } from './stores.js';

export let tools = null; // the bar's tools (below)

// ---- the map, the bar and its tools ----------------------------------------

const overlay = new Overlay($('over'), viewer);
const COLOUR = { water: '--water', dirt: '--earth', flagstones: '--stone', cliff: '--stone', planks: '--earth', grass: '--moss', reeds: '--moss', flower: '--bloom' };
const cssVar = token; // cached: read once per theme, not once per entry per frame

/** The ground's heights, from the surfaces the build made (what each is, from the mesh's own stack). */
function surfaceHeights(p) {
  const stack = meshStack(p);
  overlay.setHeights(p, (e) => elementNamed(stack[e]?.element)?.body === true, tile()?.plan?.sizeMm || p.meta.footprint);
}

/** Each region's tint: the colour of the last thing laid in it. */
function tints() {
  const out = {};
  for (const it of stackItems()) {
    const r = regionOf(it);
    if (r && COLOUR[it.element]) out[r] = cssVar(COLOUR[it.element]);
  }
  return out;
}

/**
 * Name each point of `list` (each has the `span` of its written value) by the node that holds it and its place among that
 * node's arguments: what the program's edit of a point says (`{id, slot}`), so the page writes no text.
 * @param {{span: [number, number]}[]} list @param {any} nodes the check's `nodes`
 */
function slotsOf(list, nodes) {
  const seen = new Map();
  return list.map((p) => {
    const id = idAtByte(nodes, p.span?.[0]);
    const index = seen.get(id) ?? 0;
    seen.set(id, index + 1);
    return { id, slot: { arg: index } };
  });
}
/** The program's edit of one point: `make(q)` is the body for a point moved to `q` (squares from the south-west corner). */
const pointEdit = (/** @type {{id: string, slot: any}} */ { id, slot }, /** @type {boolean} */ snap) => (/** @type {number[]} */ q) => ({ op: 'point', id, slot, q, snap });

/**
 * The selection's handles: a line's points, a region's fill points, a
 * placed thing's at=.  Each {q (squares), label (the history step of a
 * drag), make(q) (the program's edit of the point), move(q) (the overlay
 * follows)}.  A line's points land on the outline by the seam rule (the
 * program's: `snap`).
 */
function handles() {
  const s = view.get().sel, { writable, file, map, nodes } = doc.get();
  if (!s || !writable || file !== tile()?.path || !map || !nodes) return [];
  if (s.kind === 'line') {
    const l = mapNode(s);
    const slots = slotsOf(l?.points || [], nodes);
    return (l?.points || []).flatMap((p, k) => (p.at && slots[k].id != null ? [{
      q: p.at, label: `Moved a point of ${s.id}`, make: pointEdit(slots[k], true),
      move(q) { if (!l.wobbleMm && l.pts.length === l.points.length) l.pts[k] = q; },
    }] : []));
  }
  if (s.kind === 'region') {
    const fills = mapNode(s)?.fills || [];
    const slots = slotsOf(fills, nodes);
    return fills.flatMap((f, k) => (f.at && slots[k].id != null ? [{
      q: f.at, label: `Moved the fill point of ${s.id}`, make: pointEdit(slots[k], false), move(q) { f.at = q; },
    }] : []));
  }
  if (isEntry(s)) {
    const it = itemOf(s);
    const c = (it?.place || []).find((x) => x.name === 'at');
    if (!c || it.file !== file) return [];
    const v = valueOf(c.text, 'at');
    // a fixed point: where the program reads it (a written pair with units,
    // from the north-west; the program says where it is)
    const q = /^"[^"]*[\d(]/.test(v.text) ? it.at : null;
    const id = idAtByte(nodes, it.span?.[0]);
    if (!q || id == null) return [];
    return [{ q, label: `Moved ${it.label}`, make: pointEdit({ id, slot: { control: 'at' } }, false) }];
  }
  return [];
}

function drawOverlay() {
  const t = tile(), d = doc.get();
  overlay.setMap(d.tileId === view.get().tileId ? d.map : null, t?.plan?.squareMm);
  const { sel: s, hov: h } = view.get();
  overlay.draw({
    tints: tints(),
    selRegion: s?.kind === 'region' ? s.id : null,
    selLine: s?.kind === 'line' ? s.id : null,
    // the hover (drawn when it is not what is selected)
    hovRegion: h?.kind === 'region' && !same(h, s) ? h.id : null,
    hovLine: h?.kind === 'line' && !same(h, s) ? h.id : null,
    hovPoint: h?.kind === 'point' ? h.id : null,
    all: !!tools?.tool,
    ...(tools?.scene() || {}),
  });
}

/** Write a new node from a gesture in the view, then select it. */
/** @param {any} place @param {any} node @param {{select?: any}} [opts] */
async function insertNode(place, node, { select: sel } = {}) {
  const t = tile();
  if (!t) return;
  if (!doc.get().writable) return makeMine({ run: () => insertNode(place, node, { select: sel }) });
  if (doc.get().file !== t.path) {
    await openFile(null);
    if (doc.get().file !== t.path) return;
  }
  if (!spansUsable()) return;
  const r = await editCall({ op: 'insert', text: doc.get().text, place, node });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that could not be written' }); failed('Could not add to the tile', r); return; }
  if (sel) view.set({ sel, source: null }); else doc.set({ selectAt: r.body.at });
  edited(r.body.text, 'view', `Added ${node.trim().split(/\s+/)[0]}`, byteToIndex(r.body.text, r.body.at));
  askDraft();
  const l = texts.text.slice(0, byteToIndex(texts.text, r.body.at)).split('\n').length;
  texts.reveal(l);
}

/** Wire this part of the page (once, at load). */
export function init() {
  tools = new Tools($('canvas'), viewer, overlay, { bar: $('place'), input: $('place-in'), list: $('place-list'), chip: $('tool') }, {
    elements: () => server.get().elements,
    defs: () => doc.get().names?.defs ?? [],
    sq: () => tile()?.plan?.squareMm,
    size: () => [tile()?.plan?.cols || 1, tile()?.plan?.rows || 1],
    map: () => doc.get().map,
    writable: () => doc.get().writable,
    makeMine: (replay) => makeMine(replay),
    insert: (place, node, opts) => insertNode(place, node, opts),
    steps,
    handles,
    redraw: () => drawOverlay(),
    note: (text) => { const h = $('hint'); h.textContent = text; h.style.display = 'block'; h.style.left = '12px'; h.style.top = '56px'; setTimeout(() => { h.style.display = 'none'; }, 2600); },
  });
  $('tool').querySelector('button').onclick = () => tools.stop();
  viewer.onRenderHook(drawOverlay);
  // the map follows the text's last good check and the selection; the ground's heights, the mesh shown
  doc.subscribe(['map', 'tileId'], drawOverlay);
  view.subscribe(['sel', 'hov', 'tileId'], () => { tools.paint(); drawOverlay(); });
  server.subscribe(['tiles'], drawOverlay);
  view.subscribe(['shown', 'draft'], () => { if (viewer.payload) surfaceHeights(viewer.payload); });
  // while a tool is placing, Esc, Enter and Backspace are its (before the page's own keys)
  addEventListener('keydown', (e) => {
    const at = /** @type {HTMLElement} */ (e.target);
    const typing = at.tagName === 'INPUT' || at.tagName === 'TEXTAREA' || at.isContentEditable;
    if (!typing && tools.key(e)) e.preventDefault();
  }, true);
}
