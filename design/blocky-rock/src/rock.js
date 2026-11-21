/* ---------- the rock: lobes, chamfered, joined ----------
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


/* ---------- chamfers: each convex edge rounded off by a chain of two or three planes, tangent to a circle of radius r that wanders along the edge ---------- */
function edgeKey(a, t, n1, n2, ax) {
  const p0 = add(a, mul(t, -a[ax] / t[ax])), q = (v) => Math.round(v * 1000), nk = (n) => n.map((v) => Math.round(v * 100)).join(',');
  return ax + '|' + [0, 1, 2].filter((k) => k !== ax).map((k) => q(p0[k])).join(',') + '|' + [nk(n1), nk(n2)].sort().join('/');
}
function chamferPlanes(F, ctx) {
  if (ctx.noChamfer) return [];
  const key = (p) => Math.round(p[0] * 1e5) + ',' + Math.round(p[1] * 1e5) + ',' + Math.round(p[2] * 1e5), E = new Map();
  F.forEach((f, fi) => { for (let i = 0; i < f.p.length; i++) { const a = f.p[i], b = f.p[(i + 1) % f.p.length], ka = key(a), kb = key(b); if (ka === kb) continue; const k = ka < kb ? ka + '|' + kb : kb + '|' + ka; let e = E.get(k); if (!e) E.set(k, e = { a, b, f: [] }); if (!e.f.includes(fi)) e.f.push(fi); } });
  const out = [];
  for (const e of E.values()) {
    if (e.f.length !== 2) continue;
    let f1 = F[e.f[0]], f2 = F[e.f[1]]; if (f2.n[2] > f1.n[2]) [f1, f2] = [f2, f1];   /* nA: the face that is more up */
    const nA = f1.n, nB = f2.n, cs = dot(nA, nB);
    if (cs > 0.82) continue;   /* a shallow bend stays a crease */
    const a = e.a, b = e.b; if (len(sub(b, a)) < 0.04) continue;
    const bottom = nB[2] < -.95, top = !bottom && nA[2] > .5;
    if (bottom && !ctx.bottomBevel) continue;
    const t = norm(sub(b, a)), ax = Math.abs(t[0]) > Math.abs(t[1]) ? (Math.abs(t[0]) > Math.abs(t[2]) ? 0 : 2) : (Math.abs(t[1]) > Math.abs(t[2]) ? 1 : 2);
    const type = bottom ? 'bottom' : top ? 'top' : 'vert', k = edgeKey(a, t, nA, nB, ax), r = mulberry(hash(ctx.seed + ':' + k));
    const ext = (f, n) => { const g = norm(cross(n, t)); const sg = dot(g, sub(centroid(f.p), a)) < 0 ? -1 : 1; return Math.max(...f.p.map((p) => dot(sub(p, a), mul(g, sg)))); };
    const R = ctx.chamfer[type], jt = f1.pl.joint || f2.pl.joint, rad = Math.min(Math.max(jt ? ctx.rjoint : ctx.rmin, U(r, R[0], R[1]) * ctx.m * (jt ? ctx.jf : 1)), .3 * Math.min(ctx.center[0], ctx.center[1], ctx.center[2]) * 2);   /* capped by the whole rock, not by one lobe, so lobes that share an edge cut it alike */
    const E0 = add(a, mul(t, (ctx.center[ax] - a[ax]) / t[ax])), kv = mul(add(nA, nB), 1 / (1 + cs)), C = sub(E0, mul(kv, rad));
    const nf = bottom ? 2 : +pick(r, { 2: .4, 3: .6 }), S = bottom ? [.32, .56] : nf === 2 ? [.33, .67] : [.2, .5, .8];
    for (const s0 of S) {
      const s = Math.min(.95, Math.max(.05, s0 + U(r, -.07, .07)));
      let nn = norm(add(mul(nA, 1 - s), mul(nB, s))); if (-nn[2] > .8192) continue;   /* a face looking down no flatter than 35 degrees above horizontal */
      const rr = rad * U(r, .88, 1.15), P0 = add(C, mul(nn, rr)), tilt = norm(add(nn, mul(t, U(r, -.4, .4) * rad / Math.max(.2, ctx.center[ax]))));   /* the plane's width changes by up to 0.4 r from the middle of the edge to either end */
      if (-tilt[2] > .8192) continue;
      out.push({ n: tilt, d: dot(tilt, P0) });
    }
  }
  return out;
}

/* ---------- a part: a column with its own top, cut by chamfers, facets and chips ---------- */
function buildPart(sp, ctx) {
  const L = sp.lean, B = sp.bat, K = sp.sk || [0, 0, 0, 0], pl = [];
  pl.push(planeOf([-1, -K[0], L[0] + B[0]], -sp.x0)); pl.push(planeOf([1, K[1], -(L[0] - B[1])], sp.x1));
  pl.push(planeOf([-K[2], -1, L[1] + B[2]], -sp.y0)); pl.push(planeOf([K[3], 1, -(L[1] - B[3])], sp.y1));
  pl.push(planeOf([-sp.sx, -sp.sy, 1], sp.h0)); pl.push(planeOf([0, 0, -1], -(sp.z0 || 0)));
  for (const k of sp.joints || []) pl[k].joint = 1;
  let F = cube(40); for (const p of pl) F = clip(F, p);
  for (const p of sp.clips || []) F = clip(F, p);
  const F0 = F, cp = chamferPlanes(F, ctx); for (const p of cp) F = clip(F, p);
  if (!sound(F)) { F = F0; for (const p of cp) { const G = clip(F, p); if (sound(G)) F = G; } }   /* a chamfer plane that leaves a sliver is left out */
  for (const p of sp.post || []) F = clip(F, p);   /* battered corners: cut after the chamfers, so their edges stay broken, not rounded */
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
function meshOf(polys, noT) {
  const V = [], map = new Map(), Q = 1e4, cell = (p) => [Math.floor(p[0] * Q), Math.floor(p[1] * Q), Math.floor(p[2] * Q)];
  const id = (p) => { const c = cell(p); for (let i = -1; i <= 1; i++) for (let j = -1; j <= 1; j++) for (let k = -1; k <= 1; k++) { const l = map.get((c[0] + i) + ',' + (c[1] + j) + ',' + (c[2] + k)); if (l) for (const v of l) if (Math.abs(V[v][0] - p[0]) + Math.abs(V[v][1] - p[1]) + Math.abs(V[v][2] - p[2]) < 4e-5) return v; } const n = V.length; V.push(p); const kk = c.join(','); (map.get(kk) || map.set(kk, []).get(kk)).push(n); return n; };
  let loops = polys.map((f) => { const ids = []; for (const p of f.p) { const i = id(p); if (ids[ids.length - 1] !== i) ids.push(i); } if (ids.length > 1 && ids[0] === ids[ids.length - 1]) ids.pop(); return { ids, n: f.n }; }).filter((l) => l.ids.length >= 3);
  /* T-junctions: a vertex lying on another polygon's edge joins it */
  const idxBox = V.map((p) => p);
  for (const l of noT ? [] : loops) {
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
    const a = m.V[m.T[t]], b = m.V[m.T[t + 1]], c = m.V[m.T[t + 2]], cr = cross(sub(b, a), sub(c, a)); if (len(cr) < 3e-4) continue; const n = norm(cr);   /* a sliver has no real direction */
    if (n[2] < -.995 && a[2] < 1e-3 && b[2] < 1e-3 && c[2] < 1e-3) { flat++; continue; }
    if (-n[2] > worst) worst = -n[2];
  }
  return { open, many, edges: E.size, overhangDeg: Math.asin(Math.max(0, Math.min(1, worst))) * 180 / Math.PI, flatBottom: flat };
}


/* ---------- a rock: lobes (rounded-off prisms) grown together, from a recipe ----------
   recipe: { W, D, H, lean:[x,y], r: radius scale, seed, lobes:[{x0,x1,y0,y1,h, sx, sy, flat}], kinks:n }
   x runs along the left face (y = 0), y into the picture; a lobe's top is h (at its far corner) with slope sx along x. Lobes that touch share a joint plane,
   and the V between them is the two lobes' chamfers. */
function partsOf(rc, r) {
  const irr = rc.irr == null ? 1 : rc.irr, g = (a, b) => U(r, a, b) * irr;
  /* nothing is straight: every outer side has its own lean and its own twist in plan, the top its own small slope */
  const bat = [g(-.008, .025), g(-.008, .025), g(-.008, .025), g(-.008, .025)], skO = [g(-.03, .03), g(-.03, .03), g(-.03, .03), g(-.03, .03)], lean = [(rc.lean || [0, 0])[0] + g(-.02, .02), (rc.lean || [0, 0])[1] + g(-.008, .008)], tx = g(-.018, .018), ty = g(-.018, .018);
  const jx = {}, jy = {}, jk = (tab, key) => (tab[key] == null ? (tab[key] = g(-.09, .09)) : tab[key]), kk = (v) => Math.round(v * 1e4);
  return rc.lobes.map((q) => {
    const sx = (q.sx || 0) + tx, sy = (q.sy || 0) + ty, ht = q.h - sx * q.x1 - sy * q.y1, ox0 = q.x0 < 1e-9, ox1 = q.x1 > rc.W - 1e-9, oy0 = q.y0 < 1e-9, oy1 = q.y1 > rc.D - 1e-9;
    return { x0: q.x0, x1: q.x1, y0: q.y0, y1: q.y1, lean, bat: q.bat || [ox0 ? bat[0] : 0, ox1 ? bat[1] : 0, oy0 ? bat[2] : 0, oy1 ? bat[3] : 0], sx, sy, h0: ht, clips: q.clips || [], post: [],
      sk: [ox0 ? skO[0] : jk(jx, kk(q.x0)), ox1 ? skO[1] : jk(jx, kk(q.x1)), oy0 ? skO[2] : jk(jy, kk(q.y0)), oy1 ? skO[3] : jk(jy, kk(q.y1))],
      outer: [ox0, ox1, oy0, oy1],
      joints: [!ox0 ? 0 : -1, !ox1 ? 1 : -1, !oy0 ? 2 : -1, !oy1 ? 3 : -1].filter((k) => k >= 0) };
  });
}
/* a plane a few degrees off a face: wobble. face: 'L' (y=0), 'R' (x=W), 'T' top; at: where along it; ang in degrees; dep: how far below the face at the crease */
function kinkPlane(rc, B, face, at, ang, dir, dep) {
  const f = B.faces.filter((q) => !q.pl.joint && (face === 'L' ? q.n[1] < -.9 : face === 'R' ? q.n[0] > .9 : face === 'B' ? q.n[1] > .9 : q.n[2] > .9)).sort((a, b) => area(b.p, b.n) - area(a.p, a.n))[0];
  if (!f) return null;
  const c = centroid(f.p), up = face === 'T' ? [1, 0, 0] : [0, 0, 1], u1 = norm(cross(f.n, up)), u2 = norm(cross(f.n, u1));
  const w = norm(add(mul(u2, Math.cos(dir * Math.PI / 180)), mul(u1, Math.sin(dir * Math.PI / 180)))), al = ang * Math.PI / 180, n = norm(add(mul(f.n, Math.cos(al)), mul(w, Math.sin(al))));
  const ext = Math.max(...f.p.map((p) => Math.abs(dot(sub(p, c), w)))) || .1, q = add(add(c, mul(w, at * ext)), mul(f.n, -dep));
  return { n, d: dot(n, q) };
}
const sound = (F, shift, tj) => { const m = meshOf(F.map((f) => ({ p: shift ? f.p.map((q) => add(q, shift)) : f.p, n: f.n })), !tj), c = check(m); return c.open === 0 && c.many === 0 && c.overhangDeg <= 55.05; };
function randomKinks(r, rc, parts, ctx, n) {
  for (let i = 0; i < n; i++) {
    const pi = Math.floor(r() * parts.length), B = buildPart(parts[pi], ctx), face = pick(r, { L: .45, R: .35, T: .2 });
    const pl = kinkPlane(rc, B, face, U(r, -.4, .4), face === 'T' ? U(r, .5, 1.3) : U(r, 1.2, 3.2), face === 'T' ? U(r, 0, 90) : pick(r, { 0: .5, 45: .25, 135: .25 }) | 0, U(r, .012, .035) * ctx.m / .5);
    if (pl && -pl.n[2] < .8192) { parts[pi].clips.push(pl); if (!sound(buildPart(parts[pi], ctx).faces)) parts[pi].clips.pop(); }   /* a plane that leaves a sliver is not used */
  }
}
/* battered corners and edges: bold chunks of two to four planes knocked off exposed corners, low edges and top edges, some a third of an edge long; downward faces stay within 55 degrees */
function chips(r, parts, ctx, n) {
  for (let i = 0; i < n; i++) {
    const q = parts[Math.floor(r() * parts.length)], w = [q.x1 - q.x0, q.y1 - q.y0], H = q.h0 + q.sx * q.x1 + q.sy * q.y1;
    const sx = q.outer[0] && q.outer[1] ? (r() < .5 ? -1 : 1) : q.outer[0] ? -1 : q.outer[1] ? 1 : 0, sy = q.outer[2] && q.outer[3] ? (r() < .5 ? -1 : 1) : q.outer[2] ? -1 : q.outer[3] ? 1 : 0;
    if (!sx && !sy) continue;
    const kind = pick(r, { topcorner: .3, lowcorner: .2, lowvert: .2, bottomedge: .15, topedge: .15 });
    let sxx = sx, syy = sy, sz = 1, cz;
    const cx0 = sxx > 0 ? q.x1 : q.x0, cy0 = syy > 0 ? q.y1 : q.y0;
    let cx = cx0, cy = cy0;
    if (kind === 'topcorner') cz = q.h0 + q.sx * cx + q.sy * cy;
    else if (kind === 'lowcorner') { cz = H * U(r, .06, .3); sz = -1; }
    else if (kind === 'lowvert') { cz = H * U(r, .08, .4); sz = -1; if (r() < .5) { sxx = 0; cx = q.x0 + w[0] * U(r, .2, .8); } else { syy = 0; cy = q.y0 + w[1] * U(r, .2, .8); } if (!sxx && !syy) continue; }
    else if (kind === 'bottomedge') { cz = 0; sz = -1; if (r() < .5) { sxx = 0; cx = q.x0 + w[0] * U(r, .2, .8); } else { syy = 0; cy = q.y0 + w[1] * U(r, .2, .8); } }
    else { sz = 1; if (r() < .5) { sxx = 0; cx = q.x0 + w[0] * U(r, .2, .8); } else { syy = 0; cy = q.y0 + w[1] * U(r, .2, .8); } cz = q.h0 + q.sx * cx + q.sy * cy; }
    const top = kind === 'topcorner' || kind === 'topedge', depth = Math.min(U(r, .04, .16) * ctx.m, (top ? .1 : .17) * Math.min(...w, H * 1.4));
    for (let k = 0, nk = 2 + Math.floor(r() * 3); k < nk; k++) {
      const zs = sz; let nn = norm([sxx * U(r, .5, 1.3), syy * U(r, .5, 1.3), zs * U(r, .5, 1.1)]);   /* a low chunk has its planes tilted down, so each takes a cap off the bottom of the edge */ if (k) nn = norm(add(nn, [U(r, -.45, .45), U(r, -.45, .45), U(r, -.3, .4)]));
      const act = [sxx, syy, zs]; nn = nn.map((v, i) => (act[i] ? Math.sign(act[i]) * Math.max(Math.abs(v), .38 * len(nn)) : v)); nn = norm(nn);   /* every active axis counts, so no leg of the chunk runs far along one axis */
      if (-nn[2] > .8192) continue;
      const c = [cx + (k ? U(r, -.06, .06) : 0), cy + (k ? U(r, -.06, .06) : 0), cz + (k ? U(r, -.03, .03) : 0)], dd = depth * (k ? U(r, .4, .9) : 1);
      q.post.push({ n: nn, d: dot(nn, c) - dd }); if (!sound(buildPart(q, ctx).faces)) q.post.pop();
    }
  }
}
/* ---------- a crack: a wandering surface of facets (irregular both ways) separating a convex piece M from the rest S. M moves a little away. ---------- */
const polyKeep = (P, pl) => { const s = P.map((q) => dot(pl.n, q) - pl.d); if (s.every((v) => v <= EPS)) return P; if (s.every((v) => v >= -EPS)) return null; const lo = [], n = P.length;
  for (let i = 0; i < n; i++) { const a = P[i], b = P[(i + 1) % n], sa = s[i], sb = s[(i + 1) % n]; if (sa <= EPS) lo.push(a); if ((sa < -EPS && sb > EPS) || (sa > EPS && sb < -EPS)) { const t = sa / (sa - sb); lo.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]); } }
  return lo.length >= 3 ? lo : null; };
function crackSurface(r, rc) {
  const W = rc.W, D = rc.D, H = rc.H, size = Math.max(W, D, H), type = pick(r, { through: .35, corner: .3, top: .2, diag: .15 }), sg = () => (r() < .5 ? -1 : 1);
  let m0, pc;
  if (type === 'through') { const ax = r() < .7 ? 0 : 1; m0 = ax ? [U(r, -.2, .2), sg(), U(r, -.15, .15)] : [sg(), U(r, -.2, .2), U(r, -.15, .15)]; pc = [W * U(r, .3, .7), D * U(r, .35, .65), H * U(r, .35, .6)]; }
  else if (type === 'corner') { const sx = sg(), sy = sg(); m0 = [sx * U(r, .6, 1), sy * U(r, .6, 1), U(r, -.1, .2)]; pc = [sx > 0 ? W * U(r, .5, .75) : W * U(r, .25, .5), sy > 0 ? D * U(r, .5, .75) : D * U(r, .25, .5), H * U(r, .3, .6)]; }
  else if (type === 'top') { const sx = sg(), sy = r() < .5 ? 0 : sg() * .35; m0 = [sx * U(r, .6, .9), sy, U(r, .4, .7)]; pc = [sx > 0 ? W * U(r, .5, .8) : W * U(r, .2, .5), D * U(r, .35, .65), H * U(r, .45, .7)]; }
  else { m0 = [sg() * U(r, .5, 1), sg() * U(r, .5, 1), U(r, -.15, .15)]; pc = [W * U(r, .35, .65), D * U(r, .35, .65), H * U(r, .35, .6)]; }
  m0 = norm(m0);
  const u = norm(cross(m0, Math.abs(m0[2]) < .9 ? [0, 0, 1] : [1, 0, 0])), v = cross(m0, u), planes = [], L = .5 * size, J = .34;
  for (let i = 0; i < 8; i++) {
    const q = add(add(pc, mul(u, U(r, -L, L))), add(mul(v, U(r, -L, L)), mul(m0, U(r, -.03, .03) * size))), m = norm(add(m0, add(mul(u, U(r, -J, J)), mul(v, U(r, -J, J)))));
    if (m[2] > .8192 || m[2] < -.8192) continue; planes.push({ n: mul(m, -1), d: -dot(m, q) });   /* M = { m.p >= m.q }, held as the keep-side n.p <= d with n = -m */
  }
  return { type, m0, planes, gap: U(r, .01, .022), slip: U(r, -.5, .5) };
}
function crackPieces(pieces, cr) {
  const out = [], flatMove = cr.m0, sh = (minZ) => { const g = cr.gap, h = norm([cr.m0[0], cr.m0[1], 0]), t = [-h[1], h[0], 0]; return add(add(mul(h, g * len([cr.m0[0], cr.m0[1], 0])), mul(t, g * cr.slip)), [0, 0, minZ > .03 ? g * Math.max(0, cr.m0[2]) : 0]); };
  for (const pc of pieces) {
    let Mf = pc.F; for (const p of cr.planes) Mf = clip(Mf, p);
    const Mok = Mf.length >= 4 && aabbSize(Mf) > 4e-3;
    if (!Mok) { out.push(pc); continue; }
    const Sp = [];
    for (const f of pc.F) { let cur = f.p; for (const p of cr.planes) { const outside = polyKeep(cur, { n: mul(p.n, -1), d: -p.d }); if (outside && area(outside, f.n) > 1e-9) Sp.push({ p: outside, n: f.n }); cur = polyKeep(cur, p); if (!cur) break; } }
    for (const p of cr.planes) { const c = Mf.find((f) => f.pl === p); if (c) Sp.push({ p: c.p.slice().reverse(), n: mul(c.n, -1) }); }
    const mb = aabb(Mf.flatMap((f) => f.p));
    out.push({ F: Mf, shift: sh(mb.lo[2]), tag: 'moved' });
    out.push({ F: Sp, shift: [0, 0, 0], tag: 'rest', open: true });
  }
  return out;
}
const aabbSize = (F) => { const b = aabb(F.flatMap((f) => f.p)); return Math.min(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]); };

/* ---------- the simple rock: one block, every edge chamfered by a strip built directly as a mesh ----------
   Each edge gets a strip of N short segments between a line on one face and a line on the other. The width follows a smooth random curve along the
   edge and so does the tilt, so the strip swells, narrows and twists; each face is left with an inset polygon, and each corner is closed by a small
   patch joining the three strips' ends. Top edges lie 10-25 degrees below horizontal, bottom edges rise 62-68 degrees, upright edges about 45. */
function simpleRock(rc, seed, stage, N, bottomBevel) {
  const r0 = mulberry(hash('b' + seed)), one = { W: rc.W, D: rc.D, H: rc.H, lean: rc.lean, irr: rc.irr, lobes: [{ x0: 0, x1: rc.W, y0: 0, y1: rc.D, h: rc.H, sy: rc.kind === 'ramp' ? (rc.lobes[0].sy || 0) : 0 }] };
  const part = partsOf(one, r0)[0], F = buildPart(part, { noChamfer: true, m: 1, center: [0, 0, 0], seed: 'x', chamfer: { top: [0, 0], vert: [0, 0], bottom: [0, 0] } }).faces;
  if (stage === 0) { const m = meshOf(F.map((f) => ({ p: f.p, n: f.n }))); return { V: m.V, T: m.T }; }
  const V = [], key = (p) => Math.round(p[0] * 1e5) + ',' + Math.round(p[1] * 1e5) + ',' + Math.round(p[2] * 1e5), vmap = new Map(), vid = (p) => { const k = key(p); let i = vmap.get(k); if (i === undefined) { i = V.length; V.push(p); vmap.set(k, i); } return i; };
  const loops = F.map((f) => { const l = []; for (const p of f.p) { const i = vid(p); if (l[l.length - 1] !== i) l.push(i); } if (l.length > 1 && l[0] === l[l.length - 1]) l.pop(); return l; });
  const corners = V.slice(), nv = corners.length, edges = new Map(), faceN = F.map((f) => f.n), faceC = loops.map((l) => mul(l.reduce((a, i) => add(a, corners[i]), [0, 0, 0]), 1 / l.length));
  loops.forEach((l, fi) => { for (let k = 0; k < l.length; k++) { const a = l[k], b = l[(k + 1) % l.length], ka = Math.min(a, b) + '_' + Math.max(a, b); let e = edges.get(ka); if (!e) edges.set(ka, e = { a: Math.min(a, b), b: Math.max(a, b), f: [] }); e.f.push(fi); } });
  const m = Math.min(rc.W, rc.D), thin = Math.min(rc.W, rc.D, rc.H), P = [], Pb = [], tri = [];
  const pid = (p, base) => { P.push(p); Pb.push(base || p); return P.length - 1; };
  const N1 = N + 1;
  /* every face is a very slight dome or saddle, one arc each way, from a little concave to a little convex, never on the bottom */
  const fld = F.map((f, fi) => { const n = f.n, r = mulberry(hash(seed + ':f' + fi)), up = Math.abs(n[2]) > .9 ? [1, 0, 0] : [0, 0, 1], u = norm(cross(n, up)), v = cross(n, u), ext = [Math.max(...loops[fi].map((i) => Math.abs(dot(sub(corners[i], faceC[fi]), u)))) || 1, Math.max(...loops[fi].map((i) => Math.abs(dot(sub(corners[i], faceC[fi]), v)))) || 1];
    const A = f.n[2] < -.95 ? 0 : U(r, .0001, .0004) * Math.min(ext[0], ext[1]) * 2; return { n, u, v, ext, c: faceC[fi], A, dome: U(r, -1, 1), sad: U(r, -.5, .5), tl: U(r, -.03, .03), tl2: U(r, -.03, .03) }; });
  const bulge = (fi, p) => { const q = fld[fi]; if (!q.A) return p; const x = dot(sub(p, q.c), q.u) / q.ext[0], y = dot(sub(p, q.c), q.v) / q.ext[1], h = q.A * (q.dome * (1 - (x * x + y * y) / 2) + q.sad * (x * x - y * y) + q.tl * x + q.tl2 * y); return add(p, mul(q.n, h)); };
  /* pass 0: each edge's kind, size and angle */
  const info = new Map(); let ej = 0;
  for (const e of edges.values()) {
    const rw = mulberry(hash(seed + ':w' + (ej++))), A = corners[e.a], B = corners[e.b], t = norm(sub(B, A)), mid = mul(add(A, B), .5);
    let [f1, f2] = e.f; const n1 = faceN[f1], n2 = faceN[f2];
    const bottom = Math.min(n1[2], n2[2]) < -.95, top = !bottom && Math.max(n1[2], n2[2]) > .5, kind = bottom ? 'bottom' : top ? 'top' : 'vert';
    if (bottom ? n2[2] < n1[2] : top ? n2[2] > n1[2] : false) [f1, f2] = [f2, f1];   /* f1 carries the first leg: the top face, the bottom face */
    const dir = (fi) => { const c = sub(faceC[fi], mid), d = sub(c, mul(t, dot(c, t))); return norm(d); }, d1 = dir(f1), d2 = dir(f2);
    const ext = (fi, d) => Math.max(...loops[fi].map((i) => dot(sub(corners[i], mid), d)));
    const e1 = ext(f1, d1), e2 = ext(f2, d2);
    let w0, th0, tw;
    if (kind === 'top') { w0 = U(rw, .065, .12) * m; th0 = U(rw, 8, 27); tw = 4; } else if (kind === 'bottom') { w0 = U(rw, .07, .11) * m; th0 = U(rw, 40, 43); tw = 1;   /* an undercut: 35-40 degrees above horizontal, 50-55 from vertical */ } else { w0 = U(rw, .06, .11) * m; th0 = U(rw, 32, 58); tw = 8; }
    if (kind === 'bottom' && !bottomBevel) w0 = 1e-4;
    info.set(e, { kind, f1, f2, d1, d2, e1, e2, w0, th0, tw, A, B, t, mid, n1, n2 });
  }
  /* legs along the edge: one arc each; the same function gives a face's offset from this edge at any point, and at the ends it fixes the corner */
  const capLeg = (e1_) => Math.min(.16 * e1_, .24 * thin);   /* chamfers on a thin face stay small, so a slab does not pinch */
  let ei = 0;
  for (const e of edges.values()) {
    const r = mulberry(hash(seed + ':e' + (ei++))), q = info.get(e), aw = U(r, -.1, .1), at = U(r, -1, 1), bow = U(r, -.08, .08), ob = norm(add(q.n1, q.n2));
    q.legs = (tt) => { const sb = 1, w = (1 + aw * Math.sin(Math.PI * tt)) * q.w0 * sb;   /* an upright chamfer narrows toward the foot, so the bottom corner is small */
      const th = (q.th0 + q.tw * at * (2 * tt - 1)) * Math.PI / 180; let a = Math.min(w, capLeg(q.e1)), b = Math.min(w * Math.tan(th), capLeg(q.e2)); if (q.kind !== 'vert' && b < w * Math.tan(th) - 1e-9) a = Math.min(a, b / Math.tan(th)); return [a, b]; };
    q.bowAt = (tt) => mul(q.kind === 'bottom' ? [ob[0], ob[1], 0] : ob, bow * Math.sin(Math.PI * tt) * q.w0);
  }
  /* corners: on each face, the point where the two chamfers' lines meet; the three such points are the corner facet */
  const Q = new Map();   /* key face_vertex -> id */
  const qid = (fi, v) => { const k = fi + '_' + v; let id = Q.get(k); if (id !== undefined) return id;
    const es = [...edges.values()].filter((e) => (e.a === v || e.b === v) && e.f.includes(fi)), legOf = (e) => { const q = info.get(e), L = q.legs(e.a === v ? 0 : 1); return { d: q.f1 === fi ? q.d1 : q.d2, a: q.f1 === fi ? L[0] : L[1], t: norm(sub(corners[e.a === v ? e.b : e.a], corners[v])) }; };
    const L1 = legOf(es[0]), L2 = legOf(es[1]);   /* Q - v = x t1 + y t2 with dot(Q-v, d1) = a1, dot(Q-v, d2) = a2 */
    const m11 = dot(L1.t, L1.d), m12 = dot(L2.t, L1.d), m21 = dot(L1.t, L2.d), m22 = dot(L2.t, L2.d), det = m11 * m22 - m12 * m21 || 1e-9, x = (L1.a * m22 - m12 * L2.a) / det, y = (m11 * L2.a - m21 * L1.a) / det;
    const base = add(corners[v], add(mul(L1.t, x), mul(L2.t, y))); id = pid(bulge(fi, base), base); Q.set(k, id); return id; };
  const ids = new Map();
  for (const e of edges.values()) {
    const q = info.get(e), { f1, f2, d1, d2, A, B } = q, L1 = [], L2 = [], Ln = len(sub(B, A)), q1a = qid(f1, e.a), q2a = qid(f2, e.a), q1b = qid(f1, e.b), q2b = qid(f2, e.b);
    const tA = Math.max(0, (dot(sub(Pb[q1a], A), q.t) + dot(sub(Pb[q2a], A), q.t)) / 2) / Ln, tB = Math.max(0, (dot(sub(B, Pb[q1b]), q.t) + dot(sub(B, Pb[q2b]), q.t)) / 2) / Ln;   /* the strip runs between its two corner points, so it never folds back */
    for (let i = 0; i < N1; i++) {
      if (i === 0 || i === N) { const v = i ? e.b : e.a; L1.push(qid(f1, v)); L2.push(qid(f2, v)); continue; }
      const tt = i / N, [a, b] = q.legs(tt), E = add(A, mul(sub(B, A), tA + (1 - tA - tB) * tt)), bo = q.bowAt(tt), b1 = add(E, mul(d1, a)), b2 = add(E, mul(d2, b));
      { const c1 = add(b1, bo), c2 = add(b2, bo); L1.push(pid(bulge(f1, c1), c1)); L2.push(pid(bulge(f2, c2), c2)); }   /* the bow is part of the base position, so the face grid meets the strip exactly */
    }
    ids.set(e, [f1, f2, L1, L2]);
    for (let i = 0; i < N; i++) tri.push(Object.assign([L1[i], L1[i + 1], L2[i + 1], L2[i]], { ref: norm(add(q.n1, q.n2)) }));
  }
  const edgeOf = (a, b) => edges.get(Math.min(a, b) + '_' + Math.max(a, b)), W2 = (a, b) => a === b;
  loops.forEach((l, fi) => { const poly = []; for (let k = 0; k < l.length; k++) { const a = l[k], b = l[(k + 1) % l.length], e = edgeOf(a, b), [f1, , L1, L2] = ids.get(e), arr = f1 === fi ? L1 : L2; if (a === e.a) for (let i = 0; i < N1; i++) poly.push(arr[i]); else for (let i = N; i >= 0; i--) poly.push(arr[i]); }
    /* the face as an even grid: rows run between the first and third side's points; the other two sides are zipped to its straight edges */
    const nref = fld[fi].n, S = (k) => poly.slice(k * N1, (k + 1) * N1), s0 = S(0), s1 = S(1), s2 = S(2), s3 = S(3), top = s2.slice().reverse(), s3r = s3.slice().reverse(), R = N, G = [];
    /* a Coons patch between the four sides; their corners are shared, so it fits exactly and needs no seam */
    const c00 = Pb[s0[0]], c10 = Pb[s0[N]], c01 = Pb[top[0]], c11 = Pb[top[N]];
    for (let j = 0; j <= R; j++) { const v = j / R; if (j === 0) { G.push(s0); continue; } if (j === R) { G.push(top); continue; }
      G.push(s0.map((a, i) => { if (i === 0) return s3r[j]; if (i === N) return s1[j]; const u = i / N, bl = add(add(mul(c00, (1 - u) * (1 - v)), mul(c10, u * (1 - v))), add(mul(c01, (1 - u) * v), mul(c11, u * v)));
        const base = sub(add(add(mul(Pb[a], 1 - v), mul(Pb[top[i]], v)), add(mul(Pb[s3r[j]], 1 - u), mul(Pb[s1[j]], u))), bl); return pid(bulge(fi, base), base); })); }
    for (let j = 0; j < R; j++) for (let i = 0; i < N; i++) tri.push(Object.assign([G[j][i], G[j][i + 1], G[j + 1][i + 1], G[j + 1][i]], { ref: nref })); });
  for (let v = 0; v < nv; v++) {   /* the corner facet: one triangle (or quad if a face has one point per edge) */
    const fs = [...loops.keys()].filter((f) => loops[f].includes(v)), pts = fs.map((f) => qid(f, v)); let nm = [0, 0, 0]; for (const f of fs) nm = add(nm, faceN[f]);
    tri.push(Object.assign(pts, { ref: norm(nm) }));
  }
  const T = [], poly = (ts, ref) => { for (const [a, b, c] of ts) { if (a === b || b === c || a === c) continue; if (dot(cross(sub(P[b], P[a]), sub(P[c], P[a])), ref) < 0) T.push(a, c, b); else T.push(a, b, c); } };
  for (const q of tri) { if (q.length === 3) poly([[q[0], q[1], q[2]]], q.ref); else poly([[q[0], q[1], q[2]], [q[0], q[2], q[3]]], q.ref); }
  return { V: P, T };
}
function makeRock(seed, opts) {
  opts = opts || {}; const t0 = (typeof performance !== 'undefined' ? performance : Date).now();
  const rc = opts.recipe || ROCK.random(seed, opts), stage = opts.stage == null ? 3 : opts.stage;
  if (!opts.full) {
    const m = simpleRock(rc, rc.seed || seed, stage, 24, opts.bottomBevel !== false), chk = check(m), bb = aabb(m.V), cx = (bb.lo[0] + bb.hi[0]) / 2, cy = (bb.lo[1] + bb.hi[1]) / 2;
    return { rc, parts: [1], pieces: [1], check: chk, attempt: 1, kind: rc.kind || '', mesh: m, V: m.V.map((p) => [p[0] - cx, p[1] - cy, p[2]]), T: m.T, dimsOut: [bb.hi[0] - bb.lo[0], bb.hi[1] - bb.lo[1], bb.hi[2]], ms: (typeof performance !== 'undefined' ? performance : Date).now() - t0 };
  }
  const ctx = { seed: String(rc.seed || seed), m: Math.min(rc.W, rc.D), center: [rc.W / 2, rc.D / 2, rc.H / 2], rmin: .05, rjoint: .02, jf: rc.jf || .2, bottomBevel: opts.bottomBevel !== false, noChamfer: stage === 0, chamfer: rc.chamfer || { top: [.08, .13], vert: [.09, .15], bottom: [.07, .11] } };
  const r = mulberry(hash('k' + (rc.seed || seed))), parts = partsOf(rc, r);
  if (stage >= 2) randomKinks(r, rc, parts, ctx, rc.kinks == null ? 2 : rc.kinks);
  if (stage >= 2) chips(r, parts, ctx, rc.chips == null ? 0 : rc.chips);
  let pieces = parts.map((p) => ({ F: buildPart(p, ctx).faces, shift: [0, 0, 0], tag: 'lobe' }));
  if (stage >= 3 && rc.crack) { const cr = crackSurface(mulberry(hash('c' + (rc.seed || seed))), rc), np = crackPieces(pieces, cr); if (np.every((pc) => sound(pc.F, pc.shift, pc.open))) pieces = np; else rc.crack = null; }   /* a crack that leaves a sliver is dropped */
  const V = [], T = []; let chk = { open: 0, many: 0, edges: 0, overhangDeg: 0, flatBottom: 0 };
  for (const pc of pieces) {
    const m = meshOf(pc.F.map((f) => ({ p: f.p.map((q) => add(q, pc.shift)), n: f.n })), !pc.open), c = check(m), o = V.length;
    for (const p of m.V) V.push(p); for (const t of m.T) T.push(t + o);
    chk = { open: chk.open + c.open, many: chk.many + c.many, edges: chk.edges + c.edges, overhangDeg: Math.max(chk.overhangDeg, c.overhangDeg), flatBottom: chk.flatBottom + c.flatBottom };
  }
  const res = { rc, parts, pieces, check: chk, attempt: 1, kind: rc.kind || '', mesh: { V, T } };
  const bb = aabb(V), cx = (bb.lo[0] + bb.hi[0]) / 2, cy = (bb.lo[1] + bb.hi[1]) / 2;
  res.V = V.map((p) => [p[0] - cx, p[1] - cy, p[2]]); res.T = T;
  res.dimsOut = [bb.hi[0] - bb.lo[0], bb.hi[1] - bb.lo[1], bb.hi[2]];
  res.ms = (typeof performance !== 'undefined' ? performance : Date).now() - t0;
  return res;
}
root.ROCK = { makeRock, check, mulberry, RECIPES: {}, random: null, printable: false };
})(typeof window !== 'undefined' ? window : globalThis);
