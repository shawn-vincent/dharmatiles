/* wall.js: Wall O built from the recipe of "Rock at Every Scale", section 7, and nothing else.
   Units mm at the 35 mm square; x right, y toward the back, z up. Every piece is a convex solid made of half-spaces,
   so the whole is closed by construction. build(seed, stage, over) -> {parts, ground, info}.
   stage: 1 mass, 2 blocks, 3 skin, 4 edges, 5 chips, 6 cracks, 7 foot + figure. */
(function (G) {
  'use strict';
  const E = G.Eng, rng = E.rng, P = E.P, norm = E.norm, cross = E.cross, dot = E.dot, add = E.add, sub = E.sub, mul = E.mul;
  const PI = Math.PI, cos = Math.cos, sin = Math.sin, rad = (d) => d * PI / 180;
  /* the recipe's numbers, in one place so the notes can name them. `over` replaces any of them. */
  const DEF = {
    H: 39, L: 35,
    courseH: [8, 9.8],
    blockW: [9, 15], carry: .35, minStagger: 3.2,
    push: [0, 6], recess: [6, 8], above: [2, 3], tierSetback: 3.0,
    faceLean: [.4, 2.2],              /* block faces, deg */
    topLean: 3,                        /* tops lean at most 8; this is the draw's top */
    joint: .5, chamferMax: .3,
    wholeShare: .6, skinSetback: [.2, .8], skinTilt: [.2, 1.2], skinDepth: 1.2,
    softShare: .7, soft: [.7, 2.8], crisp: [0, .5], topFront: [1.6, 3.4], vertShare: .8,
    chipShare: .3, chipDepth: [.9, 2.8], chipMax: 2, chipTilt: 25,
    crackShare: .5, crackGap: .45, crackCuts: [2, 4], frontCutShare: .72, minPiece: 3, crackSetbackK: 1, crackTiltK: 1, topDrop: .45,
    capH: [3, 4.5], facetOff: .1, facetTilt: 2.5, crackDepth: [.3, .7], crackHalf: .85, crackStep: .12, edgeFade: .5, step: .3, rubble: 12, rubbleSize: [3, 6], X0: 8, relief: true, pebbles: 5, scatter: 46
  };
  /* the recipe exactly as written, first version: columns, H 38-43, one skin split, no relief, a strip of rubble. Set with ?literal=1 */
  const LIT = { literal: true, X0: 3, courseH: [11.5, 14.5], capH: [3.5, 6.5], tierSetback: 1.0, chamferMax: 9, wholeShare: .6, crackShare: .5, crackCuts: [1, 2], frontCutShare: 1, skinSetback: [.2, .8], skinTilt: [.2, 1.2], crackSetbackK: 1, crackTiltK: 1, topDrop: 0, relief: false, pebbles: 0, scatter: 0, blockW: [12, 22] };
  function ri(r, a, b) { return a + (b - a) * r(); }
  /* intersection of three planes (n.p = d) */
  function solve3(p1, p2, p3) {
    const A = [p1.n, p2.n, p3.n], b = [p1.d, p2.d, p3.d];
    const c23 = cross(A[1], A[2]), det = dot(A[0], c23); if (Math.abs(det) < 1e-9) return null;
    const c31 = cross(A[2], A[0]), c12 = cross(A[0], A[1]);
    return [(b[0] * c23[0] + b[1] * c31[0] + b[2] * c12[0]) / det, (b[0] * c23[1] + b[1] * c31[1] + b[2] * c12[1]) / det, (b[0] * c23[2] + b[1] * c31[2] + b[2] * c12[2]) / det];
  }
  function edgePt(p1, p2, axis, at) { const e = [0, 0, 0]; e[axis] = 1; return solve3(p1, p2, { n: e, d: at }); }
  function chamfer(pa, pb, axis, at, c, tag) { const Ept = edgePt(pa, pb, axis, at); if (!Ept) return null; const n = norm(add(pa.n, pb.n)); return P(n, dot(n, sub(Ept, mul(pb.n, c))), tag || 'chamfer'); }

  /* one block, returned as planes. o: x0 x1 y_f z0 z1, tilt etc. */
  function blockPlanes(b, r, par, st) {
    const cx = (b.x0 + b.x1) / 2, cz = (b.z0 + b.z1) / 2, cy = (b.y_f + b.y_b) / 2;
    const lean = (n0, p, a) => E.leaned(n0, p, a, r);
    const fa = b.fLean == null ? ri(r, par.faceLean[0], par.faceLean[1]) : b.fLean;
    const front = lean([0, -1, 0], [cx, b.y_f, cz], fa), left = lean([-1, 0, 0], [b.x0, cy, cz], ri(r, .3, 1.5)), right = lean([1, 0, 0], [b.x1, cy, cz], ri(r, .3, 1.5));
    const top = lean([0, 0, 1], [cx, cy, b.z1], b.tLean == null ? ri(r, 0, par.topLean) : b.tLean), bot = P([0, 0, -1], -b.z0, 'bot'), back = P([0, 1, 0], b.y_b, 'back');
    front.tag = 'front'; left.tag = 'left'; right.tag = 'right'; top.tag = 'top';
    return { front, left, right, top, bot, back, cx, cz, cy, base: [front, left, right, top, bot, back] };
  }
  function edgeCuts(bp, b, r, par) {
    const hh = b.z1 - b.z0, cap = (c) => Math.min(c, hh * par.chamferMax);
    const cuts = [], soft = () => r() < par.softShare ? ri(r, par.soft[0], par.soft[1]) : ri(r, par.crisp[0], par.crisp[1]);
    const c0 = Math.max(.05, ri(r, par.topFront[0], par.topFront[1]));
    cuts.push(chamfer(bp.front, bp.top, 0, bp.cx, cap(r() < .85 ? c0 : soft()), 'tf'));
    cuts.push(chamfer(bp.left, bp.top, 1, bp.cy, cap(soft()), 'tl'));
    cuts.push(chamfer(bp.right, bp.top, 1, bp.cy, cap(soft()), 'tr'));
    if (r() < par.vertShare) cuts.push(chamfer(bp.front, bp.left, 2, bp.cz, ri(r, 1, 2), 'vl'));
    if (r() < par.vertShare) cuts.push(chamfer(bp.front, bp.right, 2, bp.cz, ri(r, 1, 2), 'vr'));
    return cuts.filter(Boolean);
  }
  function chipCuts(bp, b, r, par) {
    const cuts = [], n = Math.floor(r() * (par.chipMax + 1));
    const corners = [[bp.left, 'l'], [bp.right, 'r']];
    for (let i = 0; i < n; i++) {
      const [sp] = corners[(r() * 2) | 0], pt = solve3(bp.front, sp, bp.top); if (!pt) continue;
      const nn = norm(add(add(bp.front.n, sp.n), add(bp.top.n, [0, 0, .25 + .5 * r()])));
      cuts.push(P(nn, dot(nn, pt) - ri(r, par.chipDepth[0], par.chipDepth[1]), 'chip'));
    }
    return cuts;
  }
  const finish = (planes, tag) => ({ faces: E.solidFromPlanes(planes), tone: 1, tag });

  function build(seed, stage, over) {
    const par = Object.assign({}, DEF, over || {}); if (par.literal) Object.assign(par, LIT); const X0 = par.X0, r = rng(seed * 977 + 13), parts = [], info = { blocks: 0, pieces: 0 };
    const P0 = (x0, x1, y_f, z0, z1) => ({ x0, x1, y_f, y_b: 35, z0, z1 });
    /* ---- 1 Mass: three boxes stepping up and back ---- */
    const ch = [ri(r, ...par.courseH), ri(r, ...par.courseH), ri(r, ...par.courseH)];
    const zc = [0, ch[0], ch[0] + ch[1], ch[0] + ch[1] + ch[2]];
    const ext = par.literal ? [[3, 35], [3, 35], [13 + ri(r, -1.5, 1.5), 35]] : [[X0, 35], [X0, 35], [X0, 25]];      /* x extent of course 0,1,2 */
    const capSpec = par.literal ? [[3, 12.5 + ri(r, -1, 1), 2, null], [15 + ri(r, -1, 1), 23.5 + ri(r, -1, 1), 3, null], [27 + ri(r, -1, 1), 35, 3, null]] : [[X0 + .5, X0 + 9.5 + ri(r, -.5, 1), 3, 21], [X0 + 11 + ri(r, -.5, 1), 24.5, 3, 21], [26.5 + ri(r, -1, 1), 35, 2, 19]];
    const yBase = par.literal ? 15.5 : 14.5;
    if (stage === 1) {
      /* three boxes stepping up and back: front low, middle, back high */
      par.literal ? [[3, 14, 14, 35, zc[2]], [14, 25, 16.5, 35, zc[2] + 3], [25, 35, 14.5, 35, zc[3]]] : [[X0, 35, 13.5, 24, zc[1]], [X0, 35, 17.5, 29, zc[2]], [X0, 25, 22, 35, zc[3]]].forEach(m => {
        parts.push(finish([P([-1, 0, 0], -m[0]), P([1, 0, 0], m[1]), P([0, -1, 0], -m[2]), P([0, 1, 0], m[3]), P([0, 0, -1], .5), P([0, 0, 1], m[4])], 'mass'));
      });
      return { parts, info, ground: true };
    }
    /* core: the rock behind, one body, below every cap */
    const coreY = 26.5;
    (par.literal ? [[3, 14, zc[2] - 3], [14, 25, zc[2] - 1], [25, 35, zc[3] - 3]] : [[X0, 25, zc[3] - 3], [25, 35, zc[2] - 3]]).forEach(m => parts.push(finish([P([-1, 0, 0], -m[0]), P([1, 0, 0], m[1]), P([0, -1, 0], -coreY), P([0, 1, 0], 35), P([0, 0, -1], .5), P([0, 0, 1], m[2])], 'core')));
    /* ---- 2 Blocks: courses, joints, depth push, the recess and the overhang ---- */
    const joints = [];            /* joints per course */
    const blocks = [];
    for (let c = 0; c < 3; c++) {
      const [xa, xb] = ext[c], js = [];
      let x = xa, prev = joints[c - 1] || [];
      for (;;) {
        let w = ri(r, par.blockW[0], par.blockW[1]);
        let jx = x + w;
        if (xb - jx < par.blockW[0] * .7) break;
        /* a third of the joints repeat the one below, the rest are staggered by at least minStagger */
        const near = prev.filter(p => Math.abs(p - jx) < 6)[0];
        if (near != null) { if (r() < par.carry) jx = near; else { const s = Math.abs(near - jx) < par.minStagger ? (r() < .5 ? -1 : 1) * par.minStagger : 0; jx += s; } }
        if (jx - x < par.blockW[0] * .85) jx = x + par.blockW[0] * .85;
        js.push(jx); x = jx;
      }
      joints.push(js);
      const xs = [xa].concat(js, [xb]);
      for (let i = 0; i + 1 < xs.length; i++) {
        const z0 = c === 0 ? -.5 : zc[c] + par.joint / 2, z1 = zc[c + 1] - (c === 2 ? 0 : par.joint / 2);
        blocks.push({ c, i, x0: xs[i] + (i ? par.joint / 2 : 0), x1: xs[i + 1] - (i + 2 < xs.length ? par.joint / 2 : 0), z0, z1, push: ri(r, par.push[0], par.push[1]) });
      }
    }
    /* one recess (6-8) and the block above it set forward (2-3) */
    const cand = blocks.filter(b => b.c === 0 && b.x0 > 8 && b.x1 < 33);
    const rb = cand[(r() * cand.length) | 0];
    if (rb) { rb.push = ri(r, ...par.recess); rb.recess = true; const up = blocks.filter(b => b.c === 1 && b.x0 < (rb.x0 + rb.x1) / 2 && b.x1 > (rb.x0 + rb.x1) / 2)[0]; if (up) { up.push = -ri(r, ...par.above) + par.tierSetback; up.forward = true; } }
    blocks.forEach(b => {
      b.y_f = Math.min(coreY - 2.4, yBase + par.tierSetback * b.c + (b.c === 2 && !par.literal ? Math.min(b.push, 3) : b.push) + (b.x0 < X0 + 11 ? -1.6 : 0) + (b.x0 > 24 && !par.literal ? -1.5 : 0));
      b.y_b = 35; b.fLean = null;
    });
    /* caps on the top course */
    capSpec.forEach(cs => {
      const base = zc[cs[2]], h = ri(r, ...par.capH), x0 = cs[0], x1 = cs[1];
      blocks.push({ c: 3, i: 0, x0, x1, z0: base + par.joint / 2, z1: base + par.joint / 2 + h, y_f: cs[3] == null ? yBase + 4 + ri(r, 0, 4) : cs[3] + ri(r, 0, 2.5), y_b: 35, push: 0 });
    });
    info.blocks = blocks.length;
    blocks.forEach(b => {
      const bp = blockPlanes(b, r, par, stage);
      let cuts = [];
      if (stage >= 4) cuts = cuts.concat(edgeCuts(bp, b, r, par));
      if (stage >= 5) cuts = cuts.concat(chipCuts(bp, b, r, par));
      /* skin and cracks: cells cut by half-spaces; a cut lives inside its parent cell; children are leaf pieces */
      const nSkin = (stage >= 3 && r() >= par.wholeShare) ? 1 : 0;
      const nCrack = (stage >= 6 && r() < par.crackShare) ? par.crackCuts[0] + Math.floor(r() * (par.crackCuts[1] - par.crackCuts[0] + 1)) : 0;
      if (!nSkin && !nCrack) { parts.push(finish(bp.base.concat(cuts), 'block')); info.pieces++; return; }
      const bk = Object.assign({}, b, { y_f: b.y_f + par.skinDepth, z1: b.z1 - par.skinDepth * .6 });
      const bpk = blockPlanes(bk, r, par, stage);
      parts.push(finish(bpk.base.concat(cuts), 'backing'));
      let cells = [{ cut: [], x0: b.x0, x1: b.x1, z0: b.z0, z1: b.z1, y0: b.y_f, y1: b.y_f + 8 }];
      const splitCell = (ci, gap) => {
        const c = cells[ci], front = r() < par.frontCutShare;
        let n, pt, kids;
        const cx = ri(r, c.x0 + (c.x1 - c.x0) * .3, c.x1 - (c.x1 - c.x0) * .3);
        if (front) {
          const th = rad(r() < .2 ? (r() < .5 ? -1 : 1) * ri(r, 65, 85) : ri(r, -45, 45)); n = [cos(th), 0, sin(th)];
          pt = [cx, 0, ri(r, c.z0 + (c.z1 - c.z0) * .3, c.z1 - (c.z1 - c.z0) * .3)];
        } else {
          const ph = rad(ri(r, -40, 40) + (r() < .35 ? 90 : 0)); n = [cos(ph), sin(ph), 0];
          pt = [cx, ri(r, c.y0 + (c.y1 - c.y0) * .3, c.y1 - (c.y1 - c.y0) * .3), 0];
        }
        const d = dot(n, pt), wide = Math.abs(n[0]) > .6 || (!front && Math.abs(n[0]) > .6);
        const minw = par.minPiece;
        const A = Object.assign({}, c, { cut: c.cut.concat([P(n, d - gap / 2, 'cut')]) }), B = Object.assign({}, c, { cut: c.cut.concat([P([-n[0], -n[1], -n[2]], -d - gap / 2, 'cut')]) });
        if (front) { if (wide) { A.x1 = cx; B.x0 = cx; } else { A.z1 = pt[2]; B.z0 = pt[2]; } } else { if (wide) { A.x1 = cx; B.x0 = cx; } else { A.y1 = pt[1]; B.y0 = pt[1]; } }
        if (A.x1 - A.x0 < minw || B.x1 - B.x0 < minw || A.z1 - A.z0 < minw || B.z1 - B.z0 < minw || A.y1 - A.y0 < minw * .6 || B.y1 - B.y0 < minw * .6) return false;
        cells.splice(ci, 1, A, B); return true;
      };
      if (nSkin) splitCell(0, 0);
      for (let k = 0, t = 0; k < nCrack && t < 30; t++) if (splitCell((r() * cells.length) | 0, par.crackGap)) k++;
      cells.forEach(cell => {
        const sb = ri(r, par.skinSetback[0], par.skinSetback[1]) * (cell.cut.length > 1 ? par.crackSetbackK : 1) * (r() < .5 ? 1 : .3);
        const lb = Object.assign({}, b, { y_f: b.y_f + sb, z1: b.z1 - (cell.cut.length > 1 ? ri(r, 0, par.topDrop) : 0), tLean: ri(r, 0, par.topLean) });
        const bq = blockPlanes(lb, r, par, stage), tl = ri(r, par.skinTilt[0], par.skinTilt[1]) * (cell.cut.length > 1 ? par.crackTiltK : 1);
        bq.front = E.leaned([0, -1, 0], [bq.cx, lb.y_f, bq.cz], tl, r); bq.front.tag = 'front';
        const planes = [bq.front, bq.left, bq.right, bq.top, bq.bot, P([0, 1, 0], b.y_f + par.skinDepth + 1.5, 'mid')].concat(cell.cut, cuts);
        parts.push(finish(planes, 'skin')); info.pieces++;
      });
    });
    if (stage >= 6 && par.relief) relief(parts, par, seed, info);
    if (stage >= 7) addFoot(parts, r, par, zc);
    const fig = par.literal ? [9.5, 9, 8.6] : [5.8, 7.5, 5.8];
    if (stage >= 7 && par.scatter) scatterStones(parts, r, par, fig);
    return { parts, info, ground: true, figure: stage >= 7, fig };
  }

  /* small stones over the whole plate: round the figure's base, in front, behind it; not on the base, not under the wall */
  function scatterStones(parts, r, par, fig) {
    let n = 0, tries = 0; const placed = [];
    while (n < par.scatter && tries++ < 900) {
      const big = r() < .22, s = big ? ri(r, 2.5, 5) : ri(r, 1.2, 3), x = ri(r, 1.5, 33.5), y = ri(r, 1.5, 33.5);
      if (Math.hypot(x - fig[0], y - fig[1]) < fig[2] + s * .5 + .3) continue;
      if (x > par.X0 - 1 && y > 12 - s * .5 && x < 35) continue;
      if (x > 27 && y < 15 && y > 6) continue;
      if (placed.some(p => Math.hypot(p[0] - x, p[1] - y) < p[2] + s * .55)) continue;
      placed.push([x, y, s * .5]); n++;
      if (big) parts.push(rubbleBlock(x, y, s, s * ri(r, .4, .8), ri(r, 0, PI), r, par));
      else { const h = s * ri(r, .35, .55); parts.push({ faces: E.solidFromPlanes(ballPlanes([x, y, h * .3], s * .5, 12).concat([P([0, 0, -1], .4)])), tone: 1, tag: 'pebble' }); }
    }
  }
  /* ---- relief: cracks, facets, pits and pebbles as a displaced skin on the faces that show (front, top, left return) ----
     Each face polygon is subdivided finely; the height along its normal is the sum of: a facet plane per cell of an
     irregular stretched cell grid, V grooves along a share of the cell borders (long, branching where borders meet, running to the block edge),
     small steps across a groove, patches of pits. It is 0 on the polygon's own edge, so the solid stays closed. */
  function pairHash(a, b, s) { const lo = Math.min(a, b), hi = Math.max(a, b); return E.hash2(lo, hi, s); }
  function cellField(cs, seed, rot, str) {
    const cr = cos(rot), sr = sin(rot);
    return function (u, v) {
      const x = (u * cr - v * sr) / cs, y = (u * sr + v * cr) / (cs * str), gi = Math.floor(x), gj = Math.floor(y);
      let d1 = 1e9, d2 = 1e9, i1 = 0, i2 = 0, p1 = null;
      for (let a = -1; a <= 1; a++) for (let b = -1; b <= 1; b++) {
        const i = gi + a, j = gj + b, px = i + .5 + (E.hash2(i, j, seed) - .5) * .9, py = j + .5 + (E.hash2(i, j, seed + 7) - .5) * .9, d = Math.hypot(x - px, (y - py));
        const id = (i * 7919 + j * 104729) | 0;
        if (d < d1) { d2 = d1; i2 = i1; d1 = d; i1 = id; p1 = [px, py, i, j]; } else if (d < d2) { d2 = d; i2 = id; }
      }
      return { e: (d2 - d1) * .5 * cs, i1, i2, dx: x - p1[0], dy: y - p1[1], ci: p1[2], cj: p1[3] };
    };
  }
  function distToPoly(p, pts) { let d = 1e9; for (let i = 0; i < pts.length; i++) { const a = pts[i], b = pts[(i + 1) % pts.length], e = sub(b, a), t = Math.max(0, Math.min(1, dot(sub(p, a), e) / (dot(e, e) || 1))), q = add(a, mul(e, t)); d = Math.min(d, Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2])); } return d; }
  const sstep = E.sstep;
  function displaceFace(f, seed, par, kind) {
    const n = f.n, pts = f.pts, helper = Math.abs(n[2]) < .9 ? [0, 0, 1] : [1, 0, 0], u = norm(cross(n, helper)), v = cross(n, u), c0 = pts[0];
    const rr = rng(seed), rot = kind === 'top' ? ri(rr, 0, PI) : ri(rr, -.45, .45), str = kind === 'top' ? 1 : 1.5;
    const field = cellField(kind === 'top' ? 5.5 : 7, seed, rot, str), crackShare = kind === 'top' ? .45 : .6;
    const pits = []; if (rr() < (kind === 'top' ? .8 : .35)) { let ax = ri(rr, -3, 3), ay = ri(rr, -3, 3); const k = 3 + (rr() * 3 | 0); for (let i = 0; i < k; i++) pits.push([ax + ri(rr, -2.2, 2.2), ay + ri(rr, -2.2, 2.2), ri(rr, .45, .95), ri(rr, .3, .6)]); }
    const cu = pts.reduce((a, p) => a + dot(sub(p, c0), u), 0) / pts.length, cv = pts.reduce((a, p) => a + dot(sub(p, c0), v), 0) / pts.length;
    const D = (p) => {
      const rel = sub(p, c0), pu = dot(rel, u), pv = dot(rel, v), q = field(pu, pv), edge = distToPoly(p, pts);
      const hh = pairHash(q.i1, q.i2, seed);
      let h = 0;
      /* facet: a plane per cell, slope up to 2.5 degrees, offset up to 0.25 */
      const g1 = E.hash2(q.ci, q.cj, seed + 3) - .5, g2 = E.hash2(q.ci, q.cj, seed + 5) - .5, off = (E.hash2(q.ci, q.cj, seed + 9) - .5) * par.facetOff;
      h += off + (g1 * q.dx + g2 * q.dy) * 7 * Math.tan(rad(par.facetTilt)) * .5;
      /* V groove along a share of the cell borders, with a small step across it */
      if (hh < crackShare) { const dep = par.crackDepth[0] + (par.crackDepth[1] - par.crackDepth[0]) * E.hash2(q.i1 + q.i2, q.i1 ^ q.i2, seed + 11), w = par.crackHalf, t = Math.max(0, 1 - q.e / w); h -= dep * t * t * (3 - 2 * t) * 1.0; h += (q.i1 > q.i2 ? 1 : -1) * .5 * par.crackStep * sstep(0, w * 2.2, q.e) * (1 - sstep(w * 2.2, w * 5, q.e)); }
      for (const pt of pits) { const d = Math.hypot(pu - pt[0] - cu, pv - pt[1] - cv); if (d < pt[2]) { const t = 1 - d * d / (pt[2] * pt[2]); h -= pt[3] * t * t; } }
      return h * sstep(0, par.edgeFade, edge);
    };
    const out = [], step = par.step;
    for (let k = 1; k < pts.length - 1; k++) {
      const a = pts[0], b = pts[k], c = pts[k + 1], L = Math.max(Math.hypot.apply(null, sub(b, a)), Math.hypot.apply(null, sub(c, b)), Math.hypot.apply(null, sub(a, c))), m = Math.max(1, Math.min(48, Math.ceil(L / step)));
      const Pt = (i, j) => { const p = [a[0] + (b[0] - a[0]) * i / m + (c[0] - a[0]) * j / m, a[1] + (b[1] - a[1]) * i / m + (c[1] - a[1]) * j / m, a[2] + (b[2] - a[2]) * i / m + (c[2] - a[2]) * j / m], h = D(p); return { p: add(p, mul(n, h)), p0: p, dk: Math.max(0, Math.min(.6, -h * .5)) }; };
      const grid = []; for (let i = 0; i <= m; i++) { grid[i] = []; for (let j = 0; j <= m - i; j++) grid[i][j] = Pt(i, j); }
      const emit = (V0, V1, V2) => { let nn = cross(sub(V1.p, V0.p), sub(V2.p, V0.p)); const l = Math.hypot(nn[0], nn[1], nn[2]); if (l < 1e-9) return; nn = [nn[0] / l, nn[1] / l, nn[2] / l]; const vs = dot(nn, n) < 0 ? [V0, V2, V1] : [V0, V1, V2]; if (dot(nn, n) < 0) nn = [-nn[0], -nn[1], -nn[2]]; out.push({ n: nn, n0: n, vs: vs, pts: vs.map(x => x.p), tag: 'disp' }); };
      for (let i = 0; i < m; i++) for (let j = 0; j < m - i; j++) { emit(grid[i][j], grid[i + 1][j], grid[i][j + 1]); if (j < m - i - 1) emit(grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]); }
    }
    return out;
  }
  function relief(parts, par, seed, info) {
    let fid = 0, tris = 0, peb = [];
    parts.forEach(pt => {
      if (pt.tag !== 'block' && pt.tag !== 'skin') return;
      const nf = [];
      pt.faces.forEach(f => {
        const kind = f.tag === 'front' ? 'front' : f.tag === 'top' ? 'top' : f.tag === 'left' ? 'left' : null;
        if (!kind || f.area < 2.5) { nf.push(f); return; }
        const fs = seed * 131 + (fid++) * 17 + 5;
        const d = displaceFace(f, fs, par, kind); tris += d.length; d.forEach(x => nf.push(x));
        if (kind === 'top' && par.pebbles) { const rr = rng(fs + 99), k = Math.floor(rr() * (par.pebbles + 1)), c = f.pts.reduce((a, p) => add(a, p), [0, 0, 0]).map(x => x / f.pts.length);
          for (let i = 0; i < k; i++) { const q = add(c, add(mul(norm(cross(f.n, [1, 0, 0])), ri(rr, -3, 3)), mul([1, 0, 0], ri(rr, -3, 3)))); if (distToPoly(q, f.pts) < 1.6) continue; const rad0 = ri(rr, .45, .95); peb.push({ faces: E.solidFromPlanes(ballPlanes([q[0], q[1], q[2] + rad0 * .25], rad0, 10).concat([P([0, 0, -1], -(q[2] - .6))])), tone: 1, tag: 'pebble' }); } }
      });
      pt.faces = nf;
    });
    peb.forEach(p => parts.push(p)); info.tris = tris;
  }

  /* foot stair, rubble strip */
  function rubbleBlock(cx, cy, s, h, th, r, par) {
    const pl = [], N = [[cos(th), sin(th)], [-sin(th), cos(th)]];
    const sx = s * ri(r, .8, 1.2) / 2, sy = s * ri(r, .8, 1.2) / 2;
    [[0, 1, sx], [0, -1, sx], [1, 1, sy], [1, -1, sy]].forEach(q => { const n = [N[q[0]][0] * q[1], N[q[0]][1] * q[1], 0]; pl.push(E.leaned(n, [cx + n[0] * q[2], cy + n[1] * q[2], h / 2], ri(r, 1, 6), r)); });
    pl.push(P([0, 0, -1], .6)); pl.push(E.leaned([0, 0, 1], [cx, cy, h], ri(r, 0, 5), r));
    const faces0 = pl.slice(0, 4), top = pl[5];
    for (let i = 0; i < 4; i++) pl.push(chamfer(faces0[i], top, ((i < 2) ? 1 : 0), (i < 2) ? cy : cx, ri(r, .6, 1.4), 'rc'));
    return finish(pl.filter(Boolean), 'rubble');
  }
  function addFoot(parts, r, par, zc) {
    /* foot stair at the right */
    parts.push(rubbleBlock(30.5, 9.8, 6.5, 5, 0.1, r, par)); parts.push(rubbleBlock(33, 12.2, 4.6, 7.5, -.1, r, par));
    const placed = [[30.5, 9.8, 5], [33, 12.4, 4]]; let n = 0, tries = 0;
    while (n < par.rubble && tries++ < 300) {
      const s = ri(r, ...par.rubbleSize), x = ri(r, 12, 34), y = ri(r, 2.5, 13);
      if (placed.some(p => Math.hypot(p[0] - x, p[1] - y) < p[2] + s * .75)) continue;
      placed.push([x, y, s * .6]); parts.push(rubbleBlock(x, y, s, s * ri(r, .45, .8), ri(r, 0, PI), r, par)); n++;
    }
  }
  /* the 28 mm figure stand-in: a round base, a cylinder, a ball; solids from half-spaces */
  function prismPlanes(cx, cy, rr, z0, z1, n) { const pl = [P([0, 0, -1], -z0), P([0, 0, 1], z1)]; for (let i = 0; i < n; i++) { const a = 2 * PI * (i + .5) / n, nn = [cos(a), sin(a), 0]; pl.push(P(nn, nn[0] * cx + nn[1] * cy + rr * cos(PI / n))); } return pl; }
  function ballPlanes(c, rr, n) { const pl = []; for (let i = 0; i < n; i++) { const z = 1 - 2 * (i + .5) / n, a = i * 2.399963, q = Math.sqrt(1 - z * z), nn = [q * cos(a), q * sin(a), z]; pl.push(P(nn, dot(nn, c) + rr)); } return pl; }
  function figure(parts, fig) {
    const x = fig[0], y = fig[1], R = fig[2];
    [[prismPlanes(x, y, R, -.5, 1.6, 28)], [prismPlanes(x, y, 2.6, 1.0, 20.5, 14)], [ballPlanes([x, y, 23.5], 3, 44)]].forEach(q => parts.push({ faces: E.solidFromPlanes(q[0]), tone: 1.08, tag: 'figure' }));
  }
  G.WallO = { build, figure, DEF, LIT };
})(window);
