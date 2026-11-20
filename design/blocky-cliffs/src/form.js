/* ---------- the formation: a grid of columns under an envelope, cut and shaped (proposed; chapter 11 states it) ---------- */
const FORM = { seed: 1, cls: 'pillars',
  length: 1.45, depth: .82, height: 1.75,   /* fh: along the cliff, front to back, the highest top */
  nA: 3, rows: 2,          /* stones along the back row; rows front to back */
  frontLen: .55,           /* the front row's length as a share of the back row's */
  front: .34,              /* the front row's height as a share of the back row's */
  fall: .42,               /* how far the tops fall from the peak to the far end, as a share of the height */
  peak: .12,               /* where the peak is along the cliff (0 = at the cut end) */
  sizeVar: .3,             /* how unequal the widths are */
  stagger: .5,             /* joints of one row against the row behind: 0 in line, .5 half a stone off */
  stepMin: .13,            /* least difference between neighbouring tops, fh */
  cutEvery: 0,             /* level joints: one about every so many fh (0 = none) */
  lean: 0,                 /* degrees: every stone tilts this far, top toward the cut end */
  dip: 0,                  /* degrees: beds (tops and level joints) dip this far, falling away from the cut end */
  taper: .94, bevel: .16, jit: .02, offset: .03, cutCorner: .3,
  cap: 0, over: .1,        /* cap stones: how many, and how far they jut out, fh */
  gate: false, toe: true, rough: 1,
  leanEach: 0, yawEach: 0, bevelEach: 0, gap: 0 };   /* disorder, for the experiments: each stone's own tilt (degrees), turn (degrees), bevel spread, and open joints (fh) */
const PRESET = {
  pillars: { nA: 3, rows: 2, front: .34, fall: .42, cutEvery: 0 },
  blocks: { nA: 3, rows: 2, front: .4, fall: .4, cutEvery: .62, sizeVar: .25, cap: 1, frontLen: .6 },
  courses: { nA: 1, rows: 4, front: .2, fall: .08, depth: 1.05, length: 1.5, frontLen: .92, sizeVar: .1, toe: false, stepMin: .2, peak: 0, cutCorner: .15 },
  leaners: { nA: 3, rows: 2, lean: 20, front: .36, fall: .4, depth: .72 },
  ramp: { cls: 'leaners', nA: 1, rows: 2, dip: 27, cutEvery: .42, front: .42, fall: .1, frontLen: .5, depth: .8, sizeVar: .15 },
  gates: { nA: 3, rows: 1, gate: true, fall: .1, depth: .7, peak: .3, toe: true, height: 1.7 } };
function formation(over = {}) {
  const q = Object.assign({}, FORM, PRESET[over.preset || over.cls] || {}, over), r = rng(q.seed * 104729 + 7), out = [];
  const tl = Math.tan(q.lean * Math.PI / 180), td = Math.tan(q.dip * Math.PI / 180);
  /* 1 rows: front rows are thinner and shorter than the back row */
  const wts = Array.from({ length: q.rows }, (_, i) => lerp(.72, 1.15, q.rows > 1 ? i / (q.rows - 1) : 1) * within(r, 1 - q.sizeVar * .4, 1 + q.sizeVar * .4)), wsum = wts.reduce((a, b) => a + b, 0);
  const back = PLATE; let d0 = back - q.depth; const rows = [];
  for (let i = 0; i < q.rows; i++) { const t = q.rows > 1 ? i / (q.rows - 1) : 1, dp = q.depth * wts[i] / wsum, len = q.length * lerp(q.frontLen, 1, Math.pow(t, .8)); rows.push({ i, t, d0, dp, len }); d0 += dp; }
  /* 2 columns: each row is cut along its length; joints are shifted against the row behind */
  const env = (a, t) => { const p = q.peak * q.length, far = a > p ? (a - p) / Math.max(q.length - p, .01) : (p - a) / Math.max(q.length, .01) * .5; return q.height * lerp(q.front, 1, t) * (1 - q.fall * far); };
  for (const row of rows) { const n = Math.max(1, Math.round(q.nA * row.len / q.length + (q.rows - 1 - row.i) * .001));
    let ws = Array.from({ length: n }, () => within(r, 1 - q.sizeVar, 1 + q.sizeVar)); const sum = ws.reduce((a, b) => a + b, 0); ws = ws.map((v) => v / sum * row.len);
    if (n > 1 && (q.rows - 1 - row.i) % 2 === 1) { const sh = q.stagger * row.len / n * .8; ws[0] -= sh * .5; ws[n - 1] += sh * .5; }
    let a0 = 0; row.cols = ws.map((w, k) => { const c = { a0, w, k, row }; a0 += w; return c; }); }
  /* 3 heights: under the envelope, then pushed apart so no two neighbours share a level and nothing in front is taller than what stands behind it */
  for (let i = rows.length - 1; i >= 0; i--) { const row = rows[i];
    row.cols.forEach((c, k) => { c.h = env(c.a0 + c.w / 2, row.t) * within(r, 1 - .09 * q.rough, 1 + .09 * q.rough);
      if (i < rows.length - 1) { const behind = rows[i + 1].cols.filter((b) => b.a0 < c.a0 + c.w && b.a0 + b.w > c.a0); const lowest = Math.min(...behind.map((b) => b.h)); if (isFinite(lowest)) c.h = Math.min(c.h, lowest - q.stepMin * 1.5); }
      if (k > 0) { const p = row.cols[k - 1]; if (Math.abs(p.h - c.h) < q.stepMin) c.h = p.h - q.stepMin * (1 + r() * .6); }
      c.h = Math.max(c.h, .16); }); }
  /* 4 a gate: one inner column of the back row is set back and lowered; a lintel goes over it and onto its neighbours */
  let lintel = null;
  if (q.gate && rows[rows.length - 1].cols.length >= 3) { const br = rows[rows.length - 1], k = 1, g = br.cols[k], L = br.cols[k + 1], R = br.cols[k - 1];
    const top = Math.max(L.h, R.h, q.height * .92), th = within(r, .42, .55); L.h = R.h = top - th; g.h = top - th - .04; g.recess = .55;
    lintel = [{ a0: g.a0 - R.w * .62, w: g.w * .55 + R.w * .62, h: th, z: top - th, row: br, seedK: 91, over: q.over + .04 }, { a0: g.a0 + g.w * .55, w: g.w * .45 + L.w * .7, h: th * within(r, .82, .95), z: top - th, row: br, seedK: 92, over: q.over + .08 }]; }
  /* 5 stones: each column is one stone, or is cut by level joints into two or three */
  const shape = (c, z, hh, extra = {}) => { const row = c.row, isTop = extra.top !== false, rec = c.recess || 0;
    const dp = row.dp * (1 - rec) + (row.i === rows.length - 1 ? 0 : 0), dC = row.d0 + row.dp * rec + dp / 2;
    const setb = (extra.level || 0) * q.offset * 1.5;
    return Object.assign({ w: c.w - .012, dp: dp - .012, h: hh, a: c.a0 + c.w / 2 + (r() - .5) * q.offset * (extra.level ? 1 : .3), d: dC + setb + (r() - .5) * q.offset * .6, z,
      bevel: q.bevel * within(r, .85, 1.2), bevelB: extra.level ? q.bevel * .8 : 0, taperA: q.taper * within(r, .97, 1.03), taperD: Math.min(1, q.taper * within(r, .98, 1.04)),
      dipA: -td - tl * .55 + (isTop ? -within(r, 0, .1) * q.rough : 0), dipD: isTop ? within(r, -.02, .12) * q.rough : 0, botA: extra.level ? -td : 0,
      cut: (isTop && r() < q.cutCorner) ? within(r, .32, .5) : 0, cutAt: r() < .6 ? 0 : 1, jit: q.jit, yaw: (r() - .5) * q.offset * 1.6, seed: Math.floor(r() * 1e6), kind: extra.kind || (hh > 1.35 * Math.max(c.w, dp) ? 'pillar' : hh < .7 * Math.max(c.w, dp) ? 'slab' : 'block'), col: c }, extra.o || {}); };
  for (const row of rows) for (const c of row.cols) {
    const isToe = q.toe && row.i === 0 && q.rows > 1;
    if (isToe) { out.push(shape(c, 0, c.h, { kind: 'toe', o: { taperA: q.taper * .8, taperD: q.taper * .84, bevel: q.bevel * 1.25 } })); continue; }
    let cuts = []; if (q.cutEvery > 0 && c.h > q.cutEvery * 1.45) { const n = Math.max(2, Math.round(c.h / q.cutEvery)); let z = 0; for (let i = 0; i < n - 1; i++) { z += c.h / n * within(r, .8, 1.2); cuts.push(z); } }
    let z0 = 0; cuts.forEach((zc, i) => { out.push(shape(c, z0, zc - z0 + .01, { top: false, level: i })); z0 = zc; });
    out.push(shape(c, z0, c.h - z0, { level: cuts.length }));
  }
  /* 6 caps: the top of the tallest columns becomes a stone of its own that juts out */
  if (lintel) for (const l of lintel) out.push(shape(Object.assign({ recess: 0 }, l), l.z, l.h, { kind: 'cap', level: 1, o: { dp: l.row.dp + l.over, d: l.row.d0 + (l.row.dp - l.over) / 2, w: l.w + l.over * .5, bevelB: q.bevel, taperA: 1, taperD: 1, seed: l.seedK + q.seed } }));
  else if (q.cap > 0) { const tall = out.filter((s2) => s2.col.row.i === rows.length - 1 && s2.z + s2.h >= s2.col.h - 1e-6).sort((x, y) => (y.z + y.h) - (x.z + x.h)).slice(0, q.cap);
    for (const s2 of tall) { const th = Math.min(s2.h * .5, within(r, .34, .5)); s2.h -= th; s2.cut = 0; const c = s2.col;
      out.push(shape(c, s2.z + s2.h, th, { kind: 'cap', level: 2, o: { w: c.w + q.over, a: c.a0 + c.w / 2 + q.over * .4, dp: c.row.dp + q.over, d: c.row.d0 + (c.row.dp - q.over) / 2, bevelB: q.bevel, taperA: 1, taperD: 1 } })); } }
  /* 7 the common lean, last: a shear of the whole thing about the plate */
  for (const s2 of out) { s2.leanA = -tl; s2.a -= tl * s2.z;
    if (q.leanEach) { s2.leanA += Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180); s2.leanD = Math.tan((r() - .5) * 2 * q.leanEach * Math.PI / 180); }
    if (q.yawEach) s2.yaw += (r() - .5) * 2 * q.yawEach * Math.PI / 180;
    if (q.bevelEach) s2.bevel = Math.max(.01, s2.bevel * (1 + (r() - .5) * 2 * q.bevelEach));
    if (q.gap) { s2.w -= q.gap; s2.dp -= q.gap; } }
  if (tl) { const sh = Math.max(0, tl * q.height * .55); for (const s2 of out) s2.a += sh; }
  return { q, rows, stones: out };
}
const formMeshes = (f) => f.stones.map(stoneMesh);
