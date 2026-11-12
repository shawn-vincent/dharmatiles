// A cloud as part of the selection: which cloud the source's caret (or the
// pointer) is in, and the shapes the view draws for it.  A cloud is written
// as a `top=` value, a body's or a thing's, or as a `pin` in a body's block;
// the program says where each is written (the stack's `clouds`), and the
// build sends every cloud's low and high sheets (an entry's band) and every
// cloud at a point (a tree's branch tips, a rock's middle) in the payload,
// each with its entry's key, and the view draws the selected entry's.

import { byteToIndex } from './text.js';

/**
 * The clouds written in `text`, for the stack `items` (as the page holds
 * them: `{key, clouds: [{kind, file, span: [byte, length], pin}], …}`) in
 * `file`: [{key, from, to, pin}] as character indices, sorted by place, `pin`
 * the pin's place in its entry (0 on) or null for a `top=`.  The program
 * sends each entry's clouds, read off what it parsed, so a comment, a hidden
 * (`/-`) node or a raw string holds none, and a call's `top=` is the cloud of
 * the tree inside the definition.  This only turns bytes into characters.
 */
export function cloudsIn(text, items, file) {
  if (!text) return [];
  const out = [];
  for (const it of items) {
    if (!it?.key) continue;
    for (const c of it.clouds ?? []) {
      if (c.file !== file) continue;
      out.push({ key: it.key, from: byteToIndex(text, c.span[0]), to: byteToIndex(text, c.span[0] + c.span[1]), pin: c.pin ?? null });
    }
  }
  return out.sort((a, b) => a.from - b.from);
}

/** The cloud the caret at character `i` is in (its ends count), else null. */
export function cloudAtCaret(clouds, i) {
  return clouds.find((c) => i >= c.from && i <= c.to) ?? null;
}

/** The first cloud written on 1-based line `n`, else null. */
export function cloudOnLine(text, clouds, n) {
  if (!text || n == null) return null;
  let start = 0;
  for (let k = 1; k < n; k++) {
    const j = text.indexOf('\n', start);
    if (j < 0) return null;
    start = j + 1;
  }
  const j = text.indexOf('\n', start), end = j < 0 ? text.length : j;
  return clouds.find((c) => c.from <= end && c.to >= start) ?? null;
}

/** The debug payload's sheets and clouds at a point that belong to the entry whose key is `key`. */
export function cloudOf(debug, key) {
  if (!debug || key == null) return { sheets: [], columns: [] };
  return {
    sheets: (debug.sheets ?? []).filter((s) => s.owner === key),
    columns: (debug.columns ?? []).filter((c) => c.owner === key),
  };
}

/** How strong a shade of 0 to 1 draws, as an opacity: [for 0, for 1]. A height that never comes up is nearly clear. */
export const SHADE_SHEET = [0.04, 0.55];
export const SHADE_COLUMN = [0.12, 1];

/** Opacity for a shade (0 to 1; flat odds are 0.5) over `[least, most]`. */
export function shadeAlpha(shade, [least, most]) {
  const t = Math.min(1, Math.max(0, shade ?? 0.5));
  return least + (most - least) * t;
}

/**
 * A sheet `{nx, ny, step, z: [h or null]}` as triangles (two per lattice
 * cell whose four corners are all set) and its contour: the cell edges
 * between a drawn cell and one that is not, at their heights.
 */
export function sheetGeometry(s) {
  const { nx, ny, step, z } = s;
  const at = (i, j) => z[j * nx + i];
  const cx = nx - 1, cy = ny - 1;
  const drawn = new Uint8Array(Math.max(0, cx * cy));
  for (let j = 0; j < cy; j++) {
    for (let i = 0; i < cx; i++) {
      drawn[j * cx + i] = at(i, j) != null && at(i + 1, j) != null && at(i, j + 1) != null && at(i + 1, j + 1) != null ? 1 : 0;
    }
  }
  const tri = [], edge = [], shade = [];
  const sh = (i, j) => (s.shade ? s.shade[j * nx + i] : 0.5);
  const p = (i, j) => [i * step, j * step, at(i, j)];
  const cell = (i, j) => i >= 0 && j >= 0 && i < cx && j < cy && drawn[j * cx + i] === 1;
  for (let j = 0; j < cy; j++) {
    for (let i = 0; i < cx; i++) {
      if (!cell(i, j)) continue;
      const a = p(i, j), b = p(i + 1, j), c = p(i + 1, j + 1), d = p(i, j + 1);
      tri.push(...a, ...b, ...c, ...a, ...c, ...d);
      shade.push(sh(i, j), sh(i + 1, j), sh(i + 1, j + 1), sh(i, j), sh(i + 1, j + 1), sh(i, j + 1));
      if (!cell(i, j - 1)) edge.push(...a, ...b);
      if (!cell(i + 1, j)) edge.push(...b, ...c);
      if (!cell(i, j + 1)) edge.push(...c, ...d);
      if (!cell(i - 1, j)) edge.push(...d, ...a);
    }
  }
  return { positions: new Float32Array(tri), contour: new Float32Array(edge), shade: new Float32Array(shade) };
}

/**
 * Clouds at a point, flat `v` of a payload cloud `[x, y, low, high, rolled, shade low, shade high,
 * shade rolled, …]`, as a line from low to high at each (two vertices, each
 * with its shade) and a marker at the rolled height.
 */
export function columnGeometry(flat) {
  const n = Math.floor(flat.length / 8);
  const lines = new Float32Array(6 * n), lineShade = new Float32Array(2 * n);
  const marks = new Float32Array(3 * n), markShade = new Float32Array(n);
  for (let k = 0; k < n; k++) {
    const [x, y, lo, hi, rolled, s0, s1, s2] = flat.slice(8 * k, 8 * k + 8);
    lines.set([x, y, lo, x, y, hi], 6 * k);
    lineShade.set([s0, s1], 2 * k);
    marks.set([x, y, rolled], 3 * k);
    markShade[k] = s2;
  }
  return { lines, lineShade, marks, markShade, n };
}
