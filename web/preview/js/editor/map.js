// @ts-check
// The map over the view, the bar and its tools.

import { valueOf } from '../lens.js';
import { Overlay } from '../overlay.js';
import { Tools } from '../place.js';
import { byteToIndex } from '../text.js';
import { failed } from '../problems.js';
import { token } from '../tokens.js';
import { $, host, makeMine, viewer } from './app.js';
import { askDraft, edited, openFile, replaceBytes, spansUsable, texts } from './document.js';
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
 * The selection's handles: a line's points, a region's fill points, a
 * placed thing's at=.  Each {q (squares), span (bytes of the written
 * value), snap (edge words allowed), move(q) (the overlay follows)}.
 */
function handles() {
  const s = view.get().sel, { writable, file, map } = doc.get();
  if (!s || !writable || file !== tile()?.path || !map) return [];
  if (s.kind === 'line') {
    const l = mapNode(s);
    return (l?.points || []).filter((p) => p.at).map((p, k) => ({
      q: p.at, span: p.span, snap: true,
      move(q) { if (!l.wobble && l.pts.length === l.points.length) l.pts[k] = q; },
    }));
  }
  if (s.kind === 'region') return (mapNode(s)?.fills || []).filter((f) => f.at).map((f) => ({ q: f.at, span: f.span, snap: false, move(q) { f.at = q; } }));
  if (isEntry(s)) {
    const it = itemOf(s);
    const c = (it?.place || []).find((x) => x.name === 'at');
    if (!c || it.file !== file) return [];
    const v = valueOf(c.text, 'at');
    // a fixed point: where the program reads it (a written pair with units,
    // from the north-west; the program says where it is)
    const q = /^"[^"]*[\d(]/.test(v.text) ? it.at : null;
    if (!q) return [];
    const start = c.span[0] + new TextEncoder().encode(c.text.slice(0, v.at)).length;
    return [{ q, span: [start, new TextEncoder().encode(v.text).length], snap: false }];
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
  const r = await host.edit({ op: 'insert', text: doc.get().text, place, node });
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
    sq: () => tile()?.plan?.squareMm || 35,
    size: () => [tile()?.plan?.cols || 1, tile()?.plan?.rows || 1],
    map: () => doc.get().map,
    writable: () => doc.get().writable,
    makeMine: (replay) => makeMine(replay),
    insert: (place, node, opts) => insertNode(place, node, opts),
    replace: replaceBytes,
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
