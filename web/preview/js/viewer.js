// The 3D view: one class, its parts in `viewer/` (the shaders and numbers in
// `consts.js`; the camera, the fade and the layers as methods installed below).

import * as THREE from 'three';
import { ownerVertices, worstFlagged } from './payload.js';
import { FADE, FOV, FRAG, MASK_FRAG, OUTLINE_FRAG, OUTLINE_PX, OUTLINE_VERT, PICK_FRAG, POINT_FRAG, SKETCH_PARTS, SKETCH_ROUGH, VERT, dirv } from './viewer/consts.js';
import { cameraMethods } from './viewer/camera.js';
import { fadeMethods } from './viewer/fade.js';
import { layersMethods } from './viewer/layers.js';

export { VIEWS } from './viewer/consts.js';

export class Viewer {
  constructor(canvas) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
    this.renderer.setClearColor(0x000000, 0); // the plate is the page's backdrop behind the canvas
    this.scene = new THREE.Scene();
    this.persp = new THREE.PerspectiveCamera(FOV, 1, 1, 1000);
    this.ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, 1, 1000);
    for (const c of [this.persp, this.ortho]) c.up.set(0, 0, 1); // tiles are z-up
    this.orthoHalf = 1; // half the frustum's height at zoom 1, mm
    this.isOrtho = false;
    this.camera = this.persp;
    this.makeControls(new THREE.Vector3());
    this.initInertia();
    this.uniforms = {
      uCam: { value: new THREE.Vector3() }, uDir: { value: new THREE.Vector3(0, 1, 0) }, uOrtho: { value: 0 }, uL1: { value: new THREE.Vector3() }, uL2: { value: new THREE.Vector3() },
      uSel: { value: new THREE.Vector2(-1, -1) }, uHov: { value: new THREE.Vector2(-1, -1) }, uKeep: { value: new THREE.Vector2(-1, -1) }, uLeave: { value: new THREE.Vector2(-1, -1) }, uLift: { value: new THREE.Vector2(-1, -1) }, uDrop: { value: new THREE.Vector2(-1, -1) },
      uKeepA: { value: 1 }, uLeaveA: { value: 1 }, uLiftA: { value: 1 }, uDropA: { value: 1 }, uTintSel: { value: 0 }, uAccent: { value: new THREE.Color(0.15, 0.48, 0.38) },
    };
    this.material = new THREE.ShaderMaterial({
      uniforms: { ...this.uniforms, uMode: { value: 0 }, uGhost: { value: 1 } }, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide,
    });
    // With a selection, the rest of the tile fades: drawn after it, first
    // into depth alone, then translucent, so only its nearest surface shows
    // and the selection is seen through it.
    this.fade = true;
    this.stale = false; // the mesh shown is an older build's: all of it fades
    // how much shows now (1 solid, FADE faded) of the rest, the selection
    // and the thing it just left; each eases to its own target
    this.fx = { rest: 1, keep: 1, leave: 1, lift: 1, drop: 1 };
    this.fxTarget = { ...this.fx };
    this.fadeTween = null;
    this.regionLayer = null; // {focus, group}: the selected region flat at zero, or every region faint (setRegionLayer)
    const pair = (alpha, mode) => {
      const u = { ...this.uniforms, uMode: { value: mode }, uGhost: { value: alpha } };
      return {
        u,
        depth: new THREE.ShaderMaterial({ uniforms: u, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide, colorWrite: false }),
        colour: new THREE.ShaderMaterial({ uniforms: u, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide, transparent: true, depthWrite: false }),
      };
    };
    this.ghost = pair(FADE, 2);
    this.ghostDepth = this.ghost.depth;
    this.ghostColour = this.ghost.colour;
    // the selection, drawn before the rest so it shows through it
    this.lit = pair(1, 4);
    this.ghosts = [];
    // while a build runs: each stage's parts, and rough stand-ins, faded
    this.sketchLooks = { parts: pair(SKETCH_PARTS, 3), rough: pair(SKETCH_ROUGH, 3) };
    this.sketches = new Map(); // id -> {geometry, meshes}
    this.pickMaterial = new THREE.ShaderMaterial({ vertexShader: VERT, fragmentShader: PICK_FRAG, side: THREE.DoubleSide });
    this.pickScene = new THREE.Scene();
    this.pickTarget = new THREE.WebGLRenderTarget(1, 1);
    this.pickBytes = new Uint8Array(4);
    // the selection's outline: its mask, then one full-screen pass
    this.maskScene = new THREE.Scene();
    this.maskMaterial = new THREE.ShaderMaterial({
      uniforms: { uSel: this.uniforms.uSel }, vertexShader: VERT, fragmentShader: MASK_FRAG, side: THREE.DoubleSide, depthTest: false, depthWrite: false,
    });
    this.maskTarget = new THREE.WebGLRenderTarget(1, 1, { samples: 4, depthBuffer: false });
    this.outlineUniforms = { uMask: { value: this.maskTarget.texture }, uPx: { value: new THREE.Vector2(1, 1) }, uWidth: { value: OUTLINE_PX }, uAccent: this.uniforms.uAccent };
    this.outlineScene = new THREE.Scene();
    const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), new THREE.ShaderMaterial({
      uniforms: this.outlineUniforms, vertexShader: OUTLINE_VERT, fragmentShader: OUTLINE_FRAG, transparent: true, depthTest: false, depthWrite: false,
    }));
    quad.frustumCulled = false;
    this.outlineScene.add(quad);
    this.outlineCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
    this.maskMesh = null;
    this.pointUniforms = { uLo: { value: new THREE.Vector3() }, uSpan: { value: new THREE.Vector3(1, 1, 1) } };
    this.pointMaterial = new THREE.ShaderMaterial({ uniforms: this.pointUniforms, vertexShader: VERT, fragmentShader: POINT_FRAG, side: THREE.DoubleSide });
    this.mesh = null;
    this.grid = null;
    this.payload = null;
    this.showOverhang = true;
    this.showPaint = true;
    this.showGrid = true;
    this.plan = null;
    this.debug = null; // the payload's debug marks (the grass's), drawn while showDebug
    this.clouds = null; // the payload's clouds: what a selected top= or pin is drawn from
    this.debugGroup = null;
    this.showDebug = false;
    this.cloudSel = null; // the stack entry whose cloud is selected (setCloud), and the one hovered
    this.cloudHov = null;
    this.cloudGroup = null;
    this.fatRes = { value: new THREE.Vector2(1, 1) }; // the canvas in device pixels, for lines drawn a width on screen
    this.fatDpr = 1;
    this.fatMaterials = new Set();
    this.debugColour = () => '#808080'; // the page gives the theme's tokens (setDebugColours)
    new ResizeObserver(() => this.resize()).observe(canvas);
    this.resize();
  }

  resize() {
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (!w || !h) return;
    this.renderer.setPixelRatio(window.devicePixelRatio || 1);
    this.renderer.setSize(w, h, false);
    const dpr = window.devicePixelRatio || 1;
    const bw = Math.max(1, Math.round(w * dpr)), bh = Math.max(1, Math.round(h * dpr));
    this.maskTarget.setSize(bw, bh);
    this.outlineUniforms.uPx.value.set(1 / bw, 1 / bh);
    this.outlineUniforms.uWidth.value = OUTLINE_PX * dpr;
    this.fatRes.value.set(bw, bh);
    this.fatDpr = dpr;
    for (const m of this.fatMaterials) m.uniforms.uWidth.value = m.userData.px * dpr;
    this.persp.aspect = w / h;
    this.persp.updateProjectionMatrix();
    this.fitOrtho();
    this.draw();
  }

  /** Swap in a parsed payload in place: the camera stays where it is. */
  setMesh(payload) {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(payload.positions, 3));
    g.setAttribute('normal', new THREE.BufferAttribute(payload.normals, 3));
    this.painted = new THREE.BufferAttribute(payload.painted, 3, true);
    this.clean = new THREE.BufferAttribute(payload.clean, 3, true);
    this.grey = null;
    g.setAttribute('paint', this.currentPaint());
    // who made each triangle, on its three corners
    const who = ownerVertices(payload.entry, payload.piece);
    g.setAttribute('who', new THREE.BufferAttribute(who, 2));
    if (this.mesh) { this.scene.remove(this.mesh, ...this.ghosts); this.pickScene.remove(this.pickMesh); this.maskScene.remove(this.maskMesh); this.mesh.geometry.dispose(); }
    this.mesh = new THREE.Mesh(g, this.material);
    this.mesh.frustumCulled = false;
    this.scene.add(this.mesh);
    this.ghosts = [this.lit.depth, this.lit.colour, this.ghostDepth, this.ghostColour].map((m, i) => {
      const gm = new THREE.Mesh(g, m);
      gm.frustumCulled = false; gm.renderOrder = 1 + i;
      return gm;
    });
    this.scene.add(...this.ghosts);
    this.applyFade();
    this.pickMesh = new THREE.Mesh(g, this.pickMaterial);
    this.pickMesh.frustumCulled = false;
    this.pickScene.add(this.pickMesh);
    this.maskMesh = new THREE.Mesh(g, this.maskMaterial);
    this.maskMesh.frustumCulled = false;
    this.maskScene.add(this.maskMesh);
    this.payload = payload;
    this.debug = payload.meta?.debug ?? null;
    this.clouds = payload.meta?.clouds ?? null;
    this.buildDebug();
    this.buildCloud();
    this.buildGrid();
    this.draw();
  }

  /** How the debug layer's colours are read: `fn(token)` gives a CSS colour (the theme's `--dbg-…`). */
  setDebugColours(fn) { this.debugColour = fn; this.buildDebug(); this.buildCloud(); this.draw(); }

  /** Show or hide the debug layer (the payload's `debug`: grass starts and spines; the clouds, the payload's `clouds`, are drawn as part of the selection, setCloud). */
  setDebugShown(on) { this.showDebug = on; this.buildDebug(); this.draw(); }

  clear() {
    this.setSketches([]);
    if (this.mesh) { this.scene.remove(this.mesh, ...this.ghosts); this.pickScene.remove(this.pickMesh); this.maskScene.remove(this.maskMesh); this.mesh.geometry.dispose(); this.mesh = null; this.pickMesh = null; this.maskMesh = null; this.ghosts = []; }
    this.payload = null;
    this.buildGrid();
    this.draw();
  }

  /** The tile's squares, for the grid at the square size. */
  setPlan(plan) { this.plan = plan; this.buildGrid(); this.draw(); }

  bounds() {
    // a plate: the bed is what is framed, with the pieces' height
    const bed = this.payload?.meta?.bed;
    if (bed) return { lo: [0, 0, 0], hi: [bed[0], bed[1], Math.max(this.payload.meta.hi[2], 1)] };
    if (this.payload) return { lo: this.payload.meta.lo, hi: this.payload.meta.hi };
    if (this.plan) return { lo: [0, 0, 0], hi: [...this.plan.size_mm, 1] };
    return null;
  }

  setOverhang(on) { this.showOverhang = on; this.refreshPaint(); }

  setPaint(on) { this.showPaint = on; this.refreshPaint(); }

  setWireframe(on) { this.material.wireframe = on; this.draw(); }

  setGrid(on) { this.showGrid = on; if (this.grid) this.grid.visible = on; this.draw(); }

  /**
   * What is under canvas point (x, y) in CSS pixels: {entry, piece}, or
   * null for the plate.  One pixel rendered with the pick pass, so it is
   * exactly what the eye sees there, whatever is in front.
   */
  pick(x, y) {
    if (!this.pickPixel(x, y)) return null;
    const [a, b, c, d] = this.pickBytes;
    const e = a + 256 * b - 1;
    return e < 0 ? null : { entry: e, piece: c + 256 * d };
  }

  /**
   * The point under canvas point (x, y), mm: on the tile where the eye
   * sees it, else on the plane z = `planeZ` (the datum when not given);
   * null when the ray misses even that.  {p: [x, y, z], onTile}.
   */
  pointAt(x, y, planeZ = null) {
    if (planeZ == null && this.pickMesh && this.payload) {
      const { lo, hi } = this.payload.meta;
      this.pointUniforms.uLo.value.set(...lo);
      this.pointUniforms.uSpan.value.set(Math.max(hi[0] - lo[0], 1e-3), Math.max(hi[1] - lo[1], 1e-3), Math.max(hi[2] - lo[2], 1e-3));
      this.pickMesh.material = this.pointMaterial;
      const hit = this.pickPixel(x, y);
      this.pickMesh.material = this.pickMaterial;
      const [a, b, c, d] = this.pickBytes;
      if (hit && (a | b | c | d)) {
        const qx = a + 256 * (b % 16), qy = Math.floor(b / 16) + 16 * c;
        return { p: [lo[0] + (qx / 4095) * (hi[0] - lo[0]), lo[1] + (qy / 4095) * (hi[1] - lo[1]), lo[2] + (d / 255) * (hi[2] - lo[2])], onTile: true };
      }
    }
    const z = planeZ ?? this.plan?.datum_z ?? this.payload?.meta.lo[2] ?? 0;
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    const ray = new THREE.Raycaster();
    ray.setFromCamera(new THREE.Vector2((x / w) * 2 - 1, 1 - (y / h) * 2), this.camera);
    const q = ray.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 0, 1), -z), new THREE.Vector3());
    return q ? { p: [q.x, q.y, q.z], onTile: false } : null;
  }

  /** Render the pick scene's one pixel under (x, y) into pickBytes. */
  pickPixel(x, y) {
    if (!this.pickMesh) return false;
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (x < 0 || y < 0 || x >= w || y >= h) return false;
    const cam = this.camera;
    cam.setViewOffset(w, h, Math.floor(x), Math.floor(y), 1, 1);
    const r = this.renderer;
    r.setRenderTarget(this.pickTarget);
    r.setClearColor(0x000000, 0);
    r.clear();
    r.render(this.pickScene, cam);
    r.readRenderTargetPixels(this.pickTarget, 0, 0, 1, 1, this.pickBytes);
    r.setRenderTarget(null);
    cam.clearViewOffset();
    return true;
  }

  /** Fade what isn't selected (on by default). */
  setFade(on) { this.fade = !!on; this.applyFade(); this.draw(); }

  /** The centre of what entry (and piece) made, for flying to it; null when it has no triangles. */
  centreOf(entry, piece = null) {
    const p = this.payload;
    if (!p) return null;
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    let any = false;
    for (let t = 0; t < p.triangles; t++) {
      if (p.entry[t] !== entry || (piece != null && p.piece[t] !== piece)) continue;
      any = true;
      for (let k = 0; k < 9; k++) {
        const v = p.positions[9 * t + k], a = k % 3;
        if (v < lo[a]) lo[a] = v;
        if (v > hi[a]) hi[a] = v;
      }
    }
    return any ? { lo, hi } : null;
  }

  /** The gate-painted face the user should inspect first. */
  worstFlagged() { return this.payload ? worstFlagged(this.payload) : null; }

  /** Where world point `p` falls on the canvas, in CSS pixels. */
  project(p) {
    const v = new THREE.Vector3(...p).project(this.camera);
    return { x: ((v.x + 1) / 2) * this.canvas.clientWidth, y: ((1 - v.y) / 2) * this.canvas.clientHeight };
  }

  draw() {
    if (this.pending) return;
    this.pending = requestAnimationFrame(() => { this.pending = 0; this.render(); });
  }

  render() {
    const t = this.controls.target, p = this.camera.position;
    const off = p.clone().sub(t);
    const azim = (Math.atan2(off.y, off.x) * 180) / Math.PI;
    const elev = (Math.asin(off.z / (off.length() || 1)) * 180) / Math.PI;
    this.uniforms.uCam.value.copy(p);
    this.uniforms.uDir.value.copy(t).sub(p).normalize();
    this.uniforms.uL1.value.copy(dirv(18, azim - 20));
    this.uniforms.uL2.value.copy(dirv(elev - 20, azim + 110));
    this.renderer.render(this.scene, this.camera);
    this.renderOutline();
    this.onRender?.();
  }

  /** The selection's outline over the frame: its whole shape, through what stands in front. */
  renderOutline() {
    if (!this.maskMesh || this.uniforms.uSel.value.x < 0 || this.stale || this.sketches.size > 0) return;
    const r = this.renderer;
    const clear = r.getClearAlpha(), auto = r.autoClear;
    r.setRenderTarget(this.maskTarget);
    r.setClearColor(0x000000, 0);
    r.clear();
    r.render(this.maskScene, this.camera);
    r.setRenderTarget(null);
    r.setClearColor(0x000000, clear);
    r.autoClear = false;
    r.render(this.outlineScene, this.outlineCamera);
    r.autoClear = auto;
  }
}

Object.assign(Viewer.prototype, cameraMethods, fadeMethods, layersMethods);
