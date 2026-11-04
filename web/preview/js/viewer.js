// The 3D viewport: host-independent (no fetch, no DOM beyond its canvas).
// Painted like the CPU renderer in crates/dt-kit/src/render.rs: flat shading,
// two lights placed relative to the camera, the same gamma, the same
// background and the same views.  Colours arrive per vertex from Rust.

import * as THREE from 'three';
import { OrbitControls } from '/vendor/OrbitControls.js';

THREE.ColorManagement.enabled = false; // colours are already display values

// [name, label, elevation deg, azimuth deg (round from +x), close-up?]
// painted/top/low/close are the contact sheet's four views.
export const VIEWS = [
  ['painted', 'Iso', 40, -120, false],
  ['top', 'Top', 89, -90, false],
  ['front', 'Front', 0, -90, false],
  ['right', 'Right', 0, 0, false],
  ['back', 'Back', 0, 90, false],
  ['left', 'Left', 0, 180, false],
  ['low', 'Low', 14, -150, false],
  ['close', 'Close', 38, -125, true],
  ['under', 'Under', -60, -120, false],
];

const FOV = 40;
const NARROW_FOV = 3; // the near-orthographic end of the projection tween
const easeInOutCubic = (u) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2);
const reduceMotion = () => {
  try { return matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; }
};
const rad = (d) => (d * Math.PI) / 180;
const dirv = (elev, azim) => new THREE.Vector3(
  Math.cos(rad(elev)) * Math.cos(rad(azim)), Math.cos(rad(elev)) * Math.sin(rad(azim)), Math.sin(rad(elev)));

const VERT = `
attribute vec3 paint;
varying vec3 vN; varying vec3 vC; varying vec3 vP;
void main() {
  vN = normal; vC = paint; vP = position;
  gl_Position = projectionMatrix * viewMatrix * vec4(position, 1.0);
}`;

// The render.rs shading: 0.3 + key + 0.35 fill, *0.95, gamma 1/1.25,
// back faces turned to face the camera.
const FRAG = `
uniform vec3 uCam; uniform vec3 uDir; uniform float uOrtho; uniform vec3 uL1; uniform vec3 uL2;
varying vec3 vN; varying vec3 vC; varying vec3 vP;
void main() {
  vec3 n = normalize(vN);
  vec3 ray = uOrtho > 0.5 ? uDir : vP - uCam;
  if (dot(n, ray) > 0.0) n = -n;
  float shade = 0.3 + max(dot(n, uL1), 0.0) + 0.35 * max(dot(n, uL2), 0.0);
  vec3 c = pow(clamp(vC * shade * 0.95, 0.0, 1.0), vec3(1.0 / 1.25));
  gl_FragColor = vec4(c, 1.0);
}`;

export class Viewer {
  constructor(canvas) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
    this.renderer.setClearColor(new THREE.Color(0.85, 0.88, 0.92));
    this.scene = new THREE.Scene();
    this.persp = new THREE.PerspectiveCamera(FOV, 1, 1, 1000);
    this.ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, 1, 1000);
    for (const c of [this.persp, this.ortho]) c.up.set(0, 0, 1); // tiles are z-up
    this.orthoHalf = 1; // half the frustum's height at zoom 1, mm
    this.isOrtho = false;
    this.camera = this.persp;
    this.makeControls(new THREE.Vector3());
    this.uniforms = {
      uCam: { value: new THREE.Vector3() }, uDir: { value: new THREE.Vector3(0, 1, 0) }, uOrtho: { value: 0 }, uL1: { value: new THREE.Vector3() }, uL2: { value: new THREE.Vector3() },
    };
    this.material = new THREE.ShaderMaterial({
      uniforms: this.uniforms, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide,
    });
    this.mesh = null;
    this.grid = null;
    this.payload = null;
    this.showOverhang = true;
    this.showGrid = true;
    this.plan = null;
    new ResizeObserver(() => this.resize()).observe(canvas);
    this.resize();
  }

  resize() {
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (!w || !h) return;
    this.renderer.setPixelRatio(window.devicePixelRatio || 1);
    this.renderer.setSize(w, h, false);
    this.persp.aspect = w / h;
    this.persp.updateProjectionMatrix();
    this.fitOrtho();
    this.draw();
  }

  /** The orthographic frustum: its height from `orthoHalf`, its width from the aspect. */
  fitOrtho() {
    const a = this.persp.aspect;
    const o = this.ortho;
    o.top = this.orthoHalf; o.bottom = -this.orthoHalf;
    o.left = -this.orthoHalf * a; o.right = this.orthoHalf * a;
    o.updateProjectionMatrix();
  }

  /** Orbit controls on the active camera, aimed at `target`. */
  makeControls(target) {
    if (this.controls) this.controls.dispose();
    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.enableDamping = false;
    this.controls.target.copy(target);
    this.controls.addEventListener('change', () => this.draw());
    // the user took hold (drag, wheel, touch): the tween ends and the controls own the camera
    this.controls.addEventListener('start', () => this.cancelTween(true));
    this.controls.update();
  }

  /** Run `step(eased 0..1)` over `seconds`; `done` runs once at the end (or when cut short by the user). */
  animate(seconds, step, done) {
    this.cancelTween(false);
    const t0 = performance.now();
    const tween = { done, raf: 0 };
    this.tween = tween;
    const tick = (now) => {
      if (this.tween !== tween) return;
      const u = Math.min(1, (now - t0) / (seconds * 1000));
      step(easeInOutCubic(u));
      this.draw();
      if (u < 1) { tween.raf = requestAnimationFrame(tick); return; }
      this.tween = null;
      done?.();
    };
    tween.raf = requestAnimationFrame(tick);
  }

  /** Stop a running tween; with `finish`, run its `done` (a projection switch lands where it was going). */
  cancelTween(finish) {
    const t = this.tween;
    if (!t) return;
    cancelAnimationFrame(t.raf);
    this.tween = null;
    if (finish) t.done?.();
  }

  /** The camera's pose now: target, unit direction from the target, perspective-equivalent distance. */
  pose() {
    const target = this.controls.target.clone();
    return { target, dir: this.camera.position.clone().sub(target).normalize(), dist: this.eyeDistance() };
  }

  /**
   * Orbit to a pose: the direction turns round the target along a great
   * circle (the object seems to turn in space), the distance and target
   * ease separately.  Up stays +z, so the path does not roll through Top
   * or Under.  A new call starts from wherever the last one had got to.
   */
  flyTo(to) {
    const from = this.pose();
    const q = new THREE.Quaternion().setFromUnitVectors(from.dir, to.dir);
    if (Math.abs(q.w) < 1e-6) { // opposite ends: turn about the vertical, not any axis
      q.setFromAxisAngle(new THREE.Vector3(0, 0, 1), Math.PI);
    }
    const angle = 2 * Math.acos(Math.min(1, Math.abs(q.w)));
    const travel = from.target.distanceTo(to.target) / Math.max(from.dist, 1);
    const seconds = Math.min(0.9, 0.6 + 0.2 * (angle / Math.PI) + 0.1 * Math.min(1, travel));
    const apply = (dir, target, dist) => {
      this.placeEquivalent(target, dir, dist);
      this.controls.target.copy(target);
      this.controls.update();
    };
    if (reduceMotion() || angle + travel + Math.abs(Math.log(to.dist / from.dist)) < 1e-4) {
      apply(to.dir, to.target, to.dist);
      this.draw();
      return;
    }
    const id = new THREE.Quaternion();
    this.animate(seconds, (e) => {
      const r = id.clone().slerp(q, e);
      apply(from.dir.clone().applyQuaternion(r).normalize(),
        from.target.clone().lerp(to.target, e),
        from.dist * Math.pow(to.dist / from.dist, e));
    }, null);
  }

  /** The apparent size: the perspective distance that gives the same view. */
  eyeDistance() {
    if (!this.isOrtho) return this.persp.position.distanceTo(this.controls.target);
    return this.orthoHalf / this.ortho.zoom / Math.tan(rad(FOV / 2));
  }

  /**
   * Switch camera, keeping the view: same target, direction and apparent
   * size.  The orthographic frustum comes from the perspective distance and
   * field of view; zooming it later changes `.zoom`, not the distance.
   */
  setOrtho(on, animated = false) {
    if (on === this.isOrtho) return;
    if (animated && !reduceMotion() && !this.tween) return this.projectionTween(on);
    const target = this.controls.target.clone();
    const dir = this.camera.position.clone().sub(target).normalize();
    const dist = this.eyeDistance();
    this.isOrtho = on;
    this.camera = on ? this.ortho : this.persp;
    this.uniforms.uOrtho.value = on ? 1 : 0;
    this.placeEquivalent(target, dir, dist);
    this.makeControls(target);
    this.draw();
  }

  /**
   * A smooth switch by dolly-zoom: the perspective camera narrows its field
   * of view while backing away, so the apparent size holds and the view
   * flattens into the orthographic one (and the reverse).
   */
  projectionTween(toOrtho) {
    const p = this.pose();
    const hh = p.dist * Math.tan(rad(FOV / 2)); // half the view's height at the target, mm
    const set = (fov) => {
      const dist = hh / Math.tan(rad(fov / 2));
      this.persp.fov = fov;
      this.persp.position.copy(p.target).addScaledVector(p.dir, dist);
      this.persp.near = Math.max(0.1, dist / 200);
      this.persp.far = dist * 20;
      this.persp.lookAt(p.target);
      this.persp.updateProjectionMatrix();
    };
    const land = () => {
      this.persp.fov = FOV;
      this.persp.updateProjectionMatrix();
      this.setOrtho(toOrtho);
    };
    if (toOrtho) {
      this.animate(0.5, (e) => set(FOV + (NARROW_FOV - FOV) * e), () => { set(FOV); land(); });
    } else {
      // to the narrow end at once (it looks like the orthographic view), then widen
      this.persp.fov = NARROW_FOV;
      this.isOrtho = false;
      this.camera = this.persp;
      this.uniforms.uOrtho.value = 0;
      set(NARROW_FOV);
      this.makeControls(p.target);
      this.animate(0.5, (e) => set(NARROW_FOV + (FOV - NARROW_FOV) * e), () => set(FOV));
    }
  }

  /** Put the active camera on the view at perspective `dist` from `target` along `dir`. */
  placeEquivalent(target, dir, dist) {
    const c = this.camera;
    c.position.copy(target).addScaledVector(dir, dist);
    c.near = Math.max(0.1, dist / 200);
    c.far = dist * 20;
    if (this.isOrtho) {
      this.orthoHalf = dist * Math.tan(rad(FOV / 2));
      c.zoom = 1;
      this.fitOrtho();
    }
    c.lookAt(target);
    c.updateProjectionMatrix();
  }

  /** Swap in a parsed payload in place: the camera stays where it is. */
  setMesh(payload) {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(payload.positions, 3));
    g.setAttribute('normal', new THREE.BufferAttribute(payload.normals, 3));
    this.painted = new THREE.BufferAttribute(payload.painted, 3, true);
    this.clean = new THREE.BufferAttribute(payload.clean, 3, true);
    g.setAttribute('paint', this.showOverhang ? this.painted : this.clean);
    if (this.mesh) { this.scene.remove(this.mesh); this.mesh.geometry.dispose(); }
    this.mesh = new THREE.Mesh(g, this.material);
    this.mesh.frustumCulled = false;
    this.scene.add(this.mesh);
    this.payload = payload;
    this.buildGrid();
    this.draw();
  }

  clear() {
    if (this.mesh) { this.scene.remove(this.mesh); this.mesh.geometry.dispose(); this.mesh = null; }
    this.payload = null;
    this.buildGrid();
    this.draw();
  }

  /** The tile's squares, for the grid at the square size. */
  setPlan(plan) { this.plan = plan; this.buildGrid(); this.draw(); }

  bounds() {
    if (this.payload) return { lo: this.payload.meta.lo, hi: this.payload.meta.hi };
    if (this.plan) return { lo: [0, 0, 0], hi: [...this.plan.size_mm, 1] };
    return null;
  }

  /** A ground plane under the tile, ruled at the square size (35 mm DungeonBlocks, 25.4 OpenLOCK). */
  buildGrid() {
    if (this.grid) { this.scene.remove(this.grid); this.grid.geometry.dispose(); this.grid = null; }
    const b = this.bounds();
    const sq = this.plan?.square_mm;
    if (!b || !sq) return;
    const [cols, rows] = [this.plan.cols, this.plan.rows];
    const z = b.lo[2];
    const pts = [];
    const m = 1; // squares of margin
    for (let i = -m; i <= cols + m; i++) pts.push(i * sq, -m * sq, z, i * sq, (rows + m) * sq, z);
    for (let j = -m; j <= rows + m; j++) pts.push(-m * sq, j * sq, z, (cols + m) * sq, j * sq, z);
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(pts, 3));
    this.grid = new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: new THREE.Color(0.55, 0.6, 0.68) }));
    this.grid.visible = this.showGrid;
    this.scene.add(this.grid);
  }

  setOverhang(on) {
    this.showOverhang = on;
    if (this.mesh) { this.mesh.geometry.setAttribute('paint', on ? this.painted : this.clean); }
    this.draw();
  }
  setWireframe(on) { this.material.wireframe = on; this.draw(); }
  setGrid(on) { this.showGrid = on; if (this.grid) this.grid.visible = on; this.draw(); }

  /** Put the camera on a named view, framing the tile as render.rs does. */
  setView(name, animate = true) {
    const v = VIEWS.find((x) => x[0] === name);
    const b = this.bounds();
    if (!v || !b) return;
    const [, , elev, azim, close] = v;
    const centre = new THREE.Vector3(...b.lo.map((l, k) => 0.5 * (l + b.hi[k])));
    let diag = Math.hypot(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]);
    if (close) {
      diag = Math.hypot(20, 20, Math.max(8, b.hi[2] - 0.5));
      centre.z = (0.5 + b.hi[2]) / 2;
    }
    const dist = (diag / (2 * Math.tan(rad(FOV / 2)))) * 1.1;
    const to = { target: centre, dir: dirv(elev, azim).normalize(), dist };
    if (animate) { this.flyTo(to); return; }
    this.cancelTween(true);
    this.placeEquivalent(to.target, to.dir, to.dist);
    this.controls.target.copy(to.target);
    this.controls.update();
    this.draw();
  }

  /** The view as a perspective camera would have it (works in either mode). */
  getCamera() {
    const t = this.controls.target;
    const dir = this.camera.position.clone().sub(t).normalize();
    const p = t.clone().addScaledVector(dir, this.eyeDistance());
    return { p: p.toArray(), t: t.toArray() };
  }
  setCamera(c) {
    this.cancelTween(false);
    const t = new THREE.Vector3().fromArray(c.t);
    const p = new THREE.Vector3().fromArray(c.p);
    const dist = p.distanceTo(t);
    this.placeEquivalent(t, p.sub(t).normalize(), dist);
    this.controls.target.copy(t);
    this.controls.update();
    this.draw();
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
  }
}
