// @ts-check
// The editor's core: the host, the 3D view, and the little every part shares
// (the stores themselves are in stores.js).

import { browserHost } from '../host.js';
import { Viewer } from '../viewer.js';
import { view } from './stores.js';

/** An element of the page by its id (the page's own markup: not checked here). @param {string} id @returns {any} */
export const $ = (id) => document.getElementById(id);
export const host = browserHost();
/** The one viewer (its methods come from mixins, so its class is not checked here). @type {any} */
export const viewer = new Viewer($('canvas'));

let toastTimer = 0;
/** A line over the view with, optionally, one action (`label` defaults to Undo); it goes after `ms`. */
export function toast(message, action = null, { label = 'Undo', ms = 5000 } = {}) {
  const box = $('toast');
  clearTimeout(toastTimer);
  box.querySelector('span').textContent = message;
  const button = box.querySelector('button');
  button.hidden = !action;
  button.textContent = label;
  button.onclick = action ? () => { box.hidden = true; action(); } : null;
  box.hidden = false;
  toastTimer = setTimeout(() => { box.hidden = true; }, ms);
}

/** Take the camera to one of the view presets (chrome.js moves it). */
export const pickView = (name, animate = true) => view.set({ pick: { name, animate } });

// A library tile is read only until the first change to it: that change copies
// it into My tiles and is made there (tiles.js does the copy).  A part of the
// page that was about to change a library tile calls `makeMine(replay)`, where
// `replay(how)` makes the same change once the copy is open: `how.sameText` says
// whether the copy's text is the library tile's, byte for byte (so a position
// from the library tile's text is still right in it).
let mineHandler = null;
export const onMakeMine = (fn) => { mineHandler = fn; };
/** Copy the library tile shown into My tiles and make the change `replay` makes there; false (nothing is done) when no part can. */
export function makeMine(replay = null) { return mineHandler ? mineHandler(replay) : false; }
