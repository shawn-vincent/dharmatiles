/* layout.js — owns stage 2a, the layout of Chunks. Takes the region (a Convex), the chunks' settings and the bedding
   (dip, facing); hands on Pieces that tile the region exactly. Strata are parallel planes whose spacing is a random walk
   (monotone by construction, no inverse); each stratum is cut across by the bisectors of a jittered set of points
   (a Voronoi cell each). Joints are labelled with a key both neighbours share, so contact is a lookup, never a match.
   A piece: { id, key, convex, stratum }. */
var Stone = Stone || {};
(function (S) {
  const { dot, cross, sub, add, mul, norm, perp, rotate } = S.vec, { MIN_PIECE } = S.convex, PI = Math.PI;
  const HALF_OFFSET = 0.4;      // a "halved" piece gets a twin point this far (in cross spacings) from its own: one more cut through the middle

  function frame(bedding, R) {
    const dip = bedding.dip * PI / 180, az = bedding.facing * PI / 180, n0 = [Math.sin(dip) * Math.cos(az), Math.sin(dip) * Math.sin(az), Math.cos(dip)];
    const t1 = perp(n0), t2 = cross(n0, t1), roll = R.unit('chunks', 'roll') * 2 * PI;
    const e1 = add(mul(t1, Math.cos(roll)), mul(t2, Math.sin(roll))), e2 = cross(n0, e1);
    return { n: n0, e1, e2 };
  }
  /* boundary i of the strata: b(0) is a random phase, each step out is a gap of strata*(1 + uneven*(2u-1)) */
  function boundaries(cfg, R) {
    const memo = new Map(), phase = (R.unit('chunks', 'phase') - 0.5) * cfg.strata;
    const gap = (i) => cfg.strata * (1 + cfg.uneven * (2 * R.unit('chunks', 'gap', i) - 1));    // the gap below boundary i
    const b = (i) => { if (i === 0) return phase; let v = memo.get(i); if (v === undefined) { v = i > 0 ? b(i - 1) + gap(i) : b(i + 1) - gap(i + 1); memo.set(i, v); } return v; };
    return b;
  }
  /* tag: names the convex of a cell that has several (the wall's stone cut along a sloped row plane), so keys of one never meet keys of another */
  function layout(region, cfg, bedding, R, tag = '') {
    region = region.withGuard({ seatFloor: 0 });       // the seat rule belongs to the region; a crack may split the seat as it likes
    const fr = frame(bedding, R), W = (p) => dot(fr.n, p);
    let wmin = Infinity, wmax = -Infinity; for (const p of region.vertices()) { const w = W(p); wmin = Math.min(wmin, w); wmax = Math.max(wmax, w); }
    const b = boundaries(cfg, R); let iLo = 0, iHi = 0; while (b(iLo) > wmin) iLo--; while (b(iHi) < wmax) iHi++;
    // the cracks laid: every boundary between iLo and iHi whose draw is under the strength; the two ends lie outside the region and cut nothing
    const laid = [iLo]; for (let i = iLo + 1; i < iHi; i++) if (R.laid(cfg.strength, 'chunks', 'strata-laid', i)) laid.push(i); laid.push(iHi);
    // where a cross-crack point starts: it is shared with the next stratum while the carry draw says so, back to the first stratum at most
    const origin = (q, cell) => { while (q > iLo && R.unit('chunks', 'carry', q, cell[0], cell[1]) < cfg.carry) q--; return q; };
    const pieces = [];
    for (let s = 0; s + 1 < laid.length; s++) {
      const lo = laid[s], hi = laid[s + 1];
      let sc = region.cut(fr.n, b(hi), 'joint', tag + 's' + hi); if (sc) sc = sc.cut(mul(fr.n, -1), -b(lo), 'joint', tag + 's' + lo);
      if (!sc || sc.volume() < MIN_PIECE) continue;
      const uv = (p) => [dot(fr.e1, p), dot(fr.e2, p)];
      let u0 = Infinity, u1 = -Infinity, v0 = Infinity, v1 = -Infinity; for (const p of sc.vertices()) { const q = uv(p); u0 = Math.min(u0, q[0]); u1 = Math.max(u1, q[0]); v0 = Math.min(v0, q[1]); v1 = Math.max(v1, q[1]); }
      const X = cfg.across, pts = [];
      for (let i = Math.floor(u0 / X) - 1; i <= Math.ceil(u1 / X) + 1; i++) for (let j = Math.floor(v0 / X) - 1; j <= Math.ceil(v1 / X) + 1; j++) {
        const o = origin(lo, [i, j]);
        if (!R.laid(cfg.strength, 'chunks', 'point-laid', o, i, j)) continue;
        const x = (i + 0.5 + (R.unit('chunks', 'jitter', o, i, j, 0) - 0.5) * cfg.jitter) * X, y = (j + 0.5 + (R.unit('chunks', 'jitter', o, i, j, 1) - 0.5) * cfg.jitter) * X;
        pts.push({ x, y, id: o + ':' + i + ':' + j });
        if (R.laid(cfg.halved, 'chunks', 'halved', o, i, j)) { const a = R.unit('chunks', 'halved-at', o, i, j) * 2 * PI; pts.push({ x: x + X * HALF_OFFSET * Math.cos(a), y: y + X * HALF_OFFSET * Math.sin(a), id: o + ':' + i + ':' + j + 'h' }); }
      }
      if (!pts.length) pts.push({ x: (u0 + u1) / 2, y: (v0 + v1) / 2, id: 'centre' });
      for (const p of pts) {
        let cell = sc; const order = pts.filter((q) => q !== p).sort((a, q) => Math.hypot(a.x - p.x, a.y - p.y) - Math.hypot(q.x - p.x, q.y - p.y));
        for (const q of order) {
          const dx = q.x - p.x, dy = q.y - p.y, L = Math.hypot(dx, dy); let r = 0;
          for (const v of cell.vertices()) { const w = uv(v); r = Math.max(r, Math.hypot(w[0] - p.x, w[1] - p.y)); }
          if (L > 2 * r + 1e-6) break;                                          // no farther point can touch this cell
          const n = add(mul(fr.e1, dx / L), mul(fr.e2, dy / L)), mid = add(mul(fr.e1, (p.x + q.x) / 2), mul(fr.e2, (p.y + q.y) / 2));
          const key = tag + 'b' + lo + ':' + (p.id < q.id ? p.id + '|' + q.id : q.id + '|' + p.id);
          const next = cell.cut(n, dot(n, mid), 'joint', key);
          if (!next) { cell = null; break; }
          cell = next;
        }
        if (!cell || cell.volume() < MIN_PIECE) continue;       // a cell the stratum does not reach, or under any real size
        pieces.push({ id: pieces.length, key: S.rand.fnv(tag + lo + '/' + p.id), convex: cell, stratum: lo });
      }
    }
    const tiled = pieces.reduce((a, p) => a + p.convex.volume(), 0) / region.volume();   // measured: the cells fill the region
    return { pieces, frame: fr, strata: laid.length - 1, tiled };
  }
  S.layout = { layout, frame };
})(Stone);
