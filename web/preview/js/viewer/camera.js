// The viewer's camera: orbit controls and their inertia, the perspective to orthographic
// tween, flying to a pose, the named views.
//
// Methods of `Viewer`, installed on its prototype by `viewer.js`: they run
// with `this` the viewer.

import * as THREE from 'three';
import { OrbitControls } from '/vendor/OrbitControls.js';
import { FOV, NARROW_FOV, VIEWS, dirv, easeInOutCubic, rad, reduceMotion } from './consts.js';

export const cameraMethods = {
  /** The orthographic frustum: its height from `orthoHalf`, its width from the aspect. */
  fitOrtho() {
    const a = this.persp.aspect;
    const o = this.ortho;
    o.top = this.orthoHalf; o.bottom = -this.orthoHalf;
    o.left = -this.orthoHalf * a; o.right = this.orthoHalf * a;
    o.updateProjectionMatrix();
  },

  /** Orbit controls on the active camera, aimed at `target`. */
  makeControls(target) {
    if (this.controls) this.controls.dispose();
    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.enableDamping = false;
    // zoom toward the point under the cursor: camera and target move together
    // (perspective dollies along the cursor ray, orthographic changes zoom and shifts)
    this.controls.zoomToCursor = true;
    this.controls.minDistance = 4;
    this.controls.maxDistance = this.maxDistance ?? 600;
    this.controls.minZoom = 0.2;
    this.controls.maxZoom = 40;
    this.controls.target.copy(target);
    this.controls.enableRotate = this.orbit !== false;
    let userMoving = false;
    this.controls.addEventListener('change', () => {
      if (userMoving) this.userViewHooks.run();
      this.sample();
      this.draw();
    });
    // the user took hold (drag, wheel, touch): the tween ends and the controls own the camera
    this.controls.addEventListener('start', () => { userMoving = true; this.hold(); this.cancelTween(true); });
    this.controls.addEventListener('end', () => { userMoving = false; });
    this.controls.update();
  },

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
  },

  /** Stop a running tween; with `finish`, run its `done` (a projection switch lands where it was going). */
  cancelTween(finish) {
    this.stopCoast();
    const t = this.tween;
    if (!t) return;
    cancelAnimationFrame(t.raf);
    this.tween = null;
    if (finish) t.done?.();
  },

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
  },

  angles() {
    const o = this.camera.position.clone().sub(this.controls.target);
    return [Math.atan2(o.y, o.x), Math.acos(Math.max(-1, Math.min(1, o.z / (o.length() || 1)))), o.length()];
  },

  sample() {
    if (!this.dragging) return;
    const [az, pol] = this.angles();
    this.samples.push({ t: performance.now(), az, pol });
    if (this.samples.length > 40) this.samples.shift();
  },

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
  },

  stopCoast() {
    if (!this.coast) return;
    cancelAnimationFrame(this.coast.raf);
    this.coast = null;
  },

  /** The camera's pose now: target, unit direction from the target, perspective-equivalent distance. */
  pose() {
    const target = this.controls.target.clone();
    return { target, dir: this.camera.position.clone().sub(target).normalize(), dist: this.eyeDistance() };
  },

  /**
   * Orbit to a pose: the direction turns round the target along a great
   * circle (the object seems to turn in space), the distance and target
   * ease separately.  Up stays +z, so the path does not roll through Top
   * or Under.  A new call starts from wherever the last one had got to.
   */
  flyTo(to) {
    this.hold(); // a flight somebody asked for is their view now
    this.flyToPose(to);
  },

  /** The flight itself (what the camera does by itself is `place`; this one does not take the view from the framing). */
  flyToPose(to) {
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
  },

  /** The apparent size: the perspective distance that gives the same view. */
  eyeDistance() {
    if (!this.isOrtho) return this.persp.position.distanceTo(this.controls.target);
    return this.orthoHalf / this.ortho.zoom / Math.tan(rad(FOV / 2));
  },

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
  },

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
  },

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
  },

  /** Put the camera on preset `name`, framing `b` (as render.rs does); the framing decides when (viewer/framing.js, `refit`). */
  place(name, b, animate) {
    const v = VIEWS.find((x) => x[0] === name);
    if (!v || !b) return;
    const [, , elev, azim, close] = v;
    const centre = new THREE.Vector3(...b.lo.map((l, k) => 0.5 * (l + b.hi[k])));
    let diag = Math.hypot(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]);
    if (close && b.fit) {
      // a piece of a map: all of it, a little nearer than the whole map
      diag *= 0.8;
    } else if (close) {
      diag = Math.hypot(20, 20, Math.max(8, b.hi[2] - 0.5));
      centre.z = (0.5 + b.hi[2]) / 2;
    }
    // fit the visible area: the narrower of its height and its width
    const aspect = this.canvas.clientWidth / Math.max(1, this.canvas.clientHeight) || 1;
    const dist = (diag / (2 * Math.tan(rad(FOV / 2)) * Math.min(1, aspect))) * 1.1;
    const to = { target: centre, dir: dirv(elev, azim).normalize(), dist };
    if (animate) { this.flyToPose(to); return; }
    this.cancelTween(true);
    this.placeEquivalent(to.target, to.dir, to.dist);
    this.controls.target.copy(to.target);
    this.controls.update();
    this.draw();
  },

  /** The view as a perspective camera would have it (works in either mode). */
  getCamera() {
    const t = this.controls.target;
    const dir = this.camera.position.clone().sub(t).normalize();
    const p = t.clone().addScaledVector(dir, this.eyeDistance());
    return { p: p.toArray(), t: t.toArray() };
  },

  /** The camera put back as it was (the framing's restore of the user's saved view). */
  restoreCamera(c) {
    this.cancelTween(false);
    const t = new THREE.Vector3().fromArray(c.t);
    const p = new THREE.Vector3().fromArray(c.p);
    const dist = p.distanceTo(t);
    this.placeEquivalent(t, p.sub(t).normalize(), dist);
    this.controls.target.copy(t);
    this.controls.update();
    this.draw();
  },

  /** Let a left drag orbit (true), or keep it for a tool (false). */
  setOrbit(on) {
    this.orbit = on;
    this.controls.enableRotate = on;
  }
};
