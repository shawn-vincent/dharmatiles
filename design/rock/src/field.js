/* field.js — owns M7's sampled path: the one field the mesher and the Lines view read.
     field(p) = min over nearby standing pieces i of  marks_i( rounded_i(p) )
     rounded_i = the piece's own planes rounded by the wear radius r at p (erode, then dilate); the seat plane is hard, so wear never rounds the seat.
     Exact distance inside a face, a conservative bound outside: |field| never exceeds the distance to the surface, which is all the mesher
     needs (the zero set and a gradient). Wear is therefore one r, read once per point. The union is `min`; a piece whose smallest possible value cannot beat the best so far is
   skipped (marks only remove, except grain, whose `slack` bounds how much it can add). */
var Stone = Stone || {};
(function (S) {
  const { smax, sstep } = S.vec, { noise } = S.rand, abs = Math.abs, max = Math.max, min = Math.min, fl = Math.floor;
  const BLEND = 1.66;            // chips blend with a smooth max of blend k = 1.66 r, which rounds a right-angle edge at radius r, so a chip's edges round like everything else
  const MIN_WEAR = 0.04;         // wear never drops below this share of its strength, so no edge is left sharp by a noise trough
  const BUCKET_MIN = 2;          // mm: smallest cell of the lookup grid
  const TURN_AWAY = 0.05;        // a plane faces away from another when their normals differ by more than this in cosine: only those can be the far side
  const EDGE_PAD = 1;            // mm: a piece is looked up this far beyond its own box, so the samples that straddle its surface see it

  /* wear radius at a point: strength, varied in patches (uneven) and raised at corners and the top (exposure). Exactly the strength when both are 0. */
  function wearRadius(cfg, ctx) {
    const w = cfg.strength, irr = cfg.uneven, expo = cfg.exposure, patch = ctx.patch, hx = ctx.size[0] / 2, hy = ctx.size[1] / 2, hz = ctx.size[2] / 2, s1 = ctx.R.salt('wear', 'patch1'), s2 = ctx.R.salt('wear', 'patch2');
    const at = (x, y, z) => {
      if (irr === 0 && expo === 0) return w;
      let k = 1;
      if (irr > 0) k += irr * 1.4 * (2 * (0.6 * noise(x / patch + 3.1, y / patch, z / patch, s1) + 0.4 * noise(x / (patch * 0.45) + 7.7, y / (patch * 0.45), z / (patch * 0.45), s2)) - 1);
      if (expo > 0) { const top = sstep(0.1, 1, (z - hz) / hz), e = sstep(hx * 0.45, hx, abs(x)) + sstep(hy * 0.45, hy, abs(y)) + sstep(hz * 0.45, hz, abs(z - hz)); k += expo * (1.2 * top + 0.7 * max(0, e - 1)); }
      return w * max(MIN_WEAR, k);
    };
    return { at, max: w * (1 + irr * 1.4 + expo * 1.9) };
  }

  /* standing: the pieces that stand; wear: {at, max} or null; marks: Mark[] */
  function makeField(standing, wear, marks) {
    const reach = marks.reduce((a, m) => max(a, m.reach), 0), slack = marks.reduce((a, m) => a + m.slack, 0), margin = slack + EDGE_PAD;   // rounding, chips and pockets only remove, so a piece's surface never moves out by more than the grain
    const items = standing.map((p) => {
      const pl = [], surf = [], hd = [], seat = p.convex.faces.some((f) => f.role === 'seat');
      for (const f of p.convex.faces) if (f.role !== 'seat') { pl.push(f.n[0], f.n[1], f.n[2], f.d); surf.push(f.role === 'surface' ? 1 : 0); hd.push(p.convex.held(f) ? (f.n[2] < -0.5 ? 2 : 1) : 0); }
      return { p, pl: new Float64Array(pl), surf, hd, n: pl.length / 4, seat, anyHd: hd.some(Boolean) };
    });
    // lookup grid: a piece is listed in every cell its box, grown by what can act beyond its planes, overlaps
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    const boxes = items.map((it) => { const l = [Infinity, Infinity, Infinity], h = [-Infinity, -Infinity, -Infinity]; for (const v of it.p.convex.vertices()) for (let a = 0; a < 3; a++) { l[a] = min(l[a], v[a]); h[a] = max(h[a], v[a]); } for (let a = 0; a < 3; a++) { lo[a] = min(lo[a], l[a]); hi[a] = max(hi[a], h[a]); } return [l, h]; });
    const BK = max(BUCKET_MIN, 2 * margin), org = lo.map((v) => v - margin), dim = hi.map((v, a) => Math.ceil((v + margin - org[a]) / BK) + 1), cells = new Array(dim[0] * dim[1] * dim[2]);
    boxes.forEach(([l, h], idx) => {
      Object.assign(items[idx], { x0: l[0] - margin, y0: l[1] - margin, z0: l[2] - margin, x1: h[0] + margin, y1: h[1] + margin, z1: h[2] + margin });
      const a0 = [0, 1, 2].map((a) => fl((l[a] - margin - org[a]) / BK)), a1 = [0, 1, 2].map((a) => fl((h[a] + margin - org[a]) / BK));
      for (let k = a0[2]; k <= a1[2]; k++) for (let j = a0[1]; j <= a1[1]; j++) for (let i = a0[0]; i <= a1[0]; i++) { const c = (k * dim[1] + j) * dim[0] + i; (cells[c] || (cells[c] = [])).push(idx); }
    });
    const EMPTY = [];
    // The piece's rounded value, or Infinity as soon as it cannot be below `cut` (every plane offset v is a lower bound of the value).
    // Rounding by r is erode-then-dilate: pull every plane in by r (w = v + r); inside the pulled-in piece the value is the largest v, outside it is
    // the distance to it, sqrt(sum of the positive w squared), less r. Exact for a box's face, edge and corner; a bound for sharper or blunter corners.
    // A piece thinner than r is rounded to a smaller blob, never to dust.
    // The core: rounding must open a groove where a joint meets the surface, and must not open a cavity inside the stone where joints meet. So the piece is
    // also filled by its core, the piece with only its outer faces pulled in by r (joints and the seat left where they are): adjacent cores meet exactly on
    // their joint, so no void can form beyond r of the surface. The seat is hard (part of the core's joints): the walls round among themselves and the floor stays flat. A held joint is hard in the same way (the plane clips the rounded piece).
    const rounded = (it, x, y, z, r, cut) => {
      const pl = it.pl, surf = it.surf, hd = it.hd; let mS = -Infinity, mJ = it.seat ? -z : -Infinity, s2 = 0, mH = -Infinity;
      if (mJ >= cut) return Infinity;
      for (let i = 0, q = 0; i < it.n; i++, q += 4) {
        const v = pl[q] * x + pl[q + 1] * y + pl[q + 2] * z - pl[q + 3];
        if (v >= cut) return Infinity;
        if (surf[i]) { if (v > mS) mS = v; } else if (v > mJ) mJ = v;
        if (hd[i] && v > mH) mH = v;
        const w = v + r; if (w > 0 && hd[i] !== 2) s2 += w * w;   // a held face that faces down (a bed) is not rounded: a round at its edge is an overhang
      }
      const m = mS > mJ ? mS : mJ; if (!(r > 0) || s2 === 0) return m;
      const wall = Math.sqrt(s2) - r, core = mS + r > mJ ? mS + r : mJ;
      let res = wall < core ? wall : core;
      if (mH > res) res = mH;                    // a held joint is as hard as the seat: wear opens a groove only along its edge with the surface
      return it.seat && -z > res ? -z : res;
    };
    // How far the point is from the piece's outer faces and from its joints (and the seat): [mS, mJ], the largest plane offsets of each kind (negative inside), and
    // `behind`: from the point straight in, against the nearest outer face's normal, the distance to the far side of the piece (a joint, the seat or another outer face).
    // Marks that live on the surface (grain, pockets) weigh themselves by which is nearer, so a joint is never opened, and pockets keep to a share of `behind`.
    // The array is reused: valid during the call.
    const dom = [0, 0, 0, 0], vs = new Float64Array(max(1, ...items.map((it) => it.n)));
    const dominant = (it, x, y, z) => { const pl = it.pl, surf = it.surf; let mS = -Infinity, mJ = it.seat ? -z : -Infinity, qa = 0;
      for (let i = 0, q = 0; i < it.n; i++, q += 4) { const v = pl[q] * x + pl[q + 1] * y + pl[q + 2] * z - pl[q + 3]; vs[i] = v; if (surf[i]) { if (v > mS) { mS = v; qa = q; } } else if (v > mJ) mJ = v; }
      const na = pl[qa], nb = pl[qa + 1], nc = pl[qa + 2]; let behind = Infinity;
      for (let i = 0, q = 0; i < it.n; i++, q += 4) { if (q === qa) continue; const t = pl[q] * na + pl[q + 1] * nb + pl[q + 2] * nc; if (t < -TURN_AWAY) { const s = vs[i] / t; if (s < behind) behind = s; } }
      if (it.seat && nc > TURN_AWAY) { const s = z / nc; if (s < behind) behind = s; }
      dom[0] = mS; dom[1] = mJ; dom[2] = behind; dom[3] = nc; return dom; };
    const needN = marks.some((mk) => mk.needsDepths);
    const list = (x, y, z) => { const i = fl((x - org[0]) / BK), j = fl((y - org[1]) / BK), k = fl((z - org[2]) / BK); return i < 0 || j < 0 || k < 0 || i >= dim[0] || j >= dim[1] || k >= dim[2] ? EMPTY : cells[(k * dim[1] + j) * dim[0] + i] || EMPTY; };
    /* with marks; when out is given, the id of the piece that decides the value is put in out[0] */
    const f = function (x, y, z, out) {
      const l = list(x, y, z), r = wear ? wear.at(x, y, z) : 0, k = BLEND * r; let best = Infinity, who = -1;
      for (let a = 0; a < l.length; a++) {
        const it = items[l[a]]; if (x < it.x0 || x > it.x1 || y < it.y0 || y > it.y1 || z < it.z0 || z > it.z1) continue;
        let m = rounded(it, x, y, z, r, best + slack); if (m === Infinity) continue;
        const d = needN ? dominant(it, x, y, z) : null;
        for (const mk of marks) if (abs(m) < mk.reach) m = mk.apply(it.p, x, y, z, m, k, d);
        if (m < best) { best = m; who = it.p.id; }
      }
      if (out) out[0] = who;
      return best === Infinity ? margin + BK : best;
    };
    /* the same without marks: what the mesher uses to tell which blocks are far from the surface */
    f.coarse = (x, y, z) => {
      const l = list(x, y, z), r = wear ? wear.at(x, y, z) : 0; let b = Infinity;
      for (let a = 0; a < l.length; a++) { const it = items[l[a]]; if (x < it.x0 || x > it.x1 || y < it.y0 || y > it.y1 || z < it.z0 || z > it.z1) continue; const m = rounded(it, x, y, z, r, b); if (m < b) b = m; }
      return b === Infinity ? margin + BK : b;
    };
    f.bounds = { lo, hi }; f.margin = margin; f.reach = reach + slack;
    return f;
  }
  S.field = { makeField, wearRadius, BLEND };
})(Stone);
