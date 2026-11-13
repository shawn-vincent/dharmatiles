// The map's crossfade: a piece whose mesh changes (a box becomes a tile, a tier gives way to the next) does not
// pop; the old and the new are both drawn for a moment and each pixel is given to one of them (a screen-door
// dither), so there is no blending, no sorting, depth stays right and the paint and overhang colours are the
// meshes' own.  This is the pure part: the layers a piece holds, and which share of the pixels each has.
//
// A piece's layers run newest first.  Each has a progress `p` (0 to 1, eased) over what is older than it: the
// newest takes the pixels whose dither value d is under its p, the next takes d in [p0, max(p0, p1)), and so on;
// the oldest takes the rest.  A swap that comes while another runs just puts a new layer on top: what was shown
// a moment ago is still shown where it was, so nothing restarts from blank and nothing jumps.

import { easeInOutCubic } from './consts.js';

/** Milliseconds a swap takes. */
export const CROSSFADE_MS = 300;
/** The most layers one piece holds; a swap past it lets the oldest go at once. */
export const MAX_LAYERS = 3;
/** A `hi` above every dither value: the layer keeps all that is left. */
export const ALL = 2;

/** A fresh state for a piece: no fade running (one layer, or none). */
export const newFade = () => ({ layers: [] });

/**
 * Begin a swap at `now` (ms) from the key `outgoing` to `incoming`.  Layers are `{key, t0}`, newest first; the
 * first swap puts the old key under the new one.
 */
export function swap(fade, now, outgoing, incoming) {
  if (fade.layers.length === 0) fade.layers.push({ key: outgoing, t0: now });
  fade.layers.unshift({ key: incoming, t0: now });
  if (fade.layers.length > MAX_LAYERS) fade.layers.length = MAX_LAYERS;
}

/** Eased progress of a layer at `now`. */
export const progress = (layer, now, ms = CROSSFADE_MS) => easeInOutCubic(Math.min(1, Math.max(0, (now - layer.t0) / ms)));

/**
 * The dither interval `[lo, hi)` each layer draws at `now`, newest first.  A layer that has finished takes all
 * that is left, so everything older is empty: those layers are dropped from the state in place.  The oldest
 * layer's `hi` is `ALL`.  Together the intervals cover [0, 1) exactly once.
 */
export function intervals(fade, now, ms) {
  const ps = fade.layers.map((l) => progress(l, now, ms));
  const done = ps.findIndex((p) => p >= 1);
  if (done >= 0) { fade.layers.length = done + 1; ps.length = done + 1; }
  const out = [];
  let lo = 0;
  for (let i = 0; i < ps.length; i++) {
    const hi = i === ps.length - 1 ? ALL : Math.max(lo, ps[i]);
    out.push([lo, hi]);
    lo = hi;
  }
  return out;
}

/** Whether a fade is on: more than one layer held. */
export const running = (fade) => fade.layers.length > 1;
