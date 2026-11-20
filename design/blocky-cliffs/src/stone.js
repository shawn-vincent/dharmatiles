/* ---------- the stone: a chiselled block ----------
   Frame: a = along the cliff (0 at its cut end), d = depth (grows toward the back), z = up. Lengths in figure heights (fh). */
const rng = (seed) => { let t = (seed >>> 0) + 0x6D2B79F5; return () => { t = (t + 0x6D2B79F5) >>> 0; let r = Math.imul(t ^ (t >>> 15), 1 | t); r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r; return ((r ^ (r >>> 14)) >>> 0) / 4294967296; }; };
const lerp = (a, b, t) => a + (b - a) * t;
const within = (r, a, b) => a + (b - a) * r();
/* a draw that favours the middle of its range: the mean of two uniform draws */
const mid = (r, a, b) => a + (b - a) * (r() + r()) / 2;

/* A stone's defaults: every stone on the page is this object with some numbers changed. */
const STONE = { w: .6, dp: .5, h: 1, a: 0, d: 0, z: 0,
  bevel: 0, bevelV: 0,    /* the same chamfer on every top edge, or every upright edge, as a share of the shortest side (what the single numbers of the chapters set) */
  bevelB: null,     /* the same along each bottom edge, as the same shares: only where the edge is exposed (a lintel, an overhang), mirrored from the top kind, rising a short way up the face */
  bevelT: null,     /* the depth into the top of a chamfer along each top edge, as a share of the shortest side: [cut-end side, far side, front, back]; none under the stone */
  drop: .02,        /* how far a top chamfer or a clipped corner cuts down the side, fh: small, so it lies nearly in the top plane (its depth into the top is the free number) */
  bevelVC: null,    /* a chamfer on each upright edge, as a share of the shorter plan side: [front cut-end, front far, back cut-end, back far] */
  clips: null,      /* top corners knocked off by an angled plane: [[corner 0-3, size as a share of the shortest side]], corner bit 0 the far side, bit 1 the back */
  taperA: 1, taperD: 1,   /* the top's size as a share of the bottom's, along and across */
  batter: null,           /* one-sided batter: how far each face has moved in at the top, fh: [cut-end side, far side, front, back] */
  dipA: 0, dipD: 0,       /* the top's slope along and across (rise per run); a big one makes a wedge top */
  botA: 0,                /* the bottom's slope along (for beds that dip) */
  leanA: 0, leanD: 0,     /* shear: how far the top is carried along and back, per unit height */
  yaw: 0 };
/* A stone is a box (its taper, batter, tilt and shear map applied to the eight corners) cut by planes, every cut convex: a chamfer along each exposed
   edge, top or bottom or upright (an edge is exposed when no other stone and not the ground touches most of its length), a plane across each clipped
   corner, and everywhere else a hair of a chamfer, so a crease catches the light. A stone flush on the ground or on a stone keeps a crisp bottom joint. */
const HAIR = .01;
const sub = (p, r) => [p[0] - r[0], p[1] - r[1], p[2] - r[2]], add = (p, r, k) => [p[0] + r[0] * k, p[1] + r[1] * k, p[2] + r[2] * k], cross = (p, r) => [p[1] * r[2] - p[2] * r[1], p[2] * r[0] - p[0] * r[2], p[0] * r[1] - p[1] * r[0]], dot = (p, r) => p[0] * r[0] + p[1] * r[1] + p[2] * r[2], unit = (p) => { const l = Math.hypot(...p) || 1; return [p[0] / l, p[1] / l, p[2] / l]; };
/* what a stone's numbers mean, once: its corners and its cut planes (the mesh and the fusing both read it) */
function stoneParams(o) {
  const q = Object.assign({}, STONE, o), m = Math.min(q.w, q.dp, q.h), pl = Math.min(q.w, q.dp), hx = q.w / 2, hy = q.dp / 2, hz = q.h / 2;
  const bt = q.batter || [0, 0, 0, 0], ta = q.taperA * Math.max(.2, (q.w - bt[0] - bt[1]) / q.w), tdp = q.taperD * Math.max(.2, (q.dp - bt[2] - bt[3]) / q.dp), sa0 = (bt[0] - bt[1]) / 2, sd0 = (bt[2] - bt[3]) / 2;
  const V = []; for (let i = 0; i < 2; i++) for (let j = 0; j < 2; j++) for (let k = 0; k < 2; k++) { let a = i ? hx : -hx, d = j ? hy : -hy, z = k ? hz : -hz;
    const t = z / q.h + .5; a = a * lerp(1, ta, t) + sa0 * t; d = d * lerp(1, tdp, t) + sd0 * t; z += t * (q.dipA * a + q.dipD * d) + (1 - t) * q.botA * a; const zz = z + q.h / 2; V[i * 4 + j * 2 + k] = [a + q.leanA * zz, d + q.leanD * zz, zz]; }
  const v = (i, j, k) => V[i * 4 + j * 2 + k], bT = q.bevelT || Array(4).fill(q.bevel || 0), bB = q.bevelB || [0, 0, 0, 0], vc = q.bevelVC || Array(4).fill(q.bevelV || 0), clips = q.clips || (q.cut > 0 ? [[q.cutAt, q.cut * 1.2]] : []);
  const c = [0, 0, 0]; for (const p of V) for (let x = 0; x < 3; x++) c[x] += p[x] / 8;
  const planes = [], cut = (p0, p1, p2) => { let n = unit(cross(sub(p1, p0), sub(p2, p0))); if (dot(n, sub(p0, c)) < 0) n = [-n[0], -n[1], -n[2]]; planes.push({ n, d: dot(n, p0) }); };
  /* a horizontal edge E1-E2, top (k 1) or bottom (k 0): its chamfer runs e in across the face and ez toward the stone's middle, up or down */
  [1, 0].forEach((k) => [[v(0, 0, k), v(0, 1, k), v(1, 0, k), v(0, 0, 1 - k)], [v(1, 0, k), v(1, 1, k), v(0, 0, k), v(1, 0, 1 - k)], [v(0, 0, k), v(1, 0, k), v(0, 1, k), v(0, 0, 1 - k)], [v(0, 1, k), v(1, 1, k), v(0, 0, k), v(0, 1, 1 - k)]].forEach(([E1, E2, F1, G1], n) => {
    const across = Math.hypot(...sub(F1, E1)), e = Math.min(Math.max((k ? bT : bB)[n] * m, HAIR), .45 * across), ez = Math.min(q.drop, .58 * e, .5 * q.h);   /* 30 degrees at the steepest */
    const de = unit(sub(E2, E1)), perp = (p) => unit(sub(p, [de[0] * dot(p, de), de[1] * dot(p, de), de[2] * dot(p, de)])), A = add(E1, perp(sub(F1, E1)), e), B = add(E1, perp(sub(G1, E1)), ez);   /* the cut runs parallel to the edge */
    cut(A, B, add(A, de, 1)); }));
  /* an upright edge, cut c in along both faces */
  for (let i = 0; i < 2; i++) for (let j = 0; j < 2; j++) { const w = Math.min(Math.max(vc[i + 2 * j] * pl, HAIR), .45 * pl), B = v(i, j, 0), T = v(i, j, 1);
    cut(add(B, unit(sub(v(1 - i, j, 0), B)), w), add(B, unit(sub(v(i, 1 - j, 0), B)), w), add(T, unit(sub(v(1 - i, j, 1), T)), w)); }
  for (const [k, sz] of clips) { const i = k & 1, j = k >> 1 & 1, T = v(i, j, 1), s = Math.min(sz * m, .45 * pl);
    cut(add(T, unit(sub(v(1 - i, j, 1), T)), s), add(T, unit(sub(v(i, 1 - j, 1), T)), s), add(T, unit(sub(v(i, j, 0), T)), Math.min(q.drop, .58 * s))); }
  if (q.plain) planes.length = 0;   /* a plain slab: the plate */
  return { q, m, V, c, planes, rh: Math.max(HAIR, ...vc.map((x) => x * pl)), rT: Math.max(HAIR, Math.min(q.drop, .58 * bT[2] * m)), rB: HAIR,   /* what a fused joint must grow over: the widest upright chamfer, the front top chamfer */
     cs: Math.cos(q.yaw), sn: Math.sin(q.yaw) };
}
/* the polyhedron: the box's six faces, each cut by every plane, and a new face where a plane cuts */
function stoneMesh(o) {
  const { q, V, c, planes, cs, sn } = stoneParams(o), v = (i, j, k) => V[i * 4 + j * 2 + k];
  let F = [[v(0, 0, 0), v(0, 1, 0), v(1, 1, 0), v(1, 0, 0)], [v(0, 0, 1), v(1, 0, 1), v(1, 1, 1), v(0, 1, 1)], [v(0, 0, 0), v(1, 0, 0), v(1, 0, 1), v(0, 0, 1)], [v(0, 1, 0), v(0, 1, 1), v(1, 1, 1), v(1, 1, 0)], [v(0, 0, 0), v(0, 0, 1), v(0, 1, 1), v(0, 1, 0)], [v(1, 0, 0), v(1, 1, 0), v(1, 1, 1), v(1, 0, 1)]];
  for (const pl of planes) { const side = (p) => pl.n[0] * p[0] + pl.n[1] * p[1] + pl.n[2] * p[2] - pl.d, next = [], edge = [];
    for (const f of F) { const g = []; for (let i = 0; i < f.length; i++) { const p = f[i], r = f[(i + 1) % f.length], sp = side(p), sr = side(r);
        if (sp <= 0) g.push(p); if ((sp < 0 && sr > 0) || (sp > 0 && sr < 0)) { const t = sp / (sp - sr), x = [p[0] + (r[0] - p[0]) * t, p[1] + (r[1] - p[1]) * t, p[2] + (r[2] - p[2]) * t]; g.push(x); edge.push(x); } }
      if (g.length >= 3) next.push(g); }
    if (edge.length >= 3) { const m0 = [0, 1, 2].map((x) => edge.reduce((a, p) => a + p[x], 0) / edge.length), u = Math.abs(pl.n[0]) < .9 ? [1, 0, 0] : [0, 1, 0], e1 = unit(cross(pl.n, u)), e2 = cross(pl.n, e1);
      const ang = (p) => Math.atan2(dot(sub(p, m0), e2), dot(sub(p, m0), e1)); edge.sort((p, r) => ang(p) - ang(r)); const cap = edge.filter((p, i) => i === 0 || Math.hypot(...sub(p, edge[i - 1])) > 1e-6); if (cap.length >= 3) next.push(cap); }
    F = next; }
  const P = [], idx = [];
  for (const f of F) { const n0 = P.length; let n = cross(sub(f[1], f[0]), sub(f[2], f[0])); const mf = [0, 1, 2].map((x) => f.reduce((a, p) => a + p[x], 0) / f.length); if (dot(n, sub(mf, c)) < 0) f.reverse();
    for (const p of f) P.push([q.a + p[0] * cs - p[1] * sn, q.d + p[0] * sn + p[1] * cs, q.z + p[2]]); for (let i = 1; i < f.length - 1; i++) idx.push(n0, n0 + i, n0 + i + 1); }
  return { P, idx, o };
}
const stonePoints = (o) => stoneMesh(o).P;

/* ---------- drawing: one hidden WebGL renderer makes still pictures; three = (a, z, d) ---------- */
const HAVE3 = typeof THREE !== 'undefined';
const V3 = (p) => new THREE.Vector3(p[0], p[2], p[1]);
const STAGE = 0x8c8c91, GREY = 0xbcbfc5;
let RENDER = null, RSIZE = '';
function renderer() { if (!RENDER) { RENDER = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); RENDER.setPixelRatio(1); RENDER.setClearColor(STAGE, 1); } return RENDER; }
/* what every picture shares is made once: a material for each colour, the plate and the figure */
const KEEP = { mat: new Map(), plate: new Map(), figure: null };
const material = (color) => { let m = KEEP.mat.get(color); if (!m) KEEP.mat.set(color, m = new THREE.MeshLambertMaterial({ color, side: THREE.DoubleSide })); return m; };
function geometryOf(list) {   /* one geometry for any number of meshes: positions joined, indices shifted */
  const pos = [], idx = []; let n = 0; for (const st of list) { for (const p of st.P) pos.push(p[0], p[2], p[1]); for (const i of st.idx) idx.push(i + n); n += st.P.length; }
  const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3)); g.setIndex(idx); g.computeVertexNormals(); return g;
}
const meshOf = (st, color = GREY) => new THREE.Mesh(geometryOf([st]), material(color));
const PLATE = 1.7;   /* the plate's side in fh (chapter 1: 2.4 to 2.8 base diameters, a figure 1.5 diameters tall) */
function buildScene(stones, o = {}) {
  const sc = new THREE.Scene();
  sc.add(new THREE.HemisphereLight(0xffffff, 0x5f6167, .56));
  const key = new THREE.DirectionalLight(0xffffff, .6); key.position.set(.5, 1.7, -1.3); sc.add(key);
  const fill = new THREE.DirectionalLight(0xffffff, .12); fill.position.set(-1.2, .5, -.2); sc.add(fill);
  if (o.plate !== false) { const pl = o.plateSize || [PLATE, PLATE], k = pl.join(); let m = KEEP.plate.get(k); if (!m) KEEP.plate.set(k, m = new THREE.Mesh(geometryOf([stoneMesh({ w: pl[0], dp: pl[1], h: .09, a: pl[0] / 2, d: pl[1] / 2, z: -.09, plain: 1 })]), material(0xb4b7bd))); sc.add(m); }
  if (o.color || o.shift) stones.forEach((st, i) => { const m = meshOf(st, o.color ? o.color(st, i) : GREY); if (o.shift) { const v = o.shift(st, i); m.position.set(v[0], v[2], v[1]); } m.userData.own = true; sc.add(m); });
  else if (stones.length) { const m = new THREE.Mesh(geometryOf(stones), material(GREY)); m.userData.own = true; sc.add(m); }
  if (o.figure) { const f = o.figure, mat = material(0xa9adb4); if (!KEEP.figure) KEEP.figure = [new THREE.CylinderGeometry(.33, .33, .05, 40), new THREE.CylinderGeometry(.11, .15, .95, 18)];
    const m = new THREE.Mesh(KEEP.figure[0], mat); m.position.set(f[0], .025, f[1]); sc.add(m); const b = new THREE.Mesh(KEEP.figure[1], mat); b.position.set(f[0], .525, f[1]); sc.add(b); }
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
  const cam = cameraFor(o.view || 'ref', w, hgt, o), R = renderer(), size = (w * px) + 'x' + (hgt * px); if (size !== RSIZE) { R.setSize(w * px, hgt * px, false); RSIZE = size; }
  const sc = buildScene(stones, o); R.render(sc, cam); cv.getContext('2d').drawImage(R.domElement, 0, 0);
  sc.traverse((m) => { if (m.userData.own) m.geometry.dispose(); });
  box.proj = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) / 2 * w, (1 - v.y) / 2 * hgt]; };
  return o.sub ? Object.assign(h('div', { style: 'min-width:0' }, box, h('div', { class: 'sub', html: o.sub })), { svg, proj: box.proj, box }) : box;
}
