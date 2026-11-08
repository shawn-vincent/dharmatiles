// The editor's core: the host, the 3D view, and the little every part shares
// (the stores themselves are in stores.js).

import { browserHost } from '../host.js';
import { Viewer } from '../viewer.js';
import { view } from './stores.js';

export const $ = (id) => document.getElementById(id);
export const host = browserHost();
export const viewer = new Viewer($('canvas'));

let toastTimer = 0;
export function toast(message, action = null) {
  const box = $('toast');
  clearTimeout(toastTimer);
  box.querySelector('span').textContent = message;
  const button = box.querySelector('button');
  button.hidden = !action;
  button.onclick = action ? () => { box.hidden = true; action(); } : null;
  box.hidden = false;
  toastTimer = setTimeout(() => { box.hidden = true; }, 5000);
}

/** Take the camera to one of the view presets (chrome.js moves it). */
export const pickView = (name, animate = true) => view.set({ pick: { name, animate } });

/** Ask whether to make the library tile shown your own (tiles.js shows the question). */
export const offerMine = () => view.set({ mineOpen: true });

// each element's family, for its colour (the docs site uses the same)
export const FAMILY = {
  ground: 'surface', dirt: 'surface', floor: 'surface', water: 'water',
  rock: 'thing', stone: 'thing', boulder: 'thing', pebble: 'thing', scatter: 'thing', cliff: 'thing', wall: 'thing',
  grass: 'plant', reeds: 'plant', flowers: 'plant', tree: 'plant',
  dungeonblocks: 'base', openlock: 'base', connector: 'base', stl: 'base',
};
