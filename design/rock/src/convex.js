/* convex.js — owns the one solid the stone is made of: a convex set of labelled half-spaces n.p <= d.
   Every cut that shapes the stone (outline, stratum, crack, set-back, tilt, corner) is `cut`; a face is a plane that
   survives, and it carries its role and its neighbour from the cut that made it, so nothing is ever recovered by matching.
     role: 'surface' (faces the outside), 'seat' (the plane z = 0), 'joint' (a face shared with a neighbour, whose key is `nb`).
   HELD: a face is held when something outside the stone holds it: the seat, or a joint the cell was given with `held` (the wall's neighbours). A crack the rock cuts itself is a joint that is not held. One definition, `held`; settle, the field, the marks' weights and the cut guard read it.
   Cuts only remove, so nothing ever leaves the convex it started as.
   FACING RULE: a `surface` cut whose normal faces down more steeply than 55 degrees from vertical has its normal lifted to 55. */
var Stone = Stone || {};
(function (S) {
  const { dot, cross, sub, norm, mul, add } = S.vec;
  const OVERHANG_NZ = Math.sin(55 * Math.PI / 180);   // the printable limit: a face may lean out 55 degrees from vertical (normal z >= -sin 55)
  const MIN_PIECE = 0.02;                              // mm^3: under this a convex is no part at all
  const BIG = 1e3;                                     // half-size of the slab a new face starts as, then is clipped to its true outline
  const MIN_AREA = 1e-6;                               // mm^2: a face under this is a point or a line that touched the cut

  const vecArea = (pts) => { let x = 0, y = 0, z = 0; for (let i = 0; i < pts.length; i++) { const a = pts[i], b = pts[(i + 1) % pts.length]; x += a[1] * b[2] - a[2] * b[1]; y += a[2] * b[0] - a[0] * b[2]; z += a[0] * b[1] - a[1] * b[0]; } return [x / 2, y / 2, z / 2]; };
  const area = (pts) => { const a = vecArea(pts); return Math.hypot(a[0], a[1], a[2]); };

  // keep the part of a labelled polygon with n.p <= d. lab[i] names the neighbour across the edge pts[i] -> pts[i+1]; the new edge is labelled `id`.
  function clipPoly(pts, lab, n, d, id) {
    const op = [], ol = [], m = pts.length;
    const push = (p, l) => { const q = op[op.length - 1]; if (q && q[0] === p[0] && q[1] === p[1] && q[2] === p[2]) { ol[ol.length - 1] = l; return; } op.push(p); ol.push(l); };
    for (let i = 0; i < m; i++) {
      const a = pts[i], b = pts[(i + 1) % m], da = dot(n, a) - d, db = dot(n, b) - d;
      if (da <= 0) {
        push(a, lab[i]);
        if (db > 0) { const t = da / (da - db); push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], id); }
      } else if (db <= 0) {
        const t = da / (da - db); push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], lab[i]);
      }
    }
    return [op, ol];
  }
  function slab(n, d) {
    const u = S.vec.perp(n), v = cross(n, u), c = mul(n, d), p = (a, b) => add(c, add(mul(u, a * BIG), mul(v, b * BIG)));
    return [p(-1, -1), p(1, -1), p(1, 1), p(-1, 1)];
  }

  /* the facing rule: a unit normal, lifted if it faces down past 55 degrees; null if it points straight down (no azimuth to keep) */
  function facing(n) {
    n = norm(n);
    if (-n[2] <= OVERHANG_NZ) return n;
    const h = Math.hypot(n[0], n[1]); if (h < 1e-6) return null;
    const s = Math.sqrt(1 - OVERHANG_NZ * OVERHANG_NZ) / h; return [n[0] * s, n[1] * s, -OVERHANG_NZ];
  }

  class Convex {
    constructor(faces, nid, guard) { this.faces = faces; this.nid = nid; this.guard = guard; this._v = null; }
    /* the box [lo,hi]: the floor is the seat, the rest is surface. guard.seatFloor: a cut may not leave less held area than this (mm^2). A box is the six planes of fromPlanes. */
    static box(lo, hi, guard) {
      const specs = [[[-1, 0, 0], -lo[0], 'surface'], [[1, 0, 0], hi[0], 'surface'], [[0, -1, 0], -lo[1], 'surface'], [[0, 1, 0], hi[1], 'surface'], [[0, 0, 1], hi[2], 'surface'], [[0, 0, -1], -lo[2], 'seat']];
      return Convex.fromPlanes(specs.map(([n, d, role]) => ({ n, d, role })), guard);
    }
    /* a convex from labelled planes {n, d, role, nb, held}, one plane one face, in the order given (a face's id is its plane's index): each face starts as an enormous slab clipped by the other planes, the one clip every cut uses. A plane no other plane leaves a face to is dropped. */
    static fromPlanes(planes, guard) {
      let nid = 0;
      const ps = planes.map((p) => Object.assign({}, p, { id: nid++ }));
      const faces = [];
      for (const f of ps) {
        let pts = slab(f.n, f.d), lab = pts.map(() => -1);
        for (const g of ps) if (g !== f) { [pts, lab] = clipPoly(pts, lab, g.n, g.d, g.id); if (pts.length < 3) break; }
        if (pts.length >= 3 && area(pts) > MIN_AREA) faces.push({ n: f.n, d: f.d, role: f.role, nb: f.nb == null ? null : f.nb, held: !!f.held, bear: f.bear, id: f.id, pts, lab });
      }
      if (faces.length < 4) throw new Error('a region needs at least four faces');
      return new Convex(faces, nid, guard || { seatFloor: 0 });
    }
    held(f) { return f.role === 'seat' || (f.role === 'joint' && !!f.held); }
    heldArea() { let a = 0; for (const f of this.faces) if (this.held(f)) a += area(f.pts); return a; }
    /* the faces with these ids become outer: role surface, no neighbour (the solid across them is gone) */
    outer(ids) { const set = new Set(ids); return new Convex(this.faces.map((f) => (set.has(f.id) ? Object.assign({}, f, { role: 'surface', nb: null, held: false }) : f)), this.nid, this.guard); }
    withGuard(guard) { return new Convex(this.faces, this.nid, guard); }
    vertices() { if (!this._v) { this._v = []; for (const f of this.faces) for (const p of f.pts) this._v.push(p); } return this._v; }
    support(n) { let m = -Infinity; for (const p of this.vertices()) { const v = dot(n, p); if (v > m) m = v; } return m; }
    volume() { let v = 0; for (const f of this.faces) v += area(f.pts) * f.d; return v / 3; }
    faceArea(f) { return area(f.pts); }
    centroid() { const v = this.vertices(), c = [0, 0, 0]; for (const p of v) { c[0] += p[0]; c[1] += p[1]; c[2] += p[2]; } return mul(c, 1 / v.length); }
    /* the edges: two faces meet along a segment; a, b are its ends and f1, f2 the faces (read off the labels, not matched) */
    edges() {
      const byId = new Map(this.faces.map((f) => [f.id, f])), out = [];
      for (const f of this.faces) for (let i = 0; i < f.pts.length; i++) { const g = byId.get(f.lab[i]); if (g && f.id < g.id) out.push({ a: f.pts[i], b: f.pts[(i + 1) % f.pts.length], f1: f, f2: g }); }
      return out;
    }
    /* cut away n.p > d. Returns this when nothing is cut, a new Convex, or null when the cut is refused: it would leave
       nothing, under MIN_PIECE of volume, or (guard) too little held area. */
    cut(n, d, role, nb, held) {
      let any = false; for (const p of this.vertices()) if (dot(n, p) > d + 1e-9) { any = true; break; }
      if (!any) return this;
      const id = this.nid, faces = [];
      for (const f of this.faces) { const [pts, lab] = clipPoly(f.pts, f.lab, n, d, id); if (pts.length >= 3 && area(pts) > MIN_AREA) faces.push({ n: f.n, d: f.d, role: f.role, nb: f.nb, held: f.held, bear: f.bear, id: f.id, pts, lab }); }
      let pts = slab(n, d), lab = pts.map(() => -1);
      for (const f of this.faces) [pts, lab] = clipPoly(pts, lab, f.n, f.d, f.id);
      if (pts.length >= 3 && area(pts) > MIN_AREA) faces.push({ n, d, role, nb: nb == null ? null : nb, held: !!held, id, pts, lab });
      if (faces.length < 4) return null;
      const c = new Convex(faces, id + 1, this.guard);
      if (c.volume() < MIN_PIECE) return null;
      if (this.guard.seatFloor > 0 && c.heldArea() < this.guard.seatFloor) return null;   // the guard: a cut may not leave less held area (the seat and the held joints) than this
      return c;
    }
    /* a surface cut `depth` below the highest point of `ref` (default this) in direction n, with the facing rule applied */
    shave(n, depth, ref) {
      n = facing(n); if (!n) return null;
      return this.cut(n, (ref || this).support(n) - depth, 'surface', null);
    }
  }
  /* the overlap area of two faces lying in one plane (convex polygons): the one contact measure */
  function overlapArea(fa, fb) {
    const n = fa.n, u = S.vec.perp(n), v = cross(n, u), P2 = (pts) => pts.map((p) => [dot(u, p), dot(v, p)]);
    let A = P2(fa.pts), B = P2(fb.pts);
    const orient = (q) => { let s = 0; for (let i = 0; i < q.length; i++) { const a = q[i], b = q[(i + 1) % q.length]; s += a[0] * b[1] - b[0] * a[1]; } return s; };
    if (orient(A) < 0) A.reverse();
    if (orient(B) < 0) B.reverse();
    let poly = B;
    for (let i = 0; i < A.length && poly.length >= 3; i++) {
      const p = A[i], q = A[(i + 1) % A.length], ex = q[0] - p[0], ey = q[1] - p[1], out = [];
      const side = (r) => ex * (r[1] - p[1]) - ey * (r[0] - p[0]);        // >= 0 keeps the left of the edge
      for (let j = 0; j < poly.length; j++) {
        const a = poly[j], b = poly[(j + 1) % poly.length], sa = side(a), sb = side(b);
        if (sa >= 0) { out.push(a); if (sb < 0) { const t = sa / (sa - sb); out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]); } }
        else if (sb >= 0) { const t = sa / (sa - sb); out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]); }
      }
      poly = out;
    }
    return poly.length < 3 ? 0 : Math.abs(orient(poly)) / 2;
  }
  S.convex = { Convex, overlapArea, area, facing, OVERHANG_NZ, MIN_PIECE };
})(Stone);
