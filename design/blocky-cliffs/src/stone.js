/* ---------- the stone: one bevelled box, shaped by a handful of numbers ----------
   Frame: a = along the cliff (0 at its cut end), d = depth (grows toward the back), z = up. Lengths in figure heights (fh). */
const rng = (seed) => { let t = (seed >>> 0) + 0x6D2B79F5; return () => { t = (t + 0x6D2B79F5) >>> 0; let r = Math.imul(t ^ (t >>> 15), 1 | t); r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r; return ((r ^ (r >>> 14)) >>> 0) / 4294967296; }; };
const lerp = (a, b, t) => a + (b - a) * t;
const within = (r, a, b) => a + (b - a) * r();
/* a draw that favours the middle of its range: the mean of two uniform draws */
const mid = (r, a, b) => a + (b - a) * (r() + r()) / 2;

/* The convex hull of a small set of points, as flat faces (each a ring of point indices, counter-clockwise seen from outside). */
function hull(P) {
  const n = P.length, faces = [], seen = new Set();
  let size = 0; for (const p of P) size = Math.max(size, Math.abs(p[0]), Math.abs(p[1]), Math.abs(p[2])); const eps = 1e-7 * (size || 1);
  for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) for (let k = j + 1; k < n; k++) {
    const ax = P[j][0] - P[i][0], ay = P[j][1] - P[i][1], az = P[j][2] - P[i][2], bx = P[k][0] - P[i][0], by = P[k][1] - P[i][1], bz = P[k][2] - P[i][2];
    let nx = ay * bz - az * by, ny = az * bx - ax * bz, nz = ax * by - ay * bx; const L = Math.hypot(nx, ny, nz); if (L < 1e-12) continue; nx /= L; ny /= L; nz /= L;
    let pos = 0, neg = 0; const on = [];
    for (let m = 0; m < n && !(pos && neg); m++) { const q = nx * (P[m][0] - P[i][0]) + ny * (P[m][1] - P[i][1]) + nz * (P[m][2] - P[i][2]); if (q > eps) pos++; else if (q < -eps) neg++; else on.push(m); }
    if (pos && neg) continue; if (pos) { nx = -nx; ny = -ny; nz = -nz; }
    const key = on.join(','); if (seen.has(key)) continue; seen.add(key);
    let cx = 0, cy = 0, cz = 0; for (const m of on) { cx += P[m][0]; cy += P[m][1]; cz += P[m][2]; } cx /= on.length; cy /= on.length; cz /= on.length;
    let ux = P[on[0]][0] - cx, uy = P[on[0]][1] - cy, uz = P[on[0]][2] - cz; const ul = Math.hypot(ux, uy, uz); ux /= ul; uy /= ul; uz /= ul;
    const vx = ny * uz - nz * uy, vy = nz * ux - nx * uz, vz = nx * uy - ny * ux;
    const ring = on.map((m) => [Math.atan2((P[m][0] - cx) * vx + (P[m][1] - cy) * vy + (P[m][2] - cz) * vz, (P[m][0] - cx) * ux + (P[m][1] - cy) * uy + (P[m][2] - cz) * uz), m]).sort((p, q) => p[0] - q[0]).map((p) => p[1]);
    faces.push({ ring, n: [nx, ny, nz] });
  }
  return faces;
}

/* A stone's defaults: every stone on the page is this object with some numbers changed. */
const STONE = { w: .6, dp: .5, h: 1, a: 0, d: 0, z: 0,
  bevel: .16,       /* width of the bevel round the top, as a share of the stone's shortest side */
  bevelV: null,     /* width of the chamfer on the four upright edges (the same as bevel when not given) */
  bevelB: 0,        /* the same for the bottom edges (0 for a stone that stands on the ground) */
  taperA: 1, taperD: 1,   /* the top's size as a share of the bottom's, along and across */
  dipA: 0, dipD: 0,       /* the top's slope along and across (rise per run) */
  botA: 0,                /* the bottom's slope along (for beds that dip) */
  leanA: 0, leanD: 0,     /* shear: how far the top is carried along and back, per unit height */
  cut: 0, cutAt: 0,       /* one top corner knocked off: its size as a share of the shortest side, and which corner (0-3) */
  jit: 0, seed: 1, yaw: 0 };
function stonePoints(o) {
  const q = Object.assign({}, STONE, o), r = rng(q.seed * 7919 + 13), m = Math.min(q.w, q.dp, q.h);
  const bt = q.bevel * m, bv = (q.bevelV == null ? q.bevel : q.bevelV) * m, bb = q.bevelB * m, P = []; let ci = 0;
  for (const sz of [-1, 1]) for (const sa of [-1, 1]) for (const sd of [-1, 1]) {
    const top = sz > 0, be = top ? bt : bb;
    let ca = bv, cd = bv, cz = be;
    if (top && q.cut > 0 && ((sa > 0 ? 1 : 0) + (sd > 0 ? 2 : 0)) === q.cutAt) { ca = Math.min(q.w * .45, q.cut * m); cd = Math.min(q.dp * .45, q.cut * m); cz = Math.min(q.h * .45, q.cut * m * .8); }
    const ja = (r() - .5) * 2 * q.jit * m, jd = (r() - .5) * 2 * q.jit * m, jz = (r() - .5) * 2 * q.jit * m * (top ? 1 : 0);
    const A = sa * q.w / 2 + ja, D = sd * q.dp / 2 + jd, Z = sz * q.h / 2 + jz;
    /* each corner: two points where the upright chamfer meets the side faces, and (on a bevelled end) two more on the end face, set in by the bevel */
    const three = [[A, D - sd * cd, Z - sz * cz], [A - sa * ca, D, Z - sz * cz]];
    if (cz > 0) three.push([A - sa * cz, D - sd * (cd + cz * .41), Z], [A - sa * (ca + cz * .41), D - sd * cz, Z]);
    else { three[0][2] = Z; three[1][2] = Z; }
    for (const p of three) P.push(p); ci++;
  }
  const c = Math.cos(q.yaw), s2 = Math.sin(q.yaw);
  return P.map(([a, d, z]) => { const t = z / q.h + .5;
    a *= lerp(1, q.taperA, t); d *= lerp(1, q.taperD, t);
    z += t * (q.dipA * a + q.dipD * d) + (1 - t) * q.botA * a;
    const zz = z + q.h / 2; a += q.leanA * zz; d += q.leanD * zz;
    return [q.a + a * c - d * s2, q.d + a * s2 + d * c, q.z + zz]; });
}
function stoneMesh(o) { const P = stonePoints(o); return { P, F: hull(P), o }; }

/* ---------- drawing: one hidden WebGL renderer makes still pictures; three = (a, z, d) ---------- */
const HAVE3 = typeof THREE !== 'undefined';
const V3 = (p) => new THREE.Vector3(p[0], p[2], p[1]);
const STAGE = 0x8c8c91, GREY = 0xbcbfc5;
let RENDER = null;
function renderer() { if (!RENDER) { RENDER = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); RENDER.setPixelRatio(1); RENDER.setClearColor(STAGE, 1); } return RENDER; }
function meshOf(st, color = GREY) {
  const pos = [], nor = [], edge = [];
  for (const f of st.F) { const r = f.ring; for (let i = 1; i + 1 < r.length; i++) for (const m of [r[0], r[i + 1], r[i]]) { const p = st.P[m]; pos.push(p[0], p[2], p[1]); nor.push(f.n[0], f.n[2], f.n[1]); } }
  /* three's frame (a, z, d) is mirrored against (a, d, z), so each triangle is wound the other way round */
  const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3)); g.setAttribute('normal', new THREE.Float32BufferAttribute(nor, 3));
  const mesh = new THREE.Mesh(g, new THREE.MeshLambertMaterial({ color }));
  /* primary edges only: where two faces meet at more than 14 degrees */
  const byEdge = new Map(); st.F.forEach((f, fi) => { const r = f.ring; for (let i = 0; i < r.length; i++) { const a = r[i], b = r[(i + 1) % r.length], k = a < b ? a + '_' + b : b + '_' + a; (byEdge.get(k) || byEdge.set(k, []).get(k)).push(fi); } });
  for (const [k, fs] of byEdge) { if (fs.length === 2) { const n0 = st.F[fs[0]].n, n1 = st.F[fs[1]].n; if (n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2] > .97) continue; } const [a, b] = k.split('_').map(Number); edge.push(st.P[a][0], st.P[a][2], st.P[a][1], st.P[b][0], st.P[b][2], st.P[b][1]); }
  const eg = new THREE.BufferGeometry(); eg.setAttribute('position', new THREE.Float32BufferAttribute(edge, 3));
  const lines = new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color: 0x4b4f57, transparent: true, opacity: .55 }));
  const grp = new THREE.Group(); grp.add(mesh, lines); return grp;
}
const PLATE = 1.7;   /* the plate's side in fh (chapter 1: 2.4 to 2.8 base diameters, a figure 1.5 diameters tall) */
function buildScene(stones, o = {}) {
  const sc = new THREE.Scene();
  sc.add(new THREE.HemisphereLight(0xffffff, 0x5f6167, .56));
  const key = new THREE.DirectionalLight(0xffffff, .6); key.position.set(.5, 1.7, -1.3); sc.add(key);
  const fill = new THREE.DirectionalLight(0xffffff, .12); fill.position.set(-1.2, .5, -.2); sc.add(fill);
  if (o.plate !== false) { const pl = o.plateSize || [PLATE, PLATE]; sc.add(meshOf(stoneMesh({ w: pl[0], dp: pl[1], h: .09, a: pl[0] / 2, d: pl[1] / 2, z: -.09, bevel: .25, bevelB: .1 }), 0xb4b7bd)); }
  stones.forEach((st, i) => { const g = meshOf(st, o.color ? o.color(st, i) : GREY); if (o.shift) { const v = o.shift(st, i); g.position.set(v[0], v[2], v[1]); } sc.add(g); });
  if (o.figure) { const f = o.figure; const m = new THREE.Mesh(new THREE.CylinderGeometry(.33, .33, .05, 40), new THREE.MeshLambertMaterial({ color: 0xa9adb4 })); m.position.set(f[0], .025, f[1]); sc.add(m); const b = new THREE.Mesh(new THREE.CylinderGeometry(.11, .15, .95, 18), new THREE.MeshLambertMaterial({ color: 0xa9adb4 })); b.position.set(f[0], .525, f[1]); sc.add(b); }
  return sc;
}
/* cameras: 'ref' is the references' view (from the near corner, about 20 degrees up); the others are flat, true-shape views */
function cameraFor(view, w, hgt, o = {}) {
  const c = o.centre || [PLATE / 2, PLATE / 2, .8], span = o.span || 2.5, asp = w / hgt;
  if (view === 'ref' || view === 'ref2') { const cam = new THREE.PerspectiveCamera(o.fov || 15, asp, .1, 100), R = o.dist || 9.6, el = (o.el || 20) * Math.PI / 180, az = (o.az == null ? 45 : o.az) * Math.PI / 180;
    cam.position.set(c[0] - R * Math.cos(el) * Math.sin(az), c[2] + R * Math.sin(el), c[1] - R * Math.cos(el) * Math.cos(az)); cam.lookAt(c[0], c[2], c[1]); cam.updateMatrixWorld(); cam.updateProjectionMatrix(); return cam; }
  const cam = new THREE.OrthographicCamera(-span * asp / 2, span * asp / 2, span / 2, -span / 2, -50, 50);
  if (view === 'front') { cam.position.set(c[0], c[2], c[1] - 10); cam.up.set(0, 1, 0); }
  else if (view === 'side') { cam.position.set(c[0] - 10, c[2], c[1]); cam.up.set(0, 1, 0); }
  else if (view === 'top') { cam.position.set(c[0], c[2] + 10, c[1]); cam.up.set(0, 0, 1); }
  else { cam.position.set(c[0] - 7, c[2] + 5.2, c[1] - 7); cam.up.set(0, 1, 0); }
  cam.lookAt(c[0], c[2], c[1]); cam.updateMatrixWorld(); cam.updateProjectionMatrix(); return cam;
}
/* A still picture of some stones. Returns a .cell element holding a canvas (fixed pixel size, scaled by CSS) and an SVG over it for
   annotations; el.proj(a, d, z) gives a point's place in that SVG. */
function still(stones, o = {}) {
  const w = o.w || 360, hgt = o.h || 300, px = o.px || 2;
  const cv = h('canvas', { width: w * px, height: hgt * px, style: `aspect-ratio:${w}/${hgt}` });
  const svg = s('svg', { viewBox: `0 0 ${w} ${hgt}`, style: 'position:absolute;inset:0;width:100%;height:100%' });
  const box = h('div', { class: 'cell stage' }, cv, svg, o.tag ? h('span', { class: 'tag', text: o.tag }) : null);
  box.svg = svg;
  if (!HAVE3) { box.proj = () => [0, 0]; return box; }
  const cam = cameraFor(o.view || 'ref', w, hgt, o), R = renderer(); R.setSize(w * px, hgt * px, false);
  const sc = buildScene(stones, o); R.render(sc, cam); cv.getContext('2d').drawImage(R.domElement, 0, 0);
  sc.traverse((m) => { if (m.geometry) m.geometry.dispose(); if (m.material) m.material.dispose(); });
  box.proj = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) / 2 * w, (1 - v.y) / 2 * hgt]; };
  return o.sub ? Object.assign(h('div', { style: 'min-width:0' }, box, h('div', { class: 'sub', html: o.sub })), { svg, proj: box.proj, box }) : box;
}
