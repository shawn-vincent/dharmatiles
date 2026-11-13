// The mode a .dmap is shown in (the title bar's Tile / Map / Plates): the session's choice for that file if there is
// one, else what the document is (a map when its scene says so).  It is worked out, never stored: `view.dmode` is
// derived from the open tile, the document's state and the choice, and whoever follows that key is told when its
// value changes.

import { signal } from '@preact/signals-core';
import { docs } from './docs.js';
import { isPlates, tile, view } from './stores.js';

const chosen = signal(new Map());     // document path -> 'map' | 'plates' (the session's choice)

/** The mode of tile `t` whose document state is `scene`: null where it is not a .dmap or its scene has not come. */
export function modeFor(t, scene, choice = chosen.value) {
  if (!isPlates(t)) return null;
  return choice.get(t.path) ?? (scene ? (scene.map ? 'map' : 'plates') : null);
}

view.derive('dmode', () => {
  const t = tile();
  return modeFor(t, t ? docs.scene(t.id) : null);
});

const remember = (path, mode) => { chosen.value = new Map(chosen.value).set(path, mode); };

/** The title bar's Map or Plates: the same document, the other view. */
export function chooseMode(mode) {
  const t = tile();
  if (!isPlates(t)) return;
  remember(t.path, mode);
}

/** Open `path` (a map) in `mode` the next time it is shown. */
export const setChosen = remember;
