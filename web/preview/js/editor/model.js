// The page's model: what the panels draw, as signals derived from the three things that hold the page's facts and
// from nothing else.
//
//   documents   js/documents.js (through editor/docs.js): each map, print list and source, by its revision
//   resources   the places, target, printers, inventory and the list of tiles the program tells (editor/stores.js `server`)
//   the view    what you are looking at: the tile, the selection, the property open (editor/stores.js `view`)
//
// A value here is a `computed`: worked out from those when it is read, kept only to save repeating the same work,
// and followed by whoever reads it inside a component.  Nothing here is set; to change what a panel shows, change
// the document (through its actions), the resource (the program says it) or the view.  The panels (js/view/) read
// this and call the actions; they keep no copy of a fact.

import { computed, signal } from '@preact/signals-core';
import * as M from '../map-model.js';
import { listFrom } from '../map-list.js';
import { piecesOf, selectedOf } from '../doc-views.js';
import { pill as invPill, systemName } from '../inventory.js';
import { levelOf, placesOf, settledLevel, whereIs, writable } from '../folders.js';
import { mapLevelOf } from '../nav.js';
import { token } from '../tokens.js';
import { defaultFilters } from '../inventory.js';
import { docs, planOf } from './docs.js';
import './mode.js'; // `view.dmode` is derived there
import { isInventory, isPlates, server, stem, tile, view } from './stores.js';

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The tile shown. */
export const tileNow = computed(() => tile());
/** Is the map of the document the view (the title bar's Map)? */
export const mapOn = computed(() => isPlates(tileNow.value) && view.get().dmode === 'map');
/** The map's scene as the program last said it (null when the map is not the view). */
export const scene = computed(() => (mapOn.value ? docs.scene(tileNow.value.id) : null));
/** The pieces of the map now: its scene's, with the build each piece's tile is at. */
export const pieces = computed(() => piecesOf(scene.value, server.get().tiles));
/** The selected pieces: the selection's ids that the map has now. */
export const selected = computed(() => selectedOf(view.get().msel, pieces.value));
/** What the program says of each piece with a word (have, need, other, skip). */
export const marks = computed(() => M.marks(pieces.value));
/** How this map's list of pieces is sorted, filtered and searched (view state, per map: `view.mlist`, id -> choice). */
export const mapList = computed(() => listFrom(view.get().mlist[tileNow.value?.id ?? '']));
/** The property open on the selected piece, if one is. */
export const mprop = computed(() => view.get().mprop);
/** The level the map is at: 'map', 'add', 'piece' or 'prop' (nav.js `mapLevelOf`). */
export const level = computed(() => mapLevelOf({ pieces: selected.value.length, prop: mprop.value, add: view.get().madd }));
export const inventory = computed(() => server.get().inventory);
/** May the map not be changed here (a library file, or one the program says is read only)? */
export const readOnly = computed(() => {
  const t = tileNow.value;
  return !t || !(planOf(t)?.writable ?? true) || t.path.startsWith('lib:');
});
/** Why there is no scene to draw, as the panel says it. */
export const waiting = computed(() => (tileNow.value?.status === 'failed' ? 'This map does not read: mend it in Source.' : 'Reading the map…'));

/** The strip under the crumbs. */
export const contextLines = () => {
  const sc = scene.value;
  const ps = pieces.value;
  const placed = ps.filter((p) => p.at).length;
  const tray = ps.length - placed;
  return [
    `${plural(placed, 'piece')} on the map${tray ? ` · ${tray} in the tray` : ''}`,
    sc ? `${systemName(sc.system)}${sc.grid ? ` · ${sc.grid.cols} × ${sc.grid.rows} squares` : ''}` : 'Reading the map…',
  ];
};

/** What the map's panel (js/view/map-panel.js) reads: one object of signals. */
// (the map's panel model, `mapPanel`, is after the Add tiles level's: it reads that)

// ---- the plates' panel ---------------------------------------------------------------------------------------------

/** The plates of the list shown, as the program last planned them (null while it has not). */
export const plan = computed(() => (isPlates(tileNow.value) ? planOf(tileNow.value)?.plates ?? null : null));
/** Is the plates' panel the one on show (a print list, not its map)? */
export const platesOn = computed(() => isPlates(tileNow.value) && view.get().dmode !== 'map');

const freshUi = (id) => ({ id, exporting: { busy: false, files: null, error: null, failures: [] }, after: { open: false, plate: 1, bad: [], busy: false } });
const uiSignal = signal(freshUi(null));
/**
 * What the user is in the middle of in the plates' panel, for the list shown: the sheet after a print (which plate,
 * which pieces failed) and the export under way.  It belongs to the list: another list reads its own, fresh.
 */
export const platesUi = {
  value: computed(() => { const id = tileNow.value?.id ?? null; return uiSignal.value.id === id ? uiSignal.value : freshUi(id); }),
  /** Change the state of list `id`: `edit(state)` answers the parts that change. */
  update(id, edit) {
    const cur = uiSignal.peek().id === id ? uiSignal.peek() : freshUi(id);
    uiSignal.value = { ...cur, ...edit(cur), id };
  },
};

/** What the plates' panel (js/view/plates-panel.js) reads. */
export const platesPanel = {
  on: platesOn,
  tile: tileNow,
  plan,
  ui: platesUi.value,
  inventory,
  printers: computed(() => server.get().printers),
  // the printer the document names now (its own word, from its scene), the plan's only until that is there
  currentPrinter: computed(() => (tileNow.value && docs.scene(tileNow.value.id)?.printer) ?? plan.value?.printer),
  readOnly: computed(() => !(plan.value?.writable ?? planOf(tileNow.value)?.writable)),
  name: computed(() => (tileNow.value ? stem(tileNow.value).replace(/\.dmap$/i, '') : '')),
};

// ---- the folder levels ---------------------------------------------------------------------------------------------

/** What the browser's levels draw (js/view/places-panel.js): the level `view.place` is, what the program lists there, and the filter's words. */
const isDtile = (t) => /\.dtile$/i.test(t.path);

/**
 * What a folder level draws (js/view/places-panel.js), for the browser's page and for the Add tiles level's pane alike: the one
 * walk of the places and folders.  `keep` says which tiles are listed; `extra` is what only one of them has.
 */
function folderData(level, query, system, keep, extra) {
  const { roots, tiles, folders, home, inventory } = server.get();
  const r = roots[level.root];
  return {
    roots, tiles, folders, level, query, places: placesOf(roots), home,
    // have and need pills: what the inventory says of a tile for the target system (the chip in the title bar)
    pill: (t) => invPill(inventory, t.path, system), system, inventory,
    view: level.root == null ? null : levelOf({ roots, tiles: tiles.filter(keep), folders, root: level.root, dir: level.dir, query }),
    writable: !!r && writable(r) && !query.trim(),
    ...extra,
  };
}

export const browse = {
  ready: computed(() => server.get().ready),
  data: computed(() => folderData(view.get().place, view.get().query, server.get().system, () => true, { openTile: view.get().tileId })),
};

// ---- the Add tiles level ------------------------------------------------------------------------------------------

/** The place and folder the Add tiles level opens at the first time: where the map's first tile lives, else the library. */
function addStart() {
  const { roots, tiles } = server.get();
  const lib = roots.findIndex((r) => r.role === 'library');
  const first = tiles.find((t) => t.id === scene.value?.pieces?.[0]?.tileId);
  const at = first && whereIs(roots, first);
  return at || { root: lib >= 0 ? lib : 0, dir: '' };
}
/** The folder the Add tiles level browses: the one chosen (kept for the session), else where it starts, settled among what the program lists now. */
export const addLevel = computed(() => {
  const { roots, tiles, folders } = server.get();
  return settledLevel({ roots, tiles, folders }, view.get().addAt ?? addStart());
});

/** What the Add tiles level's pane draws: the browser's own folder level in its add mode (tiles as pictures), or null when no map is the view. */
export const addData = computed(() => {
  if (!mapOn.value) return null;
  const system = scene.value?.system ?? 'db';
  return folderData(addLevel.value, view.get().addQuery, system, isDtile, {
    add: true, writable: false, openTile: null, plate: token('--plate') || '#D9E0EB',
    places: placesOf(server.get().roots).filter((r) => r.kind !== 'files'),
  });
});
// ---- a tile put on the map -----------------------------------------------------------------------------------------

const FRESH_MS = 1800;
const addedNow = signal(/** @type {{doc: string | null, tileId: string, name: string, n: number} | null} */ (null));
const freshNow = signal(false);
let freshTimer = 0;
/** The tile `t` was just put on the map: its row lights up for a moment (the Add tiles level's list, and its one line when folded). */
export function flashAdded(t) {
  addedNow.value = { doc: tileNow.value?.id ?? null, tileId: t.id, name: stem(t).replace(/\\.dtile$/i, ''), n: (addedNow.peek()?.n ?? 0) + 1 };
  freshNow.value = true;
  clearTimeout(freshTimer);
  freshTimer = setTimeout(() => { freshNow.value = false; }, FRESH_MS);
}

/** What the map's levels (js/view/map-panel.js) read: one object of signals. */
export const mapPanel = {
  on: mapOn, tile: tileNow, scene, pieces, selected, marks, list: mapList, mprop, level, inventory, readOnly, waiting,
  // the tile last put on the map (a row of the list lights up for a moment), and the Add tiles level's folder browser
  added: computed(() => (addedNow.value?.doc === tileNow.value?.id ? addedNow.value : null)), fresh: freshNow, add: addData,
};


// ---- the inventory --------------------------------------------------------------------------------------------------

const freshInv = (id) => ({ id, filters: null, face: 'view', reprint: null });
const invSignal = signal(freshInv(null));
/** What the user is choosing in the inventory's view for the document shown: the filters, the face, the piece a reprint form is open under. */
export const inventoryUi = {
  value: computed(() => { const id = tileNow.value?.id ?? null; return invSignal.value.id === id ? invSignal.value : freshInv(id); }),
  update(id, edit) {
    const cur = invSignal.peek().id === id ? invSignal.peek() : freshInv(id);
    invSignal.value = { ...cur, ...edit(cur), id };
  },
};

/** The colour mode last used for "I have this": remembered in the browser, else the one the log last wrote, else single. */
const storedColour = () => {
  try {
    const c = localStorage.getItem('dt-inventory-colour');
    if (c === 'single' || c === 'multi') return c;
  } catch { /* private mode, or nothing kept: the log's own last colour answers */ }
  return null;
};
/** The colour mode picked at the tile level, until a print is added. */
export const pickedColour = signal(/** @type {string | null} */ (null));
/** Bumped when the remembered colour changes, so what reads it follows. */
export const colourKept = signal(0);

/** The piece's log, as the program adds it up: from the snapshot when this is the home inventory (it follows at once), else the document's. */
const dataOf = (t) => {
  const snap = server.get().inventory;
  const own = t.plan?.inventory;
  if (snap?.exists && snap.path === t.path && !snap.problems) return { ...snap, text: own?.text ?? '', home: snap.home };
  return { pieces: own?.pieces ?? [], text: own?.text ?? '', problems: own?.problems, home: snap?.home ?? '' };
};

/** What the tile level's inventory block reads. */
export const haveBlockModel = {
  inventory,
  system: computed(() => server.get().system),
  colour: computed(() => { void colourKept.value; return pickedColour.value ?? storedColour() ?? server.get().inventory?.lastColour ?? 'single'; }),
};

/** What the inventory's own view reads. */
export const inventoryPanel = {
  on: computed(() => isInventory(tileNow.value)),
  tile: tileNow,
  ui: inventoryUi.value,
  data: computed(() => dataOf(tileNow.value ?? { plan: null, path: '' })),
  // the filters now: the target's system until you pick one, the colour and the shelf as you set them
  filters: computed(() => { const f = inventoryUi.value.value.filters ?? { ...defaultFilters(null), system: null }; return { ...f, system: f.system ?? server.get().system }; }),
  inventory,
  system: haveBlockModel.system,
  tiles: computed(() => server.get().tiles),
  lists: computed(() => server.get().tiles.filter((x) => /\.dmap$/i.test(x.path))),
};
