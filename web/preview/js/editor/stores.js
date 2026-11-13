// The editor's state, in three small stores and the selectors that read them.
// A store is get / set / subscribe: `set` merges a patch and tells the
// subscribers (those who named a key in it, or everyone) what changed;
// `get` returns the state itself, to be read and never written.
//
//   server  what the program says: the tiles, the roots, the settings, the
//           elements, and whether the event stream is up.  Only live.js
//           writes it.
//   view    what you are looking at: the tile, the selection, the camera's
//           view, the mesh shown, the text file wanted.
//   doc     the text being edited: the file, what is on disk, what was
//           checked, the stack and the map of that text.
//
// A part of the page renders by subscribing to the keys it draws from;
// none of them calls another to say that something changed.

import { batch, computed, effect, signal, untracked } from '@preact/signals-core';
import { showError } from '../problems.js';
import { createLog } from '../messages.js';
import { settledLevel, whereIs } from '../folders.js';

// `report(what, err)` takes what a subscriber threw: the error panel by default.
//
// Each key of a store is a signal (@preact/signals-core), and `get()` is an object whose properties read them: read
// inside a component or a `computed`, a key is followed by whoever read it; read anywhere else, it is a plain read.
// `set` writes the keys in one batch.  `subscribe` is for the parts of the page not yet written as components.
export function createStore(initial, report = showError) {
  const sigs = new Map();
  const state = /** @type {typeof initial} */ ({});
  const subs = new Set();
  const own = (k, v) => {
    const s = signal(v);
    sigs.set(k, s);
    Object.defineProperty(state, k, { enumerable: true, configurable: true, get: () => s.value });
    return s;
  };
  for (const [k, v] of Object.entries(initial)) own(k, v);
  const tell = (changed) => {
    // every subscriber runs, and one that throws is reported, never allowed to stop the rest
    for (const s of [...subs]) {
      if (s.keys && !changed.some((k) => s.keys.includes(k))) continue;
      try { s.fn(state, changed); } catch (e) { report(`A part of the page failed to follow a change (${changed.join(', ')})`, e); }
    }
  };
  return {
    get: () => state,
    /** A key worked out from other state and stored nowhere: `fn` is read like any other key, and a change of its value is told to the subscribers of its key. */
    derive(k, fn) {
      const c = computed(fn);
      Object.defineProperty(state, k, { enumerable: true, configurable: true, get: () => c.value });
      let last = c.peek();
      effect(() => { const v = c.value; if (!Object.is(v, last)) { last = v; untracked(() => tell([k])); } });
    },
    set(patch) {
      const changed = Object.keys(patch).filter((k) => !Object.is(untracked(() => state[k]), patch[k]));
      if (!changed.length) return;
      batch(() => { for (const k of changed) (sigs.get(k) ?? own(k, undefined)).value = patch[k]; });
      tell(changed);
    },
    /** `fn(state, changedKeys)` after a change to any of `keys` (all of them when `keys` is null); returns the way to stop. */
    subscribe(keys, fn) {
      const s = { keys, fn };
      subs.add(s);
      return () => subs.delete(s);
    },
  };
}

export const server = createStore({
  tiles: [], roots: [], folders: [], home: null, inventory: null, elements: [], language: null, system: 'db', printer: 'default', printers: [], overhangDeg: 55, steepestDeg: 65,
  live: false,    // the event stream is open and in step
  ever: false,    // it has been, once: so a loss reads 'reconnecting…', not 'connecting…'
  ready: false,   // the first list of tiles is in
  epoch: null,    // which run of the program the numbers (mesh versions, sketches) are from
  progress: {},   // document id -> the work in flight for it (a tile's build, a map's pieces loading): one record kind, `Progress`
});

/** What happened, one stream (js/messages.js): live.js writes it from the event stream; every surface draws from it. */
export const log = createLog();

export const view = createStore({
  tileId: null,
  placeAt: { root: null, dir: '', tile: null }, // the folder level chosen on the browser page (js/folders.js: the place's number, the folder below it; root null is Places) and the tile that was open when it was chosen; `place` is derived from it below
  query: '',           // the browser's filter: the words a tile's name must have
  sel: null,           // {kind, id, piece?}: kind entry (id: the entry's key), region or line (id: the name), tile; or null
  control: null,       // {name, group}: the control open as a level on the selected thing (js/nav.js)
  face: 'tile',        // what the panel shows: 'tile' (the levels) or 'source'
  hov: null,           // the same, under the pointer (view or text)
  cloudAt: { tile: null, text: null, sheet: null, hov: null, sheetHov: null }, // which entry's cloud (a top= or pin) is lit, for the tile `tile`: `text` the one the source's caret is in, `sheet` the one whose top field has focus in the sheet, `hov` and `sheetHov` the ones under the pointer in the source and in the sheet (the keys of entries); read with `cloudOf`
  debug: false,        // the debug layer is on (the program collects its marks only then)
  mode: 'painted',     // the view's preset
  pick: null,          // {name, animate}: a preset asked for (a fresh object each time, so asking again counts); `mode` is the preset in force
  // activeView (the preset the camera is still on, null once the user's own view holds) is derived, in editor/framing.js, from what the viewer says
  pieces: new Map(),   // entry key -> how many pieces its geometry has (from the mesh shown)
  shown: null,         // {id, epoch, version}: the build's mesh the view holds
  draft: null,         // {tileId, payload}: a draft shown in its place
  source: null,        // the id of the source document the text pane should show (`tile#file`, see `sourceId`); null: the tile's own
  addressRead: false, // the start has taken where to go from the address (open.js pickFirst): nothing writes the address before, so a reload keeps its place
  pageAt: 'browse',    // the sidebar's page the user chose: the tile browser, or the editor ('edit'); `page` is derived from it below
  // dmode (a .dmap's mode: 'map' or 'plates'; null: not a .dmap, or its plan has not come) is derived, in editor/mode.js
  msel: [],            // the pieces selected on the map (their ids: editor/scene.js `selected()` reads the ones the map has now)
  mprop: null,         // the property open as a level on the selected piece: 'bottom' | 'seed' | 'print' (editor/scene-panel.js)
  mlist: {},           // how each map's list of pieces is sorted, filtered and searched, by the map's tile id: {sort, filter, query} (editor/scene-panel.js; sort and filter are kept in localStorage)
  madd: false,         // the map's Add step is open (editor/model.js `level`: it is the level only while no piece is selected)
  addAt: null,         // the folder the Add step is at, {root, dir}: the one last browsed, kept for the session (null: the folder the map is in, else the library)
  addSlot: 0,          // which of the Add step's two sections (nav.js `ADD_PANES`) its folder is in: each step to another folder takes the other
  addQuery: '',        // the words filtering the folder the Add tiles level browses
  panel: false,        // the messages panel is open
  popover: null,       // the id of the open popover, if any
});

export const doc = createStore({
  tileId: null, file: null, base: null, text: null, writable: false, docRev: null, // tileId, file, base and docRev are derived in editor/document.js from the source the pane shows (what the documents module holds of it; a save is made against that revision): these are only their shape; text and writable are the pane's
  checked: null, checkedFile: null, checkedTile: null, // the text last checked, and the file and tile it was checked for
  ok: true, errors: [], stack: null, map: null,
  names: null,         // the definitions, regions, lines and points the text makes now, by the program's parser (kept while the text does not parse)
  note: null, conflict: null, saving: false,
  selectAt: null,      // the byte of a node a gesture just wrote: select it once it is checked
});

// ---- selectors ---------------------------------------------------------------

const byId = new WeakMap();
export function tileById(id) {
  const tiles = server.get().tiles;
  let m = byId.get(tiles);
  if (!m) { m = new Map(tiles.map((t) => [t.id, t])); byId.set(tiles, m); }
  return (id != null && m.get(id)) || null;
}
export const rootOf = (r) => server.get().roots[r] || { kind: 'files', label: 'Files', path: null };
/** May the tile's file be renamed here? */
export const renameable = (t) => !!t && !t.path.startsWith('lib:') && /\.dtile$/i.test(t.path) && !!rootOf(t.root).writable;
/** The id of the document that holds `file` of tile `t`: the tile's own file is the tile's id, a part of it `id#file`. */
export const sourceId = (t, file) => (!file || file === t.path ? t.id : `${t.id}#${file}`);
/** The file a source id of tile `t` names (null: the tile's own). */
export const sourceFile = (t, sid) => (!sid || sid === t.id || !sid.startsWith(`${t.id}#`) ? null : sid.slice(t.id.length + 1));
/** The tile shown. */
export const tile = () => tileById(view.get().tileId);
export const progressOf = (id) => server.get().progress[id] || null;
export const elementNamed = (n) => server.get().elements.find((e) => e.name === n);
/** A node of the tile's own language (`tile`, `line`, `region`, `within`, `except`) with its controls, as the program describes it. */
export const nodeNamed = (n) => server.get().language?.nodes.find((x) => x.name === n);
/** One control of a node, by name: the program's row for it. */
export const nodeControl = (node, name) => nodeNamed(node)?.controls.find((c) => c.name === name);
/** Is there text with changes not yet saved? */
export const dirty = () => { const d = doc.get(); return d.text != null && d.base != null && d.text !== d.base; };

/** A word of a stack entry's own controls (`at=bank` -> bank), or undefined. */
export const controlText = (it, name) => [...(it.place || []), ...(it.controls || [])].find((c) => c.name === name)?.text.replace(new RegExp(`^${name}=`), '').replace(/"/g, '');
/** The region an entry is put in (`at=bank` -> bank): its `at=` when that names one of the map's regions. */
export const regionOf = (it) => {
  const at = controlText(it, 'at');
  return at && doc.get().map?.regions.some((r) => r.name === at) ? at : undefined;
};
/** The file stem (`2x1-cliff`, `1x1-water+grass-tree`): the menu's name for a tile, whatever the document calls it. */
export const stem = (t) => t.path.replace(/^lib:/, '').split(/[\\/]/).pop().replace(/\.dtile$/i, '');
/** Is the document a print list (its panel and view are the plates', editor/plates.js)? */
export const isPlates = (t) => !!t && /\.dmap$/i.test(t.path);
/** Is a .dmap's map the view (the title bar's Map), not its plates? */
export const isMapView = () => isPlates(tile()) && view.get().dmode === 'map';
/** Is a .dmap's plates the view? */
export const isPlatesView = () => isPlates(tile()) && view.get().dmode !== 'map';
/** Is the document the inventory (its view is editor/inventory.js)? */
export const isInventory = (t) => !!t && /\.dinv$/i.test(t.path);

/** The stack of the text now: the last clean check of it while editing, else the last build's. */
// The list is made once for each stack it comes from (the check's, or the build's plan) and handed out again: callers read it, never change it.
const NONE = [];
let stackMemo = { from: null, items: NONE };
export function stackItems() {
  const d = doc.get(), t = tile();
  const from = (d.stack && d.tileId === view.get().tileId ? d.stack : t?.plan?.stack) || NONE;
  if (stackMemo.from !== from) stackMemo = { from, items: from.map((s) => (typeof s === 'string' ? { label: s } : s)) };
  return stackMemo.items;
}
/** Where the entry `key` is in the stack of the text now (the checked one, else the build's plan): -1 when it is not there. */
export const stackIndex = (key) => (key == null ? -1 : stackItems().findIndex((it) => it.key === key));
/** The stack item of an entry selection, in the stack of the text now; undefined for another selection or an entry the text no longer has. */
export const itemOf = (w) => (w?.kind === 'entry' ? stackItems()[stackIndex(w.id)] : undefined);
/** The map's region or line a selection names. */
export function mapNode(w) {
  const { map, tileId } = doc.get();
  if (!w || !map || tileId !== view.get().tileId) return null;
  const list = w.kind === 'region' ? map.regions : w.kind === 'line' ? map.lines : w.kind === 'point' ? map.points : null;
  return list?.find((n) => n.name === w.id) || null;
}

// ---- the selection's one shape -----------------------------------------------

export const entrySel = (id, piece = null) => ({ kind: 'entry', id, piece });
export const markSel = (kind, id) => ({ kind, id });
export const tileSel = () => ({ kind: 'tile', id: view.get().tileId });
export const isEntry = (w) => w?.kind === 'entry';
export const isMark = (w) => w?.kind === 'region' || w?.kind === 'line';
/** A place the map draws: a mark, or a point (which is hovered, not selected). */
export const isPlace = (w) => isMark(w) || w?.kind === 'point';
const keyOf = (w) => (!w ? '' : w.kind === 'tile' ? 'tile' : `${w.kind}:${w.id}:${w.piece ?? ''}`);
export const same = (a, b) => keyOf(a) === keyOf(b);

/**
 * Resolves true once tile `id` is on the list (false after a few seconds):
 * a tile the program has just made joins it with its next event.
 */
export function whenListed(id) {
  if (tileById(id)) return Promise.resolve(true);
  return whenListedWhere((t) => t.id === id).then((t) => !!t);
}

/** Resolves with the first listed tile that `pred` accepts (null after a few seconds): a tile a call has just made or moved joins the list with the program's next word. */
export function whenListedWhere(pred) {
  const now = server.get().tiles.find(pred);
  if (now) return Promise.resolve(now);
  return new Promise((resolve) => {
    const stop = (t) => { off(); clearTimeout(timer); resolve(t); };
    const off = server.subscribe(['tiles'], () => { const t = server.get().tiles.find(pred); if (t) stop(t); });
    const timer = setTimeout(() => stop(null), 5000);
  });
}

// ---- the folder level the browser shows ----------------------------------------------------------------------

const levels = new Map();
/** One object for each level, so a level that did not change is the same object. */
const levelNamed = (root, dir) => { const k = `${root ?? ''}|${dir}`; let l = levels.get(k); if (!l) { l = { root, dir }; levels.set(k, l); } return l; };

/** The level the browser stands for a choice made while the tile `tile` was open: `{root, dir, tile}`. */
export const placeAt = (l) => ({ root: l.root ?? null, dir: l.dir || '', tile: view.get().tileId });

// The level is the one chosen on the browser page, for as long as the tile that was open then is the open one; a tile
// that opens puts it at the folder the tile is in.  A level that is gone (a place taken off, a folder trashed) is its
// parent.  It is worked out, not stored, so nothing has to be told that a tile opened or a folder went.
view.derive('place', () => {
  const { placeAt: p, tileId } = view.get();
  const roots = server.get().roots;
  const inFolder = p.tile === tileId ? null : whereIs(roots, tile());
  const { tiles, folders } = server.get();
  const l = settledLevel({ roots, tiles, folders }, { root: inFolder ? inFolder.root : p.root, dir: inFolder ? inFolder.dir : p.dir });
  return levelNamed(l.root, l.dir);
});

// The sidebar's page: the one chosen, except that with no tile shown the tile browser is the page.  Worked out, so nothing has
// to put it back when the last tile goes.
view.derive('page', () => (view.get().tileId == null ? 'browse' : view.get().pageAt));

const NO_CLOUD = { text: null, sheet: null, hov: null, sheetHov: null };
/** The clouds lit now: what was set for the tile shown, and nothing for any other (so a new tile starts with none, and nothing has to clear them). */
export const cloudOf = () => { const c = view.get().cloudAt; return c.tile === view.get().tileId ? c : NO_CLOUD; };
/** Light or put out clouds (`{text, sheet, hov, sheetHov}`: the parts to change) for the tile shown. */
export const setCloud = (patch) => view.set({ cloudAt: { ...cloudOf(), ...patch, tile: view.get().tileId } });
