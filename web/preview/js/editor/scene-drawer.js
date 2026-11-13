// The catalog drawer on this page (docs/maps.html): view/drawer.js drawn from the model (editor/model.js `drawerPanel`) into a
// box along the bottom of the view, with a card pressed putting its tile on the map (the nearest free place) and a card dragged
// onto the map dropped where it should stand.  Nothing is drawn here: the drawer is drawn again by itself when the list, the
// level, the filter or the inventory changes.

import { effect } from '@preact/signals-core';
import { pill } from '../inventory.js';
import { showError } from '../problems.js';
import { html, render } from '../view/html.js';
import { Drawer } from '../view/drawer.js';
import { $ } from './app.js';
import { drawerPanel, drawerUi } from './model.js';
import { TILE_MIME } from './scene-drag.js';
import { addTile } from './scene-edit.js';
import { pictureOf } from './scene-thumbs.js';
import { server, view } from './stores.js';

/** What the drawer's cards and crumbs do. */
const act = {
  goTo: (level) => { drawerUi.value = { ...drawerUi.value, level }; },
  filter: (query) => { drawerUi.value = { ...drawerUi.value, query }; },
  close: () => view.set({ drawer: false }),
  addTile,
  dragTile: (e, t) => { e.dataTransfer.setData(TILE_MIME, t.id); e.dataTransfer.effectAllowed = 'copy'; },
  pill: (t) => pill(server.get().inventory, t.path, drawerPanel.data.value?.sys ?? 'db'),
  picture: (t, plate) => pictureOf(t.id, String(t.meshVersion ?? 0), plate),
  pictureFailed: (e) => showError('Could not draw the picture of a tile', e),
};

/** Wire this part of the page (once, at load). */
export function init() {
  const box = document.createElement('div');
  box.id = 'drawer';
  box.hidden = true;
  box.setAttribute('role', 'region');
  box.setAttribute('aria-label', 'The catalog');
  $('view').append(box);
  render(html`<${Drawer} m=${drawerPanel} act=${act} />`, box);
  // shown while it is open and a map is the view
  effect(() => {
    const on = !!drawerPanel.data.value;
    box.hidden = !on;
    document.body.classList.toggle('drawer', on);
  });
}
