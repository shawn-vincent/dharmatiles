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
uniform vec3 uCam; uniform vec3 uL1; uniform vec3 uL2;
varying vec3 vN; varying vec3 vC; varying vec3 vP;
void main() {
  vec3 n = normalize(vN);
  if (dot(n, vP - uCam) > 0.0) n = -n;
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
    this.camera = new THREE.PerspectiveCamera(FOV, 1, 1, 1000);
    this.camera.up.set(0, 0, 1); // tiles are z-up
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = false;
    this.controls.addEventListener('change', () => this.draw());
    this.uniforms = {
      uCam: { value: new THREE.Vector3() }, uL1: { value: new THREE.Vector3() }, uL2: { value: new THREE.Vector3() },
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
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.draw();
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
  setView(name) {
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
    this.camera.near = Math.max(0.1, dist / 200);
    this.camera.far = dist * 20;
    this.camera.updateProjectionMatrix();
    this.camera.position.copy(centre).addScaledVector(dirv(elev, azim), dist);
    this.controls.target.copy(centre);
    this.controls.update();
    this.draw();
  }

  getCamera() {
    return { p: this.camera.position.toArray(), t: this.controls.target.toArray() };
  }
  setCamera(c) {
    this.camera.position.fromArray(c.p);
    this.controls.target.fromArray(c.t);
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
    this.uniforms.uL1.value.copy(dirv(18, azim - 20));
    this.uniforms.uL2.value.copy(dirv(elev - 20, azim + 110));
    this.renderer.render(this.scene, this.camera);
  }
}
