/* ---------- fused faces ----------
   Most joints between two stones stay what the stones make them: the natural gap between two rounded edges. A joint that has fused
   (chapter 8's rule: which contacts, how much of the length, where, how it ends) gets a filler, a plain mesh added to the two stones, never
   a cut. The filler's front face lies on the plane of the two stones' faces and spans the gap between their rounded edges, so the faces read
   as one there. Where the two faces are not on one plane the filler is on the recessed one and meets the forward stone's side at a sharp
   inside corner. Only the filler's convex top edge is rounded, like the stones'. A ramp end tapers the filler back into the stones until the
   gap is open again; a notch end stops it square. */
/* ---- contacts: where two stones touch, and what that joint is ---- */
const aLo = (s) => s.a - s.w / 2, aHi = (s) => s.a + s.w / 2, dLo = (s) => s.d - s.dp / 2, dHi = (s) => s.d + s.dp / 2;
function contactsOf(f) {
  const S = f.stones, C = [];
  for (let i = 0; i < S.length; i++) for (let j = 0; j < S.length; j++) { if (i === j) continue; const lo = S[i], up = S[j];
    const oa = Math.min(aHi(lo), aHi(up)) - Math.max(aLo(lo), aLo(up)), od = Math.min(dHi(lo), dHi(up)) - Math.max(dLo(lo), dLo(up));
    if (oa > .08 && od > .08 && Math.abs(up.z - (lo.z + lo.h)) < .06 && up.z + up.h > lo.z + lo.h + .02) C.push({ type: 'bed', lo: i, up: j, a0: Math.max(aLo(lo), aLo(up)), a1: Math.min(aHi(lo), aHi(up)), d0: Math.max(dLo(lo), dLo(up)), d1: Math.min(dHi(lo), dHi(up)), zb: up.z }); }
  for (let i = 0; i < S.length; i++) for (let j = 0; j < S.length; j++) { if (i >= j) continue; let l = S[i], u = S[j], li = i, ui = j; if (l.a > u.a) { l = S[j]; u = S[i]; li = j; ui = i; }
    const gap = aLo(u) - aHi(l), od = Math.min(dHi(l), dHi(u)) - Math.max(dLo(l), dLo(u)), z0 = Math.max(l.z, u.z), z1 = Math.min(l.z + l.h, u.z + u.h);
    if (Math.abs(gap) < .05 && od > .1 && z1 - z0 > .2 && !C.some((c) => c.type === 'bed' && ((c.lo === i && c.up === j) || (c.lo === j && c.up === i)))) C.push({ type: 'up', lo: li, up: ui, a0: (aHi(l) + aLo(u)) / 2, d0: Math.max(dLo(l), dLo(u)), d1: Math.min(dHi(l), dHi(u)), z0, z1, front: Math.abs(dLo(l) - dLo(u)) < .05 }); }
  return C;
}

/* How a contact's joint is drawn, from the numbers the atlas's chapter 8 measures (the chance a contact fuses, how much, where, how it ends). */
const FUSE = { upFront: .20, upBack: .07, bedCourse: .30, bedBack: .04, rest: .03, far: 2, near: .7, minLen: .4, share: [.3, .7], ramp: .6, rampLen: [.15, .5], pillarFoot: .6, pillarMid: .3, full: .1 };
function settle(f, S, C, noFuse) {
  const r = rng(f.q.seed * 7919 + 13), soft = within(r, .5, 1.5), L = f.q.length, fullAt = r() < FUSE.full ? r() : -1, out = [];
  const eligible = C.map((c, n) => { const lo = S[c.lo], up = S[c.up]; return !lo.part && !up.part && (c.type === 'bed' ? c.a1 - c.a0 : c.z1 - c.z0) >= FUSE.minLen && (c.type === 'up' || up.kind === 'slab'); }), pool = eligible.map((e, n) => e ? n : -1).filter((n) => n >= 0), fullN = fullAt >= 0 && pool.length ? pool[Math.floor(fullAt * pool.length)] : -1;
  const taken = (x, y, z, skip) => S.some((s, k) => !skip.includes(k) && Math.abs(x - s.a) < s.w / 2 + .02 && Math.abs(y - s.d) < s.dp / 2 + .02 && z > s.z - .02 && z < s.z + s.h + .02);
  C.forEach((c, n) => { const lo = S[c.lo], up = S[c.up], part = lo.part || up.part, g = { c, part, fused: false, win: null, ends: [], foot: 1, sides: [] };
    if (c.type === 'bed') { const ym = (c.a0 + c.a1) / 2, zb = c.zb + .02;
      g.sides = [['L', taken(c.a0 - .06, (c.d0 + c.d1) / 2, zb, [c.lo, c.up])], ['F', taken(ym, c.d0 - .06, zb, [c.lo, c.up])], ['R', taken(c.a1 + .06, (c.d0 + c.d1) / 2, zb, [c.lo, c.up])]].filter((s) => !s[1]).map((s) => s[0]);
      g.frontFree = g.sides.includes('F'); const lenL = c.d1 - c.d0, lenF = c.a1 - c.a0; g.off = g.sides.includes('L') ? lenL : 0; g.len = (g.sides.includes('L') ? lenL : 0) + (g.sides.includes('F') ? lenF : 0) + (g.sides.includes('R') ? lenL : 0); g.lenL = lenL; g.lenF = lenF; g.jl = lenF;
    } else { g.frontFree = !taken((lo.a + up.a) / 2, c.d0 - .06, (c.z0 + c.z1) / 2, [c.lo, c.up]); g.sides = g.frontFree ? ['F'] : []; g.len = c.z1 - c.z0; g.jl = g.len; g.foot = c.z0 < .03 ? 1 : 0; }
    const centre = c.type === 'bed' ? (c.a0 + c.a1) / 2 : c.a0, restOn = c.type === 'bed' && up.kind !== 'slab';   /* a stone resting on another (a cap, a block); a course or a half of a pillar is a slab */
    if (part) { g.fused = true; g.win = [0, g.len]; }
    else if (!noFuse && g.sides.length && g.jl >= FUSE.minLen) {
      let p = c.type === 'up' ? (g.frontFree ? FUSE.upFront : FUSE.upBack) : restOn ? FUSE.rest : (g.frontFree ? FUSE.bedCourse : FUSE.bedBack);
      if (centre < .25 * L && !g.frontFree) p = 0;     /* back joints by the cut end stay open */
      p = Math.min(.9, p * (centre > 2 * L / 3 ? FUSE.far : FUSE.near) * soft);
      const full = n === fullN, roll = r();
      if (full) { g.fused = true; g.win = [0, g.len]; }
      else if (roll < p) { const share = within(r, ...FUSE.share), w = share * g.len; let u0;
        if (c.type === 'up') { const k = r(); u0 = k < FUSE.pillarFoot ? 0 : k < FUSE.pillarFoot + FUSE.pillarMid ? within(r, .1 * g.len, Math.max(.1 * g.len, .5 * g.len - w)) : g.len - w; }
        else u0 = r() < .25 ? 0 : g.len - w;
        g.win = [u0, u0 + w]; g.fused = true;
        for (const e of [0, 1]) { const inner = e ? g.win[1] < g.len - 1e-6 : g.win[0] > 1e-6; if (inner) g.ends[e] = r() < FUSE.ramp ? { ramp: within(r, ...FUSE.rampLen) } : { notch: 1 }; } }
    }
    out.push(g); });
  return out;
}

/* ---- the filler pieces ---- */
const sample = (a, b, step) => { const n = Math.max(1, Math.ceil((b - a) / step)), o = []; for (let i = 0; i <= n; i++) o.push(a + (b - a) * i / n); return o; };
const FACTS = new WeakMap(), stoneFacts = (s) => { let p = FACTS.get(s); if (!p) FACTS.set(s, p = stoneParams(s)); return p; };
/* where a stone's plan faces stand at height z: its battered, tapered and leaning sides */
const faceA = (s, z, hi) => { const P = stoneFacts(s); let t = (z - s.z) / s.h; t = t < 0 ? 0 : t > 1 ? 1 : t; return s.a + (hi ? P.hx : -P.hx) * lerp(1, P.ta, t) + P.sa0 * t + s.leanA * (z - s.z); };
const faceD = (s, z, back) => { const P = stoneFacts(s); let t = (z - s.z) / s.h; t = t < 0 ? 0 : t > 1 ? 1 : t; return s.d + (back ? P.hy : -P.hy) * lerp(1, P.tdp, t) + P.sd0 * t + s.leanD * (z - s.z); };
/* a chamfer or a corner cut on the stone's front, or on the side facing the joint, means the edge there is not a rounded gap a filler could fill */
const chamfered = (s, towards) => stoneFacts(s).cuts.some((t) => (t.n[1] < -.01 && (t.n[2] > .01 || Math.abs(t.n[0]) > .01)) || (t.n[2] > .01 && t.n[0] * towards > .01) || (t.n[2] < .01 && t.n[0] * towards > .01 && t.n[1] < 0));
const topAt = (s, a, d) => s.z + s.h + s.dipA * (a - s.a) + s.dipD * (d - s.d);
/* a closed prism through rings of four points; each of its four sides has vertices of its own, so its faces shade flat */
function loft(rings) {
  const P = [], idx = [], quad = (a, b, c, d) => { const n = P.length; P.push(a, b, c, d); idx.push(n, n + 1, n + 2, n, n + 2, n + 3); };
  for (let i = 0; i + 1 < rings.length; i++) for (let k = 0; k < 4; k++) quad(rings[i][k], rings[i][(k + 1) % 4], rings[i + 1][(k + 1) % 4], rings[i + 1][k]);
  const r0 = rings[0], r1 = rings[rings.length - 1]; quad(r0[0], r0[1], r0[2], r0[3]); quad(r1[0], r1[1], r1[2], r1[3]);
  return { P, idx, filler: true };
}
/* how far the filler's front has drawn back from the face at distance e before an end of the fused stretch (0 on the face, `depth` buried) */
const drawn = (end, e, depth) => (end && end.ramp ? depth * Math.max(0, 1 - e / end.ramp) : 0);
function fillers(f) {
  const S = f.stones, C = contactsOf(f), G = settle(f, S, C, false), out = [];
  G.forEach((g) => { if (!g.win || g.part) return; const c = g.c, lo = S[c.lo], up = S[c.up], [u0, u1] = g.win;
    if (c.type === 'up') {
      if (chamfered(lo, 1) || chamfered(up, -1)) return;
      const dm = (Math.max(dLo(lo), dLo(up)) + Math.min(dHi(lo), dHi(up))) / 2, top = Math.min(topAt(lo, c.a0, dm), topAt(up, c.a0, dm)), zlo = c.z0 + u0, topEnd = u1 >= g.len - 1e-6, zhi = c.z0 + u1;
      const rT = Math.min(stoneFacts(lo).rT, stoneFacts(up).rT), rings = [], depth0 = Math.min(.12, .8 * Math.min(lo.dp, up.dp));
      const zhiEff = topEnd ? Math.min(zhi, top - rT - .01) : zhi, zs = sample(zlo, Math.max(zhiEff, zlo + .001), .05); if (zhiEff - zlo < .08) return;   /* a stretch that runs to the top stops just under the rounded top edges, square */
      for (const z of zs) { const gapA = faceA(up, z, 0) - faceA(lo, z, 1); if (gapA < -.03 || gapA > .06 || Math.abs(faceD(lo, z, 0) - faceD(up, z, 0)) > .2) { if (rings.length) break; continue; }   /* where the faces draw apart (a battered side) the stones are not touching: no filler */
        const fa = faceD(lo, z, 0), fb = faceD(up, z, 0), fr = Math.max(fa, fb), back = Math.min(faceD(lo, z, 1), faceD(up, z, 1)), depth = Math.min(depth0, back - fr - .005); if (depth < .05) continue;
        const rh = Math.min(stoneFacts(lo).rh, stoneFacts(up).rh), loFwd = fa < fb - .01, upFwd = fb < fa - .01;
        const aL = faceA(lo, z, 1) - (loFwd ? .01 : rh + .005), aR = faceA(up, z, 0) + (upFwd ? .01 : rh + .005);
        let o = 0;
        if (u0 > 1e-6) o = Math.max(o, drawn(g.ends[0], z - zlo, depth)); if (!topEnd) o = Math.max(o, drawn(g.ends[1], zhi - z, depth));
        o = Math.min(o, depth - .01); rings.push([[aL, fr + o, z], [aR, fr + o, z], [aR, fr + depth, z], [aL, fr + depth, z]]); }
      if (rings.length > 1) out.push(loft(rings));
    } else {
      if ([lo, up].some((s) => stoneFacts(s).cuts.some((t) => t.n[1] < -.01 && (t.n[2] > .01 || Math.abs(t.n[2]) < .01)))) return;   /* a chamfered front edge or a cut front corner */
      const rh = .07, a0 = c.a0 + rh, a1 = c.a1 - rh, off = g.off, lenF = g.lenF; if (!g.sides.includes('F') || a1 - a0 < .1) return;
      const t0 = Math.max(u0 - off, 0), t1 = Math.min(u1 - off, lenF); if (t1 - t0 < .12) return;
      const aS = Math.max(a0, c.a0 + t0), aE = Math.min(a1, c.a0 + t1), e0 = u0 > off + 1e-6 && u0 - off > rh ? g.ends[0] : null, e1 = u1 < off + lenF - 1e-6 && off + lenF - u1 > rh ? g.ends[1] : null, rings = [];
      const rTl = stoneFacts(lo).rT, rBu = stoneFacts(up).rB;
      for (const a of sample(aS, aE, .05)) { const dd = Math.max(faceD(lo, lo.z + lo.h, 0), faceD(up, up.z, 0)), zbL = topAt(lo, a, dd), zbU = up.z + up.botA * (a - up.a), zb = (zbL + zbU) / 2;
        const zf = zbL - .005; const fa = faceD(lo, zf, 0), fb = faceD(up, zbU + .005, 0), fr = Math.max(fa, fb), back = Math.min(faceD(lo, zbL, 1), faceD(up, zbU, 1)), depth = Math.min(.12, back - fr - .005); if (depth < .05) continue; if (fb < fa - .03 || Math.abs(zbU - zbL) > .05) { if (rings.length) break; continue; }   /* a stone standing forward of the one under it, or a bed that opens, is left as it is */
        const recessed = fb > fa + .03, zlo = recessed ? zbL - .012 : zbL - rTl - .005, zhi = zbU + rBu + .005;
        let o = 0; if (e0) o = Math.max(o, drawn(e0, a - aS, depth)); if (e1) o = Math.max(o, drawn(e1, aE - a, depth)); o = Math.min(o, depth - .01);
        rings.push([[a, fr + o, zlo], [a, fr + o, zhi], [a, fr + depth, zhi], [a, fr + depth, zlo]]); }
      if (rings.length > 1) out.push(loft(rings)); } });
  return out;
}
