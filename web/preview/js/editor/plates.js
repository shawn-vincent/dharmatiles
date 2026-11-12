// The plates view: a print list (`.dmap`) opens on the printer's bed.  The 3D view shows one plate
// (the program lays it out and builds its meshes: low at first, then full, said in the message stream
// and the status line); this panel holds the settings that decide the layout, the plates with what is on
// each, the filament check and Export.  A setting is a word of the list's `plates` line: changing it
// rewrites that line in the file (the program does, `POST /api/plates/set`), and the list builds again.
// Docs: docs/plates.html.

import { failed, showError } from '../problems.js';
import { $, host, toast } from './app.js';
import { seg } from './controls.js';
import { exists, plateTotals, systemName, wantLine } from '../inventory.js';
import { say, today } from './inventory.js';
import { isInventory, isPlates, server, stem, tile, view } from './stores.js';

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const mm = (n) => `${Number(n).toFixed(1).replace(/\.0$/, '')}`;

const exporting = { busy: false, files: null, error: null, failures: [], id: null };
// the sheet after a print: which plate it is for and which of its pieces failed (by their place on the plate)
const after = { open: false, plate: 1, bad: new Set(), busy: false };
let drawn = null; // what the panel shows now, as text, so a tick that changes nothing draws nothing
let lastList = null, lastTile = null; // the print list and the tile last shown, for the title bar's switch

const plan = () => (isPlates(tile()) ? tile().plan?.plates ?? null : null);

/** The strip under the crumbs: the printer, and what is laid out. */
export function contextLines() {
  const p = plan(), t = tile();
  if (!p) return [t?.status === 'building' ? 'Laying out the plates…' : 'A print list that does not plan yet: mend it, or choose a printer here.'];
  if (!p.planned) return ['This list does not plan yet.', p.label ? `For the ${p.label}` : 'Choose a printer'];
  const n = p.layout.length;
  const { held } = plateTotals(p);
  return [`${plural(p.layout.reduce((a, x) => a + x.pieces.length, 0), 'piece')} on ${plural(n, 'plate')}${held ? ` · ${held} in inventory` : ''}`, `${p.label} · ${mm(p.bed[0])} × ${mm(p.bed[1])} mm · ${p.mode === 'single' ? 'single colour' : 'multicolour'}`];
}

async function set(key, value) {
  const t = tile();
  if (!t) return;
  try {
    const r = await host.platesSet(t.id, key, value);
    if (r.status !== 200) failed(`Could not change ${key}`, r);
    else exporting.files = null;
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

function printerSection(p, ro) {
  const sec = el('section', 'pl-sec');
  sec.append(el('h3', null, 'Printer'));
  const printers = p.printers ?? [];
  const brands = [...new Set(printers.map((x) => x.brand || 'Yours'))];
  const cur = printers.find((x) => x.name === p.printer);
  const brand = cur ? (cur.brand || 'Yours') : brands[0];
  const row = el('div', 'pl-field');
  row.append(el('span', 'pl-lab', 'Brand'));
  const sb = el('select', 'word-select');
  sb.setAttribute('aria-label', 'Printer brand'); sb.disabled = ro;
  for (const b of brands) { const o = el('option', null, b); o.value = b; o.selected = b === brand; sb.append(o); }
  sb.onchange = () => {
    const first = printers.find((x) => (x.brand || 'Yours') === sb.value);
    if (first) set('printer', first.name);
  };
  row.append(sb);
  sec.append(row);
  const row2 = el('div', 'pl-field');
  row2.append(el('span', 'pl-lab', 'Model'));
  const sm = el('select', 'word-select');
  sm.setAttribute('aria-label', 'Printer model'); sm.disabled = ro;
  for (const x of printers.filter((q) => (q.brand || 'Yours') === brand)) {
    const o = el('option', null, `${x.model || x.name} · ${plural(x.slots, 'filament')}${x.verify ? ' · verify' : ''}`);
    o.value = x.name; o.selected = x.name === p.printer;
    sm.append(o);
  }
  sm.onchange = () => set('printer', sm.value);
  row2.append(sm);
  sec.append(row2);
  // the AMS question, answered once and easy to change: an -ams profile and its plain one
  const base = p.printer.replace(/-ams$/, '');
  const has = (n) => printers.some((x) => x.name === n);
  if (has(`${base}-ams`) && has(base)) {
    sec.append(seg('Filaments', [[base, 'No AMS', 'One filament'], [`${base}-ams`, 'AMS', 'Four filaments a unit']], p.printer, (v) => set('printer', v), ro));
  }
  if (p.bed) sec.append(el('p', 'note', `${mm(p.bed[0])} × ${mm(p.bed[1])} mm bed${p.excluded?.length ? `, ${plural(p.excluded.length, 'zone')} kept clear` : ''} · ${plural(p.slots, 'filament slot')}`));
  if (p.verify) sec.append(el('p', 'note verify', `Not yet checked against the maker: ${p.verify}`));
  return sec;
}

function modeSection(p, ro) {
  const sec = el('section', 'pl-sec');
  sec.append(el('h3', null, 'Print mode'));
  sec.append(seg('Colour', [['single', 'Single colour', 'One filament, no paint'], ['multicolour', 'Multicolour', 'Each material in its own filament']], p.mode, (v) => set('mode', v), ro));
  if (p.mode === 'multicolour') {
    sec.append(seg('Group', [['filament', 'By filament', 'Pieces that share filaments go together, then large to small'], ['size', 'By size', 'Large to small, whatever the filaments']], p.group, (v) => set('group', v), ro));
  }
  const sc = p.slot_check;
  if (p.mode === 'multicolour' && sc) {
    sec.append(el('p', `note ${sc.ok ? 'ok' : 'bad'}`, sc.ok ? `The most any piece needs is ${plural(sc.need, 'filament')}; the printer has ${sc.have}.` : `A piece needs ${sc.need} filaments; the printer has ${sc.have}. Print in single colour or choose a printer with more.`));
  }
  return sec;
}

function numberField(label, key, value, ro, title) {
  const box = el('label', 'pl-field');
  box.append(el('span', 'pl-lab', label));
  const wrap = el('span', 'pl-num');
  const i = el('input');
  i.type = 'number'; i.min = '0'; i.max = '100'; i.step = '0.5'; i.value = String(value); i.disabled = ro;
  i.setAttribute('aria-label', `${label}, millimetres`); i.title = title;
  i.onchange = () => { const v = Number(i.value); if (Number.isFinite(v) && v >= 0 && v <= 100 && v !== value) set(key, `${v}mm`); else i.value = String(value); };
  wrap.append(i, el('span', 'unit', 'mm'));
  box.append(wrap);
  return box;
}

function spacingSection(p, ro) {
  const sec = el('section', 'pl-sec');
  sec.append(el('h3', null, 'Spacing'));
  sec.append(numberField('Between pieces', 'gap', p.gap, ro, 'The gap between two pieces on a plate'));
  sec.append(numberField('To the edge', 'margin', p.margin, ro, 'Between a piece and the edge of the bed: the brim, the skirt, the purge line'));
  sec.append(seg('Turns', [['allowed', 'Allowed', 'A piece may turn a quarter to fit'], ['off', 'Off']], p.turn ? 'allowed' : 'off', (v) => set('turn', v), ro));
  return sec;
}

function platesSection(p) {
  const sec = el('section', 'pl-sec');
  sec.append(el('h3', null, 'Plates'));
  const inv = server.get().inventory;
  const totals = plateTotals(p);
  if (exists(inv) && p.check_inventory !== false) {
    const line = el('p', 'inv-sum');
    line.append(el('b', null, `${totals.held} in inventory`), document.createTextNode(` · ${totals.toPrint} to print`));
    line.title = 'Pieces your inventory already holds are left off the plates; the switch under Inventory prints them anyway';
    sec.append(line);
  }
  const list = el('div', 'pl-plates');
  for (const pl of p.layout) {
    const i = pl.plate - 1;
    const on = i === p.view;
    const b = el('button', `pl-plate${on ? ' on' : ''}`);
    b.type = 'button'; b.setAttribute('aria-pressed', String(on));
    b.title = on ? 'On the bed in the view' : 'Show this plate on the bed';
    const counts = new Map();
    for (const q of pl.pieces) counts.set(q.name, (counts.get(q.name) ?? 0) + 1);
    b.append(el('b', null, `Plate ${pl.plate}`), el('span', 'pl-n', plural(pl.pieces.length, 'piece')));
    if (p.mode === 'multicolour' && pl.filaments.length) b.append(el('span', 'pl-fil', pl.filaments.join(' · ')));
    b.append(el('span', 'pl-what', [...counts].map(([n, c]) => `${c} × ${n}`).join(', ')));
    b.onclick = () => { if (!on) show(i); };
    list.append(b);
  }
  sec.append(list);
  if (p.grouping_cost) {
    const n = el('p', 'note warn', `Grouping by filament takes ${plural(p.grouping_cost, 'more plate')} than packing by size. `);
    const go = el('button', 'linkish', 'Pack by size only');
    go.type = 'button'; go.onclick = () => set('group', 'size');
    n.append(go);
    sec.append(n);
  }
  const low = p.kinds.filter((k) => !k.exact).length;
  sec.append(el('p', `note${low ? ' warn' : ' ok'}`, low
    ? `${plural(low, 'tile')} shown as low meshes, not yet checked: full builds start for the plate in view; the rest build when you export. The plates are laid out again by what the full builds cover.`
    : 'Every piece is a full build, checked by the printability gate.'));
  return sec;
}

/** What the inventory says of this list: the switch that prints only what you lack, and each piece against the shelf. */
function inventorySection(p, ro) {
  const inv = server.get().inventory;
  const sec = el('section', 'pl-sec pl-inv');
  sec.append(el('h3', null, 'Inventory'));
  const check = p.check_inventory !== false;
  sec.append(seg('Print', [['check', 'Only what I don’t have', 'The count, less what your inventory holds of that exact piece (tile, seed, system, colour mode)'], ['ignore', 'Everything', 'Every piece the list names, whatever you have']], check ? 'check' : 'ignore', (v) => set('inventory', v), ro));
  if (!exists(inv)) {
    sec.append(el('p', 'note', 'No inventory yet, so every piece prints. The sheet after a print, or I have this on a tile, makes it.'));
    return sec;
  }
  if (!p.planned) return sec;
  const { held, toPrint } = plateTotals(p);
  const line = el('p', 'inv-sum');
  line.append(el('b', null, check ? `${held} in inventory` : 'Inventory not used'), document.createTextNode(check ? ` · ${toPrint} to print` : ` · ${toPrint} to print`));
  sec.append(line);
  if (check) {
    const rows = el('div', 'pl-kinds');
    const row = (name, k, mode) => {
      const w = wantLine(k, mode);
      const r = el('div', `pl-kind${w.caution ? ' caution' : ''}`);
      r.append(el('b', null, name), el('span', 'pl-kw', k.want > k.count ? `${k.want} wanted` : `${k.want}`), el('span', 'pl-kt', w.text));
      if (k.system) r.title = `${systemName(k.system)}, seed ${k.seed_used}`;
      if (w.caution) r.title = `${r.title || ''}${r.title ? '. ' : ''}You have this tile and seed in the other colour mode; it prints unless you leave it off (print=skip) or switch the mode`;
      rows.append(r);
    };
    for (const k of p.kinds) if (k.have > 0 || k.other_colour > 0) row(k.name, k, p.mode);
    for (const h of p.held ?? []) {
      const r = el('div', 'pl-kind');
      r.append(el('b', null, h.name), el('span', 'pl-kw', `${h.have}`), el('span', 'pl-kt', `have ${h.have} · none to print`));
      rows.append(r);
    }
    if (rows.children.length) sec.append(rows);
  }
  return sec;
}

/** The sheet after a print: the plate's pieces, all ticked good; untick the failures and one press adds the rest. */
function afterSection(p) {
  const sec = el('section', 'pl-sec pl-after');
  sec.append(el('h3', null, 'After the print'));
  if (!p.planned) return sec;
  const n = p.layout.length;
  if (!after.open) {
    sec.append(el('p', 'note', 'When the plate is off the printer, mark what failed and the rest goes into your inventory. You can do this any time, from any plate.'));
    const b = el('button', 'btn', 'Record a plate…');
    b.type = 'button';
    b.onclick = () => { after.open = true; after.plate = p.view + 1; after.bad.clear(); redraw(); };
    sec.append(b);
    return sec;
  }
  after.plate = Math.min(Math.max(after.plate, 1), n);
  if (n > 1) sec.append(seg('Plate', p.layout.map((pl) => [pl.plate, String(pl.plate)]), after.plate, (v) => { after.plate = v; after.bad.clear(); redraw(); }));
  const pl = p.layout[after.plate - 1];
  const sheet = el('div', 'pl-sheet');
  pl.pieces.forEach((q, i) => {
    const k = p.kinds[q.kind];
    const row = el('label', `pl-piece${after.bad.has(i) ? ' bad' : ''}`);
    const box = el('input');
    box.type = 'checkbox'; box.checked = !after.bad.has(i);
    box.setAttribute('aria-label', `${q.name}, seed ${k?.seed_used}: good`);
    box.onchange = () => { if (box.checked) after.bad.delete(i); else after.bad.add(i); redraw(); };
    row.append(box, el('b', null, q.name), el('span', 'pl-seed', `seed ${k?.seed_used ?? '?'}`), el('span', 'pl-res', after.bad.has(i) ? 'failed' : 'good'));
    sheet.append(row);
  });
  sec.append(sheet);
  const bad = after.bad.size, good = pl.pieces.length - bad;
  sec.append(el('p', 'note', `Dated ${today()}, with each tile’s hash, from ${p.title ?? stem(tile()).replace(/\.dmap$/i, '')}/plate-${after.plate}. Failed ones are kept in the log and do not count.`));
  const go = el('button', 'btn on', `Add ${good} to the inventory${bad ? ` · ${bad} failed` : ''}`);
  go.type = 'button'; go.disabled = after.busy || !pl.pieces.length;
  go.onclick = recordPlate;
  const x = el('button', 'linkish', 'Close');
  x.type = 'button'; x.onclick = () => { after.open = false; after.bad.clear(); redraw(); };
  const row = el('div', 'pl-after-go');
  row.append(go, x);
  sec.append(row);
  return sec;
}

async function recordPlate() {
  const t = tile();
  if (!t || after.busy) return;
  after.busy = true; redraw();
  try {
    const r = await host.inventoryRecord({ list: t.id, plate: after.plate, failed: [...after.bad], on: today() });
    if (r.status !== 200) failed('Could not add the plate to the inventory', r);
    else if (r.body.ok === false) say(r.body, 'Could not add the plate to the inventory');
    else {
      toast(`Added ${plural(r.body.lines.length, 'piece')} to your inventory`);
      after.open = false; after.bad.clear();
    }
  } catch (e) { showError('Could not add the plate to the inventory', e); }
  after.busy = false;
  redraw();
}

function redraw() { drawn = null; render(); }

function exportSection(p, ro) {
  const sec = el('section', 'pl-sec pl-export');
  const bambu = p.format === 'bambu';
  const t = tile();
  const b = el('button', 'btn on', exporting.busy && exporting.id === t?.id ? 'Exporting…' : bambu ? 'Export Bambu project' : `Export ${plural(p.layout.length, 'STL')}`);
  b.type = 'button';
  b.disabled = exporting.busy || !p.planned || (p.mode === 'multicolour' && !p.slot_check?.ok);
  b.title = bambu ? 'One Bambu Studio project holding every plate' : 'One file per plate: every slicer reads an STL';
  b.onclick = doExport;
  sec.append(b);
  sec.append(el('p', 'note', exporting.busy ? 'Building every piece in full, through the gate…' : bambu ? 'Every plate in one project; paint per triangle in multicolour.' : 'One STL per plate; single colour needs no paint.'));
  if (exporting.error) sec.append(el('p', 'note bad', exporting.error));
  for (const f of exporting.failures) sec.append(el('p', 'note bad', `Gate: ${f}`));
  if (exporting.files) {
    const box = el('div', 'pl-files');
    for (const f of exporting.files) { const c = el('code', null, f.split(/[\\/]/).slice(-3).join('/')); c.title = f; box.append(c); }
    const r = el('button', 'linkish', 'Reveal in Finder');
    r.type = 'button';
    r.onclick = async () => { try { failed('Could not show the file', await host.reveal(exporting.files[0])); } catch (e) { showError('Could not show the file', e); } };
    sec.append(box, r);
  }
  void ro;
  return sec;
}

async function doExport() {
  const t = tile();
  if (!t || exporting.busy) return;
  Object.assign(exporting, { busy: true, files: null, error: null, failures: [], id: t.id });
  render();
  try {
    const r = await host.platesExport(t.id);
    if (r.status !== 200) { exporting.error = r.body?.error || `could not export (${r.status})`; if (r.status !== 0) failed(`Could not export ${stem(t)}`, r); }
    else if (!r.body.ok) exporting.error = r.body.errors?.[0]?.message || 'the export failed';
    else { exporting.files = r.body.files; exporting.failures = r.body.failures ?? []; after.open = true; after.plate = (plan()?.view ?? 0) + 1; after.bad.clear(); }
  } catch (e) { exporting.error = 'the export failed'; showError(`Could not export ${stem(t)}`, e); }
  exporting.busy = false;
  render();
}

function render() {
  const box = $('lens-plates');
  const t = tile();
  if (!box) return;
  const plates = isPlates(t);
  if (plates) { lastList = t.path; } else if (t && !isInventory(t)) lastTile = t.path;
  syncModes();
  document.body.classList.toggle('plates', plates); // the tile's Place or draw box is not for a print list
  if (!plates) return;
  const p = plan();
  // a build's progress ticks come through here too: draw again only when what is drawn changed
  const sig = JSON.stringify([t.id, t.plan, t.status, t.errors, exporting.busy, exporting.files, exporting.error, exporting.failures, after.open, after.plate, [...after.bad], after.busy, server.get().inventory?.exists, server.get().inventory?.pieces?.length]);
  if (sig === drawn) return;
  drawn = sig;
  const keep = box.querySelector('.body')?.scrollTop ?? 0;
  const ro = !(p?.writable ?? t.plan?.writable);
  box.replaceChildren();
  const head = el('div', 'lt');
  const h = el('div', 'heading');
  h.append(el('span', 'cap', 'Print list'), el('b', null, stem(t).replace(/\.dmap$/i, '')));
  head.append(h);
  box.append(head);
  const body = el('div', 'body');
  if (!p) { body.append(el('div', 'empty', t.status === 'failed' ? 'This list does not plan.' : 'Laying out the plates…')); }
  else {
    if (!p.planned) {
      body.append(el('p', 'note bad', 'This list does not plan yet. You can still choose the printer and the mode here.'));
      // the first few reasons, here as well as in the messages (⌃J)
      for (const e of (t.errors ?? []).slice(0, 3)) {
        const n = el('p', 'note bad', e.message);
        if (e.help) n.append(el('span', 'pl-help', ` ${e.help}`));
        body.append(n);
      }
      if ((t.errors ?? []).length > 3) body.append(el('p', 'note', `and ${(t.errors.length - 3)} more in the messages`));
    }
    if (ro) body.append(el('p', 'note', 'This file cannot be changed here.'));
    // the plates first: the core of it is to open a list, see its plates and export; the settings that decide them follow
    if (p.planned) body.append(platesSection(p), exportSection(p, ro), afterSection(p));
    body.append(inventorySection(p, ro), printerSection(p, ro), modeSection(p, ro));
    if (p.planned) body.append(spacingSection(p, ro));
  }
  box.append(body);
  body.scrollTop = keep;
  // the status line's dot and the progress live in panel.js; here only the panel
}

/** The title bar's switch: Tile or Plates, following the document in view; pressing the other one goes to the last of its kind. */
function syncModes() {
  const on = isPlates(tile());
  for (const b of $('modes').children) b.setAttribute('aria-pressed', String(b.dataset.mode === (on ? 'plates' : 'tile')));
}

function pickMode(mode) {
  const here = isPlates(tile()) ? 'plates' : 'tile';
  if (mode === here) return;
  const path = mode === 'plates' ? lastList : lastTile;
  const t = server.get().tiles.find((x) => x.path === path);
  if (t) { view.set({ tileId: t.id, page: 'edit', sel: null, control: null }); return; }
  view.set({ page: 'browse' });
  toast(mode === 'plates' ? 'Choose a print list (.dmap) from a folder.' : 'Choose a tile.');
}

/** Wire this part of the page (once, at load). */
export function init() {
  for (const b of $('modes').children) b.onclick = () => pickMode(b.dataset.mode);
  server.subscribe(['tiles'], render);
  view.subscribe(['tileId'], () => { drawn = null; exporting.files = null; exporting.error = null; exporting.failures = []; after.open = false; after.bad.clear(); render(); });
  server.subscribe(['inventory'], () => { drawn = null; render(); });
  render();
}
