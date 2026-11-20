/* ---------- one solid per formation ----------
   Each stone is a signed distance field (the rounded box of stoneParams, taken through the inverse of the stone's batter, dip, lean and yaw,
   cut softly by its planes). The stones are joined by a plain union (every concave corner stays sharp), and every contact between two stones has its own setting:
   - a bridge fills the slot between them where they touch, so a bed is one body with no lip under the upper stone;
   - a V groove (about 0.04 fh deep, flanks at 45 degrees) is cut along the free edge of the bridge to mark the joint;
   - the groove's depth along the joint is a profile: full, or nothing where the joint has died (fused), ramping to nothing or ending in a notch.
   The field is polygonised on a grid with surface nets, evaluating finely only in a narrow band round the surface. */
const SF = { fp: 0 };                    /* where a stone's field leaves its plan footprint's distance */
const GROOVE = { bed: .05, up: .06 };   /* a groove's full depth, fh */
const smax = (a, b, k) => (a + b + Math.sqrt((a - b) * (a - b) + k * k)) / 2;

/* a stone's field: ev(x, y, z) is its signed distance (negative inside) and leaves the plan footprint's distance in SF.fp */
function stoneField(o) {
  const P = stoneParams(o), { q, hx, hy, hz, rh, rT, rB, cuts, ta, tdp, sa0, sd0, c, s2, cu } = P, vcuts = cuts.filter((t) => t.n[2] === 0);
  const pts = stoneMesh(o).P, box = [1e9, 1e9, 1e9, -1e9, -1e9, -1e9];
  for (const p of pts) for (let k = 0; k < 3; k++) { if (p[k] < box[k]) box[k] = p[k]; if (p[k] > box[k + 3]) box[k + 3] = p[k]; }
  const h = q.h, ks = Math.min(1, ta, tdp) / Math.sqrt(1 + q.leanA * q.leanA + q.leanD * q.leanD + q.dipA * q.dipA + q.dipD * q.dipD + q.botA * q.botA), ev = (X, Y, Z) => {
    const x = X - q.a, y = Y - q.d, zz = Z - q.z; let a = x * c + y * s2, d = -x * s2 + y * c; a -= q.leanA * zz; d -= q.leanD * zz;
    let t = zz / h; t = t < 0 ? 0 : t > 1 ? 1 : t; const zb = zz - h / 2; let sa = lerp(1, ta, t), sd = lerp(1, tdp, t), zl = zb - t * (q.dipA * a + q.dipD * d) - (1 - t) * q.botA * a;
    if (q.dipA || q.dipD || q.botA) { t = zl / h + .5; t = t < 0 ? 0 : t > 1 ? 1 : t; sa = lerp(1, ta, t); sd = lerp(1, tdp, t); zl = zb - t * (q.dipA * a + q.dipD * d) - (1 - t) * q.botA * a; }
    const al = (a - sa0 * t) / sa, dl = (d - sd0 * t) / sd, zr = zl / hz, zq = 1 - (zr > 1 ? 1 : zr < -1 ? 1 : zr * zr), rr = dl / hy, ra = al / hx;
    const hxe = hx + cu * (rr * rr < 1 ? 1 - rr * rr : 0) * zq, hye0 = hy + cu * (ra * ra < 1 ? 1 - ra * ra : 0) * zq;
    let hyF = hye0; if (q.crease && dl < 0) { const u = Math.max(0, zr); hyF += q.crease * Math.max(0, 1 - Math.abs(al) / hx * 1.4) * (1 - u * u * u * u); }
    const qx = (al < 0 ? -al : al) - (hxe - rh), qy = (dl < 0 ? -dl : dl) - ((dl < 0 ? hyF : hye0) - rh), mx = qx > 0 ? qx : 0, my = qy > 0 ? qy : 0;
    let fp = Math.sqrt(mx * mx + my * my) + Math.min(Math.max(qx, qy), 0) - rh;
    for (let i = 0; i < vcuts.length; i++) { const ct = vcuts[i], u = ct.n[0] * al + ct.n[1] * dl - ct.d; if (u > fp - .15) fp = smax(fp, u, .035); }
    const rz = zl > 0 ? rT : rB, qz = zl > 0 ? zl - (hz - rT) : -zl - (hz - rB), mz = qz > 0 ? qz : 0, ex = mx / rh, ey = my / rh, ez = mz / rz, l = Math.sqrt(ex * ex + ey * ey + ez * ez);
    let dist = l > 0 ? (l - 1) * ((rh * (ex + ey) + rz * ez) / (ex + ey + ez)) : Math.max(qx - rh, qy - rh, qz - rz);
    for (let i = 0; i < cuts.length; i++) { const ct = cuts[i], u = ct.n[0] * al + ct.n[1] * dl + ct.n[2] * zl - ct.d; if (u > dist - .15) dist = smax(dist, u, .035); }
    SF.fp = fp * ks; return dist * ks; };
  /* where the stone's front and back faces stand at height Z (the plan faces, rounded edges and cushion left out) */
  ev.face = (Z, back) => { let t = (Z - q.z) / h; t = t < 0 ? 0 : t > 1 ? 1 : t; return q.d + (back ? hy * lerp(1, tdp, t) : -hy * lerp(1, tdp, t)) + sd0 * t + q.leanD * (Z - q.z); };
  ev.box = box; ev.o = o; return ev;
}

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
function settle(f, S, C, fields, noFuse) {
  const r = rng(f.q.seed * 7919 + 13), soft = within(r, .5, 1.5), L = f.q.length, fullAt = r() < FUSE.full ? r() : -1, out = [];
  const eligible = C.map((c, n) => { const lo = S[c.lo], up = S[c.up]; return !lo.part && !up.part && (c.type === 'bed' ? c.a1 - c.a0 : c.z1 - c.z0) >= FUSE.minLen && (c.type === 'up' || up.kind === 'slab'); }), pool = eligible.map((e, n) => e ? n : -1).filter((n) => n >= 0), fullN = fullAt >= 0 && pool.length ? pool[Math.floor(fullAt * pool.length)] : -1;
  const taken = (x, y, z, skip) => S.some((s, k) => !skip.includes(k) && fields[k](x, y, z) < .02);
  C.forEach((c, n) => { const lo = S[c.lo], up = S[c.up], part = lo.part || up.part, g = { c, part, D: GROOVE[c.type], fused: false, win: null, ends: [], foot: 1, sides: [] };
    if (c.type === 'bed') { const ym = (c.a0 + c.a1) / 2, zb = c.zb + .02;
      g.sides = [['L', taken(c.a0 - .06, (c.d0 + c.d1) / 2, zb, [c.lo, c.up])], ['F', taken(ym, c.d0 - .06, zb, [c.lo, c.up])], ['R', taken(c.a1 + .06, (c.d0 + c.d1) / 2, zb, [c.lo, c.up])]].filter((s) => !s[1]).map((s) => s[0]);
      g.frontFree = g.sides.includes('F'); const lenL = c.d1 - c.d0, lenF = c.a1 - c.a0; g.off = g.sides.includes('L') ? lenL : 0; g.len = (g.sides.includes('L') ? lenL : 0) + (g.sides.includes('F') ? lenF : 0) + (g.sides.includes('R') ? lenL : 0); g.lenL = lenL; g.lenF = lenF; g.jl = lenF;
    } else { g.frontFree = !taken((lo.a + up.a) / 2, c.d0 - .06, (c.z0 + c.z1) / 2, [c.lo, c.up]); g.sides = g.frontFree ? ['F'] : []; g.len = c.z1 - c.z0; g.jl = g.len; g.foot = c.z0 < .03 ? 1 : 0; }
    const centre = c.type === 'bed' ? (c.a0 + c.a1) / 2 : c.a0, restOn = c.type === 'bed' && up.kind !== 'slab';   /* a stone resting on another (a cap, a block); a course or a half of a pillar is a slab */
    if (part) { g.fused = true; g.win = [0, g.len]; g.D = 0; }
    else if (!noFuse && g.sides.length && g.jl >= FUSE.minLen) {
      let p = c.type === 'up' ? (g.frontFree ? FUSE.upFront : FUSE.upBack) : restOn ? FUSE.rest : (g.frontFree ? FUSE.bedCourse : FUSE.bedBack);
      if (centre < .25 * L && !g.frontFree) p = 0;     /* back joints by the cut end stay open */
      p = Math.min(.9, p * (centre > 2 * L / 3 ? FUSE.far : FUSE.near) * soft);
      const full = n === fullN, roll = r();
      if (full) { g.fused = true; g.win = [0, g.len]; g.D = 0; }
      else if (roll < p) { const share = within(r, ...FUSE.share), w = share * g.len; let u0;
        if (c.type === 'up') { const k = r(); u0 = k < FUSE.pillarFoot ? 0 : k < FUSE.pillarFoot + FUSE.pillarMid ? within(r, .1 * g.len, Math.max(.1 * g.len, .5 * g.len - w)) : g.len - w; }
        else u0 = r() < .25 ? 0 : g.len - w;
        g.win = [u0, u0 + w]; g.fused = true;
        for (const e of [0, 1]) { const inner = e ? g.win[1] < g.len - 1e-6 : g.win[0] > 1e-6; if (inner) g.ends[e] = r() < FUSE.ramp ? { ramp: within(r, ...FUSE.rampLen) } : { notch: 1 }; } }
      if (c.type === 'up' && g.foot && !g.fused) g.footD = r() < .3 ? .1 : within(r, .5, .8);
      else if (c.type === 'up' && g.foot && g.win && g.win[0] > 1e-6) g.footD = r() < .3 ? .1 : within(r, .5, .8);
    }
    if (!g.fused && !g.win && c.type === 'up' && g.foot) g.footD = r() < .3 ? .1 : within(r, .5, .8);
    out.push(g); });
  return out;
}
/* the groove's depth at distance s along the joint */
function grooveDepth(g, s) {
  if (g.part) return 0; let D = g.D;
  if (g.win) { const [u0, u1] = g.win; if (s >= u0 && s <= u1) return 0; const e = s < u0 ? g.ends[0] : g.ends[1], dist = s < u0 ? u0 - s : s - u1;
    if (e && e.ramp) D *= Math.min(1, dist / e.ramp); else if (e && e.notch && dist < .5 * D) D *= 1.6; else if (!e) { /* the window runs to the joint's end */ } }
  if (g.footD != null && g.c.type === 'up') { const t = Math.min(1, s / .25); D *= g.footD + (1 - g.footD) * t; }
  return D;
}

/* ---- the solid ---- */
function formSolid(f, opt = {}) {
  const S = f.stones, fields = S.map(stoneField), N = S.length, C = contactsOf(f), G = opt.plain ? [] : settle(f, S, C, fields, opt.nofuse), E = .09;
  const H = opt.h || .035, M = .1, MS = .04;
  let x0 = 1e9, y0 = 1e9, z0 = 1e9, x1 = -1e9, y1 = -1e9, z1 = -1e9; for (const ev of fields) { const b = ev.box; x0 = Math.min(x0, b[0]); y0 = Math.min(y0, b[1]); z0 = Math.min(z0, b[2]); x1 = Math.max(x1, b[3]); y1 = Math.max(y1, b[4]); z1 = Math.max(z1, b[5]); }
  x0 -= .06; y0 -= .06; z0 -= .06; x1 += .06; y1 += .06; z1 += .06;
  const dd = new Float64Array(N), fq = new Float64Array(N), near = new Uint8Array(N);
  const cb = G.map((g) => { const a = fields[g.c.lo].box, b = fields[g.c.up].box; return [Math.max(a[0], b[0]) - M, Math.max(a[1], b[1]) - M, Math.max(a[2], b[2]) - M, Math.min(a[3], b[3]) + M, Math.min(a[4], b[4]) + M, Math.min(a[5], b[5]) + M]; });
    const field = (x, y, z, wide) => {
    const MG = wide ? M : MS; let F = MG, any = false;
    for (let i = 0; i < N; i++) { const b = fields[i].box; if (x < b[0] - MG || x > b[3] + MG || y < b[1] - MG || y > b[4] + MG || z < b[2] - MG || z > b[5] + MG) { near[i] = 0; continue; } near[i] = 1; dd[i] = fields[i](x, y, z); fq[i] = SF.fp; F = any ? Math.min(F, dd[i]) : dd[i]; any = true; }
    if (!any) return MG; if (F > MG) F = MG;
    let Bm = MG, Gm = -MG;
    for (let n = 0; n < G.length; n++) { const g = G[n], c = g.c, b = cb[n]; if (x < b[0] || x > b[3] || y < b[1] || y > b[4] || z < b[2] || z > b[5]) continue;
      const li = c.lo, ui = c.up, lo = S[li], up = S[ui]; let mask, xin, s, dl; if (!near[li]) { near[li] = 1; dd[li] = fields[li](x, y, z); fq[li] = SF.fp; } if (!near[ui]) { near[ui] = 1; dd[ui] = fields[ui](x, y, z); fq[ui] = SF.fp; }
      if (c.type === 'bed') { mask = Math.max(fq[li], fq[ui]); xin = -mask; const a = x - up.leanA * (z - up.z), side = g.sides;
        dl = z - ((lo.z + lo.h + lo.dipA * (x - lo.a) + lo.dipD * (y - lo.d)) + (up.z + up.botA * (x - up.a))) / 2;   /* height above the bed plane */
        if (side.length) { const xl = a - c.a0, xr = c.a1 - a, xf = y - c.d0; let best = 1e9, sv = 0;
          if (side.includes('L') && xl < best) { best = xl; sv = c.d1 - y; } if (side.includes('F') && xf < best) { best = xf; sv = g.off + Math.max(0, Math.min(g.lenF, a - c.a0)); } if (side.includes('R') && xr < best) { best = xr; sv = g.off + (side.includes('F') ? g.lenF : 0) + (y - c.d0); }
          s = Math.max(0, sv); } else s = 0; }
      else { const fr = Math.max(fields[li].face(z, 0), fields[ui].face(z, 0)), bk = Math.min(fields[li].face(z, 1), fields[ui].face(z, 1)); mask = Math.max(fr - y, y - bk); xin = y - fr; s = z - c.z0; dl = x - (aHi(lo) + lo.leanA * (z - lo.z) + aLo(up) + up.leanA * (z - up.z)) / 2; }
      const zc = c.type === 'bed' ? Math.max(lo.z - z, z - up.z - up.h) : Math.max(c.z0 - z, z - c.z1), bridge = Math.max(dd[li] - E, dd[ui] - E, mask, zc); if (bridge < Bm) Bm = bridge;
      if (g.sides.length && (c.type === 'bed' || c.front)) { const D = grooveDepth(g, s); if (D > 0) { const gv = Math.min((D - Math.abs(dl) - xin) / 1.414, xin + .01); if (gv > Gm) Gm = gv; } } }
    if (Bm < F) F = Bm; if (Gm > F) F = Gm; return F; };
  /* the grid is searched as an octree: a block is dropped when its centre is further from the surface than the block is wide, so the
     field is evaluated finely only in a narrow band round the surface */
  const R = 16, gx = Math.ceil((x1 - x0) / H / R) * R, gy = Math.ceil((y1 - y0) / H / R) * R, gz = Math.ceil((z1 - z0) / H / R) * R, sx = gx + 1, sy = gy + 1;
  const val = new Float32Array(sx * sy * (gz + 1)).fill(NaN), at = (i, j, k) => (k * sy + j) * sx + i, cells = [];
  const node = (i, j, k) => { const q = at(i, j, k); let v = val[q]; if (v !== v) { v = field(x0 + i * H, y0 + j * H, z0 + k * H); val[q] = v; } return v; };
  const split = (i, j, k, b) => {
    if (b === 1) { let neg = 0; for (let c = 0; c < 8; c++) if (node(i + (c & 1), j + ((c >> 1) & 1), k + (c >> 2)) < 0) neg++; if (neg && neg < 8) cells.push(i, j, k); return; }
    { const bx0 = x0 + i * H - .02, by0 = y0 + j * H - .02, bz0 = z0 + k * H - .02, bx1 = bx0 + b * H + .04, by1 = by0 + b * H + .04, bz1 = bz0 + b * H + .04; let hit = false; for (let n = 0; n < N && !hit; n++) { const q = fields[n].box; hit = q[0] < bx1 && q[3] > bx0 && q[1] < by1 && q[4] > by0 && q[2] < bz1 && q[5] > bz0; } if (!hit) return; }
    const hb = b / 2, v = field(x0 + (i + hb) * H, y0 + (j + hb) * H, z0 + (k + hb) * H, true); if (Math.abs(v) > b * H * (opt.prune || .9)) return;
    for (let c = 0; c < 8; c++) split(i + (c & 1) * hb, j + ((c >> 1) & 1) * hb, k + (c >> 2) * hb, hb); };
  for (let k = 0; k < gz; k += R) for (let j = 0; j < gy; j += R) for (let i = 0; i < gx; i += R) split(i, j, k, R);
  return surfaceNets(node, cells, gx, gy, gz, [x0, y0, z0], H, opt.project === false ? null : field);
}

/* naive surface nets over the listed cells; a cell the search dropped but the surface needs (a neighbour across a crossed edge) is added when asked for */
function surfaceNets(node, cells, nx, ny, nz, o, H, field) {
  const P = [], idx = [], cell = new Map(), key = (i, j, k) => (k * ny + j) * nx + i, cv = new Float64Array(8), todo = cells.slice();
  const E = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
  const make = (i, j, k) => { for (let c = 0; c < 8; c++) cv[c] = node(i + (c & 1), j + ((c >> 1) & 1), k + (c >> 2));
    let px = 0, py = 0, pz = 0, m = 0;
    for (const [a, b] of E) { const va = cv[a], vb = cv[b]; if ((va < 0) === (vb < 0)) continue; const t = va / (va - vb); px += (a & 1) + ((b & 1) - (a & 1)) * t; py += ((a >> 1) & 1) + (((b >> 1) & 1) - ((a >> 1) & 1)) * t; pz += (a >> 2) + ((b >> 2) - (a >> 2)) * t; m++; }
    if (!m) return -1; let X = o[0] + (i + px / m) * H, Y = o[1] + (j + py / m) * H, Z = o[2] + (k + pz / m) * H;
    if (field) { /* one step onto the surface itself, along the gradient the cell's corners give: creases come out sharper than the averaged vertex */
      const gx = (cv[1] - cv[0] + cv[3] - cv[2] + cv[5] - cv[4] + cv[7] - cv[6]) / (4 * H), gy = (cv[2] - cv[0] + cv[3] - cv[1] + cv[6] - cv[4] + cv[7] - cv[5]) / (4 * H), gz = (cv[4] - cv[0] + cv[5] - cv[1] + cv[6] - cv[2] + cv[7] - cv[3]) / (4 * H), gg = gx * gx + gy * gy + gz * gz;
      if (gg > 1e-6) { const st = Math.max(-H, Math.min(H, field(X, Y, Z) / gg)); X -= gx * st; Y -= gy * st; Z -= gz * st; } }
    cell.set(key(i, j, k), P.length); P.push([X, Y, Z]); return P.length - 1; };
  const ensure = (i, j, k) => { const q = cell.get(key(i, j, k)); if (q !== undefined) return q; const v = make(i, j, k); if (v >= 0) todo.push(i, j, k); return v; };
  for (let n = 0; n < todo.length; n += 3) if (!cell.has(key(todo[n], todo[n + 1], todo[n + 2]))) make(todo[n], todo[n + 1], todo[n + 2]);
  for (let n = 0; n < todo.length; n += 3) { const i = todo[n], j = todo[n + 1], k = todo[n + 2];
    for (let ax = 0; ax < 3; ax++) { const u = (ax + 1) % 3, w = (ax + 2) % 3, v0 = node(i, j, k) < 0, v1 = node(i + (ax === 0), j + (ax === 1), k + (ax === 2)) < 0; if (v0 === v1) continue;
      const d = [i, j, k]; if (d[u] < 1 || d[w] < 1) continue;
      const mk = (du, dw) => { const e = [i, j, k]; e[u] += du; e[w] += dw; return ensure(e[0], e[1], e[2]); }, a = mk(-1, -1), b = mk(0, -1), c = mk(0, 0), e = mk(-1, 0);
      if (a < 0 || b < 0 || c < 0 || e < 0) continue; if (v0) idx.push(a, b, c, a, c, e); else idx.push(a, c, b, a, e, c); } }
  return { P, idx };
}
/* the downward-facing area of a mesh by angle from vertical (a horizontal underside is 90), past the plate: shares past 55, in 55 to 65, past 65 */
function overhang(meshes) {
  let tot = 0, a55 = 0, a65 = 0, over = 0;
  for (const m of meshes) for (let t = 0; t < m.idx.length; t += 3) { const A = m.P[m.idx[t]], Bp = m.P[m.idx[t + 1]], Cp = m.P[m.idx[t + 2]]; if ((A[2] + Bp[2] + Cp[2]) / 3 < .012) continue;
    const ux = Bp[0] - A[0], uy = Bp[1] - A[1], uz = Bp[2] - A[2], vx = Cp[0] - A[0], vy = Cp[1] - A[1], vz = Cp[2] - A[2], nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx, l = Math.hypot(nx, ny, nz), ar = l / 2; if (l < 1e-12) continue;
    const z = nz / l; if (z >= 0) continue; const ang = Math.asin(Math.min(1, -z)) * 180 / Math.PI; tot += ar; if (ang > 55) { over += ar; if (ang <= 65) a55 += ar; else a65 += ar; } }
  return { down: tot, band: a55, steep: a65, past55: tot ? over / tot : 0, in5565: tot ? a55 / tot : 0, past65: tot ? a65 / tot : 0 };
}
