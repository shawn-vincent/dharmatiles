/* ---------- the formation (proposed; chapter 11 states it) ----------
   Rows of columns under an envelope, then a few productions that the sheets show on top of that: a plateau (neighbours sharing a top),
   a cleft, a stone resting set back on others, an upper block oversailing a set-back wall, tiers with staggered joints, a leaning set
   beside an upright one, a gate. */
const FORM = { seed: 1, cls: 'pillars',
  length: 1.45, depth: .85, height: 1.75,   /* fh: along the cliff, front to back, the highest top */
  nA: 3, rows: 2,          /* stones along the back row; rows front to back */
  frontLen: .55,           /* the front row's length as a share of the back row's */
  front: .34,              /* the front row's height as a share of the back row's */
  fall: .35,               /* how far the tops fall from the peak to the far end, as a share of the height (0 = a wall) */
  peak: .12,               /* where the peak is along the cliff (0 = at the cut end, .5 = mid length) */
  sizeVar: .55,            /* how unequal the widths are: each within this share of the mean */
  stagger: .5,             /* joints of one row or tier against the next: 0 in line, .5 half a stone off */
  stepMin: .13,            /* least step between neighbouring tops that are not on a plateau, fh */
  plateau: 0,              /* 1: a run of two or three neighbours in the back row shares one top plane, which falls toward the far end by 7 to 14 degrees (level as the references' camera sees it) */
  cleft: 0,                /* 1: an open joint between the two tallest neighbours */
  rest: 0,                 /* stones resting set back on the back row's tops: how many (0-2) */
  oversail: 0, over: null, /* 1: an upper block at the far end oversails a set-back lower wall; over: by how much, fh (drawn 0.22-0.36 when not given) */
  tiers: 0,                /* 2-3: the back rows are built in level tiers with staggered joints (blocks) */
  topLen: 1,               /* the last row's length as a share of the full length (a stair's small top block) */
  lean: 0, closer: 1,      /* degrees: the far joints lean this far, fanning down toward the cut end; closer: an upright stone closes the cut end */
  dip: 0,                  /* degrees: the back row is beds that dip this far */
  taper: .95, bevel: .16, jit: .02, offset: .03, cutCorner: .25,
  gate: false, toe: true, rough: 1, minW: 1,   /* minW 0 lets stones be thinner than 0.4 fh on average, for the experiments */
  leanEach: 0, yawEach: 0, bevelEach: 0, gap: 0, stepAlong: null };   /* disorder, for the experiments */
const PRESET = {
  pillars: { nA: 3, rows: 2 },
  wall: { cls: 'pillars', nA: 3, rows: 2, fall: .06, plateau: 1, front: .3, peak: .3 },
  blocks: { nA: 3, rows: 2, tiers: 2, front: .36, frontLen: .5, fall: .35, sizeVar: .35 },
  courses: { nA: 1, rows: 4, front: .22, fall: .05, depth: 1.05, length: 1.5, frontLen: .95, sizeVar: .2, toe: false, stepMin: .2, stepAlong: .05, peak: 0, cutCorner: .12, topLen: .4 },
  leaners: { nA: 4, rows: 2, lean: 20, front: .36, fall: .35, depth: .75 },
  ramp: { cls: 'leaners', nA: 1, rows: 2, dip: 27, front: .4, fall: .05, frontLen: .55, depth: .85, sizeVar: .3 },
  gates: { nA: 3, rows: 2, gate: true, fall: .08, depth: .8, peak: .35, frontLen: .35, front: .3, height: 1.75 } };
function formationOnce(over = {}) {
  const q = Object.assign({}, FORM, PRESET[over.preset || over.cls] || {}, over), r = rng(q.seed * 104729 + 7), out = [];
  const td = Math.tan(q.dip * Math.PI / 180), back = PLATE, L = q.length;
  /* 1 rows: front rows are thinner, shorter and lower */
  const wts = Array.from({ length: q.rows }, (_, i) => lerp(.72, 1.15, q.rows > 1 ? i / (q.rows - 1) : 1) * within(r, .9, 1.1)), wsum = wts.reduce((a, b) => a + b, 0);
  let d0 = back - q.depth; const rows = [];
  for (let i = 0; i < q.rows; i++) { const t = q.rows > 1 ? i / (q.rows - 1) : 1, dp = q.depth * wts[i] / wsum; let len = L * lerp(q.frontLen, 1, Math.pow(t, .8)); if (i === q.rows - 1 && q.rows > 2) len = L * q.topLen; rows.push({ i, t, d0, dp, len }); d0 += dp; }
  /* 2 columns: unequal widths, no slivers; joints kept off the joints of the row behind */
  const cutRow = (len, n, avoid = []) => { let best = null; for (let tries = 0; tries < 12; tries++) { let ws = Array.from({ length: n }, () => within(r, 1 - q.sizeVar, 1 + q.sizeVar)); const s0 = ws.reduce((a, b) => a + b, 0); ws = ws.map((v) => v / s0 * len);
      const joints = []; let a = 0; for (const w of ws.slice(0, -1)) { a += w; joints.push(a); } const clear = Math.min(1, ...joints.flatMap((j) => avoid.map((k) => Math.abs(j - k)))), thin = Math.min(...ws);
      const score = Math.min(clear / (.12 + .2 * q.stagger), 1) + Math.min(thin / .3, 1); if (!best || score > best.score) best = { ws, score }; if (score >= 2) break; } return best.ws; };
  const env = (a, t) => { const p = q.peak * L, far = a > p ? (a - p) / Math.max(L - p, .01) : (p - a) / Math.max(L, .01) * .5; return q.height * lerp(q.front, 1, t) * (1 - q.fall * far); };
  for (let i = rows.length - 1; i >= 0; i--) { const row = rows[i], n = Math.max(1, Math.min(Math.round(q.nA * row.len / L), q.minW === 0 ? 99 : Math.floor(row.len / .4))), behind = i < rows.length - 1 ? rows[i + 1].cols.slice(0, -1).map((c) => c.a0 + c.w) : [];
    const ws = q.stagger === 0 ? Array(n).fill(row.len / n) : cutRow(row.len, n, behind); let a0 = 0; row.cols = ws.map((w, k) => { const c = { a0, w, k, row }; a0 += w; return c; }); }
  /* 3 heights under the envelope */
  for (const row of rows) for (const c of row.cols) c.h = Math.max(.18, env(c.a0 + c.w / 2, row.t) * within(r, 1 - .08 * q.rough, 1 + .08 * q.rough));
  const br = rows[rows.length - 1], B = br.cols;
  /* 4 a plateau: a run of neighbours in the back row shares one top; a cleft: the two tallest neighbours stand level with an open joint */
  if (q.plateau && B.length >= 2) { const n = Math.min(B.length, 2 + (r() < .4 ? 1 : 0)), k0 = Math.floor(r() * (B.length - n + 1)), run = B.slice(k0, k0 + n), m = run.reduce((a, c) => a + c.h, 0) / n, s = within(r, .2, .3), am = run.reduce((a, c) => a + c.a0 + c.w / 2, 0) / n; run.forEach((c) => { c.h = m - s * (c.a0 + c.w / 2 - am); c.plat = 1; c.platDip = s; }); }
  if (q.cleft && B.length >= 2) { let k = 0; for (let j = 1; j < B.length - 1; j++) if (B[j].h + B[j + 1].h > B[k].h + B[k + 1].h) k = j; const m = Math.max(B[k].h, B[k + 1].h); B[k].h = m; B[k + 1].h = m - .2 * (B[k].w + B[k + 1].w) / 2 * within(r, .7, 1.2); B[k].plat = B[k + 1].plat = 2; B[k].gapFar = B[k + 1].gapNear = within(r, .035, .06); }
  /* 5 the two rules: neighbours not on a plateau stand at least stepMin apart; a stone is lower than what stands behind it */
  for (let i = rows.length - 1; i >= 0; i--) { const row = rows[i];
    row.cols.forEach((c, k) => {
      if (i < rows.length - 1) { const lowest = Math.min(...rows[i + 1].cols.filter((b) => b.a0 < c.a0 + c.w - .03 && b.a0 + b.w > c.a0 + .03).map((b) => b.h)); if (isFinite(lowest)) c.h = Math.min(c.h, lowest - Math.max(.1, q.stepMin)); }
      if (k > 0) { const p = row.cols[k - 1], sm = q.stepAlong == null ? q.stepMin : q.stepAlong; if (!(p.plat && c.plat && p.plat === c.plat) && Math.abs(p.h - c.h) < sm) c.h = p.h + (c.h >= p.h && r() < .5 && i === rows.length - 1 ? 1 : -1) * sm * (1 + r() * .5); }
      c.h = Math.max(c.h, .18); }); }
  /* 6 the leaning set: joints of the back row lean, most at the far end, fanning down toward the cut end; the cut face and the feet stay put */
  if (q.lean) { const n = B.length, j0 = q.closer && n > 2 ? 1 : 0, tmax = Math.tan(q.lean * Math.PI / 180);
    B.forEach((c, k) => { const f = (j) => j <= j0 ? 0 : tmax * lerp(.4, 1, (j - j0) / Math.max(1, n - j0)); c.tN = k === j0 && j0 > 0 ? f(j0 + 1) * .85 : f(k); c.tF = f(k + 1); if (k < j0) c.tN = c.tF = 0; });
    if (j0 > 0) { const sh = B[j0].tN * Math.min(B[j0 - 1].h, B[j0].h) * .9; for (let k = j0; k < n; k++) B[k].a0 += sh; } }
  /* a stone from a column (or from any box in that column's row) */
  const shape = (c, z, hh, x = {}) => { const row = c.row, isTop = x.top !== false, rec = c.recess || 0, tN = c.tN || 0, tF = c.tF || 0;
    const gN = c.gapNear || 0, gF = c.gapFar || 0, w0 = (x.w || c.w) - gN - gF, aN = (x.a0 == null ? c.a0 : x.a0) + gN;
    const wB = w0 - z * (tF - tN), wT = w0 - (z + hh) * (tF - tN), aC = aN + w0 / 2 - z * (tN + tF) / 2;
    const dp = (x.dp || row.dp * (1 - rec)), dC = x.d == null ? row.d0 + row.dp * rec + dp / 2 : x.d, lv = x.level || 0, m = Math.min(wB, dp, hh);
    return { w: Math.max(.08, wB - .012 - q.gap), dp: dp - .012 - q.gap, h: hh, a: aC + (r() - .5) * q.offset * .3, d: dC + (r() - .5) * q.offset * .5, z,
      bevel: q.bevel * within(r, .85, 1.2) * (q.bevelEach ? Math.max(.05, 1 + (r() - .5) * 2 * q.bevelEach) : 1), bevelB: lv ? q.bevel * .45 : 0,
      taperA: Math.max(.5, wT / wB) * (x.taper || q.taper) * within(r, .98, 1.02), taperD: Math.min(1, (x.taper || q.taper) * within(r, .99, 1.04)),
      dipA: -(x.dip == null ? td : x.dip) - (tN + tF) * .28 + (isTop ? (c.platDip ? -c.platDip : -within(r, 0, .08) * q.rough) : 0), dipD: isTop ? within(r, -.02, .1) * q.rough : 0, botA: lv ? -(x.dip == null ? td : x.dip) : 0,
      leanA: -(tN + tF) / 2 + (q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0), leanD: q.leanEach ? Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180) : 0,
      cut: (isTop && !lv && m > .3 && r() < q.cutCorner) ? within(r, .28, .42) : 0, cutAt: r() < .6 ? 0 : 1, jit: lv ? q.jit * .6 : q.jit,
      yaw: (r() - .5) * q.offset * 1.4 + (q.yawEach ? (r() - .5) * 2 * q.yawEach * Math.PI / 180 : 0), seed: Math.floor(r() * 1e6),
      kind: x.kind || (hh > 1.35 * Math.max(wB, dp) ? 'pillar' : hh < .7 * Math.max(wB, dp) ? 'slab' : 'block'), col: c, level: lv }; };
  /* 7 a gate: one inner column of the back row is a set-back wall; a cap from each pier meets over it, the piers unequal */
  let caps = [];
  if (q.gate && B.length >= 3) { const g = B[1], N = B[0], F = B[2]; g.recess = .55; const thN = within(r, .4, .6), thF = within(r, .34, .5);
    N.h = Math.max(N.h, q.height * .9) - thN; F.h = N.h * within(r, .8, .93); g.h = Math.min(N.h, F.h) - .03; const split = within(r, .4, .65);
    caps = [{ a0: N.a0 + N.w * .25, w: N.w * .75 + g.w * split, z: N.h, h: thN, o: within(r, .06, .18) }, { a0: g.a0 + g.w * split, w: g.w * (1 - split) + F.w * within(r, .7, .95), z: F.h, h: N.h + thN - F.h - within(r, .2, .35), o: within(r, .1, .25) }]; }
  /* 8 stones */
  for (const row of rows) { const isBack = row === br;
    if (q.tiers && row.i > 0) {   /* tiers: a level first tier, then shorter tiers on it with their own joints, each set back a little */
      const top = Math.max(...row.cols.map((c) => c.h)), z1 = Math.min(top * .5, within(r, .5, .7)); let prev = row.cols.slice(0, -1).map((c) => c.a0 + c.w), z = 0, len = row.len, lv = 0, sup = [];
      for (const c of row.cols) { const hh = Math.min(c.h, z1 * within(r, .97, 1.03)); out.push(shape(c, 0, hh, { top: c.h <= z1 * 1.05 })); c.t1 = hh; sup.push({ a0: c.a0, w: c.w, top: hh }); }
      z = z1; lv = 1;
      while (z < top - .3 && lv < q.tiers + 1) { len *= within(r, .6, .85); const n = Math.max(1, Math.round(row.cols.length * len / row.len)), ws = cutRow(len, n, prev), lastTier = (lv === q.tiers) || top - z < .9; let a0 = 0; const made = [];
        ws.forEach((w, k) => { const under = sup.filter((c) => c.a0 < a0 + w - .02 && c.a0 + c.w > a0 + .02); if (!under.length || Math.min(...under.map((c) => c.top)) < z - .08) { a0 += w; return; }
          let hh = lastTier ? Math.max(.3, env(a0 + w / 2, row.t) * within(r, .93, 1.05) - z) : within(r, .45, .62); if (made.length && Math.abs(made[made.length - 1].h - hh) < q.stepMin && lastTier) hh -= q.stepMin;
          hh = Math.max(.28, Math.min(hh, 1)); const sb = .05 * lv * within(r, .6, 1.4); made.push(Object.assign(shape({ row, a0, w, h: z + hh }, z, hh, { level: lv, dp: row.dp - sb, d: row.d0 + sb + (row.dp - sb) / 2, top: lastTier }), { sa0: a0, sw: w })); a0 += w; });
        out.push(...made); prev = []; { let a = 0; for (const w of ws.slice(0, -1)) { a += w; prev.push(a); } } if (lastTier || !made.length) break; z += Math.min(...made.map((s2) => s2.h)); sup = made.map((s2) => ({ a0: s2.sa0, w: s2.sw, top: z })); for (const s2 of made) s2.h = z - s2.z; lv++; }
      continue; }
    for (const c of row.cols) {
      if (q.toe && row.i === 0 && q.rows > 1) { out.push(shape(c, 0, c.h, { kind: 'toe', taper: q.taper * .82 })); continue; }
      if (td && isBack) {   /* dipping beds: one thick bed and one or two thinner, shorter ones on it */
        const n = c.h > 1.3 && r() < .6 ? 3 : 2, zs = [0, c.h * within(r, .5, .62)]; if (n === 3) zs.push(zs[1] + (c.h - zs[1]) * within(r, .45, .6)); zs.push(c.h);
        for (let i = 0; i < n; i++) { const sh = 1 - .17 * i; out.push(shape(c, zs[i] + (i ? td * c.w * (1 - sh) * .5 : 0), zs[i + 1] - zs[i], { level: i, w: c.w * sh, top: i === n - 1 })); } continue; }
      if (q.oversail && isBack && c === B[B.length - 1] && c.h > .8 && !c.recess && !(c.tF || c.tN)) {   /* an upper block oversails a set-back wall */
        const zc = c.h * within(r, .45, .62), sb = q.over == null ? within(r, .22, .36) : q.over; out.push(shape(Object.assign({}, c, { recess: sb / row.dp }), 0, zc, { top: false })); out.push(shape(c, zc, c.h - zc, { level: 1, kind: 'cap', dp: row.dp + .04, d: row.d0 - .04 + (row.dp + .04) / 2 })); continue; }
      out.push(shape(c, 0, c.h, {})); } }
  for (const l of caps) out.push(shape({ row: br, a0: l.a0, w: l.w, h: l.z + l.h }, l.z, l.h, { level: 1, kind: 'cap', dp: br.dp + l.o, d: br.d0 - l.o + (br.dp + l.o) / 2, taper: 1 }));
  /* 9 stones that rest: smaller, set back from the face, over one joint where the two tops are level, else on one top */
  for (let i = 0; i < q.rest && !q.tiers && !td && !q.gate; i++) { const ok = B.map((c, k) => k).filter((k) => !B[k].used && !(q.oversail && k === B.length - 1) && !(B[k].tF || B[k].tN)); if (!ok.length) break;
    const pairK = ok.find((k) => B[k + 1] && ok.includes(k + 1) && Math.abs(B[k].h - B[k + 1].h) < .03), k = pairK != null ? pairK : ok[Math.floor(r() * ok.length)], c = B[k], two = pairK != null, n2 = two ? B[k + 1] : null;
    const span = two ? c.w + n2.w : c.w, w = span * within(r, .5, .8), a0 = c.a0 + (span - w) * within(r, .25, .75), slab = w > .7 || r() < .4, hh = slab ? within(r, .14, .26) : within(r, .3, .48), sb = within(r, .06, .16), base = Math.min(c.h, two ? n2.h : c.h);
    for (const s2 of out) if ((s2.col === c || s2.col === n2) && Math.abs(s2.z + s2.h - s2.col.h) < 1e-6) { s2.h -= hh; s2.cut = 0; } c.h -= hh; c.used = 1; if (n2) { n2.h -= hh; n2.used = 1; }
    out.push(shape({ row: br, a0, w, h: base }, base - hh, hh, { level: 1, dp: br.dp - sb, d: br.d0 + sb + (br.dp - sb) / 2, kind: slab ? 'slab' : 'block' })); }
  return { q, rows, stones: out };
}
/* the count is held to what the sheets show: at most eleven stones */
function formation(over = {}) { let f = formationOnce(over), o = Object.assign({}, over), n = 0; while (f.stones.length > 11 && n++ < 4) { o = Object.assign({}, o, { nA: Math.max(1, f.q.nA - 1), rest: 0, tiers: f.q.tiers ? Math.max(2, f.q.tiers - (n > 1 ? 1 : 0)) : 0, rows: n > 2 ? Math.max(1, f.q.rows - 1) : f.q.rows }); f = formationOnce(o); } return f; }
const formMeshes = (f) => f.stones.map(stoneMesh);

/* Scale 1: draw a whole formation's numbers from the proposed ranges. */
function drawParams(seed, want) {
  const r = rng(seed * 31 + 5), u = r() * 52, cls = want && want !== 'any' ? want : (u < 22 ? 'pillars' : u < 32 ? 'blocks' : u < 42 ? 'leaners' : u < 49 ? 'courses' : 'gates');
  const o = { cls: cls === 'ramp' || cls === 'wall' ? (cls === 'ramp' ? 'leaners' : 'pillars') : cls, preset: cls, seed, height: mid(r, 1.6, 1.98), length: within(r, 1.25, 1.7), bevel: within(r, .1, .22), fall: within(r, .35, .75), peak: r() < .1 ? within(r, .25, .45) : within(r, 0, .18), sizeVar: .62, front: r() < .15 ? within(r, .55, .85) : within(r, .22, .45) };
  if (cls === 'pillars' || cls === 'wall') { const wall = cls === 'wall' || r() < .25; Object.assign(o, { nA: 2 + Math.floor(r() * 2.3), rows: 2 + (r() < .6 ? 1 : 0), depth: within(r, .75, 1), plateau: wall || r() < .3 ? 1 : 0, cleft: r() < .15 ? 1 : 0, rest: r() < .3 ? 1 : 0, oversail: r() < .3 ? 1 : 0 }); if (wall) Object.assign(o, { fall: within(r, .15, .35), peak: within(r, .05, .35) }); }
  if (cls === 'blocks') Object.assign(o, { nA: 2 + (r() < .6 ? 1 : 0), rows: 2 + (r() < .35 ? 1 : 0), tiers: 2 + (r() < .45 ? 1 : 0), depth: within(r, .8, 1.05), fall: within(r, .3, .65), sizeVar: .35 });
  if (cls === 'leaners') { if (r() < .3) Object.assign(o, { preset: 'ramp', dip: within(r, 20, 32), fall: within(r, .05, .2) }); else Object.assign(o, { lean: within(r, 14, 26), nA: 3 + (r() < .5 ? 1 : 0), closer: r() < .65 ? 1 : 0, length: within(r, 1.2, 1.5), front: within(r, .25, .45) }); }
  if (cls === 'ramp') Object.assign(o, { dip: within(r, 20, 32), fall: within(r, 0, .12) });
  if (cls === 'courses') Object.assign(o, { height: o.height * .9, rows: 4 + (r() < .35 ? 1 : 0), nA: r() < .6 ? 1 : 2, fall: within(r, .15, .35), peak: 0, front: within(r, .16, .24), topLen: within(r, .28, .5), sizeVar: .3, depth: within(r, 1, 1.2) });
  if (cls === 'gates') Object.assign(o, { rows: 2, fall: within(r, .25, .45), peak: within(r, .1, .35), front: within(r, .22, .36) });
  return o;
}
/* What a generated formation would measure if it were traced like a reference: each stone as seen from the references' camera, in fh,
   and the relations between touching stones measured the way analyze.py measures them on the traces. */
function seen(f) {
  const cam = cameraFor('ref', 360, 300), pr = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) * 180, (1 - v.y) * 150]; };
  let X0 = 1e9, X1 = -1e9, Y0 = 1e9, Y1 = -1e9; const fig = pr(1.25, .45, 0), figTop = pr(1.25, .45, 1), fh = Math.hypot(fig[0] - figTop[0], fig[1] - figTop[1]);
  const stones = f.stones.map((o) => { const P = stonePoints(o).map((p) => pr(...p)); const x0 = Math.min(...P.map((p) => p[0])), x1 = Math.max(...P.map((p) => p[0])), y0 = Math.min(...P.map((p) => p[1])), y1 = Math.max(...P.map((p) => p[1]));
    X0 = Math.min(X0, x0); X1 = Math.max(X1, x1); Y0 = Math.min(Y0, y0); Y1 = Math.max(Y1, y1);
    const b = pr(o.a, o.d - o.dp / 2, o.z), t = pr(o.a + o.leanA * o.h, o.d - o.dp / 2, o.z + o.h), hiP = P.reduce((u, v) => (v[1] < u[1] ? v : u)); return { wb: (x1 - x0) / fh, hb: Math.hypot(b[0] - t[0], b[1] - t[1]) / fh, kind: o.kind, x0, x1, y0, y1, xTop: hiP[0], o }; });
  const W = (X1 - X0) / fh, H = (Y1 - Y0) / fh, pairs = [];
  for (let i = 0; i < stones.length; i++) for (let j = i + 1; j < stones.length; j++) { const A = stones[i].o, Bq = stones[j].o, gapA = Math.abs(A.a - Bq.a) - (A.w + Bq.w) / 2, gapD = Math.abs(A.d - Bq.d) - (A.dp + Bq.dp) / 2, zo = Math.min(A.z + A.h, Bq.z + Bq.h) - Math.max(A.z, Bq.z);
    if (zo < .1 || !((gapA < .06 && gapD < -.08) || (gapD < .06 && gapA < -.08))) continue; const lo = stones[i].y0 > stones[j].y0 ? stones[i] : stones[j], hi = lo === stones[i] ? stones[j] : stones[i];
    pairs.push({ dtop: Math.abs(stones[i].y0 - stones[j].y0) / fh, lowfront: Math.abs(lo.y1 - hi.y1) / fh > .04 ? lo.y1 > hi.y1 : null }); }
  const near = stones.reduce((a, b) => (b.y1 > a.y1 ? b : a)), top = stones.reduce((a, b) => (b.y0 < a.y0 ? b : a)), ws = stones.map((s2) => s2.wb);
  const leftTop = Math.min(...stones.filter((s2) => s2.x0 < X0 + (X1 - X0) * .12).map((s2) => s2.y0));
  return { stones, pairs, W, H, n: stones.length, footshare: (Y1 - near.y0) / (Y1 - Y0), peak: (top.xTop - X0) / (X1 - X0), wratio: Math.max(...ws) / Math.min(...ws), farend: (Y1 - leftTop) / (Y1 - Y0),
    jut: f.stones.some((o) => o.kind === 'cap') };
}
