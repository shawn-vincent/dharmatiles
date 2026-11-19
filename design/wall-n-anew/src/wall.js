/* wall.js: Wall N, anew. Built only from the recipe of "Rock at Every Scale" (section 7).
   Every piece is a convex cell cut from half-spaces; the wall is the union of the pieces and one core. z is up, mm at the 35 mm square.
   genN(seed, cfg, stage) -> { parts, ground, stats }.  stage 1..7 builds the recipe steps in order (each stage keeps the stages before it unchanged). */
(function (G) {
  'use strict';
  const E = G.Eng, rng = E.rng, norm = E.norm, cross = E.cross, dot = E.dot, add = E.add, sub = E.sub, mul = E.mul, P = E.P;
  const rad = (d) => d * Math.PI / 180, tan = Math.tan, PI = Math.PI;
  /* a random stream keyed by seed and a name, so a later stage never moves an earlier draw */
  function keyed(seed, name) { let h = (seed * 2654435761) >>> 0; for (let i = 0; i < name.length; i++) h = Math.imul(h ^ name.charCodeAt(i), 16777619) >>> 0; return rng(h); }
  const R = (r, a, b) => a + (b - a) * r();
  const dist = (a) => Math.hypot(a[0], a[1], a[2]);

  /* ------------------------------------------------------------------ the two settings */
  /* RECIPE: the numbers exactly as section 7 writes them (v1). FINAL: what was changed by eye, see notes.md. */
  const RECIPE = {
    name: 'v1',
    dip: 52, facing: 262, yf: 9, xl: 11, xr: 35, ymax: 35, topPass: [3, 13], topSlope: .94, cap: 38, dcore: 9,
    /* levels on the free face: w = width along the strike, l = length up the dip, off = setback range, p = share of cells that go on to the next level */
    front: [
      { stage: 2, w: [9, 14], off: [0, 3.8], shingle: true, lean: 3, skew: 0 },
      { stage: 2, l: [22, 36], off: [0, 2.2], carry: .12, lean: 3.5, skew: 4, tilt: 3 },
      { stage: 3, w: [8, 8], l: [18, 18], off: [0, .9], p: 1, lean: 3, skew: 3, tilt: 3 },
      { stage: 3, w: [5, 5], l: [9, 9], off: [0, .35], p: .5, lean: 3, skew: 3, tilt: 3 }
    ],
    top: [
      { stage: 2, w: [7, 11], off: [0, 1.6], lean: 3, skew: 0 },
      { stage: 2, l: [12, 20], off: [0, 1], carry: .12, lean: 3, skew: 4, tilt: 3 },
      { stage: 3, w: [5, 8], l: [8, 12], off: [0, .4], p: .5, lean: 3, skew: 3, tilt: 3 }
    ],
    end: [
      { stage: 2, w: [7, 11], off: [0, 1.6], lean: 3, skew: 0 },
      { stage: 2, l: [12, 20], off: [0, 1], carry: .12, lean: 3, skew: 4, tilt: 3 },
      { stage: 3, w: [5, 8], l: [8, 12], off: [0, .4], p: .5, lean: 3, skew: 3, tilt: 3 }
    ],
    lip: { p: .85, ang: [28, 52], depth: [.5, 1.1], frac: .18 },
    chip: { p: .30, perCell: 2, depth: [.8, 1.9], tilt: 25, up: .35 },
    groove: { p: .55, minW: 4, w: [.4, .5], depth: [.3, .6] },
    crack: { p: 0 },
    foot: { n: 11, size: [3, 6] }
  };
  const FINAL = JSON.parse(JSON.stringify(RECIPE)); FINAL.name = 'final';
  FINAL.facing = 283; FINAL.footAt = [17, 5]; FINAL.xl = 13; FINAL.cap = 33; FINAL.dip = 47; FINAL.topPass = [3, 15];            /* by eye against the photo: the free face looks at the camera, not along the strike */
  FINAL.front[1].skew = 12; FINAL.front[2].skew = 9; FINAL.front[3].skew = 9;   /* cross cuts lean in plane, like the photo's diagonal cracks */
  ['top', 'end'].forEach(k => { FINAL[k] = [{ stage: 2, w: [9, 13], off: [0, 1.4], lean: 2, skew: 0 }, { stage: 2, l: [12, 20], off: [0, .8], carry: .1, lean: 2, skew: 8 }]; });   /* crest and end are cut like the slope, but with fewer levels: three read as shards */
  FINAL.front.forEach(l => { if (l.tilt) l.tilt = 2; });
  FINAL.front[0].off = [0, 2.0]; FINAL.front[1].off = [0, 1.2]; FINAL.front[2].off = [0, .6]; FINAL.front[3].off = [0, .3];   /* the recipe's steps (3.8 + 2.2) cut canyons between 12 mm slabs; about half reads like the photo */
  FINAL.dcore = 6;
  /* round 2: chunky, not cards */
  FINAL.front[0].off = [0, 2.8]; FINAL.front[1].off = [0, 1.8]; FINAL.front[1].skew = 6; FINAL.front[2].skew = 5; FINAL.front[3].skew = 5; FINAL.front[2].off = [0, .8]; FINAL.front[3].off = [0, .4];
  FINAL.front[1].tilt = FINAL.front[2].tilt = FINAL.front[3].tilt = 1.5;
  FINAL.edge = { ang: [30, 48], dep: [.7, 1.4], side: .55, extra: 1.6 };
  FINAL.chip = { p: .5, perCell: 2, depth: [1, 2.2], tilt: 25, up: .35 };
  FINAL.foot = { n: 22, size: [2.5, 6], zone: [[17, 34, 0.8, 5.5], [1, 14, 0.8, 3], [30, 34, 5, 14]], avoid: [8, 9, 8.5] };
  FINAL.lip = { p: .85, ang: [28, 52], depth: [.8, 1.6], frac: .25 };
  G.WALLCFG = { RECIPE: RECIPE, FINAL: FINAL };

  /* ------------------------------------------------------------------ helpers */
  function tiltN(n, a, r) { return E.leaned(n, [0, 0, 0], a, r, 'f').n; }
  /* the plane of a cut at coordinate c along axis ax ('s' or 't') of face F, leaned off square by `lean` and skewed in plane by `skew` (random signs) */
  function cutPlane(F, ax, c, mid, lean, skew, r) {
    const a = ax === 's' ? F.es : F.ed, o = ax === 's' ? F.ed : F.es;
    const sl = (r() < .5 ? -1 : 1) * tan(rad(lean * (.4 + .6 * r()))), sk = (r() < .5 ? -1 : 1) * tan(rad(skew * (.3 + .7 * r())));
    const n = norm(add(add(a, mul(F.n, sl)), mul(o, sk)));
    const anchor = add(add(F.O, mul(a, c)), mul(o, mid));
    return { n: n, d: dot(n, anchor) };
  }
  const neg = (pl) => ({ n: mul(pl.n, -1), d: -pl.d });

  /* split a rect of a face into children by the level spec */
  function splitRect(F, rect, spec, r) {
    let rects = [rect];
    if (spec.w) {
      const out = []; let c = rect.s0, prev = rect.sa, ps = rect.s0, first = true;
      const widths = []; let acc = rect.s0;
      while (true) {
        const w = R(r, spec.w[0], spec.w[1]) * (first ? (.45 + .55 * r()) : 1); first = false;
        if (rect.s1 - (acc + w) < .4 * spec.w[1]) break;
        acc += w; widths.push(acc);
      }
      const tm = (rect.t0 + rect.t1) / 2;
      widths.forEach(cc => {
        const pl = cutPlane(F, 's', cc, tm, spec.lean, spec.skew, r);
        out.push({ s0: ps, s1: cc, t0: rect.t0, t1: rect.t1, sa: prev, sb: pl, ta: rect.ta, tb: rect.tb });
        prev = neg(pl); ps = cc;
      });
      out.push({ s0: ps, s1: rect.s1, t0: rect.t0, t1: rect.t1, sa: prev, sb: rect.sb, ta: rect.ta, tb: rect.tb });
      rects = out;
    }
    if (spec.l) {
      const out = []; let lastCuts = null;
      rects.forEach(rc => {
        let cuts;
        if (lastCuts && r() < (spec.carry || 0)) cuts = lastCuts.filter(c => c > rc.t0 + .4 * spec.l[0] && c < rc.t1 - .4 * spec.l[0]);
        else {
          cuts = []; let acc = rc.t0, first = true;
          while (true) {
            const l = R(r, spec.l[0], spec.l[1]) * (first ? (.5 + .5 * r()) : 1); first = false;
            if (rc.t1 - (acc + l) < .4 * spec.l[1]) break;
            acc += l; cuts.push(acc);
          }
        }
        lastCuts = cuts;
        const sm = (rc.s0 + rc.s1) / 2; let prev = rc.ta, ps = rc.t0;
        cuts.forEach(cc => {
          const pl = cutPlane(F, 't', cc, sm, spec.lean, spec.skew, r);
          out.push({ s0: rc.s0, s1: rc.s1, t0: ps, t1: cc, sa: rc.sa, sb: rc.sb, ta: prev, tb: pl });
          prev = neg(pl); ps = cc;
        });
        out.push({ s0: rc.s0, s1: rc.s1, t0: ps, t1: rc.t1, sa: rc.sa, sb: rc.sb, ta: prev, tb: rc.tb });
      });
      rects = out;
    }
    return rects;
  }

  /* ------------------------------------------------------------------ the wall */
  function genN(seed, cfg, stage) {
    cfg = cfg || FINAL; stage = stage || 7;
    const C = cfg, parts = [], stats = { pieces: 0, leaves: 0, chips: 0, lips: 0, grooves: 0 };
    /* the volume (step 1): one box cut by a top plane rising along x, a front plane that is the bed plane, a cap, a cut end */
    const dip = rad(C.dip), phi = rad(C.facing), h = [Math.cos(phi), Math.sin(phi), 0];
    const nF = norm([h[0] * Math.sin(dip), h[1] * Math.sin(dip), Math.cos(dip)]);
    const eS = norm([-h[1], h[0], 0]), eD = norm(cross(nF, eS)); if (eD[2] < 0) { eD[0] *= -1; eD[1] *= -1; eD[2] *= -1; }
    const OF = C.footAt ? [C.footAt[0], C.footAt[1], 0] : [0, C.yf, 0];
    const tp = C.topSlope, nT = norm([-tp, 0, 1]), dT = nT[0] * 3 + nT[2] * C.topPass[1] - nT[0] * 3 * 0; /* through (3, y, 13) */
    const planes = {
      front: { n: nF, d: dot(nF, OF), tiled: 1 },
      top: { n: nT, d: dot(nT, [C.topPass[0], 0, C.topPass[1]]), tiled: 1 },
      end: { n: [1, 0, 0], d: C.xr, tiled: 1 },
      cap: { n: [0, 0, 1], d: C.cap },
      left: { n: [-1, 0, 0], d: -C.xl },
      floor: { n: [0, 0, -1], d: 0 },
      back: { n: [0, 1, 0], d: C.ymax }
    };
    /* where two tiled faces meet each keeps the half of the rock that is nearer to it (a mitre plane), so one face's relief never fills another's setbacks */
    const clipsFor = (name) => Object.keys(planes).filter(k => k !== name).map(k => {
      const o = planes[k], me = planes[name];
      if (!o.tiled || !me.tiled) return { n: o.n, d: o.d };
      const n = sub(o.n, me.n), l = dist(n); return { n: mul(n, 1 / l), d: (o.d - me.d) / l };   /* depth behind me <= depth behind the other */
    });
    const all = Object.keys(planes).map(k => ({ n: planes[k].n, d: planes[k].d }));
    function solid(pl, tone, tag) { const faces = E.solidFromPlanes(pl.map(p => P(p.n, p.d, p.tag))); if (!faces.length) return null; const pt = { faces: faces, planes: pl, tone: tone || 1, tag: tag }; let mn = [1e9, 1e9, 1e9], mx = [-1e9, -1e9, -1e9]; faces.forEach(f => f.pts.forEach(p => { for (let k = 0; k < 3; k++) { mn[k] = Math.min(mn[k], p[k]); mx[k] = Math.max(mx[k], p[k]); } })); pt.mn = mn; pt.mx = mx; return pt; }
    /* step 1 only: the plain wedge */
    if (stage < 2) { const s = solid(all, 1, 'mass'); parts.push(s); return finish(); }

    /* the core: the same volume, every tiled face moved in by dcore; it is the rock behind the pieces, so no ground shows */
    const dc = C.dcore;
    const coreP = Object.keys(planes).map(k => ({ n: planes[k].n, d: planes[k].d - (planes[k].tiled ? dc : 0) }));
    const core = solid(coreP, .94, 'core'); if (core) parts.push(core);

    /* faces: frames and the extent to tile */
    const faces = {
      front: { n: nF, es: eS, ed: eD, O: OF, plane: planes.front, name: 'front', specs: C.front },
      top: { n: nT, es: [0, 1, 0], ed: norm(cross(nT, [0, 1, 0])), O: [C.topPass[0], 0, C.topPass[1]], plane: planes.top, name: 'top', specs: C.top },
      end: { n: [1, 0, 0], es: [0, 1, 0], ed: [0, 0, 1], O: [C.xr, 0, 0], plane: planes.end, name: 'end', specs: C.end }
    };
    if (faces.top.ed[2] < 0) faces.top.ed = mul(faces.top.ed, -1);
    const leaves = [];
    Object.keys(faces).forEach(fk => {
      const F = faces[fk]; F.clip = clipsFor(fk); F.back = { n: mul(F.n, -1), d: -(F.plane.d - dc - 1) };
      /* the extent to tile: project the wedge's bounding box */
      let s0 = 1e9, s1 = -1e9, t0 = 1e9, t1 = -1e9;
      [C.xl, C.xr].forEach(x => [0, C.ymax].forEach(y => [0, C.cap].forEach(z => { const p = sub([x, y, z], F.O); const s = dot(p, F.es), t = dot(p, F.ed); s0 = Math.min(s0, s); s1 = Math.max(s1, s); t0 = Math.min(t0, t); t1 = Math.max(t1, t); })));
      s0 -= .5; t0 -= .5; s1 += .5; t1 += .5;
      const root = {
        s0: s0, s1: s1, t0: t0, t1: t1,
        sa: { n: mul(F.es, -1), d: -(dot(F.es, F.O) + s0) }, sb: { n: F.es, d: dot(F.es, F.O) + s1 },
        ta: { n: mul(F.ed, -1), d: -(dot(F.ed, F.O) + t0) }, tb: { n: F.ed, d: dot(F.ed, F.O) + t1 }
      };
      (function node(rect, B, level, key) {
        const spec = F.specs[level];
        const r = keyed(seed, fk + key);
        const go = spec && spec.stage <= stage && (level < 2 || r() < (F.specs[level - 1].p == null ? 1 : F.specs[level - 1].p));
        if (go) {
          const kids = splitRect(F, rect, spec, r);
          kids.forEach((k, i) => {
            let o;
            if (spec.shingle) o = ((i * 1.25 + R(r, 0, .75)) % (spec.off[1] + .0001));  /* one way along the strike: slabs overlap like shingles */
            else o = R(r, spec.off[0], spec.off[1]);
            node(k, B + o, level + 1, key + '/' + level + '.' + i);
          });
          return;
        }
        leaves.push({ F: F, rect: rect, B: B, key: fk + key, level: level });
      })(root, 0, 0, '');
    });

    /* build each leaf: its face (leaned a little), four lateral planes, the back, the other faces of the volume */
    leaves.forEach(L => {
      const F = L.F, rc = L.rect, r = keyed(seed, L.key + '#piece'), spec = F.specs[Math.max(0, L.level - 1)];
      const sm = (rc.s0 + rc.s1) / 2, tm = (rc.t0 + rc.t1) / 2;
      const ctr = add(add(add(F.O, mul(F.es, sm)), mul(F.ed, tm)), mul(F.n, -L.B));
      L.ctr = ctr; L.rect = rc;
      let face;
      const tilt = (stage >= 3 && spec && spec.tilt) ? spec.tilt * r() : 0;
      face = tilt ? E.leaned(F.n, ctr, tilt, r, 'face') : P(F.n, dot(F.n, ctr), 'face');
      L.facePlane = face; L.pl = [face, rc.sa, rc.sb, rc.ta, rc.tb, F.back].concat(F.clip);
      L.r = r;
    });

    /* edges (step 4) and chips (step 5) are planes added to a leaf */
    function leafPlaneAt(L, s, t) { return add(add(add(L.F.O, mul(L.F.es, s)), mul(L.F.ed, t)), mul(L.F.n, -L.B)); }
    leaves.forEach(L => {
      const F = L.F, rc = L.rect, r = keyed(seed, L.key + '#edges'), extra = [];
      const wS = rc.s1 - rc.s0, wT = rc.t1 - rc.t0;
      if (wS < 2.5 || wT < 2.5) { L.extra = extra; return; }
      if (stage >= 4 && C.edge && wS > 3 && wT > 3) {
        /* every exposed lip is eased: a full-length chamfer on the top edge, on the sides of about half the cells, a small one at the foot */
        const ch = (nrm, ang, dep, pt) => { const a = rad(ang), nn = norm(add(mul(F.n, Math.cos(a)), mul(nrm, Math.sin(a)))); extra.push({ n: nn, d: dot(nn, pt) - dep, tag: 'chamfer' }); stats.lips++; };
        ch(F.ed, R(r, C.edge.ang[0], C.edge.ang[1]), R(r, C.edge.dep[0], C.edge.dep[1]), leafPlaneAt(L, (rc.s0 + rc.s1) / 2, rc.t1));
        if (r() < C.edge.side) ch(F.es, R(r, C.edge.ang[0], C.edge.ang[1]), R(r, C.edge.dep[0], C.edge.dep[1]) * .8, leafPlaneAt(L, rc.s1, (rc.t0 + rc.t1) / 2));
        if (r() < C.edge.side) ch(mul(F.es, -1), R(r, C.edge.ang[0], C.edge.ang[1]), R(r, C.edge.dep[0], C.edge.dep[1]) * .8, leafPlaneAt(L, rc.s0, (rc.t0 + rc.t1) / 2));
        if (r() < .5) ch(mul(F.ed, -1), R(r, C.edge.ang[0], C.edge.ang[1]), R(r, C.edge.dep[0], C.edge.dep[1]) * .6, leafPlaneAt(L, (rc.s0 + rc.s1) / 2, rc.t0));
        /* one or two extra cut planes per cell break the rectangle: a facet from a random corner, tilted off the face */
                for (let q = 0; q < Math.round(C.edge.extra * (.5 + r())); q++) {
          const cs = r() < .5 ? rc.s0 : rc.s1, ct = r() < .6 ? rc.t1 : rc.t0, a = rad(R(r, 25, 50)), dep = R(r, 1, Math.min(2.6, .3 * Math.min(wS, wT)));
          const dirv = add(mul(F.es, cs === rc.s0 ? -1 : 1), mul(F.ed, ct === rc.t1 ? 1 : -1)), nn = norm(add(mul(F.n, Math.cos(a)), mul(norm(add(dirv, mul(F.ed, R(r, -.4, .4)))), Math.sin(a))));
          extra.push({ n: nn, d: dot(nn, leafPlaneAt(L, cs, ct)) - dep, tag: 'facet' }); stats.chips++;
        }
      }
      if (!C.edge && (stage >= 4 && F.name !== 'end' || (stage >= 4 && L.level >= 2))) {
        if (r() < C.lip.p) {
          const a = rad(R(r, C.lip.ang[0], C.lip.ang[1])), dep = R(r, C.lip.depth[0], C.lip.depth[1]);
          const run = C.lip.frac * wS * (.7 + .3 * r()), c = dep / Math.sin(a), psi = Math.asin(Math.min(.95, c / Math.max(run, c * 1.05)));
          const sg = r() < .5 ? -1 : 1, Pt = leafPlaneAt(L, sg < 0 ? rc.s0 : rc.s1, rc.t1);   /* a lip is a bevel that starts at a top corner and runs along the edge: one half-space cannot be local anywhere else */
          const nn = norm(add(add(mul(F.n, Math.cos(a)), mul(F.ed, Math.sin(a) * Math.cos(psi))), mul(F.es, sg * Math.sin(a) * Math.sin(psi))));
          extra.push({ n: nn, d: dot(nn, Pt) - dep, tag: 'lip' }); stats.lips++;
        }
      }
      if (stage >= 5) {
        const corners = [[rc.s0, rc.t1, -1], [rc.s1, rc.t1, 1]];
        let used = 0;
        corners.forEach(c => {
          if (used >= C.chip.perCell || r() >= C.chip.p) return;
          const dep = R(r, C.chip.depth[0], C.chip.depth[1]), a = rad(R(r, 8, C.chip.tilt));
          let nn = norm(add(add(F.n, F.ed), mul(F.es, c[2])));
          nn = norm(add(add(mul(nn, Math.cos(a)), mul(F.ed, Math.sin(a) * .5)), [0, 0, C.chip.up]));
          const Pt = leafPlaneAt(L, c[0], c[1]);
          extra.push({ n: nn, d: dot(nn, Pt) - dep, tag: 'chip' }); used++; stats.chips++;
        });
      }
      L.extra = extra;
    });

    /* grooves (step 6): a lowered strip along a slab. The cell is cut in three along the dip with the middle one set back, never a gap */
    const tone = (r) => 1 + (r() - .5) * .09;
    leaves.forEach(L => {
      const F = L.F, rc = L.rect, r = keyed(seed, L.key + '#groove'), wS = rc.s1 - rc.s0;
      stats.leaves++;
      let split = null;
      if (stage >= 6 && F.name === 'front' && wS > C.groove.minW && r() < C.groove.p) {
        const gw = R(r, C.groove.w[0], C.groove.w[1]), gd = R(r, C.groove.depth[0], C.groove.depth[1]), gs = R(r, rc.s0 + 1.2, rc.s1 - 1.2 - gw);
        split = [gs, gs + gw, gd]; stats.grooves++;
      }
      if (!split) { const s = solid(L.pl.concat(L.extra), tone(L.r), L.key); if (s) { parts.push(s); stats.pieces++; } return; }
      const tm = (rc.t0 + rc.t1) / 2, mk = (c) => { const nn = F.es, Pt = add(add(F.O, mul(F.es, c)), mul(F.ed, tm)); return { n: nn, d: dot(nn, Pt) }; };
      const p1 = mk(split[0]), p2 = mk(split[1]), base = L.pl.filter(p => p !== rc.sa && p !== rc.sb);
      const lowered = (pl) => pl.map(p => p === L.facePlane ? { n: p.n, d: p.d - split[2], tag: 'face' } : p);
      [[[rc.sa, p1], L.pl, 1], [[neg(p1), p2], L.pl, 0], [[neg(p2), rc.sb], L.pl, 1]].forEach((q, i) => {
        let pl = base.concat(q[0]); if (i === 1) pl = lowered(pl);
        const s = solid(pl.concat(i === 0 || i === 2 ? L.extra : []), tone(L.r), L.key + '.g' + i); if (s) { parts.push(s); stats.pieces++; }
      });
    });

    /* step 7: foot and rubble. About 11 small blocks of the wall's own rock, 3-6 mm, chamfered, along the foot, never over the plate */
    if (stage >= 7) {
      const r = keyed(seed, 'foot');
      for (let i = 0; i < C.foot.n; i++) {
        const sz = R(r, C.foot.size[0], C.foot.size[1]), w = sz * R(r, .9, 1.5), d = sz * R(r, .8, 1.3), hgt = sz * R(r, .4, .7);
        let cx, cy; const zone = C.foot.zone || [[C.xl - 2, 33, 1, (C.foot && C.foot.y1) || 8]];
        for (let tries = 0; tries < 30; tries++) { const z = zone[(r() * zone.length) | 0]; cx = R(r, z[0], z[1]); cy = R(r, z[2], z[3]); if (!(C.foot.avoid && Math.hypot(cx - C.foot.avoid[0], cy - C.foot.avoid[1]) < C.foot.avoid[2] + w * .5)) break; }
        const yaw = rad(R(r, -40, 40)), ex = [Math.cos(yaw), Math.sin(yaw), 0], ey = [-Math.sin(yaw), Math.cos(yaw), 0];
        const pl = [], ch = Math.min(1.1, hgt * .45), sides = [[ex, w / 2], [mul(ex, -1), w / 2], [ey, d / 2], [mul(ey, -1), d / 2]];
        sides.forEach(q => { const nn = norm(add(q[0], [(r() - .5) * .1, (r() - .5) * .1, 0])); pl.push({ n: nn, d: dot(nn, mul(q[0], q[1])) + dot(nn, [cx, cy, 0]) }); });
        const top = norm([(r() - .5) * .16, (r() - .5) * .16, 1]); pl.push({ n: top, d: dot(top, [cx, cy, hgt]) });
        pl.push({ n: [0, 0, -1], d: 1 });
        sides.forEach(q => { const nn = norm(add(q[0], [0, 0, 1])); pl.push({ n: nn, d: dot(nn, [cx + q[0][0] * q[1], cy + q[0][1] * q[1], hgt]) - ch * .7 }); });
        const s2 = solid(pl, 1.0 + (r() - .5) * .1, 'foot' + i); if (s2) parts.push(s2);
      }
    }
    return finish();

    function finish() {
      let tris = 0; parts.forEach(p => { if (p) p.faces.forEach(f => { tris += f.pts.length - 2; }); });
      return { parts: parts.filter(Boolean), ground: [0, 0, 35, 35], stats: stats, frame: { nF: nF, eS: eS, eD: eD } };
    }
  }
  G.genN = genN;
})(window);
