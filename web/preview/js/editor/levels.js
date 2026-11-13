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
//
// This is the one shell of the levels, for every kind of document: a tile's levels (tile, entry, control) and a map's (map, piece,
// property, and the folders its Add step walks: each a level, in the sections `add` and `add2` in turn) are views drawn into its sections (`lvl-<id>`, one per row of nav.js `LEVELS`), and the shell alone decides which one is on
// screen, which way it slides (nav.js `slideBetween`), what it keeps (each level's scroll), where the keyboard goes after a slide and how
// reduced motion reads (all of it js/slide.js, which the folder levels and the step between a folder and a document use too, `showPage`).  A view never shows or hides itself, never animates and never says which level is on screen (a test reads the
// views for it); a level added later is a row of `LEVELS`, a section of the page and a view.

import {
  ADD_PANES, LEVELS, caretLineFor, contextFor, crumbsFor, depthOf, folderDepth, levelDepth, levelKey, levelOfHash, moveRow, orderStack, parseHash, placeOf,
  sameLevel, slideBetween, slideDir, standing, tileLevelOf, trailDepth, up as levelUp,
} from '../nav.js';
import { effect } from '@preact/signals-core';
import { afterSlide, finishSlide, slide } from '../slide.js';
import { fillCrumbs, pinEnd } from '../crumbs.js';
import { DeepCrumbs } from '../view/crumbs.js';
import { html, render as renderInto } from '../view/html.js';
import { ContextLines } from '../view/places-panel.js';
import { Icon } from '../view/icons.js';
import { levelOfParams, parentLevel, whereIs } from '../folders.js';
import { byteToIndex } from '../text.js';
import { $, toast } from './app.js';
import { texts } from './document.js';
import { linesOf, setSel } from './selection.js';
import {
  doc, entrySel, isInventory, isMapView, isMark, isPlates, itemOf, mapNode, markSel, placeAt, server, stackItems, stackIndex, stem, tile, tileSel, view,
} from './stores.js';
import { contextLines } from './plates.js';
import { addLevel, contextLines as mapLines, level as mapLevel, mprop as mapProp, scene as mapScene, selected as mapSelected } from './model.js';
import { held } from './address.js';
import { label as pieceLabel } from '../map-model.js';
import { contextLines as inventoryLines } from './inventory.js';

/** What a level's rows are, for the arrow keys and for where the keyboard lands: the tile's rows, the map's pieces, the Add step's folders and pictures. */
const ROWS = '.orow, .mp-group, .acard';
/** What scrolls in a level: its body, or the folder list of the Add step. */
const SCROLLS = '.body, .flvl';

const scrolls = new Map(); // tile id + level key -> how far its list was scrolled
let shown = { id: 'tile' }; // the level on screen: a row of LEVELS, and for the Add step the folder it is at
let pending = null;        // a level asked for by the address, until the tile has it
let started = false;
let applying = false;      // the address is being applied: it is not written back
let leftFrom = null;       // the level the last Up left, for ⌘]
let leftMap = null;        // the same for a map: the pieces and the property the last Up left
/** Is the editor page of the sidebar the one showing (not the tile browser)?  `view.page` is only its change; the class is what is on screen. */
const editing = () => $('side').classList.contains('editing');
const paneEl = (id) => $(`lvl-${id}`);
const scrollerOf = (id) => paneEl(id).querySelector(SCROLLS);

/** Tell the address's writer (address.js) whether a level it was given is still being taken. */
const hold = () => { held.value = !!pending || applying; };

const entryLike = (s) => s?.kind === 'entry' || isMark(s);
const levelNow = () => ({ sel: view.get().sel, control: view.get().control });
/** The key a folder of the Add step keeps its scroll under. */
const folderKey = (l) => `add|${l.root ?? 'places'}|${l.dir || ''}`;
/** The level the shell shows now, `{id, key}`: a row of LEVELS and the key its scroll is kept under (and, in the Add step, the `folder` it is at); null while the document has no levels (plates, inventory). */
function shellLevel() {
  const t = tile();
  if (isInventory(t)) return null;
  if (isPlates(t)) {
    if (!isMapView()) return null;
    const id = mapLevel.value;
    if (ADD_PANES.includes(id)) return { id, key: folderKey(addLevel.value), folder: addLevel.value };
    return { id, key: id === 'map' ? id : `${id}:${mapSelected.value.map((p) => p.id).join(',')}${id === 'prop' ? `/${mapProp.value}` : ''}` };
  }
  const { sel, control } = view.get();
  return { id: tileLevelOf(sel, control), key: levelKey(sel, control) };
}
const memoryKey = (l) => `${view.get().tileId}|${l.key}`;

// ---- going places ---------------------------------------------------------------

/** One level up (a control to its thing, a thing to the tile); false at the tile. */
export function goUp() {
  if (view.get().face !== 'tile') return false;
  if (isMapView()) {
    // a map's levels are the same path: a property to its pieces, the pieces to the map
    const { mprop, msel, madd } = view.get();
    if (mprop) { leftMap = { msel, mprop, add: null }; view.set({ mprop: null }); return true; }
    if (msel.length) { leftMap = { msel, mprop: null, add: null }; view.set({ msel: [] }); return true; }
    if (madd) {
      // the Add step's folders are steps of the same path: a folder to the one it is in, Places to the map
      const here = addLevel.value, up = parentLevel(here);
      leftMap = { msel: [], mprop: null, add: here };
      if (up) goAdd(up); else view.set({ madd: false });
      return true;
    }
    return false;
  }
  const next = levelUp(levelNow(), tileSel());
  if (!next) return false;
  leftFrom = levelNow();
  view.set({ control: next.control });
  if (next.sel !== view.get().sel) setSel(next.sel, { from: 'view' });
  return true;
}

/** ⌘]: back down to the level the last Up left, if you are still where it left you. */
export function goForward() {
  if (isMapView()) {
    const to = leftMap;
    if (!to) return false;
    const have = new Set(mapScene.value?.pieces.map((p) => p.id));
    const ids = to.msel.filter((i) => have.has(i));
    const here = mapLevel.value;
    if (to.add) {
      // back down into the folder the last Up left: from the folder above it, or from the map when it left Places
      const up = parentLevel(to.add);
      if (!(up ? ADD_PANES.includes(here) && sameFolder(addLevel.value, up) : here === 'map')) return false;
      goAdd(to.add);
      return true;
    }
    if (to.mprop && here === 'piece' && ids.length === 1 && mapSelected.value[0]?.id === ids[0]) view.set({ mprop: to.mprop });
    else if (!to.mprop && here === 'map' && ids.length) view.set({ msel: ids });
    else return false;
    return true;
  }
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

const sameFolder = (a, b) => (a.root ?? null) === (b.root ?? null) && (a.dir || '') === (b.dir || '');

/**
 * Go to the Add step at folder `level` (`{root, dir}`; the folder it was last at, else where it starts, with none): from the map it opens, from a
 * folder it is a step to another folder, which takes the other of the step's two sections so that the shell slides between them.  The one
 * way into the Add step, for a press on a row, a crumb, the keys and an address.
 */
export function goAdd(level = null) {
  const { madd, addSlot } = view.get();
  const to = level ? { root: level.root ?? null, dir: level.dir || '' } : addLevel.peek();
  if (madd && sameFolder(to, addLevel.peek())) return;
  view.set({ msel: [], mprop: null, madd: true, addAt: to, ...(madd ? { addQuery: '', addSlot: 1 - addSlot } : {}) });
}

/** A crumb of the trail below the tile: a folder of the Add step goes to it, any other goes to its depth. */
export function goCrumb(c) {
  if (c.add) { view.set({ face: 'tile' }); goAdd(c.add); } else goDepth(c.depth);
}

/** Go to depth `d` of the path now: 0 the tile, 1 the thing (a crumb's click). */
export function goDepth(d) {
  view.set({ face: 'tile' });
  if (isMapView()) {
    if (d === 0) view.set({ msel: [], mprop: null, madd: false }); else if (d === 1) view.set({ mprop: null });
    return;
  }
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
  } else if (focus) focusLevel(shown.id);
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

/** What the thing at the level is called in the crumbs. */
function thingName() {
  const { sel } = view.get();
  if (sel?.kind === 'entry') return itemOf(sel)?.label || stackItems().find((it) => it.key === sel.id)?.label || sel.id;
  return sel?.id;
}

const PROPS = { bottom: 'Bottom', seed: 'Seed', print: 'Print' };

/**
 * The levels below the tile, for the crumbs: `{depth, what, control}`.  A tile's are its thing and control; a map's are
 * its pieces (the piece's name, or how many) and the property open on one.  The same trail, the same crumbs.
 */
function trailNow() {
  if (isMapView()) {
    const ps = mapSelected.value;
    const id = mapLevel.value;
    const prop = id === 'prop' ? mapProp.value : null;
    if (ADD_PANES.includes(id)) return { depth: levelDepth(id, addLevel.value), add: addLevel.value };
    return { depth: LEVELS[id].depth, what: ps.length > 1 ? `${ps.length} pieces` : ps[0] ? pieceLabel(ps[0]) : '', control: PROPS[prop] ?? prop ?? '' };
  }
  const { sel, control } = view.get();
  return { depth: depthOf(sel, control), what: thingName(), control: control?.name ?? '' };
}

function renderCrumbs() {
  const t = tile(), { face } = view.get();
  const { depth, what: thing, control: ctl, add } = trailNow();
  // the folders the tile is in, as crumbs: Places › My tiles › dungeon › combo; each one goes to its level and takes a tile dropped on it
  const { roots } = server.get();
  const at = t && whereIs(roots, t);
  const place = at && roots[at.root] ? { root: at.root, label: roots[at.root].label, dir: at.dir } : null;
  // the Add step's folders are crumbs of the same trail, below the map: Add › Places › My tiles › dungeon
  const addRoot = add && add.root != null ? roots[add.root] : null;
  const list = crumbsFor({ place, tileName: t ? stem(t) : '', depth, what: thing, control: ctl, add: add ? (addRoot ? { root: add.root, label: addRoot.label, dir: add.dir } : null) : undefined });
  const up = at ? `${[roots[at.root]?.label, ...(at.dir ? at.dir.split('/') : [])].join(' › ')}: the folder (⌘[ from the tile)` : 'Places (⌘[ from the tile)';
  $('back').title = depth > 0 ? 'Up a level (Esc, ⌘[)' : up;
  fillCrumbs($('crumbs-path'), list.filter((c) => c.depth === -1), (c) => { view.set({ placeAt: placeAt(c.place ? { root: c.place.root, dir: c.place.dir } : { root: null, dir: '' }), query: '', pageAt: 'browse' }); }, { trailing: true });
  $('tilebtn').classList.toggle('cur', depth === 0 && face === 'tile');
  $('tilebtn').setAttribute('aria-current', depth === 0 ? 'location' : 'false');
  renderInto(html`<${DeepCrumbs} crumbs=${list.filter((c) => c.depth >= 1)} go=${goCrumb} />`, $('crumbs-deep'));
  // where you are is the end of the path: it stays in view when the path is longer than the bar
  pinEnd($('crumb-scroll'));
  for (const b of $('face').children) {
    if (b.dataset.face === 'tile') {
      b.textContent = 'Outline'; // what the panel shows, whatever the document is (the title bar's switch names the document)
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
  const tileInfo = {
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
  let lines = contextFor({ depth, tile: tileInfo, entry, thing: depth === 2 ? thingName() : '', control: control?.name, value });
  const other = isPlates(t) || isInventory(t);
  // the one band of a level: the name of what the level is (24 px display), then one quiet line (what it is, where it stands, its help)
  let name = '';
  if (!other && depth > 0) {
    const it = sel?.kind === 'entry' ? itemOf(sel) : null;
    name = control ? control.name : depth === 2 ? thingName() : it?.label || thingName();
    if (depth === 1) {
      const kind = it ? it.what || it.element : sel?.kind;
      lines = [html`${[kind, lines[0]].filter(Boolean).join(' · ')}${it?.href ? html` · <a class="help" href=${it.href} target="_blank" rel="noopener" title=${`Help for ${name}`}>Help <${Icon} name="external-link" size=${12} /></a>` : null}`];
    }
  }
  renderInto(html`${name ? html`<div class="ctx-name" title=${name}>${name}</div>` : null}<${ContextLines} lines=${isPlates(t) ? (isMapView() ? mapLines() : contextLines()) : isInventory(t) ? inventoryLines() : lines} />`, $('ctx-lines'));
  $('ctx').classList.toggle('named', !!name);
}

/** Put the keyboard in level `id`: its first row (the tile's rows, the map's; the Add step's folders and pictures before its list of pieces), else the level itself. */
function focusLevel(id) {
  const pane = paneEl(id);
  const first = pane.querySelector('.flvl .orow, .flvl .acard') || pane.querySelector(ROWS);
  (first || pane).focus({ preventScroll: true });
}

/** Put level `now` (`{id, folder}`, from `shellLevel`) on screen: the one slide (js/slide.js) from the old one, which way nav.js `slideBetween` says, or a swap at once. */
function showLevel(now, animate) {
  const { id } = now, from = shown;
  if (from.id === id && !paneEl(id).hidden) { shown = now; return; }
  const to = paneEl(id), out = paneEl(from.id);
  finishSlide($('levels')); // a slide still running ends first, so the panes left over are hidden
  for (const l of Object.keys(LEVELS)) if (l !== id && l !== from.id) paneEl(l).hidden = true;
  to.hidden = false;
  shown = now;
  restore(id);
  slide({ scope: $('levels'), to, out: from.id !== id && !out.hidden ? out : null, dir: slideBetween(from, now), animate, leave: (el) => { el.hidden = true; } });
}
// the scroll of a level is kept by the level (the scroll listener in init), and given back when it is shown
function restore(id) {
  const l = shellLevel();
  const b = scrollerOf(id);
  if (l && b) requestAnimationFrame(() => { b.scrollTop = scrolls.get(memoryKey(l)) ?? 0; });
}

// ---- the two pages: the folder browser and the document -------------------------------------------------------------
// The sidebar is two pages stacked in one box: the folder levels (places.js) and the document's levels.  Opening a document from its
// folder is a step one deeper in the same trail, Up from it one shallower, so the step between the pages is the same slide as any other
// (slide.js), by the same rule, measured on the trail's whole depth.

const TOP = { root: null, dir: '' };
/** How deep the page is in the trail: the folder level the browser is at, or the document's own level below the folder it is in. */
function pageDepth(edit) {
  if (!edit) return folderDepth(view.get().place || TOP);
  const t = tile();
  const l = shellLevel();
  return trailDepth((t && whereIs(server.get().roots, t)) || TOP, l ? levelDepth(l.id, l.folder) : 0);
}
let pageIsEdit = null;
/**
 * Bring the browser page (`edit` false) or the document's on screen: the step is the trail's depth before and after, read here, before
 * the new page's level can change it.  The keyboard follows only when it was in the page left (as a level's does).
 */
export function showPage(edit, animate) {
  if (pageIsEdit === edit) return;
  const to = $(edit ? 'editpage' : 'browser'), out = $(edit ? 'browser' : 'editpage');
  const first = pageIsEdit == null;
  finishSlide($('track')); // a step still running ends first
  const dir = first ? 0 : slideDir(pageDepth(!edit), pageDepth(edit));
  const had = out.contains(document.activeElement);
  pageIsEdit = edit;
  $('side').classList.toggle('editing', edit);
  to.classList.remove('off'); to.inert = false;
  out.inert = true;
  const away = (el) => { el.classList.add('off'); };
  if (first) { away(out); return; }
  slide({ scope: $('track'), to, out, dir, animate, leave: away });
  if (had) afterSlide(() => { if (pageIsEdit === edit) { if (edit) focusLevel(shown.id); else ($('folder').querySelector('.orwrap.sel .orow, .orow') || $('folder')).focus({ preventScroll: true }); } });
}

let lastLevel = null, lastTile = null, lastKey = null;
function render() {
  const { sel, control, tileId, face } = view.get();
  // a control only belongs to a thing of the tile
  if (control && !entryLike(sel) && !isPlates(tile())) { view.set({ control: null }); return; }
  const now = shellLevel();
  const settled = tileId === lastTile && editing() && face === 'tile' && started;
  lastTile = tileId;
  if (now) {
    // the views draw themselves (view/*.js, editor/sheet.js, scene-panel.js); this brings the right one on screen
    const from = lastLevel;
    showLevel(now, settled && now.id !== from?.id);
    const key = memoryKey(now);
    if (key !== lastKey && now.id === from?.id) restore(now.id);
    lastKey = key;
    // the keyboard follows only when it was already in the panel (a row or a crumb was pressed, or a key in it)
    if (now.id !== from?.id && settled && $('side').contains(document.activeElement)) {
      const dir = slideBetween(from, now);
      afterSlide(() => {
        const row = dir < 0 && returnRow(now.id);
        if (row) { row.focus({ preventScroll: true }); return; }
        focusLevel(now.id);
      });
    }
    lastLevel = now;
  }
  // a print list's panel is the plates' (plates.js), not the levels or source
  const plates = isPlates(tile()) && !isMapView();
  const inventory = isInventory(tile());
  // a print list's panel is the plates', or its source (the same switch as a tile's); the inventory's own view has its own switch
  $('stage').dataset.face = plates ? (face === 'source' ? 'source' : 'plates') : inventory ? 'inventory' : face;
  $('face').hidden = inventory;
}
/** Back up at the top of a document: the row the keyboard came from (the entry's, the piece's), when the shell still has it. */
function returnRow(id) {
  const rows = [...paneEl(id).querySelectorAll(ROWS)];
  if (isMapView()) {
    // up from a folder of the Add step: the row of the folder it left; else the piece the level was left from
    const left = leftMap?.add;
    if (left && ADD_PANES.includes(id)) return rows.find((r) => (left.dir ? r.dataset.kind === 'folder' && r.dataset.id === left.dir : r.dataset.kind === 'place' && r.dataset.id === String(left.root)));
    const first = leftMap?.msel[0];
    return first ? rows.find((r) => r.dataset.ids?.split(',').includes(first)) : null;
  }
  const k = leftFrom?.sel;
  return id === 'tile' && k ? rows.find((r) => r.dataset.kind === k.kind && r.dataset.id === k.id) : null;
}

// ---- the address -------------------------------------------------------------------
// (written by one effect for every level: address.js; this takes a level an address asks for)

/** Where the panel is, to be taken again after the tile is swapped for its copy (a first change to a library tile, a rename). */
export const levelSnapshot = () => ({ sel: view.get().sel && entryLike(view.get().sel) ? { ...view.get().sel } : { kind: 'tile' }, control: view.get().control, face: view.get().face });
/** Go back to a level kept by `levelSnapshot`, once the tile now shown has it. */
export const restoreLevel = (l) => wantLevel(l);

/** Take the level an address asks for, as soon as the tile has it (its text is checked). */
function wantLevel(want) {
  pending = want;
  hold();
  tryPending();
}
/** A map's pieces and property an address asks for, once the map's scene has come: the pieces that are there, as the selection. */
function takePieces(want) {
  const have = new Set(mapScene.value.pieces.map((p) => p.id));
  const ids = want.pieces.filter((i) => have.has(i));
  if (ids.length < want.pieces.length) toast(ids.length ? 'Some of those pieces are not on the map now' : 'The map has no such piece now');
  view.set({ face: want.face, msel: ids, mprop: ids.length === 1 && want.prop ? want.prop : null, madd: false });
  if (!want.add || ids.length) return;
  // the Add step at the folder the address names: the place by its key (a place the program does not list is said, and the step opens where it starts)
  const at = levelOfParams(server.get().roots, want.add);
  if (at?.missing != null) toast(`There is no place “${at.missing}” now`);
  goAdd(at?.missing == null ? at : null);
}
function tryPending() {
  if (!pending) return;
  const { tileId } = view.get(), d = doc.get();
  if (isPlates(tile())) {
    // a map's levels: the map's scene says which pieces there are (a print list has no levels but its face)
    if (view.get().dmode == null || (isMapView() && !mapScene.value)) return;
    const want = pending;
    pending = null;
    applying = true;
    try {
      if (isMapView()) takePieces({ ...want, pieces: want.pieces ?? [] });
      else view.set({ face: want.face });
    } finally { applying = false; }
    hold();
    return;
  }
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
  hold();
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
    // another tile: choose it, and take the level once it is open (the address is not written over meanwhile)
    pending = want;
    hold();
    import('./open.js').then(({ select }) => select(t.id)).then((ok) => { if (ok !== false) wantLevel(want); else { pending = null; hold(); } });
    return;
  }
  view.set({ pageAt: 'edit' });
  wantLevel(want);
}

// ---- keys --------------------------------------------------------------------------

const typing = (e) => e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';

function onKey(e) {
  const mod = (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey;
  if (mod && (e.key === '[' || e.key === ']') && view.get().face === 'tile') {
    if (!editing()) {
      e.preventDefault();
      if (e.key === ']') { if (tile()) view.set({ pageAt: 'edit' }); } else $('bback').click(); // up a folder level
      return;
    }
    e.preventDefault();
    if (e.key === '[') { if (!goUp()) $('back').click(); } else goForward();
    return;
  }
  // the rows of a level: the tile's, the map's
  const row = e.target.closest?.(ROWS.split(', ').map((r) => `.lvl ${r}`).join(', '));
  if (row && !e.metaKey && !e.ctrlKey && !e.altKey && ['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) {
    const rows = [...row.closest('.lvl').querySelectorAll(ROWS)];
    const to = rows[moveRow(rows.indexOf(row), e.key, rows.length)];
    e.preventDefault();
    to?.focus();
    return;
  }
  if (!typing(e) && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === 'ArrowDown' && e.target.classList?.contains('lvl')) {
    e.preventDefault();
    e.target.querySelector(ROWS)?.focus();
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
  const hasLevel = first.sel.kind !== 'tile' || first.face === 'source' || first.pieces || first.add;
  if (hasLevel) pending = first;
  hold();
  started = true;
  // the tile browser is a level above the tile: its crumb is the back button already
  $('tilebtn').onclick = () => goDepth(0);
  for (const b of $('face').children) b.onclick = () => setFace(b.dataset.face);
  for (const id of Object.keys(LEVELS)) {
    paneEl(id).addEventListener('scroll', (e) => {
      const l = shellLevel();
      if (l && l.id === id && e.target.matches?.(SCROLLS)) scrolls.set(memoryKey(l), e.target.scrollTop);
    }, true);
  }
  $('lvl-tile').addEventListener('focusin', rowFocus);
  $('lvl-tile').addEventListener('focusout', rowBlur);
  // the panes, the stage and the address follow where you are; the crumbs and the strip follow what they say; the level
  // asked for by an address is taken as soon as the tile has it.  Each reads what it shows, so each runs when that changes.
  effect(() => { void view.get().page; void view.get().dmode; render(); });
  effect(() => { renderCrumbs(); });
  effect(() => { renderContext(); });
  effect(() => { const d = doc.get(); void d.tileId; void d.checked; void d.text; void view.get().tileId; void mapScene.value; void view.get().dmode; tryPending(); });
  addEventListener('keydown', onKey);
  addEventListener('popstate', fromAddress);
}
