/* ---------- the formation (chapter 11 states it) ----------
   A formation is stones standing on the ground and stones resting on them, in three steps and one rule.
     ground  stones in a row along the back; their tops follow an envelope (the height, falling toward the far end)
     front   a buttress and a toe against the wall at the cut end
     pile    the rule: a stone is laid where there is most room under the envelope, on one top or across a joint between two
   then shape (each stone learns which of its edges are exposed) and bear (a stone with air under it is built taller, top kept).
   Every stone is first a plain box with a place (a "spec"). All lengths are in figure heights (fh); a is along the cliff from the cut end,
   d is depth (the back of the plate is PLATE), z is up. */
const FORM = { seed: 1, height: 1.7, length: 1.45, depth: .62,   /* the highest top, the length along the cliff, the back mass front to back */
  fall: .45, peak: .1,     /* how far the tops fall to the far end, as a share of the height, and where the peak is (0 = at the cut end) */
  cols: 3, spread: .5,     /* stones standing on the ground, and how unequal their widths are */
  base: .9,                /* their height as a share of the envelope: 1 stand alone, less leaves room to pile on */
  step: .15,               /* least difference between two neighbouring ground tops, fh */
  pile: 1, bond: .5,       /* stones laid on; the chance one lies across a joint rather than on one top */
  stack: .5,               /* where a stone goes that sits on one top: 0 on the lowest, 1 on the one under the highest envelope (a tower) */
  setback: .05,            /* how far a laid stone's face stands back from what carries it, fh (it is shortened by up to twice this) */
  lean: 0, dip: 0,         /* degrees: the ground stones are slabs leaning toward the cut end; every top and bed dips toward the far end */
  opening: 0,              /* share of the length left open between two piers, under lintels */
  front: 2,                /* stones against the wall at the cut end: 1 buttress, 2 and a toe, 3 and a second block */
  bevel: .16 };            /* how wide the chamfers are (0.16: the usual widths) */
/* ---------- bearing on a base ----------
   A stone that is not on the ground may overhang: part of its underside has no stone top within 0.01 fh below it. Where there is stone mass
   a little way below that part, the stone is built taller, its top where the design put it and its bottom lower, so that 3 in 4 of the
   unsupported part rests on or 0.015 fh into the stone below. It is done only when that is at most SUPPORT.reach, so a real overhang (a
   lintel over its opening) stays one. Lowest stone first, so a stone made taller bears the ones above it. */
const SUPPORT = { share: .75, reach: .25, sink: .015, ok: .01, n: 7, tol: .03 };   /* tol: how far under a stone a top may be and still hold it up */
const botZ = (s, a) => s.z + s.botA * (a - s.a), topZ = (s, a, d) => s.z + s.h + s.dipA * (a - s.a) + s.dipD * (d - s.d);
function underside(S, i) {
  const s = S[i], n = SUPPORT.n, drops = []; let sup = 0;
  for (let u = 0; u < n; u++) for (let v = 0; v < n; v++) { const a = s.a + (-.5 + (u + .5) / n) * (s.w - .1), d = s.d + (-.5 + (v + .5) / n) * (s.dp - .1), zb = botZ(s, a);
    let best = -1e9, hit = false;
    for (let k = 0; k < S.length; k++) { const t = S[k]; if (k === i || t.z > zb + .005 || Math.abs(a - t.a - t.leanA * (zb - t.z)) > t.w / 2 - .01 || Math.abs(d - t.d) > t.dp / 2 - .01) continue; const top = topZ(t, a, d); if (top >= zb - SUPPORT.ok) { hit = true; break; } if (top > best) best = top; }
    if (hit) sup++; else drops.push(best > -1e8 ? zb - best : Infinity); }
  return { frac: sup / (n * n), drops };
}
function bear(S, stats) {
  for (const i of S.map((s, k) => k).sort((p, q) => S[p].z - S[q].z)) { const s = S[i]; if (s.z < .02) continue; const u = underside(S, i); if (!u.drops.length) continue;
    const fin = u.drops.filter(isFinite).sort((p, q) => p - q), need = Math.ceil(SUPPORT.share * u.drops.length), d = fin[need - 1] + SUPPORT.sink;
    if (stats) stats.push({ i, frac: u.frac, d: fin.length >= need && d <= SUPPORT.reach && s.z - d >= 0 ? d : 0 });
    if (fin.length >= need && d <= SUPPORT.reach && s.z - d >= 0) { s.z -= d; s.h += d; } }
}
/* ---------- the support rule ----------
   One check, made when a stone is laid, keeps overhangs reasonable. The stone rests on the tops directly under its footprint and no more than
   SUPPORT.tol below it (a top any lower is not support: bear() closes only a small gap). It stands if (a) the middle half of its footprint, a quarter
   of its length and depth either side of its centre, is all over its support, so it is out past it by a quarter of its length at most; or if (b)
   it bears under the outer eighth at both ends of its long side, at a quarter of its width either side of the middle line, as a lintel or a bridge does, which a stone over a gap must. If not, the short top
   under it is raised to meet it (by up to SUPPORT.reach, if nothing is on it); failing that the stone is slid the smallest way that passes, then
   shortened and narrowed; and if nothing passes it is not laid. */
const extA = (s) => { const sh = s.lean ? -s.lean * s.h : 0; return [s.a0 + sh, s.a0 + s.w + sh]; };
const under = (o, sp) => Math.min(extA(o)[1], sp.a0 + sp.w) - Math.max(extA(o)[0], sp.a0) > .02 && Math.min(o.d0 + o.dp, sp.d0 + sp.dp) - Math.max(o.d0, sp.d0) > .02;
const supportsOf = (S, sp) => S.filter((o) => o !== sp && !o.void && o.z + o.h <= sp.z + .011 && o.z + o.h >= sp.z - SUPPORT.tol && under(o, sp));
function holds(sup, sp) {
  const on = (a, d) => sup.some((o) => a >= extA(o)[0] && a <= extA(o)[1] && d >= o.d0 && d <= o.d0 + o.dp), ca = sp.a0 + sp.w / 2, cd = sp.d0 + sp.dp / 2, ma = .25 * sp.w, md = .25 * sp.dp;
  return [-1, 0, 1].every((i) => [-1, 0, 1].every((j) => on(ca + i * ma, cd + j * md))) || [-1, 1].every((i) => [-1, 1].every((j) => (sp.w >= sp.dp ? on(ca + i * sp.w * .375, cd + j * md) : on(ca + j * ma, cd + i * sp.dp * .375))));
}
const SLIDES = (() => { const v = []; for (let i = -12; i <= 12; i++) for (let j = -12; j <= 12; j++) v.push([i * .05, j * .05]); return v.sort((p, q) => p[0] ** 2 + p[1] ** 2 - q[0] ** 2 - q[1] ** 2); })();
/* lay `sp` among S if it can be made to stand; returns whether it was laid */
function lay(S, sp) {
  const w = sp.w, dp = sp.dp, a0 = sp.a0, d0 = sp.d0, stand = () => { const sup = supportsOf(S, sp); if (!sup.length || !holds(sup, sp)) return false; Object.assign(sp, { sup, on: sup.length, level: Math.max(...sup.map((o) => o.level)) + 1, rank: sup[0].rank }); return true; };
  S.push(sp); if (stand()) return true;
  const low = S.filter((o) => o !== sp && !o.void && !o.front && o.z + o.h < sp.z - SUPPORT.tol && o.z + o.h >= sp.z - SUPPORT.reach && under(o, sp) && !S.some((c) => c.sup && c.sup.includes(o))), was = low.map((o) => o.h);
  if (low.length) { low.forEach((o) => { o.h = sp.z - o.z; }); if (stand()) return true; low.forEach((o, i) => { o.h = was[i]; }); }
  for (const k of [1, .85, .7, .55]) { if (w * k < .35 || dp * k < .3) break; sp.w = w * k; sp.dp = dp * k;
    for (const [da, dd] of SLIDES) { sp.a0 = a0 + (w - sp.w) / 2 + da; sp.d0 = d0 + (dp - sp.dp) / 2 + dd; if (sp.a0 >= -.02 && sp.d0 + sp.dp <= PLATE + .01 && stand()) return true; } }
  S.pop(); return false;
}
/* a row cut into n unequal widths, none thin */
function widths(r, len, n, spread) {
  n = Math.max(1, Math.min(n, Math.floor(len / .43))); let best = null;
  for (let t = 0; t < 14; t++) { let ws = Array.from({ length: n }, () => within(r, 1 - spread, 1 + spread)); const sum = ws.reduce((a, b) => a + b, 0); ws = ws.map((v) => v / sum * len); const thin = Math.min(...ws); if (!best || thin > best.thin) best = { ws, thin }; if (thin >= .42) break; }
  return best.ws;
}
/* `marks` times the steps and says how many specs each had made: the pictures of the algorithm page are made from them */
const now = () => (typeof performance !== 'undefined' ? performance.now() : 0);
function formation(over = {}) {
  const marks = [{ name: 'start', n: 0, t: now() }], lap = (name, n) => marks.push({ name, n, t: now() });
  const q = Object.assign({}, FORM, over), r = rng(q.seed * 104729 + 7), S = [], L = q.length, D = q.depth, dB = PLATE - D, tl = Math.tan(q.lean * Math.PI / 180), td = Math.tan(q.dip * Math.PI / 180), gate = q.opening > 0;
  const add = (o) => { const s = Object.assign({ z: 0, level: 0, on: 0, rank: 2 }, o); S.push(s); return s; };
  const env = (x) => { const a = Math.min(x, L), p = q.peak * L, far = a > p ? (a - p) / Math.max(L - p, .01) : (p - a) / Math.max(L, .01) * .5; return q.height * (1 - q.fall * far); };
  const top = (s, a = s.a0 + s.w / 2) => s.z + s.h - td * (a - s.a0 - s.w / 2), room = (s) => env(s.a0 + s.w / 2) - top(s);

  /* ---- 1 ground: stones side by side along the back, each as high as the envelope lets its share be ---- */
  { const ws = gate ? (() => { const g = L * q.opening, t = (L - g) * within(r, .45, .55); return [t, g, L - t - g]; })() : tl ? [tl * q.height + within(r, .3, .42), ...Array.from({ length: q.cols - 1 }, () => within(r, .38, .56))] : widths(r, L, q.cols, q.spread);
    let a0 = 0; const pier = gate ? q.base * env(ws[0] / 2) * within(r, .94, 1.04) : 0, hs = [];
    ws.forEach((w, k) => { let h = gate ? pier - (k === 1 ? within(r, .32, .42) : 0) : Math.max(.3, lerp(q.base, 1, r() ** 3) * env(a0 + w / 2) * within(r, .92, 1.03));
      if (k && !gate && Math.abs(h - hs[k - 1]) < q.step) h = Math.max(.3, hs[k - 1] + (h >= hs[k - 1] && r() < .5 ? 1 : -1) * q.step * within(r, 1, 1.5));
      hs.push(h); a0 += w; });
    a0 = 0; ws.forEach((w, k) => { const hole = gate && k === 1, pr = hole ? 0 : Math.max(0, Math.min(.38, within(r, 0, .12) + .9 * (Math.max(...hs) - hs[k]), 1.3 * w - D)), recess = hole ? D * within(r, .6, .85) : 0;   /* the lower of two stands the more forward */
      add({ a0, w, d0: dB - pr + recess, dp: D + pr - recess, h: hs[k], void: hole, lean: tl && k ? tl : 0, end: tl && !k }); a0 += w; }); }
  lap('ground', S.length);

  /* ---- 2 front: a chunky buttress hard against the wall, a toe in front of it or beside it, a second block along the wall ---- */
  if (q.front > 0) {
    const wallAt = (a0, a1) => { const b = S.filter((s) => s.rank === 2 && !s.void && s.a0 < a1 - .03 && s.a0 + s.w > a0 + .03); return b.length ? { d: Math.min(...b.map((s) => s.d0)), hi: Math.max(...b.map((s) => s.z + s.h)) } : { d: dB, hi: q.height * .5 }; };
    const wide = q.lean || q.dip || gate, aB = !gate && r() < .33 ? within(r, 0, .3 * L) : 0, w = Math.min(wide ? within(r, .5, .8) : within(r, .42, .6), L * .55, gate ? L * q.opening - .02 : 9), wl = wallAt(aB, aB + w), dp = wide ? within(r, .4, .58) : within(r, .32, .46);
    const hB = Math.max(.35, Math.min(within(r, .55, .82) * wl.hi, wl.hi - .2)), bu = add({ a0: aB, w, d0: wl.d - dp + within(r, .05, .14), dp, h: hB, rank: 1, front: 1 }); let aNext = aB + w - .03;
    if (q.front > 2) { const w2 = within(r, .38, .55), wl2 = wallAt(aNext, aNext + w2), dp2 = within(r, .34, .52), h2 = Math.max(.3, Math.min(hB * within(r, .6, 1.2), wl2.hi - .2));
      add({ a0: aNext, w: w2, d0: wl2.d - dp2 + within(r, .05, .12), dp: dp2, h: Math.abs(h2 - hB) < q.step ? Math.max(.28, hB - q.step * 1.4) : h2, rank: 1, front: 1 }); aNext += w2 - .03; }
    if (q.front > 1) { const w3 = Math.min(within(r, .45, .7), gate ? L * q.opening : 9), dp3 = within(r, .32, .45), beside = !gate && q.front < 3 && r() < .4, a3 = beside ? aNext : aB + within(r, 0, .08);
      add({ a0: a3, w: w3, d0: beside ? wallAt(a3, a3 + w3).d - dp3 + .07 : bu.d0 - dp3 + .06, dp: dp3, h: Math.max(.3, Math.min(.55, hB * within(r, .4, .6))), rank: 0, front: 1 }); } }
  lap('front', S.length);

  /* ---- 3 pile: lay a stone (and check it stands, above) where there is most room under the envelope. It stands on one top, or lies across the joint between two
     (bearing on both); it is set back from what carries it, and is as high as the room is, or one stone of a stack. ---- */
  const free = (s) => { const [lo, hi] = extA(s); let best = [lo, lo], x = lo; for (const [u, v] of S.filter((o) => o.sup && o.sup.includes(s)).map((o) => [o.a0, o.a0 + o.w]).sort((p, c) => p[0] - c[0]).concat([[hi, hi]])) { if (u - x > best[1] - best[0]) best = [x, u]; x = Math.max(x, v); } return best; };
  const tall = (rm) => { const t = rm <= .5 ? rm : rm * within(r, .33, .55); return rm - t < .22 ? rm : t; };
  for (let n = 0; n < q.pile && S.length < 11; n++) {
    if (gate && !n) { const [P, G, Q] = S, a0 = P.a0 + P.w * .4, a1 = Q.a0 + Q.w * .6, z = Math.max(top(P), top(Q)), d0 = Math.min(P.d0, Q.d0) - within(r, .1, .2);   /* a gate's lintel lies on both piers, or the opening is shut */
      if (lay(S, { a0, w: a1 - a0, d0, dp: .85, z, h: tall(env((a0 + a1) / 2) - z) })) continue; G.void = false; G.h = z - G.z; }
    const T = S.filter((s) => !s.front && !s.void && room(s) >= .22 && free(s)[1] - free(s)[0] >= .4), joints = [];
    for (const A of S) for (const B of S) { if (A === B || A.front || B.front || (A.void && B.void)) continue; const fa = free(A), fb = free(B), pa = extA(A), pb = extA(B);
      if (Math.abs(pa[1] - pb[0]) < .07 && fa[1] > pa[1] - .01 && fb[0] < pb[0] + .01 && (A.void || fa[1] - fa[0] >= .2) && (B.void || fb[1] - fb[0] >= .2) && (A.void || B.void || Math.min(A.d0 + A.dp, B.d0 + B.dp) - Math.max(A.d0, B.d0) > .3) && (A.void || B.void || Math.abs(top(A) - top(B)) < .3)) joints.push([A, B]); }
    const rate = (A, B) => Math.min(...[A, B].filter((s) => !s.void).map(room)), J = joints.filter(([A, B]) => rate(A, B) >= .22);
    if (J.length && r() < q.bond) {
      const [A, B] = J.reduce((u, v) => (rate(...v) + .1 * r() > rate(...u) ? v : u)), fa = free(A), fb = free(B), reach = (c) => c.void ? c.w * .5 : c.w * within(r, .45, .7), a0 = Math.max(fa[0], extA(A)[1] - reach(A)), a1 = Math.min(fb[1], extA(B)[0] + reach(B));
      if (a1 - a0 >= .42 && a1 - a0 <= .9) { const lo = [A, B].filter((c) => !c.void).sort((u, v) => top(u) - top(v))[0], hi = [A, B].filter((c) => !c.void).reduce((u, v) => (top(v) > top(u) ? v : u)), was = lo.h;
        if (!S.some((o) => o.sup && o.sup.includes(lo))) lo.h += top(hi) - top(lo);   /* the lower top is raised to the higher, so the stone lies on both */
        const z = Math.max(...[A, B].filter((c) => !c.void).map((c) => top(c, (a0 + a1) / 2))), d0 = Math.min(A.d0, B.d0) + (A.void || B.void ? -within(r, .1, .2) : within(r, .03, .1));
        if (lay(S, { a0, w: a1 - a0, d0, dp: Math.min(.85, Math.min(A.d0 + A.dp, B.d0 + B.dp) - d0), z, h: tall(env((a0 + a1) / 2) - z) })) continue; lo.h = was; } }
    if (!T.length) break;
    const sc = (v) => env(v.a0 + v.w / 2) - (1 - q.stack) * top(v) + .1 * r(), s = T.reduce((u, v) => (sc(v) > sc(u) ? v : u)), [fa, fb] = free(s), w = Math.max(.42, Math.min(fb - fa, .75) - within(r, 0, 2 * q.setback)), a0 = r() < .6 ? fa : fb - w, d0 = s.d0 + q.setback * within(r, .3, 1.7), z = top(s, a0 + w / 2);
    lay(S, { a0, w, d0, dp: Math.min(.85, 1.2 * w, s.d0 + s.dp - d0), z, h: tall(env(a0 + w / 2) - z) }); }
  lap('pile', S.length);

  /* ---- 4 shape: each stone learns which of its edges are exposed, then is shaped by that ---- */
  const near = (u, v) => Math.abs(u - v) < .08;
  /* an edge is exposed when any point along it is untouched by another stone or the ground (7 samples, 0.03 fh tolerance; the back of the plate holds the wall) */
  const touched = (sp, x, y, z) => z < .03 || y > PLATE - .04 || S.some((o) => o !== sp && x >= o.a0 - .03 && x <= o.a0 + o.w + .03 && y >= o.d0 - .03 && y <= o.d0 + o.dp + .03 && z >= o.z - .03 && z <= o.z + o.h + .03);
  const exposed = (sp, P, Q) => { let n = 0; for (let i = 0; i < 7; i++) { const t = (i + .5) / 7; if (touched(sp, P[0] + (Q[0] - P[0]) * t, P[1] + (Q[1] - P[1]) * t, P[2] + (Q[2] - P[2]) * t)) n++; } return n < 7; };
  const edgesOf = (sp) => { const a1 = sp.a0 + sp.w, d1 = sp.d0 + sp.dp, z0 = sp.z + .005, z1 = sp.z + sp.h - .005, e = (k) => [[[sp.a0, sp.d0, k], [sp.a0, d1, k]], [[a1, sp.d0, k], [a1, d1, k]], [[sp.a0, sp.d0, k], [a1, sp.d0, k]], [[sp.a0, d1, k], [a1, d1, k]]];
    return { t: e(z1).map(([P, Q]) => exposed(sp, P, Q)), b: e(z0).map(([P, Q]) => exposed(sp, P, Q)), v: [[sp.a0, sp.d0], [a1, sp.d0], [sp.a0, d1], [a1, d1]].map(([x, y]) => exposed(sp, [x, y, z0], [x, y, z1])) }; };
  const out = S.map((sp) => { const topz = sp.z + sp.h, a1 = sp.a0 + sp.w, d1 = sp.d0 + sp.dp, m = Math.min(sp.w, sp.dp, sp.h), front = !!sp.front, rest = sp.level > 0, bv = q.bevel;
    const hid = (test) => S.some((o) => o !== sp && o.z + o.h > topz - .06 && o.z < topz && test(o)), ovD = (o) => Math.min(d1, o.d0 + o.dp) - Math.max(sp.d0, o.d0) > .08, ovA = (o) => Math.min(a1, o.a0 + o.w) - Math.max(sp.a0, o.a0) > .08;
    const ex = edgesOf(sp), cov = ex.t.map((x) => !x);
    const carried = S.filter((o) => o !== sp && Math.abs(o.z - topz) < .07 && ovA(o) && ovD(o));
    const clear = (test) => !S.some((o) => o !== sp && o.z + o.h > topz - .2 && o.z < topz && test(o)), clN = sp.a0 > .05 && clear((o) => near(o.a0 + o.w, sp.a0) && ovD(o)), clF = clear((o) => near(o.a0, a1) && ovD(o));
    /* chisel: a chamfer on an exposed top edge, widest on the front and the cut-end side (0.02-0.15 fh deep, 0.01-0.04 fh down the side), seldom on the far side; the same, mirrored, under an exposed bottom edge (a lintel, an overhang); none on a covered edge;
       sometimes one broad one; a narrow chamfer on a front upright edge now and then; one to three top corners knocked off; a hair everywhere else (in stone.js) */
    const kw = q.bevel / .16, wide = sp.w >= .45, side = !cov[1] ? 1 : !cov[0] && sp.a0 > .05 ? 0 : -1, pl = Math.min(sp.w, sp.dp);
    /* chamfers are big: they shape the silhouette. Depths, in fh, none under 0.06: top 0.06-0.15 (far side 0.06-0.1), dropping 0.02-0.04; underside 0.06-0.15, rising depth / 1.4 (55 degrees from vertical, so it prints); upright 0.06-0.14. An edge on the ground is sharp; any other covered edge keeps the 0.01 hair. */
    const big = (x) => Math.max(.06, x * kw) / m, deep = (e) => big(e === 0 || e === 2 ? .06 + .09 * r() ** 2 : within(r, .06, .1)), bT = ex.t.map((x, e) => x ? deep(e) : 0), ground = sp.z < .03, bB = ex.b.map((x) => ground ? -1 : x ? big(within(r, .06, .15)) : 0), vc = ex.v.map((x) => x ? Math.max(.06, (.06 + .08 * r() ** 1.3) * kw) / pl : 0);
    if (wide && !carried.length && side >= 0 && sp.h > .5 && r() < (rest ? .3 : .5)) bT[side] = within(r, .08, .15) * kw / m;
    if (carried.length) for (let i = 0; i < 4; i++) bT[i] = Math.min(bT[i], Math.max(.03, .8 * [sp.a0, a1, sp.d0, d1].map((e, k) => k < 2 ? Math.min(...carried.map((o) => k ? a1 - o.a0 - o.w : o.a0 - sp.a0)) : Math.min(...carried.map((o) => k === 2 ? o.d0 - sp.d0 : d1 - o.d0 - o.dp)))[i]) / m);
    if (wide && sp.h > 1.3 * sp.w && !front) { const u = r(); if (u < .3) vc[0] = vc[1] = within(r, .05, .09) * kw / pl; else if (u < .44) vc[r() < .5 ? 0 : 1] = within(r, .05, .1) * kw / pl; }
    /* batter: a side stands in only above the top of the stone beside it; the top slopes unless it carries a stone */
    const sh = (test) => Math.max(0, Math.min(1, (topz - Math.max(0, ...S.filter((o) => o !== sp && o.z < topz && test(o)).map((o) => o.z + o.h))) / sp.h)), k = rest ? .8 : 1, bat = [0, 0, 0, 0]; let dipA = 0, dipD = 0;
    if (front) { bat[1] = clF ? Math.min(sp.w * .25, sp.h * within(r, .1, .2)) : 0; bat[2] = Math.min(sp.dp * .3, sp.h * within(r, .1, .2)); const u = r();   /* a wedge top rising into the wall, on the larger stones */
      if (sp.h < .5) { dipD = within(r, 0, .12); if (clF && r() < .5) dipA = -within(r, .04, .12); } else if (u < .7) dipA = (clF ? -1 : 1) * within(r, .3, .6); else dipD = u < .85 ? within(r, .3, .6) : within(r, 0, .1); }
    else { if (clN) bat[0] = Math.min(sp.w * .22, sp.h * within(r, .06, .14) * k) * sh((o) => near(o.a0 + o.w, sp.a0) && ovD(o)); if (clF) bat[1] = Math.min(sp.w * .22, sp.h * within(r, .06, .14) * k) * sh((o) => near(o.a0, a1) && ovD(o));
      if (!cov[2]) bat[2] = Math.min(sp.dp * .25, sp.h * within(r, .08, .16) * k); dipD = carried.length ? 0 : within(r, -.03, .11);
      dipA = td ? -td : carried.length ? 0 : (clF && !clN ? -1 : clN && !clF ? 1 : r() < .5 ? -1 : 1) * (clF || clN ? within(r, .04, .16) : within(r, 0, .05)) * (rest ? .6 : 1);
      if (sp.end) { bat[1] = tl * sp.h; dipA = within(r, .05, .2); } }
    const clips = [], u = r(); if (!carried.length && wide) for (let n = u < .5 ? 0 : u < .8 ? 1 : u < .93 ? 2 : 3, cs = [0, 1, 2, 3].sort(() => r() - .5); n > 0; n--) clips.push([cs[n - 1], Math.max(.06, within(r, .06, .12) * kw) / m]);
    const g = rest ? .004 : .006, kind = front && sp.rank === 0 ? 'toe' : sp.on >= 2 ? 'cap' : sp.lean || sp.end ? 'lean' : sp.h > 1.35 * Math.max(sp.w, sp.dp) ? 'pillar' : sp.h < .7 * Math.max(sp.w, sp.dp) ? 'slab' : 'block';
    return { w: Math.max(.08, sp.w - g), dp: Math.max(.08, sp.dp - g), h: sp.h, a: sp.a0 + sp.w / 2, d: sp.d0 + sp.dp / 2, z: sp.z, drop: within(r, .02, .04), bevelT: bT, bevelB: bB, bevelVC: vc, ex, batter: bat, dipA, dipD, botA: rest && td ? -td : 0, leanA: sp.lean ? -sp.lean : 0,
      clips, kind, level: sp.level, on: sp.on, rank: sp.rank }; });
  lap('shape', out.length);
  bear(out);
  lap('bear', out.length);
  return { q, stones: out, specs: S, marks };
}
/* a formation's meshes: the stones with those on a fused joint (chapter 8's rule) grown toward each other, or with `apart` the stones as built */
const formMeshes = (f, apart) => apart ? f.stones.map(stoneMesh) : grownStones(f).map(stoneMesh);

/* ---------- the families ----------
   Three families, each a region of the same parameters; a draw takes every parameter from its family's range, so the draws between the
   families are formations too. columns: tall stones standing alone (base near 1), with a stone laid on some; a low fall and a shared dip
   make a plateau wall; an opening makes a gate. tiers: stones low enough to stack, laid in courses, set back as they rise; the lower the
   base the deeper the mass and the bigger the step back (blocks, then courses). slabs: the ground stones lean (columns become fins) or
   every bed dips (tiers become a ramp). The mix is the one the sheets show: 25 : 17 : 10 of 52. */
const FAMILIES = ['columns', 'tiers', 'slabs'];
/* what a set of numbers reads as, in the old words of the sheets */
const lookOf = (q) => (q.opening ? 'a gate' : q.lean ? 'a leaning set' : q.dip > 12 ? 'dipping beds' : q.dip ? 'a plateau wall' : q.setback > .06 ? (q.base < .45 ? 'courses' : 'blocks in tiers') : 'pillars');
function drawParams(seed, want) {
  const r = rng(seed * 31 + 5), u = r() * 52, family = FAMILIES.includes(want) ? want : u < 25 ? 'columns' : u < 42 ? 'tiers' : 'slabs', pick = (...p) => { let v = r(); return p.findIndex((x) => (v -= x) < 0); };
  const o = { family, seed, height: mid(r, 1.55, 1.77), length: within(r, 1.2, 1.65), bevel: within(r, .14, .24), fall: within(r, .35, .75), peak: r() < .1 ? within(r, .25, .45) : within(r, 0, .18), front: [0, 1, 2, 3][pick(.2, .1, .5, .2)] };
  if (family === 'columns') { const wall = r() < .3, gate = !wall && r() < .15;
    Object.assign(o, { cols: (o.length < 1.45 ? 2 : 3) + (r() < .5 ? 1 : 0), spread: .5, base: within(r, .4, .95), step: wall ? .06 : .17, depth: within(r, .42, .55), fall: wall ? within(r, .1, .3) : within(r, .45, .75), peak: wall ? within(r, .05, .35) : o.peak, dip: wall ? within(r, 4, 10) : 0, pile: 4, bond: .85, stack: .7, setback: within(r, 0, .06) });
    if (gate) Object.assign(o, { opening: within(r, .3, .38), height: within(r, 1.36, 1.52), base: within(r, .62, .72), depth: within(r, .65, .8), fall: within(r, .2, .4), peak: within(r, .05, .25), pile: 2, bond: 1, front: 2, setback: 0 }); }
  else if (family === 'tiers') { const b = within(r, .3, .7), k = (b - .3) / .4;   /* b low: courses, b high: blocks */
    Object.assign(o, { base: b, cols: b < .45 ? 1 + (r() < .6 ? 1 : 0) : 2 + (r() < .65 ? 1 : 0), spread: .35, height: mid(r, 1.47, 1.73 - .1 * k), length: within(r, 1.35, 1.65), depth: lerp(within(r, 1, 1.25), within(r, .6, .75), k), fall: lerp(within(r, .25, .45), within(r, .4, .65), k), peak: b < .45 ? 0 : o.peak, step: lerp(.1, .15, k), pile: b < .45 ? 5 : 4 + (r() < .4 ? 1 : 0), bond: lerp(.7, .5, k), stack: lerp(.3, .6, k), setback: lerp(within(r, .15, .25), within(r, .05, .1), k), front: b < .45 ? 0 : o.front }); }
  else if (r() < .35) Object.assign(o, { dip: within(r, 14, 24), height: mid(r, 1.5, 1.71), cols: 1 + (r() < .5 ? 1 : 0), spread: .3, base: within(r, .6, .7), fall: within(r, .05, .2), depth: within(r, .62, .8), pile: 2 + (r() < .5 ? 1 : 0), bond: .3, setback: within(r, .05, .1), step: .1 });
  else Object.assign(o, { lean: within(r, 15, 24), height: mid(r, 1.5, 1.71), cols: 3 + (r() < .3 ? 1 : 0), spread: .12, base: within(r, .65, .8), length: within(r, 1.3, 1.6), fall: within(r, .45, .7), peak: 0, depth: within(r, .5, .68), pile: 1 + (r() < .6 ? 1 : 0), bond: .7, setback: 0, step: .1 });
  return o;
}
/* What a generated formation would measure if it were traced like a reference: each stone as seen from the references' camera, in fh,
   and the relations between touching stones measured the way analyze.py measures them on the traces. */
function seen(f, az = 45) {
  const cam = cameraFor('ref', 360, 300, { az }), pr = (a, d, z) => { const v = new THREE.Vector3(a, z, d).project(cam); return [(v.x + 1) * 180, (1 - v.y) * 150]; };
  let X0 = 1e9, X1 = -1e9, Y0 = 1e9, Y1 = -1e9; const fig = pr(1.25, .45, 0), figTop = pr(1.25, .45, 1), fh = Math.hypot(fig[0] - figTop[0], fig[1] - figTop[1]);
  const stones = f.stones.map((o) => { const P = stonePoints(o).map((p) => pr(...p)); const x0 = Math.min(...P.map((p) => p[0])), x1 = Math.max(...P.map((p) => p[0])), y0 = Math.min(...P.map((p) => p[1])), y1 = Math.max(...P.map((p) => p[1]));
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
    jut: f.stones.some((o) => o.kind === 'cap'), rest: f.stones.filter((o) => o.level > 0).length, two: f.stones.filter((o) => o.on >= 2).length, real: f.stones.length, deep: Math.max(...f.stones.map((o) => o.level)) };
}
