// The build target: the base system and the printer profile every build is
// for, as one chip in the title bar.  The server keeps the choice (in the
// user's home, so the next run starts on it) and builds again for it; this
// part shows it and asks.

import { failed } from '../problems.js';
import { $, host, toast } from './app.js';
import { server } from './stores.js';

const SYSTEMS = { db: 'DungeonBlocks', ol: 'OpenLOCK' };

/** The target in words, such as `DungeonBlocks, default printer`. */
export function targetLabel({ system, printer }) {
  return `${SYSTEMS[system] || system}, ${printer === 'default' ? 'default printer' : `printer ${printer}`}`;
}

function render() {
  const { system, printer, printers } = server.get();
  $('target-sys').textContent = SYSTEMS[system] || system;
  $('target-printer').textContent = printer;
  $('target').title = `Building for ${targetLabel({ system, printer })}. Click to change the base system or the printer.`;
  for (const b of $('target-system').children) b.classList.toggle('on', b.dataset.sys === system);
  const list = $('target-printers');
  list.replaceChildren();
  // the server lists them; one that has gone from disk is still the chosen one until it is changed
  const names = printers.some((p) => p.name === printer) ? printers : [...printers, { name: printer, mine: false }];
  for (const p of names) {
    const b = document.createElement('button');
    b.type = 'button'; b.dataset.printer = p.name;
    b.classList.toggle('on', p.name === printer);
    b.append(p.name);
    if (p.mine) { const s = document.createElement('span'); s.className = 'own'; s.textContent = 'yours'; b.append(s); }
    b.onclick = () => choose({ printer: p.name });
    list.append(b);
  }
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
  server.subscribe(['system', 'printer', 'printers'], render);
  for (const b of $('target-system').children) b.onclick = () => choose({ system: b.dataset.sys });
  render();
}
