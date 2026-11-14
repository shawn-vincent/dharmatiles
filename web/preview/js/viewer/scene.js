// The viewer's scene: a map's pieces, each its tile's mesh placed by a matrix, in the one 3D view.
//
// Methods of `Viewer`, installed on its prototype by `viewer.js`: they run with `this` the viewer.  The page
// (editor/scene.js) decides which pieces there are, which tier of each tile's mesh is loaded and where each
// piece stands; this draws them, picks the one under a point, tints and outlines the selection, and frames
// what is there.  A geometry is a tile mesh at a tier, shared by every piece of that tile at that seed (a
// piece only holds its matrix, so twelve copies of one grass tile are one mesh in memory).  The shaders are
// the tile view's (consts.js) with the model matrix put in; a piece's number rides in a per-piece uniform,
// so a geometry has no per-piece attribute.

import * as THREE from 'three';
import { FRAG, PICK_FRAG, reduceMotion } from './consts.js';
import { ALL, CROSSFADE_MS, intervals, newFade, running, swap } from './crossfade.js';

const VERT_S = `
attribute vec3 paint; uniform vec2 uWho;
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
void main() {
  vec4 w = modelMatrix * vec4(position, 1.0);
  vN = mat3(modelMatrix) * normal; vC = paint; vP = w.xyz; vWho = uWho;
  gl_Position = projectionMatrix * viewMatrix * w;
}`;
// The scene's fragment shader is the tile's with a dither in front: a mesh draws only the pixels whose dither
// value (an 8 by 8 Bayer pattern in screen space) is in its interval uXf = [lo, hi), so the meshes of a crossfade
// share the pixels exactly (crossfade.js); at rest the interval is [0, 2) and nothing is discarded.
const DITHER = `
uniform vec2 uXf;
float bayer(vec2 p) {
  vec2 q = floor(p);
  float v = 0.0;
  for (int i = 0; i < 3; i++) {
    vec2 b = mod(q, 2.0);
    v = v * 4.0 + b.x * 2.0 + b.y * 3.0 - 4.0 * b.x * b.y;
    q = floor(q / 2.0);
  }
  return (v + 0.5) / 64.0;
}
void main() {
  float d = bayer(gl_FragCoord.xy);
  if (d < uXf.x || d >= uXf.y) discard;`;
const FRAG_S = FRAG.replace('void main() {', DITHER);
const MASK_S = 'void main() { gl_FragColor = vec4(1.0); }';
const grey = (n) => new Uint8Array(n).fill(168);

/** The map's ground, just under z = 0 so a tile's own surface at zero does not fight it. */
const GROUND_MASK_Z = -0.05;

export const sceneMethods = {
  initScene() {
    this.sceneOn = false;
    this.sceneGroup = new THREE.Group();
    this.scenePickGroup = new THREE.Group();
    this.sceneMaskGroup = new THREE.Group();
    this.sceneGeoms = new Map();    // gid -> {geometry, painted, clean, triangles, grey}
    this.scenePieces = new Map();   // piece index -> {gid, shown, render, pick, mask, ghosts, role}
    this.sceneFaded = false;        // the fade's passes are drawn instead of the pieces' own (something is selected, or on its way in or out)
    this.tileHighlight = null;      // the tile's selection and hover, held while the map is on show
    this.sceneBox = null;           // {lo, hi}: the map's box, from the description (describe)
    this.sceneGridSpec = null;      // {sq, w, h, cx, cy}
    this.sceneHeld = new Set();     // geometries dropped while a crossfade still draws them
    this.crossfadeMs = CROSSFADE_MS;
    this.crossfadeRaf = 0;
    this.sceneUniforms = {
      uCam: this.uniforms.uCam, uDir: this.uniforms.uDir, uOrtho: this.uniforms.uOrtho, uL1: this.uniforms.uL1, uL2: this.uniforms.uL2, uAccent: this.uniforms.uAccent,
      uSel: { value: new THREE.Vector2(-1, -1) }, uHov: { value: new THREE.Vector2(-1, -1) },
      uKeep: { value: new THREE.Vector2(-1, -1) }, uLeave: { value: new THREE.Vector2(-1, -1) }, uLift: { value: new THREE.Vector2(-1, -1) }, uDrop: { value: new THREE.Vector2(-1, -1) },
      uKeepA: { value: 1 }, uLeaveA: { value: 1 }, uLiftA: { value: 1 }, uDropA: { value: 1 }, uTintSel: { value: 1 },
      uMode: { value: 0 }, uGhost: { value: 1 },
    };
    this.sceneSel = new Set();
    this.sceneHov = new Set();
    // The ground is solid: a plane at the ground drawn first into depth only (the backdrop is its own pass, and
    // paints it), both sides, so from above it hides what is below it (a connector at bottom 0) and from below it
    // hides what stands on it (seen from underground you see the earth, never a tile's top through it).
    this.groundMask = new THREE.Mesh(
      new THREE.PlaneGeometry(2e5, 2e5),
      new THREE.MeshBasicMaterial({ colorWrite: false, depthWrite: true, side: THREE.DoubleSide }),
    );
    this.groundMask.renderOrder = -1;
    this.groundMask.frustumCulled = false;
    this.groundMask.position.z = GROUND_MASK_Z;
    this.groundMask.visible = false;
    this.scene.add(this.groundMask); // the map's and the tile editor's alike, so not in the map's group
    this.scene.add(this.sceneGroup);
    this.pickScene.add(this.scenePickGroup);
    this.maskScene.add(this.sceneMaskGroup);
    this.sceneGroup.visible = this.scenePickGroup.visible = this.sceneMaskGroup.visible = false;
  },

  /** Show the map's pieces instead of the tile (or the plate): the tile's own mesh steps aside. */
  setSceneOn(on) {
    if (this.sceneOn === !!on) return;
    this.sceneOn = !!on;
    this.sceneGroup.visible = this.scenePickGroup.visible = this.sceneMaskGroup.visible = this.sceneOn;
    for (const g of this.ghosts) g.visible = g.visible && !this.sceneOn;
    if (this.mesh) this.mesh.visible = !this.sceneOn && !this.stale;
    // the map and the tile take turns at the fade: each starts from rest with its own selection
    const u = this.uniforms;
    if (this.sceneOn) {
      this.tileHighlight = { sel: u.uSel.value.clone(), hov: u.uHov.value.clone() };
      u.uSel.value.set(-1, -1); u.uHov.value.set(-1, -1);
      this.resetFade();
    } else {
      this.sceneFaded = false;
      this.showSceneFade(false);
      this.resetFade();
      if (this.tileHighlight) { u.uSel.value.copy(this.tileHighlight.sel); u.uHov.value.copy(this.tileHighlight.hov); this.tileHighlight = null; }
      this.applyFade();
    }
    this.buildGrid();
    this.placeGround();
    this.draw();
  },

  /** The ground grid the scene draws: squares of `sq` mm over a `w` by `h` map centred at (cx, cy). */
  setSceneGrid(spec) { this.sceneGridSpec = spec; if (this.sceneOn) { this.buildGrid(); this.draw(); } },

  /** A tile mesh at a tier, a parsed payload, kept under `gid` (replacing what was). */
  setSceneGeometry(gid, payload) {
    this.dropSceneGeometry(gid);
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(payload.positions, 3));
    g.setAttribute('normal', new THREE.BufferAttribute(payload.normals, 3));
    const painted = new THREE.BufferAttribute(payload.painted, 3, true);
    const clean = new THREE.BufferAttribute(payload.clean, 3, true);
    const entry = { geometry: g, painted, clean, triangles: payload.triangles, grey: null, meta: payload.meta, held: 0, dropped: false };
    g.setAttribute('paint', this.scenePaint(entry));
    this.sceneGeoms.set(gid, entry);
    // pieces that use it now
    for (const p of this.scenePieces.values()) if (p.gid === gid) this.useGeometry(p, gid);
  },

  dropSceneGeometry(gid) {
    const e = this.sceneGeoms.get(gid);
    if (!e) return;
    this.sceneGeoms.delete(gid);
    // a crossfade still draws it: it goes when the last one lets it go
    if (e.held > 0) { e.dropped = true; this.sceneHeld.add(e); } else e.geometry.dispose();
  },

  /** A crossfade is done with a geometry: it is freed if it was dropped meanwhile. */
  releaseSceneEntry(e) {
    e.held--;
    if (e.held === 0 && e.dropped) { this.sceneHeld.delete(e); e.geometry.dispose(); }
  },

  /** The paint to show for a geometry, as `currentPaint` does for the tile's mesh. */
  scenePaint(e) {
    if (this.showPaint) return this.showOverhang ? e.painted : e.clean;
    if (!e.grey) {
      const flagged = new Uint8Array(e.clean.array.length);
      const [p, c] = [e.painted.array, e.clean.array];
      for (let i = 0; i < p.length; i += 3) {
        if (p[i] !== c[i] || p[i + 1] !== c[i + 1] || p[i + 2] !== c[i + 2]) flagged.set(p.subarray(i, i + 3), i);
        else flagged.set([168, 168, 170], i);
      }
      e.grey = { plain: new THREE.BufferAttribute(grey(c.length), 3, true), flagged: new THREE.BufferAttribute(flagged, 3, true) };
    }
    return this.showOverhang ? e.grey.flagged : e.grey.plain;
  },

  /** Paint or overhang was switched: every geometry follows. */
  refreshScenePaint() {
    for (const e of [...this.sceneGeoms.values(), ...this.sceneHeld]) e.geometry.setAttribute('paint', this.scenePaint(e));
  },

  /**
   * Choose the backdrop (`wilderness`, `cavern`, `table`, `plain`): it is the place the tiles stand in, for the map
   * and for a tile alike; the plates keep the printer's bed (the page asks for `plain` there).
   */
  setBackdropKind(kind) {
    this.backdrop.setKind(kind);
    this.backdrop.setGrid(this.showGrid);
    if (this.grid) this.grid.visible = this.showGrid && !this.backdrop.on();
    this.placeGround();
    this.draw();
  },

  /** Daylight or dusk: the dark theme's backdrop is evening. */
  setBackdropNight(n) { this.backdrop.setNight(n); this.draw(); },

  /**
   * The ground under what is shown, with a soft shadow round each footprint.  The map's ground is at z = 0 and
   * hides what is below it (a tile's connector at bottom 0); the tile's own is at its lowest point, so its
   * connector shows while it is edited; a plate keeps the bed.
   */
  placeGround() {
    this.groundMask.visible = false;
    this.groundOn = false;
    if (!this.backdrop.on()) return;
    let z = 0, box = null, sq = null;
    const rects = [];
    if (this.sceneOn) {
      z = 0;
      box = this.sceneBox;
      sq = this.sceneGridSpec?.sq;
      for (const p of this.scenePieces.values()) {
        if (!p.shown) continue;
        const g = p.render.geometry;
        if (!g.boundingBox) g.computeBoundingBox();
        const b = g.boundingBox;
        const mat = p.render.matrix;
        // the footprint of the tile's box as the matrix puts it: its four floor corners
        const xs = [], ys = [];
        for (const [x, y] of [[b.min.x, b.min.y], [b.max.x, b.min.y], [b.max.x, b.max.y], [b.min.x, b.max.y]]) {
          const e = mat.elements;
          xs.push(e[0] * x + e[4] * y + e[12]); ys.push(e[1] * x + e[5] * y + e[13]);
        }
        rects.push([Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)]);
      }
    } else if (this.payload) {
      const { lo, hi } = this.payload.meta;
      z = lo[2];
      box = { lo, hi };
      sq = this.plan?.squareMm;
      const [w, h] = this.plan?.sizeMm ?? [hi[0] - lo[0], hi[1] - lo[1]];
      rects.push([0, 0, w, h]);
    } else if (this.plan?.sizeMm) {
      const [w, h] = this.plan.sizeMm;
      box = { lo: [0, 0, 0], hi: [w, h, 1] };
      sq = this.plan.squareMm;
      z = -7;
    }
    // the ground is ruled at the program's square size; without it there is no ground to draw
    if (!box || !(sq > 0)) return;
    const gz = this.sceneOn ? GROUND_MASK_Z : z - 0.05;
    this.groundMask.position.z = gz;
    this.groundMask.visible = true;
    this.groundOn = true;
    this.groundZ = gz;
    this.backdrop.setGround(box, sq, gz);
    this.backdrop.setRects(rects);
  },

  /** The geometry ids kept, with their triangles. */
  sceneGeometryIds() { return [...this.sceneGeoms.keys()]; },
  sceneTriangles(gid) { return this.sceneGeoms.get(gid)?.triangles ?? 0; },

  useGeometry(p, gid) {
    const e = this.sceneGeoms.get(gid);
    if (!e) return;
    for (const m of [p.render, p.pick, p.mask, ...p.ghosts]) m.geometry = e.geometry;
    p.gid = gid;
    p.entry = e;
  },

  /**
   * A piece that shows a mesh changes to another: the old one stays drawn, under the new, while the dither hands
   * its pixels over (crossfade.js).  Reduced motion, or nothing shown before, swaps at once.
   */
  swapGeometry(p, gid) {
    const from = p.shown && p.entry && p.entry.geometry === p.render.geometry ? p.entry : null;
    this.useGeometry(p, gid);
    if (!from || p.entry === from || reduceMotion()) { this.clearFade(p); return; }
    // the outgoing mesh draws in every pass the piece does, with the piece's own `uWho` and role
    const o = { ...this.sceneLayer(from.geometry, p.who, p.role, { value: new THREE.Vector2(0, ALL) }), entry: from };
    o.xf = o.render.material.uniforms.uXf;
    for (const m of [o.render, ...o.ghosts]) m.matrix.copy(p.render.matrix);
    from.held++;
    this.sceneGroup.add(o.render, ...o.ghosts);
    p.outs.unshift(o);
    this.showPiece(p);
    swap(p.fade, performance.now(), from, p.entry);
    this.trimOuts(p);
    this.crossfadeTick();
  },

  /**
   * One layer of a piece: its own mesh and the fade's four passes (the tile's: the selection first, then the rest),
   * the model matrix put in; they share one dither interval `xf`, so the layer is dithered in every pass it draws.
   */
  sceneLayer(geometry, who, role, xf) {
    const mk = (uniforms, uWho, extra = {}) => {
      const m = new THREE.Mesh(geometry, new THREE.ShaderMaterial({ uniforms: { ...uniforms, uWho, uXf: xf }, vertexShader: VERT_S, fragmentShader: FRAG_S, side: THREE.DoubleSide, ...extra }));
      m.frustumCulled = false; m.matrixAutoUpdate = false;
      return m;
    };
    const render = mk(this.sceneUniforms, who);
    const ghosts = [[this.lit, true], [this.lit, false], [this.ghost, true], [this.ghost, false]].map(([pair, depth], k) => {
      const m = mk(pair.u, role, depth ? { colorWrite: false } : { transparent: true, depthWrite: false });
      m.renderOrder = 1 + k;
      return m;
    });
    for (const m of [render, ...ghosts]) m.visible = false;
    return { render, ghosts };
  },

  /** The outgoing meshes a piece holds are as many as its layers less the newest. */
  trimOuts(p) {
    while (p.outs.length > p.fade.layers.length - 1 && p.outs.length > 0) this.dropOut(p.outs.pop());
  },

  dropOut(o) {
    this.sceneGroup.remove(o.render, ...o.ghosts);
    for (const m of [o.render, ...o.ghosts]) m.material.dispose();
    this.releaseSceneEntry(o.entry);
  },

  /** Take a piece's crossfade away at once. */
  clearFade(p) {
    for (const o of p.outs) this.dropOut(o);
    p.outs.length = 0;
    p.fade.layers.length = 0;
    p.xf.value.set(0, ALL);
  },

  /** Whether any piece is mid-crossfade. */
  sceneFading() { for (const p of this.scenePieces.values()) if (running(p.fade)) return true; return false; },

  /** Step every crossfade to now; keep stepping, a frame at a time, only while one runs. */
  crossfadeTick() {
    if (this.crossfadeRaf) return;
    const step = (now) => {
      this.crossfadeRaf = 0;
      let on = false;
      for (const p of this.scenePieces.values()) {
        if (!p.fade.layers.length) continue;
        const iv = intervals(p.fade, now, this.crossfadeMs);
        this.trimOuts(p);
        p.xf.value.set(iv[0][0], iv[0][1]);
        p.outs.forEach((o, i) => o.xf.value.set(iv[i + 1][0], iv[i + 1][1]));
        if (running(p.fade)) on = true; else p.fade.layers.length = 0;
      }
      this.draw();
      if (on) this.crossfadeRaf = requestAnimationFrame(step);
    };
    this.crossfadeRaf = requestAnimationFrame(step);
  },

  /**
   * The pieces drawn: `[{index, gid, matrix}]` (gid null: nothing yet; the piece is not drawn).  A piece the
   * list no longer has goes; one that is there keeps its meshes and only takes its new matrix and geometry.
   */
  setScenePieces(list) {
    const keep = new Set(list.map((p) => p.index));
    for (const [i, p] of this.scenePieces) {
      if (keep.has(i)) continue;
      this.clearFade(p);
      this.sceneGroup.remove(p.render, ...p.ghosts); this.scenePickGroup.remove(p.pick); this.sceneMaskGroup.remove(p.mask);
      for (const m of [p.render, p.pick, p.mask, ...p.ghosts]) m.material.dispose();
      this.scenePieces.delete(i);
    }
    for (const { index, gid, matrix } of list) {
      let p = this.scenePieces.get(index);
      if (!p) {
        const who = { value: new THREE.Vector2(index + 1, 0) };
        const mk = (frag) => new THREE.ShaderMaterial({ uniforms: { uWho: who }, vertexShader: VERT_S, fragmentShader: frag, side: THREE.DoubleSide });
        const empty = new THREE.BufferGeometry();
        // a piece's role in the fade rides in its own `uWho`: 1 selected, 2 under the pointer, 3 the rest (setSceneHighlight)
        const role = { value: new THREE.Vector2(3, 0) };
        const xf = { value: new THREE.Vector2(0, ALL) }; // the crossfade's share of the dither (crossfade.js)
        const { render, ghosts } = this.sceneLayer(empty, who, role, xf);
        p = {
          gid: null, shown: false, role, who, xf, entry: null,
          fade: newFade(),    // the crossfade's layers, newest first: the piece's own meshes, then `outs`
          outs: [],           // the meshes on their way out: {render, ghosts, xf, entry}
          render, ghosts,
          pick: new THREE.Mesh(empty, mk(PICK_FRAG)),
          mask: new THREE.Mesh(empty, mk(MASK_S)),
        };
        for (const m of [p.pick, p.mask]) { m.frustumCulled = false; m.matrixAutoUpdate = false; }
        p.mask.visible = this.sceneSel.has(index);
        p.role.value.x = this.roleOf(index);
        this.sceneGroup.add(p.render, ...ghosts); this.scenePickGroup.add(p.pick); this.sceneMaskGroup.add(p.mask);
        this.scenePieces.set(index, p);
      }
      for (const m of [p.render, p.pick, p.mask, ...p.ghosts, ...p.outs.flatMap((o) => [o.render, ...o.ghosts])]) m.matrix.fromArray(matrix);
      const shown = gid != null && this.sceneGeoms.has(gid);
      const wasShown = p.shown;
      p.shown = shown;
      this.showPiece(p);
      p.mask.visible = shown && this.sceneSel.has(index);
      if (shown && p.gid !== gid) { if (wasShown) this.swapGeometry(p, gid); else { this.clearFade(p); this.useGeometry(p, gid); } }
      else if (!shown) { this.clearFade(p); p.gid = gid; }
    }
    this.draw();
  },

  /** What a piece is to the fade: 1 selected, 2 under the pointer (and not selected), 3 the rest. */
  roleOf(i) { return this.sceneSel.has(i) ? 1 : this.sceneHov.has(i) ? 2 : 3; },

  /** A piece is on show as itself, or as the fade's passes while something is selected or on its way in or out. */
  showPiece(p) {
    p.render.visible = p.shown && !this.sceneFaded;
    for (const g of p.ghosts) g.visible = p.shown && this.sceneFaded;
    p.pick.visible = p.shown;
    for (const o of p.outs) {
      o.render.visible = p.shown && !this.sceneFaded;
      for (const g of o.ghosts) g.visible = p.shown && this.sceneFaded;
    }
  },

  /** The fade says whether the pieces are drawn as themselves or through its passes (fade.js `showFade`). */
  showSceneFade(on) {
    if (this.sceneFaded === on) return;
    this.sceneFaded = on;
    for (const p of this.scenePieces.values()) this.showPiece(p);
  },

  /**
   * The selected pieces (a Set of indices) and the ones the pointer is on (a Set, or one index; -1: none).
   * A selection fades the rest, as a selection in the tile does: the fade's own machinery (fade.js `applyFade`)
   * is told "something is selected" and "something is under the pointer", and each piece is given its role in it.
   * Where the fade is off, the selection's tint (one piece) and the hover's remain, as they were.
   */
  setSceneHighlight(sel, hov) {
    this.sceneSel = new Set(sel);
    this.sceneHov = hov instanceof Set ? hov : new Set(hov >= 0 ? [hov] : []);
    const hovered = [...this.sceneHov].filter((i) => !this.sceneSel.has(i));
    // the fade off: one selected piece tints; several are outlined (the shader tints one index)
    this.sceneUniforms.uSel.value.set(this.sceneSel.size === 1 ? [...this.sceneSel][0] : -1, -1);
    this.sceneUniforms.uHov.value.set(hovered.length ? hovered[0] : -1, -1);
    for (const [i, p] of this.scenePieces) {
      p.role.value.x = this.roleOf(i);
      p.mask.visible = this.sceneSel.has(i) && p.shown;
    }
    // the fade's: role 1 is the selection, role 2 the pointer's
    const u = this.uniforms;
    u.uSel.value.set(this.sceneSel.size ? 0 : -1, -1);
    u.uHov.value.set(hovered.length ? 1 : -1, -1);
    this.applyFade();
    this.draw();
  },

  /**
   * A rectangle on the bed (`{x0, y0, x1, y1}`, mm) in the accent, a translucent fill with its edge: where a piece
   * being dragged on a plate would stand.  Null takes it away.
   */
  setGhostRect(r) {
    if (this.ghostRectGroup) {
      this.scene.remove(this.ghostRectGroup);
      this.ghostRectGroup.traverse((o) => { o.geometry?.dispose(); if (o.material) { this.fatMaterials.delete(o.material); o.material.dispose(); } });
      this.ghostRectGroup = null;
    }
    if (r) {
      const group = new THREE.Group();
      const z = 0.5;
      const m = new THREE.Mesh(new THREE.PlaneGeometry(r.x1 - r.x0, r.y1 - r.y0), new THREE.MeshBasicMaterial({ color: this.uniforms.uAccent.value, transparent: true, opacity: 0.28, depthWrite: false, depthTest: false, side: THREE.DoubleSide }));
      m.position.set((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2, z);
      m.renderOrder = 40; m.frustumCulled = false;
      group.add(m);
      const c = [[r.x0, r.y0], [r.x1, r.y0], [r.x1, r.y1], [r.x0, r.y1]];
      const pos = new Float32Array(24), rgba = new Float32Array(32);
      const a = this.uniforms.uAccent.value;
      for (let k = 0; k < 4; k++) {
        const [p, q] = [c[k], c[(k + 1) % 4]];
        pos.set([p[0], p[1], z, q[0], q[1], z], 6 * k);
        for (const v of [0, 1]) rgba.set([a.r, a.g, a.b, 1], 8 * k + 4 * v);
      }
      const edge = this.fatLines(pos, rgba, 2.5);
      edge.renderOrder = 41; edge.frustumCulled = false;
      group.add(edge);
      this.ghostRectGroup = group;
      this.scene.add(group);
    }
    this.draw();
  },

  /** The piece under canvas point (x, y), CSS pixels, or -1. */
  scenePick(x, y) {
    const h = this.pick(x, y);
    return h ? h.entry : -1;
  },
};
