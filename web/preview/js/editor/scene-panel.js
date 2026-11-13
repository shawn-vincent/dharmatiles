// The map's panel on this page (docs/maps.html): `js/view/map-panel.js` drawn into the panel's box from the model
// (`editor/model.js`), with the buttons wired to the changes of the map (scene-edit.js) and of the view.  Nothing
// is drawn here and nothing is kept: the panel is drawn again by itself when what it read changes.

import { html, render } from '../view/html.js';
import { MapPanel } from '../view/map-panel.js';
import { $ } from './app.js';
import { keep, readKept } from './list-keep.js';
import { mapPanel } from './model.js';
import * as E from './scene-edit.js';
import { chooseMode, hoverPieces, mapMode, select } from './scene.js';
import { view } from './stores.js';

/** What the panel's buttons do. */
const act = {
  select: (ids) => select(ids),
  hover: (ids) => hoverPieces(ids),
  /** Change how this map's list is sorted, filtered or searched: view state, kept for the map (sort and filter) across visits. */
  list: (patch) => {
    const id = view.get().tileId;
    if (id == null) return;
    const next = { ...mapPanel.list.value, ...patch };
    view.set({ mlist: { ...view.get().mlist, [id]: next } });
    keep(id, next);
  },
  setProp: (kind) => view.set({ mprop: kind }),
  toggleDrawer: () => view.set({ drawer: !view.get().drawer }),
  chooseMode,
  place: E.place,
  variants: E.variants,
  turn: E.turn,
  setPiece: E.setPiece,
  duplicate: E.duplicate,
  remove: E.remove,
  openTile: E.openTile,
};

/** Esc and Back: one level up. */
export function up() {
  if (view.get().mprop) { view.set({ mprop: null }); return true; }
  if (view.get().msel.length) { select([]); return true; }
  return false;
}

/** Wire this part of the page (once, at load). */
export function init() {
  view.set({ mlist: readKept() });
  render(html`<${MapPanel} m=${mapPanel} act=${act} />`, $('lens-map'));
  // Esc is one level up: a property to its piece, a piece to the map (before the page's own Esc, which leaves the tile)
  addEventListener('keydown', (e) => {
    const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';
    if (e.key === 'Escape' && !typing && mapMode() && up()) { e.preventDefault(); e.stopImmediatePropagation(); }
  }, true);
}
