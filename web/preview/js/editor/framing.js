// What the 3D view is told: a description of what it shows, and nothing about its camera.
//
// The viewer decides its own camera from this and from what it holds (js/viewer/framing.js has the rules and
// the one memory of where the user last left it, per document and mode).  This module works the description
// out from the page's model, whenever any of it changes, and hands it over: the tile or map shown, in which
// mode (tile, map or the plates), the preset asked for (`view.mode`, and `view.pick` when it was asked for
// again), the plan, a map's box and the piece the Close view frames.  Nothing else in the page moves the
// camera: not a build, a rebuild, an edit, a piece arriving or a document opening.

import { effect, untracked } from '@preact/signals-core';
import * as M from '../map-model.js';
import { viewer } from './app.js';
import { planOf } from './docs.js';
import * as model from './model.js';
import { view } from './stores.js';

/** The world box a map is framed on: the pieces' footprints and the grid, the ground to a little over the tallest. */
export function frameBox(ps, scene) {
  const b = M.bounds(ps);
  let lo = b ? [...b.lo] : [0, 0], hi = b ? [...b.hi] : [0, 0];
  if (scene.grid) {
    const sq = ps.find((p) => p.sq)?.sq ?? 35;
    lo = [Math.min(lo[0], 0), Math.min(lo[1], 0)]; hi = [Math.max(hi[0], scene.grid.cols * sq), Math.max(hi[1], scene.grid.rows * sq)];
  }
  if (!b && !scene.grid) { lo = [0, 0]; hi = [140, 140]; }
  const top = Math.max(0, ...ps.filter((p) => p.at).map((p) => (p.bottomMm || 0))) + 14;
  return { lo: [lo[0], -hi[1], -7], hi: [hi[0], -lo[1], top] };
}

/** What the view shows now, worked out from the model (null: no tile). */
export function describe() {
  const t = model.tileNow.value;
  if (!t) return null;
  const { mode, pick } = view.get();
  const kind = model.mapOn.value ? 'map' : model.platesOn.value ? 'plates' : 'tile';
  const d = { doc: t.id, key: `${kind}:${t.path}`, kind, view: mode, asked: pick, fresh: t.meshVersion == null, plan: planOf(t), box: null, focus: null };
  if (kind === 'map' && model.scene.value) {
    const ps = model.pieces.value;
    d.box = frameBox(ps, model.scene.value);
    // Close frames the selected piece, or the first one placed
    const p = model.selected.value[0] ?? ps.find((q) => q.at);
    d.focus = p?.at ? M.worldBox(p, 14, 7) : null;
  }
  return d;
}

/** Wire this part of the page (once, at load). */
export function init() {
  // the preset the camera is on, if the user's own view does not hold: the viewer says it, nothing stores it
  view.derive('activeView', () => { const f = viewer.framing.value; return f.held ? null : f.view; });
  effect(() => {
    const d = describe();
    untracked(() => viewer.describe(d));
  });
}
