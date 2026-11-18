/* cell.js — owns the one input a stone is made in: a cell. A cell is the region the stone fills, in the stone's own frame (+z up, the seat or the lowest face at z = 0),
   as one or more convexes given by labelled planes { n, d, role, nb, held }. Roles are the three the rock already has (surface, seat, joint); `held` marks a joint
   that something outside the stone holds (a neighbour stone), so it is never outer and never cut. A cell with several convexes is one stone cut apart by seams: the seam is a joint
   that is not held (a crack inside the stone), keyed by whoever made the cell, in a namespace of its own.
   A cell is plain data (spec), so it can be keyed, sent to a worker and built there; `size` is its bounding box, and the rock reads size from it, never from width, breadth and height.
   Without a cell the rock fills the box of its own `width`, `breadth` and `height`, as before. */
/* AN AREA AND A TOP (the way to write a cell): a region is one or more convex parts, each an `area` (a polygon in plan: vertices [x, y], counter-clockwise, each edge `surface` (free) or `joint` (held by a neighbour)), a `bed` under it and a `top` over it,
   each a plane { n, d } (flat or sloped), the bed a `seat` when the stone is on the ground and held when it rests on a stone, the top held when a stone stands on it; and any other `planes` it is already cut by (surface).
   The envelope is the area extruded from the bed to the top. Parts that share an edge are one stone cut apart: that edge is a crack inside it (a joint that is not held), found by the cell, never written. The strata's lean is the bed's.
   A box is the area form: a rectangle, a seat, a top at the height. `area` makes the planes the rock reads (one plane one face, sides ordered by their normals so a plane's index never depends on the polygon's first vertex). */
var Stone = Stone || {};
(function (S) {
  const canon = (spec) => JSON.stringify(spec, (k, v) => (typeof v === 'number' ? +v.toFixed(5) : v));
  /* spec: { pieces: [{ planes: [...] }] }. Returns { spec, key, convexes(), size, centre }. */
  function make(spec, opts) {
    let built = null, size = null;
    const convexes = () => {
      if (!built) built = spec.pieces.map((p) => { const c = S.convex.Convex.fromPlanes(p.planes); return c.withGuard({ seatFloor: S.region.SEAT_SHARE * c.heldArea() }); });
      return built;
    };
    return { spec, bedding: spec.bedding || null, manifold: !(opts && opts.manifold === false), key: S.rand.fnv(canon(spec)) + ':' + spec.pieces.length, convexes, get size() { if (!size) { const lo = [1e9, 1e9, 1e9], hi = [-1e9, -1e9, -1e9]; for (const c of convexes()) { const b = S.region.boundsOf(c); for (let a = 0; a < 3; a++) { lo[a] = Math.min(lo[a], b.centre[a] - b.size[a] / 2); hi[a] = Math.max(hi[a], b.centre[a] + b.size[a] / 2); } } size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]; } return size; } };
  }
  const PL = (p, role, held) => { const o = { n: p.n, d: p.d, role, nb: role === 'joint' ? p.nb || null : null, held: !!held }; if (held && p.bear) o.bear = p.bear; return o; };
  const sideOrder = (a, b) => Math.abs(b.n[0]) - Math.abs(a.n[0]) || a.n[0] - b.n[0] || a.n[1] - b.n[1];
  const edgeKey = (n, d) => { const s = (Math.abs(n[0]) > 1e-6 ? n[0] : n[1]) < 0 ? -1 : 1; return [n[0] * s, n[1] * s, d * s].map((v) => Math.round(v * 1e4)).join(','); };
  /* the bearing patch of a held top or bed: the plan polygon [x, y] the neighbour rests on, lifted onto the plane (z from n.p = d) */
  const onPlane = (pl, xy) => (xy && xy.length >= 3 && Math.abs(pl.n[2]) > 1e-6 ? xy.map((q) => [q[0], q[1], (pl.d - pl.n[0] * q[0] - pl.n[1] * q[1]) / pl.n[2]]) : undefined);
  const orient = (P) => { let a = 0; for (let i = 0; i < P.length; i++) { const p = P[i], q = P[(i + 1) % P.length]; a += p[0] * q[1] - q[0] * p[1]; } return a < 0 ? P.slice().reverse() : P; };
  function area(parts, name, opts) {
    const sides = parts.map((p, pi) => { const P = orient(p.area), roles = p.edges || [], rev = P !== p.area, out = [];
      for (let i = 0; i < P.length; i++) { const a = P[i], b = P[(i + 1) % P.length], ex = b[0] - a[0], ey = b[1] - a[1], l = Math.hypot(ex, ey); if (l < 1e-9) continue; const n = [ey / l, -ex / l, 0], role = roles[rev ? P.length - 2 - i < 0 ? P.length - 1 : P.length - 2 - i : i] || 'surface';
        out.push({ n, d: n[0] * a[0] + n[1] * a[1], role, nb: 'w:' + (name || '') + 'p' + pi + 'e' + i }); }
      return out; });
    // an edge two parts share (reversed, on one line, overlapping) is a crack inside the stone: a joint that is not held, with a key both carry
    sides.forEach((A, i) => A.forEach((e) => { for (let j = 0; j < sides.length; j++) if (j !== i) for (const f of sides[j]) if (Math.abs(e.n[0] + f.n[0]) < 1e-6 && Math.abs(e.n[1] + f.n[1]) < 1e-6 && Math.abs(e.d + f.d) < 1e-5) { e.seam = f.seam = edgeKey(e.n, e.d); } }));
    const spec = { pieces: parts.map((p, pi) => {
      const planes = sides[pi].slice().sort(sideOrder).map((e) => e.seam ? PL({ n: e.n, d: e.d, nb: (p.seamTag || 'seam') + ':' + e.seam }, 'joint', false) : e.role === 'joint' ? PL(e, 'joint', true) : PL(e, 'surface', false));
      if (p.top) planes.push(p.top.held ? PL({ n: p.top.n, d: p.top.d + (p.top.over || 0), nb: 'w:top', bear: onPlane(p.top, p.top.bear) }, 'joint', true) : PL(p.top, 'surface', false));
      if (p.bed) planes.push(p.bed.seat ? PL(p.bed, 'seat', false) : PL({ n: p.bed.n, d: p.bed.d + (p.bed.over || 0), nb: 'w:bed', bear: onPlane(p.bed, p.bed.bear) }, 'joint', true));
      for (const q of p.planes || []) planes.push(PL(q, 'surface', false));
      return { planes }; }) };
    const b0 = parts.map((p) => p.bed).find(Boolean); if (b0 && !b0.seat) { const up = [-b0.n[0], -b0.n[1], -b0.n[2]]; let facing = Math.atan2(up[1], up[0]) * 180 / Math.PI; if (facing < 0) facing += 360; spec.bedding = { dip: Math.acos(Math.max(-1, Math.min(1, up[2]))) * 180 / Math.PI, facing }; }
    return make(spec, opts); }
  /* the box of the rock alone, as the Weathered Stone page uses it: width, breadth, height = a rectangle, a seat, a top at the height */
  const box = (size) => area([{ area: [[-size[0] / 2, -size[1] / 2], [size[0] / 2, -size[1] / 2], [size[0] / 2, size[1] / 2], [-size[0] / 2, size[1] / 2]], bed: { n: [0, 0, -1], d: 0, seat: true }, top: { n: [0, 0, 1], d: size[2] } }], 'box', { manifold: false });
  S.cell = { make, area, box };
})(Stone);
