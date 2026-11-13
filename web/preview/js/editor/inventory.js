// @ts-check
// The inventory in the editor (docs/inventory.html): what you have printed, as a log of lines in `inventory.dinv`
// beside your tiles.  Three parts of the page speak of it:
//
//   * the tile level (`haveBlock`): "3 printed · seeds 7, 12, 97" and **I have this**, which adds one good print
//     of the tile's current seed, in the colour mode you pick (the last one you used by default);
//   * the folder rows' have and need pills (view/places-panel.js, from js/inventory.js);
//   * the inventory document's own view (view/inventory-panel.js): its pieces grouped by tile with the filters
//     (system, colour, have), the per-line actions (gone, reprint), a tile that moved with its one-click re-link,
//     and **Source**, the file's own KDL.
//
// The program adds the log up and writes it (`/api/inventory/*`); every action here is one line added at the end
// of the log, never an edit of an earlier line.  The model is js/inventory.js, and the sheet after a print is in
// plates.js.

import { effect } from '@preact/signals-core';
import { failed, showError, whyOf } from '../problems.js';
import { html, render } from '../view/html.js';
import { HaveBlock, InventoryPanel } from '../view/inventory-panel.js';
import { $, host, toast } from './app.js';
import { colourName } from '../inventory.js';
import { colourKept, haveBlockModel, inventoryPanel, inventoryUi, pickedColour } from './model.js';
import { isInventory, server, tile, view } from './stores.js';
import { docs } from './docs.js';

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function rememberColour(c) {
  try { localStorage.setItem('dt-inventory-colour', c); } catch { /* private mode: the log's last colour answers next time */ }
  colourKept.value += 1;
}

/** Today in the browser's own zone, `2026-11-12`: the date a print is logged with (the command line has only UTC). */
export function today() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** What the program refused, in words (see `whyOf`), as a toast where it was asked. */
export function say(r, what) {
  toast(`${what}: ${whyOf(r)}`, null, { ms: 9000 });
}

/** Is this a tile (not a model, a print list or the inventory)? */
export const isTileDoc = (t) => !!t && /\.dtile$/i.test(t.path);

/** The strip under the crumbs. */
export function contextLines() {
  const t = tile();
  const p = t?.plan?.inventory;
  if (t?.status === 'failed') return ['This inventory does not read.', 'Source shows the file; the messages say where'];
  if (!p) return ['Reading the inventory…'];
  const lines = p.pieces.reduce((n, x) => n + x.printed + x.failed + x.gone, 0);
  return [`${plural(p.pieces.filter((x) => x.have > 0).length, 'piece')} on the shelf`, `${plural(lines, 'log line')} · a log: nothing is edited away`];
}

async function act(label, run) {
  try {
    const r = await run();
    if (r.status !== 200) { if (!failed(label, r)) say(r, label); return null; }
    return r.body;
  } catch (e) { showError(label, e); return null; }
}

async function haveThis(t, colour, button) {
  button.disabled = true;
  try {
    const r = await host.inventoryHave({ tile: t.id, colour, on: today() });
    if (r.status !== 200) { if (!failed('Could not add to the inventory', r)) say(r, 'Could not add to the inventory'); return; }
    pickedColour.value = null;
    rememberColour(colour);
    toast(`Added ${t.name} to your inventory (${colourName(colour)})`);
  } catch (e) { showError('Could not add to the inventory', e); } finally { button.disabled = false; }
}

const edit = (/** @type {(s: any) => any} */ f) => { const t = tile(); if (t) inventoryUi.update(t.id, f); };

/** What the buttons do. */
const buttons = {
  pickColour: (/** @type {string} */ c) => { pickedColour.value = c; rememberColour(c); },
  haveThis,
  setFace: (/** @type {string} */ face) => edit(() => ({ face })),
  filter: (/** @type {any} */ patch) => edit((s) => ({ filters: { ...(s.filters ?? { system: null, colour: null, show: 'all' }), ...patch } })),
  toggleReprint: (/** @type {string | null} */ key) => edit((s) => ({ reprint: s.reprint === key ? null : key })),
  gone: async (/** @type {any} */ g, /** @type {any} */ p) => {
    const b = await act('Could not mark it gone', () => host.inventoryGone({ tile: p.tile, seed: p.seed, system: p.system, colour: p.colour, on: today() }));
    if (b) toast(`Marked one ${g.name} gone`);
  },
  reprint: async (/** @type {any} */ p, /** @type {string} */ list) => {
    const b = await act('Could not reprint', () => host.inventoryReprint({ tile: p.tile, seed: p.seed, system: p.system, colour: p.colour, list: list || null, docRev: list ? docs.get(list)?.docRev : undefined }));
    if (!b) return;
    edit(() => ({ reprint: null }));
    toast(`${b.made ? 'Made' : 'Added it to'} ${b.list.split(/[\\/]/).pop()}${b.warning ? `. ${b.warning}` : ''}`, () => import('./open.js').then(({ select }) => select(b.id).then(() => view.set({ pageAt: 'edit' }))), { label: 'Open', ms: b.warning ? 12000 : 7000 });
  },
  relink: async (/** @type {any} */ g, /** @type {string} */ id, /** @type {string} */ label) => {
    const b = await act('Could not re-link', () => host.inventoryRelink({ from: g.tile, to: id, on: today() }));
    if (b) toast(`Re-linked ${g.name} to ${label}`);
  },
};

/**
 * What the inventory says of this tile, and the one button that adds to it, for the tile level's sheet.  `I have this`
 * writes one good print for the tile's current seed, the target system and the colour mode picked here; it works
 * without any export.
 */
export const haveBlock = (t) => html`<${HaveBlock} t=${t} m=${haveBlockModel} act=${buttons} />`;

/** Wire this part of the page (once, at load). */
export function init() {
  render(html`<${InventoryPanel} m=${inventoryPanel} act=${buttons} />`, $('lens-inventory'));
  effect(() => { document.body.classList.toggle('inventory', isInventory(tile())); });
  void server;
}
