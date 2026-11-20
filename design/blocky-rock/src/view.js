/* ---------- drawing a rock: flat-shaded grey on the references' dark stage; three is (x, z, -y) ---------- */
(function (root) {
'use strict';
const STAGE = 0x4a4a4e, GREY = 0xb2b4b8;
/* flat planes, with the faces that bend by under 18 degrees shaded as one (the clay look of the sheet); the geometry is the same either way */
const CREASE = Math.cos(40 * Math.PI / 180);
function geometryOf(rock) {
  const nT = rock.T.length / 3, pos = new Float32Array(nT * 9), nor = new Float32Array(nT * 9), fn = [], key = new Map();
  for (let t = 0; t < nT; t++) {
    const a = rock.V[rock.T[t * 3]], b = rock.V[rock.T[t * 3 + 1]], c = rock.V[rock.T[t * 3 + 2]];
    let n = [(b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]), (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]), (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])];
    const area = Math.hypot(...n) || 1; fn.push({ n: n.map((x) => x / area), a: area });
    for (let k = 0; k < 3; k++) { const p = rock.V[rock.T[t * 3 + k]], kk = Math.round(p[0] * 1e4) + ',' + Math.round(p[1] * 1e4) + ',' + Math.round(p[2] * 1e4); let l = key.get(kk); if (!l) key.set(kk, l = []); l.push(t); pos[t * 9 + k * 3] = p[0]; pos[t * 9 + k * 3 + 1] = p[2]; pos[t * 9 + k * 3 + 2] = -p[1]; }
  }
  for (let t = 0; t < nT; t++) for (let k = 0; k < 3; k++) {
    const p = rock.V[rock.T[t * 3 + k]], l = key.get(Math.round(p[0] * 1e4) + ',' + Math.round(p[1] * 1e4) + ',' + Math.round(p[2] * 1e4)); let s = [0, 0, 0];
    for (const u of l) { const d = fn[u].n[0] * fn[t].n[0] + fn[u].n[1] * fn[t].n[1] + fn[u].n[2] * fn[t].n[2]; if (d >= CREASE) { s[0] += fn[u].n[0] * fn[u].a; s[1] += fn[u].n[1] * fn[u].a; s[2] += fn[u].n[2] * fn[u].a; } }
    const m = Math.hypot(...s) || 1; nor[t * 9 + k * 3] = s[0] / m; nor[t * 9 + k * 3 + 1] = s[2] / m; nor[t * 9 + k * 3 + 2] = -s[1] / m;
  }
  const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.BufferAttribute(pos, 3)); g.setAttribute('normal', new THREE.BufferAttribute(nor, 3)); return g;
}
function shadowTexture() {
  const c = document.createElement('canvas'); c.width = c.height = 128; const x = c.getContext('2d'), g = x.createRadialGradient(64, 64, 4, 64, 64, 62);
  g.addColorStop(0, 'rgba(0,0,0,.55)'); g.addColorStop(.55, 'rgba(0,0,0,.28)'); g.addColorStop(1, 'rgba(0,0,0,0)'); x.fillStyle = g; x.fillRect(0, 0, 128, 128);
  const t = new THREE.CanvasTexture(c); return t;
}
function lights(sc) {
  sc.add(new THREE.HemisphereLight(0xffffff, 0x8a8c92, .8));
  const key = new THREE.DirectionalLight(0xffffff, .34); key.position.set(-1.1, 1.8, 1.2); sc.add(key);
  const fill = new THREE.DirectionalLight(0xffffff, .16); fill.position.set(1.4, .6, .4); sc.add(fill);
}
let SHADOW = null;
function buildScene(rock) {
  const sc = new THREE.Scene(); sc.background = new THREE.Color(STAGE); lights(sc);
  const tones = [GREY, 0xa9acb2, 0xd6d8dc, 0x9ea1a8, 0xc0c2c7];
  (rock.multi || [rock]).forEach((q, i) => { const m = new THREE.Mesh(geometryOf(q), new THREE.MeshLambertMaterial({ color: rock.multi ? tones[i % tones.length] : GREY })); m.userData.rock = true; sc.add(m); });
  const r = Math.max(rock.dimsOut[0], rock.dimsOut[1]) * .75 + .05;
  if (!SHADOW) SHADOW = shadowTexture();
  const sh = new THREE.Mesh(new THREE.PlaneGeometry(r * 2, r * 2), new THREE.MeshBasicMaterial({ map: SHADOW, transparent: true, depthWrite: false })); sh.rotation.x = -Math.PI / 2; sh.position.y = -.002; sc.add(sh);
  return sc;
}
/* the references' view: from the corner at x = max, y = min, about 20 degrees up */
function camera(rock, aspect, o = {}) {
  const cam = new THREE.PerspectiveCamera(o.fov || 15, aspect, .05, 100), d = rock.dimsOut, size = Math.max(d[0], d[1], d[2]);
  const R = o.dist || size * (o.k || 6.2), el = (o.el || 21) * Math.PI / 180, az = (o.az == null ? 45 : o.az) * Math.PI / 180, cy = d[2] * .48;
  cam.position.set(R * Math.cos(el) * Math.sin(az), cy + R * Math.sin(el), R * Math.cos(el) * Math.cos(az)); cam.lookAt(0, cy, 0); cam.updateMatrixWorld(); return cam;
}
let GL = null;
function renderer() { if (!GL) { GL = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); GL.setPixelRatio(1); } return GL; }
function still(rock, w, h, o = {}) {
  const R = renderer(); R.setSize(w, h, false); const sc = buildScene(rock), cam = camera(rock, w / h, o); R.render(sc, cam);
  const cv = document.createElement('canvas'); cv.width = w; cv.height = h; cv.getContext('2d').drawImage(R.domElement, 0, 0);
  sc.traverse((m) => { if (m.userData.rock) { m.geometry.dispose(); m.material.dispose(); } }); return cv;
}
root.ROCKVIEW = { geometryOf, buildScene, camera, still, renderer, STAGE, GREY };
})(window);
