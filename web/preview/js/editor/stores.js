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

export function createStore(initial) {
  const state = { ...initial };
  const subs = new Set();
  return {
    get: () => state,
    set(patch) {
      const changed = Object.keys(patch).filter((k) => !Object.is(state[k], patch[k]));
      if (!changed.length) return;
      for (const k of changed) state[k] = patch[k];
      for (const s of [...subs]) if (!s.keys || changed.some((k) => s.keys.includes(k))) s.fn(state, changed);
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
  tiles: [], roots: [], elements: [], system: 'db', printer: 'default', printers: [], overhangDeg: 55, steepestDeg: 65,
  live: false,    // the event stream is open and in step
  ever: false,    // it has been, once: so a loss reads 'reconnecting…', not 'connecting…'
  ready: false,   // the first list of tiles is in
  epoch: null,    // which run of the program the numbers (mesh versions, sketches) are from
  progress: {},   // tile id -> the build's progress while it builds
});

export const view = createStore({
  tileId: null, root: -1,
  sel: null,           // {kind, id, piece?}: kind entry (id: the stack index), region or line (id: the name), tile; or null
  hov: null,           // the same, under the pointer (view or text)
  cloudText: null,     // the stack entry whose cloud (a top= or pin) the source's caret is in
  cloudSheet: null,    // the stack entry whose top field has focus in the sheet
  cloudHov: null,      // the entry whose cloud is under the pointer in the source
  cloudSheetHov: null, // the entry whose top row is under the pointer in the sheet
  mode: 'painted',     // the view's preset
  pick: null,          // {name, animate}: a preset asked for (a fresh object each time, so asking again counts)
  activeView: null,    // the preset the camera is still on, if any
  pieces: new Map(),   // entry -> how many pieces its geometry has
  shown: null,         // {id, epoch, version}: the build's mesh the view holds
  draft: null,         // {tileId, payload}: a draft shown in its place
  textFile: null,      // the file the text pane should show (null: the tile's own)
  mineOpen: false,     // the make-it-mine question is up
  page: 'browse',      // the sidebar's page: the tile browser, or the editor ('edit')
  popover: null,       // the id of the open popover, if any
});

export const doc = createStore({
  tileId: null, file: null, base: null, text: null, writable: false,
  checked: null, ok: true, errors: [], stack: null, map: null,
  format: 0,           // the language format of the text checked last (0, or 1 where the program reads it)
  migrated: null,      // the format this file was written in, when the program reads it migrated in memory (null: as written)
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
/** The tiles listed under root `r`. */
export const inRoot = (r) => server.get().tiles.filter((t) => t.root === r);
export const rootOf = (r) => server.get().roots[r] || { kind: 'files', label: 'Files', path: null };
/** May the tile's file be renamed here? */
export const renameable = (t) => !!t && !t.path.startsWith('lib:') && /\.dtile$/i.test(t.path) && rootOf(t.root).kind === 'dir';
/** The tile shown. */
export const tile = () => tileById(view.get().tileId);
export const progressOf = (id) => server.get().progress[id] || null;
export const elementNamed = (n) => server.get().elements.find((e) => e.name === n);
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
export const stem = (t) => t.path.replace(/^lib:/, '').split(/[\\/]/).pop().replace(/\.(dtile|stl)$/i, '') + (/\.stl$/i.test(t.path) ? '.stl' : '');

/** The stack of the text now: the last clean check of it while editing, else the last build's. */
export function stackItems() {
  const d = doc.get(), t = tile();
  return ((d.stack && d.tileId === view.get().tileId ? d.stack : t?.plan?.stack) || []).map((s) => (typeof s === 'string' ? { label: s } : s));
}
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
  return new Promise((resolve) => {
    const stop = (ok) => { off(); clearTimeout(timer); resolve(ok); };
    const off = server.subscribe(['tiles'], () => { if (tileById(id)) stop(true); });
    const timer = setTimeout(() => stop(false), 5000);
  });
}
