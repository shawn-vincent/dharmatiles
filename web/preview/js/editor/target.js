// The build target: the base system and the printer profile every build is
// for, as one chip in the title bar.  The server keeps the choice (in the
// user's home, so the next run starts on it) and builds again for it; this
// part shows it and asks.
//
// A print list or a map names its own printer (in its plates panel), so on one the chip says that
// printer, "for this list"; the card still sets the target tiles build for.

import { failed } from '../problems.js';
import { $, host, toast } from './app.js';
import { printerChooser } from './printer-chooser.js';
import { isMapView, isPlates, server, tile, view } from './stores.js';

const SYSTEMS = { db: 'DungeonBlocks', ol: 'OpenLOCK' };

/** The target in words, such as `DungeonBlocks, default printer`. */
export function targetLabel({ system, printer }) {
  return `${SYSTEMS[system] || system}, ${printer === 'default' ? 'default printer' : `printer ${printer}`}`;
}

/** What the open document will use: a print list's or a map's own printer, else null (the global target stands). */
export function documentPrinter() {
  const t = tile();
  const p = isPlates(t) ? t.plan?.plates : null;
  if (!p?.printer) return null;
  return { name: p.printer, label: p.label || p.printer, what: isMapView() ? 'map' : 'print list' };
}

/** The status line's words for the target: the document's printer on a list or a map, else system and printer. */
export function targetWords() {
  const d = documentPrinter();
  const { system, printer } = server.get();
  return d ? `${d.label} · this ${d.what}` : `${SYSTEMS[system] || system} · ${printer}`;
}

let drawn = null, chooser = null;

function render() {
  const { system, printer, printers } = server.get();
  const d = documentPrinter();
  if (d) {
    $('target-sys').textContent = d.label;
    $('target-printer').textContent = `this ${d.what}`;
    $('target').title = `This ${d.what} prints on ${d.label} (${d.name}), the printer named in its plates panel. Tiles you open are built for ${targetLabel({ system, printer })}. Click to change that.`;
  } else {
    $('target-sys').textContent = SYSTEMS[system] || system;
    $('target-printer').textContent = printer;
    $('target').title = `Building for ${targetLabel({ system, printer })}. Click to change the base system or the printer.`;
  }
  $('target-for').hidden = !d;
  if (d) $('target-for').textContent = `This ${d.what} prints on ${d.label}: choose that in its plates panel. The target below is for tiles.`;
  for (const b of $('target-system').children) b.classList.toggle('on', b.dataset.sys === system);
  // the server lists them; one that has gone from disk is still the chosen one until it is changed
  const names = printers.some((p) => p.name === printer) ? printers : [...printers, { name: printer, mine: false, bed: null }];
  const sig = JSON.stringify([names, printer]);
  if (sig === drawn && chooser) return; // the same list: leave a search or an import as it is
  drawn = sig;
  chooser = printerChooser({ printers: names, current: printer, onPick: (name) => choose({ printer: name }), key: 'target' });
  $('target-printers').replaceChildren(chooser);
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
  server.subscribe(['system', 'printer', 'printers', 'tiles'], render);
  view.subscribe(['tileId', 'dmode'], render);
  for (const b of $('target-system').children) b.onclick = () => choose({ system: b.dataset.sys });
  render();
}
