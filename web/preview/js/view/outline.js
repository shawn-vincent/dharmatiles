// What the tile level holds, as rows: the stack drawn as the tile stands, the top of the tile first and the base last (the
// reverse of the order it is written and built in; each row's number is its place in that order), and the lines and
// regions that shape the tile.  Each row is one button that opens its thing's level; beside it a ⋯ button opens the
// row's menu (rename, hide, delete, move, add), the stack starts with an Add row (what stands on top) and the Shape ends
// in Add line and Add region.  A row of the stack can be dragged by the mouse to a new place, and ⌥↑ / ⌥↓ move the row
// the keyboard is on up or down the tile.  A row's `data-index` is its place in the written order, which is what the
// caller's move, step and add take.  A region wears a picture of its mask, a line a glyph of its points.
//
// `Outline({data, on})` is a function of `data`: `{lines, regions, stack, badges, cols, rows, editable}` (the map's lines
// and regions; the stack items in written order; the messages by entry key; the tile's size in squares; whether the
// document can be changed from here); `on` is what a row does:
//   `open(kind, id)`, `hover(w | null)`, `menu(kind, id, anchor)`, `act(action, kind, id, anchor)`, `add(kind, anchor)`,
//   `move(from, slot)` (a stack row dragged), `step(from, dir)` (⌥↑ / ⌥↓), `focusAfter()` (the index of the row the
//   keyboard follows to where a move put it, once; or null).
// The rules of the rows are in js/rows.js.

import { signal } from '@preact/signals-core';
import { badgeOf, placeOf } from '../nav.js';
import { clauseTitle } from '../editor/region-sheet.js';
import { coverage, idOf, linePoints, maskThumb, slotAt, thumbBox } from '../rows.js';
import { html, useEffect, useRef } from './html.js';
import { Icon } from './icons.js';
import { plural } from './widgets.js';

/** What a region says of itself in a few words: what bounds it, or its clauses. */
export function regionSummary(m) {
  const sh = m.sheet;
  if (!sh) return 'region';
  const parts = [];
  if (sh.boundedBy?.text) parts.push(`bounded by ${sh.boundedBy.text}`);
  for (const c of sh.clauses || []) parts.push(clauseTitle(c));
  return parts.join('; ') || 'region';
}

/** What a line says of itself: its points. */
export const lineSummary = (m) => (m.points?.length ? plural(m.points.length, 'point') : 'line');

// ---- the pictures a Shape row wears --------------------------------------------------------

const urls = new WeakMap(); // region -> the data URL of its picture: made once for each check's map

/** The region's mask as a small picture: the build's own cells, drawn in the region's colour through the picture as a mask. */
function regionUrl(r) {
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
  return url;
}

function RegionThumb({ r, cols, rows }) {
  const [bw, bh] = thumbBox(cols, rows);
  const style = `width: ${bw}px; height: ${bh}px`;
  if (!coverage(r)) return html`<span class="thumb region empty" style=${style} aria-hidden="true"></span>`;
  const url = regionUrl(r);
  return html`<span class="thumb region" style=${style} aria-hidden="true" title=${`covers ${Math.round(coverage(r) * 100)}% of the tile`}><i style=${`-webkit-mask-image: url(${url}); mask-image: url(${url})`}></i></span>`;
}

function LineThumb({ m, cols, rows }) {
  const [bw, bh] = thumbBox(cols, rows);
  const pts = linePoints(m.pts, cols, rows, bw, bh);
  return html`<span class="thumb line" style=${`width: ${bw}px; height: ${bh}px`} aria-hidden="true"><svg viewBox=${`0 0 ${bw} ${bh}`}>${pts.length > 1 ? html`<polyline points=${pts.map((q) => q.join(',')).join(' ')} />` : null}</svg></span>`;
}

// ---- the rows ---------------------------------------------------------------------------

const Text = ({ label, sub }) => html`<span class="tx"><span class="nm">${label}</span>${sub ? html`<span class="sub">${sub}</span>` : null}</span>`;
const Section = ({ title, hint }) => html`<div class="cap olab"><span>${title}</span>${hint ? html`<small>${hint}</small>` : null}</div>`;

/** A row and its ⋯ button, side by side (a button cannot hold a button). */
function Wrap({ kind, id, name, editable, on, cls = '', index, children }) {
  return html`<div class="orwrap">
    <button type="button" class=${`orow ${cls}`} role="option" data-kind=${kind} data-id=${id} data-index=${index} onClick=${() => on.open(kind, id)} onMouseEnter=${() => on.hover?.({ kind, id })} onMouseLeave=${() => on.hover?.(null)}>${children}</button>
    ${editable ? html`<button type="button" class="omore" title=${`${name}: rename, hide, delete, move, add`} aria-label=${`Actions for ${name}`} aria-haspopup="menu" onClick=${(e) => { e.stopPropagation(); on.menu?.(kind, id, e.currentTarget); }}><${Icon} name="ellipsis" size=${16} /></button>` : null}</div>`;
}

const AddRow = ({ kind, label, hint, on }) => html`<div class="orwrap"><button type="button" class="orow oadd" data-kind="add" data-id=${kind} aria-haspopup=${kind === 'entry' ? 'menu' : 'false'} onClick=${(e) => on.add?.(kind, e.currentTarget)}><span class="plus">+</span><${Text} label=${label} sub=${hint} /></button></div>`;

// ---- a drag of a stack row ---------------------------------------------------------------------------

const DRAG_PX = 5;
/** True while a stack row is being dragged: the rows are not drawn again under the pointer. */
const dragging = signal(false);

/**
 * Press a stack row and pull it up or down: a line shows where it will stand, and letting go moves it there.
 * The rows are drawn top of the tile first, so a place on screen (`pos`, counted from the top) is written
 * place `n - 1 - pos`, and the gap above screen row `s` is the written slot `n - s`.
 */
function wireDrag(root, getOn) {
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
    dragging.value = false;
    if (started) {
      // the click that ends a drag is not an open
      const stop = (e) => { e.stopPropagation(); e.preventDefault(); };
      addEventListener('click', stop, { capture: true, once: true });
      setTimeout(() => removeEventListener('click', stop, true), 0);
      const n = wraps().length;
      if (move) getOn().move?.(n - 1 - from, n - slot);
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
      dragging.value = true;
      root.classList.add('dragging');
      g.wrap.classList.add('lift');
      g.line = document.createElement('div');
      g.line.className = 'odrop';
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
  const down = (e) => {
    if (e.button !== 0 || e.pointerType === 'touch') return; // a finger scrolls; its way to move a row is the menu
    const b = e.target.closest?.('.orow');
    if (!b || b.dataset.kind !== 'entry' || g) return;
    const sc = scroller();
    const pos = wraps().indexOf(b.parentElement);
    g = { wrap: b.parentElement, from: pos, slot: pos, startY: e.clientY, y: e.clientY, started: false, scroll0: sc ? sc.scrollTop : 0, line: null, auto: 0 };
    addEventListener('pointermove', onMove, true);
    addEventListener('pointerup', onUp, true);
    addEventListener('pointercancel', onCancel, true);
    addEventListener('keydown', onEsc, true);
  };
  root.addEventListener('pointerdown', down);
  return () => root.removeEventListener('pointerdown', down);
}

/** What a row answers to besides the arrows (which the panel moves by): ⌥↑ ⌥↓ move an entry, the menu key opens its menu, F2 renames, Delete deletes. */
function onRowKey(e, on) {
  const b = e.target.closest?.('.orow');
  if (!b || b.classList.contains('oadd')) return;
  const kind = b.dataset.kind, id = b.dataset.id;
  const more = b.parentElement.querySelector('.omore');
  if (!more) return;
  if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && kind === 'entry') {
    e.preventDefault(); e.stopPropagation();
    on.step?.(Number(b.dataset.index), e.key === 'ArrowUp' ? 1 : -1); // up the tile is later in the stack
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
}

export function Outline({ data, on }) {
  const root = useRef(/** @type {HTMLElement | null} */ (null));
  const held = useRef(/** @type {any} */ (null));
  const onRef = useRef(on);
  onRef.current = on;
  const editable = !!data.editable;
  useEffect(() => (editable && root.current ? wireDrag(root.current, () => onRef.current) : undefined), [editable]);
  // after a move the keyboard follows the row to where it went (its key may have changed with its place)
  useEffect(() => {
    const i = on.focusAfter?.();
    if (i == null) return;
    [...(root.current?.querySelectorAll('.orow') ?? [])].find((x) => x.dataset.kind === 'entry' && Number(x.dataset.index) === i)?.focus({ preventScroll: false });
  });
  if (dragging.value && held.current) return held.current;
  const items = data.stack || [];
  const shaped = [...(data.lines || []).map((m) => ['line', m]), ...(data.regions || []).map((m) => ['region', m])];
  const vnode = html`<div class="outline" role="listbox" aria-label="What the tile holds" ref=${root} onKeyDown=${(e) => onRowKey(e, on)}>
    <${Section} title="Stack" hint="top of the tile first" />
    ${editable ? html`<${AddRow} kind="entry" label="Add to the stack" hint="what stands on top" on=${on} />` : null}
    ${items.length ? null : html`<p class="oempty">Nothing in the stack yet.</p>`}
    ${[...items.entries()].reverse().map(([i, it]) => {
      const badge = badgeOf(data.badges?.get?.(it.key));
      const id = idOf(it.key);
      const sub = [id ? it.what : it.what && it.what !== it.label ? it.what : '', placeOf(it)].filter(Boolean).join(' · ');
      return html`<${Wrap} key=${it.key} kind="entry" id=${it.key} name=${id ?? it.label} editable=${editable} on=${on} cls=${`fam-${it.family || 'thing'}`} index=${String(i)}>
        <span class="no">${String(i + 1)}</span><i class="gl"></i><${Text} label=${id ?? it.label} sub=${sub} />
        ${badge ? html`<span class=${`badge ${badge.tone}`} title=${badge.title} role="img" aria-label=${badge.n === 1 ? (badge.tone === 'error' ? 'a mistake' : 'a note') : `${badge.n} messages`}>${badge.n > 1 ? String(badge.n) : ''}</span>` : null}<span class="chev"><${Icon} name="chevron-right" size=${14} /></span><//>`;
    })}
    ${shaped.length || editable ? html`<${Section} title="Shape" hint="lines and regions" />
      ${shaped.map(([kind, m]) => html`<${Wrap} key=${`${kind}:${m.name}`} kind=${kind} id=${m.name} name=${m.name} editable=${editable} on=${on}>
        ${kind === 'line' ? html`<${LineThumb} m=${m} cols=${data.cols} rows=${data.rows} />` : html`<${RegionThumb} r=${m} cols=${data.cols} rows=${data.rows} />`}
        <${Text} label=${m.name} sub=${kind === 'line' ? lineSummary(m) : regionSummary(m)} /><span class="chev"><${Icon} name="chevron-right" size=${14} /></span><//>`)}
      ${editable ? html`<${AddRow} kind="line" label="Add line" hint="draw one through points" on=${on} /><${AddRow} kind="region" label="Add region" hint="a place bounded by lines" on=${on} />` : null}` : null}
  </div>`;
  held.current = vnode;
  return vnode;
}
