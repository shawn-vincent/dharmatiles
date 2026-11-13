// The address in the URL: one derivation of the path the crumbs show, written by one effect.
//
// Where you are is a path: Places, a place, its folders, then (when a document is open) the document and the levels
// below it (a tile's entry and control, or a map's pieces and the property of them, or the folders of its Add step).  The crumbs over the panel
// (levels.js `renderCrumbs`) are that path drawn; the hash of the URL is that path written
// (`#place=…&dir=…` for a folder level, `#tile=…&at=…&control=…` or `#tile=…&pieces=p1,p2&prop=seed` for a document's),
// so a reload, a pasted link and the browser's Back and Forward land on the same level.  Nothing else writes the
// address; the readers (places.js and levels.js `fromAddress`, open.js `pickFirst`) take it into the page's state, and
// this writes it back from that state, which is then the same hash and so no write.
//
// A new level is a step of the browser's history, anything else replaces (nav.js `stepsHistory`).  Nothing is written
// while the page has not yet taken where to go from the address (`view.addressRead`, open.js), while an address it was
// given is being taken (`held`: levels.js), or while what the path needs is not there yet (a map's scene).

import { effect, signal, untracked } from '@preact/signals-core';
import { ADD_PANES, OWNED, buildHash, levelParams, parseHash, stepsHistory } from '../nav.js';
import { placeParams } from '../folders.js';
import { addLevel, level as mapLevel, selected as mapSelected, mprop, scene as mapScene } from './model.js';
import { isMapView, isPlates, server, tile, view } from './stores.js';

/** True while a level the address asked for is being taken: the page is part way to it, and writes nothing meanwhile. */
export const held = signal(false);

/** The pairs of the address for where the page is now, or null while the page cannot say yet. */
export function pairsNow() {
  const v = view.get();
  const { ready, roots } = server.get();
  if (!ready || !v.addressRead || held.value) return null;
  const t = tile();
  if (v.page === 'browse' || !t) return placeParams(roots, v.place);
  // a map that has not said what it is, or whose pieces are not in yet, has no levels to write
  if (isPlates(t) && v.dmode == null) return null;
  const map = isMapView();
  if (map && !mapScene.value) return null;
  const pieces = map ? mapSelected.value.map((p) => p.id) : null;
  return { tile: t.path, ...levelParams({ sel: v.sel, control: v.control, face: v.face, pieces, prop: pieces?.length === 1 ? mprop.value : null, add: map && ADD_PANES.includes(mapLevel.value) ? placeParams(roots, addLevel.value) : null }) };
}

/** Write `pairs` over the address, keeping what it holds that is not the address's (the theme, the paint). */
function write(pairs) {
  const cur = parseHash(location.hash);
  const next = { ...cur };
  for (const k of OWNED) delete next[k];
  Object.assign(next, pairs);
  const h = buildHash(next);
  if (h === location.hash) return;
  if (stepsHistory(cur, pairs)) history.pushState(null, '', h); else history.replaceState(null, '', h);
}

/** Wire the address (once, at load). */
export function init() {
  effect(() => {
    const pairs = pairsNow();
    if (pairs) untracked(() => write(pairs));
  });
}
