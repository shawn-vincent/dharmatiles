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
import { piecesOf, selectedOf } from '../doc-views.js';
import { systemName } from '../inventory.js';
import { docs, planOf } from './docs.js';
import './mode.js'; // `view.dmode` is derived there
import { isPlates, server, stem, tile, view } from './stores.js';

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
/** The property open on the selected piece, if one is. */
export const mprop = computed(() => view.get().mprop);
/** The level the map's panel is on: 'map', 'piece' or 'prop'. */
export const level = computed(() => (mprop.value && selected.value.length === 1 ? 'prop' : selected.value.length ? 'piece' : 'map'));
export const inventory = computed(() => server.get().inventory);
/** May the map not be changed here (a library file, or one the program says is read only)? */
export const readOnly = computed(() => {
  const t = tileNow.value;
  return !t || !(planOf(t)?.writable ?? true) || t.path.startsWith('lib:');
});
/** The map's name: its file's, without the suffix. */
export const mapName = computed(() => (tileNow.value ? stem(tileNow.value).replace(/\.dmap$/i, '') : ''));
/** Why there is no scene to draw, as the panel says it. */
export const waiting = computed(() => (tileNow.value?.status === 'failed' ? 'This map does not read: mend it in Source.' : 'Reading the map…'));

/** The strip under the crumbs. */
export const contextLines = () => {
  const sc = scene.value;
  const ps = pieces.value;
  const placed = ps.filter((p) => p.at).length;
  const tray = ps.length - placed;
  return [
    `${plural(placed, 'piece')} on the map${tray ? ` · ${tray} in the tray` : ''} · ${M.totalsText(sc)}`,
    sc ? `${systemName(sc.system)} · ${sc.printer}${sc.grid ? ` · ${sc.grid.cols} × ${sc.grid.rows} squares` : ''}` : 'Reading the map…',
  ];
};

/** What the map's panel (js/view/map-panel.js) reads: one object of signals. */
export const mapPanel = { on: mapOn, tile: tileNow, scene, pieces, selected, marks, mprop, level, inventory, readOnly, mapName, waiting };

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
