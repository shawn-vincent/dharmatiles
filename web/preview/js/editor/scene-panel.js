// The map's levels on this page (docs/maps.html): `js/view/map-panel.js` drawn into the levels' sections from the model
// (`editor/model.js`), with the buttons wired to the changes of the map (scene-edit.js) and of the view.  Nothing
// is drawn here and nothing is kept: the panel is drawn again by itself when what it read changes.

import { html, render } from '../view/html.js';
import { AddLevel, MapLevel, PieceLevel, PropLevel } from '../view/map-panel.js';
import { parentLevel } from '../folders.js';
import { showError } from '../problems.js';
import { $ } from './app.js';
import { keep, readKept } from './list-keep.js';
import { addLevel, mapPanel } from './model.js';
import * as E from './scene-edit.js';
import { chooseMode, hoverPieces, select } from './scene.js';
import { TILE_MIME } from './scene-drag.js';
import { pictureOf } from './scene-thumbs.js';
import { tileById, view } from './stores.js';

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
  // the Add tiles level (docs/maps.html): a level of the map's trail; its pane walks the places and folders as the browser's page does
  openAdd: () => view.set({ msel: [], mprop: null, madd: true }),
  addGo: (level) => view.set({ addAt: { root: level.root ?? null, dir: level.dir || '' }, addQuery: '' }),
  addUp: () => { const up = parentLevel(addLevel.value); if (up) act.addGo(up); },
  addFilter: (query) => view.set({ addQuery: query }),
  addScrolls: new Map(),
  /** What a row or a picture of the pane does: open a place or a folder, put a tile on the map (pressed, or dragged and dropped on the view). */
  addOn: {
    place: (i) => act.addGo({ root: i, dir: '' }),
    folder: (dir) => act.addGo({ root: addLevel.value.root, dir }),
    tile: (id) => { const t = tileById(id); if (t) E.addTile(t); },
    dragTile: (e, t) => { e.dataTransfer.setData(TILE_MIME, t.id); e.dataTransfer.effectAllowed = 'copy'; },
    picture: (t, plate) => pictureOf(t.id, String(t.meshVersion ?? 0), plate),
    pictureFailed: (e) => showError('Could not draw the picture of a tile', e),
  },
  chooseMode,
  place: E.place,
  variants: E.variants,
  turn: E.turn,
  setPiece: E.setPiece,
  duplicate: E.duplicate,
  remove: E.remove,
  openTile: E.openTile,
};

/** Wire this part of the page (once, at load). */
export function init() {
  view.set({ mlist: readKept() });
  // each level of the map is a view drawn into its section of the levels shell (levels.js shows it)
  render(html`<${MapLevel} m=${mapPanel} act=${act} />`, $('lens-map'));
  render(html`<${AddLevel} m=${mapPanel} act=${act} />`, $('lens-add'));
  render(html`<${PieceLevel} m=${mapPanel} act=${act} />`, $('lens-piece'));
  render(html`<${PropLevel} m=${mapPanel} act=${act} />`, $('lens-prop'));
}
