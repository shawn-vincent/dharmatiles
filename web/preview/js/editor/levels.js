// The panel's levels: the column beside the view shows one level of the tile at a
// time, full height (docs/preview.html, "The panel").  The tile (its own controls
// and what it holds), one thing of it (a stack entry, a line, a region: its sheet),
// one control of that (a range, a height).  Nothing opens inline: a row slides to its
// level, Back or a crumb slides back.  One strip of crumbs over the panel says where
// you are and is clickable; a strip of context under it says what the level sits in.
//
// Where you are is not a state of its own: it is the selection (view.sel, which the
// 3D view and the source already share) and the control open on it (view.control).  So
// a click on a rock in the view brings the panel to the rock's level, and a level in the
// panel is the thing the view lights.  The Tile | Source switch (view.face) turns the
// same column into the source, keeping the selection both ways.  The address is in the
// URL hash (#tile=…&at=tile/rock/0&control=size) so a reload, or the browser's back and
// forward, lands on the same level.  The model (what a level is, the crumbs, the hash)
// is js/nav.js; this part draws it and listens.  Above the tile are the folder levels
// (places.js): the crumbs here begin with the same path (Places › My tiles › dungeon › the tile).

import {
  buildHash, caretLineFor, contextFor, crumbsFor, depthOf, levelKey, levelOfHash, levelParams, moveRow, orderStack, parseHash, placeOf, sameLevel, sizeWords,
  standing, systemName, up as levelUp,
} from '../nav.js';
import { fillCrumbs, pinEnd } from '../crumbs.js';
import { levelOfParams, whereIs } from '../folders.js';
import { byteToIndex } from '../text.js';
import { $, toast } from './app.js';
import { texts } from './document.js';
import { linesOf, setSel } from './selection.js';
import {
  doc, entrySel, isInventory, isMapView, isMark, isPlates, itemOf, mapNode, markSel, server, stackItems, stackIndex, tile, tileSel, view,
} from './stores.js';
import { contextLines } from './plates.js';
import { contextLines as mapLines } from './model.js';
import { contextLines as inventoryLines } from './inventory.js';

const PANES = ['lvl-tile', 'lvl-element', 'lvl-prop'];
const SLIDE_MS = 200;
const EASE = 'cubic-bezier(.2, .7, .2, 1)';
const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

const scrolls = new Map(); // tile id + level key -> how far its list was scrolled
let shown = 0;             // the depth on screen
let pending = null;        // a level asked for by the address, until the tile has it
let started = false;
let applying = false;      // the address is being applied: it is not written back
let leftFrom = null;       // the level the last Up left, for ⌘]
let lastKey = null;        // the level the address was last written for
/** Is the editor page of the sidebar the one showing (not the tile browser)?  `view.page` is only its change; the class is what is on screen. */
const editing = () => $('side').classList.contains('editing');
const paneEl = (d) => $(PANES[d]);
const bodyOf = (d) => paneEl(d).querySelector('.body');

const entryLike = (s) => s?.kind === 'entry' || isMark(s);
const levelNow = () => ({ sel: view.get().sel, control: view.get().control });
const depthNow = () => depthOf(view.get().sel, view.get().control);
const memoryKey = (l) => `${view.get().tileId}|${levelKey(l.sel, l.control)}`;

// ---- going places ---------------------------------------------------------------

/** One level up (a control to its thing, a thing to the tile); false at the tile. */
export function goUp() {
  if (view.get().face !== 'tile') return false;
  const next = levelUp(levelNow(), tileSel());
  if (!next) return false;
  leftFrom = levelNow();
  view.set({ control: next.control });
  if (next.sel !== view.get().sel) setSel(next.sel, { from: 'view' });
  return true;
}

/** ⌘]: back down to the level the last Up left, if you are still where it left you. */
export function goForward() {
  const to = leftFrom;
  if (!to) return false;
  const here = levelNow();
  const parent = levelUp(to, tileSel());
  if (!parent || !sameLevel(here, parent)) return false;
  if (to.sel.kind === 'entry' ? stackIndex(to.sel.id) < 0 : !mapNode(to.sel)) return false;
  setSel(to.sel, { from: 'view' });
  view.set({ control: to.control });
  return true;
}

/** Go to depth `d` of the path now: 0 the tile, 1 the thing (a crumb's click). */
export function goDepth(d) {
  view.set({ face: 'tile' });
  const { sel } = view.get();
  if (d === 0) { view.set({ control: null }); if (sel?.kind !== 'tile') setSel(tileSel(), { from: 'view' }); return; }
  if (d === 1) view.set({ control: null });
}

/** Open control `name` of group `group` of the selected thing as a level. */
export function openControl(group, name) {
  if (!entryLike(view.get().sel)) return;
  view.set({ control: { name, group } });
}
export function closeControl() { if (view.get().control) view.set({ control: null }); }

// ---- the source -------------------------------------------------------------------

/** The line (from 1) of byte `at` of the text shown. */
function lineOfByte(at) {
  const t = doc.get().text || '';
  return t.slice(0, byteToIndex(t, at)).split('\n').length;
}

/** The Tile | Source switch: the same column, the source or the levels.  `focus` puts the keyboard in what shows. */
export function setFace(face, { focus = false } = {}) {
  const from = view.get().face;
  view.set({ face });
  if (face === 'source') {
    // the caret goes to the first line of what the level is (the control's own when one is open)
    const { sel } = view.get();
    const at = isPlates(tile()) ? null : sourceLine(sel, null);
    // once the source is on screen (it was not drawn a moment ago)
    requestAnimationFrame(() => { if (from !== 'source' && at) texts.placeCaret(at); if (focus) texts.focus(); });
  } else if (focus) focusLevel();
}
const sourceLine = (sel, byte) => (byte != null ? lineOfByte(byte) : sel?.kind === 'tile' ? null : caretLineFor(linesOf(sel)));

/** Flip to the source with the caret on the selected thing's line, or on byte `at` (a control's own text). */
export function showSourceAt(sel, at = null) {
  view.set({ face: 'source' });
  const n = sourceLine(sel, at);
  requestAnimationFrame(() => { if (n) texts.placeCaret(n); texts.focus(); });
}

/** Turn the panel to the source with the caret at the start of line `n`. */
export function showSourceLine(n) {
  view.set({ face: 'source' });
  requestAnimationFrame(() => { texts.placeCaret(n); texts.focus(); });
}

// ---- drawing ---------------------------------------------------------------------

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};

/** What the thing at the level is called in the crumbs. */
function thingName() {
  const { sel } = view.get();
  if (sel?.kind === 'entry') return itemOf(sel)?.label || stackItems().find((it) => it.key === sel.id)?.label || sel.id;
  return sel?.id;
}

function renderCrumbs() {
  const t = tile(), { sel, control, face } = view.get();
  const depth = depthOf(sel, control);
  // the folders the tile is in, as crumbs: Places › My tiles › dungeon › combo; each one goes to its level and takes a tile dropped on it
  const { roots } = server.get();
  const at = t && whereIs(roots, t);
  const place = at && roots[at.root] ? { root: at.root, label: roots[at.root].label, dir: at.dir } : null;
  const list = crumbsFor({ place, tileName: $('tilename').textContent, sel, what: thingName(), control });
  const up = at ? `${[roots[at.root]?.label, ...(at.dir ? at.dir.split('/') : [])].join(' › ')}: the folder (⌘[ from the tile)` : 'Places (⌘[ from the tile)';
  $('back').title = up;
  fillCrumbs($('crumbs-path'), list.filter((c) => c.depth === -1), (c) => { $('filter').value = ''; view.set({ place: c.place ? { root: c.place.root, dir: c.place.dir } : { root: null, dir: '' }, page: 'browse' }); }, { trailing: true });
  $('tilebtn').classList.toggle('cur', depth === 0 && face === 'tile');
  $('tilebtn').setAttribute('aria-current', depth === 0 ? 'location' : 'false');
  const deep = list.filter((c) => c.depth >= 1);
  const nodes = [];
  for (const c of deep) {
    const sep = el('span', 'sep', '›');
    sep.setAttribute('aria-hidden', 'true');
    const b = el('button', `crumb${c.current ? ' cur' : ''}`);
    b.type = 'button';
    b.append(el('span', '', c.label));
    b.title = c.depth === 1 ? 'Back to this thing' : c.label;
    if (c.current) b.setAttribute('aria-current', 'location');
    b.onclick = () => goDepth(c.depth);
    nodes.push(sep, b);
  }
  $('crumbs-deep').replaceChildren(...nodes);
  // where you are is the end of the path: it stays in view when the path is longer than the bar
  pinEnd($('crumb-scroll'));
  // the first half of the switch is named for what the document is: a tile's levels, a map's plates and scene
  const what = isPlates(t) ? (isMapView() ? 'Map' : 'Plates') : 'Tile';
  for (const b of $('face').children) {
    if (b.dataset.face === 'tile') {
      b.textContent = what;
      b.title = isPlates(t) ? (isMapView() ? 'The map: its pieces, inventory and settings (⌘3)' : 'The plates: printer, mode and each plate (⌘3)') : 'The tile, level by level (⌘3)';
    } else b.title = isPlates(t) ? 'The source of the map (⌘1)' : isInventory(t) ? 'The source of the inventory (⌘1)' : 'The source of the tile (⌘1)';
    const on = b.dataset.face === face;
    b.classList.toggle('on', on);
    b.setAttribute('aria-pressed', on);
  }
}

/** The strip under the crumbs: a line or two about what the level sits in. */
function renderContext() {
  const t = tile(), { sel, control } = view.get();
  const depth = depthOf(sel, control);
  const map = doc.get().map;
  const items = orderStack(stackItems());
  const own = map?.tile;
  const written = (name) => own?.controls?.find((c) => c.name === name)?.text.replace(/"/g, '');
  const tileInfo = {
    name: t?.name, size: sizeWords(written('size') ?? ''), seed: written('seed'), system: systemName(server.get().system), printer: server.get().printer,
    entries: items.length, lines: map?.lines?.length ?? 0, regions: map?.regions?.length ?? 0,
  };
  let entry = null, value = '';
  if (sel?.kind === 'entry') {
    const it = itemOf(sel);
    entry = it ? { standing: standing(items, sel.id), place: placeOf(it) } : null;
    if (control && it) {
      const g = control.group === 0 ? it.controls : it.things?.[control.group - 1]?.controls;
      const w = g?.find((c) => c.name === control.name);
      value = w ? w.text.replace(/^"|"$/g, '') : '';
    }
  } else if (isMark(sel)) {
    const list = (sel.kind === 'line' ? map?.lines : map?.regions) || [];
    const i = list.findIndex((m) => m.name === sel.id);
    entry = { note: i < 0 ? sel.kind : `${sel.kind} ${i + 1} of ${list.length}` };
  }
  const lines = contextFor({ depth, tile: tileInfo, entry, thing: depth === 2 ? thingName() : '', control: control?.name, value });
  $('ctx-lines').replaceChildren(...(isPlates(t) ? (isMapView() ? mapLines() : contextLines()) : isInventory(t) ? inventoryLines() : lines).map((l) => el('div', 'ln', l)));
}

function focusLevel() {
  const d = depthNow();
  const pane = paneEl(d);
  const first = d === 0 ? pane.querySelector('.orow') : null;
  (first || pane).focus({ preventScroll: true });
}

/** Put depth `d` on screen: slide from the old one (a deeper level comes in from the right), or swap at once. */
function showDepth(d, animate) {
  const from = shown;
  if (from === d && !paneEl(d).hidden) return;
  for (const a of document.getAnimations()) if (PANES.some((id) => $(id).contains(a.effect?.target))) a.finish();
  const to = paneEl(d), out = paneEl(from);
  PANES.forEach((id, i) => { if (i !== d && i !== from) $(id).hidden = true; });
  to.hidden = false;
  shown = d;
  restore(d);
  if (!animate || reduced() || from === d || out.hidden) { out.hidden = from === d ? false : true; return; }
  const dir = d > from ? 1 : -1;
  const opts = { duration: SLIDE_MS, easing: EASE, fill: 'both' };
  to.animate([{ transform: `translateX(${dir * 100}%)` }, { transform: 'translateX(0)' }], opts);
  const gone = out.animate([{ transform: 'translateX(0)' }, { transform: `translateX(${-dir * 100}%)` }], opts);
  // the old level leaves when the slide ends; a timer backs that up, so a page whose frames are paused still ends on one level
  let over = false;
  const done = () => {
    if (over) return;
    over = true;
    for (const a of to.getAnimations()) a.cancel();
    gone.cancel();
    if (shown === d) out.hidden = true;
  };
  gone.onfinish = done;
  setTimeout(done, SLIDE_MS + 100);
}
// the scroll of a level is kept by the level (the scroll listener in init), and given back when it is shown
function restore(d) {
  const key = memoryKey(levelNow());
  const b = bodyOf(d);
  if (b) requestAnimationFrame(() => { b.scrollTop = scrolls.get(key) ?? 0; });
}

let lastDepth = -1, lastTile = null, lastLevel = null;
function render() {
  const { sel, control, tileId, face } = view.get();
  // a control only belongs to a thing of the tile
  if (control && !entryLike(sel)) { view.set({ control: null }); return; }
  const depth = depthOf(sel, control);
  const settled = tileId === lastTile && editing() && face === 'tile' && started;
  lastTile = tileId;
  // the sheets draw themselves (sheet.js); this brings the right one on screen
  showDepth(depth, settled && depth !== lastDepth);
  const key = memoryKey(levelNow());
  if (key !== lastLevel && depth === lastDepth) restore(depth);
  lastLevel = key;
  // the keyboard follows only when it was already in the panel (a row or a crumb was pressed, or a key in it)
  if (depth !== lastDepth && settled && $('side').contains(document.activeElement)) {
    const goingUp = depth < lastDepth;
    afterSlide(() => {
      if (goingUp && depth === 0 && leftFrom?.sel) {
        // back at the tile: the row it came from is where the keyboard is
        const k = leftFrom.sel;
        const row = [...paneEl(0).querySelectorAll('.orow')].find((r) => r.dataset.kind === k.kind && r.dataset.id === k.id);
        if (row) { row.focus({ preventScroll: true }); return; }
      }
      focusLevel();
    });
  }
  lastDepth = depth;
  // a print list's panel is the plates' (plates.js), not the tile's levels or source
  const plates = isPlates(tile());
  const inventory = isInventory(tile());
  // a print list's panel is the plates', or its source (the same switch as a tile's); the inventory's own view has its own switch
  $('stage').dataset.face = plates ? (face === 'source' ? 'source' : isMapView() ? 'map' : 'plates') : inventory ? 'inventory' : face;
  $('face').hidden = inventory;
  renderCrumbs();
  renderContext();
  syncAddress();
}
const afterSlide = (fn) => setTimeout(fn, reduced() ? 0 : SLIDE_MS + 20);

// ---- the address -------------------------------------------------------------------

/** Write the level into the URL: a new level of the tile is a step of the browser's history, the rest replaces. */
function syncAddress() {
  const { tileId, sel, control, face } = view.get();
  const t = tile();
  if (!started || pending || applying || !t || tileId == null) return;
  const cur = parseHash(location.hash);
  const next = { ...cur };
  for (const k of ['at', 'piece', 'control', 'group', 'face', 'place', 'dir']) delete next[k];
  Object.assign(next, { tile: t.path }, levelParams({ sel, control, face }));
  const h = buildHash(next);
  const key = `${tileId}|${levelKey(sel, control)}`;
  if (h === location.hash) { lastKey = key; return; }
  // a step in the source is not a step of navigation, and neither is a different piece of the same entry;
  // coming in from a folder level (whose address has no tile) is a step: Back goes to the folder
  const push = (lastKey != null && key !== lastKey && face === 'tile' && tileId === lastKey.split('|')[0]) || (cur.tile == null && cur.place != null);
  lastKey = key;
  if (push) history.pushState(null, '', h); else history.replaceState(null, '', h);
}

/** Where the panel is, to be taken again after the tile is swapped for its copy (a first change to a library tile, a rename). */
export const levelSnapshot = () => ({ sel: view.get().sel && entryLike(view.get().sel) ? { ...view.get().sel } : { kind: 'tile' }, control: view.get().control, face: view.get().face });
/** Go back to a level kept by `levelSnapshot`, once the tile now shown has it. */
export const restoreLevel = (l) => wantLevel(l);

/** Take the level an address asks for, as soon as the tile has it (its text is checked). */
function wantLevel(want) {
  pending = want;
  tryPending();
}
function tryPending() {
  if (!pending) return;
  const { tileId } = view.get(), d = doc.get();
  if (tileId == null || d.tileId !== tileId || d.checked !== d.text || d.text == null) return;
  const want = pending;
  pending = null;
  applying = true;
  try {
    const { control, face } = want;
    const sel = want.sel.kind === 'tile' ? tileSel() : want.sel;
    let ok = true;
    if (sel.kind === 'entry') ok = stackIndex(sel.id) >= 0;
    else if (isMark(sel)) ok = !!mapNode(sel);
    if (!ok) {
      toast(`This tile has no ${sel.kind === 'entry' ? 'such entry' : `${sel.kind} ${sel.id}`} now`);
      view.set({ face, control: null });
      setSel(tileSel(), { from: 'view' });
    } else {
      view.set({ face, control: null });
      setSel(sel, { from: 'view' });
      if (control) view.set({ control });
    }
  } finally { applying = false; }
  render();
}

/** The browser's back or forward, or an address pasted over the page. */
function fromAddress() {
  const h = parseHash(location.hash);
  // a folder level's address is the folder level's (places.js)
  if (levelOfParams(server.get().roots, h)) return;
  const t = h.tile ? server.get().tiles.find((x) => x.path === h.tile) : null;
  const want = levelOfHash(location.hash);
  if (t && t.id !== view.get().tileId) {
    // another tile: choose it, and take the level once it is open
    import('./open.js').then(({ select }) => select(t.id)).then((ok) => { if (ok !== false) wantLevel(want); });
    return;
  }
  view.set({ page: 'edit' });
  wantLevel(want);
}

// ---- keys --------------------------------------------------------------------------

const typing = (e) => e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';

function onKey(e) {
  const mod = (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey;
  if (mod && (e.key === '[' || e.key === ']') && view.get().face === 'tile') {
    if (!editing()) {
      e.preventDefault();
      if (e.key === ']') { if (tile()) view.set({ page: 'edit' }); } else $('bback').click(); // up a folder level
      return;
    }
    e.preventDefault();
    if (e.key === '[') { if (!goUp()) $('back').click(); } else goForward();
    return;
  }
  // the rows of the tile level
  const row = e.target.closest?.('#lvl-tile .orow');
  if (row && !e.metaKey && !e.ctrlKey && !e.altKey && ['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) {
    const rows = [...$('lvl-tile').querySelectorAll('.orow')];
    const to = rows[moveRow(rows.indexOf(row), e.key, rows.length)];
    e.preventDefault();
    to?.focus();
    return;
  }
  if (!typing(e) && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === 'ArrowDown' && e.target.id === 'lvl-tile') {
    e.preventDefault();
    $('lvl-tile').querySelector('.orow')?.focus();
  }
}

/** The row under the keyboard lights its thing in the view (hover), as the pointer does. */
function rowFocus(e) {
  const row = e.target.closest?.('.orow');
  if (!row || row.dataset.kind === 'add') return;
  view.set({ hov: row.dataset.kind === 'entry' ? entrySel(row.dataset.id) : markSel(row.dataset.kind, row.dataset.id) });
}
function rowBlur(e) { if (e.target.closest?.('.orow') && e.target.closest('.orow').dataset.kind !== 'add' && !e.relatedTarget?.closest?.('.orow')) view.set({ hov: null }); }

// ---- wiring --------------------------------------------------------------------------

/** Wire this part of the page (once, at load). */
export function init() {
  pending = null;
  const first = levelOfHash(location.hash);
  const hasLevel = first.sel.kind !== 'tile' || first.face === 'source';
  if (hasLevel) pending = first;
  started = true;
  // the tile browser is a level above the tile: its crumb is the back button already
  $('tilebtn').onclick = () => goDepth(0);
  for (const b of $('face').children) b.onclick = () => setFace(b.dataset.face);
  for (const id of PANES) {
    const p = $(id);
    p.addEventListener('scroll', (e) => { if (e.target.classList?.contains('body')) scrolls.set(memoryKey(levelNow()), e.target.scrollTop); }, true);
  }
  $('lvl-tile').addEventListener('focusin', rowFocus);
  $('lvl-tile').addEventListener('focusout', rowBlur);
  view.subscribe(['sel', 'control', 'tileId', 'page', 'face', 'dmode'], render);
  // the thing's name in the crumbs and the strip follow the text
  doc.subscribe(['stack', 'map', 'file', 'tileId', 'checked'], () => { renderCrumbs(); renderContext(); tryPending(); });
  server.subscribe(['tiles', 'system'], () => { renderCrumbs(); renderContext(); });
  addEventListener('keydown', onKey);
  addEventListener('popstate', fromAddress);
  render();
  tryPending();
}
