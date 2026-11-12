// A cloud as part of the selection: which cloud the source's caret (or the
// pointer) is in, and the shapes the view draws for it.  A cloud is written
// as a `top=` value, a body's or a thing's, or as a `pin` in a body's block;
// the build sends every cloud's low and high sheets (an entry's band) and
// every cloud at a point (a tree's branch tips, a rock's middle) in the
// payload's `debug`, each with its stack entry, and the view draws the
// selected entry's.

import { byteToIndex } from './text.js';

// a written `top=` value: quoted (escapes kept) or a bare word; after
// whitespace, so `stop=` and the like are not it
const TOP = /(?<=[\s{;])top=(?:"(?:[^"\\\n]|\\.)*"?|[^\s;{}]*)/g;
// a `pin` node, from its name to the end of its line (or a `;` or `}`)
const PIN = /(?<=^|[\s{;])pin(?=[\s;{}]|$)(?:"(?:[^"\\\n]|\\.)*"|[^\n;}"])*/gm;

/** A test for "is character `i` in a `//` comment": strings are skipped, a comment runs to its line's end. */
function commented(text) {
  const cut = new Int32Array(text.length + 1).fill(0);
  let inStr = false, on = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === '\n') { inStr = false; on = 0; continue; }
    if (!on) {
      if (ch === '\\' && inStr) { i++; continue; }
      if (ch === '"') inStr = !inStr;
      else if (!inStr && ch === '/' && text[i + 1] === '/') on = 1;
    }
    cut[i] = on;
  }
  return (i) => cut[i] === 1;
}

/**
 * The clouds written in `text` for the stack `items` (as the page holds
 * them: `{file, span: [byte, length], …}`) in `file`: [{entry, from, to,
 * pin}] as character indices, `pin` the pin's place in its entry (0 on) or
 * null for a `top=`.  Each belongs to the innermost entry written round it,
 * so a scatter's things' tops are the scatter's.
 */
export function cloudsIn(text, items, file) {
  if (!text) return [];
  const spans = items.map((it, entry) => {
    if (!it?.span || it.file !== file) return null;
    const from = byteToIndex(text, it.span[0]);
    return { entry, from, to: byteToIndex(text, it.span[0] + it.span[1]) };
  }).filter(Boolean);
  const owner = (i) => {
    let best = null;
    for (const s of spans) if (i >= s.from && i < s.to && (!best || s.to - s.from < best.to - best.from)) best = s;
    return best;
  };
  const inComment = commented(text);
  const out = [];
  const pins = new Map();
  // the pins first: a pin's own top= is the pin
  for (const [re, isPin] of [[PIN, true], [TOP, false]]) {
    for (const m of text.matchAll(re)) {
      if (inComment(m.index)) continue;
      if (!isPin && out.some((c) => c.pin != null && m.index >= c.from && m.index < c.to)) continue;
      const s = owner(m.index);
      if (!s) continue;
      let pin = null;
      if (isPin) { pin = pins.get(s.entry) ?? 0; pins.set(s.entry, pin + 1); }
      out.push({ entry: s.entry, from: m.index, to: m.index + m[0].trimEnd().length, pin });
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

/** The debug payload's sheets and clouds at a point that belong to stack entry `entry`. */
export function cloudOf(debug, entry) {
  if (!debug || entry == null) return { sheets: [], columns: [] };
  return {
    sheets: (debug.sheets ?? []).filter((s) => s.entry === entry),
    columns: (debug.columns ?? []).filter((c) => c.entry === entry),
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
