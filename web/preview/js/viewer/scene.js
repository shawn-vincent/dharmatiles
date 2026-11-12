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

  /** The piece under canvas point (x, y), CSS pixels, or -1. */
  scenePick(x, y) {
    const h = this.pick(x, y);
    return h ? h.entry : -1;
  },
};
