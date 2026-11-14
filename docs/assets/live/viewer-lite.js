// The card's 3D view: one tile's mesh as the program painted it (a colour for every face, overhang past the band red, the
// 55 to 65 degree band orange: the program's flags, never a second classification here), drawn by three.js with an orbit.  It
// draws nothing else: the overlay is the flat renderer's (overlay-render.js) on a canvas above this one, asking `project` where
// a point of the tile falls.  Light enough for a page of cards: it renders only when something changed.
//
// Browsers allow only a handful of live WebGL contexts, so a card makes its context when it is first looked at and gives it
// back (`release`) when another card takes the view; the others show their poster.

import * as THREE from 'three';
import { OrbitControls } from './vendor/OrbitControls.js';

/** The plate the tile is shown on (theme.css `--plate`), as the program's own pictures have it. */
const PLATE = 0xd9e0eb;

export class LiteViewer {
  /** @param {HTMLCanvasElement} canvas @param {() => void} onChange called after every frame the camera moved */
  constructor(canvas, onChange) {
    this.canvas = canvas;
    this.onChange = onChange;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setClearColor(PLATE, 1);
    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(30, 1, 1, 3000);
    this.camera.up.set(0, 0, 1);
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x8899aa, 2.4));
    const sun = new THREE.DirectionalLight(0xffffff, 2.2);
    sun.position.set(-1, -2, 3);
    this.scene.add(sun);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = false;
    this.controls.enablePan = false;
    this.controls.addEventListener('change', () => this.invalidate());
    /** @type {THREE.Mesh | null} */
    this.mesh = null;
    this.footprint = [35, 35];
    this.dirty = true;
    this.raf = 0;
    this.size = { w: 0, h: 0 };
  }

  /** A mesh (`positions`, `colours`: three floats a corner, three corners a triangle) on a tile of `footprint` mm. */
  setMesh(positions, colours, footprint, { keepView = true } = {}) {
    const first = !this.mesh;
    if (this.mesh) { this.mesh.geometry.dispose(); this.scene.remove(this.mesh); }
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    g.setAttribute('color', new THREE.BufferAttribute(colours, 3));
    g.computeVertexNormals();
    this.mesh = new THREE.Mesh(g, new THREE.MeshLambertMaterial({ vertexColors: true }));
    this.scene.add(this.mesh);
    this.footprint = footprint;
    if (first || !keepView) this.reset();
    this.invalidate();
  }

  /** The camera from above and a little south, the way the poster is drawn: the first picture is the one the poster promised. */
  reset() {
    const [w, h] = this.footprint;
    const c = new THREE.Vector3(w / 2, h / 2, 2);
    const dist = Math.max(w, h) * 2.15;
    this.controls.target.copy(c);
    this.camera.position.set(c.x, c.y - dist * 0.16, c.z + dist);
    this.controls.update();
    this.invalidate();
  }

  resize(w, h) {
    if (!w || !h) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    this.renderer.setPixelRatio(dpr);
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.size = { w, h };
    this.invalidate();
  }

  invalidate() {
    this.dirty = true;
    if (!this.raf) this.raf = requestAnimationFrame(() => this.frame());
  }

  frame() {
    this.raf = 0;
    if (!this.dirty) return;
    this.dirty = false;
    this.renderer.render(this.scene, this.camera);
    this.onChange();
  }

  /** Where the point (x, y, z) mm of the tile falls on the canvas, in CSS pixels. */
  project(x, y, z = 0) {
    const v = new THREE.Vector3(x, y, z).project(this.camera);
    return { x: ((v.x + 1) / 2) * this.size.w, y: ((1 - v.y) / 2) * this.size.h };
  }

  /** Give the WebGL context back: another card is being looked at. */
  release() {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.controls.dispose();
    this.mesh?.geometry.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
  }
}
