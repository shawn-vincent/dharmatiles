// @ts-check
// The editor's core: the host, the 3D view, and the little every part shares
// (the stores themselves are in stores.js).

import { need } from '../dom.js';
import { chooseHost } from '../host.js';
import { Viewer } from '../viewer.js';
import { view } from './stores.js';

/**
 * The kinds of the page's elements that are not a plain `HTMLElement` (an input, a dialog, a canvas, a form, a button), by
 * id, as index.html marks them up (a test holds this table to the markup).  Any other id is an `HTMLElement`.
 * @typedef {{
 *   'filter': HTMLInputElement, 't-paint': HTMLInputElement, 't-overhang': HTMLInputElement, 't-wire': HTMLInputElement,
 *   't-grid': HTMLInputElement, 't-fade': HTMLInputElement, 't-pills': HTMLInputElement, 't-debug': HTMLInputElement,
 *   't-right': HTMLInputElement, 'export-3mf': HTMLInputElement, 'place-in': HTMLInputElement, 'new-name': HTMLInputElement,
 *   'new-size': HTMLInputElement,
 *   'new-dialog': HTMLDialogElement, 'keys': HTMLDialogElement, 'about': HTMLDialogElement, 'leave-dialog': HTMLDialogElement,
 *   'canvas': HTMLCanvasElement, 'over': HTMLCanvasElement,
 *   'new-form': HTMLFormElement,
 *   'undo': HTMLButtonElement, 'undo-drop': HTMLButtonElement, 'redo': HTMLButtonElement, 'redo-drop': HTMLButtonElement,
 *   'target': HTMLButtonElement, 'export': HTMLButtonElement, 'panelbtn': HTMLButtonElement, 'msgbtn': HTMLButtonElement,
 *   'theme': HTMLButtonElement, 'bback': HTMLButtonElement, 'forward': HTMLButtonElement, 'back': HTMLButtonElement,
 *   'tilebtn': HTMLButtonElement, 'part-back': HTMLButtonElement, 'dispbtn': HTMLButtonElement, 'b-proj': HTMLButtonElement,
 *   'b-keys': HTMLButtonElement, 'b-about': HTMLButtonElement, 'export-reveal': HTMLButtonElement, 'export-go': HTMLButtonElement,
 *   'msg-close': HTMLButtonElement, 'sb-main': HTMLButtonElement, 'empty-new': HTMLButtonElement, 'choose': HTMLButtonElement,
 *   'new-cancel': HTMLButtonElement, 'new-ok': HTMLButtonElement, 'leave-discard': HTMLButtonElement,
 * }} IdKinds
 */

/**
 * An element of the page by its id.  The one DOM lookup boundary of the editor: an id the page has is the element it names,
 * typed by `IdKinds`, and one it does not have throws, naming it (js/dom.js `need`).  Ask `document.getElementById` for an
 * element that may not be there yet (one a component draws).
 * @template {string} K
 * @param {K} id
 * @returns {K extends keyof IdKinds ? IdKinds[K] : HTMLElement}
 */
export const $ = (id) => /** @type {any} */ (need(document, `#${id}`)); // the conditional type is the table's reading of the id, which a body cannot state
export const host = chooseHost();
/** The one viewer. */
export const viewer = new Viewer(/** @type {HTMLCanvasElement} */ ($('canvas')));

let toastTimer = 0;
/**
 * A line over the view with, optionally, one action (`label` defaults to Undo); it goes after `ms`.
 * @param {string} message
 * @param {(() => void) | null} [action]
 * @param {{ label?: string, ms?: number }} [opts]
 */
export function toast(message, action = null, { label = 'Undo', ms = 5000 } = {}) {
  const box = $('toast');
  clearTimeout(toastTimer);
  need(box, 'span').textContent = message;
  const button = need(box, 'button');
  button.hidden = !action;
  button.textContent = label;
  button.onclick = action ? () => { box.hidden = true; action(); } : null;
  box.hidden = false;
  toastTimer = setTimeout(() => { box.hidden = true; }, ms);
}

/** Ask for one of the view presets: the view's preset and the ask are one write, and the viewer frames what it shows (editor/framing.js describes it). */
export const pickView = (/** @type {string} */ name, animate = true) => view.set({ pick: { name, animate }, mode: name });

// A library tile is read only until the first change to it: that change copies
// it into My tiles and is made there (tiles.js does the copy).  A part of the
// page that was about to change a library tile calls `makeMine(replay)`, where
// `replay(how)` makes the same change once the copy is open: `how.sameText` says
// whether the copy's text is the library tile's, byte for byte (so a position
// from the library tile's text is still right in it).
/** @typedef {{ patch?: *, run?: () => Promise<*> }} Replay  the change to make once the copy is open */
/** @type {((replay: Replay | null) => boolean | Promise<boolean>) | null} */
let mineHandler = null;
export const onMakeMine = (/** @type {(replay: Replay | null) => boolean | Promise<boolean>} */ fn) => { mineHandler = fn; };
/**
 * Copy the library tile shown into My tiles and make the change `replay` makes there; false (nothing is done) when no part can.
 * @param {Replay | null} [replay]
 */
export function makeMine(replay = null) { return mineHandler ? mineHandler(replay) : false; }
