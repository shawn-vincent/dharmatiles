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
  const V = [], map = new Map(), Q = 1e4, cell = (p) => [Math.floor(p[0] * Q), Math.floor(p[1] * Q), Math.floor(p[2] * Q)];
  const id = (p) => { const c = cell(p); for (let i = -1; i <= 1; i++) for (let j = -1; j <= 1; j++) for (let k = -1; k <= 1; k++) { const l = map.get((c[0] + i) + ',' + (c[1] + j) + ',' + (c[2] + k)); if (l) for (const v of l) if (Math.abs(V[v][0] - p[0]) + Math.abs(V[v][1] - p[1]) + Math.abs(V[v][2] - p[2]) < 4e-5) return v; } const n = V.length; V.push(p); const kk = c.join(','); (map.get(kk) || map.set(kk, []).get(kk)).push(n); return n; };
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


/* ---------- a rock: lobes (rounded-off prisms) grown together, from a recipe ----------
   recipe: { W, D, H, lean:[x,y], r: radius scale, seed, lobes:[{x0,x1,y0,y1,h, sx, sy, flat}], kinks:n }
   x runs along the left face (y = 0), y into the picture; a lobe's top is h (at its far corner) with slope sx along x. Lobes that touch share a joint plane,
   and the V between them is the two lobes' chamfers. */
function partsOf(rc) {
  return rc.lobes.map((q) => {
    const sx = q.sx || 0, sy = q.sy || 0, ht = q.h - sx * q.x1 - sy * q.y1;   /* h is the height at the lobe's far corner (x1, y1) */
    return { x0: q.x0, x1: q.x1, y0: q.y0, y1: q.y1, lean: rc.lean || [0, 0], bat: [0, 0, 0, 0], sx, sy, h0: ht, clips: q.clips || [],
      joints: [q.x0 > 1e-9 ? 0 : -1, q.x1 < rc.W - 1e-9 ? 1 : -1, q.y0 > 1e-9 ? 2 : -1, q.y1 < rc.D - 1e-9 ? 3 : -1].filter((k) => k >= 0) };
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
function randomKinks(r, rc, parts, ctx, n) {
  for (let i = 0; i < n; i++) {
    const pi = Math.floor(r() * parts.length), B = buildPart(parts[pi], ctx), face = pick(r, { L: .45, R: .35, T: .2 });
    const pl = kinkPlane(rc, B, face, U(r, -.4, .4), face === 'T' ? U(r, .5, 1.3) : U(r, 1.2, 3.2), face === 'T' ? U(r, 0, 90) : pick(r, { 0: .5, 45: .25, 135: .25 }) | 0, U(r, .012, .035) * ctx.m / .5);
    if (pl && -pl.n[2] < .8192) parts[pi].clips.push(pl);
  }
}
function makeRock(seed, opts) {
  opts = opts || {}; const t0 = (typeof performance !== 'undefined' ? performance : Date).now();
  const rc = opts.recipe || ROCK.random(seed, opts), stage = opts.stage == null ? 3 : opts.stage;
  let res = null;
  for (let attempt = 0; attempt < (opts.tries || 12); attempt++) {
    const ctx = { seed: (rc.seed || seed) + ':' + attempt, m: Math.min(rc.W, rc.D), center: [rc.W / 2, rc.D / 2, rc.H / 2], rmin: .05, rjoint: .018, jf: rc.jf || .25, bottomBevel: opts.bottomBevel !== false, noChamfer: stage === 0, chamfer: rc.chamfer || { top: [.14, .22], vert: [.12, .2], bottom: [.1, .16] } };
    const parts = partsOf(rc); const r = mulberry(hash('k' + (rc.seed || seed)));
    if (stage >= 2) randomKinks(r, rc, parts, ctx, rc.kinks == null ? 3 : rc.kinks);
    const built = parts.map((p) => buildPart(p, ctx)), polys = built.length > 1 ? unite(built) : built[0].faces.map((f) => ({ p: f.p, n: f.n })), mesh = meshOf(polys), chk = check(mesh);
    res = { rc, parts, built, polys, mesh, check: chk, attempt: attempt + 1, kind: rc.kind || '' };
    if (chk.open === 0 && chk.many === 0 && chk.overhangDeg <= 55.05) break;
  }
  const bb = aabb(res.mesh.V), cx = (bb.lo[0] + bb.hi[0]) / 2, cy = (bb.lo[1] + bb.hi[1]) / 2;
  res.V = res.mesh.V.map((p) => [p[0] - cx, p[1] - cy, p[2]]); res.T = res.mesh.T;
  res.dimsOut = [bb.hi[0] - bb.lo[0], bb.hi[1] - bb.lo[1], bb.hi[2]];
  res.ms = (typeof performance !== 'undefined' ? performance : Date).now() - t0;
  return res;
}
root.ROCK = { makeRock, check, mulberry, RECIPES: {}, random: null };
})(typeof window !== 'undefined' ? window : globalThis);
