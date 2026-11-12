// The rows of a folder level: the places at the top, then a folder's subfolders and tiles.  Each row
// is one button that opens its level (or its tile); beside it a ⋯ button opens the row's menu, as on
// the tile level (outline.js), and a tile row can be dragged onto a folder row (or a folder crumb)
// to move it there.  A page here (it makes elements), but it reads no store: the caller gives it the
// data, and what a row does is the caller's (editor/places.js).  What a level lists is folders.js.

import { sizeLine, sizeSquares, stemOf, wherePath } from './folders.js';
import { thumbBox } from './rows.js';

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};
const NS = 'http://www.w3.org/2000/svg';
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

// ---- the pictures ---------------------------------------------------------------------------

/** A glyph for a kind of row, in the text colour: a folder, a house (My tiles), shelves (the library), a flask (the tests), a disk (a folder you added). */
const GLYPHS = {
  folder: 'M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2.5h8.5A1.5 1.5 0 0 1 21 10v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z',
  home: 'M4 11 12 4l8 7M6.5 9.5V19h11V9.5M10 19v-5h4v5',
  library: 'M5 5h3.5v14H5zM10.5 5H14v14h-3.5zM15.6 6.4l3.3-.9 2.6 12.6-3.3.9z',
  tests: 'M9.5 4h5M10.5 4v5.5L5.5 18a1.4 1.4 0 0 0 1.2 2h10.6a1.4 1.4 0 0 0 1.2-2l-5-8.5V4M8 15h8',
  added: 'M3.5 9.5 6 5h12l2.5 4.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5zM3.5 9.5h17M16.5 14.5h.01',
  files: 'M7 3.5h7l4 4V20H7zM14 3.5V8h4',
  stl: 'M12 3.5 20 8v8l-8 4.5L4 16V8zM4 8l8 4.5L20 8M12 12.5v8',
};
function glyph(name) {
  const box = el('span', 'fico');
  box.setAttribute('aria-hidden', 'true');
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  const path = document.createElementNS(NS, 'path');
  path.setAttribute('d', GLYPHS[name] || GLYPHS.folder);
  svg.append(path);
  box.append(svg);
  return box;
}

/** A tile's picture: its footprint as squares on the plate, as many across and up as the tile is. */
function sizeThumb(t) {
  const sq = sizeSquares(t);
  const [bw, bh] = thumbBox(sq?.[0], sq?.[1], 34);
  const box = el('span', 'thumb sz');
  box.style.width = `${bw}px`; box.style.height = `${bh}px`;
  box.setAttribute('aria-hidden', 'true');
  if (sq && (sq[0] > 1 || sq[1] > 1) && sq[0] <= 8 && sq[1] <= 8) {
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', `0 0 ${bw} ${bh}`);
    for (let i = 1; i < sq[0]; i++) { const l = document.createElementNS(NS, 'line'); const x = (bw * i) / sq[0]; l.setAttribute('x1', x); l.setAttribute('x2', x); l.setAttribute('y1', 0); l.setAttribute('y2', bh); svg.append(l); }
    for (let j = 1; j < sq[1]; j++) { const l = document.createElementNS(NS, 'line'); const y = (bh * j) / sq[1]; l.setAttribute('y1', y); l.setAttribute('y2', y); l.setAttribute('x1', 0); l.setAttribute('x2', bw); svg.append(l); }
    box.append(svg);
  }
  box.title = sq ? `${sq[0]}×${sq[1]} squares` : 'size not read yet';
  return box;
}

const dotClass = (t) => {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
};

// ---- the rows ---------------------------------------------------------------------------------

/**
 * The list.  `data`: `{roots, places, level, view, query, openTile, home, writable}`: what Places lists (the
 * places in order), or the folder level `view` (folders.js `levelOf`) of place `level.root`; the filter's
 * words; the open tile's id; the home folder (for short paths); whether the place can be changed.  `on`:
 *   `place(index)`, `folder(dir)`, `tile(id)`   a row's level (a tile opens);
 *   `menu(kind, id, anchor)`                    a row's menu (its ⋯, the menu key);
 *   `act(action, kind, id, anchor)`             a row's key: 'rename' (F2) or 'trash' (Delete);
 *   `canDrop(tileId, target)`                   {ok}: may a tile go on `target` ({root, dir});
 *   `drop(tileId, target)`                      a tile let go on a folder.
 * The root element is a listbox of `.orwrap > .orow` rows, and its folder rows carry `data-drop-root` and `data-drop-dir`.
 */
export function buildFolderList(data, on) {
  const root = el('div', 'flist');
  const rowOf = (kind, id, cls) => {
    const b = el('button', `orow ${cls || ''}`);
    b.type = 'button'; b.setAttribute('role', 'option');
    b.dataset.kind = kind; b.dataset.id = String(id);
    return b;
  };
  const wrap = (b, name, menuTitle) => {
    const w = el('div', 'orwrap');
    w.append(b);
    if (menuTitle) {
      const m = el('button', 'omore', '⋯');
      m.type = 'button';
      m.title = `${name}: ${menuTitle}`;
      m.setAttribute('aria-label', `Actions for ${name}`);
      m.setAttribute('aria-haspopup', 'menu');
      m.onclick = (e) => { e.stopPropagation(); on.menu?.(b.dataset.kind, b.dataset.id, m); };
      w.append(m);
    }
    root.append(w);
    return w;
  };
  const text = (label, sub) => {
    const t = el('span', 'tx');
    const nm = el('span', 'nm', label);
    nm.title = label;
    t.append(nm);
    if (sub) t.append(el('span', 'sub', sub));
    return t;
  };
  const label = (title, hint) => {
    const h = el('div', 'flab');
    h.append(el('span', '', title));
    if (hint) h.append(el('small', '', hint));
    root.append(h);
  };
  const note = (html) => { const p = el('p', 'fnote'); p.append(...html); root.append(p); };

  if (data.level.root == null) places(data, on, { rowOf, wrap, text, label });
  else folderLevel(data, on, { rowOf, wrap, text, label, note });
  wireKeys(root, on);
  wireDrag(root, on);
  return root;
}

function places(data, on, { rowOf, wrap, text, label }) {
  const mine = data.places.filter((r) => r.writable || r.role === 'files');
  const included = data.places.filter((r) => !r.writable && r.role !== 'files');
  const row = (r) => {
    const b = rowOf('place', r.index);
    const n = (data.tiles || []).filter((t) => t.root === r.index).length;
    const where = wherePath(r, '', data.home);
    b.append(glyph(r.role === 'home' ? 'home' : r.role === 'library' ? 'library' : r.role === 'tests' ? 'tests' : r.role === 'files' ? 'files' : 'added'),
      text(r.label, [plural(n, 'tile'), where].filter(Boolean).join(' · ')), el('span', 'chev', '›'));
    b.onclick = () => on.place(r.index);
    b.title = r.path || r.label;
    wrap(b, r.label, r.role === 'added' ? 'take it off the list' : null);
  };
  if (mine.length) { label('Your places', 'folders on this computer'); mine.forEach(row); }
  if (included.length) { label('Included', 'read only'); included.forEach(row); }
}

function folderLevel(data, on, { rowOf, wrap, text, label, note }) {
  const v = data.view;
  const can = data.writable;
  const searching = !!data.query.trim();
  if (!v) { note([document.createTextNode('This place is not there.')]); return; }
  if (!v.folders.length && !v.items.length) {
    if (searching) note([document.createTextNode(`No tile here matches “${data.query.trim()}”.`)]);
    else if (can) note([document.createTextNode('Nothing here yet. Make a tile or a folder with the buttons above, or save a '), el('code', '', '.dtile'), document.createTextNode(' here and it shows up.')]);
    else note([document.createTextNode('No tiles.')]);
    return;
  }
  if (v.folders.length) {
    label('Folders');
    for (const f of v.folders) {
      const b = rowOf('folder', f.dir);
      b.append(glyph('folder'), text(f.name, [plural(f.tiles, 'tile'), f.folders ? plural(f.folders, 'folder') : ''].filter(Boolean).join(' · ')), el('span', 'chev', '›'));
      b.onclick = () => on.folder(f.dir);
      const w = wrap(b, f.name, can ? 'rename, trash' : null);
      w.dataset.dropRoot = String(data.level.root); w.dataset.dropDir = f.dir;
    }
  }
  if (v.items.length) {
    label(searching ? 'Tiles' : 'Tiles here', searching ? `${v.items.length} found` : null);
    for (const { tile: t, where } of v.items) {
      const b = rowOf('tile', t.id);
      const sub = [sizeLine(t), searching && where ? where : ''].filter(Boolean).join(' · ');
      const isInventory = /\.dinv$/i.test(t.path);
      b.append(isInventory ? glyph('library') : sizeThumb(t), text(stemOf(t.path), isInventory ? 'what you have printed' : /\.dmap$/i.test(t.path) ? 'print list' : sub));
      // have and need: what the inventory says of this tile for the target system (data.pill, from js/inventory.js)
      const pill = /\.dtile$/i.test(t.path) ? data.pill?.(t) : null;
      if (pill) { const pe = el('span', `ipill ${pill.kind}`, pill.text); pe.title = pill.title; b.append(pe); }
      const d = el('span', dotClass(t));
      d.title = t.status;
      b.append(d);
      b.onclick = () => on.tile(t.id);
      b.title = t.path;
      const w = wrap(b, stemOf(t.path), 'rename, copy, move, trash');
      if (t.id === data.openTile) w.classList.add('sel');
    }
  }
}

// ---- keys ---------------------------------------------------------------------------------------

/** What a row answers to besides the arrows (which the page moves by): the menu key opens its menu, F2 renames, Delete trashes. */
function wireKeys(root, on) {
  root.addEventListener('keydown', (e) => {
    const b = e.target.closest?.('.orow');
    if (!b) return;
    const kind = b.dataset.kind, id = b.dataset.id;
    const more = b.parentElement.querySelector('.omore') || b;
    if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault(); e.stopPropagation();
      if (b.parentElement.querySelector('.omore')) on.menu?.(kind, id, more);
    } else if (e.key === 'F2' && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopPropagation();
      on.act?.('rename', kind, id, more);
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopPropagation();
      on.act?.('trash', kind, id, more);
    }
  });
}

// ---- dragging a tile onto a folder ---------------------------------------------------------------

const DRAG_PX = 5;

/** The folder under a point that `dragTile` could be let go on: a folder row, or a crumb that names a folder. */
function targetAt(x, y) {
  const hit = document.elementFromPoint(x, y)?.closest?.('[data-drop-root]');
  if (!hit) return null;
  return { el: hit, target: { root: Number(hit.dataset.dropRoot), dir: hit.dataset.dropDir || '' } };
}

/** Press a tile row and pull it: the folder that can take it lights up under the pointer, and letting go moves it there. */
function wireDrag(root, on) {
  let g = null;
  const scroller = () => root.closest('.flvl');
  const light = (hit) => {
    if (g.lit?.el === hit?.el) return;
    g.lit?.el.classList.remove('drop');
    g.lit = hit;
    hit?.el.classList.add('drop');
  };
  const end = (go) => {
    if (!g) return;
    const { wrap, started, lit, id, chip } = g;
    g = null;
    chip?.remove();
    removeEventListener('pointermove', onMove, true);
    removeEventListener('pointerup', onUp, true);
    removeEventListener('pointercancel', onCancel, true);
    removeEventListener('keydown', onEsc, true);
    wrap.classList.remove('lift');
    root.classList.remove('dragging');
    for (const c of document.querySelectorAll('.cand, .drop')) c.classList.remove('cand', 'drop');
    if (!started) return;
    // the click that ends a drag is not an open
    const stop = (e) => { e.stopPropagation(); e.preventDefault(); };
    addEventListener('click', stop, { capture: true, once: true });
    setTimeout(() => removeEventListener('click', stop, true), 0);
    if (go && lit) on.drop?.(id, lit.target);
  };
  const onUp = () => end(true);
  const onCancel = () => end(false);
  const onEsc = (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); end(false); } };
  const frame = () => {
    const sc = scroller();
    if (g?.started && sc && g.auto) { sc.scrollTop += g.auto; requestAnimationFrame(frame); }
  };
  function onMove(e) {
    if (!g) return;
    if (!g.started) {
      if (Math.abs(e.clientY - g.startY) + Math.abs(e.clientX - g.startX) < DRAG_PX) return;
      g.started = true;
      root.classList.add('dragging');
      g.wrap.classList.add('lift');
      // the row stays where it is, dimmed; its name follows the pointer, so the folder under it stays in sight
      g.chip = el('div', 'dragchip', g.wrap.querySelector('.nm')?.textContent || '');
      document.body.append(g.chip);
      // every place a tile could go is faintly marked, so it is clear what is a target
      const targets = [...document.querySelectorAll('[data-drop-root]')];
      for (const t of targets) if (on.canDrop?.(g.id, { root: Number(t.dataset.dropRoot), dir: t.dataset.dropDir || '' }).ok) t.classList.add('cand');
    }
    g.chip.style.transform = `translate(${e.clientX + 14}px, ${e.clientY + 12}px)`;
    const hit = targetAt(e.clientX, e.clientY);
    light(hit && on.canDrop?.(g.id, hit.target).ok ? hit : null);
    const sc = scroller();
    g.auto = 0;
    if (sc) {
      const r = sc.getBoundingClientRect();
      if (e.clientY < r.top + 36) g.auto = -9;
      else if (e.clientY > r.bottom - 36) g.auto = 9;
    }
    if (g.auto) requestAnimationFrame(frame);
  }
  root.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.pointerType === 'touch' || g) return; // a finger scrolls; its way to move a tile is the menu
    const b = e.target.closest?.('.orow');
    if (!b || b.dataset.kind !== 'tile') return;
    g = { wrap: b.parentElement, id: b.dataset.id, startX: e.clientX, startY: e.clientY, started: false, lit: null, auto: 0 };
    addEventListener('pointermove', onMove, true);
    addEventListener('pointerup', onUp, true);
    addEventListener('pointercancel', onCancel, true);
    addEventListener('keydown', onEsc, true);
  });
}

/** The rows of a folder list, in order. */
export const rows = (root) => [...root.querySelectorAll('.orow')];
