// The folder levels of the panel (docs/preview.html, "Places and folders"): Places at the top (My tiles, folders you
// added, the library, the tests), a place opens to a folder level (its subfolders, then its tiles), a subfolder is
// another level.  `Levels` draws the level the model says and slides between levels (the old level stays as a ghost
// for the length of the slide); `FolderList` is its rows: one button each, a ⋯ beside it for the row's menu, and a
// tile row can be dragged onto a folder row or a folder crumb to move it there.  `ContextLines` and `FolderActions`
// are the strip under the crumbs and the buttons above the list.
//
// The same list (`FolderList`) is the Add step's pane (docs/maps.html) in its add mode (`d.add`): the folders are the same rows, the
// tiles are pictures in a two-column grid (`AddCard`), pressed to put the tile on the map or dragged to where it should
// stand.  There is one walk of the places and folders; the add mode only draws the tiles another way.  Only the browser slides
// between folders here (`Levels`); the Add step's folders are levels of the panel's own shell (editor/levels.js).
//
// These are functions of the model (js/editor/model.js `browse`) and of `on`, what a row or a button does
// (editor/places.js).  Nothing is stored here but how far each level was scrolled and the drag in progress.  What a
// level lists is js/folders.js.

import { signal } from '@preact/signals-core';
import { folderDepth, slideDir } from '../nav.js';
import { slide } from '../slide.js';
import { sizeLine, sizeSquares, stemOf, wherePath } from '../folders.js';
import { thumbBox } from '../rows.js';
import { brokenCode, brokenWords } from '../thumbs.js';
import { html, useEffect, useLayoutEffect, useRef, useState } from './html.js';
import { Icon } from './icons.js';
import { plural } from './widgets.js';

const keyOf = (l) => `${l.root ?? 'places'}|${l.dir || ''}`;

// ---- the pictures ---------------------------------------------------------------------------

/** A glyph for a kind of row, in the text colour: a folder, a house (My tiles), shelves (the library), a flask (the tests), a disk (a folder you added). */
const Glyph = ({ name }) => html`<span class="fico" aria-hidden="true"><${Icon} name=${name} size=${20} /></span>`;

/** A tile's picture: its footprint as squares on the plate, as many across and up as the tile is. */
function SizeThumb({ t }) {
  const sq = sizeSquares(t);
  const [bw, bh] = thumbBox(sq?.[0], sq?.[1], 34);
  const grid = sq && (sq[0] > 1 || sq[1] > 1) && sq[0] <= 8 && sq[1] <= 8;
  const cols = grid ? Array.from({ length: sq[0] - 1 }, (_, i) => (bw * (i + 1)) / sq[0]) : [];
  const rows = grid ? Array.from({ length: sq[1] - 1 }, (_, j) => (bh * (j + 1)) / sq[1]) : [];
  return html`<span class="thumb sz" style=${`width: ${bw}px; height: ${bh}px`} aria-hidden="true" title=${sq ? `${sq[0]}×${sq[1]} squares` : 'size not read yet'}>
    ${grid ? html`<svg viewBox=${`0 0 ${bw} ${bh}`}>${cols.map((x) => html`<line x1=${x} x2=${x} y1="0" y2=${bh} />`)}${rows.map((y) => html`<line y1=${y} y2=${y} x1="0" x2=${bw} />`)}</svg>` : null}</span>`;
}

/**
 * A tile's picture: the tile's own (`on.picture` answers its url) once its card is near the visible part of the list, else the empty
 * plate.  The list scrolls down, so the margin is above and below.
 */
function Pic({ t, plate, on }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  const [url, setUrl] = useState(/** @type {string | null} */ (null));
  const [bad, setBad] = useState(/** @type {string | null} */ (null));
  useEffect(() => {
    const el = box.current;
    if (!el) return undefined;
    let live = true;
    const seen = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      seen.disconnect();
      on.picture(t, plate).then((u) => { if (live && u) setUrl(u); }).catch((e) => { if (live) setBad(brokenWords(e)); on.pictureFailed(e); });
    }, { root: el.closest('.flvl'), rootMargin: '300px 0px 300px 0px' });
    seen.observe(el);
    return () => { live = false; seen.disconnect(); };
  }, [t.id, t.meshVersion, plate]);
  return html`<div class=${`apic${bad ? ' bad' : ''}`} ref=${box} data-id=${t.id} data-key=${String(t.meshVersion ?? 0)} title=${bad ?? undefined}>${url ? html`<img alt="" src=${url} draggable=${false} />` : bad ? html`<span class="broken"><b>${brokenCode(bad)}</b></span>` : null}</div>`;
}

/** A tile of the add mode: its picture, its name and size, and what you have of it.  Pressed it goes on the map; dragged it is let go where it should stand. */
function AddCard({ t, d, on }) {
  const pill = d.pill?.(t);
  return html`<div class="acard" role="button" tabindex="0" draggable=${true} data-id=${t.id} title=${`${stemOf(t.path)}\nPress to put it on the map; drag it to choose where`}
    onDragStart=${(e) => on.dragTile(e, t)} onClick=${() => on.tile(t.id)} onKeyDown=${(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); on.tile(t.id); } }}>
    <${Pic} t=${t} plate=${d.plate} on=${on} />
    <span class="arow"><b>${stemOf(t.path)}</b><span class="asize">${sizeLine(t)}</span></span>
    ${pill ? html`<span class=${`ipill ${pill.kind}`} title=${pill.title}>${pill.text}</span>` : null}</div>`;
}

const dotClass = (t) => {
  const over = t.record && (t.record.audit.overhangMm2 > 0 || t.record.audit.bandMm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
};

// ---- the rows ---------------------------------------------------------------------------------

const Text = ({ label, sub }) => html`<span class="tx"><span class="nm" title=${label}>${label}</span>${sub ? html`<span class="sub">${sub}</span>` : null}</span>`;
const Label = ({ title, hint }) => html`<div class="cap flab"><span>${title}</span>${hint ? html`<small>${hint}</small>` : null}</div>`;

/** A row: its button, and its ⋯ when `menuTitle` says it has a menu.  `drop` makes it a place a tile can be let go. */
function Row({ kind, id, name, menuTitle, on, sel = false, drop = null, cls = '', onOpen, title, children }) {
  return html`<div class=${`orwrap${sel ? ' sel' : ''}`} data-drop-root=${drop ? String(drop.root) : undefined} data-drop-dir=${drop ? drop.dir : undefined}>
    <button type="button" class=${`orow ${cls}`} role="option" data-kind=${kind} data-id=${String(id)} title=${title} onClick=${onOpen}>${children}</button>
    ${menuTitle ? html`<button type="button" class="omore" title=${`${name}: ${menuTitle}`} aria-label=${`Actions for ${name}`} aria-haspopup="menu" onClick=${(e) => { e.stopPropagation(); on.menu?.(kind, String(id), e.currentTarget); }}><${Icon} name="ellipsis" size=${16} /></button>` : null}
  </div>`;
}

function Places({ d, on }) {
  const mine = d.places.filter((r) => r.writable || r.role === 'files');
  const included = d.places.filter((r) => !r.writable && r.role !== 'files');
  const row = (r) => {
    const n = (d.tiles || []).filter((t) => t.root === r.index).length;
    const where = wherePath(r, '', d.home);
    return html`<${Row} key=${`p${r.index}`} kind="place" id=${r.index} name=${r.label} menuTitle=${r.role === 'added' ? 'take it off the list' : null} on=${on} title=${r.path || r.label} onOpen=${() => on.place(r.index)}>
      <${Glyph} name=${r.role === 'home' ? 'home' : r.role === 'library' ? 'library' : r.role === 'tests' ? 'tests' : r.role === 'files' ? 'files' : 'added'} />
      <${Text} label=${r.label} sub=${[plural(n, 'tile'), where].filter(Boolean).join(' · ')} /><span class="chev"><${Icon} name="chevron-right" size=${14} /></span><//>`;
  };
  return html`${mine.length ? html`<${Label} title="Your places" hint="folders on this computer" />${mine.map(row)}` : null}${included.length ? html`<${Label} title="Included" hint="read only" />${included.map(row)}` : null}`;
}

function FolderLevel({ d, on }) {
  const v = d.view;
  const can = d.writable;
  const searching = !!d.query.trim();
  if (!v) return html`<p class="fnote">This place is not there.</p>`;
  if (!v.folders.length && !v.items.length) {
    if (searching) return html`<p class="fnote">${`No tile here matches “${d.query.trim()}”.`}</p>`;
    if (d.add) return html`<p class="fnote">No tiles in this folder.</p>`;
    if (can) return html`<p class="fnote">Nothing here yet. Make a tile or a folder with the buttons above, or save a <code>.dtile</code> here and it shows up.</p>`;
    return html`<p class="fnote">No tiles.</p>`;
  }
  return html`
    ${v.folders.length ? html`<${Label} title="Folders" />${v.folders.map((f) => html`<${Row} key=${`f${f.dir}`} kind="folder" id=${f.dir} name=${f.name} menuTitle=${can ? 'rename, trash' : null} on=${on} drop=${{ root: d.level.root, dir: f.dir }} onOpen=${() => on.folder(f.dir)}>
      <${Glyph} name="folder" /><${Text} label=${f.name} sub=${[plural(f.tiles, 'tile'), f.folders ? plural(f.folders, 'folder') : ''].filter(Boolean).join(' · ')} /><span class="chev"><${Icon} name="chevron-right" size=${14} /></span><//>`)}` : null}
    ${v.items.length && d.add ? html`<${Label} title=${searching ? 'Tiles' : 'Tiles here'} hint=${searching ? `${v.items.length} found` : 'press one to put it on the map'} /><div class="agrid">${v.items.map(({ tile: t }) => html`<${AddCard} key=${`t${t.id}`} t=${t} d=${d} on=${on} />`)}</div>` : null}
    ${v.items.length && !d.add ? html`<${Label} title=${searching ? 'Tiles' : 'Tiles here'} hint=${searching ? `${v.items.length} found` : null} />${v.items.map(({ tile: t, where }) => {
      const sub = [sizeLine(t), searching && where ? where : ''].filter(Boolean).join(' · ');
      const isInventory = /\.dinv$/i.test(t.path);
      const pill = /\.dtile$/i.test(t.path) ? d.pill?.(t) : null;
      return html`<${Row} key=${`t${t.id}`} kind="tile" id=${t.id} name=${stemOf(t.path)} menuTitle="rename, copy, move, trash" on=${on} sel=${t.id === d.openTile} title=${t.path} onOpen=${() => on.tile(t.id)}>
        ${isInventory ? html`<${Glyph} name="library" />` : html`<${SizeThumb} t=${t} />`}<${Text} label=${stemOf(t.path)} sub=${isInventory ? 'what you have printed' : /\.dmap$/i.test(t.path) ? 'print list' : sub} />
        ${pill ? html`<span class=${`ipill ${pill.kind}`} title=${pill.title}>${pill.text}</span>` : null}<span class=${dotClass(t)} title=${t.status}></span><//>`;
    })}` : null}`;
}

// ---- a drag: pull a tile row onto a folder -----------------------------------------------------

const DRAG_PX = 5;
/** True while a tile is being dragged: the list is not drawn again under the pointer. */
const dragging = signal(false);

const targetAt = (x, y) => {
  const hit = document.elementFromPoint(x, y)?.closest?.('[data-drop-root]');
  return hit ? { el: hit, target: { root: Number(hit.dataset.dropRoot), dir: hit.dataset.dropDir || '' } } : null;
};

/** Press a tile row and pull it: the folder that can take it lights up under the pointer, and letting go moves it there. */
function wireDrag(root, getOn) {
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
    dragging.value = false;
    if (!started) return;
    // the click that ends a drag is not an open
    const stop = (e) => { e.stopPropagation(); e.preventDefault(); };
    addEventListener('click', stop, { capture: true, once: true });
    setTimeout(() => removeEventListener('click', stop, true), 0);
    if (go && lit) getOn().drop?.(id, lit.target);
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
      dragging.value = true;
      root.classList.add('dragging');
      g.wrap.classList.add('lift');
      // the row stays where it is, dimmed; its name follows the pointer, so the folder under it stays in sight
      g.chip = document.createElement('div');
      g.chip.className = 'dragchip';
      g.chip.textContent = g.wrap.querySelector('.nm')?.textContent || '';
      document.body.append(g.chip);
      // every place a tile could go is faintly marked, so it is clear what is a target
      for (const t of document.querySelectorAll('[data-drop-root]')) if (getOn().canDrop?.(g.id, { root: Number(t.dataset.dropRoot), dir: t.dataset.dropDir || '' }).ok) t.classList.add('cand');
    }
    g.chip.style.transform = `translate(${e.clientX + 14}px, ${e.clientY + 12}px)`;
    const hit = targetAt(e.clientX, e.clientY);
    light(hit && getOn().canDrop?.(g.id, hit.target).ok ? hit : null);
    const sc = scroller();
    g.auto = 0;
    if (sc) {
      const r = sc.getBoundingClientRect();
      if (e.clientY < r.top + 36) g.auto = -9;
      else if (e.clientY > r.bottom - 36) g.auto = 9;
    }
    if (g.auto) requestAnimationFrame(frame);
  }
  const down = (e) => {
    if (e.button !== 0 || e.pointerType === 'touch' || g) return; // a finger scrolls; its way to move a tile is the menu
    const b = e.target.closest?.('.orow');
    if (!b || b.dataset.kind !== 'tile') return;
    g = { wrap: b.parentElement, id: b.dataset.id, startX: e.clientX, startY: e.clientY, started: false, lit: null, auto: 0 };
    addEventListener('pointermove', onMove, true);
    addEventListener('pointerup', onUp, true);
    addEventListener('pointercancel', onCancel, true);
    addEventListener('keydown', onEsc, true);
  };
  root.addEventListener('pointerdown', down);
  return () => root.removeEventListener('pointerdown', down);
}

/** What a row answers to besides the arrows (which the page moves by): the menu key opens its menu, F2 renames, Delete trashes. */
function onRowKey(e, on) {
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
}

/**
 * The list.  `d`: `{roots, places, level, view, query, openTile, home, writable, pill, tiles}`; `on`: `place(index)`,
 * `folder(dir)`, `tile(id)`, `menu(kind, id, anchor)`, `act(action, kind, id, anchor)`, `canDrop(tileId, target)`,
 * `drop(tileId, target)`.  While a drag is on, the list is the one that was drawn when it began.
 */
export function FolderList({ d, on, ghost = false }) {
  const held = useRef(/** @type {any} */ (null));
  const root = useRef(/** @type {HTMLElement | null} */ (null));
  const onRef = useRef(on);
  onRef.current = on;
  useEffect(() => (ghost || !root.current ? undefined : wireDrag(root.current, () => onRef.current)), []);
  if (!ghost && dragging.value && held.current) return held.current;
  const vnode = html`<div class="flist" ref=${root} onKeyDown=${(e) => onRowKey(e, on)}>${d.level.root == null ? html`<${Places} d=${d} on=${on} />` : html`<${FolderLevel} d=${d} on=${on} />`}</div>`;
  held.current = vnode;
  return vnode;
}

/**
 * The level in the viewport.  A step between levels is the panel's one slide (js/slide.js, which way: nav.js `slideDir` of the folder
 * depths): the level the list was on stays one more moment as a ghost for it to take away.  How far each level was scrolled is
 * remembered, and given back when it is shown again.
 */
export function Levels({ d, on, scrolls, arrived }) {
  const key = keyOf(d.level);
  const last = useRef({ key, depth: folderDepth(d.level), d, ghost: null });
  const cur = useRef(/** @type {HTMLElement | null} */ (null));
  const old = useRef(/** @type {HTMLElement | null} */ (null));
  const [, redraw] = useState(0);
  const moved = last.current.key !== key;
  if (moved) {
    // the level the list was on is the ghost of the slide
    last.current.ghost = { d: last.current.d, dir: slideDir(last.current.depth, folderDepth(d.level)), scroll: scrolls.get(last.current.key) ?? 0 };
    last.current.key = key; last.current.depth = folderDepth(d.level);
  }
  last.current.d = d;
  const ghost = last.current.ghost;
  useLayoutEffect(() => {
    if (cur.current) cur.current.scrollTop = scrolls.get(key) ?? 0;
    if (moved) arrived?.();
    if (!ghost || !moved || !cur.current || !old.current) return;
    old.current.scrollTop = ghost.scroll;
    slide({ scope: cur.current.parentElement, to: cur.current, out: old.current, dir: ghost.dir, leave: () => { if (last.current.ghost === ghost) { last.current.ghost = null; redraw((n) => n + 1); } } });
  }, [key]);
  return html`<div id="folder" class="flvl" role="listbox" aria-label="Places and folders" tabindex="-1" ref=${cur} onScroll=${(e) => scrolls.set(key, e.currentTarget.scrollTop)}><${FolderList} d=${d} on=${on} /></div>
    ${ghost ? html`<div class="flvl ghost" ref=${old} inert=${true}><${FolderList} d=${ghost.d} on=${on} ghost=${true} /></div>` : null}`;
}

export const ContextLines = ({ lines }) => html`${lines.map((l, i) => html`<div class="ln" key=${i}>${l}</div>`)}`;

/** The buttons above the list: what can be added here. */
export const FolderActions = ({ buttons }) => html`${buttons.map((b) => html`<button type="button" class=${`btn ${b.cls || ''}`} key=${b.label} title=${b.title} onClick=${(e) => b.run(e.currentTarget)}>${b.label}</button>`)}`;
