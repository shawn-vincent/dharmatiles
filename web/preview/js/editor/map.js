// The map over the view, the bar and its tools.

import { valueOf } from '../lens.js';
import { Overlay } from '../overlay.js';
import { Tools } from '../place.js';
import { byteToIndex } from '../text.js';
import { failed } from '../problems.js';
import { $, host, offerMine, viewer } from './app.js';
import { askDraft, edited, openFile, replaceBytes, texts } from './document.js';
import { doc, isEntry, mapNode, regionOf, same, server, stackItems, tile, view } from './stores.js';

export let tools = null; // the bar's tools (below)

// ---- the map, the bar and its tools ----------------------------------------

const overlay = new Overlay($('over'), viewer);
const SURFACES = new Set(['dirt', 'flagstones', 'planks', 'cliff', 'water']);
const COLOUR = { water: '--water', dirt: '--earth', flagstones: '--stone', cliff: '--stone', planks: '--earth', grass: '--moss', reeds: '--moss', flower: '--bloom' };
const cssVar = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();

/** The ground's heights, from the surfaces the build made. */
function surfaceHeights(p) {
  const items = stackItems();
  overlay.setHeights(p, (e) => SURFACES.has(items[e]?.element), tile()?.plan?.size_mm || p.meta.footprint);
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
    const it = stackItems()[s.id];
    const c = (it?.place || []).find((x) => x.name === 'at');
    if (!c || it.file !== file) return [];
    const v = valueOf(c.text, 'at');
    const m = v.text.match(/^"(-?[\d.]+),(-?[\d.]+)"$/);
    if (!m) return [];
    const start = c.span[0] + new TextEncoder().encode(c.text.slice(0, v.at)).length;
    return [{ q: [Number(m[1]), Number(m[2])], span: [start, new TextEncoder().encode(v.text).length], snap: false }];
  }
  return [];
}

function drawOverlay() {
  const t = tile(), d = doc.get();
  overlay.setMap(d.tileId === view.get().tileId ? d.map : null, t?.plan?.square_mm);
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
async function insertNode(place, node, { select: sel } = {}) {
  const t = tile();
  if (!t) return;
  if (!doc.get().writable) return offerMine();
  if (doc.get().file !== t.path) {
    await openFile(null);
    if (doc.get().file !== t.path) return;
  }
  const r = await host.edit({ op: 'insert', text: doc.get().text, place, node });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that could not be written' }); failed('Could not add to the tile', r); return; }
  if (sel) view.set({ sel, textFile: null }); else doc.set({ selectAt: r.body.at });
  edited(r.body.text, 'view', `Added ${node.trim().split(/\s+/)[0]}`, byteToIndex(r.body.text, r.body.at));
  askDraft();
  const l = texts.text.slice(0, byteToIndex(texts.text, r.body.at)).split('\n').length;
  texts.reveal(l);
}

/** Wire this part of the page (once, at load). */
export function init() {
  tools = new Tools($('canvas'), viewer, overlay, { bar: $('place'), input: $('place-in'), list: $('place-list'), chip: $('tool') }, {
    elements: () => server.get().elements,
    defs: () => [...(doc.get().text || '').matchAll(/^\s*define\s+([\w+\-.]+)/gm)].map((m) => m[1]),
    sq: () => tile()?.plan?.square_mm || 35,
    size: () => [tile()?.plan?.cols || 1, tile()?.plan?.rows || 1],
    map: () => doc.get().map,
    writable: () => doc.get().writable,
    offerMine: () => offerMine(),
    insert: (place, node, opts) => insertNode(place, node, opts),
    replace: replaceBytes,
    handles,
    redraw: () => drawOverlay(),
    note: (text) => { const h = $('hint'); h.textContent = text; h.style.display = 'block'; h.style.left = '12px'; h.style.top = '56px'; setTimeout(() => { h.style.display = 'none'; }, 2600); },
  });
  $('tool').querySelector('button').onclick = () => tools.stop();
  viewer.onRender = () => drawOverlay();
  // the map follows the text's last good check and the selection; the ground's heights, the mesh shown
  doc.subscribe(['map', 'tileId'], drawOverlay);
  view.subscribe(['sel', 'hov', 'tileId'], () => { tools.paint(); drawOverlay(); });
  server.subscribe(['tiles'], drawOverlay);
  view.subscribe(['shown', 'draft'], () => { if (viewer.payload) surfaceHeights(viewer.payload); });
  // while a tool is placing, Esc, Enter and Backspace are its (before the page's own keys)
  addEventListener('keydown', (e) => {
    const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable;
    if (!typing && tools.key(e)) e.preventDefault();
  }, true);
}
