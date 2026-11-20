/* ---------- the rock: columns, chamfered, joined ----------
   A rock is a few convex columns standing on the ground (a split of the footprint, each part with its own top), every one cut by planes
   (a chamfer along each edge, broken into one to three facets whose width runs along the edge; a few big facets and corner chips), and the
   columns joined: a polygon of one column that lies inside another is dropped, the rest is the surface. Where two columns of the same height
   meet, their chamfers make a V-groove (a split); where they differ in height they make a step. Lengths are figure heights (fh); the frame is
   x to the right of the left face (the face on y = 0), y into the picture, z up. */
(function (root) {
'use strict';
const EPS = 1e-7;
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]], add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]], mul = (a, k) => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2], cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = (a) => Math.hypot(a[0], a[1], a[2]), norm = (a) => { const l = len(a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };
const mulberry = (seed) => { let t = seed >>> 0; return () => { t = (t + 0x6D2B79F5) >>> 0; let r = Math.imul(t ^ (t >>> 15), 1 | t); r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r; return ((r ^ (r >>> 14)) >>> 0) / 4294967296; }; };
const hash = (s) => { let h = 2166136261; for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619); return h >>> 0; };
const U = (r, a, b) => a + (b - a) * r();
const pick = (r, table) => { let t = 0; for (const k in table) t += table[k]; let x = r() * t; for (const k in table) { x -= table[k]; if (x < 0) return k; } return Object.keys(table)[0]; };
const planeOf = (n, d) => { const l = len(n); return { n: [n[0] / l, n[1] / l, n[2] / l], d: d / l }; };

/* ---------- convex solids as face lists, cut by planes ---------- */
function cube(S) {
  const f = (n, p) => ({ p, n, pl: { n, d: S } });
  return [f([1, 0, 0], [[S, -S, -S], [S, S, -S], [S, S, S], [S, -S, S]]), f([-1, 0, 0], [[-S, -S, -S], [-S, -S, S], [-S, S, S], [-S, S, -S]]),
    f([0, 1, 0], [[-S, S, -S], [-S, S, S], [S, S, S], [S, S, -S]]), f([0, -1, 0], [[-S, -S, -S], [S, -S, -S], [S, -S, S], [-S, -S, S]]),
    f([0, 0, 1], [[-S, -S, S], [S, -S, S], [S, S, S], [-S, S, S]]), f([0, 0, -1], [[-S, -S, -S], [-S, S, -S], [S, S, -S], [S, -S, -S]])];
}
const samePlane = (a, b) => dot(a.n, b.n) > 1 - 1e-10 && Math.abs(a.d - b.d) < 1e-9;
function clip(faces, pl) {
  for (const f of faces) if (samePlane(f.pl, pl)) return faces;
  const out = [], cut = [];
  for (const f of faces) {
    const P = f.p, n = P.length, s = P.map((q) => dot(pl.n, q) - pl.d);
    if (s.every((v) => v <= EPS)) { out.push(f); for (let i = 0; i < n; i++) if (Math.abs(s[i]) <= EPS) cut.push(P[i]); continue; }
    if (s.every((v) => v >= -EPS)) { for (let i = 0; i < n; i++) if (Math.abs(s[i]) <= EPS) cut.push(P[i]); continue; }
    const g = [];
    for (let i = 0; i < n; i++) {
      const a = P[i], b = P[(i + 1) % n], sa = s[i], sb = s[(i + 1) % n];
      if (sa <= EPS) g.push(a);
      if ((sa < -EPS && sb > EPS) || (sa > EPS && sb < -EPS)) { const t = sa / (sa - sb), x = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]; g.push(x); cut.push(x); }
      else if (Math.abs(sa) <= EPS) cut.push(a);
    }
    if (g.length >= 3) out.push({ p: g, n: f.n, pl: f.pl });
  }
  const u = []; for (const p of cut) if (!u.some((q) => Math.abs(q[0] - p[0]) + Math.abs(q[1] - p[1]) + Math.abs(q[2] - p[2]) < 1e-6)) u.push(p);
  if (u.length >= 3) {
    const c = mul(u.reduce((a, p) => add(a, p), [0, 0, 0]), 1 / u.length), e1 = norm(cross(pl.n, Math.abs(pl.n[0]) < .9 ? [1, 0, 0] : [0, 1, 0])), e2 = cross(pl.n, e1);
    u.sort((p, q) => Math.atan2(dot(sub(p, c), e2), dot(sub(p, c), e1)) - Math.atan2(dot(sub(q, c), e2), dot(sub(q, c), e1)));
    if (area(u, pl.n) > 1e-9) out.push({ p: u, n: pl.n, pl });
  }
  return out;
}
function area(P, n) { let a = [0, 0, 0]; for (let i = 0; i < P.length; i++) a = add(a, cross(P[i], P[(i + 1) % P.length])); return Math.abs(dot(a, n)) / 2; }
const centroid = (P) => mul(P.reduce((a, p) => add(a, p), [0, 0, 0]), 1 / P.length);
const aabb = (P) => { const lo = [1e9, 1e9, 1e9], hi = [-1e9, -1e9, -1e9]; for (const p of P) for (let k = 0; k < 3; k++) { if (p[k] < lo[k]) lo[k] = p[k]; if (p[k] > hi[k]) hi[k] = p[k]; } return { lo, hi }; };
const overlap = (a, b, m = 1e-6) => a.lo[0] <= b.hi[0] + m && b.lo[0] <= a.hi[0] + m && a.lo[1] <= b.hi[1] + m && b.lo[1] <= a.hi[1] + m && a.lo[2] <= b.hi[2] + m && b.lo[2] <= a.hi[2] + m;

/* ---------- chamfers: one to three planes along each edge, their width a straight line along the edge ---------- */
const FACE_BIAS = 1 / 1.4;   /* an underside chamfer rises 1/1.4 of its run: 55 degrees from vertical, so it prints */
function edgeKey(a, t, n1, n2, ax) {
  const p0 = add(a, mul(t, -a[ax] / t[ax])), q = (v) => Math.round(v * 1000), nk = (n) => n.map((v) => Math.round(v * 100)).join(',');
  const ns = [nk(n1), nk(n2)].sort().join('/');
  return ax + '|' + [0, 1, 2].filter((k) => k !== ax).map((k) => q(p0[k])).join(',') + '|' + ns;
}
function chamferPlanes(F, ctx) {
  if (ctx.noChamfer) return [];
  const key = (p) => Math.round(p[0] * 1e5) + ',' + Math.round(p[1] * 1e5) + ',' + Math.round(p[2] * 1e5), E = new Map();
  F.forEach((f, fi) => { for (let i = 0; i < f.p.length; i++) { const a = f.p[i], b = f.p[(i + 1) % f.p.length], ka = key(a), kb = key(b); if (ka === kb) continue; const k = ka < kb ? ka + '|' + kb : kb + '|' + ka; let e = E.get(k); if (!e) E.set(k, e = { a, b, f: [] }); if (!e.f.includes(fi)) e.f.push(fi); } });
  const out = [];
  for (const e of E.values()) {
    if (e.f.length !== 2) continue;
    const f1 = F[e.f[0]], f2 = F[e.f[1]], n1 = f1.n, n2 = f2.n;
    if (dot(n1, n2) > 0.82) continue;   /* a shallow bend is left as a crease */
    const a = e.a, b = e.b, L = len(sub(b, a)); if (L < 0.05) continue;
    const bottom = n1[2] < -.95 || n2[2] < -.95, top = !bottom && (n1[2] > .3 || n2[2] > .3);
    if (bottom && !ctx.bottomBevel) continue;
    const t = norm(sub(b, a)), ax = Math.abs(t[0]) > Math.abs(t[1]) ? (Math.abs(t[0]) > Math.abs(t[2]) ? 0 : 2) : (Math.abs(t[1]) > Math.abs(t[2]) ? 1 : 2);
    const inFace = (f) => { let g = norm(cross(f.n, t)); if (dot(g, sub(centroid(f.p), a)) < 0) g = mul(g, -1); return g; };
    const g1 = inFace(f1), g2 = inFace(f2), ext = (f, g) => Math.max(...f.p.map((p) => dot(sub(p, a), g)));
    const cap = .4 * Math.min(ext(f1, g1), ext(f2, g2));
    const type = bottom ? 'bottom' : top ? 'top' : 'vert', k = edgeKey(a, t, n1, n2, ax), r = mulberry(hash(ctx.seed + ':' + k));
    const nf = pick(r, { 1: .25, 2: .45, 3: .3 }) | 0, rng = ctx.chamfer[type], base = U(r, rng[0], rng[1]) * ctx.m * ((f1.pl.joint || f2.pl.joint) ? .26 : 1);
    /* which face is which: the bottom or top face takes the long leg */
    let bigIsF1 = bottom ? n1[2] < -.95 : top ? n1[2] > .3 && n1[2] >= n2[2] : true;
    const ratio = bottom ? FACE_BIAS : top ? U(r, .45, .95) : 1;
    for (let i = 0; i < nf; i++) {
      const e0 = base * U(r, .62, 1.38), slope = U(r, -.45, .45) * ctx.m;
      const jt = f1.pl.joint || f2.pl.joint, floor = Math.min(jt ? .022 : bottom ? .05 : .06, .11 * ctx.m), wid = (c) => Math.min(Math.max(floor, e0 + slope * c), Math.max(floor, .2 * ctx.m));   /* big silhouette features: none under 0.06 fh (a groove's two halves 0.045) */
      const leg = (c) => { const w = Math.min(wid(c), cap / Math.max(1, ratio < 1 ? 1 : ratio)); return bigIsF1 ? [w, w * ratio] : [w * ratio, w]; };
      const la = leg(a[ax]), lb = leg(b[ax]);
      const P1 = add(a, mul(g1, la[0])), P2 = add(a, mul(g2, la[1])), Q1 = add(b, mul(g1, lb[0]));
      let n = cross(sub(P2, P1), sub(Q1, P1)); if (len(n) < 1e-9) continue; n = norm(n); if (dot(n, add(n1, n2)) < 0) n = mul(n, -1);
      if (-n[2] > .8192) {   /* a chamfer facing down no flatter than 35 degrees above horizontal: lengthen the leg on the standing face, or leave the edge */
        let ok = false, k = 1;
        for (let t = 0; t < 3 && !ok; t++) { k *= 1.2; const ga = bigIsF1 ? [la[0], la[1] * k] : [la[0] * k, la[1]], gb = bigIsF1 ? [lb[0], lb[1] * k] : [lb[0] * k, lb[1]];
          const A1 = add(a, mul(g1, ga[0])), A2 = add(a, mul(g2, ga[1])), B1 = add(b, mul(g1, gb[0])); let m = cross(sub(A2, A1), sub(B1, A1)); if (len(m) < 1e-9) continue; m = norm(m); if (dot(m, add(n1, n2)) < 0) m = mul(m, -1); if (-m[2] <= .8192) { n = m; P1.splice(0, 3, ...A1); ok = true; } }
        if (!ok) continue;
      }
      out.push({ n, d: dot(n, P1) });
    }
  }
  return out;
}

/* ---------- a part: a column with its own top, cut by chamfers, facets and chips ---------- */
function buildPart(sp, ctx) {
  const L = sp.lean, B = sp.bat, pl = [];
  pl.push(planeOf([-1, 0, L[0] + B[0]], -sp.x0)); pl.push(planeOf([1, 0, -(L[0] - B[1])], sp.x1));
  pl.push(planeOf([0, -1, L[1] + B[2]], -sp.y0)); pl.push(planeOf([0, 1, -(L[1] - B[3])], sp.y1));
  pl.push(planeOf([-sp.sx, -sp.sy, 1], sp.h0)); pl.push(planeOf([0, 0, -1], -(sp.z0 || 0)));
  for (const k of sp.joints || []) pl[k].joint = 1;
  let F = cube(40); for (const p of pl) F = clip(F, p);
  for (const p of sp.clips || []) F = clip(F, p);
  for (const p of chamferPlanes(F, ctx)) F = clip(F, p);
  const box = aabb(F.flatMap((f) => f.p));
  return { faces: F, planes: F.map((f) => f.pl), box, sp };
}

/* ---------- joining: the surface of the union ---------- */
function splitFrag(P, pl) {
  const s = P.map((q) => dot(pl.n, q) - pl.d);
  if (s.every((v) => v <= EPS) || s.every((v) => v >= -EPS)) return [P];
  const lo = [], hi = [], n = P.length;
  for (let i = 0; i < n; i++) {
    const a = P[i], b = P[(i + 1) % n], sa = s[i], sb = s[(i + 1) % n];
    if (sa <= EPS) lo.push(a); if (sa >= -EPS) hi.push(a);
    if ((sa < -EPS && sb > EPS) || (sa > EPS && sb < -EPS)) { const t = sa / (sa - sb), x = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]; lo.push(x); hi.push(x); }
  }
  const r = []; if (lo.length >= 3) r.push(lo); if (hi.length >= 3) r.push(hi); return r;
}
function unite(parts) {
  const polys = [];
  parts.forEach((A, i) => {
    for (const f of A.faces) {
      let frags = [f.p];
      parts.forEach((B, j) => {
        if (j === i) return;
        for (const pl of B.planes) { const next = []; for (const g of frags) { if (!overlap(aabb(g), B.box)) next.push(g); else next.push(...splitFrag(g, pl)); } frags = next; }
      });
      for (const g of frags) {
        if (area(g, f.n) < 1e-10) continue;
        const c = centroid(g); let keep = true;
        for (let j = 0; j < parts.length && keep; j++) {
          if (j === i) continue; const B = parts[j]; if (!overlap({ lo: c, hi: c }, B.box, 1e-5)) continue;
          let smax = -1e9, kmax = -1; B.planes.forEach((pl, k) => { const s = dot(pl.n, c) - pl.d; if (s > smax) { smax = s; kmax = k; } });
          if (smax < -1e-6) keep = false;
          else if (smax <= 1e-6) { const d = dot(f.n, B.planes[kmax].n); if (d < 0 || j < i) keep = false; }
        }
        if (keep) polys.push({ p: g, n: f.n });
      }
    }
  });
  return polys;
}

/* ---------- a mesh: weld, repair T-junctions, triangulate, check ---------- */
function meshOf(polys) {
  const V = [], map = new Map(), Q = 1e5, id = (p) => { const k = Math.round(p[0] * Q) + ',' + Math.round(p[1] * Q) + ',' + Math.round(p[2] * Q); let i = map.get(k); if (i === undefined) { i = V.length; V.push(p); map.set(k, i); } return i; };
  let loops = polys.map((f) => { const ids = []; for (const p of f.p) { const i = id(p); if (ids[ids.length - 1] !== i) ids.push(i); } if (ids.length > 1 && ids[0] === ids[ids.length - 1]) ids.pop(); return { ids, n: f.n }; }).filter((l) => l.ids.length >= 3);
  /* T-junctions: a vertex lying on another polygon's edge joins it */
  const idxBox = V.map((p) => p);
  for (const l of loops) {
    const out = [];
    for (let i = 0; i < l.ids.length; i++) {
      const a = l.ids[i], b = l.ids[(i + 1) % l.ids.length], pa = V[a], pb = V[b]; out.push(a);
      const d = sub(pb, pa), L2 = dot(d, d), found = [];
      const lo = [Math.min(pa[0], pb[0]) - 1e-4, Math.min(pa[1], pb[1]) - 1e-4, Math.min(pa[2], pb[2]) - 1e-4], hi = [Math.max(pa[0], pb[0]) + 1e-4, Math.max(pa[1], pb[1]) + 1e-4, Math.max(pa[2], pb[2]) + 1e-4];
      for (let v = 0; v < V.length; v++) { if (v === a || v === b) continue; const q = V[v]; if (q[0] < lo[0] || q[0] > hi[0] || q[1] < lo[1] || q[1] > hi[1] || q[2] < lo[2] || q[2] > hi[2]) continue; const t = dot(sub(q, pa), d) / L2; if (t <= 1e-6 || t >= 1 - 1e-6) continue; if (len(sub(q, add(pa, mul(d, t)))) < 3e-5) found.push([t, v]); }
      found.sort((x, y) => x[0] - y[0]); for (const f of found) out.push(f[1]);
    }
    l.ids = out;
  }
  /* triangles: a fan from the centroid, so collinear repair points never make a flat triangle */
  const T = [], tn = [];
  for (const l of loops) {
    const P = l.ids.map((i) => V[i]);
    if (l.ids.length === 3) { T.push(l.ids[0], l.ids[1], l.ids[2]); }
    else { const c = centroid(P), ci = V.length; V.push(c); for (let i = 0; i < l.ids.length; i++) T.push(ci, l.ids[i], l.ids[(i + 1) % l.ids.length]); }
    const k = T.length / 3; for (let i = l.ids.length === 3 ? 1 : l.ids.length; i > 0; i--) tn.push(l.n);
    /* orient outward */
    for (let t = T.length - 3 * (l.ids.length === 3 ? 1 : l.ids.length); t < T.length; t += 3) { const n = cross(sub(V[T[t + 1]], V[T[t]]), sub(V[T[t + 2]], V[T[t]])); if (dot(n, l.n) < 0) { const x = T[t + 1]; T[t + 1] = T[t + 2]; T[t + 2] = x; } }
  }
  return { V, T };
}
function check(m) {
  const E = new Map(); let bad = 0;
  for (let t = 0; t < m.T.length; t += 3) for (let k = 0; k < 3; k++) { const a = m.T[t + k], b = m.T[t + (k + 1) % 3], key = a < b ? a + '_' + b : b + '_' + a; E.set(key, (E.get(key) || 0) + 1); }
  let open = 0, many = 0; for (const c of E.values()) { if (c === 1) open++; else if (c > 2) many++; }
  /* overhang: the steepest angle from vertical of a downward face that is not the flat bottom */
  let worst = 0, flat = 0;
  for (let t = 0; t < m.T.length; t += 3) {
    const a = m.V[m.T[t]], b = m.V[m.T[t + 1]], c = m.V[m.T[t + 2]], n = norm(cross(sub(b, a), sub(c, a)));
    if (n[2] < -.999 && a[2] < 1e-4 && b[2] < 1e-4 && c[2] < 1e-4) { flat++; continue; }
    if (-n[2] > worst) worst = -n[2];
  }
  return { open, many, edges: E.size, overhangDeg: Math.asin(Math.max(0, Math.min(1, worst))) * 180 / Math.PI, flatBottom: flat };
}

/* ---------- composing a rock from a seed ---------- */
const KINDS = ['slab', 'block', 'pillar', 'ramp', 'stepped'];
const MIX = { slab: .22, block: .30, pillar: .22, ramp: .26 };   /* the sheet: 9 slabs, 12 blocks, 9 pillars, 10 ramps of 40; "stepped" is a control, not a share */
/* what the sheet gives, as numbers (see the study): sizes are against the longer plan side W; cuts is how many splits; step the chance of a lowered corner */
const KIND_SPEC = {
  slab: { W: [.75, 1.15], D: [.55, .85], H: [.28, .42], cuts: { 1: .55, 2: .45 }, lean: 0, step: .15 },
  block: { W: [.5, .85], D: [.72, 1], H: [.5, .85], cuts: { 0: .17, 1: .5, 2: .33 }, lean: .1, step: .25 },
  pillar: { W: [.4, .6], D: [.6, .9], H: [1.6, 2.4], cuts: { 0: .78, 1: .22 }, lean: .6, step: .45 },
  ramp: { W: [.6, 1], D: [.6, .9], H: [.62, .95], cuts: { 0: .6, 1: .4 }, lean: .2, step: .5 },
};
function compose(seed, attempt, opts) {
  const r = mulberry(hash('rock' + seed + ':' + attempt));
  let kind = opts.kind && opts.kind !== 'any' ? opts.kind : pick(r, MIX), forceStep = false;
  if (kind === 'stepped') { kind = pick(r, { block: .45, pillar: .4, ramp: .15 }); forceStep = true; }
  const S = KIND_SPEC[kind];
  let W = U(r, ...S.W), D = W * U(r, ...S.D), H = W * U(r, ...S.H);
  if (kind === 'pillar') { H = Math.min(H, 1.3); } else if (kind === 'ramp') { H = W * U(r, ...S.H); }
  if (r() < .5) { const t = W; W = D; D = t; }   /* which plan side faces left */
  const size = Math.max(W, D, H);
  const ctx = { seed, size, m: Math.min(W, D), bottomBevel: opts.bottomBevel !== false, noChamfer: opts.stage === 0, chamfer: { top: [.08, .14], vert: [.08, .14], bottom: [.07, .12] } };
  /* lean: the whole column sheared; batter: a face moves in a little as it rises */
  const lean = [0, 0]; if (r() < S.lean) { const ax = r() < .7 ? 0 : 1; lean[ax] = U(r, .05, .14) * (r() < .5 ? -1 : 1); }
  const bat = [W, W, D, D].map((w) => (kind !== 'pillar' && r() < .45 ? U(r, .03, .08) * w / Math.max(H, w * 1.2) : 0));
  let sx = 0, sy = 0, h0 = H, flatLow = 0;
  if (kind === 'ramp') { const hL = H * U(r, .25, .45); sx = (H - hL) / W; h0 = hL; }
  /* the footprint, split into columns by cuts that run the full depth (a split, or a step when one side is lower) */
  let cols = [{ x0: 0, x1: W, y0: 0, y1: D, drop: 0 }];
  const ncut = forceStep ? Math.max(1, +pick(r, S.cuts)) : +pick(r, S.cuts), feats = [];
  const minw = Math.max(.3 * Math.min(W, D), H / 5, .1);
  for (let c = 0; c < ncut; c++) {
    const cand = cols.filter((q) => Math.max(q.x1 - q.x0, q.y1 - q.y0) > 2.2 * minw).sort((a, b) => (b.x1 - b.x0) * (b.y1 - b.y0) - (a.x1 - a.x0) * (a.y1 - a.y0));
    if (!cand.length) break;
    const q = cand[Math.min(cand.length - 1, r() < .7 ? 0 : 1)], wx = q.x1 - q.x0, wy = q.y1 - q.y0, alongX = (wx > wy) === (r() < .75) ? wx >= 2.2 * minw : wy < 2.2 * minw;
    const ax = (alongX && wx >= 2.2 * minw) || wy < 2.2 * minw;
    const lo = ax ? q.x0 : q.y0, hi = ax ? q.x1 : q.y1, u = lo + (hi - lo) * U(r, .3, .7);
    const a = ax ? { ...q, x1: u } : { ...q, y1: u }, b = ax ? { ...q, x0: u } : { ...q, y0: u };
    cols.splice(cols.indexOf(q), 1, a, b);
    feats.push({ k: 'split', axis: ax ? 'x' : 'y', at: u });
  }
  /* steps: a corner column lowered (the near corner of the left face, mostly): a split with one side lower */
  const stepNow = forceStep || r() < S.step, cutLeaf = (q, ax, u) => { const a = ax === 'x' ? { ...q, x1: u } : { ...q, y1: u }, b = ax === 'x' ? { ...q, x0: u } : { ...q, y0: u }; cols.splice(cols.indexOf(q), 1, a, b); feats.push({ k: 'split', axis: ax, at: u }); return [a, b]; };
  if (stepNow) {
    const atFar = r() < .35, cx = atFar ? W : 0;   /* the corner at x = 0 (the left end of the left face) or at x = W (the near corner) */
    let q = cols.find((c) => (c.x0 <= cx + 1e-9 && c.x1 >= cx - 1e-9) && c.y0 === 0);
    const wmin = Math.max(.34 * Math.min(W, D), H / 3.4, .1);
    if ((q.x1 - q.x0) > wmin * 2.1 && (q.x1 - q.x0) > .55 * W) { const lo = Math.max(wmin, kind === 'ramp' ? .28 * W : .2 * W), hi = Math.max(lo * 1.01, .5 * W), u = atFar ? q.x1 - U(r, lo, hi) : q.x0 + U(r, lo, hi), pr = cutLeaf(q, 'x', u); q = atFar ? pr[1] : pr[0]; }
    if ((q.y1 - q.y0) > wmin * 2.1 && r() < .6) { const v = U(r, Math.max(wmin, .35 * D), Math.max(wmin * 1.01, .7 * D)); q = cutLeaf(q, 'y', v)[0]; }
    if (kind === 'ramp') {
      /* a seat: a flat shelf lower than the ramp where the column stands, with the ramp rising behind it */
      const top = (x) => h0 + sx * x, hs = Math.min(top(q.x0) * U(r, .55, .9), top(q.x1) * U(r, .35, .6)); q.flat = Math.max(hs, .12 * size);
    } else q.drop = H * U(r, kind === 'pillar' ? .3 : .25, kind === 'pillar' ? .55 : .5);
    feats.push({ k: 'step', drop: q.drop || null, flat: q.flat || null, corner: atFar ? 'near' : 'left' });
  } else if (cols.length > 1 && r() < .3) { const q = cols[Math.floor(r() * cols.length)]; q.drop = H * U(r, .02, .05); feats.push({ k: 'offset', drop: q.drop }); }
  /* a bed line: every column cut at one height, joined behind so the groove stays on the near faces */
  let bed = null;
  if (kind !== 'ramp' && kind !== 'slab' && r() < .12) bed = { z: H * U(r, .5, .8), depth: Math.min(W, D) * U(r, .28, .5) };
  const parts = [];
  const topPlane = (q) => q.flat ? { sx: 0, sy: 0, h0: q.flat } : { sx, sy, h0: h0 - q.drop };
  for (const q of cols) {
    const tp = topPlane(q), ht = tp.h0 + tp.sx * q.x1 + tp.sy * q.y1;
    const base = { x0: q.x0, x1: q.x1, y0: q.y0, y1: q.y1, lean, bat: [q.x0 === 0 ? bat[0] : 0, q.x1 === W ? bat[1] : 0, q.y0 === 0 ? bat[2] : 0, q.y1 === D ? bat[3] : 0], ...tp, clips: [], joints: [q.x0 !== 0 ? 0 : -1, q.x1 !== W ? 1 : -1, q.y0 !== 0 ? 2 : -1, q.y1 !== D ? 3 : -1].filter((k) => k >= 0) };
    if (bed && ht - .22 * size > bed.z && !q.flat && q.y0 < bed.depth + 1e-9) { parts.push({ ...base, sx: 0, sy: 0, h0: bed.z, bedLow: 1, joints: [...base.joints, 4] }, { ...base, z0: bed.z, joints: [...base.joints, 5] }); } else parts.push(base);
  }
  if (bed) { const hmin = Math.min(...parts.filter((p) => !p.bedLow).map((p) => p.h0 + p.sx * p.x1 + p.sy * p.y1)); parts.push({ x0: 0, x1: W, y0: bed.depth, y1: D, lean, bat: [bat[0], bat[1], 0, bat[3]], sx, sy, h0: hmin - 0 - sx * W, clips: [] }); feats.push({ k: 'bed', z: bed.z }); }
  /* facets and chips on the parts: planes at slight angles; planes of corners cut off */
  const nfac = +pick(r, { 0: .1, 1: .3, 2: .35, 3: .25 }), nchip = +pick(r, { 0: .45, 1: .35, 2: .2 });
  const dims = { W, D, H, size };
  const built0 = parts.map((p) => buildPart(p, ctx));
  if (opts.stage == null || opts.stage >= 2) for (let i = 0; i < nfac; i++) addFacet(r, parts, built0, ctx, dims, feats);
  if (opts.stage == null || opts.stage >= 2) for (let i = 0, nk = 1 + Math.floor(r() * 4); i < nk; i++) addFacet(r, parts, built0, ctx, dims, feats, true);
  if (opts.stage == null || opts.stage >= 2) for (let i = 0; i < nchip; i++) addChip(r, parts, built0, ctx, dims, feats);
  return { parts, feats, kind, dims, ctx, lean, bat, forceStep };
}
const CHIPS_MAX_TRY = 8, bvol = (b) => (b.box.hi[0] - b.box.lo[0]) * (b.box.hi[1] - b.box.lo[1]) * (b.box.hi[2] - b.box.lo[2]);
function pickPart(r, parts, built) { let tot = 0; const w = built.map((b) => { const q = Math.max(.02, (b.box.hi[0] - b.box.lo[0]) * (b.box.hi[1] - b.box.lo[1])); tot += q; return q; }); let x = r() * tot; for (let i = 0; i < w.length; i++) { x -= w[i]; if (x < 0) return i; } return 0; }
/* a facet: a plane a few degrees off a face, cutting a shallow wedge from it */
function addFacet(r, parts, built, ctx, dims, feats, kink) {
  const i = pickPart(r, parts, built), B = built[i], faces = B.faces.filter((f) => !f.pl.joint && (Math.abs(f.n[2]) < .6 || (f.n[2] > .6 && f.n[2] < .95)));
  for (let tr = 0; tr < CHIPS_MAX_TRY; tr++) {
    const f = faces[Math.floor(r() * faces.length)]; if (!f || area(f.p, f.n) < .03 * dims.size * dims.size) continue;
    const side = Math.abs(f.n[2]) < .6, c = centroid(f.p);
    /* in-face direction the plane tilts toward: mostly vertical on a side, diagonal sometimes */
    const up = side ? [0, 0, 1] : [1, 0, 0], u1 = norm(cross(f.n, up)), u2 = norm(cross(f.n, u1));
    const ang = side ? pick(r, { v: .45, d: .4, h: .15 }) : 'h', a = ang === 'v' ? 0 : ang === 'h' ? Math.PI / 2 : (r() < .5 ? 1 : -1) * U(r, .35, .65) * Math.PI / 2;
    const w = norm(add(mul(u2, Math.cos(a)), mul(u1, Math.sin(a)))), al = (kink ? U(r, 2.5, 6) : U(r, 6, side ? 24 : 9)) * Math.PI / 180;
    let sgn = r() < .5 ? 1 : -1; if (Math.abs(w[2]) > .3) sgn = Math.sign(w[2]) * (r() < .85 ? 1 : -1);   /* a plane that is tilted up the face narrows the rock toward the top, as the sheet's pillars do; down, rarely */
    const n = norm(add(mul(f.n, Math.cos(al)), mul(w, sgn * Math.sin(al))));
    const ext = Math.max(...f.p.map((p) => Math.abs(dot(sub(p, c), w)))) || .1, off = U(r, -.5, .35) * ext, dep = (kink ? U(r, .015, .05) : U(r, .05, .2)) * Math.min(ctx.m, 1.2 * Math.min(B.box.hi[0] - B.box.lo[0], B.box.hi[1] - B.box.lo[1]));
    const q = add(add(c, mul(w, sgn * off)), mul(f.n, -dep)), pl = { n, d: dot(n, q) };
    if (-pl.n[2] > .8192) continue;   /* a downward plane no flatter than 35 degrees above horizontal */
    parts[i].clips.push(pl); const nb = buildPart(parts[i], ctx); if (bvol(nb) < .72 * bvol(B)) { parts[i].clips.pop(); continue; } built[i] = nb; if (!kink) feats.push({ k: 'facet', part: i, deg: +(al * 180 / Math.PI).toFixed(1) }); return;
  }
}
/* a chip: the plane through a point on each of the three edges at a corner, the legs unequal */
function addChip(r, parts, built, ctx, dims, feats) {
  const i = pickPart(r, parts, built), B = built[i];
  for (let tr = 0; tr < CHIPS_MAX_TRY; tr++) {
    const verts = []; for (const f of B.faces) for (const p of f.p) if (!verts.some((q) => len(sub(p, q)) < 1e-5)) verts.push(p);
    const lowBias = r() < .45, cand = verts.filter((v) => (lowBias ? v[2] < .12 * dims.H + 1e-3 : v[2] > .4 * dims.H));
    if (!cand.length) continue;
    const v = cand[Math.floor(r() * cand.length)], at = B.faces.filter((f) => f.p.some((p) => len(sub(p, v)) < 1e-5));
    if (at.length < 3 || at.some((f) => f.pl.joint)) continue;   /* never into a face that is a joint: that would open the groove into a gap */
    /* edges at the corner: directions to the neighbouring vertices along faces */
    const dirs = [];
    for (const f of at) { const k = f.p.findIndex((p) => len(sub(p, v)) < 1e-5), n = f.p.length; for (const nb of [f.p[(k + 1) % n], f.p[(k + n - 1) % n]]) { const d = norm(sub(nb, v)); if (!dirs.some((e) => dot(e, d) > .999)) dirs.push(d); } }
    if (dirs.length !== 3) continue;
    const m = Math.min(dims.W, dims.D, dims.H, 1), legs = [U(r, .16, .4), U(r, .16, .4), U(r, .16, .4)].map((x) => x * Math.max(.35, m) * 1);
    const pts = dirs.map((d, k) => add(v, mul(d, Math.min(legs[k], .45 * len(sub(B.box.hi, B.box.lo)) / 2))));
    let n = cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])); if (len(n) < 1e-9) continue; n = norm(n);
    const out = mul(norm(at.reduce((a, f) => add(a, f.n), [0, 0, 0])), 1); if (dot(n, out) < 0) n = mul(n, -1);
    if (-n[2] > .8192) continue;
    parts[i].clips.push({ n, d: dot(n, pts[0]) }); const nb = buildPart(parts[i], ctx); if (bvol(nb) < .8 * bvol(B)) { parts[i].clips.pop(); continue; } built[i] = nb; feats.push({ k: 'chip', part: i, low: v[2] < .3 * dims.H }); return;
  }
}

function makeRock(seed, opts) {
  opts = opts || {}; const t0 = (typeof performance !== 'undefined' ? performance : Date).now();
  let res = null;
  for (let attempt = opts.attempt ? opts.attempt - 1 : 0; attempt < (opts.attempt ? opts.attempt : 10); attempt++) {
    const c = compose(seed, attempt, opts), built = c.parts.map((p) => buildPart(p, c.ctx)), polys = unite(built), m = meshOf(polys), chk = check(m);
    res = { ...c, built, polys, mesh: m, check: chk, attempt: attempt + 1 };
    if (chk.open === 0 && chk.many === 0 && chk.overhangDeg <= 55.5) break;
  }
  /* put the rock on the ground at the origin, scaled to its size in fh */
  const bb = aabb(res.mesh.V), want = opts.size, k = want ? want / Math.max(bb.hi[0] - bb.lo[0], bb.hi[1] - bb.lo[1], bb.hi[2]) : 1, cx = (bb.lo[0] + bb.hi[0]) / 2, cy = (bb.lo[1] + bb.hi[1]) / 2;
  res.V = res.mesh.V.map((p) => [(p[0] - cx) * k, (p[1] - cy) * k, p[2] * k]); res.T = res.mesh.T; res.scale = k;
  res.dimsOut = [(bb.hi[0] - bb.lo[0]) * k, (bb.hi[1] - bb.lo[1]) * k, bb.hi[2] * k];
  res.ms = (typeof performance !== 'undefined' ? performance : Date).now() - t0;
  return res;
}
/* each column on its own, in the rock's final frame (for showing the stages) */
function columns(res) {
  const bb = aabb(res.mesh.V), k = res.scale, cx = (bb.lo[0] + bb.hi[0]) / 2, cy = (bb.lo[1] + bb.hi[1]) / 2;
  return res.built.map((b) => { const m = meshOf(b.faces.map((f) => ({ p: f.p, n: f.n }))); return { V: m.V.map((p) => [(p[0] - cx) * k, (p[1] - cy) * k, p[2] * k]), T: m.T }; });
}
function columnMesh(b) { return meshOf(b.faces.map((f) => ({ p: f.p, n: f.n }))); }
root.ROCK = { makeRock, columns, columnMesh, KINDS, MIX, check, mulberry };
})(typeof window !== 'undefined' ? window : globalThis);
