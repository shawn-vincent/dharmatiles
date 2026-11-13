// The size picker: a grid under a tile's size, shared by the property sheet's `size` control and the New tile dialog.
//
// `SizeGridBox` is the grid: pointing at a cell shows that many squares across and up, clicking takes it.  It is a
// function of `pick` (the squares shown: the one pointed at, else the one typed) and of whether the whole grid is
// shown at once (`full`: over the field, so no cell moves under the pointer); it says what was pointed at (`onPoint`,
// null when the pointer left) and what was taken (`onTake`).  `openSizeGrid` puts one on the page by an input and
// returns what closes it; `attachSizeGrid` does that for a plain text field of the page (the New tile dialog).

import { html, render, useLayoutEffect, useRef } from './html.js';

const MIN = 4, MAX = 8;

/** The squares a typed size says (`2x1`), or null. */
export const parseSize = (t) => {
  const m = t.match(/(\d+)\s*x\s*(\d+)/i);
  return m ? [Math.max(1, Number(m[1])), Math.max(1, Number(m[2]))] : null;
};

/** The grid's own size for a pick: one spare row and column past it, so the grid grows as you go. */
const gridFor = (pick, full) => {
  const [x, y] = pick || [0, 0];
  return full ? [MAX, MAX] : [Math.min(MAX, Math.max(MIN, x + 1)), Math.min(MAX, Math.max(MIN, y + 1))];
};

export function SizeGridBox({ pick, full, input, onPoint, onTake }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  const [cols, rows] = gridFor(pick, full);
  const [x, y] = pick || [0, 0];
  // placed on the window, not in the pane (which would clip it): under the field, growing down, or over it, whole,
  // when the window has no room below for the whole grid
  useLayoutEffect(() => {
    const place = () => {
      const f = input.getBoundingClientRect();
      const el = box.current;
      if (!el) return;
      const below = f.bottom + 4 + el.offsetHeight <= window.innerHeight - 4;
      el.style.left = `${Math.max(4, Math.min(f.left, window.innerWidth - el.offsetWidth - 4))}px`;
      el.style.top = `${below ? f.bottom + 4 : Math.max(4, f.top - 4 - el.offsetHeight)}px`;
    };
    place();
    window.addEventListener('scroll', place, true);
    return () => window.removeEventListener('scroll', place, true);
  });
  const at = (/** @type {any} */ e) => {
    const b = e.target.closest?.('.size-cell');
    return b ? [Number(b.dataset.x), Number(b.dataset.y)] : null;
  };
  const cells = [];
  for (let j = 1; j <= rows; j++) for (let i = 1; i <= cols; i++) cells.push(html`<span class=${`size-cell${i <= x && j <= y ? ' on' : ''}`} data-x=${i} data-y=${j} key=${`${i},${j}`}></span>`);
  // the field keeps the focus: a click in the grid is not a blur
  return html`<div class="size-grid" ref=${box} onPointerDown=${(e) => e.preventDefault()} onMouseDown=${(e) => e.preventDefault()}>
    <div class="size-cells" style=${`grid-template-columns: repeat(${cols}, 1fr)`}
      onPointerMove=${(e) => { const p = at(e); if (p) onPoint(p); }} onPointerLeave=${() => onPoint(null)} onClick=${(e) => { const p = at(e); if (p) onTake(p); }}>${cells}</div>
    <div class="size-label">${pick ? `${x} across × ${y} up` : 'squares across × up'}</div></div>`;
}

/**
 * Put a size grid by `input` (a plain field of the page: its value is read and written here).  `quote`: the field
 * holds the size as a KDL string ("2x1"); off, bare (2x1).  `parent`: where the grid goes; inside an open dialog, which
 * would cover it on the page.  Returns what closes it.
 */
export function openSizeGrid(input, take, { quote = true, parent = document.body } = {}) {
  const fmt = (p) => (quote ? `"${p[0]}x${p[1]}"` : `${p[0]}x${p[1]}`);
  const host = document.createElement('div');
  parent.append(host);
  let typed = input.value; // what the field holds apart from pointing
  let pointed = null;
  const below = () => input.getBoundingClientRect().bottom + 4 + 330 <= window.innerHeight - 4;
  let full = !below();
  const draw = () => render(html`<${SizeGridBox} pick=${pointed ?? parseSize(typed)} full=${full} input=${input}
    onPoint=${(p) => { pointed = p; input.value = p ? fmt(p) : typed; draw(); }} onTake=${(p) => { input.value = fmt(p); take(); }} />`, host);
  const onInput = () => { typed = input.value; pointed = null; draw(); };
  input.addEventListener('input', onInput);
  draw();
  return () => { input.removeEventListener('input', onInput); render(null, host); host.remove(); };
}

/**
 * Make a plain text field a size field: the grid opens under it on focus or click, and closes when a size is taken, on
 * Enter, Tab or Escape, or on blur.
 */
export function attachSizeGrid(input, { parent } = {}) {
  let close = null;
  const shut = () => { close?.(); close = null; };
  const open = () => { if (!close) close = openSizeGrid(input, shut, { quote: false, parent }); };
  input.addEventListener('focus', open);
  input.addEventListener('click', open);
  input.addEventListener('blur', shut);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === 'Tab') shut();
    else if (e.key === 'Escape' && close) { e.preventDefault(); e.stopPropagation(); shut(); }
    else if (e.key === 'ArrowDown' && !close) open();
  });
  return shut;
}
