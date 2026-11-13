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
import { FRAG, PICK_FRAG } from './consts.js';

const VERT_S = `
attribute vec3 paint; uniform vec2 uWho;
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
void main() {
  vec4 w = modelMatrix * vec4(position, 1.0);
  vN = mat3(modelMatrix) * normal; vC = paint; vP = w.xyz; vWho = uWho;
  gl_Position = projectionMatrix * viewMatrix * w;
}`;
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
    this.scenePieces = new Map();   // piece index -> {gid, render, pick, mask}
    this.sceneBox = null;           // {lo, hi}: what is framed
    this.sceneGridSpec = null;      // {sq, w, h, cx, cy}
    this.sceneUniforms = {
      uCam: this.uniforms.uCam, uDir: this.uniforms.uDir, uOrtho: this.uniforms.uOrtho, uL1: this.uniforms.uL1, uL2: this.uniforms.uL2, uAccent: this.uniforms.uAccent,
      uSel: { value: new THREE.Vector2(-1, -1) }, uHov: { value: new THREE.Vector2(-1, -1) },
      uKeep: { value: new THREE.Vector2(-1, -1) }, uLeave: { value: new THREE.Vector2(-1, -1) }, uLift: { value: new THREE.Vector2(-1, -1) }, uDrop: { value: new THREE.Vector2(-1, -1) },
      uKeepA: { value: 1 }, uLeaveA: { value: 1 }, uLiftA: { value: 1 }, uDropA: { value: 1 }, uTintSel: { value: 1 },
      uMode: { value: 0 }, uGhost: { value: 1 },
    };
    this.sceneSel = new Set();
    this.sceneHov = -1;
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
    if (!this.sceneOn) this.applyFade();
    this.buildGrid();
    this.placeGround();
    this.draw();
  },

  /** What a view frames when the scene is on: `{lo, hi}` in the world, mm. */
  setSceneBox(box) { this.sceneBox = box; this.reframe?.(); },

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
    const entry = { geometry: g, painted, clean, triangles: payload.triangles, grey: null, meta: payload.meta };
    g.setAttribute('paint', this.scenePaint(entry));
    this.sceneGeoms.set(gid, entry);
    // pieces that use it now
    for (const p of this.scenePieces.values()) if (p.gid === gid) this.useGeometry(p, gid);
  },

  dropSceneGeometry(gid) {
    const e = this.sceneGeoms.get(gid);
    if (!e) return;
    this.sceneGeoms.delete(gid);
    e.geometry.dispose();
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
    for (const e of this.sceneGeoms.values()) e.geometry.setAttribute('paint', this.scenePaint(e));
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
    if (!this.backdrop.on()) return;
    let z = 0, box = null, sq = 35;
    const rects = [];
    if (this.sceneOn) {
      z = 0;
      box = this.sceneBox;
      sq = this.sceneGridSpec?.sq ?? 35;
      for (const p of this.scenePieces.values()) {
        if (!p.render.visible) continue;
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
      sq = this.plan?.square_mm ?? 35;
      const [w, h] = this.plan?.size_mm ?? [hi[0] - lo[0], hi[1] - lo[1]];
      rects.push([0, 0, w, h]);
    } else if (this.plan?.size_mm) {
      const [w, h] = this.plan.size_mm;
      box = { lo: [0, 0, 0], hi: [w, h, 1] };
      sq = this.plan.square_mm ?? 35;
      z = -7;
    }
    if (!box) return;
    const gz = this.sceneOn ? GROUND_MASK_Z : z - 0.05;
    this.groundMask.position.z = gz;
    this.groundMask.visible = true;
    this.backdrop.setGround(box, sq, gz);
    this.backdrop.setRects(rects);
  },

  /** The geometry ids kept, with their triangles. */
  sceneGeometryIds() { return [...this.sceneGeoms.keys()]; },
  sceneTriangles(gid) { return this.sceneGeoms.get(gid)?.triangles ?? 0; },

  useGeometry(p, gid) {
    const e = this.sceneGeoms.get(gid);
    if (!e) return;
    for (const m of [p.render, p.pick, p.mask]) m.geometry = e.geometry;
    p.gid = gid;
  },

  /**
   * The pieces drawn: `[{index, gid, matrix}]` (gid null: nothing yet; the piece is not drawn).  A piece the
   * list no longer has goes; one that is there keeps its meshes and only takes its new matrix and geometry.
   */
  setScenePieces(list) {
    const keep = new Set(list.map((p) => p.index));
    for (const [i, p] of this.scenePieces) {
      if (keep.has(i)) continue;
      this.sceneGroup.remove(p.render); this.scenePickGroup.remove(p.pick); this.sceneMaskGroup.remove(p.mask);
      p.render.material.dispose(); p.pick.material.dispose(); p.mask.material.dispose();
      this.scenePieces.delete(i);
    }
    for (const { index, gid, matrix } of list) {
      let p = this.scenePieces.get(index);
      if (!p) {
        const who = { value: new THREE.Vector2(index + 1, 0) };
        const mk = (frag, extra = {}) => new THREE.ShaderMaterial({ uniforms: { ...extra, uWho: who }, vertexShader: VERT_S, fragmentShader: frag, side: THREE.DoubleSide });
        const empty = new THREE.BufferGeometry();
        p = {
          gid: null,
          render: new THREE.Mesh(empty, mk(FRAG, this.sceneUniforms)),
          pick: new THREE.Mesh(empty, mk(PICK_FRAG)),
          mask: new THREE.Mesh(empty, mk(MASK_S)),
        };
        for (const m of [p.render, p.pick, p.mask]) { m.frustumCulled = false; m.matrixAutoUpdate = false; }
        p.mask.visible = this.sceneSel.has(index);
        this.sceneGroup.add(p.render); this.scenePickGroup.add(p.pick); this.sceneMaskGroup.add(p.mask);
        this.scenePieces.set(index, p);
      }
      for (const m of [p.render, p.pick, p.mask]) m.matrix.fromArray(matrix);
      const shown = gid != null && this.sceneGeoms.has(gid);
      p.render.visible = p.pick.visible = shown;
      p.mask.visible = shown && this.sceneSel.has(index);
      if (shown && p.gid !== gid) this.useGeometry(p, gid);
      else if (!shown) p.gid = gid;
    }
    this.draw();
  },

  /** The selected pieces (a Set of indices) and the one under the pointer (-1: none). */
  setSceneHighlight(sel, hov) {
    this.sceneSel = new Set(sel);
    this.sceneHov = hov;
    // one selected piece tints; several are outlined (the shader tints one index)
    this.sceneUniforms.uSel.value.set(this.sceneSel.size === 1 ? [...this.sceneSel][0] : -1, -1);
    this.sceneUniforms.uHov.value.set(hov, -1);
    for (const [i, p] of this.scenePieces) p.mask.visible = this.sceneSel.has(i) && p.render.visible;
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
