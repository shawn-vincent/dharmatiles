// @ts-check
// One thing alone (docs/preview.html, "One thing alone"): a rock, a tree, a flower, a definition of one, drawn at a seed in its own
// frame with no tile and no base, in a view of its own beside the tile's, with a strip of draws, the room it takes and its controls.
//
// What is shown is worked out, never stored: the cursor in a `define`, or the selected entry, is the thing (`targetOf`), the source
// the page holds is what is drawn, and a change to any of them draws it again.  That is how the tile's own view follows an edit
// (`doc.text` is what the draft is of), so the thing view follows the same signals; the one call, `buildThing`, is superseded by
// the next of its kind in the host (js/host.js LATEST), so a stale draw is never taken.  A draw is not a tile's draft: it is not
// built by the tile's draft lane, which is for tiles, and the call needs no lane (one thing builds in a fraction of a second).
//
// The view is a second `Viewer` on its own canvas, the same class and paint as the tile's, so overhang is red here as everywhere.

import * as THREE from 'three';
import { computed, effect, signal, untracked } from '@preact/signals-core';
import { Viewer } from '../viewer.js';
import { parsePayload } from '../payload.js';
import { token } from '../tokens.js';
import { failed, showError } from '../problems.js';
import { drawMesh } from './scene-thumbs.js';
import { $, host } from './app.js';
import { doc, elementNamed, isInventory, isPlates, itemOf, server, tile, view } from './stores.js';
import { texts } from './document.js';
import { STRIP, bytesOf, frameOf, stripSeeds, targetOf } from '../thing-model.js';
import { html, render } from '../view/html.js';
import { ThingPane } from '../view/thing-pane.js';

/** What the view holds of one draw: its payload, and what it came to. */
/** @typedef {{ seed: number, url: string, triangles: number }} Pic */
/** @typedef {{ element: string, shown: import('../api').ThingControl[], extent: import('../api').ThingExtent, draws: import('../api').ThingDraw[] }} Drawn */

/** The cursor in the source, as an index into it. */
const caret = signal(0);
/** The thing view is the view (the switch's Thing), not the tile's. */
export const shown = signal(false);
/** The seed the view draws at. */
export const seed = signal(1);
/** Which twelve seeds the strip shows. */
export const page = signal(0);
/** Which of a scatter's things is drawn. */
export const child = signal(0);
/** What the last draw of the view came to: null before the first, and after the thing goes. */
export const drawn = signal(/** @type {Drawn | null} */ (null));
/** The strip's pictures, in the order of the seeds. */
export const pics = signal(/** @type {Pic[]} */ ([]));
/** Why there is no thing to show (a mistake in the source, a layer): the program's words. */
export const problem = signal(/** @type {string | null} */ (null));
/** The view shows a draw the gate has not checked yet (a low one): the pane offers to draw it in full. */
export const later = signal(false);
/** A draw is being made. */
export const busy = signal(false);

/** The thing the last cursor or selection was on, kept while the thing view is the view: typing moves the cursor out of the nodes the last check read, and the view must not drop the thing under the edit. */
const held = signal(/** @type {import('../thing-model.js').Target | null} */ (null));

/** The thing the cursor or the selection is on now (null: none). */
const on = computed(() => {
  const t = tile();
  const d = doc.get(), v = view.get();
  const it = itemOf(v.sel) ?? null;
  return targetOf({
    thingFile: !!t && !isPlates(t) && !isInventory(t),
    face: v.face,
    item: it,
    generator: it ? elementNamed(it.element)?.generator ?? null : null,
    child: child.value,
    nodes: d.nodes,
    things: d.names?.things ?? null,
    caret: caret.value,
  });
});

/** The thing the page is on: the one under the cursor or the selection, or, with the thing view the view, the one it was last on. */
export const target = computed(() => on.value ?? (shown.value ? held.value : null));

/** The key of the thing, so a new one starts from the first seed and the first page: what is drawn is a different thing. */
const keyOf = (/** @type {import('../thing-model.js').Target | null} */ t) => (t ? JSON.stringify(t.which) : '');

/** @type {Viewer | null} */
let viewer = null;
/** @type {THREE.Group | null} */
let outlines = null;

/** The thing view's own viewer, made when first asked for. */
export function viewerOf() {
  viewer ??= new Viewer(/** @type {HTMLCanvasElement} */ ($('thing-canvas')));
  return viewer;
}

/** Ask the viewer to frame what it shows (`preset` a name of VIEWS: a fresh ask each time). @param {string} key @param {string} [preset] */
export function describe(key, preset) {
  const v = viewerOf();
  v.describe({ doc: 'thing', key: `thing:${key}`, kind: 'tile', view: preset ?? 'painted', asked: preset ? { name: preset, animate: true } : null, fresh: false, plan: null, box: null, focus: null });
}
/** The preset the thing view's camera is on, for the pane's buttons. */
export const lookFrom = () => viewerOf().framing;

/** The smallest and the largest box, drawn as two outlines round the origin (the smallest in ink, the largest in the accent). @param {import('../api').ThingExtent} e */
function drawOutlines(e) {
  const v = viewerOf();
  if (outlines) { v.scene.remove(outlines); for (const o of outlines.children) { /** @type {THREE.LineSegments} */ (o).geometry.dispose(); } }
  outlines = new THREE.Group();
  /** The twelve edges of a box standing on z = 0 and lying round the origin. @param {{ x: number, y: number, z: number }} b @param {string} colour */
  const box = (b, colour) => {
    const [x, y] = [b.x / 2, b.y / 2];
    const c = [[-x, -y, 0], [x, -y, 0], [x, y, 0], [-x, y, 0], [-x, -y, b.z], [x, -y, b.z], [x, y, b.z], [-x, y, b.z]];
    const edges = [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]];
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(edges.flatMap(([i, j]) => [...c[i], ...c[j]])), 3));
    return new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: colour }));
  };
  outlines.add(box(e.least, token('--plate-ink') || '#2B3340'), box(e.most, token('--accent') || '#27796a'));
  v.scene.add(outlines);
}

/** @param {import('../api').ThingDraw} d @param {import('../api').ThingExtent} e */
function payloadOf(d, e) {
  const p = parsePayload(bytesOf(d.mesh));
  // the camera frames the mesh and the largest it can be: the frame does not move from one seed to the next
  const f = frameOf(e.most);
  p.meta.lo = [Math.min(p.meta.lo[0], f.lo[0]), Math.min(p.meta.lo[1], f.lo[1]), Math.min(p.meta.lo[2], f.lo[2])];
  p.meta.hi = [Math.max(p.meta.hi[0], f.hi[0]), Math.max(p.meta.hi[1], f.hi[1]), Math.max(p.meta.hi[2], f.hi[2])];
  return p;
}

let generation = 0;
/** How long a low draw may take for the gate's draw to follow it at once, ms. */
const QUICK_MS = 400;
/** Draws the view in full: made by the last run, so it is of that run's source and seed (a newer run replaces it). */
let fullNow = /** @type {(() => Promise<void>) | null} */ (null);
/** @type {ReturnType<typeof setTimeout> | null} */
let timer = null;

/** What a refusal says: the program's own words. @param {{ body?: any, status: number }} r */
function wordsOf(r) {
  const e = r.body?.error;
  const m = typeof e === 'string' ? e : e?.message;
  const first = r.body?.errors?.[0];
  return first?.message ?? m ?? `the program answered ${r.status}`;
}

/**
 * Draw the thing: the view at `seed` and the strip.  Each is its own call, and a newer call of its kind replaces an older (the host
 * says so with status 0), so the answer taken is always of the newest source.
 * @param {import('../thing-model.js').Target} t @param {number} at @param {number} pageAt
 */
async function run(t, at, pageAt) {
  const mine = ++generation;
  fullNow = null;
  const tl = tile(), d = doc.get();
  if (!tl || d.text == null) return;
  const base = { text: d.text, file: d.file === tl.path ? undefined : (d.file ?? undefined), which: t.which };
  busy.value = true;
  /** One call of the verb: the answer when it is of the newest ask and says yes, else null (a refusal is said where the pane shows it). @param {number[]} seeds @param {'low' | 'full'} tier */
  const ask = async (seeds, tier) => {
    const r = await host.buildThing(tl.id, { ...base, seeds, tier });
    if (mine !== generation || r.status === 0) return null; // a newer ask took its place, or the program is away
    if (failed('Could not draw the thing', r)) { problem.value = null; return null; }
    if (r.status !== 200) { problem.value = wordsOf(r); return null; }
    problem.value = null;
    return /** @type {import('../api').ThingRes} */ (r.body);
  };
  /** The view shows a draw: the viewer is told what it shows first (it drops what it holds of another document), then the mesh. @param {import('../api').ThingRes} res */
  const show = (res) => {
    drawn.value = res;
    const v = viewerOf();
    describe(keyOf(t));
    v.setMesh(payloadOf(res.draws[0], res.extent));
    drawOutlines(res.extent);
  };
  /** The view again at the gate's tier, replacing the low one only if the source and the seed are still the ones drawn. */
  const full = async () => {
    busy.value = true;
    try {
      const res = await ask([at], 'full');
      if (res) { show(res); later.value = false; }
    } catch (e) {
      showError('Could not draw the thing in full', e);
    } finally {
      if (mine === generation) busy.value = false;
    }
  };
  try {
    // quick first, as a tile's draft is: the view and the strip at a low tier.  A thing that draws quickly is then drawn at the
    // gate's tier at once; one that does not (a tree takes tens of seconds there, a rock a fraction of one) waits to be asked
    // (`drawFull`), so the next edit is not made to wait behind it
    const began = performance.now();
    const low = await ask([at], 'low');
    if (!low) return;
    const quick = performance.now() - began < QUICK_MS;
    show(low);
    later.value = true;
    fullNow = full;
    const many = await ask(stripSeeds(pageAt), 'low');
    if (!many) return;
    const box = frameOf(many.extent.most);
    pics.value = many.draws.map((x) => ({ seed: x.seed, triangles: x.triangles, url: drawMesh(bytesOf(x.mesh), box) }));
    if (quick) await full();
  } catch (e) {
    showError('Could not draw the thing', e);
  } finally {
    if (mine === generation) busy.value = false;
  }
}

/** Draw the thing in view at the gate's tier now (the draft's button). */
export function drawFull() {
  if (fullNow) void fullNow();
}

/** Wire this part of the page (once, at load). */
export function init() {
  const prior = texts.cb.onCaret;
  texts.cb.onCaret = () => { prior?.(); caret.value = texts.selectionStart; };
  effect(() => { const t = on.value; if (t) untracked(() => { held.value = t; }); });
  // another tile holds none of the last tile's things: what is held is let go, and the thing view stays only where the new tile's own
  // cursor or selection is on a thing (the first edit of a library tile makes a copy of it, a tile of its own, with the selection kept)
  let tileWas = '';
  effect(() => {
    const id = tile()?.id ?? '';
    untracked(() => { if (id !== tileWas) { tileWas = id; held.value = null; } });
  });
  // a new thing starts from the first seed, the first page and the first of a scatter's things
  let was = '';
  effect(() => {
    const now = keyOf(target.value);
    untracked(() => {
      if (now !== was) { was = now; seed.value = 1; page.value = 0; pics.value = []; drawn.value = null; problem.value = null; }
    });
  });
  // the switch is offered where there is a thing; with none, the tile's view is the view
  const sw = $('thing-switch');
  effect(() => {
    const has = target.value != null;
    sw.hidden = !has;
    if (!has && shown.value) shown.value = false;
  });
  for (const b of sw.querySelectorAll('button')) b.addEventListener('click', () => { shown.value = b.dataset.thing === 'thing'; });
  effect(() => {
    const on = shown.value;
    for (const b of sw.querySelectorAll('button')) b.setAttribute('aria-pressed', String(on === (b.dataset.thing === 'thing')));
    $('thingview').hidden = !on;
    $('view').classList.toggle('thing-on', on);
  });
  // live: the source, the target, the seed, the page and the build target are what a draw is of
  effect(() => {
    const t = target.value, on = shown.value;
    const at = seed.value, pageAt = page.value;
    void doc.get().text; void doc.get().file; void server.get().system; void server.get().printer; void tile()?.id;
    if (timer) clearTimeout(timer);
    if (!on || !t) return;
    timer = setTimeout(() => { void run(t, at, pageAt); }, 150);
  });
  render(html`<${ThingPane} m=${{ target, shown, seed, page, child, drawn, pics, problem, busy, later, drawFull, describe, lookFrom, STRIP }} />`, $('thing-panel'));
}
