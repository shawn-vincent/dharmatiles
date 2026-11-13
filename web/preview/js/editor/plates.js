// @ts-check
// The plates view: a print list (`.dmap`) opens on the printer's bed.  The 3D view shows one plate (the program lays
// it out and builds its meshes: low at first, then full, said in the message stream and the status line); the panel
// (js/view/plates-panel.js, drawn from the model in model.js) holds the settings that decide the layout, the plates
// with what is on each, the filament check and Export.  This part is what the buttons do, the title bar's switch and
// the strip under the crumbs.  A setting is a word of the list's `plates` line: changing it rewrites that line in the
// file (the program does, `POST /api/plates/set`), and the list builds again.  Docs: docs/plates.html.

import { effect } from '@preact/signals-core';
import { failed, showError } from '../problems.js';
import { html, render } from '../view/html.js';
import { PlatesPanel } from '../view/plates-panel.js';
import { $, host, toast } from './app.js';
import { docs, undoToast } from './docs.js';
import { chooserFor } from './printer-chooser.js';
import { say, today } from './inventory.js';
import { removePiece, setCount } from './lists.js';
import { isInventory, isMapView, isPlates, server, stem, tile, view } from './stores.js';
import { chooseMode, setChosen } from './scene.js';
import { platesPanel, platesUi, plan } from './model.js';
import { unpin, unpinAll } from './plates-pin.js';
import { steps } from '../steps.js';

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const mm = (n) => `${Number(n).toFixed(1).replace(/\.0$/, '')}`;

let lastList = null, lastTile = null; // the print list and the tile last shown, for the title bar's switch

/** The strip under the crumbs: what is laid out, and the bed (the printer's name is the title bar's chip's). */
export function contextLines() {
  const p = plan.value, t = tile();
  if (!p) return [t?.status === 'building' ? 'Laying out the plates…' : 'A print list that does not plan yet: mend it, or choose a printer here.'];
  if (!p.planned) return ['This list does not plan yet.', p.label ? `For the ${p.label}` : 'Choose a printer'];
  const n = p.layout.length;
  return [`${plural(p.layout.reduce((a, x) => a + x.pieces.length, 0), 'piece')} on ${plural(n, 'plate')}`, `${mm(p.bed[0])} × ${mm(p.bed[1])} mm · ${p.mode === 'single' ? 'single colour' : 'multicolour'}`];
}

const ui = () => platesUi.value.value;
const change = (/** @type {(s: any) => any} */ edit) => { const t = tile(); if (t) platesUi.update(t.id, edit); };
const exporting = (/** @type {any} */ patch) => change((s) => ({ exporting: { ...s.exporting, ...patch } }));
const sheet = (/** @type {any} */ patch) => change((s) => ({ after: { ...s.after, ...patch } }));

/** A setting is a change to the list's document: the plates line's word, against the revision held, one step of the history. */
async function set(key, value) {
  const t = tile();
  if (!t) return;
  try {
    const r = await docs.edit(t.id, () => ({ label: `Could not change ${key}`, note: `Set ${key}`, steps: [steps.platesSet({ key, value })] }));
    if (r) { exporting({ files: null }); toast(r.note, () => undoToast(t.id, r.entry)); }
  } catch (e) { showError(`Could not change ${key}`, e); }
}

async function show(plate) {
  const t = tile();
  if (!t) return;
  try {
    const r = await host.platesShow(t.id, plate);
    if (r.status !== 200) failed('Could not show that plate', r);
  } catch (e) { showError('Could not show that plate', e); }
}

async function recordPlate() {
  const t = tile();
  const a = ui().after;
  if (!t || a.busy) return;
  sheet({ busy: true });
  try {
    const r = await host.inventoryRecord({ list: t.id, plate: a.plate, failed: [...a.bad], on: today() });
    if (r.status !== 200) { if (!failed('Could not add the plate to the inventory', r)) say(r, 'Could not add the plate to the inventory'); }
    else {
      toast(`Added ${plural(r.body.lines.length, 'piece')} to your inventory`);
      sheet({ open: false, bad: [] });
    }
  } catch (e) { showError('Could not add the plate to the inventory', e); }
  sheet({ busy: false });
}

async function doExport() {
  const t = tile();
  if (!t || ui().exporting.busy) return;
  exporting({ busy: true, files: null, error: null, failures: [] });
  try {
    const r = await host.platesExport(t.id);
    if (r.status !== 200) { exporting({ error: r.body?.error || `could not export (${r.status})` }); if (r.status !== 0) failed(`Could not export ${stem(t)}`, r); }
    else { exporting({ files: r.body.files, failures: r.body.failures ?? [] }); sheet({ open: true, plate: (plan.value?.view ?? 0) + 1, bad: [] }); }
  } catch (e) { exporting({ error: 'the export failed' }); showError(`Could not export ${stem(t)}`, e); }
  exporting({ busy: false });
}

/** What the panel's buttons do. */
const act = {
  set, show, unpin, unpinAll, setCount, removePiece, doExport, recordPlate, today,
  chooser: chooserFor,
  openSheet: (/** @type {number} */ plate) => sheet({ open: true, plate, bad: [] }),
  closeSheet: () => sheet({ open: false, bad: [] }),
  sheetPlate: (/** @type {number} */ plate) => sheet({ plate, bad: [] }),
  sheetMark: (/** @type {number} */ i, /** @type {boolean} */ good) => change((s) => ({ after: { ...s.after, bad: good ? s.after.bad.filter((x) => x !== i) : [...new Set([...s.after.bad, i])] } })),
  reveal: async (/** @type {string} */ path) => { try { failed('Could not show the file', await host.reveal(path)); } catch (e) { showError('Could not show the file', e); } },
};

/** The title bar's switch: Tile, Map or Plates, following the document in view; pressing the other one goes to the last of its kind. */
function pickMode(mode) {
  const here = isPlates(tile()) ? (isMapView() ? 'map' : 'plates') : 'tile';
  if (mode === here) return;
  // the map and the plates are two views of one document
  if (mode !== 'tile' && isPlates(tile())) { chooseMode(mode); return; }
  const path = mode === 'tile' ? lastTile : lastList;
  const t = server.get().tiles.find((x) => x.path === path);
  if (t) {
    if (mode !== 'tile') setChosen(t.path, mode);
    view.set({ tileId: t.id, pageAt: 'edit', sel: null, control: null });
    return;
  }
  view.set({ pageAt: 'browse' });
  toast(mode === 'map' ? 'Choose a map (.dmap) from a folder.' : mode === 'plates' ? 'Choose a print list (.dmap) from a folder.' : 'Choose a tile.');
}

/** Wire this part of the page (once, at load). */
export function init() {
  for (const b of $('modes').children) b.onclick = () => pickMode(b.dataset.mode);
  render(html`<${PlatesPanel} m=${platesPanel} act=${act} />`, $('lens-plates'));
  // the title bar's switch and the page's class follow the document in view (bound to the model, not stored)
  effect(() => {
    const t = tile();
    const doc = isPlates(t);
    if (doc) lastList = t.path; else if (t && !isInventory(t)) lastTile = t.path;
    const now = doc ? (isMapView() ? 'map' : 'plates') : 'tile';
    for (const b of $('modes').children) b.setAttribute('aria-pressed', String(b.dataset.mode === now));
    document.body.classList.toggle('plates', doc); // the tile's Place or draw box is not for a print list or a map
  });
}
