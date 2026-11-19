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


  /* ============ the sliders of every model ============
     SPEC[name] = list of variants. A variant is one sheet's reading of the same thing: its own sliders (name, label, unit, min, max, step, default),
     its own defaults inside its own range, and constants it does not slide (fix). The first variant is the one whose range row the page lists first.
     MODELS.x(p) and PROFILES.x(p) take a parameter object; a missing key falls back to the first variant's default. */
  const S = (k, label, unit, min, max, step, def) => ({ k: k, label: label, unit: unit, min: min, max: max, step: step, def: def });
  const V = (id, label, sl, fix) => ({ id: id, label: label, sl: sl, fix: fix || {} });
  const SPEC = {};
  const par = (name, p) => { const d = {}, v0 = SPEC[name][0]; v0.sl.forEach(s => { d[s.k] = s.def; }); Object.assign(d, v0.fix); return Object.assign(d, p); };
  const clamp01 = (x) => min(1, max(0, x));

  /* ============ the primitives, as heightfield patches ============ */
  /* facet change: three planes meeting at a small angle, creases not straight */
  SPEC.facet = [
    V('flat', 'flat sheet', [S('ang', 'plane change', '°', 1, 8, .1, 2.6), S('irr', 'crease wander', 'mm', 0, 3.2, .1, 1.6), S('rise', 'plane height', 'mm', .3, 1.4, .05, .55)]),
    V('rock', 'rock sheet', [S('ang', 'plane', '°', 5, 45, 1, 20), S('irr', 'irregularity', 'mm', 1.4, 7, .1, 3)], { rise: .55 })];
  const facetF = (p) => {
    const k1 = tan(rad(p.ang * 2.2 / 2.6)), k2 = tan(rad(p.ang)), k3 = tan(rad(p.ang * 3.4 / 2.6)), w1 = p.irr, w2 = p.irr * 1.4 / 1.6;
    const U = (x, y) => [(x - 15) * cos(.38) + (y - 15) * sin(.38) + wob(y, w1, 7, 3), (x - 15) * cos(2.2) + (y - 15) * sin(2.2) + wob(x, w2, 6, 8)];
    return {
      h: (x, y) => { const u = U(x, y); return min(k1 * u[0] + p.rise, -k2 * u[0] + p.rise, .25 + k3 * u[1]) + grain(x, y, .03, 2); },
      tone: (x, y) => { const u = U(x, y), a = k1 * u[0] + p.rise, b = -k2 * u[0] + p.rise, c = .25 + k3 * u[1]; return (a <= b && a <= c) ? .95 : (b <= c ? 1.03 : 1.0); } };
  };
  MODELS.facet = (p) => { const f = facetF(par('facet', p)); return E.hfPatch({ size: 30, N: 150, h: f.h, tone: f.tone }); };
  PROFILES.facet = (p) => { const f = facetF(par('facet', p)); return { w: 30, y: 15, fn: (x) => f.h(x, 15) }; };

  /* steps and ledges: treads tilted back, the lips not straight */
  SPEC.steps = [
    V('rock', 'rock sheet', [S('rise', 'riser height', 'mm', 2.8, 21, .1, 3), S('hd', 'riser : tread', '1 :', 2, 6, .1, 2.7), S('ang', 'riser angle', '°', 70, 90, 1, 70), S('tilt', 'tread tilt back', '°', 0, 10, .1, 3), S('n', 'risers', '', 1, 5, 1, 3)]),
    V('no', 'N and O sheets', [S('H', 'wall height H', 'mm', 20, 50, .5, 38), S('sh', 'step height', '× H', .1, .3, .01, .16), S('ld', 'ledge depth', '× H', .1, .35, .01, .26), S('ang', 'riser angle', '°', 70, 90, 1, 70), S('tilt', 'tread tilt back', '°', 0, 10, .1, 3), S('n', 'risers', '', 1, 5, 1, 3)])];
  /* the sheets give steps as ratios: riser to tread (rock sheet), or step height and ledge depth as fractions of the wall's height H (N and O sheets).
     The flat tread is what the ratio names; the pitch from lip to lip adds the riser's run and the lip's wear (about 0.7 mm). */
  const stepsF = (p) => {
    const rise = p.H != null ? p.sh * p.H : p.rise, D = p.H != null ? p.ld * p.H : p.rise * p.hd;
    const x0 = 4, w = max(.05, rise / tan(rad(p.ang)) / 2), tread = D + 2 * w + .7, size = min(120, max(30, x0 + (p.n - .5) * tread));
    /* the tread tilting back lowers its back edge, which makes the riser above it taller by that drop: build the step that much lower so the riser seen is the riser asked for */
    p = Object.assign({}, p, { rise: max(.3, rise - tan(rad(p.tilt)) * .7 * tread), tread: tread });
    return { size: size, h: (x, y) => {
      let h = 0; const xs = [];
      for (let k = 0; k < p.n; k++) { const xb = x0 + k * p.tread + wob(y + k * 13, 1.1, 5, 11 + k); h += p.rise * sstep(xb - w, xb + w, x); xs.push(xb); }
      let k = -1; for (let i = 0; i < xs.length; i++) if (x >= xs[i]) k = i;
      const loc = k < 0 ? x - (xs[0] - p.tread) : x - xs[k];
      return h - tan(rad(p.tilt)) * loc * .7 + grain(x, y, .05, 5) - (k >= 0 ? .55 * sstep(.35, .0, abs(x - xs[k] - .3)) * sstep(.55, .75, vn(y * .5, k, 4)) : 0);
    } };
  };
  MODELS.steps = (p) => { const f = stepsF(par('steps', p)); return E.hfPatch({ size: f.size, N: min(260, Math.ceil(f.size * 5)), h: f.h }); };
  PROFILES.steps = (p) => { const f = stepsF(par('steps', p)); return { w: f.size, y: 15, fn: (x) => f.h(x, 15) }; };

  /* chip / flake: a removal from an edge, as every reference shows it (none sits in the middle of a plane): an irregular polygon centred on the
     patch's front edge, cut down to a floor that falls toward the edge, walls steep, so the front face carries the notch; a small second chip takes the corner */
  SPEC.chip = [
    V('flat', 'flat sheet', [S('depth', 'depth', 'mm', .3, 2.1, .05, 1.25), S('size', 'size across', 'mm', 1.4, 11, .1, 11), S('wall', 'edge angle', '°', 15, 45, 1, 45), S('tilt', 'floor tilt', '°', 0, 10, .1, 3.4)]),
    V('rock', 'rock sheet', [S('depth', 'depth', 'mm', 1.4, 7, .1, 3), S('size', 'width', 'mm', 2.8, 21, .1, 14), S('wall', 'wall angle', '°', 45, 80, 1, 60), S('tilt', 'floor tilt', '°', 0, 10, .1, 3.4)])];
  const chipPoly2 = E.jag(30, 0, 2.2, 1.8, 5, 1.2, 9, .2);
  const chipF = (p) => {
    const poly = E.jag(15, 0, p.size / 2, p.size * .5, 6, .5, 4, .18), w = p.depth / tan(rad(p.wall)), kt = tan(rad(p.tilt));
    return (x, y) => {
      const base = grain(x, y, .08, 6) + .25 * fbm(x / 13, y / 13, 4, 2);
      const D = max(.02, p.depth - kt * y), sd = E.polySD(poly, x, y), sd2 = E.polySD(chipPoly2, x, y);
      let cut = sd < 0 ? D : D * (1 - sstep(0, w, sd)); cut += .15 * fbm(x * 2, y * 2, 7, 2) * (sd < 0 ? 1 : 0);
      const cut2 = sd2 < 0 ? .55 : .55 * (1 - sstep(0, .5, sd2));
      return base - max(cut, cut2);
    };
  };
  MODELS.chip = (p) => E.hfPatch({ size: 30, N: 170, h: chipF(par('chip', p)), aoR: 2.3 });
  PROFILES.chip = (p) => { const h = chipF(par('chip', p)); return { w: 30, y: .6, fn: (x) => h(x, .6) }; };

  /* dish: one irregular dish, deeper on one side, the rim not round */
  SPEC.dish = [
    V('flat', 'flat sheet', [S('depth', 'depth', 'mm', .3, 1.4, .05, .95), S('len', 'long axis', 'mm', 7, 42, .5, 25), S('wid', 'short axis', 'mm', 7, 42, .5, 16.4)]),
    V('rock', 'rock sheet', [S('depth', 'depth', 'mm', .7, 7, .1, 2.4), S('len', 'long axis', 'mm', 4, 35, .5, 24), S('wid', 'short axis', 'mm', 4, 35, .5, 16)])];
  const dishF = (p) => {
    const size = max(30, Math.round(p.len * 1.15)), cx = size / 2, cy = size / 2 - .5;
    return { size: size, cy: cy, h: (x, y) => {
      const dx = x - cx, dy = y - cy, c = cos(.5), s = sin(.5), u = (dx * c + dy * s) / (p.len / 2), v = (-dx * s + dy * c) / (p.wid / 2), r = hyp(u, v), a = Math.atan2(v, u);
      const rim = 1 + .2 * (vn(a * 1.15 + 6, 1.3, 5) - .5) * 2 + .1 * (vn(a * 3.1 + 2, 2.3, 7) - .5) * 2;
      const rr = r / rim, f = rr < 1 ? Math.pow(1 - rr * rr, .85) : 0, depth = p.depth * (1 + .32 * u) * (1 + .12 * sin(a * 2) * min(1, r * 1.5));
      return -depth * f + .22 * fbm(x / 14, y / 14, 3, 2) + grain(x, y, .035, 4);
    } };
  };
  MODELS.dish = (p) => { const f = dishF(par('dish', p)); return E.hfPatch({ size: f.size, N: Math.ceil(f.size * 5), h: f.h }); };
  PROFILES.dish = (p) => { const f = dishF(par('dish', p)); return { w: f.size, y: f.cy, fn: (x) => f.h(x, f.cy) }; };

  /* pits: small irregular pits, clustered, never a lattice; none below 0.4 */
  SPEC.pits = [
    V('flat', 'flat sheet', [S('n', 'pits placed', '', 10, 120, 1, 80), S('dmin', 'smallest across', 'mm', .4, 2.8, .1, .8), S('dmax', 'largest across', 'mm', .4, 2.8, .1, 2.8), S('depth', 'deepest', 'mm', .15, .85, .01, .54)]),
    V('rock', 'rock sheet', [S('n', 'pits placed', '', 5, 60, 1, 25), S('dmin', 'smallest across', 'mm', 1.4, 14, .1, 2), S('dmax', 'largest across', 'mm', 1.4, 14, .1, 8)], { depth: 1.2 })];
  const pitsF = (p) => {
    const r = E.rng(21), pts = [], cl = [[7, 8], [17, 15], [10, 18]], dmax = max(p.dmax, p.dmin);
    for (let i = 0; i < p.n; i++) {
      const c = cl[i % 3], a = r() * 2 * PI, d = Math.pow(r(), .7) * (7 + 3 * (i % 3)), x = c[0] + cos(a) * d * .9, y = c[1] + sin(a) * d * .8;
      if (x < 1 || x > 23 || y < 1 || y > 23) continue;
      const dia = p.dmin + (dmax - p.dmin) * Math.pow(r(), 2.2), dep = clamp01(.12 + .3 * dia / dmax + .12 * r()) * p.depth / .54;
      pts.push({ x: x, y: y, r: dia / 2, d: dep, ph: r() * 6, el: .8 + .4 * r() });
    }
    return { count: pts.length, h: (x, y) => {
      let h = grain(x, y, .05, 9) + .18 * fbm(x / 9, y / 9, 3, 2), cut = 0;
      for (const q of pts) { const dx = x - q.x, dy = y - q.y; if (abs(dx) > q.r * 1.6 || abs(dy) > q.r * 1.6) continue; const a = Math.atan2(dy, dx), rr = hyp(dx, dy * q.el) / (q.r * (1 + .13 * sin(a * 2 + q.ph) + .07 * sin(a * 5 + q.ph * 2))); if (rr < 1) cut = max(cut, q.d * (1 - rr * rr) ** .7); }
      return h - cut;
    } };
  };
  MODELS.pits = (p) => E.hfPatch({ size: 24, N: 190, h: pitsF(par('pits', p)).h, aoR: 1.6 });
  PROFILES.pits = (p) => { const f = pitsF(par('pits', p)); return { w: 24, y: 8, fn: (x) => f.h(x, 8), count: f.count }; };

  /* ridge and swell: an angular ridge that fades out, beside a broad round swell */
  SPEC.ridge = [
    V('flat', 'flat sheet', [S('height', 'height', 'mm', .3, 1.4, .05, 1), S('width', 'width', 'mm', 4, 28, .1, 6.4)]),
    V('rock', 'rock sheet', [S('height', 'height', 'mm', 1.4, 8.4, .1, 3), S('width', 'width', 'mm', 4.2, 28, .1, 9)])];
  const RIDGE = E.wander([0, 8], [30, 20], 10, 2.2, 5);
  const ridgeF = (p) => (x, y) => {
    const q = E.distPath(RIDGE, x, y), fade = 1 - .85 * sstep(.62, .95, q.t), hw = p.width / 6.4 * (3.2 + 1.2 * sin(q.t * 6)), prof = Math.pow(max(0, 1 - q.d / hw), 1.35);
    const swell = Math.exp(-(((x - 9) / 7.5) ** 2 + ((y - 24) / 5.5) ** 2) * 1.0) * .7;
    return p.height * fade * prof + swell + .18 * fbm(x / 12, y / 12, 6, 2) + grain(x, y, .035, 3);
  };
  MODELS.ridge = (p) => E.hfPatch({ size: 30, N: 150, h: ridgeF(par('ridge', p)) });
  PROFILES.ridge = (p) => { const h = ridgeF(par('ridge', p)); return { w: 30, y: 14, fn: (x) => h(x, 14) }; };

  /* crack: a meandering groove that stops inside the plate, with a short branch and a small step across it */
  SPEC.crack = [V('', '', [S('width', 'width at half depth', 'mm', .15, .7, .01, .7), S('depth', 'depth', 'mm', .15, 1.1, .05, 1), S('len', 'length', 'mm', 7, 70, .5, 31)])];
  const crackF = (p) => {
    const L0 = 31.06, ux = 26 / L0, uy = 17 / L0, nn = max(14, Math.round(p.len / 2.2)), CR = E.wander([2, 5], [2 + ux * p.len, 5 + uy * p.len], nn, 2.6, 7), bi = Math.round(nn * .43), bp = CR[bi], BR = E.wander(bp, [bp[0] + 9, bp[1] - 8], 6, 1.1, 3);
    const size = max(30, Math.ceil(max(2 + ux * p.len, 5 + uy * p.len) + 3)), w = p.width, w2 = p.width * .55 / .75;
    return { size: size, h: (x, y) => {
      const q = E.distPath(CR, x, y), taper = sstep(0, .1, q.t) * (1 - sstep(.78, .96, q.t)), b = E.distPath(BR, x, y), bt = sstep(0, .12, b.t) * (1 - sstep(.5, .95, b.t));
      const g1 = q.d < w ? p.depth * taper * (1 - q.d / w) : 0, g2 = b.d < w2 ? p.depth * .6 * bt * (1 - b.d / w2) : 0, step = .22 * q.s * sstep(0, 1.4, q.d) * taper;
      return .18 * fbm(x / 11, y / 11, 2, 2) + grain(x, y, .04, 8) - max(g1, g2) + step;
    } };
  };
  MODELS.crack = (p) => { const f = crackF(par('crack', p)); return E.hfPatch({ size: f.size, N: min(260, Math.ceil(f.size * 6.67)), h: f.h, aoR: 2 }); };
  PROFILES.crack = (p) => { const f = crackF(par('crack', p)); return { w: f.size, y: 13.4, fn: (x) => f.h(x, 13.4) }; };

  /* undulation: broad, low, no feature stands out */
  SPEC.undul = [V('', '', [S('amp', 'amplitude', 'mm', .7, 2.8, .05, 1.5), S('wl', 'width', 'mm', 14, 56, .5, 20)])];
  const undF = (p) => (x, y) => p.amp * fbm(x / p.wl, y / p.wl, 5, 2) + .12 * fbm(x / 4, y / 4, 8, 2);
  MODELS.undul = (p) => E.hfPatch({ size: 36, N: 150, h: undF(par('undul', p)) });
  PROFILES.undul = (p) => { const h = undF(par('undul', p)); return { w: 36, y: 18, fn: (x) => h(x, 18) }; };

  /* grain and striation: fine, stretched along the bedding, never below 0.4 mm */
  SPEC.grain = [
    V('flat', 'flat sheet', [S('depth', 'micro depth', 'mm', .15, .85, .01, .41), S('ang', 'striation angle', '°', 0, 40, 1, 14)]),
    V('rock', 'rock sheet', [S('depth', 'micro depth', 'mm', .4, 2.1, .05, 1), S('ang', 'striation angle', '°', 0, 40, 1, 14)])];
  const grF = (p) => (x, y) => {
    const a = rad(p.ang), u = x * cos(a) + y * sin(a), v = -x * sin(a) + y * cos(a), k = p.depth / .41;
    return k * (.26 * (vn(v * 1.5, u * .12, 3) - .5) * 2 + .1 * (vn(v * 3.4, u * .3, 5) - .5) * 2 + .05 * (vn(x * 2.8, y * 2.8, 11) - .5) * 2);
  };
  MODELS.grain = (p) => E.hfPatch({ size: 20, N: 200, h: grF(par('grain', p)), aoR: 1.4 });
  PROFILES.grain = (p) => { const h = grF(par('grain', p)); return { w: 20, y: 10, fn: (x) => h(x, 10) }; };

  /* ============ the eight characteristic surface forms (patches 22 mm) ============ */
  const vor = (cs, s, j) => E.voronoi(cs, s, j);
  SPEC.form_flat = [V('', '', [S('slope', 'slope', '°', 10, 30, 1, 20)])];
  SPEC.form_layered = [V('', '', [S('tilt', 'tread tilt', '°', 0, 15, .5, 5)])];
  SPEC.form_blocky = [V('', '', [S('ang', 'joint angle', '°', 70, 110, 1, 90)])];
  SPEC.form_sheared = [V('', '', [S('ang', 'layer angle', '°', 25, 45, 1, 28)])];
  SPEC.form_weathered = [V('', '', [S('R', 'rounding radius R', 'mm', 7, 35, .5, 11)])];
  SPEC.form_pitted = [V('', '', [S('ratio', 'width : depth', ': 1', 6, 10, .1, 6)])];
  SPEC.form_cracked = [V('', '', [S('cell', 'plate size', 'mm', 6, 10, .1, 6.4), S('jit', 'irregularity', '', .2, 1, .05, .85)])];
  SPEC.form_columnar = [V('', '', [S('wall', 'wall angle', '°', 80, 90, .1, 83.4)])];
  const formFn = {
    flat: (p) => { const Vo = vor(11, 3, .8); return { h: (x, y) => { const q = Vo(x, y), tx = (q.id - .5) * 2 * tan(rad(4)), ty = (q.id2 - .5) * 2 * tan(rad(4)); return -tan(rad(p.slope)) * x * .55 + tx * (x - q.cx) + ty * (y - q.cy) + (q.e < .3 ? -.3 * (1 - q.e / .3) : 0) + grain(x, y, .04, 2); }, tone: (x, y) => .93 + .14 * Vo(x, y).id }; },
    layered: (p) => { const bs = [-1]; let u = 1, r = E.rng(4); while (u < 60) { bs.push(u); u += 3 + 2.6 * r(); } return { h: (x, y) => { const xx = x + wob(y, 1.1, 6, 3); let h = 0, k = -1; for (let i = 0; i < bs.length; i++) { h += (.5 + .9 * hash2(i, 3, 4)) * sstep(bs[i] - .35, bs[i] + .35, xx); if (xx >= bs[i]) k = i; } const loc = xx - (bs[max(k, 0)] || 0); return h - tan(rad(p.tilt)) * loc * .8 + grain(x, y, .04, 7); }, tone: (x, y) => .95 + .1 * hash2(fl(x / 4), 0, 3) }; },
    blocky: (p) => { const a = rad(8), sh = 1 / tan(rad(p.ang)); const uv = (x, y) => { const v = -x * sin(a) + y * cos(a); return [x * cos(a) + y * sin(a) + v * sh, v]; };
      return { h: (x, y) => { const q = uv(x, y), u = q[0], v = q[1], row = fl(v / 7), off = hash2(row, 1, 4) * 8, uu = u + off, col = fl(uu / 8), lu = uu - col * 8, lv = v - row * 7, ed = min(lu, 8 - lu, lv, 7 - lv), id = hash2(col, row, 6), top = 2.4 * hash2(col, row, 8) + (id - .5) * .3 * (lu - 4) * .3; return top - 1.4 * (1 - sstep(0, .55, ed)) - .6 * (1 - sstep(.5, 1.6, ed)) * (hash2(col, row, 12) > .3 ? 1 : 0) + grain(x, y, .05, 9); }, tone: (x, y) => { const q = uv(x, y), row = fl(q[1] / 7), uu = q[0] + hash2(row, 1, 4) * 8; return .93 + .14 * hash2(fl(uu / 8), row, 6); } }; },
    sheared: (p) => ({ h: (x, y) => { const a = rad(p.ang), u = x * sin(a) + y * cos(a), s = 4.2, k = fl(u / s), f = u / s - k; return -tan(rad(30)) * y * .8 + .75 * (f ** 1.15) * (1 + .5 * (hash2(k, 2, 9) - .5)) + grain(x, y, .05, 5); }, tone: (x, y) => { const a = rad(p.ang), u = x * sin(a) + y * cos(a); return .94 + .12 * hash2(fl(u / 4.2), 1, 3); } }),
    weathered: (p) => ({ h: (x, y) => 2.9 * fbm(x / p.R, y / p.R, 4, 2) + .12 * fbm(x / 3, y / 3, 6, 2) - .5 * sstep(.55, .1, hyp(x - 14, y - 7) / 3.5), tone: () => 1 }),
    pitted: (p) => { const Sp = []; const r = E.rng(13); for (let i = 0; i < 8; i++) for (let j = 0; j < 8; j++) { const x = (i + .5 + (r() - .5) * .7) * 3.1 + (j % 2) * 1.5, y = (j + .5 + (r() - .5) * .7) * 2.9; if (r() < .88) Sp.push({ x: x, y: y, r: 1.15 + .75 * r(), d: (.4 + .45 * r()) * 5 / p.ratio, ph: r() * 6, el: .85 + .3 * r() }); } return { h: (x, y) => grain(x, y, .05, 12) - cutSpots(Sp, x, y), tone: () => 1 }; },
    cracked: (p) => { const Vo = vor(p.cell, 6, p.jit); return { h: (x, y) => { const q = Vo(x, y), dome = .5 * (1 - clamp01(q.d1 / (p.cell * .5625)) ** 2) + (q.id - .5) * .45, crack = q.e < .55 ? 1.0 * (1 - q.e / .55) ** 1.1 : 0; return dome - crack + (q.id2 - .5) * tan(rad(3)) * (x - q.cx) + grain(x, y, .035, 4); }, tone: (x, y) => .93 + .14 * Vo(x, y).id }; },
    columnar: (p) => { const Vo = vor(4.6, 7, .55), w = max(.02, 2.6 / tan(rad(p.wall))); return { h: (x, y) => { const q = Vo(x, y), top = q.id * 5.2 + (q.id2 - .5) * tan(rad(3)) * (x - q.cx), gap = sstep(.12 + w, .12, q.e); return top - gap * 2.6 + grain(x, y, .04, 6); }, tone: (x, y) => .92 + .16 * Vo(x, y).id }; }
  };
  ['flat', 'layered', 'blocky', 'sheared', 'weathered', 'pitted', 'cracked', 'columnar'].forEach(k => { MODELS['form_' + k] = (p) => { const f = formFn[k](par('form_' + k, p)); return E.hfPatch({ size: 22, N: 130, h: f.h, tone: f.tone, aoR: 2.3 }); }; });

  /* ============ the five patterns (patches 30 mm): a macro, a meso and a micro amplitude each ============ */
  const spots = (seed, n, rmin, rmax, dmax, box, el) => { const r = E.rng(seed), a = []; for (let i = 0; i < n; i++) { const rr = rmin + (rmax - rmin) * Math.pow(r(), 2.2); a.push({ x: box[0] + r() * (box[2] - box[0]), y: box[1] + r() * (box[3] - box[1]), r: rr, d: min(dmax, .15 + .32 * rr / rmax * dmax * 1.5) * (.7 + .5 * r()), ph: r() * 6, el: .8 + .4 * r() }); } return a; };
  function cutSpots(Sp, x, y, k) { let cut = 0; for (const p of Sp) { const dx = x - p.x, dy = y - p.y; if (abs(dx) > p.r * 1.5 || abs(dy) > p.r * 1.5) continue; const a = Math.atan2(dy, dx), rr = hyp(dx, dy * p.el) / (p.r * (1 + .13 * sin(a * 2 + p.ph) + .07 * sin(a * 5 + p.ph * 2))); if (rr < 1) cut = max(cut, p.d * (1 - rr * rr) ** .75); } return cut * (k == null ? 1 : k); }
  const PS = (macro, meso, micro) => [V('', '', [S('macro', 'macro relief', 'mm', 0, 2.8, .05, macro), S('meso', 'meso depth', 'mm', 0, 2.1, .05, meso), S('micro', 'micro grain', 'mm', 0, .85, .005, micro)])];
  SPEC.pat_subtle = PS(1.1, .35, .045); SPEC.pat_plates = PS(1.7, .95, .05); SPEC.pat_pitted = PS(.9, .9, .6); SPEC.pat_layered = PS(.35, .63, .04); SPEC.pat_network = PS(.55, 1.1, .04);
  const P1 = spots(3, 14, .5, 1.1, .35, [2, 2, 28, 28]), P1c = E.jag(21, 0, 3.4, 2.6, 6, .3, 8, .2)   /* the one soft chip takes a piece off the front edge: every chip in the references sits on an edge or a corner */;
  const VP = E.voronoi(11.5, 12, .85), CHP = [E.jag(8, 22, 3.3, 2.4, 6, .3, 5, .2), E.jag(23, 8, 2.8, 2.2, 6, 1.1, 6, .2), E.jag(19, 24, 2.2, 1.6, 5, .8, 7, .2)];
  const P3 = spots(11, 70, .5, 1.5, .6, [1, 1, 29, 29]), P3b = spots(15, 7, 3, 5, .9, [3, 3, 27, 27]);
  const VN = E.voronoi(8.2, 14, .9);
  const patF = {
    subtle: (p) => ({ N: 160, h: (x, y) => p.macro * fbm(x / 18, y / 18, 3, 2) - cutSpots(P1, x, y, p.meso / .35) - .55 * sstep(.9, .0, E.polySD(P1c, x, y)) * .6 * p.meso / .35 + grain(x, y, p.micro, 5), aoR: 2 }),
    plates: (p) => ({ N: 160, h: (x, y) => { const q = VP(x * .86 + y * .22, y * 1.1 - x * .12), base = (q.id - .5) * p.macro + ((q.id2 - .5) * tan(rad(3)) * (x - q.cx) * .86) + ((q.id3 - .5) * tan(rad(3)) * (y - q.cy)); let c = 0; for (const pp of CHP) { const sd = E.polySD(pp, x, y); c = max(c, sd < 0 ? p.meso : p.meso * (1 - sstep(0, .8, sd))); } return base - (q.e < .32 ? .55 * (1 - q.e / .32) : 0) - c + grain(x, y, p.micro, 5); }, tone: (x, y) => .93 + .14 * VP(x * .86 + y * .22, y * 1.1 - x * .12).id, aoR: 2.2 }),
    pitted: (p) => ({ N: 190, h: (x, y) => p.macro * fbm(x / 15, y / 15, 8, 2) - cutSpots(P3b, x, y, p.meso / .9) - cutSpots(P3, x, y, p.micro / .6) + grain(x, y, p.micro * .06 / .6, 7), aoR: 1.8 }),
    layered: (p) => { const bs = [], r = E.rng(8); let u = -4; while (u < 70) { bs.push(u); u += 2.2 + 3.6 * r(); } const kk = p.meso / .63; return { N: 170, h: (x, y) => { const a = rad(10), uu = (x * sin(a) + y * cos(a)) + wob(x, .9, 9, 4), vv = (x * cos(a) - y * sin(a)); let h = 0, k = -1; for (let i = 0; i < bs.length; i++) { h += (.18 + .45 * hash2(i, 5, 5)) * kk * sstep(bs[i] - .3, bs[i] + .3, uu); if (uu >= bs[i]) k = i; } const loc = uu - (bs[max(k, 0)] || 0); const vein = max(0, 1 - abs(vv - 12 + wob(uu, 1.5, 8, 9)) / .55) * .28 * kk; return h - tan(rad(4)) * loc * .5 + vein + grain(x, y, p.micro, 5) + p.macro * fbm(x / 10, y / 10, 3, 2); }, tone: (x, y) => .94 + .12 * hash2(fl((x * sin(rad(10)) + y * cos(rad(10)) + 4) / 3), 4, 4), aoR: 2 }; },
    network: (p) => ({ N: 170, h: (x, y) => { const xx = x + 1.6 * fbm(x / 7, y / 7, 3, 2), yy = y + 1.6 * fbm(x / 7 + 9, y / 7, 5, 2), q = VN(xx, yy), crack = q.e < .65 ? p.meso * (1 - q.e / .65) ** 1.2 : 0; return p.macro * (1 - clamp01(q.d1 / 4.6) ** 2) + (q.id - .5) * .4 - crack + (q.id2 - .5) * tan(rad(2.5)) * (x - q.cx) + grain(x, y, p.micro, 4); }, tone: (x, y) => .93 + .14 * VN(x, y).id, aoR: 2.4 })
  };
  ['subtle', 'plates', 'pitted', 'layered', 'network'].forEach(k => { MODELS['pat_' + k] = (p) => { const f = patF[k](par('pat_' + k, p)); return E.hfPatch({ size: 30, N: f.N, h: f.h, tone: f.tone, aoR: f.aoR }); }; });

  /* the one mass, broken at three sizes: cracks live inside their parent cell; offsets fall by a ratio each level */
  SPEC.levels = [V('', '', [S('cell', 'level 1 cell', 'mm', 12, 40, .5, 15), S('off', 'level 1 offset', 'mm', .3, 1.7, .05, 1.7), S('ratio', 'offset per level', '×', .1, .4, .01, .4), S('share', 'cells with level 3', '', .5, .8, .01, .62)])];
  MODELS.levels = (p) => {
    p = par('levels', p);
    const k = p.cell / 15, V1 = E.voronoi(p.cell, 3, .85), cache = {}, sub = (seed, cs) => cache[seed + ':' + cs] || (cache[seed + ':' + cs] = E.voronoi(cs, seed, .85)), O = [p.off, p.off * p.ratio, p.off * p.ratio * p.ratio];
    function cellAt(x, y) {
      const a = V1(x, y), s2 = 100 + fl(a.id * 9000), b = sub(s2, 6.4 * k)(x, y), on3 = b.id < p.share, c = on3 ? sub(s2 + fl(b.id2 * 7000) + 1, 3.3 * k)(x, y) : null;
      return { h: (a.id - .5) * 2 * O[0] + (b.id2 - .5) * 2 * O[1] + (c ? (c.id3 - .5) * 2 * O[2] : 0), e1: a.e, e2: b.e, e3: c ? c.e : 9, id1: a.id, id2: b.id2, id3: c ? c.id3 : .5 };
    }
    return E.hfPatch({ size: 34, N: 180, h: (x, y) => { const c = cellAt(x, y); return c.h - (c.e1 < .35 ? .5 * (1 - c.e1 / .35) : 0) - (c.e2 < .22 ? .22 * (1 - c.e2 / .22) : 0) + grain(x, y, .025, 6); }, tone: (x, y) => { const c = cellAt(x, y); return .93 + .1 * c.id1 + .05 * c.id2; }, aoR: 2.6 });
  };

  /* ============ solids ============ */
  /* ============ solids ============ */
  const V3 = (a, b, c) => [a, b, c];
  const AX = { '+x': [1, 0, 0], '-x': [-1, 0, 0], '+y': [0, 1, 0], '-y': [0, -1, 0], '+z': [0, 0, 1], '-z': [0, 0, -1] };
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

  /* three blocks: sharp, eased, eased and chipped (and one more, crisp) */
  const edgeS = (crisp, eased, side, cLo, cHi) => [S('crisp', 'crisp chamfer', 'mm', cLo, cHi, .05, crisp[0]), S('eased', 'eased radius', 'mm', cLo, cHi, .05, eased), S('side', 'side bevel', 'mm', cLo, cHi, .05, side), S('chip', 'corner chip', '× narrow side', .12, .3, .01, .2), S('w', 'block width', 'mm', 6, 20, .5, 11), S('d', 'block depth', 'mm', 6, 20, .5, 11), S('h', 'block height', 'mm', 5, 16, .5, 9)];
  SPEC.edges = [V('rock', 'rock sheet', edgeS([.45], 2.4, .5, 0, 2.8)), V('no', 'N and O sheets', edgeS([.7], 2.4, .7, .7, 2.8))];
  MODELS.edges = (p) => {
    p = par('edges', p);
    const parts = [], r = E.rng(5), sp = max(15, p.w + 4);
    [[0, 'sharp'], [1, 'crisp'], [2, 'soft'], [3, 'chip']].forEach(([i, kind]) => {
      const b = { x0: i * sp, x1: i * sp + p.w, y0: 4, y1: 4 + p.d, z0: 0, z1: p.h }, pl = E.blockPlanes(b, r);
      if (kind === 'crisp') edgePlanes(b, nm => ({ kind: 'crisp', c: p.crisp })).forEach(q => pl.push(q));
      if (kind === 'soft' || kind === 'chip') edgePlanes(b, nm => nm.indexOf('-z') >= 0 ? null : ({ kind: nm.indexOf('+z') >= 0 ? 'soft' : 'crisp', c: nm.indexOf('+z') >= 0 ? p.eased : p.side })).forEach(q => pl.push(q));
      if (kind === 'chip') { pl.push(cornerChip(b, 1, -1, 1, p.chip, r), cornerChip(b, -1, -1, 1, p.chip * .7, r), cornerChip(b, 1, 1, 1, p.chip * .5, r)); }
      parts.push(solid(pl, .98));
    });
    return E.solidsMesh(parts, { ground: [-4, -2, 3 * sp + p.w + 6, 4 + p.d + 7], seed: 3 });
  };
  MODELS.edges.cam = { az: -14, el: 27, dist: 1.2 };

  /* one block with the full vocabulary */
  SPEC.chipped = [V('', '', [S('w', 'width', 'mm', 10, 30, .5, 18), S('d', 'depth', 'mm', 8, 24, .5, 14), S('h', 'height', 'mm', 8, 24, .5, 13), S('lean', 'face lean (most)', '°', 0, 6, .1, 4), S('ease', 'top edge eased', 'mm', .7, 2.8, .05, 1.8), S('front', 'top-front bevel', 'mm', 1.6, 8.4, .1, 3), S('side', 'side bevel', 'mm', .7, 2.8, .05, 2), S('chip', 'corner chip', '× narrow side', .12, .3, .01, .22)])];
  MODELS.chipped = (p) => {
    p = par('chipped', p);
    const r = E.rng(11), s = p.lean / 4, b = { x0: 0, x1: p.w, y0: 0, y1: p.d, z0: 0, z1: p.h, burn: true, lean: { '+x': 2.5 * s, '-x': 2 * s, '+y': 3 * s, '-y': 1.5 * s, '+z': 4 * s } };
    const pl = E.blockPlanes(b, r);
    edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0) return null; if (nm === '+y+z' || nm === '-y+z') return null; if (nm.indexOf('+z') >= 0) return { kind: 'soft', c: p.ease }; return r() < .3 ? { kind: 'crisp', c: .4 } : { kind: 'soft', c: 1.1 }; }).forEach(q => pl.push(q));
    pl.push(E.chamferExact(AX['-y'], AX['+z'], [p.w / 2, 0, p.h], p.front, 'top-front'), E.chamferExact(AX['+x'], AX['-y'], [p.w, 0, p.h * 6 / 13], p.side, 'side'));
    pl.push(cornerChip(b, 1, 1, 1, p.chip, r), cornerChip(b, -1, 1, 1, p.chip * .68, r), cornerChip(b, 1, -1, 1, p.chip * .55, r));
    bendFace(b, '-y', r, 4).forEach(q => { q.d += .5; pl.push(q); }); bendFace(b, '+z', r, 3).forEach(q => { q.d += .35; pl.push(q); });
    return E.solidsMesh([solid(pl)], { ground: [-5, -5, p.w + 5, p.d + 5], seed: 3 });
  };
  MODELS.chipped.cam = { az: -32, el: 26, dist: 1.45 };

  /* shingled slabs on a dip slope (N): thin slabs, each ends at its own line, each leaning cross cut a little off square */
  SPEC.shingle = [V('', '', [S('bed', 'bed plane dip', '°', 32, 50, .5, 50), S('th', 'slab thickness', 'mm', 2.6, 5, .1, 2.6), S('n', 'slabs', '', 3, 8, 1, 6), S('cross', 'cross cut off square', '°', 0, 25, .5, 12)])];
  MODELS.shingle = (p) => {
    p = par('shingle', p);
    const al = rad(p.bed), S_ = [cos(al), 0, sin(al)], Tn = [-sin(al), 0, cos(al)], W = [0, 1, 0], r = E.rng(9), parts = [], th = p.th, nS = p.n, kc = p.cross / 12;
    const at = (s, w, t) => [S_[0] * s + W[0] * w + Tn[0] * t, S_[1] * s + W[1] * w + Tn[1] * t, S_[2] * s + W[2] * w + Tn[2] * t];
    const sEnd = []; for (let k = 0; k < nS; k++) sEnd.push(9 + 5.5 * k);
    const cuts = [0, 14, 29];
    const mk = (k, w0, w1) => {
      const t0 = -k * th * .85, t1 = t0 - th * 1.05, sHi = sEnd[k] + (r() - .5) * 2.5, pl = [];
      const faceOut = E.P(Tn, E.dot(Tn, at(0, 0, t0)), 'bed-out'), faceIn = E.P(E.mul(Tn, -1), -E.dot(Tn, at(0, 0, t1)), 'bed-in');
      pl.push(faceOut, faceIn);
      pl.push(E.P(E.mul(S_, -1), -2, 'foot'));
      const lean = rad((r() - .5) * 2 * 12 * kc), nEnd = E.norm(E.add(E.mul(S_, cos(lean * .4)), E.mul(W, sin(lean)))); pl.push(E.P(nEnd, E.dot(nEnd, at(sHi, 0, 0)), 'end'));
      const l0 = rad((r() - .5) * 2 * 10 * kc), l1 = rad((r() - .5) * 2 * 10 * kc); pl.push(E.P(E.norm(E.add(E.mul(W, -cos(l0)), E.mul(S_, sin(l0)))), E.dot(E.norm(E.add(E.mul(W, -cos(l0)), E.mul(S_, sin(l0)))), at(0, w0, 0)), 'cut'));
      pl.push(E.P(E.norm(E.add(E.mul(W, cos(l1)), E.mul(S_, sin(l1)))), E.dot(E.norm(E.add(E.mul(W, cos(l1)), E.mul(S_, sin(l1)))), at(0, w1, 0)), 'cut'));
      const th2 = rad(28 + 24 * r()), nl = E.norm(E.add(E.mul(Tn, cos(th2)), E.mul(S_, sin(th2)))); pl.push(E.cutAt(at(sHi, 0, t0), nl, .5 + .6 * r(), 'lip'));
      if (r() < .45) { const wc = r() < .5 ? w0 : w1, nn = E.norm(E.add(E.add(Tn, S_), E.mul(W, wc === w0 ? -1 : 1))); pl.push(E.cutAt(at(sHi, wc, t0), E.add(nn, [r() * .3, r() * .3, r() * .3]), 1.0 + 1.2 * r(), 'chip')); }
      pl.push(E.P([0, 0, -1], 0, 'ground'));
      return solid(pl, .96 + .06 * r());
    };
    for (let k = 0; k < nS; k++) for (let c = 0; c < 2; c++) parts.push(mk(k, cuts[c] + (r() - .5) * 1, cuts[c + 1] + (r() - .5) * 1 + (c === 1 ? 4 : 0) + 0.4));
    const stack = nS * th * .85 + th * 1.05, back = sEnd[nS - 1] + 4.5;
    const core = [E.P(Tn, E.dot(Tn, at(0, 0, -stack)), 'top'), E.P(E.mul(Tn, -1), max(30, stack + 14), 'bot'), E.P(E.mul(S_, -1), -2, 'foot'), E.P(S_, E.dot(S_, at(back, 0, 0)), 'back'), E.P(E.mul(W, -1), -0, 'l'), E.P(W, 31, 'r'), E.P([0, 0, -1], 0, 'ground')];
    parts.push(solid(core, .8));
    return E.solidsMesh(parts, { ground: [-10, -6, max(46, back * cos(al) + 12), 40], seed: 4 });
  };
  MODELS.shingle.cam = { az: -58, el: 24, dist: 1.0 };

  /* two courses of blocks with an overhang: the upper course set forward, one lower block recessed */
  SPEC.courses = [V('', '', [S('recess', 'recess', 'mm', 6, 8, .1, 7), S('fwd', 'course set forward', 'mm', 2, 3, .05, 2.5), S('h', 'course height', 'mm', 11.5, 14.5, .1, 12.5)])];
  MODELS.courses = (p) => {
    p = par('courses', p);
    const r = E.rng(14), parts = [], back = 20, f = p.fwd / 2.5, h = p.h;
    const rows = [
      { z0: 0, z1: h, blocks: [[0, 15, 0], [15.4, 31, p.recess], [31.4, 49, 0]] },
      { z0: h + .4, z1: 2 * h + 1.4, blocks: [[0, 14, -2.5 * f], [14.4, 33, -3 * f], [33.4, 49, -2 * f]] }
    ];
    rows.forEach((row, ri) => row.blocks.forEach((bl, bi) => {
      const b = { x0: bl[0], x1: bl[1], y0: bl[2], y1: back, z0: row.z0, z1: row.z1 + ((bi * 7 + ri * 3) % 3) * .4, lean: { '-y': 2.5, '+x': 2, '-x': 2, '+z': 4, '-z': 0 } };
      const pl = E.blockPlanes(b, r);
      edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0 || nm.indexOf('+y') >= 0) return null; if (nm === '-y+z') return { kind: 'crisp', c: 2.4 + r() * 1 }; return r() < .3 ? { kind: 'crisp', c: .45 } : { kind: 'soft', c: 1.0 + r() * .8 }; }).forEach(q => pl.push(q));
      if (r() < .6) pl.push(E.chamferExact(AX['+x'], AX['-y'], [b.x1, b.y0, (b.z0 + b.z1) / 2], 1.6 + r() * 1.3, 'side'));
      bendFace(b, '-y', r, 3.5).forEach(q => { q.d += .45; pl.push(q); }); if (r() < .6) bendFace(b, '+z', r, 2.5).forEach(q => { q.d += .3; pl.push(q); });
      const nChip = 1 + fl(r() * 2); for (let i = 0; i < nChip; i++) pl.push(cornerChip(b, r() < .5 ? 1 : -1, -1, 1, .1 + .1 * r(), r));
      parts.push(solid(pl, .95 + .08 * r()));
    }));
    parts.push(solid([E.P([0, 0, 1], 2 * h + 1, 'top'), E.P([0, 0, -1], 0, 'bot'), E.P([-1, 0, 0], 0, 'l'), E.P([1, 0, 0], 49, 'r'), E.P([0, -1, 0], -(back - 2), 'f'), E.P([0, 1, 0], back + 6, 'b')], .62));
    return E.solidsMesh(parts, { ground: [-5, -6, 56, 28], seed: 6, pitch: 4 });
  };
  MODELS.courses.cam = { az: -24, el: 20, dist: 1.2 };

  /* a skin over a core: the face moves, the mass stays one body. Cut away at the middle. */
  SPEC.skin = [V('', '', [S('W', 'width', 'mm', 14, 40, .5, 26), S('H', 'height', 'mm', 10, 30, .5, 17), S('skin', 'skin thickness', 'mm', 1.5, 6, .1, 3.6), S('setback', 'largest setback', 'mm', .4, 1.9, .05, 1.9), S('lean', 'largest lean', '°', 3, 9, .5, 9)])];
  MODELS.skin = (p) => {
    p = par('skin', p);
    const r = E.rng(21), parts = [], W = p.W, D = 14, Hh = p.H, skinT = p.skin, kx = W / 26, kz = Hh / 17;
    const cut = E.P([1, 0, 0], W / 2 + 1, 'cutaway');
    const coreP = [E.P([-1, 0, 0], 0, 'l'), E.P([1, 0, 0], W, 'r'), E.P([0, -1, 0], -skinT, 'f'), E.P([0, 1, 0], D, 'b'), E.P([0, 0, -1], 0, 'bot'), E.P([0, 0, 1], Hh, 'top'), cut];
    parts.push(solid(coreP, .55));
    const cx = [0, 5.4 * kx, 13.8 * kx, 19.2 * kx, W], cz = [0, 6.2 * kz, 11.8 * kz, Hh];
    for (let i = 0; i < cx.length - 1; i++) for (let j = 0; j < cz.length - 1; j++) {
      const sb = .4 + (p.setback - .4) * r(), lean = (3 + (p.lean - 3) * r()), pl = [E.P([-1, 0, 0], -cx[i], 'a'), E.P([1, 0, 0], cx[i + 1] - .05, 'b'), E.P([0, 0, -1], -cz[j], 'c'), E.P([0, 0, 1], cz[j + 1] - .05, 'd'), E.P([0, 1, 0], skinT + .05, 'back'), cut];
      const ax = E.norm(E.cross([0, -1, 0], [r() - .5, 0, r() - .5])), a = rad(lean), n = E.norm(E.add(E.mul([0, -1, 0], cos(a)), E.mul(E.cross(ax, [0, -1, 0]), sin(a))));
      pl.push(E.P(n, E.dot(n, [(cx[i] + cx[i + 1]) / 2, sb, (cz[j] + cz[j + 1]) / 2]), 'face'));
      if (i === 3 && j === 2) pl.push(E.cutAt([cx[i + 1], sb, cz[j + 1]], [1, -1, 1], 1.4, 'chip'));
      if (i === 0 && j === 2) pl.push(E.cutAt([cx[i], sb, cz[j + 1]], [-1, -1, 1], 1.1, 'chip'));
      parts.push(solid(pl, .96 + .08 * r()));
    }
    return E.solidsMesh(parts, { ground: [-8, -8, W + 8, 22], seed: 9, pitch: 3 });
  };
  MODELS.skin.cam = { az: 30, el: 26, dist: 1.25 };

  /* the mistakes, as models: even round pockets, and uniform noise (no numbers on the page, so no sliders) */
  MODELS.dish_round = () => { const Sp = []; for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) Sp.push({ x: 5.5 + i * 9.5, y: 5.5 + j * 9.5, r: 3.6, d: .8, ph: 0, el: 1 }); return E.hfPatch({ size: 30, N: 150, h: (x, y) => { let cut = 0; for (const q of Sp) { const rr = hyp(x - q.x, y - q.y) / q.r; if (rr < 1) cut = max(cut, q.d * (1 - rr * rr)); } return -cut + grain(x, y, .03, 3); } }); };
  MODELS.uniform = () => E.hfPatch({ size: 30, N: 170, h: (x, y) => .55 * fbm(x * .9, y * .9, 5, 3) + grain(x, y, .12, 9), aoR: 1.6 });
  MODELS.steps.cam = { az: -12, el: 24, dist: 1.0 };
  G.MODELS = MODELS; G.PROFILES = PROFILES; G.SPEC = SPEC;
})(window);
