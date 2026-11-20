/* ---------- fused faces ----------
   Most joints between two stones stay what the stones make them: the natural gap between two rounded edges. A joint that has fused
   (chapter 8's chance: which contacts) is made by growing the two stones toward each other when
   they are built, so that they overlap: the stones are then simply a union, no material is added and nothing is cut. An upright joint
   grows each stone sideways by its edge radius and a little more, so each stone's flat face runs on across the other's rounded edge and the
   two faces are one; a bed grows the upper stone downward by the two edge radii and a little more, so its front face runs down over the
   lower stone's rounded top edge. Where the faces are not on one plane the overlap leaves a sharp inside corner. */

/* ---- contacts: where two stones touch ---- */
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

/* ---- which contacts fuse: chapter 8's chance ---- */
const FUSE = { up: [.20, .07], course: [.30, .04], rest: [.03, .03], far: 2, near: .7, minLen: .4 };   /* chance in front / behind; times 2 in the far third of the length, 0.7 elsewhere, times a softness drawn once */
function fusedContacts(f) {
  const S = f.stones, r = rng(f.q.seed * 7919 + 13), soft = within(r, .5, 1.5), L = f.q.length, hidden = (x, y, z, skip) => S.some((s, k) => !skip.includes(k) && Math.abs(x - s.a) < s.w / 2 + .02 && Math.abs(y - s.d) < s.dp / 2 + .02 && z > s.z - .02 && z < s.z + s.h + .02);
  return contactsOf(f).filter((c) => { const lo = S[c.lo], up = S[c.up];
    const bed = c.type === 'bed', len = bed ? c.a1 - c.a0 : c.z1 - c.z0, at = bed ? (c.a0 + c.a1) / 2 : c.a0; if (len < FUSE.minLen) return false;
    const front = !hidden(bed ? at : (lo.a + up.a) / 2, c.d0 - .06, bed ? c.zb + .02 : (c.z0 + c.z1) / 2, [c.lo, c.up]), kind = !bed ? FUSE.up : up.kind === 'slab' ? FUSE.course : FUSE.rest;
    if (at < .25 * L && !front) return false;   /* back joints by the cut end stay open */
    return r() < Math.min(.9, kind[front ? 0 : 1] * (at > 2 * L / 3 ? FUSE.far : FUSE.near) * soft); });
}
/* the stones as drawn: those on a fused joint grown toward each other (the whole joint) */
function grownStones(f) {
  const S = f.stones.map((s) => Object.assign({}, s)), P = f.stones.map(stoneParams);
  fusedContacts(f).forEach((c) => { const lo = S[c.lo], up = S[c.up];
    if (c.type === 'up') { const e = Math.max(P[c.lo].rh, P[c.up].rh) + .01; lo.w += e; lo.a += e / 2; up.w += e; up.a -= e / 2; }
    else { const e = P[c.up].rB + P[c.lo].rT + .01; up.z -= e; up.h += e; } });
  return S;
}
