/* marks.js — owns Chips, Pockets and Grain. A mark is a field edit anchored to one piece's feature (chips to its outer edges,
   pockets to its surface cells, grain to its surface), so none ever crosses a joint.
     Mark { id, reach, slack, apply(piece, x, y, z, m, k) -> m }
   reach: mm from the surface where the mark can act; the caller evaluates it only when |m| < reach.
   slack: the most the mark can lower m (add material); chips and pockets only remove, so 0; grain is signed, so its depth.
   m is the piece's rounded field at the point and k the wear blend there. Every constant below is a look, with its reason. */
var Stone = Stone || {};
(function (S) {
  const { dot, sub, add, mul, norm, cross, sstep, smax } = S.vec, { noise } = S.rand, PI = Math.PI, sqrt = Math.sqrt, abs = Math.abs, max = Math.max, min = Math.min, fl = Math.floor;
  // what a mark works out once for a piece, held by the mark itself (so a new setting is a new mark and starts clean)
  const perPiece = (make) => { const held = new Map(); return (piece) => { let v = held.get(piece.id); if (v === undefined) held.set(piece.id, v = make(piece)); return v; }; };

  /* ---- chips: runs of flat facets, one big among small, along each sharp outer edge ---- */
  const FAMILY = 4;               // one big chip and three small make a family; chips per cm = families * 4 / cm
  const MIN_EDGE = 1.2;           // mm: shorter edges are not chipped
  const GOLDEN = 0.6180339887;    // families are strided along an edge by the golden ratio, so any prefix is spread out
  function chips(cfg, ctx) {
    // a chip only removes material below the surface, down to its own depth: at most 3 x 0.38 x depth (the biggest chip, the deepest draw), and a leaning plane
    // is a little deeper along the face normal, hence the 1.5
    const reach = 1.5 * 3 * 0.38 * cfg.depth + 0.3;
    const groupsOf = perPiece((piece) => {
      const h = ctx.R.keyed('chips', 'draw', piece.key), groups = [];
      piece.convex.edges().forEach((ed, ei) => {
        const heldDown = (f) => piece.convex.held(f) && f.n[2] < -0.5;
        if (ed.f1.role === 'seat' || ed.f2.role === 'seat' || (ed.f1.role !== 'surface' && ed.f2.role !== 'surface') || heldDown(ed.f1) || heldDown(ed.f2)) return;   // outer edges only, never the floor, a joint, or the edge of a held face that faces down (a chip there is an overhang)
        const va = ed.a, vb = ed.b, L = Math.hypot(vb[0] - va[0], vb[1] - va[1], vb[2] - va[2]); if (L < MIN_EDGE) return;
        const turn = Math.acos(Math.max(-1, Math.min(1, dot(ed.f1.n, ed.f2.n)))) * 180 / PI; if (turn < cfg.blunt) return;
        const sharp = turn >= 90 ? 1 : max(0.15, (turn - cfg.blunt) / max(1, 90 - cfg.blunt)), sn = max(0.25, Math.sin(turn * PI / 360));
        const ev = mul(sub(vb, va), 1 / L), list = []; let r = 0;
        const fams = Math.ceil(cfg.strength * L * sharp / (10 * FAMILY)), off = h(ei, 0, 0, 1);
        for (let fam = 0; fam < fams; fam++) {
          const mk = (tt, Rr, tag) => {
            const w1 = 0.5 + h(ei, fam, tag, 2), w2 = 0.5 + h(ei, fam, tag, 3);
            let n = norm(add(mul(ed.f1.n, w1), mul(ed.f2.n, w2))); n = norm([n[0] + (h(ei, fam, tag, 4) - 0.5) * 0.4, n[1] + (h(ei, fam, tag, 5) - 0.5) * 0.4, n[2] + (h(ei, fam, tag, 6) - 0.5) * 0.4]);
            n = S.convex.facing(n) || n;                                           // a chip face leans out no more than 55 degrees either
            const c = add(va, mul(ev, max(0.3, min(L - 0.3, tt * L)))), d = cfg.depth * Rr * (0.1 + 0.28 * h(ei, fam, tag, 7)) * sharp, Lc = 1.15 * d / sn + 0.25, La = cfg.length * Rr / 2;
            r = max(r, max(La, Lc) + 0.5); list.push({ c, n, e: ev, La, Lc, c0: sub(c, mul(n, d)) });
          };
          const tpos = (off + fam * GOLDEN) % 1;
          mk(tpos, 1 + 2 * Math.pow(h(ei, fam, 0, 8), 1.4), 0);
          for (let j = 1; j <= 3; j++) mk(max(0.02, min(0.98, tpos + (h(ei, fam, j, 9) < 0.5 ? -1 : 1) * (0.05 + 0.1 * h(ei, fam, j, 10)))), 0.5 + h(ei, fam, j, 11), j);
        }
        if (list.length) groups.push({ mid: add(va, mul(ev, L / 2)), r2: (L / 2 + r) * (L / 2 + r), list });
      });
      return groups;
    });
    return {
      id: 'chips', reach, slack: 0,
      apply(piece, x, y, z, m, k) {
        const groups = groupsOf(piece);
        for (const g of groups) {
          const mx = x - g.mid[0], my = y - g.mid[1], mz = z - g.mid[2]; if (mx * mx + my * my + mz * mz > g.r2) continue;
          for (const c of g.list) {
            const dx = x - c.c[0], dy = y - c.c[1], dz = z - c.c[2], al = dx * c.e[0] + dy * c.e[1] + dz * c.e[2], ac2 = dx * dx + dy * dy + dz * dz - al * al;
            if (al * al > c.La * c.La * 1.6 || ac2 > c.Lc * c.Lc * 1.6) continue;
            const sd = c.n[0] * (x - c.c0[0]) + c.n[1] * (y - c.c0[1]) + c.n[2] * (z - c.c0[2]), rr = (sqrt(al * al / (c.La * c.La) + ac2 / (c.Lc * c.Lc)) - 1) * c.Lc;
            m = smax(m, -max(-sd, rr), k);                                          // the removed region is a plane and an ellipsoid
          }
        }
        return m;
      }
    };
  }

  /* The weight of a surface mark at a point of a piece: 1 where the nearest boundary is an outer face, 0 where it is a joint or the seat, a smooth blend between.
     d = [mS, mJ] from the field. A mark that bit a joint would open a crack through the stone; this is continuous, so no step or ridge. */
  const JOINT_BLEND = 1.5;       // mm over which the weight goes from 0 to 1 (as the difference in distance to the nearest outer face and nearest joint)
  /* The most a surface mark may cut at a point: what the piece has behind it, kept to half (dS to the outer face + half the distance to the nearest joint or seat).
     Without it a bite or grain on a thin piece cuts through it and leaves a floating part. */
  const allowed = (d) => { const a = -d[0] + 0.5 * -d[1]; return a > 0 ? a : 0; };
  /* d[3] is the z of the nearest outer face's normal. A mark that roughens or dishes a face that leans down steeply makes it lean further, and an overhang is the one thing a print cannot do: such a face is left as it is,
     the weight fading from 1 at a normal z of -0.3 (about 17 degrees of overhang) to 0 at -0.6 (37), so every face the facing rule leaves (up to 55) is untouched past that. */
  const notDown = (z) => (z >= -0.3 ? 1 : z <= -0.6 ? 0 : (z + 0.6) / 0.3);
  const onSurface = (d, blend) => { const t = 0.5 + (d[0] - d[1]) / blend; return (t >= 1 ? 1 : t <= 0 ? 0 : t * t * (3 - 2 * t)) * notDown(d[3]); };

  /* ---- pockets: a Worley field of dishes, one cell for every seed on the outer surface ----
     Seeds are scattered over the outer faces of a piece, by area, about `size` apart. A point of space belongs to the cell of its nearest seed, found in 3D, so the cells
     run on across an edge or a corner exactly as grain's solid noise does: no crease, no face-by-face seam. The cut at a point is
         cut(p) = depth x scale x dish(w),   w = (F2 - F1) / (F2 + F1)
     with F1 and F2 the distances to the nearest and second nearest seed: w is 1 at a seed and 0 on the rim between two cells, so every cell is a dish that is deepest
     at its seed and flat on its rim (sharp rim, no gap, no flat patch). `cover` is the share of seeds that get a dish; a cell with none stays at the surface.
     NEVER THROUGH is a rule of the construction, not a cleanup: the cut is held to CAP of the stone behind the point, the thickness T from the outer face to the
     far side of the piece (a joint, the seat or its other outer face, `behind` in field.js). The cut is scaled by min(1, CAP T / depth), so it is smooth, never
     above CAP T, and a thin part gets shallow dishes, not holes. Pockets act only on outer faces (the same joint weight as grain) and at depth 0 are not in the list. */
  const POCKET_SEP = 0.5;        // seeds closer than this share of `size` are dropped (in a fixed order), so no tiny cell is left between two faces at an edge
  const POCKET_CELL = 0.7;       // the lookup grid's cell, in sizes: fine enough that a cell's list of candidate seeds is short
  const POCKET_JITTER = 0.7;     // share of its grid cell a seed wanders in: an irregular scatter, never a lattice
  const POCKET_DEPTHS = [0.55, 1];   // each dish's depth is this range of the setting (drawn per seed): the dishes are not all alike
  const POCKET_CAP = 0.4;        // the most of the stone behind it a dish may take: two faces of a slab each take up to this, so 20% of its thickness is always left
  const POCKET_SMALL = 0.8;      // a cell narrower than this share of `size` (seed to seed) is dished less, in proportion, so a small cell is not a steep pit
  /* The dish is a parabolic cap across the whole cell: dish(w) = w (2 - w), 0 on the rim, 1 at the seed, level (slope 0) at the seed and curving continuously to the rim, so it has no flat
     floor at any depth. Its sag is the depth; the angle at the rim follows from the sag and the cell (about 4 depth / cell width), so a shallow scoop is a gentle curve, and the rim, where
     the slope is not zero, is a sharp crease where two scoops meet. */
  const dish = (w) => w * (2 - w);
  const SAG = 0.3;               // a scoop's sag is limited to about this share of its cell's width (F1 + F2): a deeper setting only approaches it (sag x tanh(depth / sag)), so the angle at the rim stays under about 45 degrees and a rim that runs along a cube edge leaves a wedge the mesher can draw. Deep scoops did not read as rock anyway (Shawn, 18 Nov)
  function pockets(cfg, ctx) {
    const size = cfg.size, depth = cfg.strength, cell = POCKET_CELL * size;
    const key = (i, j, k) => ((i + 64) * 128 + (j + 64)) * 128 + (k + 64);
    const seedsOf = perPiece((piece) => {
      const h = ctx.R.keyed('pockets', 'seed', piece.key), grid = new Map(), all = []; let fi = 0;
      const near = (c) => { const i = fl(c[0] / cell), j = fl(c[1] / cell), k = fl(c[2] / cell); for (let a = -1; a <= 1; a++) for (let b = -1; b <= 1; b++) for (let e = -1; e <= 1; e++) for (const q of grid.get(key(i + a, j + b, k + e)) || []) if ((q.c[0] - c[0]) ** 2 + (q.c[1] - c[1]) ** 2 + (q.c[2] - c[2]) ** 2 < (POCKET_SEP * size) ** 2) return true; return false; };
      for (const f of piece.convex.faces) {
        if (f.role !== 'surface') continue; fi++;
        const n = f.n, u = S.vec.perp(n), v = cross(n, u), pts = f.pts.map((q) => [dot(u, q), dot(v, q)]), o = dot(n, f.pts[0]);
        let u0 = 1e9, u1 = -1e9, v0 = 1e9, v1 = -1e9; for (const q of pts) { u0 = min(u0, q[0]); u1 = max(u1, q[0]); v0 = min(v0, q[1]); v1 = max(v1, q[1]); }
        const inside = (a, b) => { let sg = 0; for (let i = 0; i < pts.length; i++) { const p = pts[i], q = pts[(i + 1) % pts.length], c = (q[0] - p[0]) * (b - p[1]) - (q[1] - p[1]) * (a - p[0]); if (c !== 0) { if (sg && (c > 0) !== (sg > 0)) return false; sg = c; } } return true; };
        for (let i = fl(u0 / size); i <= Math.ceil(u1 / size); i++) for (let j = fl(v0 / size); j <= Math.ceil(v1 / size); j++) {
          const a = (i + (1 - POCKET_JITTER) / 2 + POCKET_JITTER * h(fi, i, j, 1)) * size, b = (j + (1 - POCKET_JITTER) / 2 + POCKET_JITTER * h(fi, i, j, 2)) * size; if (!inside(a, b)) continue;
          const c = add(add(mul(u, a), mul(v, b)), mul(n, o)); if (near(c)) continue;
          const sd = { c, dish: h(fi, i, j, 0) < cfg.cover ? depth * (POCKET_DEPTHS[0] + (POCKET_DEPTHS[1] - POCKET_DEPTHS[0]) * h(fi, i, j, 3)) : 0 };   // cover: the share of seeds dished, in a fixed order
          const kk = key(fl(c[0] / cell), fl(c[1] / cell), fl(c[2] / cell)); let l = grid.get(kk); if (!l) grid.set(kk, l = []); l.push(sd); all.push(sd);
        }
      }
      // The two nearest seeds of any point of a lookup cell are among a short list worked out once for the cell (on first use): a seed is kept unless the cell lies
      // farther from it than the second nearest seed is from every point of the cell. So the search at a point reads one list, exactly.
      const n = all.length, vs = piece.convex.vertices(), reach = depth + 1, lo = [0, 1, 2].map((a) => Math.min(...vs.map((q) => q[a])) - reach), dim = [0, 1, 2].map((a) => Math.ceil((Math.max(...vs.map((q) => q[a])) + reach - lo[a]) / cell) + 1);
      const pos = new Float64Array(4 * n); all.forEach((q, i) => { pos.set([q.c[0], q.c[1], q.c[2], q.dish], 4 * i); });
      const first = new Int32Array(dim[0] * dim[1] * dim[2]).fill(-1), count = new Uint16Array(first.length), list = [];
      const listFor = (ci, i, j, k) => {
        const c0 = [lo[0] + i * cell, lo[1] + j * cell, lo[2] + k * cell];
        const far = [], near2 = []; for (let s = 0; s < n; s++) { let mn = 0, mx = 0; for (let a = 0; a < 3; a++) { const p = pos[4 * s + a], lo1 = c0[a] - p, hi1 = c0[a] + cell - p; mn += lo1 > 0 ? lo1 * lo1 : hi1 < 0 ? hi1 * hi1 : 0; mx += Math.max(lo1 * lo1, hi1 * hi1); } far.push(mx); near2.push(mn); }
        const reachD = far.slice().sort((x, y) => x - y)[Math.min(1, n - 1)]; first[ci] = list.length;
        for (let s = 0; s < n; s++) if (n < 2 || near2[s] <= reachD) { list.push(s); count[ci]++; }
      };
      return { n, lo, dim, pos, first, count, list, listFor };
    });
    return {
      id: 'pockets', reach: depth + 0.5, slack: 0, needsDepths: true,
      apply(piece, x, y, z, m, k, d) {
        const w = onSurface(d, JOINT_BLEND); if (w <= 0) return m;
        const { n, lo, dim, pos, first, count, list, listFor } = seedsOf(piece); if (!n) return m;
        const ix = fl((x - lo[0]) / cell), iy = fl((y - lo[1]) / cell), iz = fl((z - lo[2]) / cell); if (ix < 0 || iy < 0 || iz < 0 || ix >= dim[0] || iy >= dim[1] || iz >= dim[2]) return m;
        const ci = (iz * dim[1] + iy) * dim[0] + ix; if (first[ci] < 0) listFor(ci, ix, iy, iz);
        let f1 = Infinity, f2 = Infinity, s1 = -1;
        for (let q = first[ci], e = q + count[ci]; q < e; q++) { const o = 4 * list[q], dx = x - pos[o], dy = y - pos[o + 1], dz = z - pos[o + 2], r2 = dx * dx + dy * dy + dz * dz; if (r2 < f1) { f2 = f1; f1 = r2; s1 = o; } else if (r2 < f2) f2 = r2; }
        f1 = sqrt(f1); f2 = f2 === Infinity ? f1 + size : sqrt(f2);
        const dep = pos[s1 + 3]; if (!(dep > 0)) return m;
        const wc = (f2 - f1) / (f2 + f1), small = min(1, (f1 + f2) / (POCKET_SMALL * size));
        const sag = SAG * (f1 + f2), cut0 = sag * Math.tanh(dep * small / sag);   // the sag is softly limited by the cell: sag x tanh(depth / sag)
        let cut = cut0 * dish(wc);
        const T = max(0, -d[0]) + max(0, d[2]), capk = POCKET_CAP * T / depth;
        if (capk < 1) cut *= capk;
        return m + w * cut;
      }
    };
  }

  /* ---- grain: three octaves of solid noise, in the strata frame, added to the field at the point itself ----
     It is the same noise on both sides of a corner, so a cube goes uniformly fuzzy, edges and corners included. */
  const MASK_EDGE = 0.15;        // the patch mask ramps over this much noise, so patches have soft rims
  const GRAIN_REACH = 1.5;       // x depth: past this the grain cannot change the sign of the field
  function grain(cfg, ctx) {
    const amp = cfg.strength, fr = ctx.frame, patch = ctx.patch, R = ctx.R, sA = R.salt('grain', 'a'), sB = R.salt('grain', 'b'), sC = R.salt('grain', 'c'), sM1 = R.salt('grain', 'mask1'), sM2 = R.salt('grain', 'mask2');
    return {
      id: 'grain', reach: GRAIN_REACH * amp, slack: amp, needsDepths: true,
      apply(piece, x, y, z, m, k, d) {
        const w = onSurface(d, JOINT_BLEND); if (w <= 0) return m;
        const mk = (1.3 * cfg.cover - (0.6 * noise(x / patch + 1.7, y / patch, z / patch, sM1) + 0.4 * noise(x / (0.44 * patch), y / (0.44 * patch), z / (0.44 * patch) + 4.1, sM2))) / MASK_EDGE;
        if (mk <= 0) return m;
        const q = fr.n[0] * x + fr.n[1] * y + fr.n[2] * z, u = fr.e1[0] * x + fr.e1[1] * y + fr.e1[2] * z, t = fr.e2[0] * x + fr.e2[1] * y + fr.e2[2] * z;
        const a = noise(q * 0.9, u * 0.55, t * 1.4, sA) - 0.5, b = (1 - abs(2 * noise(q * 1.9 + 7, u * 1.2, t * 3, sB) - 1)) - 0.5, c = noise(q * 0.33 + 3, u * 0.2, t * 0.5, sC) - 0.5;
        const g = (0.6 * c + 0.9 * a + 0.5 * b) * amp * (mk > 1 ? 1 : mk) * w;
        return m + (g > 0 && g > allowed(d) ? allowed(d) : g);
      }
    };
  }
  // pockets first, then chips, then grain: chips bite pocket ridges, and grain roughens what is left
  const MAKERS = [['pockets', pockets], ['chips', chips], ['grain', grain]];
  const active = (P, ctx) => MAKERS.filter(([id]) => P[id] > 0).map(([id, make]) => make(S.settingsOf(P, id), ctx));
  S.marks = { active };
})(Stone);
