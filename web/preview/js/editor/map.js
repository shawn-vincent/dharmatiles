// The map over the view, the bar and its tools.

import { valueOf } from '../lens.js';
import { Overlay } from '../overlay.js';
import { Tools } from '../place.js';
import { byteToIndex } from '../text.js';
import { $, host, isEntry, state, tile, toast, viewer } from './app.js';
import { askDraft, doc, edited, elements, loadText, mapNode, renderTextBar, stackItems, texts, undo } from './document.js';
import { canvas, setSel } from './selection.js';
import { lens } from './sheet.js';
import { offerMine } from './tiles.js';

export let tools = null; // the bar's tools (below)

// ---- the map, the bar and its tools ----------------------------------------

const overlay = new Overlay($('over'), viewer);
const SURFACES = new Set(['ground', 'dirt', 'floor', 'water']);
const COLOUR = { water: '--water', dirt: '--earth', ground: '--earth', floor: '--stone', grass: '--moss', reeds: '--moss', flowers: '--bloom' };
const cssVar = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
export const controlText = (it, name) => [...(it.place || []), ...(it.controls || [])].find((c) => c.name === name)?.text.replace(new RegExp(`^${name}=`), '').replace(/"/g, '');

/** The ground's heights, from the surfaces the build made. */
export function surfaceHeights(p) {
  const items = stackItems();
  overlay.setHeights(p, (e) => SURFACES.has(items[e]?.element), tile()?.plan?.size_mm || p.meta.footprint);
}

/** Each region's tint: the colour of the last thing laid in it. */
function tints() {
  const out = {};
  for (const it of stackItems()) {
    const r = controlText(it, 'in');
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
  const s = state.sel;
  if (!s || !doc.writable || texts.file !== tile()?.path || !doc.map) return [];
  if (s.line != null) {
    const l = mapNode(s);
    return (l?.points || []).filter((p) => p.at).map((p, k) => ({
      q: p.at, span: p.span, snap: true,
      move(q) { if (!l.wobble && l.pts.length === l.points.length) l.pts[k] = q; },
    }));
  }
  if (s.region != null) return (mapNode(s)?.fills || []).filter((f) => f.at).map((f) => ({ q: f.at, span: f.span, snap: false, move(q) { f.at = q; } }));
  if (isEntry(s)) {
    const it = stackItems()[s.entry];
    const c = (it?.place || []).find((x) => x.name === 'at');
    if (!c || it.file !== texts.file) return [];
    const v = valueOf(c.text, 'at');
    const m = v.text.match(/^"(-?[\d.]+),(-?[\d.]+)"$/);
    if (!m) return [];
    const start = c.span[0] + new TextEncoder().encode(c.text.slice(0, v.at)).length;
    return [{ q: [Number(m[1]), Number(m[2])], span: [start, new TextEncoder().encode(v.text).length], snap: false }];
  }
  return [];
}

export function drawOverlay() {
  const t = tile();
  overlay.setMap(doc.tile === state.selected ? doc.map : null, t?.plan?.square_mm);
  const s = state.sel;
  overlay.draw({
    tints: tints(),
    selRegion: s?.region ?? null,
    selLine: s?.line ?? null,
    all: !!tools?.tool,
    ...(tools?.scene() || {}),
  });
}

/** Write a new node from a gesture in the view, then select it. */
async function insertNode(place, node, { select: sel } = {}) {
  const t = tile();
  if (!t) return;
  if (!doc.writable) return offerMine();
  if (texts.file !== t.path && !(await loadText(null))) return;
  const r = await host.edit({ op: 'insert', text: texts.text, place, node });
  if (r.status !== 200) { doc.note = r.body?.error || 'that could not be written'; renderTextBar(); return; }
  texts.setText(r.body.text, byteToIndex(r.body.text, r.body.at));
  if (sel) { state.sel = null; setSel(sel, { from: 'view' }); } else doc.selectAt = r.body.at;
  edited(texts.text, 'view', `Added ${node.trim().split(/\s+/)[0]}`);
  askDraft();
  const l = texts.text.slice(0, byteToIndex(texts.text, r.body.at)).split('\n').length;
  texts.reveal(l);
}

// each element's family, for its colour (the docs site uses the same)
export const FAMILY = {
  ground: 'surface', dirt: 'surface', floor: 'surface', water: 'water',
  rock: 'thing', stone: 'thing', boulder: 'thing', pebble: 'thing', scatter: 'thing', cliff: 'thing', wall: 'thing',
  grass: 'plant', reeds: 'plant', flowers: 'plant', tree: 'plant',
  dungeonblocks: 'base', openlock: 'base', connector: 'base', stl: 'base',
};

export function muteAt(byte, label = 'Entry') {
  if (!doc.writable) return offerMine();
  const t = texts.text, i = byteToIndex(t, byte);
  if (isEntry(state.sel)) setSel(null);
  texts.setText(`${t.slice(0, i)}/- ${t.slice(i)}`);
  edited(texts.text, 'view', `Hid ${label}`);
  askDraft();
  toast(`${label} hidden`, () => undo(-1));
}
export async function removeAt(byte, label = 'Entry') {
  if (!doc.writable) return offerMine();
  const r = await host.edit({ op: 'remove', text: texts.text, at: byte });
  if (r.status !== 200) { doc.note = r.body?.error || 'that could not be taken out'; renderTextBar(); return; }
  setSel(null);
  texts.setText(r.body.text);
  edited(texts.text, 'view', `Deleted ${label}`);
  askDraft();
  toast(`${label} removed`, () => undo(-1));
}

/** Wire this part of the page (once, at load). */
export function init() {
  tools = new Tools(canvas, viewer, overlay, { bar: $('place'), input: $('place-in'), list: $('place-list'), chip: $('tool') }, {
    elements: () => elements,
    defs: () => [...(texts.text || '').matchAll(/^\s*define\s+([\w+\-.]+)/gm)].map((m) => m[1]),
    sq: () => tile()?.plan?.square_mm || 35,
    size: () => [tile()?.plan?.cols || 1, tile()?.plan?.rows || 1],
    map: () => doc.map,
    writable: () => doc.writable,
    offerMine: () => offerMine(),
    insert: (place, node, opts) => insertNode(place, node, opts),
    replace: (start, len, text, end) => lens.api.replace(start, len, text, end),
    handles,
    redraw: () => drawOverlay(),
    note: (text) => { const h = $('hint'); h.textContent = text; h.style.display = 'block'; h.style.left = '12px'; h.style.top = '56px'; setTimeout(() => { h.style.display = 'none'; }, 2600); },
  });
  $('tool').querySelector('button').onclick = () => tools.stop();
}
