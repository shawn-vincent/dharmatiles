/* models.js: the models of the page. Each returns { meshes, center, radius } through Eng.hfPatch or Eng.solidsMesh,
   and some also give a profile function (one section line through the same height function) for the SVG beside it.
   All lengths are mm at the 35 mm square. */
(function (G) {
  'use strict';
  const E = G.Eng, sstep = E.sstep, vn = E.vnoise, fbm = E.fbm, rad = E.rad, hash2 = E.hash2;
  const sin = Math.sin, cos = Math.cos, tan = Math.tan, abs = Math.abs, min = Math.min, max = Math.max, PI = Math.PI, hyp = Math.hypot, fl = Math.floor;
  const MODELS = {}, PROFILES = {};
  const grain = (x, y, a, s) => a * (fbm(x * 1.7, y * 1.7, s, 2) * .6 + fbm(x * 4.1, y * 4.1, s + 9, 1) * .4);
  const wob = (v, a, wl, s) => (vn(v / wl, s, s) - .5) * 2 * a;

  /* ============ the primitives, as heightfield patches ============ */
  /* facet change: three planes meeting at 3 to 5 degrees, creases not straight */
  const facetH = (x, y) => {
    const u1 = (x - 15) * cos(.38) + (y - 15) * sin(.38) + wob(y, 1.6, 7, 3), u2 = (x - 15) * cos(2.2) + (y - 15) * sin(2.2) + wob(x, 1.4, 6, 8);
    const a = tan(rad(2.2)) * u1 + .55, b = -tan(rad(2.6)) * u1 + .55, c = tan(rad(3.4)) * u2 + .25 * 0;
    return min(a, b, 0.25 + c * 1.0) + grain(x, y, .03, 2);
  };
  const facetPlate = (x, y) => { const u1 = (x - 15) * cos(.38) + (y - 15) * sin(.38) + wob(y, 1.6, 7, 3), u2 = (x - 15) * cos(2.2) + (y - 15) * sin(2.2) + wob(x, 1.4, 6, 8); const a = tan(rad(2.2)) * u1 + .55, b = -tan(rad(2.6)) * u1 + .55, c = tan(rad(3.4)) * u2 + .25; return (a <= b && a <= c) ? .95 : (b <= c ? 1.03 : 1.0); };
  MODELS.facet = () => E.hfPatch({ size: 30, N: 150, h: facetH, tone: facetPlate });
  PROFILES.facet = { w: 30, y: 15, fn: (x) => facetH(x, 15), vex: 10 };

  /* steps and ledges: a flight of treads tilted back 3 degrees, risers near 75 degrees, the lips not straight */
  const STEP = { rise: 3, tread: 9.8, n: 3, x0: 4 };
  const stepsH = (x, y) => {
    let h = 0; const xs = [], w = .55;
    for (let k = 0; k < STEP.n; k++) { const xb = STEP.x0 + k * STEP.tread + wob(y + k * 13, 1.1, 5, 11 + k); h += STEP.rise * sstep(xb - w, xb + w, x); xs.push(xb); }
    let k = -1; for (let i = 0; i < xs.length; i++) if (x >= xs[i]) k = i;
    const loc = k < 0 ? x - (xs[0] - STEP.tread) : x - xs[k];
    return h - tan(rad(3)) * loc * .7 + grain(x, y, .05, 5) - (k >= 0 ? .55 * sstep(.35, .0, abs(x - xs[k] - .3)) * sstep(.55, .75, vn(y * .5, k, 4)) : 0);
  };
  MODELS.steps = () => E.hfPatch({ size: 30, N: 150, h: stepsH, tone: (x, y) => 1 + .05 * (fl((x - STEP.x0) / STEP.tread)) * 0 });
  PROFILES.steps = { w: 30, y: 15, fn: (x) => stepsH(x, 15), vex: 3.2 };

  /* chip / flake: an irregular polygon cut down to a floor tilted 3 degrees, walls near 55 degrees; a smaller second chip */
  const chipPoly = E.jag(15, 15, 5.6, 4.0, 6, .5, 4, .18), chipPoly2 = E.jag(22.4, 9.4, 1.8, 1.4, 5, 1.2, 9, .2);
  const chipH = (x, y) => {
    const base = grain(x, y, .08, 6) + .25 * fbm(x / 13, y / 13, 4, 2);
    const D = 1.25 + .06 * (x - 15), sd = E.polySD(chipPoly, x, y), w = 1.25 / tan(rad(55)), sd2 = E.polySD(chipPoly2, x, y);
    let cut = sd < 0 ? D : D * (1 - sstep(0, w, sd)); cut += .15 * fbm(x * 2, y * 2, 7, 2) * (sd < 0 ? 1 : 0);
    const cut2 = sd2 < 0 ? .55 : .55 * (1 - sstep(0, .5, sd2));
    return base - max(cut, cut2);
  };
  MODELS.chip = () => E.hfPatch({ size: 30, N: 170, h: chipH, aoR: 2.3 });
  PROFILES.chip = { w: 30, y: 15, fn: (x) => chipH(x, 15), vex: 6 };

  /* dish: one irregular dish, deeper on one side, the rim not round (not the explorer's round pocket) */
  const dishH = (x, y) => {
    const dx = x - 15, dy = y - 14.5, c = cos(.5), s = sin(.5), u = (dx * c + dy * s) / 12.5, v = (-dx * s + dy * c) / 8.2, r = hyp(u, v), a = Math.atan2(v, u);
    const rim = 1 + .2 * (vn(a * 1.15 + 6, 1.3, 5) - .5) * 2 + .1 * (vn(a * 3.1 + 2, 2.3, 7) - .5) * 2;
    const rr = r / rim, f = rr < 1 ? Math.pow(1 - rr * rr, .85) : 0, depth = .95 * (1 + .32 * u) * (1 + .12 * sin(a * 2) * min(1, r * 1.5));
    return -depth * f + .22 * fbm(x / 14, y / 14, 3, 2) + grain(x, y, .035, 4) - .1 * sstep(.8, 1, rr) * (rr < 1.1 ? 1 : 0) * 0;
  };
  MODELS.dish = () => E.hfPatch({ size: 30, N: 150, h: dishH });
  PROFILES.dish = { w: 30, y: 14.5, fn: (x) => dishH(x, 14.5), vex: 10 };

  /* pits: small irregular pits, clustered, never a lattice; diameter 0.8 to 2.8, none below 0.4 */
  const PITS = (function () {
    const r = E.rng(21), pts = [], cl = [[7, 8], [17, 15], [10, 18]];
    for (let i = 0; i < 80; i++) {
      const c = cl[i % 3], a = r() * 2 * PI, d = Math.pow(r(), .7) * (7 + 3 * (i % 3)), x = c[0] + cos(a) * d * .9, y = c[1] + sin(a) * d * .8;
      if (x < 1 || x > 23 || y < 1 || y > 23) continue;
      const dia = .8 + 2.0 * Math.pow(r(), 2.2), dep = clamp01(.12 + .3 * dia / 2.8 + .12 * r()) * 1;
      pts.push({ x: x, y: y, r: dia / 2, d: dep, ph: r() * 6, el: .8 + .4 * r() });
    }
    return pts;
  })();
  function clamp01(x) { return min(1, max(0, x)); }
  const pitsH = (x, y) => {
    let h = grain(x, y, .05, 9) + .18 * fbm(x / 9, y / 9, 3, 2), cut = 0;
    for (const p of PITS) { const dx = x - p.x, dy = y - p.y; if (abs(dx) > p.r * 1.6 || abs(dy) > p.r * 1.6) continue; const a = Math.atan2(dy, dx), rr = hyp(dx, dy * p.el) / (p.r * (1 + .13 * sin(a * 2 + p.ph) + .07 * sin(a * 5 + p.ph * 2))); if (rr < 1) cut = max(cut, p.d * (1 - rr * rr) ** .7); }
    return h - cut;
  };
  MODELS.pits = () => E.hfPatch({ size: 24, N: 190, h: pitsH, aoR: 1.6 });
  PROFILES.pits = { w: 24, y: 8, fn: (x) => pitsH(x, 8), vex: 8 };

  /* ridge and swell: an angular ridge that fades out, beside a broad round swell */
  const RIDGE = E.wander([0, 8], [30, 20], 10, 2.2, 5);
  const ridgeH = (x, y) => {
    const q = E.distPath(RIDGE, x, y), fade = 1 - .85 * sstep(.62, .95, q.t), hw = 3.2 + 1.2 * sin(q.t * 6), prof = Math.pow(max(0, 1 - q.d / hw), 1.35) * (.18 * 0 + 1);
    const swell = Math.exp(-(((x - 9) / 7.5) ** 2 + ((y - 24) / 5.5) ** 2) * 1.0) * .7;
    return 1.0 * fade * prof + swell + .18 * fbm(x / 12, y / 12, 6, 2) + grain(x, y, .035, 3);
  };
  MODELS.ridge = () => E.hfPatch({ size: 30, N: 150, h: ridgeH });
  PROFILES.ridge = { w: 30, y: 14, fn: (x) => ridgeH(x, 14), vex: 8 };

  /* crack: a meandering groove 0.7 wide, 1.0 deep, that stops inside the plate, with a short branch and a small step across it */
  const CR = E.wander([2, 5], [28, 22], 14, 2.6, 7), BR = (function () { const p = CR[6]; return E.wander(p, [p[0] + 9, p[1] - 8], 6, 1.1, 3); })();
  const crackH = (x, y) => {
    const q = E.distPath(CR, x, y), taper = sstep(0, .1, q.t) * (1 - sstep(.78, .96, q.t)), b = E.distPath(BR, x, y), bt = sstep(0, .12, b.t) * (1 - sstep(.5, .95, b.t));
    const g1 = q.d < .75 ? 1.0 * taper * (1 - q.d / .75) : 0, g2 = b.d < .55 ? .6 * bt * (1 - b.d / .55) : 0, step = .22 * q.s * sstep(0, 1.4, q.d) * taper;
    return .18 * fbm(x / 11, y / 11, 2, 2) + grain(x, y, .04, 8) - max(g1, g2) + step;
  };
  MODELS.crack = () => E.hfPatch({ size: 30, N: 200, h: crackH, aoR: 2 });
  PROFILES.crack = { w: 30, y: 13.4, fn: (x) => crackH(x, 13.4), vex: 8 };

  /* undulation: broad, low, no feature stands out */
  const undH = (x, y) => 1.5 * fbm(x / 20, y / 20, 5, 2) + .12 * fbm(x / 4, y / 4, 8, 2);
  MODELS.undul = () => E.hfPatch({ size: 36, N: 150, h: undH });
  PROFILES.undul = { w: 36, y: 18, fn: (x) => undH(x, 18), vex: 8 };

  /* grain and striation: fine, stretched along the bedding, never below 0.4 mm */
  const grH = (x, y) => {
    const a = rad(14), u = x * cos(a) + y * sin(a), v = -x * sin(a) + y * cos(a);
    return .26 * (vn(v * 1.5, u * .12, 3) - .5) * 2 + .1 * (vn(v * 3.4, u * .3, 5) - .5) * 2 + .05 * (vn(x * 2.8, y * 2.8, 11) - .5) * 2;
  };
  MODELS.grain = () => E.hfPatch({ size: 20, N: 200, h: grH, aoR: 1.4 });
  PROFILES.grain = { w: 20, y: 10, fn: (x) => grH(x, 10), vex: 10 };

  /* ============ the eight characteristic surface forms (patches 22 mm) ============ */
  const vor = (cs, s, j) => E.voronoi(cs, s, j);
  const formFn = {
    flat: () => { const V = vor(11, 3, .8); return { h: (x, y) => { const q = V(x, y), tx = (q.id - .5) * 2 * tan(rad(4)), ty = (q.id2 - .5) * 2 * tan(rad(4)); return -tan(rad(20)) * x * .55 + tx * (x - q.cx) + ty * (y - q.cy) + (q.e < .3 ? -.3 * (1 - q.e / .3) : 0) + grain(x, y, .04, 2); }, tone: (x, y) => .93 + .14 * V(x, y).id }; },
    layered: () => { const bs = [-1]; let u = 1, r = E.rng(4); while (u < 60) { bs.push(u); u += 3 + 2.6 * r(); } return { h: (x, y) => { const xx = x + wob(y, 1.1, 6, 3); let h = 0, k = -1; for (let i = 0; i < bs.length; i++) { h += (.5 + .9 * hash2(i, 3, 4)) * sstep(bs[i] - .35, bs[i] + .35, xx); if (xx >= bs[i]) k = i; } const loc = xx - (bs[max(k, 0)] || 0); return h - tan(rad(5)) * loc * .8 + tan(rad(7)) * -x * 0 + grain(x, y, .04, 7) - .25 * sstep(.5, 0, abs(loc)) * 0; }, tone: (x, y) => .95 + .1 * hash2(fl(x / 4), 0, 3) }; },
    blocky: () => { const a = rad(8), r = E.rng(5); return { h: (x, y) => { const u = x * cos(a) + y * sin(a), v = -x * sin(a) + y * cos(a), row = fl(v / 7), off = hash2(row, 1, 4) * 8, uu = u + off, col = fl(uu / 8), lu = uu - col * 8 - 0, lv = v - row * 7, ed = min(lu, 8 - lu, lv, 7 - lv), id = hash2(col, row, 6), top = 2.4 * hash2(col, row, 8) + (id - .5) * .3 * (lu - 4) * .3; return top - 1.4 * (1 - sstep(0, .55, ed)) - .6 * (1 - sstep(.5, 1.6, ed)) * (hash2(col, row, 12) > .3 ? 1 : 0) + grain(x, y, .05, 9); }, tone: (x, y) => { const a2 = rad(8), u = x * cos(a2) + y * sin(a2), v = -x * sin(a2) + y * cos(a2), row = fl(v / 7), uu = u + hash2(row, 1, 4) * 8; return .93 + .14 * hash2(fl(uu / 8), row, 6); } }; },
    sheared: () => ({ h: (x, y) => { const a = rad(28), u = x * sin(a) + y * cos(a), s = 4.2, k = fl(u / s), f = u / s - k; return -tan(rad(30)) * y * .8 + .75 * (f ** 1.15) * (1 + .5 * (hash2(k, 2, 9) - .5)) + wob(x + k * 9, .35, 4, 5) * 0 + grain(x, y, .05, 5); }, tone: (x, y) => { const a = rad(28), u = x * sin(a) + y * cos(a); return .94 + .12 * hash2(fl(u / 4.2), 1, 3); } }),
    weathered: () => ({ h: (x, y) => 2.9 * fbm(x / 11, y / 11, 4, 2) + .12 * fbm(x / 3, y / 3, 6, 2) - .5 * sstep(.55, .1, hyp(x - 14, y - 7) / 3.5) , tone: () => 1 }),
    pitted: () => { const S = []; const r = E.rng(13); for (let i = 0; i < 8; i++) for (let j = 0; j < 8; j++) { const x = (i + .5 + (r() - .5) * .7) * 3.1 + (j % 2) * 1.5, y = (j + .5 + (r() - .5) * .7) * 2.9; if (r() < .88) S.push({ x: x, y: y, r: 1.15 + .75 * r(), d: .4 + .45 * r(), ph: r() * 6, el: .85 + .3 * r() }); } return { h: (x, y) => grain(x, y, .05, 12) - cutSpots(S, x, y), tone: () => 1 }; },
    cracked: () => { const V = vor(6.4, 6, .85); return { h: (x, y) => { const q = V(x, y), dome = .5 * (1 - clamp01(q.d1 / 3.6) ** 2) + (q.id - .5) * .45, crack = q.e < .55 ? 1.0 * (1 - q.e / .55) ** 1.1 : 0; return dome - crack + (q.id2 - .5) * tan(rad(3)) * (x - q.cx) + grain(x, y, .035, 4); }, tone: (x, y) => .93 + .14 * V(x, y).id }; },
    columnar: () => { const V = vor(4.6, 7, .55); return { h: (x, y) => { const q = V(x, y), top = q.id * 5.2 + (q.id2 - .5) * tan(rad(3)) * (x - q.cx) * 1 , gap = sstep(.42, .12, q.e); return top - gap * 2.6 + grain(x, y, .04, 6); }, tone: (x, y) => .92 + .16 * V(x, y).id }; }
  };
  function clampF(x, a, b) { return min(b, max(a, x)); }
  ['flat', 'layered', 'blocky', 'sheared', 'weathered', 'pitted', 'cracked', 'columnar'].forEach(k => { MODELS['form_' + k] = () => { const f = formFn[k](); return E.hfPatch({ size: 22, N: 130, h: f.h, tone: f.tone, aoR: 2.3 }); }; });

  /* ============ the five patterns (patches 30 mm) ============ */
  const spots = (seed, n, rmin, rmax, dmax, box, el) => { const r = E.rng(seed), a = []; for (let i = 0; i < n; i++) { const rr = rmin + (rmax - rmin) * Math.pow(r(), 2.2); a.push({ x: box[0] + r() * (box[2] - box[0]), y: box[1] + r() * (box[3] - box[1]), r: rr, d: min(dmax, .15 + .32 * rr / rmax * dmax * 1.5) * (.7 + .5 * r()), ph: r() * 6, el: .8 + .4 * r() }); } return a; };
  function cutSpots(S, x, y) { let cut = 0; for (const p of S) { const dx = x - p.x, dy = y - p.y; if (abs(dx) > p.r * 1.5 || abs(dy) > p.r * 1.5) continue; const a = Math.atan2(dy, dx), rr = hyp(dx, dy * p.el) / (p.r * (1 + .13 * sin(a * 2 + p.ph) + .07 * sin(a * 5 + p.ph * 2))); if (rr < 1) cut = max(cut, p.d * (1 - rr * rr) ** .75); } return cut; }
  const P1 = spots(3, 14, .5, 1.1, .35, [2, 2, 28, 28]);
  MODELS.pat_subtle = () => E.hfPatch({ size: 30, N: 160, h: (x, y) => 1.1 * fbm(x / 18, y / 18, 3, 2) + .02 * x * 0 - cutSpots(P1, x, y) - .55 * sstep(.9, .0, E.polySD(E.jag(21, 21, 3.4, 2.6, 6, .3, 8, .2), x, y)) * .6 + grain(x, y, .045, 5), aoR: 2 });
  const VP = E.voronoi(11.5, 12, .85), CHP = [E.jag(8, 22, 3.3, 2.4, 6, .3, 5, .2), E.jag(23, 8, 2.8, 2.2, 6, 1.1, 6, .2), E.jag(19, 24, 2.2, 1.6, 5, .8, 7, .2)];
  MODELS.pat_plates = () => E.hfPatch({ size: 30, N: 160, h: (x, y) => { const q = VP(x * .86 + y * .22, y * 1.1 - x * .12), base = (q.id - .5) * 1.7 + ((q.id2 - .5) * tan(rad(3)) * (x - q.cx) * .86) + ((q.id3 - .5) * tan(rad(3)) * (y - q.cy)); let c = 0; for (const pp of CHP) { const sd = E.polySD(pp, x, y); c = max(c, sd < 0 ? .95 : .95 * (1 - sstep(0, .8, sd))); } return base - (q.e < .32 ? .55 * (1 - q.e / .32) : 0) - c + grain(x, y, .05, 5); }, tone: (x, y) => .93 + .14 * VP(x * .86 + y * .22, y * 1.1 - x * .12).id, aoR: 2.2 });
  const P3 = spots(11, 70, .5, 1.5, .6, [1, 1, 29, 29]), P3b = spots(15, 7, 3, 5, .9, [3, 3, 27, 27]);
  MODELS.pat_pitted = () => E.hfPatch({ size: 30, N: 190, h: (x, y) => .9 * fbm(x / 15, y / 15, 8, 2) - cutSpots(P3b, x, y) - cutSpots(P3, x, y) + grain(x, y, .06, 7), aoR: 1.8 });
  MODELS.pat_layered = () => { const bs = [], r = E.rng(8); let u = -4; while (u < 70) { bs.push(u); u += 2.2 + 3.6 * r(); } return E.hfPatch({ size: 30, N: 170, h: (x, y) => { const a = rad(10), uu = (x * sin(a) + y * cos(a)) + wob(x, .9, 9, 4) * 1, vv = (x * cos(a) - y * sin(a)); let h = 0, k = -1; for (let i = 0; i < bs.length; i++) { h += (.18 + .45 * hash2(i, 5, 5)) * sstep(bs[i] - .3, bs[i] + .3, uu); if (uu >= bs[i]) k = i; } const loc = uu - (bs[max(k, 0)] || 0); const vein = max(0, 1 - abs(vv - 12 + wob(uu, 1.5, 8, 9)) / .55) * .28; return h - tan(rad(4)) * loc * .5 + vein + grain(x, y, .04, 5) + .35 * fbm(x / 10, y / 10, 3, 2); }, tone: (x, y) => .94 + .12 * hash2(fl((x * sin(rad(10)) + y * cos(rad(10)) + 4) / 3), 4, 4), aoR: 2 }); };
  const VN = E.voronoi(8.2, 14, .9);
  MODELS.pat_network = () => E.hfPatch({ size: 30, N: 170, h: (x, y) => { const xx = x + 1.6 * fbm(x / 7, y / 7, 3, 2), yy = y + 1.6 * fbm(x / 7 + 9, y / 7, 5, 2), q = VN(xx, yy), crack = q.e < .65 ? 1.1 * (1 - q.e / .65) ** 1.2 : 0; return .55 * (1 - clamp01(q.d1 / 4.6) ** 2) + (q.id - .5) * .4 - crack + (q.id2 - .5) * tan(rad(2.5)) * (x - q.cx) + grain(x, y, .04, 4); }, tone: (x, y) => .93 + .14 * VN(x, y).id, aoR: 2.4 });

  /* the one mass, broken at three sizes: cracks live inside their parent cell; offsets fall by 0.4 a level */
  MODELS.levels = () => {
    const V1 = E.voronoi(15, 3, .85), cache = {}, sub = (seed, cs) => cache[seed + ':' + cs] || (cache[seed + ':' + cs] = E.voronoi(cs, seed, .85)), O = [1.7, .68, .27];
    function cellAt(x, y) {
      const a = V1(x, y), s2 = 100 + fl(a.id * 9000), b = sub(s2, 6.4)(x, y), on3 = b.id < .62, c = on3 ? sub(s2 + fl(b.id2 * 7000) + 1, 3.3)(x, y) : null;
      return { h: (a.id - .5) * 2 * O[0] + (b.id2 - .5) * 2 * O[1] + (c ? (c.id3 - .5) * 2 * O[2] : 0), e1: a.e, e2: b.e, e3: c ? c.e : 9, id1: a.id, id2: b.id2, id3: c ? c.id3 : .5 };
    }
    return E.hfPatch({ size: 34, N: 180, h: (x, y) => { const c = cellAt(x, y); return c.h - (c.e1 < .35 ? .5 * (1 - c.e1 / .35) : 0) - (c.e2 < .22 ? .22 * (1 - c.e2 / .22) : 0) + grain(x, y, .025, 6); }, tone: (x, y) => { const c = cellAt(x, y); return .93 + .1 * c.id1 + .05 * c.id2; }, aoR: 2.6 });
  };

  /* ============ solids ============ */
  const V3 = (a, b, c) => [a, b, c];
  const AX = { '+x': [1, 0, 0], '-x': [-1, 0, 0], '+y': [0, 1, 0], '-y': [0, -1, 0], '+z': [0, 0, 1], '-z': [0, 0, -1] };
  MODELS.chipped_cam = 0;
  /* a block with its edges: soft = rounded in 3 tangent planes of radius rho; crisp = a 0.35 chamfer; none = a sharp edge */
  function edgePlanes(b, spec) {
    /* spec: function(edgeName) -> {kind:'crisp'|'soft'|'none', c} ; edges named by two face tags */
    const pl = [], X = [b.x0, b.x1], Y = [b.y0, b.y1], Z = [b.z0, b.z1], faces = ['-x', '+x', '-y', '+y', '-z', '+z'];
    const pt = { '-x': b.x0, '+x': b.x1, '-y': b.y0, '+y': b.y1, '-z': b.z0, '+z': b.z1 };
    for (let i = 0; i < 6; i++) for (let j = i + 1; j < 6; j++) {
      const f = faces[i], g = faces[j]; if (f[1] === g[1]) continue;
      const a = AX[f], c = AX[g], nm = f + g, s = spec(nm); if (!s || s.kind === 'none') continue;
      const E3 = [0, 0, 0]; ['x', 'y', 'z'].forEach((ax, k) => { E3[k] = (f[1] === ax) ? pt[f] : (g[1] === ax) ? pt[g] : (b[ax + '0'] + b[ax + '1']) / 2; });
      if (s.kind === 'crisp') pl.push(E.chamferExact(a, c, E3, s.c, nm));
      else { const rho = s.c; const C = E.sub(E3, E.mul(E.add(a, c), rho)); [22.5, 45, 67.5].forEach(t => { const n = E.norm(E.add(E.mul(a, cos(rad(t))), E.mul(c, sin(rad(t))))); pl.push(E.P(n, E.dot(n, C) + rho, nm)); }); }
    }
    return pl;
  }
  /* a face bent by two planes that meet in a crest: the face keeps its outline, gains a crease (the intersection of half-spaces keeps the lower of the two) */
  function bendFace(b, face, r, amp) {
    const n0 = AX[face], c = [(b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2, (b.z0 + b.z1) / 2], pt = { '-x': b.x0, '+x': b.x1, '-y': b.y0, '+y': b.y1, '-z': b.z0, '+z': b.z1 }, out = [];
    const base = [c[0], c[1], c[2]]; base[{ x: 0, y: 1, z: 2 }[face[1]]] = pt[face];
    const tang = ['x', 'y', 'z'].filter(a => a !== face[1]), ta = tang[r() < .5 ? 0 : 1], k = { x: 0, y: 1, z: 2 }[ta], other = { x: 0, y: 1, z: 2 }[tang[ta === tang[0] ? 1 : 0]];
    const ext = [b.x1 - b.x0, b.y1 - b.y0, b.z1 - b.z0][k], crest = base.slice(); crest[k] += (r() - .5) * ext * .5;
    const tdir = [0, 0, 0]; tdir[k] = 1;
    [-1, 1].forEach(sg => { const a = rad(amp * (.7 + .6 * r())) * sg, n = E.norm(E.add(E.mul(n0, Math.cos(a)), E.mul(tdir, Math.sin(a)))); out.push(E.P(n, E.dot(n, crest), 'bend')); });
    return out;
  }
  const corner = (b, sx, sy, sz) => [sx > 0 ? b.x1 : b.x0, sy > 0 ? b.y1 : b.y0, sz > 0 ? b.z1 : b.z0];
  function cornerChip(b, sx, sy, sz, frac, r) {
    const V = corner(b, sx, sy, sz), n0 = E.norm([sx, sy, sz]), ax = E.norm(E.cross(n0, [r() - .5, r() - .5, r() - .5])), a = rad(8 + 17 * r()), n = E.norm(E.add(E.mul(n0, cos(a)), E.mul(E.cross(ax, n0), sin(a))));
    return E.cutAt(V, n, frac * min(b.x1 - b.x0, b.y1 - b.y0, b.z1 - b.z0), 'chip');
  }
  const solid = (planes, tone) => ({ faces: E.solidFromPlanes(planes), tone: tone || 1 });

  /* three blocks: sharp, eased, eased and chipped */
  MODELS.edges = () => {
    const parts = [], r = E.rng(5);
    [[0, 'sharp'], [1, 'crisp'], [2, 'soft'], [3, 'chip']].forEach(([i, kind]) => {
      const b = { x0: i * 15, x1: i * 15 + 11, y0: 4, y1: 15, z0: 0, z1: 9 }, pl = E.blockPlanes(b, r);
      if (kind === 'crisp') edgePlanes(b, nm => ({ kind: 'crisp', c: .45 })).forEach(p => pl.push(p));
      if (kind === 'soft' || kind === 'chip') edgePlanes(b, nm => nm.indexOf('-z') >= 0 ? null : ({ kind: nm.indexOf('+z') >= 0 ? 'soft' : 'crisp', c: nm.indexOf('+z') >= 0 ? 2.4 : .5 })).forEach(p => pl.push(p));
      if (kind === 'chip') { pl.push(cornerChip(b, 1, -1, 1, .2, r), cornerChip(b, -1, -1, 1, .14, r), cornerChip(b, 1, 1, 1, .1, r)); }
      parts.push(solid(pl, .98));
    });
    return E.solidsMesh(parts, { ground: [-4, -2, 62, 22], seed: 3 });
  };
  MODELS.edges.cam = { az: -14, el: 27, dist: 1.2 };

  /* the three kinds of edge as one row, 70% soft, 30% crisp, is a ratio: ten edges of one block coloured by kind are drawn in the SVG; here a block carries the full vocabulary */
  MODELS.chipped = () => {
    const r = E.rng(11), b = { x0: 0, x1: 18, y0: 0, y1: 14, z0: 0, z1: 13, lean: { '+x': 2.5, '-x': 2, '+y': 3, '-y': 1.5, '+z': 4 } };
    const pl = E.blockPlanes(b, r);
    edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0) return null; if (nm === '+y+z' || nm === '-y+z') return null; if (nm.indexOf('+z') >= 0) return { kind: 'soft', c: 1.8 }; return r() < .3 ? { kind: 'crisp', c: .4 } : { kind: 'soft', c: 1.1 }; }).forEach(p => pl.push(p));
    pl.push(E.chamferExact(AX['-y'], AX['+z'], [9, 0, 13], 3.0, 'top-front'), E.chamferExact(AX['+x'], AX['-y'], [18, 0, 6], 2.0, 'side'));
    pl.push(cornerChip(b, 1, 1, 1, .22, r), cornerChip(b, -1, 1, 1, .15, r), cornerChip(b, 1, -1, 1, .12, r));
    bendFace(b, '-y', r, 4).forEach(p => { p.d += .5; pl.push(p); }); bendFace(b, '+z', r, 3).forEach(p => { p.d += .35; pl.push(p); });
    return E.solidsMesh([solid(pl)], { ground: [-5, -5, 23, 19], seed: 3 });
  };
  MODELS.chipped.cam = { az: -32, el: 26, dist: 1.45 };

  /* shingled slabs on a dip slope (N): thin slabs, each ends at its own line, each leaning cross cut a little off square */
  MODELS.shingle = () => {
    const al = rad(50), S = [cos(al), 0, sin(al)], Tn = [-sin(al), 0, cos(al)], W = [0, 1, 0], r = E.rng(9), parts = [], th = 2.6, nS = 6;
    const org = [0, 0, 0], at = (s, w, t) => [S[0] * s + W[0] * w + Tn[0] * t, S[1] * s + W[1] * w + Tn[1] * t, S[2] * s + W[2] * w + Tn[2] * t];
    const sEnd = [9, 14.5, 20, 25.5, 31, 36.5], cuts = [0, 14, 29];
    const mk = (k, w0, w1) => {
      const t0 = -k * th * .85, t1 = t0 - th * 1.05, sHi = sEnd[k] + (r() - .5) * 2.5, pl = [];
      const faceOut = E.P(Tn, E.dot(Tn, at(0, 0, t0)), 'bed-out'), faceIn = E.P(E.mul(Tn, -1), -E.dot(Tn, at(0, 0, t1)), 'bed-in');
      pl.push(faceOut, faceIn);
      pl.push(E.P(E.mul(S, -1), -2, 'foot'));
      const lean = rad((r() - .5) * 2 * 12), nEnd = E.norm(E.add(E.mul(S, cos(lean * .4)), E.mul(W, sin(lean)))); pl.push(E.P(nEnd, E.dot(nEnd, at(sHi, 0, 0)), 'end'));
      const l0 = rad((r() - .5) * 2 * 10), l1 = rad((r() - .5) * 2 * 10); pl.push(E.P(E.norm(E.add(E.mul(W, -cos(l0)), E.mul(S, sin(l0)))), E.dot(E.norm(E.add(E.mul(W, -cos(l0)), E.mul(S, sin(l0)))), at(0, w0, 0)), 'cut'));
      pl.push(E.P(E.norm(E.add(E.mul(W, cos(l1)), E.mul(S, sin(l1)))), E.dot(E.norm(E.add(E.mul(W, cos(l1)), E.mul(S, sin(l1)))), at(0, w1, 0)), 'cut'));
      /* eased lip on the end edge, 28 to 52 degrees, 0.5 to 1.1 mm */
      const th2 = rad(28 + 24 * r()), nl = E.norm(E.add(E.mul(Tn, cos(th2)), E.mul(S, sin(th2)))); pl.push(E.cutAt(at(sHi, 0, t0), nl, .5 + .6 * r(), 'lip'));
      /* a corner chip on one top corner of about one slab in three */
      if (r() < .45) { const wc = r() < .5 ? w0 : w1, nn = E.norm(E.add(E.add(Tn, S), E.mul(W, wc === w0 ? -1 : 1))); pl.push(E.cutAt(at(sHi, wc, t0), E.add(nn, [r() * .3, r() * .3, r() * .3]), 1.0 + 1.2 * r(), 'chip')); }
      pl.push(E.P([0, 0, -1], 0, 'ground'));
      return solid(pl, .96 + .06 * r());
    };
    for (let k = 0; k < nS; k++) for (let c = 0; c < 2; c++) parts.push(mk(k, cuts[c] + (r() - .5) * 1, cuts[c + 1] + (r() - .5) * 1 + (c === 1 ? 4 : 0) + 0.4));
    /* the core behind the slabs: one body, darker */
    const core = [E.P(Tn, E.dot(Tn, at(0, 0, -nS * th * .85 - th * 1.05)), 'top'), E.P(E.mul(Tn, -1), 30, 'bot'), E.P(E.mul(S, -1), -2, 'foot'), E.P(S, E.dot(S, at(41, 0, 0)), 'back'), E.P(E.mul(W, -1), -0, 'l'), E.P(W, 31, 'r'), E.P([0, 0, -1], 0, 'ground')];
    parts.push(solid(core, .8));
    return E.solidsMesh(parts, { ground: [-10, -6, 46, 40], seed: 4 });
  };
  MODELS.shingle.cam = { az: -58, el: 24, dist: 1.0 };

  /* two courses of blocks with an overhang: the upper course set forward, one lower block recessed */
  MODELS.courses = () => {
    const r = E.rng(14), parts = [], back = 20;
    const rows = [
      { z0: 0, z1: 12.5, blocks: [[0, 15, 0], [15.4, 31, 7], [31.4, 49, 0]] },
      { z0: 12.9, z1: 26.4, blocks: [[0, 14, -2.5], [14.4, 33, -3], [33.4, 49, -2]] }
    ];
    rows.forEach((row, ri) => row.blocks.forEach((bl, bi) => {
      const b = { x0: bl[0], x1: bl[1], y0: bl[2], y1: back, z0: row.z0, z1: row.z1 + (ri ? 0 : 0) + ((bi * 7 + ri * 3) % 3) * .4, lean: { '-y': 2.5, '+x': 2, '-x': 2, '+z': 4, '-z': 0 } };
      const pl = E.blockPlanes(b, r);
      edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0 || nm.indexOf('+y') >= 0) return null; if (nm === '-y+z') return { kind: 'crisp', c: 2.4 + r() * 1 }; return r() < .3 ? { kind: 'crisp', c: .45 } : { kind: 'soft', c: 1.0 + r() * .8 }; }).forEach(p => pl.push(p));
      if (r() < .6) pl.push(E.chamferExact(AX['+x'], AX['-y'], [b.x1, b.y0, (b.z0 + b.z1) / 2], 1.6 + r() * 1.3, 'side'));
      bendFace(b, '-y', r, 3.5).forEach(p => { p.d += .45; pl.push(p); }); if (r() < .6) bendFace(b, '+z', r, 2.5).forEach(p => { p.d += .3; pl.push(p); });
      const nChip = 1 + fl(r() * 2); for (let i = 0; i < nChip; i++) pl.push(cornerChip(b, r() < .5 ? 1 : -1, -1, 1, .1 + .1 * r(), r));
      parts.push(solid(pl, .95 + .08 * r()));
    }));
    parts.push(solid([E.P([0, 0, 1], 26, 'top'), E.P([0, 0, -1], 0, 'bot'), E.P([-1, 0, 0], 0, 'l'), E.P([1, 0, 0], 49, 'r'), E.P([0, -1, 0], -(back - 2), 'f'), E.P([0, 1, 0], back + 6, 'b')], .62));
    return E.solidsMesh(parts, { ground: [-5, -6, 56, 28], seed: 6, pitch: 4 });
  };
  MODELS.courses.cam = { az: -24, el: 20, dist: 1.2 };

  /* a skin over a core: the face moves, the mass stays one body. Cut away at the middle. */
  MODELS.skin = () => {
    const r = E.rng(21), parts = [], W = 26, D = 14, Hh = 17, skinT = 3.6;
    const cut = E.P([1, 0, 0], W / 2 + 1, 'cutaway');
    const coreP = [E.P([-1, 0, 0], 0, 'l'), E.P([1, 0, 0], W, 'r'), E.P([0, -1, 0], -skinT, 'f'), E.P([0, 1, 0], D, 'b'), E.P([0, 0, -1], 0, 'bot'), E.P([0, 0, 1], Hh, 'top'), cut];
    parts.push(solid(coreP, .55));
    /* pieces of the front skin: a few straight cracks across the face */
    const cx = [0, 5.4, 13.8, 19.2, W], cz = [0, 6.2, 11.8, Hh];
    for (let i = 0; i < cx.length - 1; i++) for (let j = 0; j < cz.length - 1; j++) {
      const sb = .4 + 1.5 * r(), lean = (3 + 6 * r()), pl = [E.P([-1, 0, 0], -cx[i], 'a'), E.P([1, 0, 0], cx[i + 1] - .05, 'b'), E.P([0, 0, -1], -cz[j], 'c'), E.P([0, 0, 1], cz[j + 1] - .05, 'd'), E.P([0, 1, 0], skinT + .05, 'back'), cut];
      const ax = E.norm(E.cross([0, -1, 0], [r() - .5, 0, r() - .5])), a = rad(lean), n = E.norm(E.add(E.mul([0, -1, 0], cos(a)), E.mul(E.cross(ax, [0, -1, 0]), sin(a))));
      pl.push(E.P(n, E.dot(n, [(cx[i] + cx[i + 1]) / 2, sb, (cz[j] + cz[j + 1]) / 2]), 'face'));
      if (i === 3 && j === 2) pl.push(E.cutAt([cx[i + 1], sb, cz[j + 1]], [1, -1, 1], 1.4, 'chip'));
      if (i === 0 && j === 2) pl.push(E.cutAt([cx[i], sb, cz[j + 1]], [-1, -1, 1], 1.1, 'chip'));
      parts.push(solid(pl, .96 + .08 * r()));
    }
    return E.solidsMesh(parts, { ground: [-8, -8, 34, 22], seed: 9, pitch: 3 });
  };
  MODELS.skin.cam = { az: 30, el: 26, dist: 1.25 };


  /* the mistakes, as models: even round pockets, and uniform noise */
  MODELS.dish_round = () => { const S = []; for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) S.push({ x: 5.5 + i * 9.5, y: 5.5 + j * 9.5, r: 3.6, d: .8, ph: 0, el: 1 }); return E.hfPatch({ size: 30, N: 150, h: (x, y) => { let cut = 0; for (const p of S) { const rr = hyp(x - p.x, y - p.y) / p.r; if (rr < 1) cut = max(cut, p.d * (1 - rr * rr)); } return -cut + grain(x, y, .03, 3); } }); };
  MODELS.uniform = () => E.hfPatch({ size: 30, N: 170, h: (x, y) => .55 * fbm(x * .9, y * .9, 5, 3) + grain(x, y, .12, 9), aoR: 1.6 });
  /* a ledge's proportion is the point of steps, so it is drawn at true height, like the blocks, seen nearly along the lips */
  MODELS.steps.cam = { az: -12, el: 24, dist: 1.0 };
  ['steps', 'edges', 'chipped', 'shingle', 'courses', 'skin'].forEach(k => { MODELS[k].noRelief = true; });
  G.MODELS = MODELS; G.PROFILES = PROFILES;
})(window);
