// @ts-check
// The build target: the base system and the printer profile every build is
// for, as one chip in the title bar.  The server keeps the choice (in the
// user's home, so the next run starts on it) and builds again for it; this
// part shows it and asks.
//
// A print list or a map names its own printer (in its plates panel), so on one the chip says that
// printer, "this map", and the card's printer list changes that one: what the chip names is what
// you pick.  The base system stays the target tiles build for.

import { failed } from '../problems.js';
import { printerOf } from '../doc-views.js';
import { $, host, toast } from './app.js';
import { docs, undoToast } from './docs.js';
import { effect } from '@preact/signals-core';
import { render } from '../view/html.js';
import { chooserFor } from './printer-chooser.js';
import { isMapView, isPlates, server, tile } from './stores.js';
import { steps } from '../steps.js';

const SYSTEMS = { db: 'DungeonBlocks', ol: 'OpenLOCK' };

/** The target in words, such as `DungeonBlocks, default printer`. */
export function targetLabel({ system, printer }) {
  return `${SYSTEMS[system] || system}, ${printer === 'default' ? 'default printer' : `printer ${printer}`}`;
}

/** What the open document will use: a print list's or a map's own printer, from the document's state now, else null (the global target stands). */
export function documentPrinter() {
  const t = tile();
  if (!isPlates(t)) return null;
  return printerOf(docs.get(t.id), server.get().printers, { map: isMapView() });
}

/** Show the target: the chip and the card's words (bound to the model, redone when it changes) and the printer chooser. */
function bind() {
  effect(() => {
    const { system, printer, printers } = server.get();
    const d = documentPrinter();
    if (d) {
      $('target-sys').textContent = d.label;
      $('target-printer').textContent = `this ${d.what}`;
      $('target').title = `This ${d.what} prints on ${d.label} (${d.name}). Click to choose its printer.`;
    } else {
      $('target-sys').textContent = SYSTEMS[system] || system;
      $('target-printer').textContent = printer;
      $('target').title = `Building for ${targetLabel({ system, printer })}. Click to change the base system or the printer.`;
    }
    $('target-for').hidden = !d;
    if (d) $('target-for').textContent = `The printer below is this ${d.what}'s (its plates line). Tiles you open build for ${targetLabel({ system, printer })}; the base system here is theirs.`;
    for (const b of $('target-system').children) b.classList.toggle('on', b.dataset.sys === system);
    // the server lists them; one that has gone from disk is still the chosen one until it is changed
    const current = d ? d.name : printer;
    const names = printers.some((p) => p.name === current) ? printers : [...printers, { name: current, mine: false, bed: null }];
    const t = tile();
    const onPick = d ? (name) => chooseFor(t.id, name) : (name) => choose({ printer: name });
    render(chooserFor({ printers: names, current, onPick, needBed: !!d, skey: 'target' }), $('target-printers'));
  });
}

/** A print list's or a map's own printer: its plates line, as its plates panel sets it. */
async function chooseFor(id, name) {
  // a change to the document, like any other: against the revision held, one step of the history; the answer's
  // scene names the new printer, and the chip and the status line read it there at once
  const r = await docs.edit(id, () => ({ label: 'Could not change the printer', note: `Printer: ${name}`, steps: [steps.platesSet({ key: 'printer', value: name })] }));
  if (r) toast(r.note, () => undoToast(id, r.entry));
}

async function choose(body) {
  const r = await host.setTarget(body);
  if (r.status !== 200) {
    toast(r.body?.error || 'could not change the build target');
    failed('Could not change the build target', r);
  }
  // the new target comes back on the event stream, as a snapshot
}

/** Wire this part of the page (once, at load). */
export function init() {
  for (const b of $('target-system').children) b.onclick = () => choose({ system: b.dataset.sys });
  bind();
}
