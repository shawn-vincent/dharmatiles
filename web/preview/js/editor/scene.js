// The map in the 3D view (docs/maps.html): a `.dmap` with placed pieces opens as a map, and the view shows
// every piece's tile at its place, turned and raised, on the ground of the chosen backdrop.
//
// The program says what the map is (the scene: each piece's tile, seed, system, place, turn and height, in
// `tile.plan.scene`); this module draws it.  A piece is its tile's mesh at one of three tiers (the program's
// `/api/tiles/ID/tier`, from its cache or built): super-low far away, low in the middle, full close up and
// for what you select, chosen by how large the piece is on screen and held under a triangle budget, so a
// table of forty pieces moves smoothly and nothing waits for a build.  Full is asked for only when close,
// one at a time, and never blocks: the lower tier stays on screen until it lands.  What is not full is
// marked `low · not yet checked` in the status line, as the plates mark theirs.
//
// The pieces of the map are one selection (`view.msel`, piece indices); editing them is scene-edit.js, the
// panel is scene-panel.js.  Mode (Tile, Map, Plates) is the title bar's switch: a document with placed
// pieces opens as a map, a print list opens on its plates, and the choice is kept for the session.

import * as THREE from 'three';
import { parsePayload } from '../payload.js';
import { isOffline } from '../host.js';
import { showError } from '../problems.js';
import { FOV } from '../viewer/consts.js';
import * as M from '../map-model.js';
import { $, host, pickView, viewer } from './app.js';
import { createStore, isPlates, server, tile, view } from './stores.js';

/** What the map is now: the pieces (the program's scene, plus the page's), the tiers drawn, and the budget. */
export const sceneStore = createStore({ pieces: [], scene: null, counts: M.tierCounts([]), triangles: 0, busy: 0 });

const chosen = new Map();     // document path -> 'map' | 'plates' (the session's choice)
const geoms = new Map();      // gkey -> {tiers: Map(tier -> {tris}), asked: Set(tier), failed: Set(tier)}
const cameras = new Map();    // document path -> {camera, activeView}
let mounted = false;          // the scene is in the view
let mountedPath = null;
let local = null;             // {rev, at}: the scene a call answered with, until the program's update says the same
let lastSig = '';
let evalTimer = 0;
const lanes = { fast: 0, full: 0 };
const queue = [];

/** Is the map of the document in view (the title bar's Map)? */
export const mapMode = () => isPlates(tile()) && view.get().dmode === 'map';

export const pieces = () => sceneStore.get().pieces;
export const sceneNow = () => sceneStore.get().scene;
export const selected = () => view.get().msel.map((i) => pieces().find((p) => p.index === i)).filter(Boolean);
export const pieceAt = (index) => pieces().find((p) => p.index === index) ?? null;

// ---- which scene: the program's, or the one a call just answered with ------------------------------

/**
 * The scene to show: the one in the tile's plan, unless a call has just answered with a newer one (its `rev`
 * is the hash of the text it wrote): then that, until the program's update carries the same `rev` (it has
 * planned the new text), or a few seconds pass (the program's word is the truth).
 */
function currentScene() {
  const t = tile();
  const inc = isPlates(t) ? t.plan?.scene ?? null : null;
  if (local) {
    if (inc && inc.rev === local.scene.rev) local = null;
    else if (Date.now() - local.at > 4000) local = null;
    else return local.scene;
  }
  return inc;
}

/** A call answered with the map as it now reads: show it at once. */
export function takeScene(scene) {
  if (!scene) return;
  local = { scene, at: Date.now() };
  sync();
}

// ---- the mode ----------------------------------------------------------------------------------------

function modeFor(t) {
  if (!isPlates(t)) return null;
  const c = chosen.get(t.path);
  if (c) return c;
  const sc = t.plan?.scene;
  return sc ? (sc.map ? 'map' : 'plates') : null;
}

/** The title bar's Map or Plates: the same document, the other view. */
export function chooseMode(mode) {
  const t = tile();
  if (!isPlates(t)) return;
  chosen.set(t.path, mode);
  view.set({ dmode: mode });
}

/** Open `path` (a map) in `mode` the next time it is shown. */
export const setChosen = (path, mode) => chosen.set(path, mode);

// ---- the pieces ---------------------------------------------------------------------------------------

const gkeyOf = (p, scene) => {
  const t = server.get().tiles.find((x) => x.id === p.tile_id);
  return `${p.tile_id}|${p.seed ?? ''}|${p.system}|${scene.printer}|${t?.mesh_version ?? 0}`;
};
const gidOf = (gkey, tier) => `${gkey}#${tier}`;

function augment(scene) {
  return scene.pieces.map((p) => ({ ...p, rot: p.rot ?? 0, gkey: p.tile_id && !p.error ? gkeyOf(p, scene) : null }));
}

/** The world box the view frames: the pieces' footprints and the grid, the ground to a little over the tallest. */
function frameBox(ps, scene) {
  const b = M.bounds(ps);
  let lo = b ? [...b.lo] : [0, 0], hi = b ? [...b.hi] : [0, 0];
  if (scene.grid) {
    const sq = ps.find((p) => p.sq)?.sq ?? 35;
    lo = [Math.min(lo[0], 0), Math.min(lo[1], 0)]; hi = [Math.max(hi[0], scene.grid.cols * sq), Math.max(hi[1], scene.grid.rows * sq)];
  }
  if (!b && !scene.grid) { lo = [0, 0]; hi = [140, 140]; }
  const top = Math.max(0, ...ps.filter((p) => p.at).map((p) => (p.height_mm || 0))) + 14;
  return { lo: [lo[0], -hi[1], -7], hi: [hi[0], -lo[1], top] };
}

function sync() {
  const t = tile();
  const want = modeFor(t);
  if (view.get().dmode !== want && (want || !isPlates(t))) view.set({ dmode: want });
  const on = mapMode();
  const sc = on ? currentScene() : null;
  if (!on || !sc) {
    if (mounted) unmount();
    if (on) sceneStore.set({ pieces: [], scene: null });
    return;
  }
  if (!mounted || mountedPath !== t.path) mount(t, sc);
  const ps = augment(sc);
  sceneStore.set({ pieces: ps, scene: sc });
  // a piece that is gone leaves the selection
  const live = new Set(ps.map((p) => p.index));
  const keep = view.get().msel.filter((i) => live.has(i));
  if (keep.length !== view.get().msel.length) view.set({ msel: keep });
  const box = frameBox(ps, sc);
  viewer.setSceneBox(box);
  const sq = ps.find((p) => p.sq)?.sq ?? 35;
  viewer.setSceneGrid({ sq, w: Math.max(box.hi[0] - box.lo[0], sq), h: Math.max(box.hi[1] - box.lo[1], sq), cx: (box.lo[0] + box.hi[0]) / 2, cy: (box.lo[1] + box.hi[1]) / 2, z: -7 });
  viewer.backdrop?.setGround?.(box, sq);
  request(ps);
  evaluate(true);
}

function mount(t, sc) {
  if (mounted && mountedPath && mountedPath !== t.path) unmount();
  mounted = true;
  mountedPath = t.path;
  document.body.classList.add('map');
  viewer.setSceneOn(true);
  viewer.frameFor = frameFor;
  viewer.maxDistance = 4000;
  viewer.controls.maxDistance = 4000;
  viewer.setSceneBox(frameBox(augment(sc), sc));
  // the camera: the one this map had, else Iso over all of it
  const saved = cameras.get(t.path);
  if (saved) { viewer.setCamera(saved.camera); view.set({ activeView: saved.activeView }); } else pickView('painted', false);
  lastSig = '';
}

function unmount() {
  const t = mountedPath;
  if (t && viewer.controls) cameras.set(t, { camera: viewer.getCamera(), activeView: view.get().activeView });
  mounted = false;
  mountedPath = null;
  document.body.classList.remove('map');
  viewer.setScenePieces([]);
  viewer.setSceneOn(false);
  viewer.frameFor = null;
  viewer.controls.maxDistance = 600;
  viewer.maxDistance = null;
  view.set({ msel: [], mprop: null });
  $('tileplan').textContent = '';
  // the plates (or the tile) frame themselves again
  pickView(view.get().mode, false);
}

/** Close frames the selected piece, or the first; the other presets frame all of the map. */
function frameFor(name) {
  if (!mounted || name !== 'close') return null;
  const p = selected()[0] ?? pieces().find((q) => q.at);
  if (!p?.at) return null;
  return M.worldBox(p, 14);
}

// ---- the tiers ----------------------------------------------------------------------------------------

/** Ask for what the pieces want at least: every tile's super-low, so nothing is a box for long. */
function request(ps) {
  for (const p of ps) {
    if (!p.gkey || !p.at) continue;
    ensure(p, 'super-low');
  }
}

function entry(gkey) {
  let e = geoms.get(gkey);
  if (!e) { e = { tiers: new Map(), asked: new Set(), failed: new Set() }; geoms.set(gkey, e); }
  return e;
}

/** Fetch piece `p`'s tile at `tier` if it is not here, asked for or refused. */
function ensure(p, tier) {
  const e = entry(p.gkey);
  if (e.tiers.has(tier) || e.asked.has(tier) || e.failed.has(tier)) return;
  e.asked.add(tier);
  queue.push({ p, tier, gkey: p.gkey });
  pump();
}

function pump() {
  for (let i = 0; i < queue.length;) {
    const j = queue[i];
    const lane = j.tier === 'full' ? 'full' : 'fast';
    if (lanes[lane] >= (lane === 'full' ? 1 : 2)) { i += 1; continue; }
    queue.splice(i, 1);
    lanes[lane] += 1;
    sceneStore.set({ busy: lanes.fast + lanes.full + queue.length });
    run(j).finally(() => { lanes[lane] -= 1; sceneStore.set({ busy: lanes.fast + lanes.full + queue.length }); pump(); });
  }
}

async function run({ p, tier, gkey }) {
  const e = entry(gkey);
  try {
    const buf = await host.fetchTier(p.tile_id, { tier, seed: p.seed ?? undefined, system: p.system, printer: sceneNow()?.printer });
    const payload = parsePayload(buf);
    // the tile moved on while it came (another build, another map): what came is not kept
    if (!geoms.has(gkey)) return;
    e.tiers.set(tier, { tris: payload.triangles, checked: payload.meta.checked !== false, failures: payload.meta.failures ?? [] });
    viewer.setSceneGeometry(gidOf(gkey, tier), payload);
    evaluate(true);
  } catch (err) {
    e.asked.delete(tier);
    if (isOffline(err)) return; // offline: asked again with the next look
    e.failed.add(tier);
    showError(`Could not load ${p.name} at ${tier} detail`, err);
  }
}

const sizeOf = (p) => Math.max(...p.size);

/** A piece's size on screen, CSS pixels, by its longer side at its distance; 0 when it is out of the view. */
function screenSize(p) {
  const c = M.toWorld(p, p.size[0] / 2, p.size[1] / 2, 6);
  const h = viewer.canvas.clientHeight || 1;
  const mm = sizeOf(p);
  const px = viewer.isOrtho
    ? M.pxOfOrtho(mm, viewer.orthoHalf / viewer.camera.zoom, h)
    : M.pxOfPerspective(mm, viewer.camera.position.distanceTo(new THREE.Vector3(c[0], c[1], c[2])), FOV, h);
  const s = viewer.project(c);
  const w = viewer.canvas.clientWidth || 1;
  const out = s.x < -px || s.x > w + px || s.y < -px || s.y > h + px;
  return out ? 0 : px;
}

/** The tile of a box, for a piece whose tile has not come: its footprint, a hand high, in the plate's grey. */
function boxPayload(w, d, h) {
  const v = [[0, 0, 0], [w, 0, 0], [w, d, 0], [0, d, 0], [0, 0, h], [w, 0, h], [w, d, h], [0, d, h]];
  const faces = [[0, 1, 2, 3, [0, 0, -1]], [4, 7, 6, 5, [0, 0, 1]], [0, 4, 5, 1, [0, -1, 0]], [1, 5, 6, 2, [1, 0, 0]], [2, 6, 7, 3, [0, 1, 0]], [3, 7, 4, 0, [-1, 0, 0]]];
  const pos = [], nor = [];
  for (const [a, b, c, e, n] of faces) for (const k of [a, b, c, a, c, e]) { pos.push(...v[k]); nor.push(...n); }
  const colour = new Uint8Array(pos.length).fill(170);
  return { positions: new Float32Array(pos), normals: new Float32Array(nor), painted: colour, clean: colour, triangles: 12, meta: {} };
}

/**
 * Work out which tier each piece is drawn at and put the pieces in the view.  `now`: at once (a mesh came,
 * the scene changed); else after the camera has rested a moment.  Asks for the full builds of what is close,
 * one at a time.
 */
export function evaluate(now = false) {
  clearTimeout(evalTimer);
  if (!now) { evalTimer = setTimeout(() => evaluate(true), 140); return; }
  if (!mounted) return;
  const ps = pieces().filter((p) => p.at);
  const sel = new Set(view.get().msel);
  const px = ps.map(screenSize);
  const want = ps.map((p, i) => M.wantedTier(px[i], sel.has(p.index)));
  const have = ps.map((p) => new Set(p.gkey ? [...(geoms.get(p.gkey)?.tiers.keys() ?? [])] : []));
  const tris = ps.map((p) => (tier) => geoms.get(p.gkey)?.tiers.get(tier)?.tris ?? M.GUESS[tier]);
  const { tiers, total } = M.chooseTiers(want, px, have, tris);
  const list = ps.map((p, i) => {
    const t = tiers[i];
    let gid = null;
    if (p.gkey && t) gid = gidOf(p.gkey, t);
    else {
      gid = `box|${p.size[0]}|${p.size[1]}`;
      if (!viewer.sceneGeoms.has(gid)) viewer.setSceneGeometry(gid, boxPayload(p.size[0], p.size[1], 8));
    }
    return { index: p.index, gid, matrix: M.matrixOf(p) };
  });
  viewer.setScenePieces(list);
  // what is drawn that is not the checked build, and what a close piece still wants in full
  const counts = M.tierCounts(tiers);
  sceneStore.set({ counts, triangles: total });
  const order = ps.map((_, i) => i).filter((i) => want[i] === 'full' && tiers[i] !== 'full' && ps[i].gkey).sort((a, b) => px[b] - px[a]);
  // the budget decides what is shown, not what is fetched: only the nearest one is asked for at a time
  if (order.length && total < M.BUDGET) ensure(ps[order[0]], 'full');
  for (const i of ps.map((_, k) => k)) {
    if (want[i] === 'low' && !have[i].has('low') && ps[i].gkey) ensure(ps[i], 'low');
  }
  viewer.setSceneHighlight(sel, hovered);
}

/** The camera moved: look again once it rests. */
function cameraMoved() {
  if (!mounted) return;
  const c = viewer.camera, t = viewer.controls.target;
  const sig = [c.position.x, c.position.y, c.position.z, t.x, t.y, t.z, c.zoom, viewer.canvas.clientWidth, viewer.canvas.clientHeight].map((v) => v.toFixed(1)).join(',');
  if (sig === lastSig) return;
  lastSig = sig;
  evaluate(false);
}

// ---- the status line and the chip ---------------------------------------------------------------------

/** What the status line says of the map: `{verdict, counts}`. */
export function statusLines() {
  const { counts, pieces: ps } = sceneStore.get();
  const placed = ps.filter((p) => p.at);
  const inv = server.get().inventory;
  const verdict = `${placed.length} piece${placed.length === 1 ? '' : 's'} · ${M.totalsText(ps, inv)}`;
  const notFull = counts.low + counts['super-low'] + counts.none;
  const parts = [];
  if (notFull) parts.push('low · not yet checked');
  const tiers = [counts.full && `${counts.full} full`, counts.low && `${counts.low} low`, counts['super-low'] && `${counts['super-low']} distant`, counts.none && `${counts.none} loading`].filter(Boolean).join(' · ');
  return { verdict, counts: [...parts, tiers].filter(Boolean).join(' · '), notChecked: notFull > 0 };
}

/** The title bar's chip: `3 in inventory · 11 to print`. */
export const chip = () => M.totalsText(pieces(), server.get().inventory);

// ---- hover and click ---------------------------------------------------------------------------------

let hovered = -1;
let down = null;

/** Set the piece under the pointer. */
function hover(i) {
  if (hovered === i) return;
  hovered = i;
  viewer.setSceneHighlight(view.get().msel, hovered);
  $('canvas').classList.toggle('grab', i >= 0);
}

/** Select pieces (indices); `add` toggles one into the selection. */
export function select(indices, add = false) {
  const cur = view.get().msel;
  const next = add ? (indices.length === 1 && cur.includes(indices[0]) ? cur.filter((i) => i !== indices[0]) : [...new Set([...cur, ...indices])]) : indices;
  view.set({ msel: next, mprop: next.length ? view.get().mprop : null });
}

const rect = () => $('canvas').getBoundingClientRect();

function onPointerDown(e) {
  if (!mapMode()) return;
  down = { x: e.clientX, y: e.clientY, t: performance.now(), b: e.button, shift: e.shiftKey };
}

function onPointerUp(e) {
  const d = down;
  down = null;
  if (!mapMode() || !d || d.b !== 0 || sceneEdit.dragged) return;
  if (Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4 || performance.now() - d.t > 600) return;
  const r = rect();
  const i = viewer.scenePick(e.clientX - r.left, e.clientY - r.top);
  if (i >= 0) select([i], d.shift);
  else if (!d.shift) select([]);
}

let moveRaf = 0, lastMove = null;
function onPointerMove(e) {
  if (!mapMode() || e.buttons || e.pointerType === 'touch') return;
  lastMove = e;
  if (moveRaf) return;
  moveRaf = requestAnimationFrame(() => {
    moveRaf = 0;
    const r = rect();
    hover(viewer.scenePick(lastMove.clientX - r.left, lastMove.clientY - r.top));
  });
}

/** What edit.js sets while a drag is on (a click that follows a drag selects nothing). */
export const sceneEdit = { dragged: false };

// ---- wiring --------------------------------------------------------------------------------------------

/** Wire this part of the page (once, at load). */
export function init() {
  view.set({ dmode: null, msel: [], mprop: null });
  const c = $('canvas');
  c.addEventListener('pointerdown', onPointerDown);
  c.addEventListener('pointerup', onPointerUp);
  c.addEventListener('pointermove', onPointerMove);
  c.addEventListener('pointerleave', () => hover(-1));
  const prev = viewer.onRender;
  viewer.onRender = () => { prev?.(); cameraMoved(); };
  server.subscribe(['tiles', 'inventory'], sync);
  view.subscribe(['tileId', 'dmode'], sync);
  view.subscribe(['msel'], () => { evaluate(true); });
  // a theme or a display switch repaints the pieces' colours
  sceneStore.subscribe(['pieces'], () => { $('tileplan').textContent = mapMode() ? chip() : $('tileplan').textContent; });
  sync();
}
