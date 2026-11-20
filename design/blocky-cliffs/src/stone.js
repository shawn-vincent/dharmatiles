/* ---------- the stone: one bevelled box, shaped by a handful of numbers ----------
   Frame: a = along the cliff (0 at its cut end), d = depth (grows toward the back), z = up. Lengths in figure heights (fh). */
const rng = (seed) => { let t = (seed >>> 0) + 0x6D2B79F5; return () => { t = (t + 0x6D2B79F5) >>> 0; let r = Math.imul(t ^ (t >>> 15), 1 | t); r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r; return ((r ^ (r >>> 14)) >>> 0) / 4294967296; }; };
const lerp = (a, b, t) => a + (b - a) * t;
const within = (r, a, b) => a + (b - a) * r();
/* a draw that favours the middle of its range: the mean of two uniform draws */
const mid = (r, a, b) => a + (b - a) * (r() + r()) / 2;

/* A stone's defaults: every stone on the page is this object with some numbers changed. */
const STONE = { w: .6, dp: .5, h: 1, a: 0, d: 0, z: 0,
  bevel: .16,       /* how round the edges are: 0.16 gives a radius of 0.07 fh on the top and upright edges, whatever the stone's size */
  bevelT: null,     /* a top edge cut back further than its rounding, edge by edge: [cut-end side, far side, front, back], as shares of the shortest side (a shoulder cut) */
  bevelV: null, bevelVC: null,   /* an upright corner cut back, as a share of the shorter plan side: all four, or [front cut-end, front far, back cut-end, back far] */
  level: 0,         /* 0: stands on the ground (a small radius at the foot); above 0: rests on a stone (a larger radius below, so the bed joint is a groove) */
  taperA: 1, taperD: 1,   /* the top's size as a share of the bottom's, along and across */
  batter: null,           /* one-sided batter: how far each face has moved in at the top, fh: [cut-end side, far side, front, back] */
  dipA: 0, dipD: 0,       /* the top's slope along and across (rise per run); a big one makes a wedge top */
  botA: 0,                /* the bottom's slope along (for beds that dip) */
  leanA: 0, leanD: 0,     /* shear: how far the top is carried along and back, per unit height */
  crease: 0,              /* how far the middle of the front face stands out, fh */
  cut: 0, cutAt: 0,       /* one top corner knocked off: its size as a share of the shortest side, and which corner (0-3) */
  cushion: .035,          /* broad faces swell by this share of the shortest side */
  seed: 1, yaw: 0 };
/* A stone is a rounded box: the surface of a small lattice (nine places along each side, three of them round each edge), every node
   pushed out to a true rounded box, cut softly by any shoulder or corner planes, then battered, tilted, sheared and turned. */
const LAT_T = [0, .27, .58, 1];
function latAxis(hh, rLo, rHi) { const a = LAT_T.map((t) => -hh + rLo * t), b = LAT_T.map((t) => hh - rHi * t).reverse(), m0 = a[3], m1 = b[0]; return a.concat([m0 + (m1 - m0) / 2], b); }
/* what a stone's numbers mean, once: the one place its radii, cuts and batter are worked out (the mesh and the field both read it) */
function stoneParams(o) {
  const q = Object.assign({}, STONE, o), m = Math.min(q.w, q.dp, q.h), hx = q.w / 2, hy = q.dp / 2, hz = q.h / 2, pl = Math.min(q.w, q.dp);
  const R = Math.max(.002, .07 * q.bevel / .16), rh = Math.min(R, .4 * pl), rT = Math.min(R, .4 * q.h, .4 * pl), rB = Math.max(.002, Math.min(q.level > 0 ? R * .6 : R * .28, .3 * q.h));
  const bT = q.bevelT || [q.bevel, q.bevel, q.bevel, q.bevel], cuts = [];
  [[0, -1, 0], [1, 1, 0], [2, 0, -1], [3, 0, 1]].forEach(([i, sa, sd]) => { const half = sa ? hx : hy, e = Math.min(bT[i] * m, (sa ? q.w : q.dp) * .46), za = Math.min(e * .8, q.h * .55); if (e < 1.7 * rT) return; const l = Math.hypot(za, e), na = za / l, nz = e / l; cuts.push({ n: [sa * na, sd * na, nz], d: na * (half - e) + nz * hz }); });
  if (q.cut > 0) { const sa = q.cutAt & 1 ? 1 : -1, sd = q.cutAt & 2 ? 1 : -1, l = Math.sqrt(3); cuts.push({ n: [sa / l, sd / l, 1 / l], d: (hx + hy + hz - q.cut * m * 1.2) / l }); }
  const vc = q.bevelVC || (q.bevelV ? [q.bevelV, q.bevelV, q.bevelV, q.bevelV] : null);
  if (vc) vc.forEach((v, ci) => { const c = v * pl; if (c < 1.7 * rh) return; const sa = ci & 1 ? 1 : -1, sd = ci & 2 ? 1 : -1; cuts.push({ n: [sa * .7071, sd * .7071, 0], d: (hx + hy - c) * .7071 }); });
  const bt = q.batter || [0, 0, 0, 0], ta = q.taperA * Math.max(.2, (q.w - bt[0] - bt[1]) / q.w), tdp = q.taperD * Math.max(.2, (q.dp - bt[2] - bt[3]) / q.dp), sa0 = (bt[0] - bt[1]) / 2, sd0 = (bt[2] - bt[3]) / 2;
  return { q, m, hx, hy, hz, pl, rh, rT, rB, cuts, ta, tdp, sa0, sd0, c: Math.cos(q.yaw), s2: Math.sin(q.yaw), cu: q.cushion * m };
}
function stoneMesh(o) {
  const { q, m, hx, hy, hz, rh, rT, rB, cuts, ta, tdp, sa0, sd0, c, s2, cu } = stoneParams(o);
  const X = latAxis(hx, rh, rh), Y = latAxis(hy, rh, rh), Z = latAxis(hz, rB, rT), L = X.length - 1, P = [], id = new Map(), idx = [];
  const node = (i, j, k) => { const key = i * 10000 + j * 100 + k; let v = id.get(key); if (v != null) return v; let a = X[i], d = Y[j], z = Z[k];
    const rz = z > 0 ? rT : rB, ca = Math.max(-hx + rh, Math.min(hx - rh, a)), cd = Math.max(-hy + rh, Math.min(hy - rh, d)), cz = Math.max(-hz + rB, Math.min(hz - rT, z));
    const da = (a - ca) / rh, dd = (d - cd) / rh, dz = (z - cz) / rz, l = Math.hypot(da, dd, dz); if (l > 1e-9) { a = ca + da / l * rh; d = cd + dd / l * rh; z = cz + dz / l * rz; }
    const fa = i === 0 || i === L, fd = j === 0 || j === L;
    if (fa) a += Math.sign(a) * cu * (1 - (d / hy) ** 2) * (1 - (z / hz) ** 2); if (fd) d += Math.sign(d) * cu * (1 - (a / hx) ** 2) * (1 - (z / hz) ** 2);
    if (q.crease && j === 0) d -= q.crease * Math.max(0, 1 - Math.abs(a) / hx * 1.4) * (1 - Math.max(0, z / hz) ** 4);
    for (const ct of cuts) { const t = ct.n[0] * a + ct.n[1] * d + ct.n[2] * z - ct.d, mv = (t + Math.sqrt(t * t + .035 * .035)) / 2; a -= ct.n[0] * mv; d -= ct.n[1] * mv; z -= ct.n[2] * mv; }
    const t = z / q.h + .5; a = a * lerp(1, ta, t) + sa0 * t; d = d * lerp(1, tdp, t) + sd0 * t; z += t * (q.dipA * a + q.dipD * d) + (1 - t) * q.botA * a; const zz = z + q.h / 2; a += q.leanA * zz; d += q.leanD * zz;
    v = P.length; P.push([q.a + a * c - d * s2, q.d + a * s2 + d * c, q.z + zz]); id.set(key, v); return v; };
  const quad = (p0, p1, p2, p3) => { idx.push(p0, p1, p2, p0, p2, p3); };
  for (let u = 0; u < L; u++) for (let v = 0; v < L; v++) {
    quad(node(u, v, L), node(u + 1, v, L), node(u + 1, v + 1, L), node(u, v + 1, L)); quad(node(u, v, 0), node(u, v + 1, 0), node(u + 1, v + 1, 0), node(u + 1, v, 0));
    quad(node(u, 0, v), node(u + 1, 0, v), node(u + 1, 0, v + 1), node(u, 0, v + 1)); quad(node(u, L, v), node(u, L, v + 1), node(u + 1, L, v + 1), node(u + 1, L, v));
    quad(node(0, u, v), node(0, u, v + 1), node(0, u + 1, v + 1), node(0, u + 1, v)); quad(node(L, u, v), node(L, u + 1, v), node(L, u + 1, v + 1), node(L, u, v + 1)); }
  return { P, idx, o };
}
const stonePoints = (o) => stoneMesh(o).P;

/* ---------- drawing: one hidden WebGL renderer makes still pictures; three = (a, z, d) ---------- */
const HAVE3 = typeof THREE !== 'undefined';
const V3 = (p) => new THREE.Vector3(p[0], p[2], p[1]);
const STAGE = 0x8c8c91, GREY = 0xbcbfc5;
let RENDER = null;
function renderer() { if (!RENDER) { RENDER = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); RENDER.setPixelRatio(1); RENDER.setClearColor(STAGE, 1); } return RENDER; }
function meshOf(st, color = GREY) {
  const pos = []; for (const p of st.P) pos.push(p[0], p[2], p[1]);
  const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3)); g.setIndex(st.idx); g.computeVertexNormals();
  const grp = new THREE.Group(); grp.add(new THREE.Mesh(g, new THREE.MeshLambertMaterial({ color, side: THREE.DoubleSide }))); return grp;
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
