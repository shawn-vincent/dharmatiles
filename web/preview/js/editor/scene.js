// The map in the 3D view (docs/maps.html): a `.dmap` with placed pieces opens as a map, and the view shows
// every piece's tile at its place, turned and raised, on the ground of the chosen backdrop.
//
// The program says what the map is (the scene: each piece's tile, seed, system, place, turn and bottom, in the
// document's state, js/documents.js); this module draws it.  A piece is its tile's mesh at one of three tiers (the program's
// `/api/tiles/ID/tier`, from its cache or built): super-low far away, low in the middle, full close up and
// for what you select, chosen by how large the piece is on screen and held under a triangle budget, so a
// table of forty pieces moves smoothly and nothing waits for a build.  Full is asked for only when close,
// one at a time, and never blocks: the lower tier stays on screen until it lands.  What is not full is
// marked `low · not yet checked` in the status line, as the plates mark theirs.
//
// The pieces of the map are one selection (`view.msel`, piece ids); editing them is scene-edit.js, the
// panel is scene-panel.js.  Mode (Tile, Map, Plates) is the title bar's switch: a document with placed
// pieces opens as a map, a print list opens on its plates, and the choice is kept for the session.

import * as THREE from 'three';
import { parsePayload } from '../payload.js';
import { isOffline } from '../host.js';
import { showError } from '../problems.js';
import { FOV } from '../viewer/consts.js';
import * as M from '../map-model.js';
import { $, host, pickView, viewer } from './app.js';
import { piecesOf, selectedOf, chipOf, verdictOf } from '../doc-views.js';
import { createStore, isPlates, log, server, tile, tileById, view } from './stores.js';
import { docs } from './docs.js';
import { applyMode, chooseMode, init as initMode, setChosen } from './mode.js';

export { chooseMode, setChosen };

/** How the map is drawn: the tiers in use and the budget.  The map itself (its pieces) is the document's state, read when asked. */
export const sceneStore = createStore({ counts: M.tierCounts([]), triangles: 0, busy: 0 });

const geoms = new Map();      // gkey -> {tiers: Map(tier -> {tris}), asked: Set(tier), failed: Set(tier)}
const cameras = new Map();    // document path -> {camera, activeView}
let mounted = false;          // the scene is in the view
let mountedPath = null;
let lastSig = '';
let evalTimer = 0;
const lanes = { fast: 0, full: 0 };
const shown = new Map();      // piece id -> {gid, tileId}: what it was last drawn with, kept until its new mesh comes
/** Pieces being dragged: id -> [x, y] mm, shown there until the change is written. */
export const live = new Map();
const queue = [];

/** Is the map of the document in view (the title bar's Map)? */
export const mapMode = () => isPlates(tile()) && view.get().dmode === 'map';

/** The map's scene as the program last said it (null when the map is not the view). */
export const sceneNow = () => (mapMode() ? docs.scene(tile().id) : null);
/** The pieces of the map now: worked out from the document's state each time it is asked. */
export const pieces = () => piecesOf(sceneNow(), server.get().tiles);
/** The selected pieces: the selection's ids that the map has now. */
export const selected = () => selectedOf(view.get().msel, pieces());
export const pieceAt = (id) => pieces().find((p) => p.id === id) ?? null;

// ---- the pieces ---------------------------------------------------------------------------------------

const gidOf = (gkey, tier) => `${gkey}#${tier}`;

/** The world box the view frames: the pieces' footprints and the grid, the ground to a little over the tallest. */
function frameBox(ps, scene) {
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

/** Put the map in the view as the document's state says it is now (or take it out when the map is not the view). */
function sync() {
  applyMode();
  const t = tile();
  const on = mapMode();
  const sc = on ? docs.scene(t.id) : null;
  if (!on || !sc) {
    if (mounted) unmount();
    return;
  }
  if (!mounted || mountedPath !== t.path) mount(t, sc);
  const ps = pieces();
  release(ps);
  const box = frameBox(ps, sc);
  viewer.setSceneBox(box);
  const sq = ps.find((p) => p.sq)?.sq ?? 35;
  viewer.setSceneGrid({ sq, w: Math.max(box.hi[0] - box.lo[0], sq), h: Math.max(box.hi[1] - box.lo[1], sq), cx: (box.lo[0] + box.hi[0]) / 2, cy: (box.lo[1] + box.hi[1]) / 2, z: 0 });
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
  viewer.setSceneBox(frameBox(pieces(), sc));
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
  // the plates (or the tile) frame themselves again
  pickView(view.get().mode, false);
}

/** Close frames the selected piece, or the first; the other presets frame all of the map. */
function frameFor(name) {
  if (!mounted || name !== 'close') return null;
  const p = selected()[0] ?? pieces().find((q) => q.at);
  if (!p?.at) return null;
  return { ...M.worldBox(p, 14, 7), fit: true };
}

// ---- the tiers ----------------------------------------------------------------------------------------

/** Ask for what the pieces want at least: every tile's super-low, so nothing is a box for long. */
function request(ps) {
  for (const p of ps) {
    if (!p.gkey || !p.at) continue;
    ensure(p, 'super-low');
  }
}

/** Let go of the geometry no piece of the scene uses any more: the page's record, its queued and the viewer's meshes. */
function release(ps) {
  const stale = new Set(M.staleKeys(geoms.keys(), ps));
  if (!stale.size) return;
  for (const g of stale) {
    for (const tier of geoms.get(g).tiers.keys()) viewer.dropSceneGeometry(gidOf(g, tier));
    geoms.delete(g);
  }
  for (let i = queue.length - 1; i >= 0; i--) if (stale.has(queue[i].gkey)) queue.splice(i, 1);
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
  const e = geoms.get(gkey);
  if (!e) return; // let go of while it waited
  try {
    const docRev = tileById(p.tileId)?.docRev;
    const buf = await host.fetchTier(p.tileId, { tier, docRev, seed: p.seed ?? undefined, system: p.system, printer: sceneNow()?.printer });
    if (!buf) { e.asked.delete(tier); return; } // the tile is at another revision: asked again with the next look
    const payload = parsePayload(buf);
    // the tile moved on while it came (another build, another map, another revision of its document): what came is not kept
    if (geoms.get(gkey) !== e) return;
    if (docRev != null && payload.meta.docRev !== docRev) { e.asked.delete(tier); return; }
    if (tileById(p.tileId)?.docRev !== docRev) { e.asked.delete(tier); return; }
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
  const want = ps.map((p, i) => M.wantedTier(px[i], sel.has(p.id)));
  const have = ps.map((p) => new Set(p.gkey ? [...(geoms.get(p.gkey)?.tiers.keys() ?? [])] : []));
  const tris = ps.map((p) => (tier) => geoms.get(p.gkey)?.tiers.get(tier)?.tris ?? M.GUESS[tier]);
  const { tiers, total } = M.chooseTiers(want, px, have, tris);
  const list = ps.map((p, i) => {
    const t = tiers[i];
    let gid = null;
    if (p.gkey && t) { gid = gidOf(p.gkey, t); shown.set(p.id, { gid, tileId: p.tileId }); }
    else if (shown.get(p.id)?.tileId === p.tileId && viewer.sceneGeoms.has(shown.get(p.id).gid)) gid = shown.get(p.id).gid; // a new variant: the old one stays until it swaps in
    else {
      gid = `box|${p.size[0]}|${p.size[1]}`;
      if (!viewer.sceneGeoms.has(gid)) viewer.setSceneGeometry(gid, boxPayload(p.size[0], p.size[1], 8));
    }
    // the view names a piece by its place in this list (a slot); `drawn` is how a slot is read back
    return { index: i, gid, matrix: M.matrixOf(live.has(p.id) ? { ...p, at: live.get(p.id) } : p) };
  });
  drawn = ps.map((p) => p.id);
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
  viewer.setSceneHighlight(slotsOf([...sel]), hovered);
  viewer.placeGround();
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
  const { counts } = sceneStore.get();
  const verdict = verdictOf(pieces(), server.get().inventory);
  const notFull = counts.low + counts['super-low'] + counts.none;
  const parts = [];
  if (notFull) parts.push('low · not yet checked');
  const tiers = [counts.full && `${counts.full} full`, counts.low && `${counts.low} low`, counts['super-low'] && `${counts['super-low']} distant`, counts.none && `${counts.none} loading`].filter(Boolean).join(' · ');
  const notes = (log.view(tile()?.id ?? '').counts?.notes ?? 0);
  return { verdict, counts: [...parts, tiers].filter(Boolean).join(' · '), notChecked: notFull > 0, notes };
}

/** The title bar's chip: `3 in inventory · 11 to print`. */
export const chip = () => chipOf(pieces(), server.get().inventory);

// ---- pills on the pieces -------------------------------------------------------------------------------

let pillsOn = true;
const tags = new Map(); // piece id -> its element

/** A small pill over each piece the program has a word for: have (on your shelf), to print, or the caution of the other colour mode. */
function drawTags() {
  const box = $('mpills');
  if (!mounted || !pillsOn) { if (box.childElementCount) { box.replaceChildren(); tags.clear(); } return; }
  const ps = pieces().filter((p) => p.at && !p.error);
  const states = M.pills(ps, server.get().inventory);
  const live2 = new Set();
  for (const p of ps) {
    const st = states.get(p.id);
    if (!st) continue;
    const c = M.toWorld(p.at ? { ...p, at: live.get(p.id) ?? p.at } : p, p.size[0] / 2, p.size[1] / 2, 14);
    const px = screenSize(p);
    if (px < 34) continue;
    const s = viewer.project(c);
    let e = tags.get(p.id);
    if (!e) { e = document.createElement('span'); box.append(e); tags.set(p.id, e); }
    const word = { have: 'have', need: 'to print', other: 'have, other colour', skip: 'left off' }[st.state];
    const cls = `mp-tag ${st.state === 'have' ? 'have' : st.state === 'other' ? 'caution' : 'need'}`;
    if (e.className !== cls) e.className = cls;
    if (e.textContent !== word) e.textContent = word;
    e.style.transform = `translate(${s.x.toFixed(1)}px, ${s.y.toFixed(1)}px) translate(-50%, -100%)`;
    live2.add(p.id);
  }
  for (const [i, e] of tags) if (!live2.has(i)) { e.remove(); tags.delete(i); }
}

// ---- hover and click ---------------------------------------------------------------------------------

let hovered = -1;       // the slot under the pointer
let down = null;
let drawn = [];         // the ids of the pieces in the order the view was last given them: the view names a piece by its place here
const slotsOf = (ids) => new Set(drawn.flatMap((id, i) => (ids.includes(id) ? [i] : [])));
const idAt = (slot) => (slot >= 0 ? drawn[slot] ?? null : null);
/** The id of the piece the view's pick `slot` names (what scene-drag.js asks after a pick). */
export const pickAt = (x, y) => idAt(viewer.scenePick(x, y));

/** Set the piece under the pointer. */
function hover(i) {
  if (hovered === i) return;
  hovered = i;
  viewer.setSceneHighlight(slotsOf(view.get().msel), hovered);
  $('canvas').classList.toggle('grab', i >= 0);
}

/** Select pieces (ids); `add` toggles one into the selection. */
export function select(ids, add = false) {
  const cur = view.get().msel;
  const next = add ? (ids.length === 1 && cur.includes(ids[0]) ? cur.filter((i) => i !== ids[0]) : [...new Set([...cur, ...ids])]) : ids;
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
  const i = pickAt(e.clientX - r.left, e.clientY - r.top);
  if (i != null) select([i], d.shift);
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
  viewer.onRenderHook(() => { cameraMoved(); drawTags(); });
  try { pillsOn = localStorage.getItem('dt-pills') !== '0'; } catch { /* private mode: the default stands */ }
  $('t-pills').checked = pillsOn;
  $('t-pills').onchange = (e) => { pillsOn = e.target.checked; try { localStorage.setItem('dt-pills', pillsOn ? '1' : '0'); } catch { /* private mode */ } viewer.draw(); };
  server.subscribe(['tiles', 'inventory'], sync);
  view.subscribe(['tileId', 'dmode'], sync);
  view.subscribe(['msel'], () => { evaluate(true); });
  // a theme or a display switch repaints the pieces' colours
  // the document's state changed (a reply, the stream): the map is drawn from it again
  docs.subscribe(sync);
  initMode();
  sync();
}
