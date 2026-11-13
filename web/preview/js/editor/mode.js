// The mode a .dmap is shown in (the title bar's Tile / Map / Plates): the session's choice for that file if there is
// one, else what the document is (a map when its scene says so).  Worked out in one place, from the open tile and
// the document's state, and written to `view.dmode` for the parts that follow that key.  The document module calls
// `derivedMode` when a new state is taken, before anyone is told.

import { docs } from './docs.js';
import { isPlates, server, tile, view } from './stores.js';

const chosen = new Map();     // document path -> 'map' | 'plates' (the session's choice)

/** The mode of tile `t` whose document state is `scene`: null where it is not a .dmap or its scene has not come. */
export function modeFor(t, scene) {
  if (!isPlates(t)) return null;
  return chosen.get(t.path) ?? (scene ? (scene.map ? 'map' : 'plates') : null);
}

/** Write the mode of the tile shown to `view.dmode` (a print list whose plan has not come keeps what it had). */
export function applyMode() {
  const t = tile();
  const want = modeFor(t, t ? docs.scene(t.id) : null);
  if (view.get().dmode !== want && (want || !isPlates(t))) view.set({ dmode: want });
}

/** A new document state was taken: if it is the tile shown, its mode follows. */
export function derivedMode(id) {
  if (id === view.get().tileId) applyMode();
}

/** The title bar's Map or Plates: the same document, the other view. */
export function chooseMode(mode) {
  const t = tile();
  if (!isPlates(t)) return;
  chosen.set(t.path, mode);
  applyMode();
}

/** Open `path` (a map) in `mode` the next time it is shown. */
export const setChosen = (path, mode) => chosen.set(path, mode);

/** Wire this part of the page (once, at load): the tile shown, or the list of tiles, changed. */
export function init() {
  view.subscribe(['tileId'], applyMode);
  server.subscribe(['tiles'], applyMode);
  applyMode();
}
