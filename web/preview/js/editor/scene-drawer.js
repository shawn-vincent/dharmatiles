// The catalog drawer (docs/maps.html): the folder level with pictures, as a drawer along the bottom of the map.
// The same folders as the panel's, so there is no second place to keep tiles: a card is a tile with its
// picture, its size and what you have of it (the pills of the folder rows), and it either goes on the map
// when pressed (the nearest free place) or is dragged onto the map and dropped where it should stand.

import { levelOf, placesOf, sizeLine, stemOf, whereIs } from '../folders.js';
import { pill } from '../inventory.js';
import { token } from '../tokens.js';
import { showError } from '../problems.js';
import { $ } from './app.js';
import { TILE_MIME } from './scene-drag.js';
import { addTile } from './scene-edit.js';
import { pictureOf } from './scene-thumbs.js';
import { mapMode, sceneNow } from './scene.js';
import { server, view } from './stores.js';

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};

let level = null;          // {root, dir}
let query = '';
let drawn = '';
let seen = null;           // the IntersectionObserver that asks for a card's picture when it is near

const isTile = (t) => /\.dtile$/i.test(t.path);

/** The place and folder the drawer opens at: where the map's first tile lives, else the library. */
function startLevel() {
  const { roots, tiles } = server.get();
  const lib = roots.findIndex((r) => r.role === 'library');
  const first = tiles.find((t) => t.id === sceneNow()?.pieces?.[0]?.tileId);
  const at = first && whereIs(roots, first);
  if (at) return at;
  return { root: lib >= 0 ? lib : 0, dir: '' };
}

function head(lv) {
  const { roots } = server.get();
  const h = el('div', 'dr-head');
  h.append(el('b', null, 'Catalog'));
  const sel = el('select', 'word-select');
  sel.setAttribute('aria-label', 'Place');
  for (const r of placesOf(roots).filter((r) => r.kind !== 'files')) {
    const o = el('option', null, r.label);
    o.value = String(r.index);
    o.selected = r.index === level.root;
    sel.append(o);
  }
  sel.onchange = () => { level = { root: Number(sel.value), dir: '' }; render(true); };
  h.append(sel);
  const crumbs = el('nav', 'dr-crumbs');
  const parts = level.dir ? level.dir.split('/') : [];
  const go = (dir, text) => { const b = el('button', 'crumb', text); b.type = 'button'; b.onclick = () => { level = { root: level.root, dir }; render(true); }; return b; };
  crumbs.append(go('', '/'));
  parts.forEach((p, i) => { crumbs.append(el('span', 'sep', '›'), go(parts.slice(0, i + 1).join('/'), p)); });
  h.append(crumbs);
  const q = el('input');
  q.type = 'search'; q.placeholder = 'Filter tiles'; q.value = query; q.setAttribute('aria-label', 'Filter the catalog');
  q.oninput = () => { query = q.value; render(true, true); };
  h.append(q);
  const x = el('button', 'pane-x', '×');
  x.type = 'button'; x.title = 'Close the catalog'; x.setAttribute('aria-label', 'Close the catalog');
  x.onclick = () => view.set({ drawer: false });
  h.append(x);
  void lv;
  return h;
}

function card(t, sys) {
  const c = el('div', 'dr-card');
  c.draggable = true;
  c.title = `${t.path.replace(/^lib:/, '')}\nPress to put it on the map; drag it to choose where`;
  const pic = el('div', 'dr-pic');
  pic.dataset.id = t.id;
  pic.dataset.key = String(t.meshVersion ?? 0);
  c.append(pic);
  const row = el('div', 'dr-row');
  row.append(el('b', null, stemOf(t.path)), el('span', 'dr-size', sizeLine(t)));
  c.append(row);
  const pl = pill(server.get().inventory, t.path, sys);
  if (pl) { const p = el('span', `ipill ${pl.kind}`, pl.text); p.title = pl.title; c.append(p); }
  c.ondragstart = (e) => { e.dataTransfer.setData(TILE_MIME, t.id); e.dataTransfer.effectAllowed = 'copy'; };
  c.onclick = () => addTile(t);
  return c;
}

function fill(pic) {
  const plate = token('--plate') || '#D9E0EB';
  pictureOf(pic.dataset.id, pic.dataset.key, plate).then((url) => {
    if (!url || !pic.isConnected) return;
    const img = el('img');
    img.alt = ''; img.src = url; img.draggable = false;
    pic.replaceChildren(img);
  }).catch((e) => { pic.classList.add('bad'); showError('Could not draw the picture of a tile', e); });
}

/** Draw the drawer again when what it shows changed. */
export function render(force = false, keepFocus = false) {
  const box = $('drawer');
  const on = view.get().drawer && mapMode();
  box.hidden = !on;
  document.body.classList.toggle('drawer', on);
  if (!on) return;
  const { roots, tiles, folders } = server.get();
  level ??= startLevel();
  const lv = levelOf({ roots, tiles: tiles.filter(isTile), folders, root: level.root, dir: level.dir, query });
  const sys = sceneNow()?.system ?? 'db';
  const sig = JSON.stringify([level, query, lv ? lv.items.map((i) => i.tile.id) : null, lv?.folders.map((f) => f.dir), server.get().inventory?.pieces, sys, token('--plate')]);
  if (!force && sig === drawn) return;
  drawn = sig;
  const q = box.querySelector('input');
  const hadFocus = keepFocus && document.activeElement === q;
  const strip = el('div', 'dr-strip');
  if (!lv) strip.append(el('p', 'note', 'This place is not listed now.'));
  else {
    for (const f of lv.folders) {
      const b = el('button', 'dr-folder');
      b.type = 'button';
      b.append(el('b', null, f.name), el('span', 'dr-size', `${f.tiles} tile${f.tiles === 1 ? '' : 's'}`));
      b.onclick = () => { level = { root: level.root, dir: f.dir }; render(true); };
      strip.append(b);
    }
    for (const it of lv.items) strip.append(card(it.tile, sys));
    if (!lv.items.length && !lv.folders.length) strip.append(el('p', 'note', query ? 'No tile here has that in its name.' : 'No tiles in this folder.'));
  }
  box.replaceChildren(head(lv), strip);
  seen?.disconnect();
  seen = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { seen.unobserve(e.target); fill(e.target); }
  }, { root: strip, rootMargin: '0px 400px 0px 400px' });
  for (const p of strip.querySelectorAll('.dr-pic')) seen.observe(p);
  if (hadFocus) { const i = box.querySelector('input'); i.focus(); i.setSelectionRange(i.value.length, i.value.length); }
}

/** Wire this part of the page (once, at load). */
export function init() {
  const box = el('div');
  box.id = 'drawer';
  box.hidden = true;
  box.setAttribute('role', 'region');
  box.setAttribute('aria-label', 'The catalog');
  $('view').append(box);
  view.subscribe(['drawer', 'dmode', 'tileId'], () => render());
  server.subscribe(['tiles', 'roots', 'inventory'], () => render());
}
