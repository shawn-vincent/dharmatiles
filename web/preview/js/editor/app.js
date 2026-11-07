// The editor's core: the host, the view, the shared state and the selection's
// identity; every other module reads these.

import { browserHost } from '../host.js';
import { Viewer } from '../viewer.js';

export const $ = (id) => document.getElementById(id);
export const host = browserHost();
export const viewer = new Viewer($('canvas'));

export const state = {
  tiles: [], roots: [], root: -1, selected: -1, overhangDeg: 55, steepestDeg: 65, system: 'db',
  live: false,         // the event stream is open and its snapshot in
  ever: false,         // it has been, once: so a loss reads 'reconnecting…', not 'connecting…'
  gap: false,          // it dropped since: the server's numbering may have started again
  loaded: new Map(),   // tile index -> mesh_version shown (the server's numbering)
  cameras: new Map(),  // tile path -> saved camera and active preset
  view: 'painted', activeView: null, buildStart: new Map(),
  sel: null,           // {entry, piece} (piece null: the whole entry) | {region} | {line} | {tile: true} | null
  hov: null,           // the same, under the pointer (view or text)
  pieces: new Map(),   // entry -> how many pieces its geometry has
  textFor: null,       // [tile index, file, mesh_version] the text pane shows
};
let toastTimer = 0;

export const tile = () => state.tiles[state.selected];
const keyOf = (w) => (!w ? '' : w.tile ? 'tile' : w.region != null ? `r:${w.region}` : w.line != null ? `l:${w.line}` : `e:${w.entry}:${w.piece ?? ''}`);
export const same = (a, b) => keyOf(a) === keyOf(b);
export const isEntry = (w) => !!w && w.entry != null;

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
