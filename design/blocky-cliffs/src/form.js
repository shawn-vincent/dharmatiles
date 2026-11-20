/* ---------- the formation (proposed; chapter 11 states it) ----------
   A pile. A back mass stands along the back edge: pillars, tiers of blocks, a stair of courses, a set of leaning slabs, dipping beds,
   or a gate. A front cluster stands against it at the cut end: a buttress and a toe. Stones may stand proud of the wall, be applied to
   it, or be tucked into a corner. Then stones are piled on: a summit cap, a block spanning a joint, a second storey.
   Every stone is first a plain box with a place (a "spec"); shapes are given last, when each stone knows which of its edges are exposed. */
const FORM = { seed: 1, cls: 'pillars',
  length: 1.45, depth: .62, height: 1.7,    /* fh: along the cliff; the back mass front to back; the highest top of the back mass (caps add to it) */
  nA: 3, rows: 2,          /* stones along the back mass; rows: 1 = no front cluster, 2 = buttress and toe, 3 = and a second block along the wall */
  front: .65,              /* the buttress's height as a share of the wall behind it */
  fall: .45,               /* how far the tops fall from the peak to the far end, as a share of the height */
  peak: .1,                /* where the peak is along the cliff (0 = at the cut end, .5 = mid length) */
  sizeVar: .55,            /* how unequal the widths are */
  stagger: .5,             /* joints of one tier against the next: 0 in line */
  stepMin: .13,            /* least step between neighbouring tops that are not on a plateau, fh */
  plateau: 0,              /* 1: a run of two or three neighbours shares one sloping top plane */
  cleft: 0,                /* 1: an open joint between the two tallest neighbours */
  stacks: 0, blockCourse: 0, tallOne: 0,   /* stacks: blocks built as columns of stacked blocks; blockCourse: a stair's tiers cut into blocks; tallOne: one column stands above the envelope */
  caps: 0, split: null,    /* stones piled on: an upper block or a block spanning a joint; their height comes off what carries them. split: the chance a tall pillar is split by a level joint (0.35 when not given) */
  proud: 1, applied: 0, tuck: 0,   /* stones standing forward of the wall; a pillar applied over a joint; a small stone tucked in a corner */
  tilt: 0,                 /* degrees: every column of the back mass leans this far toward the cut end (columns become slabs) */
  gate: 0,                 /* share of the length left open between two piers, under a lintel; 0 none */
  oversail: 0, over: null, /* 1: an upper block at the far end oversails a set-back lower wall; over: by how much, fh */
  tiers: 0,                /* 2-4: the back mass is tiers of blocks, each shorter and set back, joints staggered */
  lean: 0,                 /* degrees: the back mass is parallel slabs leaning this far toward the cut end */
  dip: 0,                  /* degrees: the back mass is beds dipping this far */
  taper: .95, bevel: .16, offset: .03, cutCorner: .2,
  toe: true, rough: 1, minW: 1,
  leanEach: 0, yawEach: 0, bevelEach: 0, gap: 0 };
const PRESET = {
  pillars: { nA: 3, rows: 2 },
  wall: { cls: 'pillars', nA: 3, rows: 2, fall: .25, plateau: 1, peak: .25 },
  blocks: { nA: 3, rows: 2, tiers: 3, depth: .72, fall: .4, sizeVar: .35, caps: 1 },
  courses: { nA: 2, rows: 4, depth: 1.1, length: 1.5, fall: .2, sizeVar: .3, toe: false, peak: 0, caps: 0 },
  leaners: { nA: 4, rows: 2, lean: 20, fall: .35, depth: .6 },
  ramp: { cls: 'leaners', nA: 1, rows: 2, dip: 27, fall: .1, depth: .7, caps: 1 },
  gates: { nA: 3, rows: 2, gate: .34, fall: .3, depth: .72, peak: .15, height: 1.3, caps: 1 } };
/* ---------- bearing on a base ----------
   A stone that is not on the ground may overhang: part of its underside has no stone top within 0.01 fh below it. Where there is stone mass
   a little way below that part, the stone is built taller, its top where the design put it and its bottom lower, so that 3 in 4 of the
   unsupported part rests on or 0.015 fh into the stone below. It is done only when that is at most SUPPORT.reach, so a real overhang (a
   gate's lintel over its opening, an oversail over a set-back wall) stays one. It is done when the pile is made: the stone's own height is the height it bears at. */
const SUPPORT = { share: .75, reach: .25, sink: .015, ok: .01, n: 7 };
const botZ = (s, a) => s.z + s.botA * (a - s.a), topZ = (s, a, d) => s.z + s.h + s.dipA * (a - s.a) + s.dipD * (d - s.d);
/* under a stone's footprint: the share supported, and for each unsupported sample the drop to the highest stone top below it (Infinity: open air) */
function underside(S, i) {
  const s = S[i], n = SUPPORT.n, drops = []; let sup = 0, tot = 0;
  for (let u = 0; u < n; u++) for (let v = 0; v < n; v++) { const a = s.a + (-.5 + (u + .5) / n) * (s.w - .1), d = s.d + (-.5 + (v + .5) / n) * (s.dp - .1), zb = botZ(s, a); tot++;
    let best = -1e9, hit = false; for (let k = 0; k < S.length; k++) { if (k === i) continue; const t = S[k]; if (Math.abs(a - t.a) > t.w / 2 - .01 || Math.abs(d - t.d) > t.dp / 2 - .01 || t.z > zb + .005) continue; const top = topZ(t, a, d); if (top >= zb - SUPPORT.ok) { hit = true; break; } if (top > best) best = top; }
    if (hit) sup++; else drops.push(best > -1e8 ? zb - best : Infinity); }
  return { frac: sup / tot, drops, tot };
}
/* lowest stone first, so a stone that has been made taller bears the ones above it; `stats` collects what was done */
function bear(S, stats) {
  const order = S.map((s, i) => i).sort((p, q) => S[p].z - S[q].z);
  for (const i of order) { const s = S[i]; if (s.part || s.z < .02) continue; const u = underside(S, i); if (u.drops.length === 0) continue;
    const fin = u.drops.filter((x) => isFinite(x)).sort((p, q) => p - q), need = Math.ceil(SUPPORT.share * u.drops.length);
    if (stats) stats.push({ i, frac: u.frac, done: false });
    if (fin.length < need) continue; const d = fin[need - 1] + SUPPORT.sink; if (d > SUPPORT.reach || s.z - d < 0) continue;
    s.z -= d; s.h += d; if (stats) { stats[stats.length - 1].done = true; stats[stats.length - 1].d = d; } }
}
/* `marks` times the steps and says how many specs each had made: the pictures of the algorithm page are made from them */
const now = () => (typeof performance !== 'undefined' ? performance.now() : 0);
function formationOnce(over = {}) {
  const marks = [{ name: 'start', n: 0, t: now() }], lap = (name, n) => marks.push({ name, n, t: now() });
  const q = Object.assign({}, FORM, PRESET[over.preset || over.cls] || {}, over), r = rng(q.seed * 104729 + 7), S = [];
  const td = Math.tan(q.dip * Math.PI / 180), tl = Math.tan(q.lean * Math.PI / 180), L = q.length, back = PLATE, D = q.depth, dB = back - D, courses = q.cls === 'courses';
  const add = (o) => { const sp = Object.assign({ z: 0, level: 0, on: 0, depth: 0, role: 'wall' }, o); S.push(sp); return sp; };
  /* a row cut into unequal widths, none thin, joints kept off the joints in `avoid` */
  const cutRow = (len, n, avoid = []) => { n = Math.max(1, q.minW === 0 ? n : Math.min(n, Math.floor(len / (q.tiers || q.stacks || courses ? .45 : .43)))); if (q.stagger === 0 && !avoid.length) return Array(n).fill(len / n); let best = null;
    for (let tries = 0; tries < 14; tries++) { let ws = Array.from({ length: n }, () => within(r, 1 - q.sizeVar, 1 + q.sizeVar)); const s0 = ws.reduce((a, b) => a + b, 0); ws = ws.map((v) => v / s0 * len);
      const joints = []; let a = 0; for (const w of ws.slice(0, -1)) { a += w; joints.push(a); } const clear = q.stagger === 0 ? 1 : Math.min(1, ...joints.flatMap((j) => avoid.map((k) => Math.abs(j - k)))), thin = Math.min(...ws);
      const score = Math.min(clear / (.12 + .2 * q.stagger), 1) + Math.min(thin / (q.minW === 0 ? .1 : .42), 1); if (!best || score > best.score) best = { ws, score }; if (score >= 2) break; } return best.ws; };
  const jointsOf = (ws, a0 = 0) => { const j = []; let a = a0; for (const w of ws.slice(0, -1)) { a += w; j.push(a); } return j; };
  const env = (a) => { const p = q.peak * L, far = a > p ? (a - p) / Math.max(L - p, .01) : (p - a) / Math.max(L, .01) * .5; return q.height * (1 - q.fall * far); };
  const rowB = { i: Math.max(0, q.rows - 1), t: 1, d0: dB, dp: D, len: L, cols: [] }; let rows = [rowB];
  const col = (o, row = rowB) => { const c = Object.assign({ row }, o); row.cols.push(c); return c; };

  /* ---- 1 the back mass ---- */
  if (courses) {   /* a stair: tiers of the whole footprint, each lying on the one below and set back toward the back corner. A tier is two or three
       stones of unequal length whose treads stand 0.05 to 0.15 fh apart, the taller usually nearer the cut end and a little forward. */
    const K = Math.max(2, q.rows); rows = []; let sa = 1, sd = 1, z = 0, prev = []; const wing = r() < .5 ? 1 + Math.floor(r() * Math.max(1, K - 2)) : -1, T = [];
    for (let k = 0; k < K; k++) { const last = k === K - 1; if (k) { sd *= within(r, .66, .8); if (k !== wing) sa *= last ? within(r, .5, .7) : within(r, .9, 1); } const hh = q.height / K * within(r, .88, 1.12); T.push({ len: L * sa, dp: D * sd, hh, z, last }); z += hh; }
    T.forEach((t, k) => { const row = { i: k, t: K > 1 ? k / (K - 1) : 1, d0: back - t.dp, dp: t.dp, len: t.len, cols: [] }; rows.push(row); const up = T[k + 1];
      const n = q.blockCourse ? Math.max(1, Math.min(t.last ? 2 : 3, Math.round(t.len / .6))) : t.last ? (r() < .35 ? 2 : 1) : Math.max(1, Math.min(3, (t.len > .9 ? 2 : 1) + (q.nA > 2 && r() < .4 ? 1 : 0) - (r() < .2 ? 1 : 0)));
      const ws = cutRow(t.len, n, prev), tallFirst = r() < .8; let a0 = 0; for (const o of S) if (o.role === 'course' && o.level === k - 1) o.covered = 1;
      ws.forEach((w, j) => { const under = up && a0 < up.len - .05, clean = !up || a0 >= up.len - .05 || (a0 + w <= up.len + .03 && t.dp - up.dp > .18), drop = clean && ws.length > 1 && (tallFirst ? j > 0 : j < ws.length - 1) ? within(r, .05, .15) : 0, sb = drop ? within(r, .02, .07) : 0;
        /* a lowered stone is lowered only in its tread; under the tier above it keeps the tier's height, as a second piece whose joint hides in the corner */
        const split = drop && under, dpF = split ? t.dp - up.dp + .02 : t.dp;
        const sp = add({ a0, w, d0: back - t.dp + sb, dp: dpF - sb, z: t.z, h: t.hh - drop, level: k, depth: k, on: k ? Math.max(1, prev.filter((p) => p > a0 + .06 && p < a0 + w - .06).length + 1) : 0, role: 'course', col: col({ a0, w, h: t.z + t.hh - drop }, row) });
        if (split) add({ a0, w, d0: back - up.dp, dp: up.dp, z: t.z, h: t.hh, level: k, depth: k, role: 'course', part: 1, covered: 1, col: sp.col });
        a0 += w; });
      prev = jointsOf(ws); });
  } else if (q.stacks) {   /* blocks: two or three columns of big blocks; where two neighbours' first blocks top out level, the next block lies across the joint */
    const ws = cutRow(L, q.nA, []), C = []; let a0 = 0, lastTop = -9, lastHb = 0;
    ws.forEach((w, k) => { let top = Math.max(.55 * q.height, env(a0 + w / 2) * within(r, .88, 1.06)); if (Math.abs(top - lastTop) < .18) top = Math.max(.55 * q.height, lastTop - within(r, .18, .3)); lastTop = top;
      let hb = k && r() < .65 ? lastHb + (r() - .5) * .08 : within(r, .6, 1.05); hb = top >= .95 ? Math.max(.45, Math.min(hb, top - .4)) : top; lastHb = hb;
      const pr = r() < .5 ? within(r, .05, .2) : 0, dp = Math.min(D + pr, 1.3 * w + .25), sp = add({ a0, w, d0: dB - (dp - D), dp, h: hb, col: col({ a0, w, k, h: top }) }); C.push({ a0, w, top, hb, sp, pr: dp - D }); a0 += w; });
    for (let k = 0; k < C.length; k++) { const c = C[k], n = C[k + 1]; if (c.done) continue;
      if (n && !n.done && Math.abs(c.hb - n.hb) < .15 && Math.min(c.top, n.top) - Math.max(c.hb, n.hb) >= .4 && r() < .8) {   /* a block across the joint, bearing on both */
        const zb = (c.hb + n.hb) / 2; c.sp.h = n.sp.h = zb; const wS = within(r, .5, .8), aS = Math.max(c.a0 + .05, Math.min(c.a0 + c.w - wS * within(r, .35, .65), n.a0 + n.w - wS - .05)), hh = Math.max(.4, Math.min(.85, Math.min(c.top, n.top) - zb)), fwd = r() < .33 ? within(r, .05, .15) : -within(r, 0, .08), pr = Math.min(c.pr, n.pr);
        add({ a0: aS, w: wS, d0: dB - pr - fwd, dp: Math.min(D + pr + fwd, 1.3 * wS), z: zb, h: hh, level: 1, depth: 1, on: 2, role: 'tier', col: { row: rowB, a0: aS, w: wS, h: zb + hh } }); c.sp.covered = n.sp.covered = 1; c.done = n.done = 1; continue; }
      if (c.top - c.hb >= .4) { const hh = c.top - c.hb, fwd = r() < .33 ? within(r, .05, .15) : -within(r, 0, .1), sh2 = (r() - .5) * .1, w2 = c.w - Math.abs(sh2); c.sp.covered = 1;
        add({ a0: c.a0 + Math.max(0, sh2), w: w2, d0: dB - c.pr - fwd, dp: Math.min(D + c.pr + fwd, 1.3 * w2), z: c.hb, h: hh, level: 1, depth: 1, on: 1, role: 'tier', col: { row: rowB, a0: c.a0, w: c.w, h: c.top } }); } }
  } else if (q.tiers) {   /* blocks: a first tier, shorter tiers on it set back from the face, joints staggered; one stone may run two tiers high */
    const z1 = within(r, .6, .8), ws = cutRow(L, q.nA, []); let a0 = 0, sup = [], lo = 0, hi = L, z = z1, sb = 0, tallSp = null; const tallK = ws.length > 1 && q.tiers >= 3 && r() < .4 ? 0 : -1;
    ws.forEach((w, k) => { const hh = k === tallK ? z1 * within(r, 1.6, 2) : z1 + (r() - .5) * .03, sp = add({ a0, w, d0: dB, dp: D, h: hh, col: col({ a0, w, h: hh }) }); if (k === tallK) { tallSp = sp; if (k === 0) lo = w; else hi = L - w; } else sup.push({ a0, w, top: hh, sp }); a0 += w; });
    let prev = jointsOf(ws); const nT = Math.max(2, Math.min(q.tiers, 1 + Math.ceil((env(0) - z1) / .72))); let below = z1;
    for (let lv = 1; lv < nT; lv++) { const lastT = lv === nT - 1, span = hi - lo, len = span * (r() < .5 ? within(r, .85, 1) : within(r, .62, .84)), start = lo + (r() < .1 ? within(r, 0, span - len) : 0); sb += within(r, .1, .2);
      const n = Math.max(1, Math.round(q.nA * len / L)), w2 = cutRow(len, n, prev.map((p) => p - start)); a0 = start; const made = [], th = lastT ? Math.min(below * 1.05, Math.max(.4, env(start + len / 2) - z)) : within(r, .5, .72);
      w2.forEach((w) => { const under = sup.filter((c) => c.a0 < a0 + w - .04 && c.a0 + c.w > a0 + .04); if (!under.length || Math.min(...under.map((c) => c.top)) < z - .16) { a0 += w; return; }
        for (const u of under) u.sp.covered = 1; made.push(add({ a0, w, d0: dB + sb, dp: D - sb, z, h: th, level: lv, depth: lv, on: under.length, role: 'tier', col: { row: rowB, a0, w, h: z + th } })); a0 += w; });
      if (!made.length) break; z += th; below = th; lo = made[0].a0; hi = made[made.length - 1].a0 + made[made.length - 1].w; sup = made.map((m) => ({ a0: m.a0, w: m.w, top: z, sp: m })); prev = jointsOf(w2, start);
      /* the stone two tiers high ends level with the second tier, and the third may lie over both */
      if (lv === 1 && tallSp) { tallSp.h = z; tallSp.col.h = z; sup.push({ a0: tallSp.a0, w: tallSp.w, top: z, sp: tallSp }); sup.sort((u, v) => u.a0 - v.a0); lo = Math.min(lo, tallSp.a0); hi = Math.max(hi, tallSp.a0 + tallSp.w); tallSp = null; } }
    /* tops nothing lies on break the bed joints: each takes its own height */
    for (const sp of S) if (!sp.covered && sp.role !== 'cap' && (sp.level || sp.h < z1 * 1.2)) { sp.h = Math.max(.3, sp.h + (sp.a0 + sp.w / 2 > L * .5 ? -r() * .14 : (r() - .5) * .2)); sp.col.h = sp.z + sp.h; }
  } else if (td) {   /* dipping beds: one thick bed from the ground, one or two thick beds lying tight on it, each shorter and narrower */
    const c = col({ a0: 0, w: L, h: q.height }), z1 = q.height * within(r, .6, .7), n = q.height - z1 > .75 ? 3 : 2; let w = L, dp = D, zc = z1, ac = L / 2;
    const cut0 = L * within(r, .45, .6), b0 = add({ a0: 0, w: cut0, d0: dB, dp: D, h: z1 + td * (L / 2 - cut0 / 2), dipA: -td, role: 'bed', col: c }); add({ a0: cut0, w: L - cut0, d0: dB + within(r, 0, .08), dp: D * within(r, .85, .95), h: (z1 - td * (cut0 / 2)) * within(r, .92, 1), dipA: -td, role: 'bed', covered: 1, col: { row: rowB, a0: cut0, w: L - cut0, h: z1 } });
    let last = b0; for (let i = 1; i < n; i++) { const w2 = w * within(r, .7, .85), dp2 = dp * within(r, .82, .92), hh = i === n - 1 ? Math.min(.55, Math.max(.35, q.height - zc)) : within(r, .35, .5), a2 = w2 / 2, zb = zc + td * (ac - a2);
      last.covered = 1; const cutAt = w2 * within(r, .4, .6), zf = zb - td * (cutAt / 2 + (w2 - cutAt) / 2), hf = hh * within(r, .75, 1.1);
      last = add({ a0: 0, w: cutAt, d0: back - dp2, dp: dp2, z: zb + td * (a2 - cutAt / 2), h: hh, dipA: -td, botA: -td, level: i, depth: i, on: 1, role: 'bed', col: { row: rowB, a0: 0, w: cutAt, h: zb + hh } });
      add({ a0: cutAt, w: (w2 - cutAt) * within(r, .8, 1), d0: back - dp2 * within(r, .85, 1), dp: dp2 * .9, z: zb + td * (a2 - (cutAt + (w2 - cutAt) / 2)), h: hf, dipA: -td, botA: -td, level: i, depth: i, on: 1, role: 'bed', covered: 1, col: { row: rowB, a0: cutAt, w: w2 - cutAt, h: zb + hf } }); w = cutAt + (w2 - cutAt); dp = dp2; zc = zb + hh; ac = a2; }
  } else if (tl) {   /* leaning slabs: fat, parallel, each shorter than the one nearer the cut end; an end stone fills the space under the first */
    /* leaning slabs: three or four, tight, graded up to the cut end; the first is cut by the cut plane (its near side upright, its far side leaning) */
    const h0 = q.height, n = Math.max(2, Math.min(4, q.nA)), w1 = tl * h0 + within(r, .3, .42), wS = Math.max(.38, Math.min(.56, (L - w1) / Math.max(1, n - 1))); let a0 = w1, hh = h0 * within(r, .86, .94);
    add({ a0: 0, w: w1, d0: dB, dp: D, h: h0, batFar: tl * h0, role: 'end', col: col({ a0: 0, w: w1, h: h0 }) });
    for (let k = 1; k < n; k++) { const w = wS * within(r, .88, 1.12), pr = k === 1 && r() < .4 ? within(r, .06, .16) : 0;
      add({ a0, w, d0: dB - pr, dp: D + pr, h: hh, lean: tl, role: 'slab', col: col({ a0, w, h: hh }) }); a0 += w; hh = Math.max(.6, hh * within(r, .8, .9)); }
  } else {   /* pillars, walls, gates: columns under the envelope */
    const ws = cutRow(L, q.nA, []); let a0 = 0; const B = ws.map((w, k) => { const c = col({ a0, w, k, h: Math.max(.3, env(a0 + w / 2) * within(r, 1 - .08 * q.rough, 1 + .08 * q.rough)) }); a0 += w; return c; });
    if (q.plateau && B.length >= 2) { const n = Math.min(B.length, 2 + (r() < .45 ? 1 : 0)), k0 = Math.floor(r() * (B.length - n + 1)), run = B.slice(k0, k0 + n), m = run.reduce((a, c) => a + c.h, 0) / n, s = within(r, .2, .3), am = run.reduce((a, c) => a + c.a0 + c.w / 2, 0) / n; run.forEach((c) => { c.h = m - s * (c.a0 + c.w / 2 - am); c.plat = 1; c.platDip = s; }); }
    if (q.cleft && B.length >= 2) { let k = 0; for (let j = 1; j < B.length - 1; j++) if (B[j].h + B[j + 1].h > B[k].h + B[k + 1].h) k = j; const m = Math.max(B[k].h, B[k + 1].h); B[k].h = m; B[k + 1].h = m - .2 * (B[k].w + B[k + 1].w) / 2 * within(r, .7, 1.2); B[k].plat = B[k + 1].plat = 2; B[k].gapFar = B[k + 1].gapNear = within(r, .035, .06); }
    B.forEach((c, k) => { if (k > 0) { const p = B[k - 1]; if (!(p.plat && c.plat && p.plat === c.plat) && Math.abs(p.h - c.h) < q.stepMin) c.h = Math.max(.3, p.h + (c.h >= p.h && r() < .5 ? 1 : -1) * q.stepMin * (1 + r() * .5)); } });
    for (const c of B) c.spire = !c.plat && c.h > 1.3 * c.w && r() < .1; const farPr = q.proud && r() < .33 ? within(r, .2, .38) : 0;
    let caps = [], gateW = 0;
    if (q.gate && B.length >= 3) { const g = B[1], N = B[0], F = B[2], gw = L * q.gate, tw = (L - gw) * within(r, .45, .55); N.w = tw; g.a0 = tw; g.w = gw; F.a0 = tw + gw; F.w = L - tw - gw; g.recess = within(r, .6, .85);
      N.h = within(r, .9, 1.05); const th = Math.max(.38, Math.min(.58, q.height - N.h)); F.h = N.h; g.h = Math.min(N.h, F.h) - .02; N.spire = F.spire = false; N.plat = F.plat = 0; N.gateTop = F.gateTop = N.h; F.h = N.h; gateW = tw;
      /* the lintel course: two blocks over the opening and both piers, forward of them */
      const split = g.a0 + g.w * within(r, .35, .65), e0 = N.a0 + N.w * within(r, .05, .3), e1 = F.a0 + F.w * within(r, .7, 1);
      caps = [{ a0: e0, w: split - e0, z: N.h, h: th, o: within(r, .1, .2), on: 2 }, { a0: split, w: e1 - split, z: N.h, h: th * within(r, .82, 1.1), o: within(r, .1, .2), on: 2 }]; }
    const stepFwd = r() < .5, prs = B.map((c, k) => (stepFwd ? (B.length - 1 - k) * within(r, .03, .08) : 0) + (r() < .5 ? within(r, .1, .26) : within(r, 0, .06))); for (let k = 1; k < prs.length; k++) if (Math.abs(prs[k] - prs[k - 1]) < .06) prs[k] = prs[k - 1] + (prs[k] >= prs[k - 1] ? .07 : -.07); if (farPr) prs[prs.length - 1] = Math.max(prs[prs.length - 1], farPr);
    if (q.tallOne && B.length > 1) { const c = B[Math.floor(r() * B.length)]; if (!c.plat) { c.h += within(r, .25, .45); c.tallOne = 1; } }
    B.forEach((c, k) => { const pr = c.recess || !q.proud ? 0 : Math.max(0, Math.min(.38, prs[k], 1.3 * c.w - D)), gN = c.gapNear || 0, gF = c.gapFar || 0; c.pr = pr; const rec = c.recess ? D * c.recess : 0;
      if (q.oversail && k === B.length - 1 && c.h > .8 && !c.recess) { const zc = c.h * within(r, .45, .62), sb = q.over == null ? Math.min(.3 * D, within(r, .14, .24)) : q.over, wU = c.w * within(r, .68, .78); add({ a0: c.a0, w: c.w, d0: dB + sb, dp: D - sb, h: zc, col: c, covered: 1 }); add({ a0: c.a0, w: wU, d0: dB - .03, dp: D + .03, z: zc, h: c.h - zc, level: 1, depth: 1, on: 1, role: 'cap', jut: 1, col: c }); return; }
      /* a pillar split by a level joint: the upper block is the top of the column */
      if (q.split !== 0 && !c.recess && !c.spire && !c.plat && c.h > 1.2 && r() < (q.split == null ? .35 : q.split)) { const zc = c.h * within(r, .55, .75), sh2 = (r() - .5) * .08; add({ a0: c.a0 + gN, w: c.w - gN - gF, d0: dB - pr, dp: D + pr, h: zc, col: c, covered: 1 }); add({ a0: c.a0 + gN, w: c.w - gN - gF, d0: dB - pr + sh2, dp: D + pr - Math.max(0, sh2), z: zc, h: c.h - zc, level: 1, depth: 1, on: 1, role: 'upper', col: c }); return; }
      add({ a0: c.a0 + gN, w: c.w - gN - gF, d0: dB - pr + rec, dp: D + pr - rec, h: c.h, lean: Math.tan(q.tilt * Math.PI / 180), platDip: c.platDip, spire: c.spire, tallOne: c.tallOne, col: c, covered: c.recess || (q.gate && k !== 1) ? 1 : 0 }); });
    for (const l of caps) add({ a0: l.a0, w: l.w, d0: dB - l.o, dp: D + l.o, z: l.z, h: l.h, level: 1, depth: 1, on: l.on, role: 'cap', jut: 1, col: { row: rowB, a0: l.a0, w: l.w, h: l.z + l.h } });
    if (gateW) q.gateW = gateW;
  }
  /* what stands behind a stretch of the front: the face (smallest d) and the lowest ground-standing top there */
  const wallAt = (a0, a1) => { const b = S.filter((s) => s.level === 0 && s.role !== 'front' && s.a0 < a1 - .03 && s.a0 + s.w > a0 + .03); return b.length ? { d: Math.min(...b.map((s) => s.d0)), h: Math.min(...b.map((s) => s.h)), hi: Math.max(...b.map((s) => s.h)) } : { d: dB, h: q.height * .5, hi: q.height * .5 }; };

  lap('back', S.length);
  /* ---- 2 the front cluster at the cut end: a chunky buttress hard against the wall, a toe in front of it or beside it ---- */
  if (q.rows > 1 && !courses) { const rowF = { i: 0, t: 0, d0: dB - .5, dp: .5, len: L * .5, cols: [] }; rows.unshift(rowF); if (q.rows > 2) rows.splice(1, 0, { i: 1, t: .5, d0: dB - .5, dp: .5, len: L * .7, cols: [] });
    const wallOver = (a) => { const b = S.filter((s) => s.level === 0 && s.role !== 'front' && s.a0 <= a && s.a0 + s.w >= a); return b.length ? Math.max(...b.map((s) => s.col && s.col.h > s.h ? s.col.h : s.h)) : q.height * .6; };
    /* a main pillar applied to the wall over a joint near mid length: the third rank */
    if (q.applied && rowB.cols.length >= 2 && !tl && !td && !q.tiers && !q.gate) { const js = rowB.cols.slice(0, -1).map((c) => c.a0 + c.w).filter((j) => j > .5 && j < L - .3); if (js.length) { const j = js.reduce((u, v) => (Math.abs(v - L * .5) < Math.abs(u - L * .5) ? v : u)), w5 = within(r, .42, .6), dp5 = within(r, .35, .5), wl5 = wallAt(j - w5 / 2, j + w5 / 2), h5 = wl5.h * within(r, .7, .93);
      add({ a0: j - w5 / 2, w: w5, d0: wl5.d - dp5 + .05, dp: dp5, h: h5, role: 'wall', kind: 'pillar', applied: 1, col: { row: rows[q.rows > 2 ? 1 : 0], a0: j - w5 / 2, w: w5, h: h5 } }); } }
    const wide = tl || td || q.gate, aB = !q.gate && r() < .33 ? within(r, 0, .3 * L) : 0, w = Math.min(wide ? within(r, .5, .8) : within(r, .42, .6), q.gateW ? q.gateW - .02 : L * .55), wl = wallAt(aB, aB + w), dp = wide ? within(r, .4, .58) : within(r, .32, .46), hB = Math.max(.35, Math.min(q.front * within(r, .92, 1.08) * wl.hi, wallOver(aB + w / 2) - .2));
    const bu = add({ a0: aB, w, d0: wl.d - dp + within(r, .05, .14), dp, h: hB, role: 'front', kind: 'buttress', col: col({ a0: aB, w, h: hB }, rowF) }); let aNext = aB + w - .03;
    if (q.rows > 2) { const w2 = within(r, .38, .55), wl2 = wallAt(aNext, aNext + w2), dp2 = within(r, .34, .52), h2 = Math.max(.3, Math.min(hB * within(r, .6, 1.2), wallOver(aNext + w2 / 2) - .2)), hh = Math.abs(h2 - hB) < q.stepMin ? Math.max(.28, hB - q.stepMin * 1.4) : h2;
      add({ a0: aNext, w: w2, d0: wl2.d - dp2 + within(r, .05, .12), dp: dp2, h: hh, role: 'front', kind: 'buttress', col: col({ a0: aNext, w: w2, h: hh }, rows[1]) }); aNext += w2 - .03; }
    if (q.toe && r() < .9) { const w3 = Math.min(within(r, .45, .7), q.gateW ? q.gateW : 9), dp3 = within(r, .32, .45), h3 = Math.max(.3, Math.min(.55, hB * within(r, .4, .6))), beside = !q.gate && q.rows < 3 && r() < .4, a3 = beside ? aNext : aB + within(r, 0, .08), d3 = beside ? wallAt(a3, a3 + w3).d - dp3 + .07 : bu.d0 - dp3 + .06;
      add({ a0: a3, w: w3, d0: d3, dp: dp3, h: h3, role: 'front', kind: 'toe', col: col({ a0: a3, w: w3, h: h3 }, rowF) }); if (beside) aNext += w3; }
    else if (q.toe) { let a3 = aB + within(r, 0, .06); for (let i = 0; i < 2; i++) { const w3 = i ? within(r, .3, .38) : within(r, .4, .5), dp3 = within(r, .28, .4), h3 = i ? within(r, .2, .28) : within(r, .32, .42); add({ a0: a3, w: w3, d0: bu.d0 - dp3 + .05 - i * within(r, 0, .1), dp: dp3, h: h3, role: 'front', kind: 'toe', rubble: 1, col: col({ a0: a3, w: w3, h: h3 }, rowF) }); a3 += w3 * within(r, .85, 1); } }
    /* a small stone tucked into the corner between the cluster and the wall */
    if (q.tuck && aNext < L - .35) { const w4 = within(r, .22, .32), wl4 = wallAt(aNext, aNext + w4), h4 = hB * within(r, .3, .5); add({ a0: aNext, w: w4, d0: wl4.d - w4 + .04, dp: w4, h: h4, role: 'front', kind: 'toe', col: col({ a0: aNext, w: w4, h: h4 }, rowF) }); } }

  lap('front', S.length);
  /* ---- 3 the pile: stones laid on the tops. They add height. ---- */
  const tops = () => S.filter((s) => !s.covered && !s.spire && s.role !== 'front' && s.role !== 'slab' && !s.applied && s.w > .3 && s.dp > .3);
  const lower = (s, by) => { const d = Math.min(by, Math.max(0, s.h - .4)); s.h -= d; if (s.col && Math.abs(s.col.h - (s.z + s.h + d)) < 1e-6) s.col.h -= d; return d; };
  for (let i = 0; i < q.caps; i++) { const T = tops(); if (!T.length) break; const top = (s) => s.z + s.h;
    const pairs = []; for (const a of T) for (const b of T) if (a !== b && Math.abs(a.a0 + a.w - b.a0) < .07 && (a.platDip && b.platDip ? true : Math.abs(top(a) - top(b)) < (a.level || b.level ? .2 : .45) && !a.platDip && !b.platDip) && !a.dipA && !b.dipA && a.role !== 'cap' && b.role !== 'cap' && Math.min(a.d0 + a.dp, b.d0 + b.dp) - Math.max(a.d0, b.d0) > .3) pairs.push([a, b]);
    if (pairs.length && r() < .9) {   /* a block spanning a joint, bearing on both, flush with their face; its height comes off the two under it */
      const [a, b] = pairs[Math.floor(r() * pairs.length)], hh = within(r, .35, .6); if (!a.platDip) { const mt = Math.max(a.z + .3, b.z + .3, (top(a) + top(b)) / 2 - within(r, .15, .4) * hh); a.h = mt - a.z; b.h = mt - b.z; if (!a.level) a.col.h = a.h; if (!b.level) b.col.h = b.h; }
      const a0 = a.a0 + a.w * (1 - within(r, .45, .7)), a1 = b.a0 + b.w * within(r, .45, .7), d0 = Math.max(a.d0, b.d0) + within(r, .03, .1), dp = Math.min(.85, Math.min(a.d0 + a.dp, b.d0 + b.dp) - d0), sl = a.platDip ? -a.platDip : 0, am = (a0 + a1) / 2, z = sl ? top(a) + sl * (am - (a.a0 + a.w / 2)) : Math.max(top(a), top(b));
      add({ a0, w: a1 - a0, d0, dp, z, h: hh, dipA: sl, botA: sl, level: Math.max(a.level, b.level) + 1, depth: Math.max(a.depth, b.depth) + 1, on: 2, role: 'cap', col: { row: rowB, a0, w: a1 - a0, h: z + hh } }); a.covered = b.covered = 1; continue; }
    /* an upper block on the highest top: nearly as wide as what carries it, flush at the front, set back on one side; a second storey only on a broad block */
    const ok = T.filter((s) => (s.role !== 'cap' || s.w >= .6) && s.w > .36 && !s.tallOne); if (!ok.length) break; const sup = ok.reduce((u, v) => (top(v) > top(u) ? v : u)), slope = sup.dipA || (sup.platDip ? -sup.platDip : 0), second = sup.role === 'cap' || sup.role === 'upper';
    const hh = q.gate ? within(r, .18, .3) : second || sup.level ? within(r, .32, .5) : within(r, .4, .85), w = Math.min(sup.w * (second ? within(r, .75, .92) : within(r, .88, 1)), within(r, .5, .75)), a0 = sup.a0 + (sup.w - w) * (r() < .5 ? 0 : 1), fr = within(r, 0, .06), dp = Math.min(.85, sup.dp + fr - (second ? within(r, .05, .15) : 0));
    if (!sup.level || sup.role === 'bed') { if (sup.role !== 'bed') lower(sup, within(r, .35, .7) * hh); }
    const z = top(sup) + slope * (a0 + w / 2 - (sup.a0 + sup.w / 2)); add({ a0, w, d0: sup.d0 - fr + (second ? .06 : 0), dp: Math.min(dp, 1.2 * w), z, h: hh, dipA: slope, botA: slope, level: sup.level + 1, depth: sup.depth + 1, on: 1, role: 'cap', col: { row: rowB, a0, w, h: z + hh } }); sup.covered = 1;
    /* on a wide support, a second block beside the first */
    if (sup.w - w > .42 && !second) { const w2 = Math.min(sup.w - w - .01, within(r, .45, .7)), aa = a0 > sup.a0 + .01 ? a0 - w2 : a0 + w, h2 = Math.max(.3, hh + (r() < .5 ? -1 : 1) * within(r, .12, .25)); add({ a0: aa, w: w2, d0: sup.d0 - fr + within(r, .02, .1), dp: dp * within(r, .8, .95), z: top(sup), h: h2, level: sup.level + 1, depth: sup.depth + 1, on: 1, role: 'cap', col: { row: rowB, a0: aa, w: w2, h: top(sup) + h2 } }); } }

  lap('pile', S.length);
  /* the whole is held to its height: if the pile has carried it past the envelope by more than a tenth, everything is pressed down in proportion */
  { const zm = Math.max(...S.map((s) => s.z + s.h)), k = Math.min(1, q.height * (q.gate ? 1.12 : 1) / zm); if (k < 1) { for (const s of S) { s.z *= k; s.h *= k; } for (const rw of rows) for (const c of rw.cols) c.h *= k; } }

  /* ---- 4 shapes: each stone learns which of its top edges are exposed, then is shaped by its role ---- */
  const out = [], near = (u, v) => Math.abs(u - v) < .08;
  for (const sp of S) { const top = sp.z + sp.h, a1 = sp.a0 + sp.w, d1 = sp.d0 + sp.dp, m = Math.min(sp.w, sp.dp, sp.h), front = sp.role === 'front';
    const hid = (test) => S.some((o) => o !== sp && o.z + o.h > top - .06 && o.z < top && test(o)), ovD = (o) => Math.min(d1, o.d0 + o.dp) - Math.max(sp.d0, o.d0) > .08, ovA = (o) => Math.min(a1, o.a0 + o.w) - Math.max(sp.a0, o.a0) > .08;
    const cov = [hid((o) => near(o.a0 + o.w, sp.a0) && ovD(o)), hid((o) => near(o.a0, a1) && ovD(o)), hid((o) => near(o.d0 + o.dp, sp.d0) && ovA(o)), d1 > back - .05 || hid((o) => (near(o.d0, d1) || (o.d0 < d1 && o.d0 + o.dp > d1)) && ovA(o))];
    const bv = q.bevel * (q.bevelEach ? Math.max(.05, 1 + (r() - .5) * 2 * q.bevelEach) : 1), toe = sp.kind === 'toe';
    /* stones this one carries: no wide bevel under them, so the joint stays a crease */
    const carried = S.filter((o) => o !== sp && Math.abs(o.z - top) < .07 && ovA(o) && ovD(o)), bigCap = Math.min(.25, .5 * sp.h) / m;
    const big = !carried.length && !cov[2] && sp.dp > .34 && sp.role !== 'course' && !sp.level && !toe && r() < (front ? .3 : .25);
    const bT = cov.map((c, i) => c ? bv * within(r, .25, .45) : (i === 2 && big ? Math.min(bigCap, within(r, .3, .55) * sp.dp / m) : bv * within(r, .95, 1.5)));
    /* a shoulder cut: one exposed side edge chamfered a third to a half of the width, which makes the face a pentagon */
    const wideEnough = sp.w >= .45, side = !cov[1] ? 1 : !cov[0] && sp.a0 > .05 ? 0 : -1; if (wideEnough && !carried.length && !big && side >= 0 && sp.h > .5 && sp.role !== 'slab' && sp.role !== 'bed' && sp.role !== 'course' && r() < (sp.level ? .35 : .6)) bT[side] = Math.min(Math.min(.3, .5 * sp.h) / m, (front ? within(r, .22, .35) : within(r, .28, .45)) * sp.w / m);
    if (carried.length) { const sbk = [Math.min(...carried.map((o) => o.a0 - sp.a0)), Math.min(...carried.map((o) => a1 - (o.a0 + o.w))), Math.min(...carried.map((o) => o.d0 - sp.d0)), Math.min(...carried.map((o) => d1 - (o.d0 + o.dp)))]; for (let i = 0; i < 4; i++) bT[i] = Math.min(bT[i], Math.max(.03, .8 * sbk[i]) / m); }
    const tall = sp.h > 1.3 * sp.w, poly = wideEnough && tall && !front && r() < .3, prow = wideEnough && tall && !poly && r() < .2, vc = [bv * within(r, .7, 1.2), bv * within(r, .7, 1.2), bv * .4, bv * .4];
    if (poly) { vc[0] = within(r, .25, .38); vc[1] = within(r, .25, .38); } else if (prow) { vc[r() < .5 ? 0 : 1] = within(r, .3, .42); }
    /* which sides stand clear above the neighbour: only those are battered, so joints between neighbours stay closed */
    const clear = (test) => !S.some((o) => o !== sp && o.z + o.h > top - .2 && o.z < top && test(o)), clN = sp.a0 > .05 && clear((o) => near(o.a0 + o.w, sp.a0) && ovD(o)), clF = clear((o) => near(o.a0, a1) && ovD(o));
    let bat = [0, 0, 0, 0], tA = 1, tD = 1, dipA = sp.dipA || 0, dipD = 0, leanA = 0, kind = sp.kind;
    if (toe) { const bq = sp.rubble ? .12 : within(r, .15, .25); bat = [0, clF ? Math.min(sp.w * .25, sp.h * bq) : 0, Math.min(sp.dp * .3, sp.h * bq), 0]; dipD = within(r, 0, .12); if (!sp.rubble && clF && r() < .5) dipA = -within(r, .04, .12); }
    else if (front) { bat = [0, clF ? Math.min(sp.w * .25, sp.h * within(r, .1, .2)) : 0, Math.min(sp.dp * .3, sp.h * within(r, .1, .2)), 0]; const u = r(), wedge = sp.h >= .5 && !big;
      if (wedge && u < .7) dipA = (clF ? -1 : 1) * within(r, .3, .6); else if (wedge && u < .85) dipD = within(r, .3, .6); else dipD = within(r, 0, .1); }
    else if (sp.role === 'end') { bat = [0, sp.batFar, sp.h * within(r, .03, .08), 0]; dipA = within(r, .05, .2); dipD = within(r, 0, .1); }
    else if (sp.role === 'slab') { leanA = -sp.lean; dipA = sp.lean * within(r, .5, 1); bat[2] = sp.h * within(r, .02, .06); }
    else if (sp.level > 0) { dipD = within(r, -.03, .1); if (!sp.dipA && sp.role !== 'bed') dipA = (clF && !clN ? -1 : clN && !clF ? 1 : (r() < .5 ? -1 : 1)) * within(r, .05, .2) * (carried.length ? .3 : 1); if (!cov[2]) bat[2] = Math.min(sp.dp * .25, sp.h * within(r, .08, .18)); if (clN) bat[0] = Math.min(sp.w * .2, sp.h * within(r, .06, .16)); if (clF) bat[1] = Math.min(sp.w * .2, sp.h * within(r, .06, .16)); }
    else { if (sp.spire && clN && clF) { tA = within(r, .55, .7); tD = within(r, .7, .85); } else { if (clN) bat[0] = Math.min(sp.w * .22, sp.h * within(r, .06, .14)); if (clF) bat[1] = Math.min(sp.w * .22, sp.h * within(r, .06, .14)); }
      bat[2] = Math.min(sp.dp * .25, sp.h * within(r, .08, .16)); dipD = sp.role === 'bed' ? 0 : within(r, -.02, .12) * q.rough; if (!sp.dipA) dipA = sp.platDip ? -sp.platDip : (clF && !clN ? -1 : clN && !clF ? 1 : (r() < .5 ? -1 : 1)) * (clF || clN ? within(r, .1, .32) : within(r, 0, .12)) * q.rough * (carried.length ? .3 : 1); }
    if (sp.lean && sp.role !== 'slab') leanA = -sp.lean;   /* a column of a tilted back mass leans toward the cut end */
    /* a stone that carries another has a flat top under it (or the slope its plateau shares), so what it carries bears on it all over */
    if (carried.length) { dipD = 0; if (!sp.dipA && !sp.platDip) dipA = 0; }
    /* a side is battered only above the top of the stone beside it, so the joint between two neighbours stays closed up to the lower one's top */
    if (sp.role !== 'end' && sp.role !== 'slab' && !front) { const nbTop = (test) => Math.max(0, ...S.filter((o) => o !== sp && !o.part && o.z < top && test(o)).map((o) => o.z + o.h)), fr2 = (t2) => Math.max(0, Math.min(1, (top - t2) / sp.h)); bat[0] *= fr2(nbTop((o) => near(o.a0 + o.w, sp.a0) && ovD(o))); bat[1] *= fr2(nbTop((o) => near(o.a0, a1) && ovD(o))); }
    tA *= q.taper / .95; if (!kind) kind = sp.role === 'cap' && (sp.on >= 2 || sp.jut) ? 'cap' : sp.role === 'slab' ? 'lean' : sp.h > 1.35 * Math.max(sp.w, sp.dp) ? 'pillar' : sp.h < .7 * Math.max(sp.w, sp.dp) ? 'slab' : 'block';
    const g = sp.part ? 0 : sp.level ? .004 : .006, st = { w: Math.max(.08, sp.w - g - q.gap), dp: Math.max(.08, sp.dp - g - q.gap), h: sp.h, a: sp.a0 + sp.w / 2 + (r() - .5) * q.offset * .3, d: sp.d0 + sp.dp / 2 + (r() - .5) * q.offset * .3, z: sp.z,
      bevel: bv, bevelT: bT, bevelVC: vc, crease: sp.w > .5 && !cov[2] && sp.role !== 'course' && r() < .4 ? within(r, .03, .06) : 0, taperA: tA, taperD: tD, batter: bat, dipA, dipD, botA: sp.botA || 0,
      leanA: leanA + (q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0), leanD: q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0,
      cut: (wideEnough && !sp.level && !front && !carried.length && m > .32 && !big && r() < q.cutCorner) ? within(r, .26, .4) : 0, cutAt: r() < .6 ? 0 : 1, 
      yaw: (sp.level || front || sp.role === 'bed' || sp.role === 'end' || sp.role === 'slab' ? 0 : (r() - .5) * q.offset * 1.4) + (q.yawEach ? (r() - .5) * 2 * q.yawEach * Math.PI / 180 : 0), seed: Math.floor(r() * 1e6), kind, col: sp.col, level: sp.level, on: sp.on, depth: sp.depth, part: sp.part ? true : undefined };
    out.push(st);
    /* a stepped stone: a shoulder on the top, flush with the back and one side, the two read as one stone */
    if ((front || kind === 'block') && !sp.level && sp.h > .4 && sp.w > .4 && Math.abs(dipD) < .2 && Math.abs(dipA) < .15 && r() < .16) { const w2 = sp.w * within(r, .45, .65), dp2 = sp.dp * within(r, .5, .7), h2 = Math.min(.3, sp.h * within(r, .25, .45)), nearSide = r() < .5;
      out.push(Object.assign({}, st, { part: true, w: w2, dp: dp2, h: h2 + .02, a: nearSide ? sp.a0 + w2 / 2 + .01 : a1 - w2 / 2 - .01, d: d1 - dp2 / 2 - .01, z: top - .02, bevelT: [bv, bv, bv * 1.3, bv * .4], bevelVC: null, bevelV: bv * .7, batter: null, taperA: .97, taperD: .97, dipA: 0, dipD: within(r, 0, .08), cut: 0, yaw: st.yaw, seed: st.seed + 1 })); st.cut = 0; } }
  lap('shape', out.length);
  if (!q.noBear) bear(out);
  lap('bear', out.length);
  return { q, rows, stones: out, specs: S, marks };
}
/* the count is held to what the sheets show: at most eleven stones */
function formation(over = {}) { let f = formationOnce(over), o = Object.assign({}, over), n = 0; const count = (g) => g.stones.filter((s) => !s.part).length;
  while (count(f) > 11 && n++ < 5) { o = Object.assign({}, o, f.q.caps > 1 ? { caps: f.q.caps - 1 } : f.q.tuck || f.q.applied ? { tuck: 0, applied: 0 } : { nA: Math.max(1, f.q.nA - 1), rows: f.q.cls === 'courses' ? Math.max(3, f.q.rows - 1) : f.q.rows }); f = formationOnce(o); } return f; }
/* a formation's meshes: the stones with those on a fused joint (chapter 8's rule) grown toward each other, or with `apart` the stones as built */
const formMeshes = (f, apart) => apart ? f.stones.map(stoneMesh) : grownStones(f).map(stoneMesh);

/* Scale 1: draw a whole formation's numbers from the proposed ranges. */
function drawParams(seed, want) {
  const r = rng(seed * 31 + 5), u = r() * 52, cls = want && want !== 'any' ? want : (u < 22 ? 'pillars' : u < 32 ? 'blocks' : u < 42 ? 'leaners' : u < 49 ? 'courses' : 'gates');
  const o = { cls: cls === 'ramp' ? 'leaners' : cls === 'wall' ? 'pillars' : cls, preset: cls, seed, height: mid(r, 1.4, 1.64), length: within(r, 1.25, 1.7), bevel: within(r, .14, .24), fall: within(r, .35, .75), peak: r() < .1 ? within(r, .25, .45) : within(r, 0, .18), sizeVar: .6,
    front: within(r, .6, .85), rows: r() < .25 ? 1 : r() < .4 ? 3 : 2, tuck: r() < .05 ? 1 : 0, toe: r() < .65 };
  const pile = (p0, p1, p2) => { const v = r(); return v < p0 ? 0 : v < p0 + p1 ? 1 : v < p0 + p1 + p2 ? 2 : 3; };
  if (cls === 'pillars' || cls === 'wall') { const wall = cls === 'wall' || r() < .4; Object.assign(o, { nA: (o.length < 1.45 ? 2 : 3) + (r() < .5 ? 1 : 0), sizeVar: .5, stepMin: .17, fall: within(r, .15, .42), tallOne: r() < .17 ? 1 : 0, depth: within(r, .42, .55), plateau: wall || r() < .25 ? 1 : 0, cleft: r() < .12 ? 1 : 0, oversail: r() < .22 ? 1 : 0, applied: r() < .4 ? 1 : 0, caps: pile(.1, .45, .45), split: .1 }); if (wall) Object.assign(o, { fall: within(r, .1, .3), peak: within(r, .05, .35) }); else if (r() < .2) Object.assign(o, { fall: within(r, 0, .15), peak: within(r, .15, .45) }); }
  if (cls === 'blocks') Object.assign(o, { nA: 2 + (r() < .65 ? 1 : 0), stacks: r() < .8 ? 1 : 0, tiers: 2 + (r() < .75 ? 1 : 0), depth: within(r, .6, .75), fall: within(r, .15, .45), peak: r() < .33 ? within(r, .1, .3) : within(r, 0, .1), sizeVar: .5, caps: pile(.35, .5, .15), rows: r() < .2 ? 1 : r() < .85 ? 2 : 3 });
  if (cls === 'leaners') { if (r() < .35) Object.assign(o, { preset: 'ramp', dip: within(r, 14, 24), fall: within(r, .05, .2), depth: within(r, .62, .8), caps: pile(.3, .5, .2) }); else Object.assign(o, { lean: within(r, 15, 25), height: mid(r, 1.4, 1.65), nA: r() < .5 ? 3 : 4, length: within(r, 1.3, 1.6), caps: r() < .6 ? 1 : 0, lean: within(r, 15, 24), depth: within(r, .5, .68), front: within(r, .45, .65), fall: within(r, .3, .55) }); }
  if (cls === 'ramp') Object.assign(o, { dip: within(r, 14, 24), fall: within(r, .05, .2), depth: within(r, .62, .8), caps: pile(.3, .5, .2) });
  if (cls === 'courses') Object.assign(o, { height: mid(r, 1.45, 1.75), rows: 3 + (r() < .7 ? 1 : 0), nA: 1 + Math.floor(r() * 2.5), depth: within(r, 1, 1.25), length: within(r, 1.35, 1.65), caps: 0, sizeVar: .3, blockCourse: r() < .67 ? 1 : 0 });
  if (cls === 'gates') Object.assign(o, { gate: within(r, .3, .38), height: within(r, 1.36, 1.52), caps: r() < .33 ? 1 : 0, rows: 2, fall: within(r, .2, .4), peak: within(r, .05, .25), depth: within(r, .65, .8), length: within(r, 1.4, 1.7) });
  /* a budget for extras: a second front block, a tucked stone and piled stones beyond the first come to two at most */
  while ((o.rows > 2 ? 1 : 0) + (o.tuck ? 1 : 0) + Math.max(0, (o.caps || 0) - 1) > 2) { if (o.tuck) o.tuck = 0; else if (o.rows > 2) o.rows = 2; else o.caps--; }
  return o;
}
/* What a generated formation would measure if it were traced like a reference: each stone as seen from the references' camera, in fh,
   and the relations between touching stones measured the way analyze.py measures them on the traces. */
function seen(f, az = 45) {
  const cam = cameraFor('ref', 360, 300, { az }), pr = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) * 180, (1 - v.y) * 150]; };
  let X0 = 1e9, X1 = -1e9, Y0 = 1e9, Y1 = -1e9; const fig = pr(1.25, .45, 0), figTop = pr(1.25, .45, 1), fh = Math.hypot(fig[0] - figTop[0], fig[1] - figTop[1]);
  const stones = f.stones.filter((o) => !o.part).map((o) => { const P = stonePoints(o).map((p) => pr(...p)); const x0 = Math.min(...P.map((p) => p[0])), x1 = Math.max(...P.map((p) => p[0])), y0 = Math.min(...P.map((p) => p[1])), y1 = Math.max(...P.map((p) => p[1]));
    X0 = Math.min(X0, x0); X1 = Math.max(X1, x1); Y0 = Math.min(Y0, y0); Y1 = Math.max(Y1, y1);
    const b = pr(o.a, o.d - o.dp / 2, o.z), t = pr(o.a + o.leanA * o.h, o.d - o.dp / 2, o.z + o.h), hiP = P.reduce((u, v) => (v[1] < u[1] ? v : u)); return { wb: (x1 - x0) / fh, hb: Math.hypot(b[0] - t[0], b[1] - t[1]) / fh, kind: o.kind, ground: !o.level, x0, x1, y0, y1, xTop: hiP[0], o, P, far: cam.position.distanceTo(new THREE.Vector3(o.a, o.z + o.h / 2, o.d)) }; });
  /* what shows of each stone once nearer stones hide part of it, as a traced outline would have it: the stones are painted far to near on a coarse grid */
  { const GX = 110, GY = 92, grid = new Int16Array(GX * GY).fill(-1), order = stones.map((s2, i) => i).sort((u, v) => stones[v].far - stones[u].far);
    const chain = (pp) => { pp = pp.slice().sort((u, v) => u[0] - v[0] || u[1] - v[1]); const cr = (o2, a2, b2) => (a2[0] - o2[0]) * (b2[1] - o2[1]) - (a2[1] - o2[1]) * (b2[0] - o2[0]), lo = [], up = []; for (const p of pp) { while (lo.length >= 2 && cr(lo[lo.length - 2], lo[lo.length - 1], p) <= 0) lo.pop(); lo.push(p); } for (const p of pp.reverse()) { while (up.length >= 2 && cr(up[up.length - 2], up[up.length - 1], p) <= 0) up.pop(); up.push(p); } return lo.slice(0, -1).concat(up.slice(0, -1)); };
    const sx = (X1 - X0) / GX, sy = (Y1 - Y0) / GY;
    for (const i of order) { const s2 = stones[i], H2 = chain(s2.P), n = H2.length; for (let gy = Math.max(0, Math.floor((s2.y0 - Y0) / sy)); gy < Math.min(GY, Math.ceil((s2.y1 - Y0) / sy)); gy++) for (let gx = Math.max(0, Math.floor((s2.x0 - X0) / sx)); gx < Math.min(GX, Math.ceil((s2.x1 - X0) / sx)); gx++) { const px = X0 + (gx + .5) * sx, py = Y0 + (gy + .5) * sy; let inside = true; for (let k = 0; k < n && inside; k++) { const a2 = H2[k], b2 = H2[(k + 1) % n]; if ((b2[0] - a2[0]) * (py - a2[1]) - (b2[1] - a2[1]) * (px - a2[0]) < 0) inside = false; } if (inside) grid[gy * GX + gx] = i; } }
    const bx = stones.map(() => [GX, -1, GY, -1]); for (let gy = 0; gy < GY; gy++) for (let gx = 0; gx < GX; gx++) { const i = grid[gy * GX + gx]; if (i < 0) continue; const b2 = bx[i]; if (gx < b2[0]) b2[0] = gx; if (gx > b2[1]) b2[1] = gx; if (gy < b2[2]) b2[2] = gy; if (gy > b2[3]) b2[3] = gy; }
    stones.forEach((s2, i) => { const b2 = bx[i]; s2.wv = b2[1] < 0 ? 0 : (b2[1] - b2[0] + 1) * sx / fh; s2.hv = b2[1] < 0 ? 0 : (b2[3] - b2[2] + 1) * sy / fh; delete s2.P; }); }
  const W = (X1 - X0) / fh, H = (Y1 - Y0) / fh, pairs = [];
  for (let i = 0; i < stones.length; i++) for (let j = i + 1; j < stones.length; j++) { const A = stones[i].o, Bq = stones[j].o, gapA = Math.abs(A.a - Bq.a) - (A.w + Bq.w) / 2, gapD = Math.abs(A.d - Bq.d) - (A.dp + Bq.dp) / 2, zo = Math.min(A.z + A.h, Bq.z + Bq.h) - Math.max(A.z, Bq.z);
    if (zo < .1 || !((gapA < .06 && gapD < -.08) || (gapD < .06 && gapA < -.08))) continue; const lo = stones[i].y0 > stones[j].y0 ? stones[i] : stones[j], hi = lo === stones[i] ? stones[j] : stones[i];
    pairs.push({ dtop: Math.abs(stones[i].y0 - stones[j].y0) / fh, lowfront: Math.abs(lo.y1 - hi.y1) / fh > .04 ? lo.y1 > hi.y1 : null }); }
  const near = stones.reduce((a, b) => (b.y1 > a.y1 ? b : a)), top = stones.reduce((a, b) => (b.y0 < a.y0 ? b : a)), ws = stones.map((s2) => s2.wb);
  const leftTop = Math.min(...stones.filter((s2) => s2.x0 < X0 + (X1 - X0) * .12).map((s2) => s2.y0));
  return { stones, pairs, W, H, n: stones.length, footshare: (Y1 - near.y0) / (Y1 - Y0), peak: (top.xTop - X0) / (X1 - X0), wratio: Math.max(...ws) / Math.min(...ws), wratioV: (() => { const v = stones.map((s2) => s2.wv).filter((x) => x > .12); return v.length > 1 ? Math.max(...v) / Math.min(...v) : 1; })(), farend: (Y1 - leftTop) / (Y1 - Y0),
    g80: stones.filter((s2) => s2.ground && (Y1 - s2.y0) / (Y1 - Y0) >= .8).length, topRests: !top.ground,
    jut: f.stones.some((o) => o.kind === 'cap'), rest: f.stones.filter((o) => !o.part && o.level > 0).length, two: f.stones.filter((o) => !o.part && o.on >= 2).length, real: f.stones.filter((o) => !o.part).length, deep: Math.max(...f.stones.map((o) => o.depth || 0)) };
}
