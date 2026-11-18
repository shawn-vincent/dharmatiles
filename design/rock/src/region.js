/* region.js — owns stage 1, Outline cuts, and the box they cut. Takes the box and the cuts' settings; hands on one Convex,
   the region every other stage fills. The cuts are drawn at full count and switched on by strength, so raising a
   zone only adds cuts and never moves those laid.
   SEAT RULE: a cut that would leave the stone under a quarter of its footprint is refused (counted, never clamped). */
var Stone = Stone || {};
(function (S) {
  const { norm, dot } = S.vec, { Convex } = S.convex, PI = Math.PI;
  const SEAT_SHARE = 0.25;         // the stone keeps at least this share of its footprint on the ground: a printability rule, not a setting
  const CLUSTER = 0.8;             // chance that the next cut lands beside one already laid: cuts cluster at low counts and spread at full
  // each zone: how many slots (cut positions round the ring), the range of its normals' elevation in degrees, and the z lean aim pulls toward
  const ZONES = [
    { id: 'above', slots: 8, elev: [12, 75], dz: 1 },
    { id: 'around', slots: 12, elev: [-12, 30], dz: 0 },
    { id: 'below', slots: 16, elev: [-55, -25], dz: -1 }
  ];
  /* how the cut slots are shared between the top, the sides and the bottom: fixed (they were the defaults of the retired controls `above`, `around`, `below`), so every cut is drawn as it always was; what a cut may reach is the three preserve limits' */
  const SLOT_WEIGHTS = { above: 1, around: 0.5, below: 0.25 };
  const counts = (cfg) => { const by = {}; let total = 0; for (const z of ZONES) { by[z.id] = cfg.strength > 0 ? Math.ceil(SLOT_WEIGHTS[z.id] * cfg.strength * z.slots) : 0; total += by[z.id]; } return { by, total }; };
  const box = (size) => Convex.box([-size[0] / 2, -size[1] / 2, 0], [size[0] / 2, size[1] / 2, size[2]], { seatFloor: SEAT_SHARE * size[0] * size[1] });

  // the order slots are laid in: the first anywhere, then each beside one laid (CLUSTER) or anywhere free
  function clusterOrder(n, u) {
    const free = []; for (let i = 0; i < n; i++) free.push(i);
    const pick = (arr, r) => arr[Math.min(arr.length - 1, (r * arr.length) | 0)], out = [free.splice((u(0, 0) * n) | 0, 1)[0]];
    for (let step = 1; free.length; step++) {
      const adj = free.filter((f) => out.some((o) => Math.min((f - o + n) % n, (o - f + n) % n) === 1));
      const c = adj.length && u(step, 0) < CLUSTER ? pick(adj, u(step, 1)) : pick(free, u(step, 1));
      free.splice(free.indexOf(c), 1); out.push(c);
    }
    return out;
  }
  /* the region: the box cut by the laid cuts. Returns {convex, refused}. */
  /* the box of a convex: its bounding box in the convex's own frame. size and centre are read off it, so a cell is cut like a box. */
  function boundsOf(cv) { const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity]; for (const p of cv.vertices()) for (let a = 0; a < 3; a++) { lo[a] = Math.min(lo[a], p[a]); hi[a] = Math.max(hi[a], p[a]); } return { size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]], centre: [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2] }; }
  /* THE RULE FOR HELD FACES (one rule, every course). A held bed or top (stone on stone) may have its EDGES and CORNERS cut, never its BEARING CORE; a head joint (a held face that is vertical: nothing bears on it) is as free as an outer face and is cut like one. The core is the patch the neighbour rests on (a top or bed: what the stone above or below
     touches, given by the wall as `bear`; the whole face when no patch is given), shrunk to CORE (0.7) of itself about its centre; a face that points up keeps all of its patch (CORE_UP 1: a notch in a top is a flat roof under the stone above). A cut that takes any part of a core is refused; a cut that stays outside every core is allowed, however many held faces it meets.
     Two more things hold for every cut, held or not. It must open to the air: the face it makes must meet a surface face or a head joint along an edge (a cut that touches only held faces would leave a pocket no air reaches). And a cut that meets a held bed (a joint
     facing down) makes a face that points down, so it is tilted until that face is at most OVER (45 degrees) from vertical, inside the gate's 55, by construction. A stone with no held joint (the rock alone, the seat) meets none of this: nothing changes for it. */
  const BEARMAX = 30, CORE = 0.7, CORE_UP = 1.0, OVER = 45 * PI / 180;
  const coreOf = (g) => { const P = g.bear && g.bear.length >= 3 ? g.bear : g.pts, k = g.n[2] > 0.5 ? CORE_UP : CORE, c = [0, 0, 0]; for (const p of P) for (let a = 0; a < 3; a++) c[a] += p[a] / P.length; return P.map((p) => [c[0] + (p[0] - c[0]) * k, c[1] + (p[1] - c[1]) * k, c[2] + (p[2] - c[2]) * k]); };
  const caps = (cv) => { const cap = cv.faces[cv.faces.length - 1], byId = new Map(cv.faces.map((f) => [f.id, f])); return { cap, nb: cap.lab.map((id) => byId.get(id)).filter(Boolean) }; };
  /* the verdict on a cut that was made: 'ok', or why not */
  function verdict(cv, st) { const { cap, nb } = caps(cv);
    const side = (g) => g.role === 'joint' && cv.held(g) && Math.abs(g.n[2]) <= 0.5;   /* a head joint, between neighbours of one course: nothing bears on it, so it is as free as an outer face */
    if (!nb.some((g) => g.role === 'surface' || side(g))) return 'noSurface';
    if (nb.some((g) => g.role === 'joint' && typeof g.nb === 'string' && g.nb.startsWith('gseam:'))) return 'seam';   /* a crack between the parts of one stone is not an edge of it */
    const held = nb.filter((g) => g.role === 'joint' && cv.held(g) && !side(g));
    for (const g of held) for (const v of coreOf(g)) if (dot(cap.n, v) > cap.d + 1e-7) return 'core';
    if (held.some((g) => g.n[2] < -0.5) && cap.n[2] < -Math.sin(OVER) - 1e-9) return 'tilt';
    return 'ok'; }
  /* PRESERVE: how deep into a face a cut may go. A cut touches a face if the material it takes (the old corners it removes and its own face) reaches the face's plane; its depth into that face is the farthest that material lies inside it. A top face (normal up), a bottom face (down) and a side face (the rest) have each a limit (1 - preserve) of the stone's height, height, or half-width; 0% is no limit, 100% is none allowed. The ordinary cuts only: the bear outline's cuts always run. */
  function preserveLimits(b, cfg) { const bb = boundsOf(b), pt = +cfg.preserveTop || 0, ps = +cfg.preserveSides || 0, pb = +cfg.preserveBottom || 0; if (!(pt > 0 || ps > 0 || pb > 0)) return null;
    return { top: pt > 0 ? (1 - pt / 100) * bb.size[2] : null, sides: ps > 0 ? (1 - ps / 100) * Math.min(bb.size[0], bb.size[1]) / 2 : null, bottom: pb > 0 ? (1 - pb / 100) * bb.size[2] : null }; }
  function violations(prev, next, lim) { const cap = next.faces[next.faces.length - 1], rem = prev.vertices().filter((v) => dot(cap.n, v) > cap.d + 1e-9).concat(cap.pts), bad = new Set();
    for (const f of prev.faces) { const k = f.n[2] > 0.5 ? 'top' : f.n[2] < -0.5 ? 'bottom' : 'sides'; if (lim[k] == null) continue; let touch = false, depth = 0;
      for (const v of rem) { const g = f.d - dot(f.n, v); if (Math.abs(g) < 1e-6) touch = true; if (g > depth) depth = g; }
      if (touch && depth > lim[k] + 1e-6) bad.add(k); }
    return bad; }
  const within = (prev, next, lim) => violations(prev, next, lim).size === 0;
  function outlineCuts(b, cfg, R) {
    const bb = boundsOf(b), size = bb.size, cnt = counts(cfg), minHalf = Math.min(...size) / 2, c = bb.centre, aim = cfg.aim, bite = cfg.bite;
    let region = b, refused = 0;
    const lim = preserveLimits(b, cfg);
    ZONES.forEach((z, zi) => {
      const order = clusterOrder(z.slots, (i, w) => R.unit('cuts', 'order', zi, i, w));
      for (let i = 0; i < cnt.by[z.id]; i++) {
        const az = ((order[i] + 0.12 * R.unit('cuts', 'azimuth', zi, i)) / z.slots) * 2 * PI, ef = R.unit('cuts', 'elevation', zi, i), dr = R.unit('cuts', 'bite', zi, i);
        const e = (z.elev[0] + (z.elev[1] - z.elev[0]) * ef) * PI / 180;
        let n = [Math.cos(az) * Math.cos(e), Math.sin(az) * Math.cos(e), Math.sin(e)];
        if (aim > 0) { const D = norm([n[0] < 0 ? -1 : 1, n[1] < 0 ? -1 : 1, z.dz]); n = norm([n[0] * (1 - aim) + D[0] * aim, n[1] * (1 - aim) + D[1] * aim, n[2] * (1 - aim) + D[2] * aim]); }
        const sup = b.support(n) - dot(n, c);                                    // how far the box reaches that way
        const t = 0.15 + Math.max(0, sup - minHalf) * Math.pow(bite, 1.1) * (0.6 + 0.4 * dr) + 0.5 * bite * dr;   // 0.15 mm is a nick; the rest grows with bite
        /* a cut that would take part of a bearing core is not dropped: its normal is turned down until the plane leaves the top (a cut at a bottom edge, 45 degrees at most from vertical), then its depth is halved until it stays outside the cores. A cut that makes a down-facing face is tilted to 45 degrees. */
        const tryCut = (nn, tt) => { let nx = region.shave(nn, tt, b), vv = nx && nx !== region ? verdict(nx) : 'ok';
          if (vv === 'tilt') { const h = Math.hypot(nn[0], nn[1]); if (h > 1e-6) { const m = [nn[0] / h * Math.cos(OVER), nn[1] / h * Math.cos(OVER), -Math.sin(OVER)]; nx = region.shave(m, tt, b); vv = nx && nx !== region ? verdict(nx) : 'ok'; if (vv === 'tilt') vv = 'core'; } }
          return [nx, vv]; };
        let [next, v] = tryCut(n, t);
        if (v === 'core') { const h = Math.hypot(n[0], n[1]); if (h > 1e-6 && n[2] > -Math.sin(30 * PI / 180)) { const lo = [n[0] / h * Math.cos(30 * PI / 180), n[1] / h * Math.cos(30 * PI / 180), -Math.sin(30 * PI / 180)]; [next, v] = tryCut(lo, t); } }
        let tc = t; for (let k = 0; k < 3 && v === 'core'; k++) { tc *= 0.5; [next, v] = tryCut(n, tc); }
        /* a cut that goes deeper into a face than its preserve allows is SHORTENED where it crosses in: turned away from the top (down, to 45 degrees at most) or from the bottom (up) by steps of 10 degrees until it no longer reaches past the limit there, so a long side cut loses its end at that face; a cut limited by the sides is redrawn shallower, its depth halved up to three times; then it is refused and counted */
        if (lim && next && v === 'ok' && next !== region) {
          const bad = () => violations(region, next, lim), n0 = n, h0 = Math.hypot(n[0], n[1]), e0 = Math.atan2(n[2], h0);
          let b0 = bad(), curN = n;
          if (b0.has('top') && h0 > 1e-6) for (let k = 1; k <= 8 && next && v === 'ok' && next !== region && b0.has('top'); k++) { const e = Math.max(-OVER, e0 - k * 10 * PI / 180), m = [n0[0] / h0 * Math.cos(e), n0[1] / h0 * Math.cos(e), Math.sin(e)]; [next, v] = tryCut(m, tc); curN = m; if (next && v === 'ok' && next !== region) b0 = bad(); }
          else if (b0.has('bottom') && h0 > 1e-6) for (let k = 1; k <= 8 && next && v === 'ok' && next !== region && b0.has('bottom'); k++) { const e = Math.min(85 * PI / 180, e0 + k * 10 * PI / 180), m = [n0[0] / h0 * Math.cos(e), n0[1] / h0 * Math.cos(e), Math.sin(e)]; [next, v] = tryCut(m, tc); curN = m; if (next && v === 'ok' && next !== region) b0 = bad(); }
          for (let k = 0; k < 3 && next && v === 'ok' && next !== region && !within(region, next, lim); k++) { tc *= 0.5; [next, v] = tryCut(curN, tc); }
          if (next && v === 'ok' && next !== region && !within(region, next, lim)) v = 'preserve';
        }
        const ST = S.region.stat; if (ST) { const q = ST[z.id] = ST[z.id] || { tried: 0, ok: 0, guard: 0, noSurface: 0, core: 0, tilt: 0, seam: 0, preserve: 0 }; q.tried++; if (!next) q.guard++; else if (v === 'ok') q.ok++; else q[v]++; }
        if (next && v === 'ok') region = next; else refused++;
      }
    });
    /* THE BEAR OUTLINE: when a held bed carries a `bear` polygon (convex, counter-clockwise, in plan: the outline the bed must lie within), the same pass adds, for every edge of it, one cut through that edge at the bed's height that rises outward at an elevation drawn from the bottom cuts' own spread (the 'below' zone, clamped to the OVER a cut on a held bed keeps), so the bed ends inside the base and the stone widens above it. Mandatory: it is not subject to the seat floor (the wall's minimum support is the guard), only to degenerate geometry, and a refusal is counted, never clamped; an edge outside the area cuts nothing; a plane that climbs past the top takes that part of the top too (counted). No bear on the bed: nothing here runs. */
    const bed = b.faces.filter((f) => f.n[2] < -0.9 && f.bear && f.bear.length >= 3).sort((p, q) => q.pts.length - p.pts.length)[0], base = bed && bed.bear.map((p) => [p[0], p[1]]);
    if (base) {
      const top = Math.max(...b.vertices().map((v) => v[2])), ST = S.region.stat;
      for (let i = 0; i < base.length; i++) {
        const p0 = base[i], p1 = base[(i + 1) % base.length], ex = p1[0] - p0[0], ey = p1[1] - p0[1], l = Math.hypot(ex, ey); if (l < 1e-6) continue;
        const e = Math.min(BEARMAX, 25 + 30 * R.unit('cuts', 'base', i)) * PI / 180, nx = ey / l, ny = -ex / l, n = [nx * Math.cos(e), ny * Math.cos(e), -Math.sin(e)];
        const mx = (p0[0] + p1[0]) / 2, my = (p0[1] + p1[1]) / 2, zb = (bed.d - bed.n[0] * mx - bed.n[1] * my) / bed.n[2], d = n[0] * mx + n[1] * my + n[2] * zb;
        if (ST) { ST.base = ST.base || { edges: 0, cut: 0, top: 0, refused: 0 }; ST.base.edges++; }
        const g = region.guard, next = region.withGuard(Object.assign({}, g, { seatFloor: 0 })).cut(n, d, 'surface', null);
        if (!next) { refused++; if (ST) ST.base.refused++; continue; }
        const cap = next.faces[next.faces.length - 1];
        if (ST && cap && cap.pts.some((v) => v[2] >= top - 1e-6)) ST.base.top++;
        if (ST) ST.base.cut++;
        region = next.withGuard(g);
      }
    }
    return { convex: region, refused };
  }
  S.region = { ZONES, SEAT_SHARE, counts, box, boundsOf, outlineCuts };
})(Stone);
