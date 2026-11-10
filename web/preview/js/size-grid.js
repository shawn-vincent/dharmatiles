// The size picker: a grid under a tile's size, shared by the property sheet's
// `size` control (lens.js) and the New tile dialog (editor/tiles.js).

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

/**
 * A grid under a tile's size, as a word processor offers for a table:
 * pointing at a cell shows that many squares across and up in the field,
 * clicking takes it.  Typing in the field moves the grid with it.
 * Returns what closes it.  `quote`: the field holds the size as a KDL string
 * ("2x1"), as the property sheet's does; off, bare (2x1).  `parent`: where the
 * grid is put; inside an open dialog, which would cover it on the page.
 */
export function sizeGrid(input, take, { quote = true, parent = document.body } = {}) {
  const MIN = 4, MAX = 8;
  const fmt = (p) => (quote ? `"${p[0]}x${p[1]}"` : `${p[0]}x${p[1]}`);
  const pop = el('div', 'size-grid');
  const cells = el('div', 'size-cells');
  const label = el('div', 'size-label');
  pop.append(cells, label);
  const parse = (t) => {
    const m = t.match(/(\d+)\s*x\s*(\d+)/i);
    return m ? [Math.max(1, Number(m[1])), Math.max(1, Number(m[2]))] : null;
  };
  let typed = input.value; // what the field holds apart from pointing
  let made = [0, 0];
  let full = false; // over the field: all of it at once, so no cell moves under the pointer
  const draw = (pick) => {
    const [x, y] = pick || [0, 0];
    // one spare row and column past what is picked, so the grid grows as you go
    const n = full ? [MAX, MAX] : [Math.min(MAX, Math.max(MIN, x + 1)), Math.min(MAX, Math.max(MIN, y + 1))];
    if (n[0] !== made[0] || n[1] !== made[1]) {
      made = n;
      cells.replaceChildren();
      cells.style.gridTemplateColumns = `repeat(${n[0]}, 1fr)`;
      for (let j = 1; j <= n[1]; j++) {
        for (let i = 1; i <= n[0]; i++) {
          const b = el('span', 'size-cell');
          b.dataset.x = i; b.dataset.y = j;
          cells.append(b);
        }
      }
    }
    for (const b of cells.children) b.classList.toggle('on', Number(b.dataset.x) <= x && Number(b.dataset.y) <= y);
    label.textContent = pick ? `${x} across × ${y} up` : 'squares across × up';
  };
  // placed on the window, not in the pane (which would clip it): under
  // the field, growing down, or over it, whole, when the window has no
  // room below for the whole grid
  let below = true;
  const place = () => {
    const f = input.getBoundingClientRect();
    pop.style.left = `${Math.max(4, Math.min(f.left, window.innerWidth - pop.offsetWidth - 4))}px`;
    pop.style.top = `${below ? f.bottom + 4 : Math.max(4, f.top - 4 - pop.offsetHeight)}px`;
  };
  window.addEventListener('scroll', place, true);
  const at = (e) => {
    const b = e.target.closest?.('.size-cell');
    return b ? [Number(b.dataset.x), Number(b.dataset.y)] : null;
  };
  // the field keeps the focus: a click in the grid is not a blur
  pop.addEventListener('pointerdown', (e) => e.preventDefault());
  pop.addEventListener('mousedown', (e) => e.preventDefault());
  cells.addEventListener('pointermove', (e) => {
    const p = at(e);
    if (!p) return;
    input.value = fmt(p);
    draw(p);
  });
  cells.addEventListener('pointerleave', () => { input.value = typed; draw(parse(typed)); });
  cells.addEventListener('click', (e) => {
    const p = at(e);
    if (!p) return;
    input.value = fmt(p);
    take();
  });
  input.addEventListener('input', () => { typed = input.value; draw(parse(typed)); });
  full = true; draw(null);
  // on the page itself: the sidebar slides (a transform), which would
  // carry a fixed box with it
  parent.append(pop);
  below = input.getBoundingClientRect().bottom + 4 + pop.offsetHeight <= window.innerHeight - 4;
  full = !below;
  draw(parse(typed));
  place();
  return () => { window.removeEventListener('scroll', place, true); pop.remove(); };
}

/**
 * Make a plain text field a size field: the grid opens under it on focus or
 * click, and closes when a size is taken, on Enter, Tab or Escape, or on blur.
 */
export function attachSizeGrid(input, { parent } = {}) {
  let close = null;
  const shut = () => { close?.(); close = null; };
  const open = () => { if (!close) close = sizeGrid(input, shut, { quote: false, parent }); };
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
