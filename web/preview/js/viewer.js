// The 3D viewport: host-independent (no fetch, no DOM beyond its canvas).
// Painted like the CPU renderer in crates/dt-kit/src/render.rs: flat shading,
// two lights placed relative to the camera, the same gamma, the same
// background and the same views.  Colours arrive per vertex from Rust.

import * as THREE from 'three';
import { OrbitControls } from '/vendor/OrbitControls.js';
import { ownerVertices, worstFlagged } from './payload.js';

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
const GRID_BELOW_MM = 0.02;
// ink lines on the plate: bold at each square, fine at a fifth of one, faded with distance
const GRID_FRAG = `
uniform float uDim; uniform float uSq; uniform vec2 uCentre; uniform float uR0; uniform float uR1; uniform vec3 uInk;
varying vec2 vXY;
float line(float v, float px, float w) {
  float d = abs(fract(v - 0.5) - 0.5) / px; // distance to the nearest line, in pixels
  return 1.0 - smoothstep(w - 0.5, w + 0.5, d);
}
void main() {
  vec2 g = vXY / uSq;
  vec2 px = max(fwidth(g), vec2(1e-5));
  float major = max(line(g.x, px.x, 1.1), line(g.y, px.y, 1.1));
  vec2 f = g * 5.0;
  float minor = max(line(f.x, px.x * 5.0, 0.6), line(f.y, px.y * 5.0, 0.6));
  float fade = 1.0 - smoothstep(uR0, uR1, distance(vXY, uCentre));
  float a = max(major * 0.42, minor * 0.13) * fade * uDim;
  if (a < 0.01) discard;
  gl_FragColor = vec4(uInk, a);
}`;
const NARROW_FOV = 3; // the near-orthographic end of the projection tween
const easeInOutCubic = (u) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2);
const reduceMotion = () => {
  try { return matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; }
};
const rad = (d) => (d * Math.PI) / 180;
const dirv = (elev, azim) => new THREE.Vector3(
  Math.cos(rad(elev)) * Math.cos(rad(azim)), Math.cos(rad(elev)) * Math.sin(rad(azim)), Math.sin(rad(elev)));

const VERT = `
attribute vec3 paint; attribute vec2 who;
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
void main() {
  vN = normal; vC = paint; vP = position; vWho = who;
  gl_Position = projectionMatrix * viewMatrix * vec4(position, 1.0);
}`;

// The render.rs shading: 0.3 + key + 0.35 fill, *0.95, gamma 1/1.25,
// back faces turned to face the camera.
// The selection and the thing under the pointer are tinted by the theme accent. `who`
// stores (entry + 1, piece), with entry 0 meaning none; the uniforms keep
// ordinary zero-based entries and a piece of -1 means the whole entry.
const FRAG = `
uniform vec3 uCam; uniform vec3 uDir; uniform float uOrtho; uniform vec3 uL1; uniform vec3 uL2;
uniform vec2 uSel; uniform vec2 uHov; uniform vec3 uAccent;
uniform vec2 uKeep; uniform vec2 uLeave; uniform float uKeepA; uniform float uLeaveA;
uniform float uMode; uniform float uGhost; // 0 all; 2 the rest, faded; 3 all, faded; 4 the kept and leaving things
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
float is(vec2 w) {
  vec2 v = floor(vWho + 0.5);
  return (w.x >= 0.0 && v.x == w.x + 1.0 && (w.y < 0.0 || v.y == w.y)) ? 1.0 : 0.0;
}
void main() {
  // what stands out of the fade: the selection (uKeep), and while the
  // selection moves, the thing it left (uLeave), each at its own alpha
  float keep = is(uKeep), leave = is(uLeave) * (1.0 - keep);
  if (uMode > 1.5 && uMode < 2.5 && keep + leave > 0.5) discard;
  if (uMode > 3.5 && keep + leave < 0.5) discard;
  vec3 n = normalize(vN);
  vec3 ray = uOrtho > 0.5 ? uDir : vP - uCam;
  if (dot(n, ray) > 0.0) n = -n;
  float shade = 0.3 + max(dot(n, uL1), 0.0) + 0.35 * max(dot(n, uL2), 0.0);
  vec3 c = pow(clamp(vC * shade * 0.95, 0.0, 1.0), vec3(1.0 / 1.25));
  float k = max(is(uSel) * 0.42, is(uHov) * 0.22);
  c = mix(c, uAccent * (0.55 + 0.45 * shade), k);
  float a = uMode > 3.5 ? mix(uLeaveA, uKeepA, keep) : uMode > 1.5 ? uGhost : 1.0;
  gl_FragColor = vec4(c, a);
}`;

// The pick pass: `who` already carries entry + 1, so zero stays no owner.
const PICK_FRAG = `
varying vec2 vWho;
void main() {
  vec2 v = floor(vWho + 0.5);
  float e = v.x;
  gl_FragColor = vec4(mod(e, 256.0), floor(e / 256.0), mod(v.y, 256.0), floor(v.y / 256.0)) / 255.0;
}`;

// The point pass: each fragment writes where it is, x and y in 12 bits
// each and z in 8, across the mesh's bounds.
const POINT_FRAG = `
uniform vec3 uLo; uniform vec3 uSpan;
varying vec3 vP;
void main() {
  vec3 q = clamp((vP - uLo) / uSpan, 0.0, 1.0);
  float x = floor(q.x * 4095.0 + 0.5), y = floor(q.y * 4095.0 + 0.5), z = floor(q.z * 255.0 + 0.5);
  gl_FragColor = vec4(mod(x, 256.0), floor(x / 256.0) + 16.0 * mod(y, 16.0), floor(y / 16.0), z) / 255.0;
}`;

// How much of a faded thing shows: the rest beside a selection; an older
// build's mesh while the new one builds; a stage's finished parts and a
// rough stand-in in a build's sketch.
const FADE = 0.45, STALE = 0.3, SKETCH_PARTS = 0.7, SKETCH_ROUGH = 0.35;
// Seconds the rest takes to fade when something is selected, and to come
// back when nothing is.
const FADE_IN = 0.22, FADE_OUT = 0.18;

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
      uSel: { value: new THREE.Vector2(-1, -1) }, uHov: { value: new THREE.Vector2(-1, -1) }, uKeep: { value: new THREE.Vector2(-1, -1) }, uLeave: { value: new THREE.Vector2(-1, -1) }, uKeepA: { value: 1 }, uLeaveA: { value: 1 }, uAccent: { value: new THREE.Color(0.15, 0.48, 0.38) },
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
    this.fx = { rest: 1, keep: 1, leave: 1 };
    this.fxTarget = { rest: 1, keep: 1, leave: 1 };
    this.fadeTween = null;
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
    this.pointUniforms = { uLo: { value: new THREE.Vector3() }, uSpan: { value: new THREE.Vector3(1, 1, 1) } };
    this.pointMaterial = new THREE.ShaderMaterial({ uniforms: this.pointUniforms, vertexShader: VERT, fragmentShader: POINT_FRAG, side: THREE.DoubleSide });
    this.mesh = null;
    this.grid = null;
    this.payload = null;
    this.showOverhang = true;
    this.showPaint = true;
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
    // zoom toward the point under the cursor: camera and target move together
    // (perspective dollies along the cursor ray, orthographic changes zoom and shifts)
    this.controls.zoomToCursor = true;
    this.controls.minDistance = 4;
    this.controls.maxDistance = 600;
    this.controls.minZoom = 0.2;
    this.controls.maxZoom = 40;
    this.controls.target.copy(target);
    this.controls.enableRotate = this.orbit !== false;
    let userMoving = false;
    this.controls.addEventListener('change', () => {
      if (userMoving) this.onUserViewChange?.();
      this.sample();
      this.draw();
    });
    // the user took hold (drag, wheel, touch): the tween ends and the controls own the camera
    this.controls.addEventListener('start', () => { userMoving = true; this.cancelTween(true); });
    this.controls.addEventListener('end', () => { userMoving = false; });
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
    this.stopCoast();
    const t = this.tween;
    if (!t) return;
    cancelAnimationFrame(t.raf);
    this.tween = null;
    if (finish) t.done?.();
  }

  /**
   * Momentum, like a flung spinner.  Rotation only (pan and zoom stop when
   * released).  The angular velocity comes from the last ~100 ms of a left
   * drag; on release the view keeps turning at it, decaying exponentially
   * with a time constant of 0.3 s for a gentle drag up to about 1.2 s for a
   * hard fling, so the decay does not depend on the frame rate.  A pointer
   * that rested before release (80 ms) leaves no momentum.  Azimuth about
   * the vertical is unlimited; the polar angle stops just short of the poles.
   */
  initInertia() {
    this.samples = [];
    this.dragging = false;
    this.coast = null;
    const c = this.canvas;
    c.addEventListener('pointerdown', (e) => {
      this.stopCoast();
      this.samples = [];
      this.dragging = e.button === 0 && !e.shiftKey && !e.ctrlKey && !e.metaKey;
    }, true);
    const up = () => { if (this.dragging) { this.dragging = false; this.release(); } };
    addEventListener('pointerup', up);
    addEventListener('pointercancel', () => { this.dragging = false; this.samples = []; });
    c.addEventListener('wheel', () => this.stopCoast(), { passive: true });
  }

  angles() {
    const o = this.camera.position.clone().sub(this.controls.target);
    return [Math.atan2(o.y, o.x), Math.acos(Math.max(-1, Math.min(1, o.z / (o.length() || 1)))), o.length()];
  }

  sample() {
    if (!this.dragging) return;
    const [az, pol] = this.angles();
    this.samples.push({ t: performance.now(), az, pol });
    if (this.samples.length > 40) this.samples.shift();
  }

  release() {
    const s = this.samples;
    this.samples = [];
    if (reduceMotion() || s.length < 2) return;
    const now = performance.now();
    const last = s[s.length - 1];
    if (now - last.t > 80) return; // it stopped before it was let go
    const first = s.find((x) => last.t - x.t <= 100) ?? s[0];
    const dt = (last.t - first.t) / 1000;
    if (dt < 0.008) return;
    let dAz = last.az - first.az;
    dAz -= Math.round(dAz / (2 * Math.PI)) * 2 * Math.PI; // across the +-pi seam
    const v = [dAz / dt, (last.pol - first.pol) / dt];
    const speed = Math.hypot(v[0] * Math.sin(last.pol), v[1]);
    if (speed < 0.15) return;
    const tau = 0.3 + 0.9 * Math.min(1, speed / 8);
    this.coast = { v, tau, t: now, raf: 0 };
    const tick = (tNow) => {
      const k = this.coast;
      if (!k) return;
      const dt2 = Math.min(0.05, Math.max(0, (tNow - k.t) / 1000));
      k.t = tNow;
      const f = Math.exp(-dt2 / k.tau);
      const [az, pol, len] = this.angles();
      const nAz = az + k.v[0] * dt2;
      const nPol = Math.min(Math.PI - 0.02, Math.max(0.02, pol + k.v[1] * dt2));
      if (nPol !== pol + k.v[1] * dt2) k.v[1] = 0; // met a pole
      const o = new THREE.Vector3(Math.sin(nPol) * Math.cos(nAz), Math.sin(nPol) * Math.sin(nAz), Math.cos(nPol)).multiplyScalar(len);
      this.camera.position.copy(this.controls.target).add(o);
      this.camera.lookAt(this.controls.target);
      this.controls.update();
      this.draw();
      k.v[0] *= f; k.v[1] *= f;
      if (Math.hypot(k.v[0], k.v[1]) < 0.02) { this.coast = null; return; }
      k.raf = requestAnimationFrame(tick);
    };
    this.coast.raf = requestAnimationFrame((n) => { this.coast.t = n; tick(n); });
  }

  stopCoast() {
    if (!this.coast) return;
    cancelAnimationFrame(this.coast.raf);
    this.coast = null;
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
    this.grey = null;
    g.setAttribute('paint', this.currentPaint());
    // who made each triangle, on its three corners
    const who = ownerVertices(payload.entry, payload.piece);
    g.setAttribute('who', new THREE.BufferAttribute(who, 2));
    if (this.mesh) { this.scene.remove(this.mesh, ...this.ghosts); this.pickScene.remove(this.pickMesh); this.mesh.geometry.dispose(); }
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
    this.payload = payload;
    this.buildGrid();
    this.draw();
  }

  /** The sketches of a build running: [{id, rough, payload}], replacing those shown. */
  setSketches(list) {
    const want = new Set(list.map((k) => k.id));
    for (const [id, k] of this.sketches) {
      if (want.has(id)) continue;
      this.scene.remove(...k.meshes);
      k.geometry.dispose();
      this.sketches.delete(id);
    }
    for (const { id, rough, payload } of list) {
      if (this.sketches.has(id)) continue;
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(payload.positions, 3));
      g.setAttribute('normal', new THREE.BufferAttribute(payload.normals, 3));
      g.setAttribute('paint', new THREE.BufferAttribute(payload.clean, 3, true));
      g.setAttribute('who', new THREE.BufferAttribute(new Float32Array(2 * payload.triangles * 3), 2));
      const look = rough ? this.sketchLooks.rough : this.sketchLooks.parts;
      const meshes = [look.depth, look.colour].map((m, i) => {
        const gm = new THREE.Mesh(g, m);
        gm.frustumCulled = false; gm.renderOrder = (rough ? 5 : 3) + i;
        return gm;
      });
      this.scene.add(...meshes);
      this.sketches.set(id, { geometry: g, meshes });
    }
    this.applyFade();
    this.draw();
  }

  /** The mesh shown is an older build's (a new one is being built): fade all of it. */
  setStale(on) {
    if (this.stale === !!on) return;
    this.stale = !!on;
    this.applyFade();
    this.draw();
  }

  clear() {
    this.setSketches([]);
    if (this.mesh) { this.scene.remove(this.mesh, ...this.ghosts); this.pickScene.remove(this.pickMesh); this.mesh.geometry.dispose(); this.mesh = null; this.pickMesh = null; this.ghosts = []; }
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

  /**
   * The ground grid, ruled at the square size (35 mm DungeonBlocks, 25.4
   * OpenLOCK): theme-contrasting lines, bold at every square and fine in between, a
   * plane at the tile's datum (z = 0, where the base meets the tile body;
   * an STL has none, so its lowest point) that fades out toward the
   * horizon.  It is drawn after the tile, without writing depth, a hair
   * below the datum so it never fights the tile's flat underside.
   */
  buildGrid() {
    if (this.grid) { this.scene.remove(this.grid); this.grid.geometry.dispose(); this.grid.material.dispose(); this.grid = null; }
    const b = this.bounds();
    const sq = this.plan?.square_mm;
    if (!b || !sq) return;
    const [w, h] = this.plan.size_mm;
    const z = (this.plan.datum_z ?? b.lo[2]) - GRID_BELOW_MM;
    const reach = Math.max(w, h) * 6;
    const mat = new THREE.ShaderMaterial({
      uniforms: {
        uSq: { value: sq }, uDim: { value: this.gridDim ?? 1 }, uCentre: { value: new THREE.Vector2(w / 2, h / 2) },
        uR0: { value: Math.max(w, h) * 1.2 }, uR1: { value: reach * 0.5 },
        uInk: { value: this.gridInk?.clone() ?? new THREE.Color(0.17, 0.2, 0.25) },
      },
      vertexShader: 'varying vec2 vXY; void main() { vec4 w = modelMatrix * vec4(position, 1.0); vXY = w.xy; gl_Position = projectionMatrix * viewMatrix * w; }',
      fragmentShader: GRID_FRAG,
      transparent: true, depthWrite: false, side: THREE.DoubleSide,
    });
    this.grid = new THREE.Mesh(new THREE.PlaneGeometry(reach, reach), mat);
    this.grid.position.set(w / 2, h / 2, z);
    this.grid.renderOrder = 1;
    this.grid.frustumCulled = false;
    this.grid.visible = this.showGrid;
    this.scene.add(this.grid);
  }

  /** Set the grid's opacity independently of the tile's paint. */
  setGridDim(v) {
    this.gridDim = v;
    if (this.grid) this.grid.material.uniforms.uDim.value = v;
    this.draw();
  }

  /** The theme's grid ink: dark by day, pale against Moonpool's twilight. */
  setGridColor(css) {
    this.gridInk ??= new THREE.Color(0.17, 0.2, 0.25);
    try { this.gridInk.setStyle(css); } catch { /* keep the previous colour */ }
    if (this.grid) this.grid.material.uniforms.uInk.value.copy(this.gridInk);
    this.draw();
  }

  /**
   * The colours to show: the acrylic paint, or unpainted grey plastic (the
   * overhang's red and orange kept when that is on, so print checks still
   * work). Grey is made here from the same buffers.
   */
  currentPaint() {
    if (this.showPaint) return this.showOverhang ? this.painted : this.clean;
    if (!this.grey) {
      const grey = new Uint8Array(this.clean.array.length).fill(168);
      const flagged = new Uint8Array(this.clean.array.length);
      const [p, c] = [this.painted.array, this.clean.array];
      for (let i = 0; i < p.length; i += 3) {
        if (p[i] !== c[i] || p[i + 1] !== c[i + 1] || p[i + 2] !== c[i + 2]) {
          flagged.set(p.subarray(i, i + 3), i);
        } else flagged.set([168, 168, 170], i);
      }
      this.grey = {
        plain: new THREE.BufferAttribute(grey, 3, true),
        flagged: new THREE.BufferAttribute(flagged, 3, true),
      };
    }
    return this.showOverhang ? this.grey.flagged : this.grey.plain;
  }

  refreshPaint() {
    if (this.mesh) this.mesh.geometry.setAttribute('paint', this.currentPaint());
    this.draw();
  }

  setOverhang(on) { this.showOverhang = on; this.refreshPaint(); }
  setPaint(on) { this.showPaint = on; this.refreshPaint(); }
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

  /** Let a left drag orbit (true), or keep it for a tool (false). */
  setOrbit(on) {
    this.orbit = on;
    this.controls.enableRotate = on;
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

  /** Tint the selection and the hovered thing: each {entry, piece} (piece null: the whole entry) or null. */
  setHighlight(sel, hov) {
    const put = (u, w) => u.value.set(w ? w.entry : -1, w && w.piece != null ? w.piece : -1);
    put(this.uniforms.uSel, sel);
    put(this.uniforms.uHov, hov);
    this.applyFade();
    this.draw();
  }

  /** Fade what isn't selected (on by default). */
  setFade(on) { this.fade = !!on; this.applyFade(); this.draw(); }

  applyFade() {
    // a build's sketches stand in for the older mesh, which steps aside
    const sketched = this.sketches.size > 0;
    if (this.mesh) this.mesh.visible = !this.stale && !sketched;
    this.ghost.u.uMode.value = this.stale ? 3 : 2;
    const u = this.uniforms, sel = u.uSel.value, keep = u.uKeep.value, leave = u.uLeave.value;
    const fading = this.fade && sel.x >= 0 && !this.stale && !sketched;
    const same = (a, b) => a.x === b.x && a.y === b.y;
    const fx = this.fx;
    const moved = fading && !same(sel, keep);
    if (moved) {
      // the selection moves: the new one rises from where it stood (in the
      // rest, or leaving), the old one sinks into the rest
      const from = same(sel, leave) ? fx.leave : fx.rest;
      if (keep.x >= 0) { leave.copy(keep); fx.leave = fx.keep; } else leave.set(-1, -1);
      keep.copy(sel);
      fx.keep = from;
    }
    const target = fading ? { rest: FADE, keep: 1, leave: FADE } : { rest: 1, keep: 1, leave: 1 };
    const t = this.fxTarget;
    const retarget = Object.keys(target).some((k) => t[k] !== target[k]);
    if (this.stale || sketched || reduceMotion()) this.fadeTo(target, 0);
    else if (moved || retarget) this.fadeTo(target, fading ? FADE_IN : FADE_OUT);
    this.showFade();
  }

  /** Ease the rest, the selection and what it left to `target` over `seconds` (0: at once). */
  fadeTo(target, seconds) {
    if (this.fadeTween) cancelAnimationFrame(this.fadeTween);
    this.fadeTween = null;
    this.fxTarget = target;
    const from = { ...this.fx };
    const far = Math.max(...Object.keys(target).map((k) => Math.abs(target[k] - from[k])));
    if (!seconds || far === 0) { Object.assign(this.fx, target); this.settleFade(); return; }
    // part of the way there already: the rest of the time, not all of it
    const ms = seconds * 1000 * Math.min(1, far / (1 - FADE));
    const t0 = performance.now();
    const tick = (now) => {
      const e = easeInOutCubic(Math.min(1, (now - t0) / ms));
      for (const k of Object.keys(target)) this.fx[k] = from[k] + (target[k] - from[k]) * e;
      this.fadeTween = e < 1 ? requestAnimationFrame(tick) : null;
      if (e >= 1) this.settleFade();
      this.showFade();
      this.draw();
    };
    this.fadeTween = requestAnimationFrame(tick);
  }

  /** At rest: what left the selection is part of the rest again, and with nothing selected nothing is kept. */
  settleFade() {
    const u = this.uniforms;
    u.uLeave.value.set(-1, -1);
    this.fx.leave = this.fx.rest;
    if (u.uSel.value.x < 0 || !this.fade) u.uKeep.value.set(-1, -1);
  }

  /** Set the passes for the fade as it stands now. */
  showFade() {
    const sketched = this.sketches.size > 0;
    const [litDepth, litColour, ...rest] = this.ghosts;
    if (this.stale) {
      this.ghost.u.uGhost.value = STALE;
      for (const g of rest) g.visible = !sketched;
      if (litDepth) litDepth.visible = litColour.visible = false;
      return;
    }
    // faded, or on the way in or out: the selection over the rest, each at its alpha
    const u = this.uniforms, fx = this.fx;
    const on = !sketched && u.uKeep.value.x >= 0;
    if (this.mesh) this.mesh.visible = !sketched && !on;
    this.ghost.u.uGhost.value = fx.rest;
    u.uKeepA.value = fx.keep;
    u.uLeaveA.value = fx.leave;
    for (const g of this.ghosts) g.visible = on;
  }

  /** The theme's accent (`--accent`), used to tint selected geometry. */
  setAccent(css) {
    try { this.uniforms.uAccent.value.setStyle(css); } catch { /* keep the default */ }
    this.draw();
  }

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
    this.onRender?.();
  }
}
