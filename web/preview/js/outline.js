// What the tile level holds, as rows: the stack in the order it builds and the lines
// and regions that shape the tile.  Each row is one button that opens its thing's level;
// beside it a ⋯ button opens the row's menu (rename, hide, delete, move, add), the stack
// ends in an Add row and the Shape ends in Add line and Add region.  A row of the stack
// can be dragged by the mouse to a new place, and ⌥↑ / ⌥↓ move the row the keyboard is on.
// A region wears a picture of its mask, a line a glyph of its points.  A page here (it makes
// elements), but it reads no store: the caller gives it the data, and what a row does is
// the caller's.  The rules of the rows are in rows.js.

import { badgeOf, placeOf } from './nav.js';
import { clauseTitle } from './editor/region-sheet.js';
import { coverage, idOf, linePoints, maskThumb, slotAt, thumbBox } from './rows.js';

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** What a region says of itself in a few words: what bounds it, or its clauses. */
export function regionSummary(m) {
  const sh = m.sheet;
  if (!sh) return 'region';
  const parts = [];
  if (sh.bounded_by?.text) parts.push(`bounded by ${sh.bounded_by.text}`);
  for (const c of sh.clauses || []) parts.push(clauseTitle(c));
  return parts.join('; ') || 'region';
}

/** What a line says of itself: its points. */
export const lineSummary = (m) => (m.points?.length ? plural(m.points.length, 'point') : 'line');

// ---- the pictures a Shape row wears --------------------------------------------------------

const urls = new WeakMap(); // region -> the data URL of its picture: made once for each check's map

/** The region's mask as a small picture: the build's own cells, drawn in the region's colour through the picture as a mask. */
function regionThumb(r, cols, rows) {
  const [bw, bh] = thumbBox(cols, rows);
  const box = el('span', 'thumb region');
  box.style.width = `${bw}px`; box.style.height = `${bh}px`;
  box.setAttribute('aria-hidden', 'true');
  if (!coverage(r)) { box.classList.add('empty'); return box; }
  let url = urls.get(r);
  if (!url) {
    const t = maskThumb(r, 72);
    const c = document.createElement('canvas');
    c.width = t.w; c.height = t.h;
    const ctx = c.getContext('2d');
    const img = ctx.createImageData(t.w, t.h);
    for (let i = 0; i < t.a.length; i++) img.data[i * 4 + 3] = t.a[i]; // black, as opaque as the cell is covered
    ctx.putImageData(img, 0, 0);
    url = c.toDataURL('image/png');
    urls.set(r, url);
  }
  const fill = el('i');
  fill.style.webkitMaskImage = fill.style.maskImage = `url(${url})`;
  box.append(fill);
  box.title = `covers ${Math.round(coverage(r) * 100)}% of the tile`;
  return box;
}

/** The line's points as a tiny polyline in a box the tile's shape. */
function lineThumb(m, cols, rows) {
  const [bw, bh] = thumbBox(cols, rows);
  const box = el('span', 'thumb line');
  box.style.width = `${bw}px`; box.style.height = `${bh}px`;
  box.setAttribute('aria-hidden', 'true');
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${bw} ${bh}`);
  const pts = linePoints(m.pts, cols, rows, bw, bh);
  if (pts.length > 1) {
    const p = document.createElementNS(NS, 'polyline');
    p.setAttribute('points', pts.map((q) => q.join(',')).join(' '));
    svg.append(p);
  }
  box.append(svg);
  return box;
}

// ---- the rows ---------------------------------------------------------------------------

/**
 * The rows.  `data`: `{lines, regions, stack, badges, cols, rows, editable}` (the map's lines and regions; the
 * stack items in written order; the messages by entry key; the tile's size in squares; whether the document can
 * be changed from here).  `on`:
 *   `open(kind, id)`     a row's level;
 *   `hover(w | null)`    light it in the view;
 *   `menu(kind, id, anchor)`  a row's menu (its ⋯, the menu key);
 *   `act(action, kind, id, anchor)`  a row's key: 'rename' (F2) or 'delete' (Delete);
 *   `add(kind, anchor)`  the Add row of the stack ('entry'), or of the Shape ('line', 'region');
 *   `move(from, slot)`   a stack row dragged (`slot`: the list position it goes before);
 *   `step(from, dir)`    ⌥↑ / ⌥↓ on a stack row: one place up (-1) or down (+1).
 * Returns the root element; arrow keys move between rows (`rows()` lists them).
 */
export function buildOutline(data, on) {
  const { open, hover } = on;
  const root = el('div', 'outline');
  root.setAttribute('role', 'listbox');
  root.setAttribute('aria-label', 'What the tile holds');
  const row = (kind, id, cls) => {
    const b = el('button', `orow ${cls || ''}`);
    b.type = 'button'; b.setAttribute('role', 'option');
    b.dataset.kind = kind; b.dataset.id = id;
    b.onclick = () => open(kind, id);
    b.onmouseenter = () => hover?.({ kind, id });
    b.onmouseleave = () => hover?.(null);
    return b;
  };
  /** A row and its ⋯ button, side by side (a button cannot hold a button). */
  const wrap = (b, name) => {
    const w = el('div', 'orwrap');
    w.append(b);
    if (data.editable) {
      const m = el('button', 'omore', '⋯');
      m.type = 'button';
      m.title = `${name}: rename, hide, delete, move, add`;
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
    t.append(el('span', 'nm', label));
    if (sub) t.append(el('span', 'sub', sub));
    return t;
  };
  const section = (title, hint) => {
    const h = el('div', 'olab');
    h.append(el('span', '', title));
    if (hint) h.append(el('small', '', hint));
    root.append(h);
  };
  const addRow = (kind, label, hint) => {
    const b = el('button', 'orow oadd');
    b.type = 'button';
    b.dataset.kind = 'add'; b.dataset.id = kind;
    b.setAttribute('aria-haspopup', kind === 'entry' ? 'menu' : 'false');
    b.append(el('span', 'plus', '+'), text(label, hint));
    b.onclick = () => on.add?.(kind, b);
    const w = el('div', 'orwrap');
    w.append(b);
    root.append(w);
  };

  section('Stack', 'builds top to bottom');
  const items = data.stack || [];
  if (!items.length) root.append(el('p', 'oempty', 'Nothing in the stack yet.'));
  items.forEach((it, i) => {
    const b = row('entry', it.key, `fam-${it.family || 'thing'}`);
    b.dataset.index = String(i);
    const badge = badgeOf(data.badges?.get?.(it.key));
    const id = idOf(it.key);
    const sub = [id ? it.what : it.what && it.what !== it.label ? it.what : '', placeOf(it)].filter(Boolean).join(' · ');
    b.append(el('span', 'no', String(i + 1)), el('i', 'gl'), text(id ?? it.label, sub));
    if (badge) {
      const x = el('span', `badge ${badge.tone}`, badge.n > 1 ? String(badge.n) : '');
      x.title = badge.title;
      x.setAttribute('role', 'img');
      x.setAttribute('aria-label', badge.n === 1 ? (badge.tone === 'error' ? 'a mistake' : 'a note') : `${badge.n} messages`);
      b.append(x);
    }
    b.append(el('span', 'chev', '›'));
    wrap(b, id ?? it.label);
  });
  if (data.editable) addRow('entry', 'Add to the stack', 'what can stand at the end');
  const shaped = [...(data.lines || []).map((m) => ['line', m]), ...(data.regions || []).map((m) => ['region', m])];
  if (shaped.length || data.editable) {
    section('Shape', 'lines and regions');
    for (const [kind, m] of shaped) {
      const b = row(kind, m.name);
      const thumb = kind === 'line' ? lineThumb(m, data.cols, data.rows) : regionThumb(m, data.cols, data.rows);
      b.append(thumb, text(m.name, kind === 'line' ? lineSummary(m) : regionSummary(m)), el('span', 'chev', '›'));
      wrap(b, m.name);
    }
    if (data.editable) {
      addRow('line', 'Add line', 'draw one through points');
      addRow('region', 'Add region', 'a place bounded by lines');
    }
  }
  wireKeys(root, on);
  if (data.editable) wireDrag(root, on);
  return root;
}

/** The rows of an outline, in order. */
export const rows = (root) => [...root.querySelectorAll('.orow')];

// ---- keys ---------------------------------------------------------------------------------------

/** What a row answers to besides the arrows (which the panel moves by): ⌥↑ ⌥↓ move an entry, the menu key opens its menu, F2 renames, Delete deletes. */
function wireKeys(root, on) {
  root.addEventListener('keydown', (e) => {
    const b = e.target.closest?.('.orow');
    if (!b || b.classList.contains('oadd')) return;
    const kind = b.dataset.kind, id = b.dataset.id;
    const more = b.parentElement.querySelector('.omore');
    if (!more) return;
    if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && kind === 'entry') {
      e.preventDefault(); e.stopPropagation();
      on.step?.(Number(b.dataset.index), e.key === 'ArrowUp' ? -1 : 1);
    } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault(); e.stopPropagation();
      on.menu?.(kind, id, more);
    } else if (e.key === 'F2' && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopPropagation();
      on.act?.('rename', kind, id, more);
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopPropagation();
      on.act?.('delete', kind, id, more);
    }
  });
}

// ---- dragging a stack row ---------------------------------------------------------------------------

const DRAG_PX = 5;

/** Press a stack row and pull it up or down: a line shows where it will stand, and letting go moves it there. */
function wireDrag(root, on) {
  let g = null; // the gesture under way
  const wraps = () => [...root.querySelectorAll('.orwrap')].filter((w) => w.firstElementChild.dataset.kind === 'entry');
  const scroller = () => root.closest('.body');
  const end = (move) => {
    if (!g) return;
    const { wrap, line, slot, from, started } = g;
    g = null;
    removeEventListener('pointermove', onMove, true);
    removeEventListener('pointerup', onUp, true);
    removeEventListener('pointercancel', onCancel, true);
    removeEventListener('keydown', onEsc, true);
    wrap.classList.remove('lift');
    wrap.style.transform = '';
    line?.remove();
    root.classList.remove('dragging');
    if (started) {
      // the click that ends a drag is not an open
      const stop = (e) => { e.stopPropagation(); e.preventDefault(); };
      addEventListener('click', stop, { capture: true, once: true });
      setTimeout(() => removeEventListener('click', stop, true), 0);
      if (move) on.move?.(from, slot);
    }
  };
  const onUp = () => end(true);
  const onCancel = () => end(false);
  const onEsc = (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); end(false); } };
  const frame = () => {
    if (!g?.started) return;
    const sc = scroller();
    if (sc && g.auto) { sc.scrollTop += g.auto; paint(); requestAnimationFrame(frame); }
  };
  /** Place the lifted row under the pointer and the line at the slot it means. */
  function paint() {
    const sc = scroller();
    const list = wraps();
    const rects = list.map((w) => w.getBoundingClientRect());
    // the lifted row stays at its own place in the flow: its middle is where it was, so the others' middles are fair
    const mids = rects.map((r, i) => (i === g.from ? Number.NEGATIVE_INFINITY : (r.top + r.bottom) / 2));
    const raw = slotAt(mids.filter((m) => m !== Number.NEGATIVE_INFINITY), g.y);
    // slots count the rows without the lifted one: put the lifted one's own place back
    g.slot = raw >= g.from ? raw + 1 : raw;
    const r0 = root.getBoundingClientRect();
    const at = g.slot >= list.length ? rects[list.length - 1].bottom : rects[g.slot].top;
    g.line.style.top = `${at - r0.top}px`;
    g.wrap.style.transform = `translateY(${g.y - g.startY + (sc ? sc.scrollTop - g.scroll0 : 0)}px)`;
  }
  function onMove(e) {
    if (!g) return;
    g.y = e.clientY;
    if (!g.started) {
      if (Math.abs(e.clientY - g.startY) < DRAG_PX) return;
      g.started = true;
      root.classList.add('dragging');
      g.wrap.classList.add('lift');
      g.line = el('div', 'odrop');
      root.append(g.line);
    }
    // near the edge of the panel the list scrolls
    const sc = scroller();
    g.auto = 0;
    if (sc) {
      const r = sc.getBoundingClientRect();
      if (e.clientY < r.top + 36) g.auto = -9;
      else if (e.clientY > r.bottom - 36) g.auto = 9;
    }
    paint();
    if (g.auto) requestAnimationFrame(frame);
  }
  root.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.pointerType === 'touch') return; // a finger scrolls; its way to move a row is the menu
    const b = e.target.closest?.('.orow');
    if (!b || b.dataset.kind !== 'entry' || g) return;
    const sc = scroller();
    g = { wrap: b.parentElement, from: Number(b.dataset.index), slot: Number(b.dataset.index), startY: e.clientY, y: e.clientY, started: false, scroll0: sc ? sc.scrollTop : 0, line: null, auto: 0 };
    addEventListener('pointermove', onMove, true);
    addEventListener('pointerup', onUp, true);
    addEventListener('pointercancel', onCancel, true);
    addEventListener('keydown', onEsc, true);
  });
}
