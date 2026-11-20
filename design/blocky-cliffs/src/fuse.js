/* ---------- fused faces ----------
   Most joints between two stones stay what the stones make them: the natural gap between two rounded edges. A joint that has fused
   (chapter 8's rule: which contacts, how much of the length, where, how it ends) is made by growing the two stones toward each other when
   they are built, so that they overlap: the stones are then simply a union, no material is added and nothing is cut. An upright joint
   grows each stone sideways by its edge radius and a little more, so each stone's flat face runs on across the other's rounded edge and the
   two faces are one; a bed grows the upper stone downward by the two edge radii and a little more, so its front face runs down over the
   lower stone's rounded top edge. Where the faces are not on one plane the overlap leaves a sharp inside corner. */

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


/* the stones as they are drawn: those on a fused joint grown toward each other (the whole stretch; chapter 8's ends and places are not drawn) */
function grownStones(f) {
  const S = f.stones.map((s) => Object.assign({}, s)), C = contactsOf(f), G = settle(f, f.stones, C, false), P = f.stones.map(stoneParams);
  G.forEach((g) => { if (!g.win || g.part) return; const c = g.c, lo = S[c.lo], up = S[c.up];
    if (c.type === 'up') { const e = Math.max(P[c.lo].rh, P[c.up].rh) + .01; lo.w += e; lo.a += e / 2; up.w += e; up.a -= e / 2; }
    else { const e = P[c.up].rB + P[c.lo].rT + .01; up.z -= e; up.h += e; } });
  return S;
}
