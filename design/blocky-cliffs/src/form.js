/* ---------- the formation (proposed; chapter 11 states it) ----------
   A pile. A back mass stands along the back edge: pillars, tiers of blocks, a stair of courses, a set of leaning slabs, dipping beds,
   or a gate. A front cluster stands against it at the cut end: a buttress and a toe. Stones may stand proud of the wall, be applied to
   it, or be tucked into a corner. Then stones are piled on: a summit cap, a block spanning a joint, a second storey.
   Every stone is first a plain box with a place (a "spec"); shapes are given last, when each stone knows which of its edges are exposed. */
const FORM = { seed: 1, cls: 'pillars',
  length: 1.45, depth: .62, height: 1.7,    /* fh: along the cliff; the back mass front to back; the highest top of the back mass (caps add to it) */
  nA: 3, rows: 2,          /* stones along the back mass; rows: 1 = no front cluster, 2 = buttress and toe, 3 = and a second block along the wall */
  front: .4,               /* the buttress's height as a share of the wall behind it */
  fall: .45,               /* how far the tops fall from the peak to the far end, as a share of the height */
  peak: .1,                /* where the peak is along the cliff (0 = at the cut end, .5 = mid length) */
  sizeVar: .55,            /* how unequal the widths are */
  stagger: .5,             /* joints of one tier against the next: 0 in line */
  stepMin: .13,            /* least step between neighbouring tops that are not on a plateau, fh */
  plateau: 0,              /* 1: a run of two or three neighbours shares one sloping top plane */
  cleft: 0,                /* 1: an open joint between the two tallest neighbours */
  caps: 0,                 /* stones piled on: the first is a summit cap, the rest span a joint or make a second storey */
  proud: 1, applied: 0, tuck: 0,   /* stones standing forward of the wall; a pillar applied over a joint; a small stone tucked in a corner */
  oversail: 0, over: null, /* 1: an upper block at the far end oversails a set-back lower wall; over: by how much, fh */
  tiers: 0,                /* 2-4: the back mass is tiers of blocks, each shorter and set back, joints staggered */
  lean: 0,                 /* degrees: the back mass is parallel slabs leaning this far toward the cut end */
  dip: 0,                  /* degrees: the back mass is beds dipping this far */
  taper: .95, bevel: .16, jit: .02, offset: .03, cutCorner: .2,
  gate: false, toe: true, rough: 1, minW: 1, shapes: 1,   /* shapes 0: every stone a plain bevelled box (for the experiments) */
  leanEach: 0, yawEach: 0, bevelEach: 0, gap: 0 };
const PRESET = {
  pillars: { nA: 3, rows: 2 },
  wall: { cls: 'pillars', nA: 3, rows: 2, fall: .25, plateau: 1, peak: .25 },
  blocks: { nA: 3, rows: 2, tiers: 3, depth: .72, fall: .4, sizeVar: .35, caps: 1 },
  courses: { nA: 2, rows: 4, depth: 1.1, length: 1.5, fall: .2, sizeVar: .3, toe: false, peak: 0, caps: 0 },
  leaners: { nA: 4, rows: 2, lean: 20, fall: .35, depth: .6 },
  ramp: { cls: 'leaners', nA: 1, rows: 2, dip: 27, fall: .1, depth: .7, caps: 1 },
  gates: { nA: 3, rows: 2, gate: true, fall: .3, depth: .72, peak: .15, height: 1.3, caps: 1 } };
function formationOnce(over = {}) {
  const q = Object.assign({}, FORM, PRESET[over.preset || over.cls] || {}, over), r = rng(q.seed * 104729 + 7), S = [];
  const td = Math.tan(q.dip * Math.PI / 180), tl = Math.tan(q.lean * Math.PI / 180), L = q.length, back = PLATE, D = q.depth, dB = back - D, courses = q.cls === 'courses';
  const add = (o) => { const sp = Object.assign({ z: 0, level: 0, on: 0, depth: 0, role: 'wall' }, o); S.push(sp); return sp; };
  /* a row cut into unequal widths, none thin, joints kept off the joints in `avoid` */
  const cutRow = (len, n, avoid = []) => { n = Math.max(1, q.minW === 0 ? n : Math.min(n, Math.floor(len / .4))); if (q.stagger === 0 && !avoid.length) return Array(n).fill(len / n); let best = null;
    for (let tries = 0; tries < 14; tries++) { let ws = Array.from({ length: n }, () => within(r, 1 - q.sizeVar, 1 + q.sizeVar)); const s0 = ws.reduce((a, b) => a + b, 0); ws = ws.map((v) => v / s0 * len);
      const joints = []; let a = 0; for (const w of ws.slice(0, -1)) { a += w; joints.push(a); } const clear = q.stagger === 0 ? 1 : Math.min(1, ...joints.flatMap((j) => avoid.map((k) => Math.abs(j - k)))), thin = Math.min(...ws);
      const score = Math.min(clear / (.12 + .2 * q.stagger), 1) + Math.min(thin / (q.minW === 0 ? .1 : .34), 1); if (!best || score > best.score) best = { ws, score }; if (score >= 2) break; } return best.ws; };
  const jointsOf = (ws, a0 = 0) => { const j = []; let a = a0; for (const w of ws.slice(0, -1)) { a += w; j.push(a); } return j; };
  const env = (a) => { const p = q.peak * L, far = a > p ? (a - p) / Math.max(L - p, .01) : (p - a) / Math.max(L, .01) * .5; return q.height * (1 - q.fall * far); };
  const rowB = { i: Math.max(0, q.rows - 1), t: 1, d0: dB, dp: D, len: L, cols: [] }; let rows = [rowB];
  const col = (o, row = rowB) => { const c = Object.assign({ row }, o); row.cols.push(c); return c; };

  /* ---- 1 the back mass ---- */
  if (courses) {   /* a stair: tiers of the whole footprint, each lying on the one below, set back on two sides, joints staggered */
    const K = Math.max(2, q.rows); rows = []; let sa = 1, sd = 1, z = 0, prev = [], wing = r() < .25 ? 1 + Math.floor(r() * (K - 2)) : -1;
    for (let k = 0; k < K; k++) { const last = k === K - 1; if (k) { sd *= within(r, .66, .8); if (k !== wing) sa *= last ? within(r, .4, .6) : within(r, .74, .9); }
      const len = L * sa, dp = D * sd, hh = q.height / K * within(r, .88, 1.12), row = { i: k, t: K > 1 ? k / (K - 1) : 1, d0: back - dp, dp, len, cols: [] }; rows.push(row);
      const ws = cutRow(len, last ? 1 : Math.max(1, Math.min(q.nA, 1 + Math.floor(r() * q.nA + (len > 1.1 ? .4 : 0)))), prev); let a0 = 0;
      ws.forEach((w, j) => { const zz = z + (j && !last ? (r() - .5) * .08 : 0) * (k ? 0 : 1); add({ a0, w, d0: back - dp, dp, z, h: hh + (j ? (r() - .5) * .1 : 0), level: k, depth: k, on: k ? Math.max(1, prev.filter((p) => p > a0 + .06 && p < a0 + w - .06).length + 1) : 0, role: 'course', col: col({ a0, w, h: z + hh }, row) }); a0 += w; });
      prev = jointsOf(ws); z += hh; }
  } else if (q.tiers) {   /* blocks: a first tier nearly level, shorter tiers on it, each set back from the face */
    const z1 = within(r, .5, .7); let ws = cutRow(L, q.nA, []), a0 = 0, sup = [], prev = jointsOf(ws), z = z1, len = L, sb = 0;
    ws.forEach((w, k) => { const hh = z1 + (k === ws.length - 1 && ws.length > 2 ? -within(r, .05, .14) : (r() - .5) * .03); add({ a0, w, d0: dB, dp: D, h: hh, col: col({ a0, w, h: hh }) }); sup.push({ a0, w, top: hh }); a0 += w; });
    const nT = Math.max(q.tiers, Math.min(4, 1 + Math.ceil((env(0) - z1) / .62))); for (let lv = 1; lv < nT; lv++) { const lastT = lv === nT - 1; len *= within(r, .62, .84); sb += within(r, .1, .22); const n = Math.max(1, Math.round(q.nA * len / L)), w2 = cutRow(len, n, prev); a0 = 0; const made = [];
      w2.forEach((w) => { const under = sup.filter((c) => c.a0 < a0 + w - .04 && c.a0 + c.w > a0 + .04); if (!under.length || Math.min(...under.map((c) => c.top)) < z - .16) { a0 += w; return; }
        let hh = lastT ? env(a0 + w / 2) * within(r, .94, 1.04) - z : within(r, .42, .6); if (lastT && made.length && Math.abs(made[made.length - 1].h - hh) < q.stepMin) hh -= q.stepMin; hh = Math.max(.3, Math.min(hh, .72));
        made.push(add({ a0, w, d0: dB + sb, dp: D - sb, z, h: hh, level: lv, depth: lv, on: under.length, role: 'tier', col: { row: rowB, a0, w, h: z + hh } })); a0 += w; });
      if (!made.length) break; if (!lastT) { const zz = z + Math.min(...made.map((m) => m.h)); for (const m of made) m.h = zz - z; z = zz; }
      sup = made.map((m) => ({ a0: m.a0, w: m.w, top: m.z + m.h })); prev = jointsOf(w2); }
  } else if (td) {   /* dipping beds: one thick bed from the ground, thinner and shorter beds lying tight on it */
    const c = col({ a0: 0, w: L, h: q.height }), n = r() < .5 ? 3 : 2, z1 = q.height * within(r, .5, .6); let w = L, dp = D, zc = z1, ac = L / 2;
    add({ a0: 0, w: L, d0: dB, dp: D, h: z1, dipA: -td, role: 'bed', col: c });
    for (let i = 1; i < n; i++) { const w2 = w * within(r, .7, .85), dp2 = dp * within(r, .8, .9), hh = i === n - 1 ? Math.min(.6, Math.max(.3, q.height - zc)) : within(r, .3, .45), a2 = w2 / 2, zb = zc + td * (ac - a2);
      add({ a0: 0, w: w2, d0: back - dp2, dp: dp2, z: zb, h: hh, dipA: -td, botA: -td, level: i, depth: i, on: 1, role: 'bed', col: { row: rowB, a0: 0, w: w2, h: zb + hh } }); w = w2; dp = dp2; zc = zb + hh; ac = a2; }
  } else if (tl) {   /* leaning slabs: parallel, of near-constant thickness, each shorter than the one nearer the cut end; a block fills the cut end under them */
    const n = Math.max(2, Math.min(4, q.nA)), h0 = q.height, sh = tl * h0 * .78; let a0 = sh, hh = h0;
    const bl = col({ a0: 0, w: sh + .12, h: h0 * (r() < .35 ? within(r, 1, 1.08) : within(r, .42, .62)) }); add({ a0: 0, w: bl.w, d0: dB, dp: D, h: bl.h, dipA: within(r, .25, .5), col: bl });
    for (let k = 0; k < n; k++) { const w = within(r, .3, .45), th = Math.tan((q.lean + (r() - .5) * 6) * Math.PI / 180), pr = r() < .4 ? within(r, .06, .18) : 0;
      add({ a0, w, d0: dB - pr, dp: D + pr, h: hh, lean: th, role: 'slab', col: col({ a0, w, h: hh }) }); a0 += w; hh = Math.max(.6, hh * (1 - q.fall * within(r, .35, .6) * (2.2 / n)) - .05); }
  } else {   /* pillars, walls, gates: columns under the envelope */
    const ws = cutRow(L, q.nA, []); let a0 = 0; const B = ws.map((w, k) => { const c = col({ a0, w, k, h: Math.max(.3, env(a0 + w / 2) * within(r, 1 - .08 * q.rough, 1 + .08 * q.rough)) }); a0 += w; return c; });
    if (q.plateau && B.length >= 2) { const n = Math.min(B.length, 2 + (r() < .45 ? 1 : 0)), k0 = Math.floor(r() * (B.length - n + 1)), run = B.slice(k0, k0 + n), m = run.reduce((a, c) => a + c.h, 0) / n, s = within(r, .2, .3), am = run.reduce((a, c) => a + c.a0 + c.w / 2, 0) / n; run.forEach((c) => { c.h = m - s * (c.a0 + c.w / 2 - am); c.plat = 1; c.platDip = s; }); }
    if (q.cleft && B.length >= 2) { let k = 0; for (let j = 1; j < B.length - 1; j++) if (B[j].h + B[j + 1].h > B[k].h + B[k + 1].h) k = j; const m = Math.max(B[k].h, B[k + 1].h); B[k].h = m; B[k + 1].h = m - .2 * (B[k].w + B[k + 1].w) / 2 * within(r, .7, 1.2); B[k].plat = B[k + 1].plat = 2; B[k].gapFar = B[k + 1].gapNear = within(r, .035, .06); }
    B.forEach((c, k) => { if (k > 0) { const p = B[k - 1]; if (!(p.plat && c.plat && p.plat === c.plat) && Math.abs(p.h - c.h) < q.stepMin) c.h = Math.max(.3, p.h + (c.h >= p.h && r() < .5 ? 1 : -1) * q.stepMin * (1 + r() * .5)); } });
    let caps = [];
    if (q.gate && B.length >= 3) { const g = B[1], N = B[0], F = B[2], thN = within(r, .5, .7), thF = within(r, .42, .6); g.recess = within(r, .32, .45); N.h = Math.max(N.h, q.height * .92); F.h = N.h * within(r, .78, .92); g.h = Math.min(N.h, F.h) - .02; const split = within(r, .4, .62);
      caps = [{ a0: N.a0 + N.w * .12, w: N.w * .88 + g.w * split, z: N.h, h: thN, o: within(r, .22, .36), on: 2 }, { a0: g.a0 + g.w * split, w: g.w * (1 - split) + F.w * within(r, .8, 1), z: F.h, h: Math.max(.35, N.h + thN - F.h - within(r, .18, .32)), o: within(r, .26, .4), on: 2 }]; }
    B.forEach((c, k) => { const pr = q.proud && !c.recess && !(k && B[k - 1].pr) && r() < .42 ? within(r, .08, .24) : 0, gN = c.gapNear || 0, gF = c.gapFar || 0; c.pr = pr; const rec = c.recess ? D * c.recess : 0;
      if (q.oversail && k === B.length - 1 && c.h > .8 && !c.recess) { const zc = c.h * within(r, .45, .62), sb = q.over == null ? within(r, .22, .36) : q.over; add({ a0: c.a0, w: c.w, d0: dB + sb, dp: D - sb, h: zc, col: c, covered: 1 }); add({ a0: c.a0, w: c.w, d0: dB - .04, dp: D + .04, z: zc, h: c.h - zc, level: 1, depth: 1, on: 1, role: 'cap', col: c }); return; }
      add({ a0: c.a0 + gN, w: c.w - gN - gF, d0: dB - pr + rec, dp: D + pr - rec, h: c.h, platDip: c.platDip, col: c, covered: c.recess ? 1 : 0 }); });
    for (const l of caps) add({ a0: l.a0, w: l.w, d0: dB - l.o, dp: D + l.o, z: l.z, h: l.h, level: 1, depth: 1, on: l.on, role: 'cap', col: { row: rowB, a0: l.a0, w: l.w, h: l.z + l.h } });
  }
  /* what stands behind a stretch of the front: the face (smallest d) and the lowest ground-standing top there */
  const wallAt = (a0, a1) => { const b = S.filter((s) => s.level === 0 && s.role !== 'front' && s.a0 < a1 - .03 && s.a0 + s.w > a0 + .03); return b.length ? { d: Math.min(...b.map((s) => s.d0)), h: Math.min(...b.map((s) => s.h)), hi: Math.max(...b.map((s) => s.h)) } : { d: dB, h: q.height * .5, hi: q.height * .5 }; };

  /* ---- 2 the front cluster at the cut end: a chunky buttress hard against the wall, a toe in front of it or beside it ---- */
  if (q.rows > 1 && !courses) { const rowF = { i: 0, t: 0, d0: dB - .5, dp: .5, len: L * .5, cols: [] }; rows.unshift(rowF); if (q.rows > 2) rows.splice(1, 0, { i: 1, t: .5, d0: dB - .5, dp: .5, len: L * .7, cols: [] });
    const w = Math.min(within(r, .42, .62), L * .5), wl = wallAt(0, w), dp = Math.max(.3, Math.min(.58, w * within(r, .62, .98))), hB = Math.max(.3, Math.min(q.front * wl.hi, wl.h - q.stepMin - .04));
    const bu = add({ a0: 0, w, d0: wl.d - dp + within(r, .05, .14), dp, h: hB, role: 'front', kind: 'buttress', col: col({ a0: 0, w, h: hB }, rowF) }); let aNext = w - .03;
    if (q.rows > 2) { const w2 = within(r, .36, .55), wl2 = wallAt(aNext, aNext + w2), dp2 = Math.max(.3, Math.min(.55, w2 * within(r, .7, 1.05))), h2 = Math.max(.28, Math.min(hB * within(r, .6, 1.25), wl2.h - q.stepMin - .04)); if (Math.abs(h2 - hB) < q.stepMin) { /* never level with the buttress */ }
      const hh = Math.abs(h2 - hB) < q.stepMin ? Math.max(.25, hB - q.stepMin * 1.4) : h2; add({ a0: aNext, w: w2, d0: wl2.d - dp2 + within(r, .05, .12), dp: dp2, h: hh, role: 'front', kind: 'buttress', col: col({ a0: aNext, w: w2, h: hh }, rows[1]) }); aNext += w2 - .03; }
    if (q.toe) { const w3 = within(r, .3, .45), dp3 = within(r, .28, .42), h3 = Math.min(within(r, .2, .4), hB * .65), beside = q.rows < 3 && r() < .4, a3 = beside ? aNext : within(r, 0, .12), d3 = beside ? wallAt(a3, a3 + w3).d - dp3 + .08 : bu.d0 - dp3 + .06;
      add({ a0: a3, w: w3, d0: d3, dp: dp3, h: h3, role: 'front', kind: 'toe', col: col({ a0: a3, w: w3, h: h3 }, rowF) }); if (beside) aNext += w3; }
    /* a small stone tucked into the corner between the cluster and the wall */
    if (q.tuck && aNext < L - .35) { const w4 = within(r, .2, .32), wl4 = wallAt(aNext, aNext + w4), h4 = hB * within(r, .3, .55); add({ a0: aNext, w: w4, d0: wl4.d - w4 + .04, dp: w4, h: h4, role: 'front', kind: 'toe', col: col({ a0: aNext, w: w4, h: h4 }, rowF) }); }
    /* a pillar applied to the wall, over a joint */
    if (q.applied && rowB.cols.length >= 2 && !tl && !td && !q.tiers) { const js = rowB.cols.slice(0, -1).map((c) => c.a0 + c.w).filter((j) => j > aNext + .2 && j < L - .2); if (js.length) { const j = js[Math.floor(r() * js.length)], w5 = within(r, .3, .45), dp5 = within(r, .25, .35), wl5 = wallAt(j - w5 / 2, j + w5 / 2), h5 = wl5.h * within(r, .7, .93);
      add({ a0: j - w5 / 2, w: w5, d0: wl5.d - dp5 + .04, dp: dp5, h: h5, role: 'wall', kind: 'pillar', applied: 1, col: { row: rowB, a0: j - w5 / 2, w: w5, h: h5 } }); } } }

  /* ---- 3 the pile: stones laid on the tops. They add height. ---- */
  const tops = () => S.filter((s) => !s.covered && s.role !== 'front' && s.role !== 'slab' && !s.applied && s.w > .32 && s.dp > .3);
  for (let i = 0; i < q.caps; i++) { const T = tops(); if (!T.length) break; const top = (s) => s.z + s.h;
    const pairs = []; for (const a of T) for (const b of T) if (a !== b && Math.abs(a.a0 + a.w - b.a0) < .07 && (a.platDip && b.platDip ? true : Math.abs(top(a) - top(b)) < (a.level || b.level ? .09 : .22) && !a.platDip && !b.platDip) && !a.dipA && !b.dipA && a.role !== 'cap' && b.role !== 'cap' && Math.min(a.d0 + a.dp, b.d0 + b.dp) - Math.max(a.d0, b.d0) > .3) pairs.push([a, b]);
    if (pairs.length && (i > 0 ? r() < .85 : r() < .65)) {   /* a block spanning a joint, bearing on both */
      const [a, b] = pairs[Math.floor(r() * pairs.length)]; if (!a.platDip && !a.level && !b.level) { const mt = (top(a) + top(b)) / 2; a.h = mt - a.z; b.h = mt - b.z; a.col.h = a.h; b.col.h = b.h; } const a0 = a.a0 + a.w * (1 - within(r, .35, .7)), a1 = b.a0 + b.w * within(r, .35, .7), d0 = Math.max(a.d0, b.d0) + within(r, .05, .15), dp = Math.min(.8, Math.min(a.d0 + a.dp, b.d0 + b.dp) - d0), hh = within(r, .25, .48), sl = a.platDip ? -a.platDip : 0, am = (a0 + a1) / 2, z = sl ? top(a) + sl * (am - (a.a0 + a.w / 2)) : Math.max(top(a), top(b));
      add({ a0, w: a1 - a0, d0, dp, z, h: hh, dipA: sl, botA: sl, level: Math.max(a.level, b.level) + 1, depth: Math.max(a.depth, b.depth) + 1, on: 2, role: 'cap', col: { row: rowB, a0, w: a1 - a0, h: z + hh } }); a.covered = b.covered = 1; continue; }
    /* a summit cap on the highest top, or a second storey on a cap already placed; smaller than what carries it, set back from its exposed sides */
    const sup = T.reduce((u, v) => (top(v) > top(u) ? v : u)), slope = sup.dipA || (sup.platDip ? -sup.platDip : 0), wide = sup.w > .5, w = sup.w * (wide ? within(r, .5, .8) : within(r, .82, .94)), a0 = sup.a0 + (sup.w - w) * within(r, .1, .6), sb = within(r, .05, .15), dp = Math.min(.72, sup.dp - sb - (sup.dp > .6 ? within(r, 0, .12) : 0)), hh = sup.role === 'cap' ? within(r, .2, .32) : within(r, .22, .45);
    const z = top(sup) + slope * (a0 + w / 2 - (sup.a0 + sup.w / 2)); add({ a0, w, d0: sup.d0 + sb, dp, z, h: hh, dipA: slope, botA: slope, level: sup.level + 1, depth: sup.depth + 1, on: 1, role: 'cap', col: { row: rowB, a0, w, h: z + hh } }); sup.covered = 1; }

  /* ---- 4 shapes: each stone learns which of its top edges are exposed, then is shaped by its role ---- */
  const out = [], near = (u, v) => Math.abs(u - v) < .08;
  for (const sp of S) { const top = sp.z + sp.h, a1 = sp.a0 + sp.w, d1 = sp.d0 + sp.dp, m = Math.min(sp.w, sp.dp, sp.h), front = sp.role === 'front';
    const hid = (test) => S.some((o) => o !== sp && o.z + o.h > top - .06 && o.z < top && test(o)), ovD = (o) => Math.min(d1, o.d0 + o.dp) - Math.max(sp.d0, o.d0) > .08, ovA = (o) => Math.min(a1, o.a0 + o.w) - Math.max(sp.a0, o.a0) > .08;
    const cov = [hid((o) => near(o.a0 + o.w, sp.a0) && ovD(o)), hid((o) => near(o.a0, a1) && ovD(o)), hid((o) => near(o.d0 + o.dp, sp.d0) && ovA(o)), d1 > back - .05 || hid((o) => (near(o.d0, d1) || (o.d0 < d1 && o.d0 + o.dp > d1)) && ovA(o))];
    const bv = q.bevel * (q.bevelEach ? Math.max(.05, 1 + (r() - .5) * 2 * q.bevelEach) : 1), big = q.shapes && !cov[2] && sp.dp > .34 && r() < (front ? .45 : .3);
    const bT = cov.map((c, i) => c ? bv * within(r, .25, .45) : (i === 2 && big ? within(r, .3, .55) * sp.dp / m : bv * within(r, .95, 1.5)));
    const tall = sp.h > 1.3 * sp.w, poly = q.shapes && tall && !front && r() < .3, prow = q.shapes && tall && !poly && r() < .2, vc = [bv * within(r, .7, 1.2), bv * within(r, .7, 1.2), bv * .4, bv * .4];
    if (poly) { vc[0] = within(r, .25, .38); vc[1] = within(r, .25, .38); } else if (prow) { vc[r() < .5 ? 0 : 1] = within(r, .3, .42); }
    let bat = [0, 0, 0, 0], tA = 1, tD = 1, dipA = sp.dipA || 0, dipD = 0, leanA = 0, kind = sp.kind;
    if (!q.shapes) { tA = q.taper; tD = Math.min(1, q.taper + .02); dipD = within(r, -.02, .1) * q.rough; dipA += sp.platDip ? -sp.platDip : -within(r, 0, .08) * q.rough; }
    else if (front) { bat = [0, Math.min(sp.w * .2, sp.h * within(r, .04, .12)), Math.min(sp.dp * .35, sp.h * within(r, .1, .25)), 0]; const u = r(); if (big) dipD = within(r, 0, .1); else if (u < .6) dipD = within(r, .35, .7); else if (u < .75) dipA = within(r, .25, .45); else dipD = within(r, 0, .12); }
    else if (sp.role === 'slab') { leanA = -sp.lean; dipA = sp.lean * within(r, .5, 1); tA = within(r, .93, 1); bat[2] = sp.h * within(r, .02, .06); }
    else if (sp.level > 0) { tA = within(r, .95, 1); tD = within(r, .96, 1); if (sp.role === 'cap' && !sp.dipA) dipA = (r() - .5) * .2; dipD = within(r, -.03, .08); if (!cov[2]) bat[2] = sp.h * within(r, 0, .12); }
    else { const spire = tall && r() < .1; tA = spire ? within(r, .55, .7) : tall ? within(r, .84, .97) : within(r, .92, 1); tD = spire ? within(r, .7, .85) : 1; bat[2] = Math.min(sp.dp * .25, sp.h * within(r, .03, .09)); dipD = sp.role === 'bed' ? within(r, 0, .06) : within(r, -.02, .12) * q.rough; if (!sp.dipA) dipA = sp.platDip ? -sp.platDip : -within(r, 0, .09) * q.rough; }
    tA *= q.taper / .95; if (!kind) kind = sp.role === 'cap' ? 'cap' : sp.role === 'slab' ? 'lean' : sp.h > 1.35 * Math.max(sp.w, sp.dp) ? 'pillar' : sp.h < .7 * Math.max(sp.w, sp.dp) ? 'slab' : 'block';
    const g = sp.level ? .004 : .012, st = { w: Math.max(.08, sp.w - g - q.gap), dp: Math.max(.08, sp.dp - g - q.gap), h: sp.h, a: sp.a0 + sp.w / 2 + (r() - .5) * q.offset * .3, d: sp.d0 + sp.dp / 2 + (r() - .5) * q.offset * .3, z: sp.z,
      bevel: bv, bevelT: bT, bevelVC: vc, bevelB: 0, taperA: tA, taperD: tD, batter: bat, dipA, dipD, botA: sp.botA || 0,
      leanA: leanA + (q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0), leanD: q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0,
      cut: (q.shapes && !sp.level && !front && m > .32 && !big && r() < q.cutCorner) ? within(r, .26, .4) : 0, cutAt: r() < .6 ? 0 : 1, jit: sp.level || front ? q.jit * .5 : q.jit,
      yaw: (sp.level || sp.role === 'bed' ? 0 : (r() - .5) * q.offset * 1.4) + (q.yawEach ? (r() - .5) * 2 * q.yawEach * Math.PI / 180 : 0), seed: Math.floor(r() * 1e6), kind, col: sp.col, level: sp.level, on: sp.on, depth: sp.depth };
    out.push(st);
    /* a stepped stone: a shoulder on the top, flush with the back and one side, the two read as one stone */
    if (q.shapes && (front || kind === 'block') && !sp.level && sp.h > .4 && sp.w > .4 && Math.abs(dipD) < .2 && Math.abs(dipA) < .15 && r() < .16) { const w2 = sp.w * within(r, .45, .65), dp2 = sp.dp * within(r, .5, .7), h2 = Math.min(.3, sp.h * within(r, .25, .45)), nearSide = r() < .5;
      out.push(Object.assign({}, st, { part: true, w: w2, dp: dp2, h: h2 + .02, a: nearSide ? sp.a0 + w2 / 2 + .01 : a1 - w2 / 2 - .01, d: d1 - dp2 / 2 - .01, z: top - .02, bevelT: [bv, bv, bv * 1.3, bv * .4], bevelVC: null, bevelV: bv * .7, batter: null, taperA: .97, taperD: .97, dipA: 0, dipD: within(r, 0, .08), cut: 0, yaw: st.yaw, seed: st.seed + 1 })); st.cut = 0; } }
  return { q, rows, stones: out };
}
/* the count is held to what the sheets show: at most eleven stones */
function formation(over = {}) { let f = formationOnce(over), o = Object.assign({}, over), n = 0; const count = (g) => g.stones.filter((s) => !s.part).length;
  while (count(f) > 11 && n++ < 5) { o = Object.assign({}, o, f.q.caps > 1 ? { caps: f.q.caps - 1 } : f.q.tuck || f.q.applied ? { tuck: 0, applied: 0 } : { nA: Math.max(1, f.q.nA - 1), rows: f.q.cls === 'courses' ? Math.max(3, f.q.rows - 1) : f.q.rows }); f = formationOnce(o); } return f; }
const formMeshes = (f) => f.stones.map(stoneMesh);

/* Scale 1: draw a whole formation's numbers from the proposed ranges. */
function drawParams(seed, want) {
  const r = rng(seed * 31 + 5), u = r() * 52, cls = want && want !== 'any' ? want : (u < 22 ? 'pillars' : u < 32 ? 'blocks' : u < 42 ? 'leaners' : u < 49 ? 'courses' : 'gates');
  const o = { cls: cls === 'ramp' ? 'leaners' : cls === 'wall' ? 'pillars' : cls, preset: cls, seed, height: mid(r, 1.42, 1.75), length: within(r, 1.25, 1.7), bevel: within(r, .1, .2), fall: within(r, .35, .75), peak: r() < .1 ? within(r, .25, .45) : within(r, 0, .18), sizeVar: .6,
    front: r() < .15 ? within(r, .62, .82) : within(r, .42, .6), rows: r() < .12 ? 1 : r() < .45 ? 3 : 2, tuck: r() < .3 ? 1 : 0 };
  const pile = (p0, p1, p2) => { const v = r(); return v < p0 ? 0 : v < p0 + p1 ? 1 : v < p0 + p1 + p2 ? 2 : 3; };
  if (cls === 'pillars' || cls === 'wall') { const wall = cls === 'wall' || r() < .4; Object.assign(o, { nA: 3 + (r() < .45 ? 1 : 0) - (r() < .2 ? 1 : 0), depth: within(r, .42, .58), plateau: wall || r() < .25 ? 1 : 0, cleft: r() < .12 ? 1 : 0, oversail: r() < .22 ? 1 : 0, applied: r() < .25 ? 1 : 0, caps: pile(.1, .42, .34) }); if (wall) Object.assign(o, { fall: within(r, .15, .35), peak: within(r, .05, .35) }); }
  if (cls === 'blocks') Object.assign(o, { nA: 2 + (r() < .55 ? 1 : 0), tiers: 2 + (r() < .55 ? 1 : 0), depth: within(r, .62, .82), fall: within(r, .3, .65), sizeVar: .35, caps: pile(.25, .5, .25), rows: r() < .6 ? 2 : 3 });
  if (cls === 'leaners') { if (r() < .35) Object.assign(o, { preset: 'ramp', dip: within(r, 20, 32), fall: within(r, .05, .2), depth: within(r, .62, .8), caps: pile(.25, .5, .25) }); else Object.assign(o, { lean: within(r, 15, 25), nA: 3 + (r() < .5 ? 1 : 0), length: within(r, 1.25, 1.5), depth: within(r, .5, .68), caps: pile(.55, .4, .05), front: within(r, .3, .45) }); }
  if (cls === 'ramp') Object.assign(o, { dip: within(r, 20, 32), fall: within(r, .05, .2), depth: within(r, .62, .8), caps: pile(.25, .5, .25) });
  if (cls === 'courses') Object.assign(o, { height: mid(r, 1.4, 1.7), rows: 3 + (r() < .7 ? 1 : 0), nA: 1 + Math.floor(r() * 2.5), depth: within(r, 1, 1.25), length: within(r, 1.35, 1.65), caps: pile(.5, .42, .08), sizeVar: .3 });
  if (cls === 'gates') Object.assign(o, { height: within(r, 1.05, 1.2), rows: 2 + (r() < .4 ? 1 : 0), fall: within(r, .2, .4), peak: within(r, .05, .25), depth: within(r, .65, .8), caps: pile(.5, .45, .05), length: within(r, 1.4, 1.7) });
  if (cls !== 'courses') o.height *= 1 - .07 * o.caps;
  return o;
}
/* What a generated formation would measure if it were traced like a reference: each stone as seen from the references' camera, in fh,
   and the relations between touching stones measured the way analyze.py measures them on the traces. */
function seen(f) {
  const cam = cameraFor('ref', 360, 300), pr = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) * 180, (1 - v.y) * 150]; };
  let X0 = 1e9, X1 = -1e9, Y0 = 1e9, Y1 = -1e9; const fig = pr(1.25, .45, 0), figTop = pr(1.25, .45, 1), fh = Math.hypot(fig[0] - figTop[0], fig[1] - figTop[1]);
  const stones = f.stones.filter((o) => !o.part).map((o) => { const P = stonePoints(o).map((p) => pr(...p)); const x0 = Math.min(...P.map((p) => p[0])), x1 = Math.max(...P.map((p) => p[0])), y0 = Math.min(...P.map((p) => p[1])), y1 = Math.max(...P.map((p) => p[1]));
    X0 = Math.min(X0, x0); X1 = Math.max(X1, x1); Y0 = Math.min(Y0, y0); Y1 = Math.max(Y1, y1);
    const b = pr(o.a, o.d - o.dp / 2, o.z), t = pr(o.a + o.leanA * o.h, o.d - o.dp / 2, o.z + o.h), hiP = P.reduce((u, v) => (v[1] < u[1] ? v : u)); return { wb: (x1 - x0) / fh, hb: Math.hypot(b[0] - t[0], b[1] - t[1]) / fh, kind: o.kind, x0, x1, y0, y1, xTop: hiP[0], o }; });
  const W = (X1 - X0) / fh, H = (Y1 - Y0) / fh, pairs = [];
  for (let i = 0; i < stones.length; i++) for (let j = i + 1; j < stones.length; j++) { const A = stones[i].o, Bq = stones[j].o, gapA = Math.abs(A.a - Bq.a) - (A.w + Bq.w) / 2, gapD = Math.abs(A.d - Bq.d) - (A.dp + Bq.dp) / 2, zo = Math.min(A.z + A.h, Bq.z + Bq.h) - Math.max(A.z, Bq.z);
    if (zo < .1 || !((gapA < .06 && gapD < -.08) || (gapD < .06 && gapA < -.08))) continue; const lo = stones[i].y0 > stones[j].y0 ? stones[i] : stones[j], hi = lo === stones[i] ? stones[j] : stones[i];
    pairs.push({ dtop: Math.abs(stones[i].y0 - stones[j].y0) / fh, lowfront: Math.abs(lo.y1 - hi.y1) / fh > .04 ? lo.y1 > hi.y1 : null }); }
  const near = stones.reduce((a, b) => (b.y1 > a.y1 ? b : a)), top = stones.reduce((a, b) => (b.y0 < a.y0 ? b : a)), ws = stones.map((s2) => s2.wb);
  const leftTop = Math.min(...stones.filter((s2) => s2.x0 < X0 + (X1 - X0) * .12).map((s2) => s2.y0));
  return { stones, pairs, W, H, n: stones.length, footshare: (Y1 - near.y0) / (Y1 - Y0), peak: (top.xTop - X0) / (X1 - X0), wratio: Math.max(...ws) / Math.min(...ws), farend: (Y1 - leftTop) / (Y1 - Y0),
    jut: f.stones.some((o) => o.kind === 'cap'), rest: f.stones.filter((o) => !o.part && o.level > 0).length, two: f.stones.filter((o) => !o.part && o.on >= 2).length, real: f.stones.filter((o) => !o.part).length, deep: Math.max(...f.stones.map((o) => o.depth || 0)) };
}
