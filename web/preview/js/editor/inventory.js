// @ts-check
// The inventory in the editor (docs/inventory.html): what you have printed, as a log of lines in `inventory.dinv`
// beside your tiles.  Three parts of the page speak of it:
//
//   * the tile level (`haveBlock`): "3 printed · seeds 7, 12, 97" and **I have this**, which adds one good print
//     of the tile's current seed, in the colour mode you pick (the last one you used by default);
//   * the folder rows' have and need pills (folder-list.js, from js/inventory.js);
//   * the inventory document's own view (this file's `render`): its pieces grouped by tile with the filters
//     (system, colour, have), the per-line actions (gone, reprint), a tile that moved with its one-click re-link,
//     and **Source**, the file's own KDL.
//
// The program adds the log up and writes it (`/api/inventory/*`); every action here is one line added at the end
// of the log, never an edit of an earlier line.  The model is js/inventory.js, and the sheet after a print is in
// plates.js.

import { failed, showError, whyOf } from '../problems.js';
import { $, host, toast } from './app.js';
import {
  SYSTEMS, canonOf, colourName, defaultFilters, exists, groups, haveLine, nameOf, numbered, relinkCandidates, systemName, tileHave, totalsLine,
} from '../inventory.js';
import { isInventory, server, tile, view } from './stores.js';
import { seg } from './controls.js';
import { docs } from './docs.js';

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

// ---- what the page remembers ---------------------------------------------------------------------------

/** The colour mode last used for "I have this": remembered in the browser, else the one the log last wrote, else single. */
export function lastColour() {
  try {
    const c = localStorage.getItem('dt-inventory-colour');
    if (c === 'single' || c === 'multi') return c;
  } catch { /* private mode, or nothing kept: the log's own last colour answers */ }
  return server.get().inventory?.lastColour || 'single';
}
function rememberColour(c) {
  try { localStorage.setItem('dt-inventory-colour', c); } catch { /* private mode: the log's last colour answers next time */ }
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

// ---- the tile level ------------------------------------------------------------------------------------

let pickedColour = null; // the colour mode chosen at the tile level, until a print is added

/** Is this a tile (not a model, a print list or the inventory)? */
export const isTileDoc = (t) => !!t && /\.dtile$/i.test(t.path);

/**
 * What the inventory says of this tile, and the one button that adds to it.  `I have this` writes one good
 * print for the tile's current seed, the target system and the colour mode picked here; it works without any export.
 */
export function haveBlock(t) {
  const inv = server.get().inventory;
  const system = server.get().system;
  const box = el('section', 'inv-block');
  box.append(el('h3', null, `Inventory · ${systemName(system)}`));
  const h = tileHave(inv, canonOf(t.path, inv?.home), system);
  const line = el('p', `inv-have${h.have ? '' : ' none'}`, exists(inv) ? haveLine(h) : 'No inventory yet. The first print you add makes the file.');
  box.append(line);
  if (h.edited) {
    const n = el('p', 'inv-edited', 'Edited since printing');
    n.title = 'The tile changed since the print you have was made. It still counts as had; this is only to tell you.';
    box.append(n);
  }
  const colour = pickedColour ?? lastColour();
  const ask = el('div', 'inv-ask');
  ask.append(seg('Colour', [['single', 'Single colour', 'One filament, no paint'], ['multi', 'Multicolour', 'Each material in its own filament']], colour, (c) => { pickedColour = c; rememberColour(c); box.replaceWith(haveBlock(t)); }));
  const b = el('button', 'btn', 'I have this');
  b.type = 'button';
  b.title = `Add one good print of this tile (its seed now, ${systemName(system)}, ${colourName(colour)}) to your inventory, without exporting anything`;
  b.onclick = () => haveThis(t, colour, b);
  ask.append(b);
  box.append(ask);
  return box;
}

async function haveThis(t, colour, button) {
  button.disabled = true;
  try {
    const r = await host.inventoryHave({ tile: t.id, colour, on: today() });
    if (r.status !== 200) { if (!failed('Could not add to the inventory', r)) say(r, 'Could not add to the inventory'); return; }
    pickedColour = null;
    rememberColour(colour);
    toast(`Added ${t.name} to your inventory (${colourName(colour)})`);
  } catch (e) { showError('Could not add to the inventory', e); } finally { button.disabled = false; }
}

// ---- the document's view -------------------------------------------------------------------------------

const ui = { filters: null, face: 'view', reprint: null, drawn: null };

/** The filters now: the target's system until you pick one, the colour and the shelf as you set them. */
const filters = () => {
  ui.filters ??= { ...defaultFilters(null), system: null };
  return { ...ui.filters, system: ui.filters.system ?? server.get().system };
};

/** The strip under the crumbs. */
export function contextLines() {
  const t = tile();
  const p = t?.plan?.inventory;
  if (t?.status === 'failed') return ['This inventory does not read.', 'Source shows the file; the messages say where'];
  if (!p) return ['Reading the inventory…'];
  const lines = p.pieces.reduce((n, x) => n + x.printed + x.failed + x.gone, 0);
  return [`${plural(p.pieces.filter((x) => x.have > 0).length, 'piece')} on the shelf`, `${plural(lines, 'log line')} · a log: nothing is edited away`];
}

/** The piece's log, as the program adds it up: from the snapshot when this is the home inventory (it follows at once), else the document's. */
function dataOf(t) {
  const snap = server.get().inventory;
  const own = t.plan?.inventory;
  if (snap?.exists && snap.path === t.path && !snap.problems) return { ...snap, text: own?.text ?? '', home: snap.home };
  return { pieces: own?.pieces ?? [], text: own?.text ?? '', problems: own?.problems, home: snap?.home ?? '' };
}

function filterBar(f) {
  const bar = el('div', 'inv-filters');
  const set = (patch) => { Object.assign(ui.filters, patch); ui.drawn = null; render(); };
  ui.filters ??= { ...defaultFilters(null), system: null };
  bar.append(
    seg('System', SYSTEMS.map(([v, n]) => [v, n, `Only ${n} pieces: DungeonBlocks and OpenLOCK never count for each other`]), f.system, (v) => set({ system: v })),
    seg('Colour', [[null, 'Both'], ['single', 'Single'], ['multi', 'Multi']], f.colour, (v) => set({ colour: v })),
    seg('Show', [['all', 'All'], ['have', 'On the shelf'], ['off', 'Not on the shelf']], f.show, (v) => set({ show: v })),
  );
  return bar;
}

async function act(label, run) {
  try {
    const r = await run();
    if (r.status !== 200) { if (!failed(label, r)) say(r, label); return null; }
    return r.body;
  } catch (e) { showError(label, e); return null; }
}

function pieceRow(g, p, system) {
  const row = el('div', `inv-piece${p.have ? '' : ' off'}`);
  const what = el('span', 'inv-what');
  what.append(el('b', null, `seed ${p.seed}`), el('span', 'inv-colour', colourName(p.colour)));
  const nums = el('span', 'inv-nums');
  nums.append(el('span', 'inv-count', `×${p.have}`));
  if (p.failed) nums.append(el('span', 'inv-tag', `${p.failed} failed`));
  if (p.gone) nums.append(el('span', 'inv-tag', `${p.gone} gone`));
  if (p.last) nums.append(el('span', 'inv-tag date', p.last));
  const acts = el('span', 'inv-acts');
  const gone = el('button', 'linkish', 'Gone');
  gone.type = 'button'; gone.disabled = p.have === 0;
  gone.title = p.have ? 'One of these is lost or broken: a line is added to the log, nothing is edited away' : 'None of these is on the shelf';
  gone.onclick = async () => {
    const b = await act('Could not mark it gone', () => host.inventoryGone({ tile: p.tile, seed: p.seed, system: p.system, colour: p.colour, on: today() }));
    if (b) toast(`Marked one ${g.name} gone`);
  };
  const re = el('button', 'linkish', 'Reprint');
  re.type = 'button';
  re.title = 'Add this exact piece (its seed) to a print list';
  const key = `${p.tile}|${p.seed}|${p.system}|${p.colour}`;
  re.onclick = () => { ui.reprint = ui.reprint === key ? null : key; ui.drawn = null; render(); };
  acts.append(gone, re);
  row.append(what, nums, acts);
  if (system && p.system !== system) row.title = systemName(p.system);
  const wrap = el('div', 'inv-wrap');
  wrap.append(row);
  if (ui.reprint === key) wrap.append(reprintRow(p));
  return wrap;
}

/** The small form under a piece: which print list it goes into (or a new one). */
function reprintRow(p) {
  const box = el('div', 'inv-reprint');
  const lists = server.get().tiles.filter((x) => /\.dmap$/i.test(x.path));
  const s = el('select', 'word-select');
  s.setAttribute('aria-label', 'Print list');
  const fresh = el('option', null, 'A new print list'); fresh.value = '';
  s.append(fresh);
  for (const l of lists) { const o = el('option', null, `${l.name}`); o.value = l.id; s.append(o); }
  const go = el('button', 'btn', 'Add');
  go.type = 'button';
  go.onclick = async () => {
    const b = await act('Could not reprint', () => host.inventoryReprint({ tile: p.tile, seed: p.seed, system: p.system, colour: p.colour, list: s.value || null, docRev: s.value ? docs.get(s.value)?.docRev : undefined }));
    if (!b) return;
    ui.reprint = null; ui.drawn = null; render();
    toast(`${b.made ? 'Made' : 'Added it to'} ${b.list.split(/[\\/]/).pop()}${b.warning ? `. ${b.warning}` : ''}`, () => import('./open.js').then(({ select }) => select(b.id).then(() => view.set({ page: 'edit' }))), { label: 'Open', ms: b.warning ? 12000 : 7000 });
  };
  const x = el('button', 'linkish', 'Cancel');
  x.type = 'button'; x.onclick = () => { ui.reprint = null; ui.drawn = null; render(); };
  box.append(el('span', 'pl-lab', `Seed ${p.seed} into`), s, go, x);
  return box;
}

/** A tile that is not at its path any more: the one-click re-link, which adds a `renamed` line. */
function relinkRow(g) {
  const box = el('div', 'inv-relink');
  const cands = relinkCandidates(server.get().tiles, g.tile);
  const same = cands.filter((c) => nameOf(canonOf(c.path, '')) === nameOf(g.tile));
  const link = (id, label) => async () => {
    const b = await act('Could not re-link', () => host.inventoryRelink({ from: g.tile, to: id, on: today() }));
    if (b) toast(`Re-linked ${g.name} to ${label}`);
  };
  box.append(el('span', 'inv-gone', 'Not at this path any more.'));
  if (same.length === 1) {
    const b = el('button', 'btn', `Re-link to ${canonOf(same[0].path, server.get().inventory?.home)}`);
    b.type = 'button'; b.onclick = link(same[0].id, same[0].name);
    box.append(b);
  } else if (cands.length) {
    const s = el('select', 'word-select');
    s.setAttribute('aria-label', 'The tile it is now');
    for (const c of cands) { const o = el('option', null, canonOf(c.path, server.get().inventory?.home)); o.value = c.id; s.append(o); }
    const b = el('button', 'btn', 'Re-link');
    b.type = 'button'; b.onclick = () => link(s.value, s.selectedOptions[0].textContent)();
    box.append(s, b);
  }
  return box;
}

function groupBlock(g, system) {
  const box = el('section', 'inv-group');
  const head = el('div', 'inv-head');
  head.append(el('b', null, g.name), el('span', 'inv-path', g.tile));
  if (g.edited) { const e = el('span', 'inv-tag edited', 'edited since printing'); e.title = 'The tile changed since the print was made. It still counts; this is only to tell you.'; head.append(e); }
  box.append(head);
  if (g.missing) box.append(relinkRow(g));
  for (const p of g.pieces) box.append(pieceRow(g, p, system));
  return box;
}

function sourceView(text) {
  const pre = el('pre', 'inv-source');
  pre.tabIndex = 0;
  for (const [n, line] of numbered(text)) {
    const r = el('div', 'ln');
    r.append(el('span', 'no', String(n)), el('span', 'tx', line || ' '));
    pre.append(r);
  }
  return pre;
}

function render() {
  const box = $('lens-inventory');
  const t = tile();
  const on = isInventory(t);
  document.body.classList.toggle('inventory', on);
  if (!box || !on) return;
  const d = dataOf(t);
  const f = filters();
  const sig = JSON.stringify([t.id, t.status, d.pieces, d.text, d.problems, f, ui.face, ui.reprint, server.get().tiles.filter((x) => /\.dmap$/i.test(x.path)).map((x) => x.id)]);
  if (sig === ui.drawn) return;
  ui.drawn = sig;
  const keep = box.querySelector('.body')?.scrollTop ?? 0;
  box.replaceChildren();
  const head = el('div', 'lt');
  const h = el('div', 'heading');
  h.append(el('span', 'cap', 'Inventory'), el('b', null, t.path.split(/[\\/]/).pop()));
  head.append(h);
  const faces = el('div', 'seg inv-face');
  faces.setAttribute('role', 'group'); faces.setAttribute('aria-label', 'What the panel shows');
  for (const [v, label] of [['view', 'Tile'], ['source', 'Source']]) {
    const b = el('button', null, label);
    b.type = 'button'; b.setAttribute('aria-pressed', String(ui.face === v));
    b.title = v === 'view' ? 'What you have, by tile' : 'The file itself: KDL, one line a print, a loss or a rename';
    b.onclick = () => { ui.face = v; ui.drawn = null; render(); };
    faces.append(b);
  }
  head.append(faces);
  box.append(head);
  const body = el('div', 'body');
  if (d.problems?.length || t.status === 'failed') {
    const n = el('p', 'note bad', 'This inventory does not read, so nothing is counted from it.');
    body.append(n);
    for (const e of (d.problems ?? t.errors ?? []).slice(0, 4)) {
      const p = el('p', 'note bad', e.message);
      if (e.help) p.append(el('span', 'pl-help', ` ${e.help}`));
      body.append(p);
    }
    ui.face = ui.face === 'view' && d.text ? 'source' : ui.face;
  }
  if (ui.face === 'source') {
    body.append(sourceView(d.text), el('p', 'note', 'Edit the file by hand in any editor: this view follows it. Counts are never stored; they are worked out from these lines.'));
  } else if (!d.problems?.length && t.status !== 'failed') {
    body.append(filterBar(f));
    const gs = groups({ pieces: d.pieces }, f);
    body.append(el('p', 'inv-totals', gs.length ? totalsLine(gs) : 'Nothing matches these filters.'));
    for (const g of gs) body.append(groupBlock(g, f.system));
    const others = d.pieces.filter((p) => p.system !== f.system).length;
    if (others) body.append(el('p', 'note', `${plural(others, 'piece line')} in ${systemName(f.system === 'db' ? 'ol' : 'db')} not shown: the other system is a switch away.`));
  }
  box.append(body);
  body.scrollTop = keep;
}

/** Wire this part of the page (once, at load). */
export function init() {
  server.subscribe(['tiles', 'inventory', 'system'], render);
  view.subscribe(['tileId'], () => { ui.drawn = null; ui.reprint = null; render(); });
  render();
}
