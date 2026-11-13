// The map's panel (docs/maps.html): the levels of a map, one at a time, as the tile's are: the map (its pieces
// by tile, what the shelf says of them, the variants and the way to the plates), one piece (its place, turn,
// level, seed and print setting, with Open tile), and a property of it (the level's names, the seeds you
// own).  Where you are is the selection (`view.msel`) and the property open on it (`view.mprop`), so a click
// on a piece in the view brings the panel to it and Back climbs.  Nothing here changes a document itself:
// each button is one of scene-edit.js's changes.

import * as M from '../map-model.js';
import { exists, tileHave, haveLine, systemName } from '../inventory.js';
import { $, toast } from './app.js';
import { seg } from './controls.js';
import * as E from './scene-edit.js';
import { chooseMode, mapMode, pieces, sceneNow, select, selected } from './scene.js';
import { docs, planOf } from './docs.js';
import { isPlates, server, stem, tile, view } from './stores.js';

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const button = (text, run, { title, cls = 'btn', disabled = false } = {}) => {
  const b = el('button', cls, text);
  b.type = 'button';
  if (title) b.title = title;
  b.disabled = disabled;
  b.onclick = run;
  return b;
};

/** The level the panel is on: 'map', 'piece' or 'prop'. */
export const levelOf = () => (view.get().mprop && selected().length === 1 ? 'prop' : selected().length ? 'piece' : 'map');

/** The strip under the crumbs. */
export function contextLines() {
  const sc = sceneNow();
  const ps = pieces();
  const placed = ps.filter((p) => p.at).length;
  const tray = ps.length - placed;
  return [
    `${plural(placed, 'piece')} on the map${tray ? ` · ${tray} in the tray` : ''} · ${M.totalsText(ps, server.get().inventory)}`,
    sc ? `${systemName(sc.system)} · ${sc.printer}${sc.grid ? ` · ${sc.grid.cols} × ${sc.grid.rows} squares` : ''}` : 'Reading the map…',
  ];
}

// ---- pills ----------------------------------------------------------------------------------------------

/** The have or to-print pill of a piece (caution yellow where only the other colour mode is on the shelf). */
export function pill(state, extra = '') {
  const word = { have: 'have', need: 'to print', other: 'have, other colour', skip: 'left off' }[state] ?? state;
  const p = el('span', `ipill ${state === 'have' ? 'have' : state === 'other' ? 'caution' : 'need'}`, word);
  p.title = {
    have: 'A print you own is this piece (tile, seed, system and colour mode): it is left off the plates',
    need: 'It prints',
    other: 'You own this tile and seed in the other colour mode; it prints unless you leave it off',
    skip: 'print=skip: it stays on the map and off the plates',
  }[state] + extra;
  return p;
}

// ---- the levels ---------------------------------------------------------------------------------------------

function head(t, crumbs) {
  const lt = el('div', 'lt');
  const h = el('div', 'heading');
  h.append(el('span', 'cap', 'Map'));
  const trail = el('nav', 'mp-crumbs');
  crumbs.forEach((c, i) => {
    if (i) trail.append(el('span', 'sep', '›'));
    const b = el('button', `crumb${i === crumbs.length - 1 ? ' cur' : ''}`, c.label);
    b.type = 'button';
    if (c.go) b.onclick = c.go;
    trail.append(b);
  });
  h.append(trail);
  lt.append(h);
  void t;
  return lt;
}

const section = (title) => {
  const s = el('section', 'pl-sec');
  if (title) s.append(el('h3', null, title));
  return s;
};

/** The groups of the map's pieces by tile: [{canon, name, pieces}], in the order the tiles first appear. */
function groups(ps) {
  const by = new Map();
  for (const p of ps) {
    if (!by.has(p.canon)) by.set(p.canon, { canon: p.canon, name: p.name, pieces: [] });
    by.get(p.canon).pieces.push(p);
  }
  return [...by.values()];
}

function mapLevel(body, ro) {
  const ps = pieces();
  const inv = server.get().inventory;
  const states = M.pills(ps, inv);
  const sum = section('On the map');
  const line = el('p', 'inv-sum');
  const { have, toPrint } = M.totals(ps, inv);
  line.append(el('b', null, exists(inv) ? `${have} in inventory` : 'No inventory yet'), document.createTextNode(` · ${toPrint} to print`));
  sum.append(line);
  const row = el('div', 'mp-row');
  row.append(
    button('Add tiles', () => view.set({ drawer: !view.get().drawer }), { title: 'The catalog: drag a tile onto the map, or press one to put it down', disabled: ro }),
    button('Print what’s missing', () => chooseMode('plates'), { cls: 'btn on', title: 'The plates for this map: only the pieces your inventory lacks' }),
  );
  sum.append(row);
  body.append(sum);

  const placed = ps.filter((p) => p.at);
  const tray = ps.filter((p) => !p.at);
  const list = section(placed.length ? 'Pieces' : 'No pieces yet');
  if (!placed.length) list.append(el('p', 'note', 'Open the catalog and put tiles on the map. Each is a piece with a place, a turn and a seed; nothing prints until you ask for the plates.'));
  for (const g of groups(placed)) {
    const r = el('button', 'mp-group');
    r.type = 'button';
    const have = g.pieces.filter((p) => states.get(p.id)?.state === 'have').length;
    r.append(el('b', null, g.name), el('span', 'mp-n', `× ${g.pieces.length}`));
    const pills = el('span', 'mp-pills');
    if (have) pills.append(el('span', 'ipill have', `${have} have`));
    if (g.pieces.length - have) pills.append(el('span', 'ipill need', `${g.pieces.length - have} to print`));
    r.append(pills);
    r.title = `Select the ${plural(g.pieces.length, 'piece')} of ${g.name}`;
    r.onclick = () => select(g.pieces.map((p) => p.id));
    list.append(r);
  }
  body.append(list);

  if (tray.length) {
    const t = section('In the tray');
    t.append(el('p', 'note', 'Pieces with no place yet: they print, and wait to be put on the map.'));
    for (const p of tray) {
      const r = el('div', 'mp-tray');
      r.append(el('b', null, p.name), el('span', 'mp-n', p.count > 1 ? `× ${p.count}` : ''));
      r.append(button('Place', () => E.place(p.id), { cls: 'btn', disabled: ro || !!p.error, title: p.count > 1 ? 'Put one copy on the map at the nearest free place, with a seed of its own' : 'Put it on the map at the nearest free place' }));
      if (p.count > 1) r.append(button('Place all', () => E.place(p.id, true), { cls: 'btn', disabled: ro || !!p.error, title: `Put all ${p.count} on the map, each with a seed of its own` }));
      t.append(r);
    }
    body.append(t);
  }

  const v = section('Variants');
  v.append(el('p', 'note', 'A new variant is the same tile with a new seed: a new print. New pieces already get a seed you do not own.'));
  const vr = el('div', 'mp-row wrap');
  vr.append(
    button('New seed for every piece I don’t have', () => E.variants('missing'), { disabled: ro || !placed.length, title: 'Each piece not on your shelf gets a fresh variant; what you own stays as it is' }),
    button('Whole board', () => E.variants('all'), { disabled: ro || !placed.length, title: 'A new variant of every piece' }),
  );
  v.append(vr);
  body.append(v);
}

function pieceLevel(body, ro) {
  const sel = selected();
  const inv = server.get().inventory;
  const states = M.pills(pieces(), inv);
  const p = sel[0];
  const many = sel.length > 1;
  const sc = sceneNow();
  const top = section(null);
  const title = el('div', 'mp-title');
  title.append(el('b', null, many ? `${sel.length} pieces` : p.name));
  if (!many) {
    if (p.id) title.append(el('span', 'mp-id', p.id));
    title.append(pill(states.get(p.id)?.state ?? 'need'));
  }
  top.append(title);
  if (!many) top.append(el('p', 'note', `${p.cols}×${p.rows} · ${systemName(p.system)} · seed ${p.seedUsed}${p.seed == null ? ' (the tile’s own)' : ''}`));
  if (!many && p.error) top.append(el('p', 'note bad', p.error));
  if (!many && p.pinned) top.append(el('p', 'note', 'Pinned on a plate: the packer keeps it where it was dropped.'));
  body.append(top);

  const props = section('Piece');
  if (!many) {
    const at = el('div', 'pl-field');
    at.append(el('span', 'pl-lab', 'Place'), el('span', 'mp-val', p.at ? M.atWord(p.at[0] / p.sq, p.at[1] / p.sq) : 'in the tray'));
    props.append(at);
  }
  const turnRow = el('div', 'pl-field');
  turnRow.append(el('span', 'pl-lab', 'Turn'));
  const tb = el('span', 'mp-btns');
  tb.append(
    button('⟲', () => E.turn(sel.map((q) => q.id), -1), { cls: 'pl-step', title: 'A quarter turn counter-clockwise (⇧R)', disabled: ro }),
    button('⟳', () => E.turn(sel.map((q) => q.id), 1), { cls: 'pl-step', title: 'A quarter turn clockwise (R)', disabled: ro }),
  );
  if (!many) tb.append(el('span', 'mp-val', `${p.rot}°`));
  turnRow.append(tb);
  props.append(turnRow);
  if (!many) {
    const lv = el('button', 'mp-prop');
    lv.type = 'button';
    lv.append(el('span', 'pl-lab', 'Bottom'), el('span', 'mp-val', M.bottomWords(p, sc?.heights)), el('span', 'mp-go', '›'));
    lv.title = 'How high the tile’s zero (its ground level) stands above the map’s ground: a name from the standard heights, or a length. The connector hangs below it.';
    lv.onclick = () => view.set({ mprop: 'bottom' });
    props.append(lv);
    const sd = el('button', 'mp-prop');
    sd.type = 'button';
    sd.append(el('span', 'pl-lab', 'Seed'), el('span', 'mp-val', String(p.seedUsed)), el('span', 'mp-go', '›'));
    sd.title = 'Which variant of the tile: a new one, or one you already have';
    sd.onclick = () => view.set({ mprop: 'seed' });
    props.append(sd);
    props.append(seg('Print', [['missing', 'If missing', 'Print it unless your inventory has it'], ['always', 'Always', 'Print it whatever you have'], ['skip', 'Skip', 'Leave it off the plates']], p.print, (v) => E.setPiece(p.id, { print: v }, `${p.name}: print ${v === 'missing' ? 'if missing' : v}`), ro));
  }
  body.append(props);

  const vars = section('Variants');
  const vr = el('div', 'mp-row wrap');
  vr.append(
    button('New variant', () => E.variants('piece'), { disabled: ro, title: 'A new seed for the selected piece' }),
    button('All of this tile', () => E.variants('tile'), { disabled: ro, title: 'A new seed for every piece of this tile, each its own' }),
    button('Whole board', () => E.variants('all'), { disabled: ro }),
  );
  vars.append(vr);
  body.append(vars);

  const acts = section(null);
  const ar = el('div', 'mp-row wrap');
  ar.append(
    button('Duplicate', () => E.duplicate(sel.map((q) => q.id)), { disabled: ro, title: 'A copy next to it, with the same seed (D)' }),
    button('Remove', () => E.remove(sel.map((q) => q.id)), { cls: 'btn danger', disabled: ro, title: 'Take it off the map (Backspace)' }),
  );
  if (!many) ar.prepend(button('Open tile', () => E.openTile(p.id), { cls: 'btn on', disabled: !p.tileId, title: 'The tile itself, in the tile editor: the map is one click back' }));
  acts.append(ar);
  body.append(acts);
}

function propLevel(body, ro) {
  const p = selected()[0];
  const sc = sceneNow();
  const kind = view.get().mprop;
  const s = section(kind === 'bottom' ? 'Bottom' : kind === 'seed' ? 'Seed' : 'Print');
  if (kind === 'bottom') {
    s.append(el('p', 'note', 'How high the tile’s zero (its ground level) stands above the map’s ground. At zero the connector is under the ground; a raised piece hangs its connector in the air, and a riser under it comes later.'));
    const choose = (name, text, sub) => {
      const on = (p.bottom ?? null) === name;
      const b = el('button', `mp-choice${on ? ' on' : ''}`);
      b.type = 'button';
      b.append(el('b', null, text), el('span', 'mp-sub', sub));
      b.setAttribute('aria-pressed', String(on));
      b.disabled = ro;
      b.onclick = () => E.setPiece(p.id, { bottom: name }, `${p.name}: ${text}`);
      s.append(b);
    };
    choose(null, 'Ground level', 'zero');
    for (const n of (sc?.heights ?? []).filter((h) => /Height$/.test(h.name))) choose(`$${n.name}`, `$${n.name}`, `${Number(n.dmm.toFixed(2))} dmm`);
    const own = el('div', 'pl-field');
    own.append(el('span', 'pl-lab', 'A length'));
    const i = el('input');
    i.type = 'text'; i.placeholder = '20dmm'; i.spellcheck = false; i.value = p.bottom && !p.bottom.startsWith('$') ? p.bottom : ''; i.disabled = ro;
    i.setAttribute('aria-label', 'A bottom as a length with units');
    i.onkeydown = (e) => { if (e.key === 'Enter' && i.value.trim()) E.setPiece(p.id, { bottom: i.value.trim() }, `${p.name}: ${i.value.trim()}`); };
    own.append(i);
    s.append(own);
  } else if (kind === 'seed') {
    s.append(el('p', 'note', 'A seed is a variant of the tile. Pick one you own to make this piece the same as one you have; otherwise it is a new print.'));
    const mine = M.ownedSeeds(server.get().inventory, p);
    const chips = el('div', 'mp-chips');
    for (const o of mine) {
      const c = el('button', `mp-chip${o.seed === p.seedUsed ? ' on' : ''}`, `${o.seed}${o.have > 1 ? ` ×${o.have}` : ''}`);
      c.type = 'button';
      c.title = `Same as the ${o.have} you have: this piece is on your shelf`;
      c.setAttribute('aria-pressed', String(o.seed === p.seedUsed));
      c.disabled = ro;
      c.onclick = () => E.setPiece(p.id, { seed: o.seed }, `${p.name}: seed ${o.seed}, same as one you have`);
      chips.append(c);
    }
    if (mine.length) {
      const h = server.get().inventory ? tileHave(server.get().inventory, p.canon, p.system) : null;
      s.append(el('p', 'inv-have', h ? haveLine(h) : ''), chips);
    } else s.append(el('p', 'note', 'You own none of this tile in this system and colour mode.'));
    const row = el('div', 'pl-field');
    row.append(el('span', 'pl-lab', 'Seed'));
    const i = el('input');
    i.type = 'number'; i.min = '0'; i.step = '1'; i.value = String(p.seedUsed); i.disabled = ro;
    i.setAttribute('aria-label', 'Seed');
    i.onkeydown = (e) => { if (e.key === 'Enter' && i.value !== '') E.setPiece(p.id, { seed: Number(i.value) }, `${p.name}: seed ${i.value}`); };
    row.append(i);
    s.append(row);
    const go = el('div', 'mp-row wrap');
    go.append(
      button('New variant', () => E.variants('piece'), { disabled: ro }),
      button('The tile’s own seed', () => E.setPiece(p.id, { seed: null }, `${p.name}: the tile’s own seed`), { disabled: ro || p.seed == null }),
    );
    s.append(go);
  }
  body.append(s);
}

// ---- drawing -----------------------------------------------------------------------------------------------------

let drawn = '';

function crumbsFor(level, t) {
  const name = stem(t).replace(/\.dmap$/i, '');
  const sel = selected();
  const out = [{ label: name, go: () => { view.set({ mprop: null }); select([]); } }];
  if (level !== 'map') {
    const p = sel[0];
    out.push({ label: sel.length > 1 ? `${sel.length} pieces` : (p.id ? `${p.name} · ${p.id}` : p.name), go: () => view.set({ mprop: null }) });
  }
  if (level === 'prop') out.push({ label: { bottom: 'Bottom', seed: 'Seed', print: 'Print' }[view.get().mprop] ?? 'Property' });
  return out;
}

/** Draw the panel again when what it shows changed (a tick that changes nothing draws nothing). */
export function render() {
  const box = $('lens-map');
  const t = tile();
  if (!box || !isPlates(t) || !mapMode()) return;
  const ps = pieces(), scene = sceneNow();
  const level = levelOf();
  const sig = JSON.stringify([t.id, level, view.get().msel, view.get().mprop, ps, scene?.heights?.length, server.get().inventory?.pieces, server.get().inventory?.exists, planOf(t)?.writable]);
  if (sig === drawn) return;
  drawn = sig;
  const keep = box.querySelector('.body')?.scrollTop ?? 0;
  box.replaceChildren();
  const ro = !(planOf(t)?.writable ?? true) || !!t.path.startsWith('lib:');
  box.append(head(t, crumbsFor(level, t)));
  const body = el('div', 'body');
  if (!scene) body.append(el('div', 'empty', t.status === 'failed' ? 'This map does not read: mend it in Source.' : 'Reading the map…'));
  else {
    if (ro) body.append(el('p', 'note', 'This file cannot be changed here.'));
    if (level === 'map') mapLevel(body, ro);
    else if (level === 'piece') pieceLevel(body, ro);
    else propLevel(body, ro);
  }
  box.append(body);
  body.scrollTop = keep;
}

/** Esc and Back: one level up. */
export function up() {
  if (view.get().mprop) { view.set({ mprop: null }); return true; }
  if (view.get().msel.length) { select([]); return true; }
  return false;
}

/** Wire this part of the page (once, at load). */
export function init() {
  // Esc is one level up: a property to its piece, a piece to the map (before the page's own Esc, which leaves the tile)
  addEventListener('keydown', (e) => {
    const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';
    if (e.key === 'Escape' && !typing && mapMode() && up()) { e.preventDefault(); e.stopImmediatePropagation(); }
  }, true);
  docs.subscribe(render);
  view.subscribe(['msel', 'mprop', 'dmode', 'tileId'], () => { drawn = ''; render(); });
  server.subscribe(['tiles', 'inventory'], render);
  render();
  void toast;
}
