// The debug layer: what a build shows of its own working (the payload's
// `debug`, see crates/dharmatiles/src/preview/payload.rs debug_json), as
// plain arrays the viewer draws: grass starts and spines.  Off by default;
// ⌃⌥D or the view's ⋯ menu.  The clouds are drawn as part of the selection
// (cloud.js).
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
 * The layer's legend: one row per class drawn, in CLASSES order, with its
 * count.  `[{token, label, kind, n}]`.
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
  return rows;
}
