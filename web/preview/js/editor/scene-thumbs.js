// The catalog's pictures: each tile as the program's super-low mesh seen from above at an angle, on the plate
// colour, rendered here once and kept for the session.  The picture is the real tile (its colours, its overhang
// painted red and orange as everywhere), not a drawing of its size.  One small offscreen renderer draws them
// one after another, so a folder of forty tiles does not make forty contexts or forty builds at once; the
// program's cache makes the second visit instant.

import * as THREE from 'three';
import { parsePayload } from '../payload.js';
import { FOV, FRAG, VERT, dirv } from '../viewer/consts.js';
import { host } from './app.js';

const W = 176, H = 124;
const pictures = new Map(); // key -> Promise<string | null>
let gl = null;
let chain = Promise.resolve();

function setup() {
  const canvas = document.createElement('canvas');
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false, preserveDrawingBuffer: true });
  renderer.setSize(W, H, false);
  const u = {
    uCam: { value: new THREE.Vector3() }, uDir: { value: new THREE.Vector3() }, uOrtho: { value: 0 }, uL1: { value: new THREE.Vector3() }, uL2: { value: new THREE.Vector3() },
    uSel: { value: new THREE.Vector2(-1, -1) }, uHov: { value: new THREE.Vector2(-1, -1) }, uKeep: { value: new THREE.Vector2(-1, -1) }, uLeave: { value: new THREE.Vector2(-1, -1) },
    uLift: { value: new THREE.Vector2(-1, -1) }, uDrop: { value: new THREE.Vector2(-1, -1) }, uKeepA: { value: 1 }, uLeaveA: { value: 1 }, uLiftA: { value: 1 }, uDropA: { value: 1 },
    uTintSel: { value: 0 }, uAccent: { value: new THREE.Color(0.15, 0.48, 0.38) }, uMode: { value: 0 }, uGhost: { value: 1 },
  };
  const material = new THREE.ShaderMaterial({ uniforms: u, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide });
  const camera = new THREE.PerspectiveCamera(FOV, W / H, 1, 2000);
  camera.up.set(0, 0, 1);
  return { renderer, canvas, u, material, camera, scene: new THREE.Scene() };
}

function draw(buffer, plate) {
  gl ??= setup();
  const { renderer, canvas, u, material, camera, scene } = gl;
  const p = parsePayload(buffer);
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.BufferAttribute(p.positions, 3));
  g.setAttribute('normal', new THREE.BufferAttribute(p.normals, 3));
  g.setAttribute('paint', new THREE.BufferAttribute(p.painted, 3, true));
  g.setAttribute('who', new THREE.BufferAttribute(new Float32Array(2 * p.triangles * 3), 2));
  const mesh = new THREE.Mesh(g, material);
  mesh.frustumCulled = false;
  scene.add(mesh);
  const lo = p.meta.tileLo, hi = p.meta.tileHi;
  const centre = new THREE.Vector3((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (Math.max(lo[2], -1) + hi[2]) / 2);
  const diag = Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - Math.max(lo[2], -1));
  const [elev, azim] = [40, -120];
  const dir = dirv(elev, azim).normalize();
  const dist = (diag / (2 * Math.tan((FOV * Math.PI) / 360) * Math.min(1, W / H))) * 1.0;
  camera.position.copy(centre).addScaledVector(dir, dist);
  camera.near = dist / 50; camera.far = dist * 6;
  camera.lookAt(centre);
  camera.updateProjectionMatrix();
  u.uCam.value.copy(camera.position);
  u.uDir.value.copy(centre).sub(camera.position).normalize();
  u.uL1.value.copy(dirv(18, azim - 20));
  u.uL2.value.copy(dirv(elev - 20, azim + 110));
  renderer.setClearColor(new THREE.Color(plate), 1);
  renderer.render(scene, camera);
  const url = canvas.toDataURL('image/png');
  scene.remove(mesh);
  g.dispose();
  return url;
}

/**
 * A picture of tile `id` (a data URL), or null when it could not be made (the reason is shown where it is
 * asked for).  `key` names the tile and its look (its mesh version, the theme's plate) so a changed tile is drawn again.
 */
export function pictureOf(id, key, plate) {
  const k = `${id}|${key}|${plate}`;
  if (!pictures.has(k)) {
    const job = chain.then(async () => {
      const buf = await host.fetchTier(id, { tier: 'super-low' });
      return draw(buf, plate);
    });
    // one failure does not stop the pictures after it
    chain = job.then(() => undefined, () => undefined);
    pictures.set(k, job);
  }
  return pictures.get(k);
}
