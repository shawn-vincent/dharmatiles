// The debug layer: what a build shows of its own working (the payload's
// `debug`, see crates/dharmatiles/src/preview/payload.rs debug_json), as
// plain arrays the viewer draws.  Off by default; ⌃⌥D or the view's ⋯ menu.
// No THREE here, so the tests can read it.

/** Every class the layer draws: [class, theme token, legend label, kind]. */
export const CLASSES = [
  ['spine', '--dbg-spine', 'Blade spine', 'line'],
  ['spine-cut', '--dbg-cut', 'Spine cut short (at a line, the rim, bare soil, a rock)', 'line'],
  ['spine-edge', '--dbg-edge', 'Spine of a start added along the edge', 'line'],
  ['flow', '--dbg-flow', 'Grass flow', 'line'],
  ['grew', '--dbg-spine', 'Start: grew', 'dot'],
  ['cut', '--dbg-cut', 'Start: grew, cut short', 'dot'],
  ['outside', '--dbg-outside', 'Start: outside the area', 'dot'],
  ['short-line', '--dbg-short-line', 'Start: too short at a line', 'dot'],
  ['short-rim', '--dbg-short-rim', 'Start: too short at the tile edge', 'dot'],
  ['short-other', '--dbg-short-other', 'Start: too short (soil, rock, fold)', 'dot'],
  ['crowded', '--dbg-crowded', 'Start: crowded', 'dot'],
  ['under', '--dbg-under', 'Start: under a stone', 'dot'],
  ['soil', '--dbg-soil', 'Start: bare soil', 'dot'],
  ['cover', '--dbg-cover', 'Start: left out by cover', 'dot'],
];

/** The hues band sheets take, by their entry's place in the stack. */
export const BAND_TOKENS = ['--dbg-band-1', '--dbg-band-2', '--dbg-band-3', '--dbg-band-4'];

const byClass = new Map(CLASSES.map((c) => [c[0], c]));

/** A class's [token, label, kind]; an unknown class (a newer server) shows under its own name in the flow colour. */
export function classInfo(cls) {
  const c = byClass.get(cls);
  return c ? { token: c[1], label: c[2], kind: c[3] } : { token: '--dbg-flow', label: cls, kind: 'line' };
}

/** Polylines `[[x, y, z, …], …]` as segment pairs: a Float32Array of 6 numbers per segment. */
export function segments(trails) {
  let n = 0;
  for (const t of trails) n += Math.max(0, t.length / 3 - 1);
  const out = new Float32Array(6 * n);
  let o = 0;
  for (const t of trails) {
    for (let k = 3; k < t.length; k += 3) {
      out.set(t.subarray ? t.subarray(k - 3, k + 3) : t.slice(k - 3, k + 3), o);
      o += 6;
    }
  }
  return out;
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
  const tri = [], edge = [];
  const p = (i, j) => [i * step, j * step, at(i, j)];
  const cell = (i, j) => i >= 0 && j >= 0 && i < cx && j < cy && drawn[j * cx + i] === 1;
  for (let j = 0; j < cy; j++) {
    for (let i = 0; i < cx; i++) {
      if (!cell(i, j)) continue;
      const a = p(i, j), b = p(i + 1, j), c = p(i + 1, j + 1), d = p(i, j + 1);
      tri.push(...a, ...b, ...c, ...a, ...c, ...d);
      if (!cell(i, j - 1)) edge.push(...a, ...b);
      if (!cell(i + 1, j)) edge.push(...b, ...c);
      if (!cell(i, j + 1)) edge.push(...c, ...d);
      if (!cell(i - 1, j)) edge.push(...d, ...a);
    }
  }
  return { positions: new Float32Array(tri), contour: new Float32Array(edge) };
}

/** A stack entry's path as a reader names it: `tile/dirt/0` is `dirt`, `tile/water.pool/3` is `water.pool`. */
export function entryName(owner) {
  return owner.split('/').filter((s) => s !== 'tile' && !/^\d+$/.test(s)).join(' ') || owner;
}

/**
 * The layer's legend: one row per class drawn, in CLASSES order, with its
 * count, then one per band sheet pair.  `[{token, label, kind, n}]`.
 */
export function legendRows(debug) {
  if (!debug) return [];
  const rows = [];
  const seen = new Set([...Object.keys(debug.dots ?? {}), ...Object.keys(debug.trails ?? {})]);
  const order = [...CLASSES.map((c) => c[0]), ...[...seen].filter((c) => !byClass.has(c)).sort()];
  for (const cls of order) {
    if (!seen.has(cls)) continue;
    const info = classInfo(cls);
    const kind = debug.dots?.[cls] ? 'dot' : 'line';
    const n = kind === 'dot' ? debug.dots[cls].length / 3 : debug.trails[cls].length;
    if (n > 0) rows.push({ ...info, kind, n });
  }
  const bands = new Map();
  for (const s of debug.sheets ?? []) if (!bands.has(s.entry)) bands.set(s.entry, s.owner);
  let k = 0;
  for (const [, owner] of bands) {
    rows.push({ token: BAND_TOKENS[k++ % BAND_TOKENS.length], label: `${entryName(owner)}: band low and high`, kind: 'sheet', n: 1 });
  }
  return rows;
}

/** The hue token of each sheet's entry, the same order legendRows gives. */
export function sheetTokens(debug) {
  const k = new Map();
  for (const s of debug?.sheets ?? []) if (!k.has(s.entry)) k.set(s.entry, BAND_TOKENS[k.size % BAND_TOKENS.length]);
  return (debug?.sheets ?? []).map((s) => k.get(s.entry));
}
