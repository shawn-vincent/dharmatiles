/* models.js: the models of the page. Each returns { meshes, center, radius } through Eng.hfPatch or Eng.solidsMesh,
   and some also give a profile function (one section line through the same height function) for the SVG beside it.
   All lengths are mm at the 35 mm square. */
(function (G) {
  'use strict';
  const E = G.Eng, sstep = E.sstep, vn = E.vnoise, fbm = E.fbm, rad = E.rad, hash2 = E.hash2;
  const sin = Math.sin, cos = Math.cos, tan = Math.tan, abs = Math.abs, min = Math.min, max = Math.max, PI = Math.PI, hyp = Math.hypot, fl = Math.floor;
  const MODELS = {}, PROFILES = {};
  const f1 = (x) => (Math.round(x * 10) / 10).toString(), f2 = (x) => (Math.round(x * 100) / 100).toString();
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

  /* piecewise linear noise: straight runs with angular kinks, never a smooth curve */
  const vnl = (t, s) => { const i = fl(t), f = t - i; return hash2(i, 0, s) * (1 - f) + hash2(i + 1, 0, s) * f; };
  const pathLen = (pts) => { let L = 0; for (let i = 0; i < pts.length - 1; i++) L += hyp(pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]); return L; };
  /* the x of a polyline where it crosses y = v */
  const xAtY = (pts, v) => { for (let i = 0; i < pts.length - 1; i++) { const a = pts[i], b = pts[i + 1]; if ((a[1] - v) * (b[1] - v) <= 0 && a[1] !== b[1]) return a[0] + (b[0] - a[0]) * (v - a[1]) / (b[1] - a[1]); } return pts[0][0]; };

  /* cracks: a main crack runs from one edge of the plate to the other in straight runs with kinks; one crack runs from the edge and stops on the main crack, another leaves the main crack for the opposite edge.
     The cracks cut the plate into four planes, each tilted a little and set a little higher or lower than its neighbour. A crack is a V groove whose width wanders along its length and whose lips are chipped. */
  SPEC.crack = [V('', '', [S('width', 'width at half depth', 'mm', .15, .7, .01, .5), S('depth', 'depth', 'mm', .15, 1.1, .05, .8), S('len', 'crack length (plate size)', 'mm', 7, 70, .5, 30), S('tilt', 'plate tilt', '°', 1, 5, .1, 2.5)])];
  const crackF = (p) => {
    const size = p.len, U = (pts) => pts.map(q => [q[0] * size, q[1] * size]), r = E.rng(17);
    const M = U([[.40, 0], [.46, .22], [.43, .45], [.55, .70], [.52, 1]]), B = U([[1, .80], [.76, .64], [xAtY(M, .60 * size) / size, .60]]), C = U([[xAtY(M, .28 * size) / size, .28], [.24, .20], [0, .30]]);
    const mk = (pts, seed) => {
      const L = pathLen(pts), chips = []; for (let i = 0; i < max(2, Math.round(L / 5)); i++) chips.push({ t: r(), len: (1 + 2.2 * r()) / L, side: r() < .5 ? -1 : 1, amp: .5 + .9 * r() });
      return (x, y) => {
        const q = E.distPath(pts, x, y), w = max(.4, p.width * (.7 + .6 * vnl(q.t * L / 6, seed))), D = p.depth * (.8 + .2 * vnl(q.t * L / 9, seed + 3));
        let c = 0; for (const k of chips) if (k.side === q.s) c = max(c, max(0, 1 - abs(q.t - k.t) / k.len) * k.amp);
        const v = q.d < w ? D * (1 - q.d / w) : 0, ch = c > 0 && q.d < w * (1 + 1.4 * c) ? c * .45 * D * (1 - q.d / (w * (1 + 1.4 * c))) : 0;
        return max(v, ch);
      };
    };
    const gM = mk(M, 5), gB = mk(B, 11), gC = mk(C, 19), sE = E.distPath(M, .9 * size, .5 * size).s, sB = E.distPath(B, .8 * size, .97 * size).s, sC = E.distPath(C, .2 * size, .03 * size).s;
    const pl = []; for (let k = 0; k < 4; k++) { const a = r() * 2 * PI, m = tan(rad(p.tilt)) * (.6 + .4 * r()); pl.push({ c: (r() - .5) * .14, ax: cos(a) * m, ay: sin(a) * m }); }
    const plate = (x, y) => E.distPath(M, x, y).s === sE ? (E.distPath(B, x, y).s === sB ? 0 : 1) : (E.distPath(C, x, y).s === sC ? 2 : 3);
    return { size: size, M: M, h: (x, y) => { const q = pl[plate(x, y)]; return q.c + q.ax * (x - size / 2) + q.ay * (y - size / 2) + .05 * fbm(x / 9, y / 9, 2, 2) + grain(x, y, .03, 8) - max(gM(x, y), gB(x, y), gC(x, y)); } };
  };
  /* what a section across the main crack shows: rim height (mean of both sides 1.5 mm out), depth, width at half depth, and the step across */
  const crackAt = (f, v) => {
    const y = v * f.size, xc0 = xAtY(f.M, y); let xb = xc0, hb = 1e9; for (let x = xc0 - 1.2; x <= xc0 + 1.2; x += .02) { const h = f.h(x, y); if (h < hb) { hb = h; xb = x; } }
    const hl = f.h(xb - 1.5, y), hr = f.h(xb + 1.5, y), rim = min(hl, hr), dep = rim - hb, hlf = hb + (hl - hb) / 2, hrf = hb + (hr - hb) / 2; let xl = xb, xr = xb; while (xl > xb - 3 && f.h(xl, y) < hlf) xl -= .02; while (xr < xb + 3 && f.h(xr, y) < hrf) xr += .02;
    return { xb: xb, hb: hb, rim: rim, dep: dep, wid: xr - xl, xl: xl, xr: xr, step: hr - hl };
  };
  MODELS.crack = (p) => { const f = crackF(par('crack', p)); return E.hfPatch({ size: f.size, N: min(330, Math.ceil(f.size / .2)), h: f.h, aoR: 2 }); };
  PROFILES.crack = (p) => {
    const f = crackF(par('crack', p)), y0 = .12, m = crackAt(f, y0), all = [.06, .12, .38, .46, .8, .9].map(v => crackAt(f, v)), rg = (k) => { const a = all.map(q => q[k]); return [min.apply(null, a), max.apply(null, a)]; }, W = rg('wid'), D = rg('dep'), St = all.map(q => abs(q.step));
    return { w: f.size, y: y0 * f.size, fn: (x) => f.h(x, y0 * f.size),
      dims: [{ t: 'h', x0: m.xl, x1: m.xr, y: m.rim, txt: f2(m.wid) + ' mm wide' }, { t: 'v', x: m.xb, ya: m.rim, yb: m.hb, txt: f2(m.dep) + ' mm deep', side: 'r' }],
      facts: 'width at half depth ' + f2(W[0]) + ' to ' + f2(W[1]) + ' mm, depth ' + f2(D[0]) + ' to ' + f2(D[1]) + ' mm, step across up to ' + f2(max.apply(null, St)) + ' mm (six sections of the main crack; the floor is 0.4 mm wide)' };
  };

  /* undulation: swells of a stated peak-to-trough height and width, on an end-to-end tilt of their own */
  SPEC.undul = [V('', '', [S('amp', 'swell, peak to trough', 'mm', .7, 2.8, .05, 1.5), S('wl', 'swell width (crest to crest)', 'mm', 14, 56, .5, 20), S('tilt', 'end-to-end tilt', '°', 1, 4, .1, 2)])];
  const undSize = (p) => Math.round(Math.min(72, Math.max(40, p.wl * 1.8)));
  const undF = (p) => { const th = rad(8), cs = cos(th), sn = sin(th), lam = p.wl, t = tan(rad(p.tilt)); return (x, y) => { const u = x * cs + y * sn, wp = lam * .2 * fbm(x / (lam * .9), y / (lam * .9), 5, 2), mod = .88 + .12 * fbm(x / lam, y / lam, 8, 1); return p.amp / 2 * mod * sin(2 * PI * (u + wp) / lam + 1.1) - t * x + .1 * fbm(x / 4, y / 4, 8, 2); }; };
  MODELS.undul = (p) => { p = par('undul', p); return E.hfPatch({ size: undSize(p), N: undSize(p) * 4, h: undF(p) }); };
  PROFILES.undul = (p) => {
    p = par('undul', p); const h = undF(p), w = undSize(p), n = 420, xs = [], ys = []; for (let i = 0; i <= n; i++) { xs.push(w * i / n); ys.push(h(w * i / n, w / 2)); }
    /* alternating extremes, a swing counting only once it passes a third of the stated amplitude */
    const thr = p.amp * .3, ex = []; let dir = 0, ci = 0; for (let i = 1; i <= n; i++) { if (!dir) { if (ys[i] - ys[ci] > thr) { dir = 1; ex.push(ci); ci = i; } else if (ys[ci] - ys[i] > thr) { dir = -1; ex.push(ci); ci = i; } else if ((ys[i] - ys[0]) * 0 === 0 && ex.length === 0 && false) ci = i; } else if (dir > 0) { if (ys[i] > ys[ci]) ci = i; else if (ys[ci] - ys[i] > thr) { ex.push(ci); ci = i; dir = -1; } } else { if (ys[i] < ys[ci]) ci = i; else if (ys[i] - ys[ci] > thr) { ex.push(ci); ci = i; dir = 1; } } } ex.push(ci);
    let best = { d: 0 }; for (let i = 0; i < ex.length - 1; i++) { const d = abs(ys[ex[i]] - ys[ex[i + 1]]); if (d > best.d) best = { d: d, a: ex[i], b: ex[i + 1] }; }
    const ymax = Math.max.apply(null, ys), ymin = Math.min.apply(null, ys), dims = [];
    if (best.d) dims.push({ t: 'v', x: xs[best.a], ya: ys[best.a], yb: ys[best.b], txt: f2(best.d) + ' mm swell', side: 'r' });
    dims.push({ t: 'v', x: w * .97, ya: ymax, yb: ymin, txt: f2(ymax - ymin) + ' mm overall', side: 'l' });
    return { w: w, y: w / 2, fn: (x) => h(x, w / 2), dims: dims, facts: 'largest crest-to-trough drop in the section ' + f2(best.d) + ' mm (swells stated ' + f2(p.amp) + '; the tilt adds a little between neighbours); overall top to bottom ' + f2(ymax - ymin) + ' mm over ' + w + ' mm, of which the ' + f1(p.tilt) + '° tilt is ' + f2(w * tan(rad(p.tilt))) + ' mm' };
  };

  /* grain and striation: bedded slab faces with steps between them; on each slab, ridges and grooves run along the slab, fine and long, never below 0.4 mm */
  SPEC.grain = [
    V('flat', 'flat sheet', [S('depth', 'micro depth', 'mm', .15, .85, .01, .41), S('ang', 'striation direction', '°', 0, 60, 1, 14), S('sp', 'striation spacing', 'mm', .4, 2.5, .05, 1.1), S('step', 'step between slabs', 'mm', 0, 1.2, .05, .4)]),
    V('rock', 'rock sheet', [S('depth', 'micro depth', 'mm', .4, 2.1, .05, 1), S('ang', 'striation direction', '°', 0, 60, 1, 14), S('sp', 'striation spacing', 'mm', .6, 5, .05, 1.8), S('step', 'step between slabs', 'mm', 0, 2.1, .05, .8)])];
  const grF = (p) => {
    const a0 = rad(p.ang), r = E.rng(23), bs = [-100], slabs = []; let u = -12; for (let k = 0; k < 7; k++) { u += 3.6 + 3 * r(); bs.push(u); } bs.push(100);
    for (let k = 0; k < bs.length - 1; k++) slabs.push({ c: (r() - .5) * 2 * p.step, tl: tan(rad(1.5 + 3 * r())) * (r() < .5 ? -1 : 1), da: (r() - .5) * rad(10), ph: r() * 9, tone: .9 + .2 * r() });
    const R = (t) => 1 - abs(2 * t - 1);
    const tex = (v, uu, k) => .7 * R(vn(v / p.sp + slabs[k].ph, uu / (p.sp * 9), 3 + k)) + .3 * R(vn(v / (p.sp * .4), uu / (p.sp * 4), 5 + k)) + .1 * (vn(uu * 2.8, v * 2.8, 11) - .5);
    const geo = (x, y) => { const X = x - 12, Y = y - 12; let v = -X * sin(a0) + Y * cos(a0), uu = X * cos(a0) + Y * sin(a0), k = 0; while (k < bs.length - 2 && v > bs[k + 1] + 1.1 * (vnl(uu / 5, 7 + k) - .5) * 2) k++; const s = slabs[k], ca = cos(s.da), sa = sin(s.da), v2 = v * ca - uu * sa, u2 = uu * ca + v * sa; return { k: k, v: v, v2: v2, u2: u2 }; };
    let lo = 1e9, hi = -1e9; for (let j = 0; j < 50; j++) for (let i = 0; i < 50; i++) { const g = geo(2 + i * .4, 2 + j * .4), t = tex(g.v2, g.u2, g.k); lo = min(lo, t); hi = max(hi, t); }
    const sc = p.depth / (hi - lo), h = (x, y) => { const g = geo(x, y), s = slabs[g.k], k0 = bs[g.k] === -100 ? g.v : g.v - (bs[g.k] + bs[g.k + 1]) / 2; return s.c + s.tl * k0 + sc * tex(g.v2, g.u2, g.k); };
    return { h: h, tone: (x, y) => slabs[geo(x, y).k].tone, slabs: slabs, geo: geo, tex: (x, y) => { const g = geo(x, y); return sc * tex(g.v2, g.u2, g.k); }, a0: a0 };
  };
  MODELS.grain = (p) => { const f = grF(par('grain', p)); return E.hfPatch({ size: 24, N: 240, h: f.h, tone: f.tone, aoR: 1.4 }); };
  PROFILES.grain = (p) => {
    p = par('grain', p); const f = grF(p), a = f.a0, fn = (t) => f.h(12 - (t - 10) * sin(a), 12 + (t - 10) * cos(a));
    let lo = 1e9, hi = -1e9; for (let j = 0; j < 60; j++) for (let i = 0; i < 60; i++) { const t = f.tex(2 + i * .333, 2 + j * .333); lo = min(lo, t); hi = max(hi, t); }
    const ks = {}, cs = []; for (let i = 0; i <= 200; i++) { const t = 20 * i / 200, g = f.geo(12 - (t - 10) * sin(a), 12 + (t - 10) * cos(a)); if (!ks[g.k]) { ks[g.k] = 1; cs.push(f.slabs[g.k].c); } }
    let stp = 0; for (let i = 1; i < cs.length; i++) stp = max(stp, abs(cs[i] - cs[i - 1])); let ymin = 1e9, ymax = -1e9; for (let i = 0; i <= 420; i++) { const y = fn(20 * i / 420); ymin = min(ymin, y); ymax = max(ymax, y); }
    return { w: 20, y: 10, fn: fn, dims: [{ t: 'v', x: 18.2, ya: ymax, yb: ymin, txt: f2(ymax - ymin) + ' mm', side: 'l' }], facts: 'texture depth ' + f2(hi - lo) + ' mm (stated ' + f2(p.depth) + '), ' + cs.length + ' slabs crossed in the section, offsets between slabs up to ' + f2(stp) + ' mm, section across the striations' };
  };

  /* ============ the eight characteristic surface forms (patches 22 mm) ============ */
  const vor = (cs, s, j) => E.voronoi(cs, s, j);
  /* lens plates: long, pointed plates with curved, irregular outlines, each a little tilted, overlapping like shingles.
     o: n plates, a half-length, elong length : width, irr outline irregularity, th grain direction (deg), jt jitter of each plate's direction,
     tilt plate tilt (deg), stack (plates stack up across the grain, each covering the one below), rise (stack: height gained per mm across), seed */
  const lensField = (o) => {
    const r = E.rng(o.seed), th = rad(o.th), cs = cos(th), sn = sin(th), pl = [];
    for (let k = 0; k < o.n; k++) {
      const a = o.a * (.75 + .5 * r()), b = a / o.elong * (.8 + .4 * r()), uc = -5 + 32 * r(), sc = o.stack ? -5 + 32 * (k + .5 + (r() - .5) * .8) / o.n : -5 + 32 * r(), tk = th + (r() - .5) * 2 * rad(o.jt);
      const cx = 11 + (uc - 11) * cs - (sc - 11) * sn, cy = 11 + (uc - 11) * sn + (sc - 11) * cs, z = o.stack ? (sc - 11) * o.rise + (r() - .5) * o.rise * 2.2 : (r() - .5) * 1.1;
      const ta = o.stack ? th + PI / 2 + (r() - .5) * .5 : r() * 2 * PI, tm = tan(rad(o.tilt)) * (.4 + .6 * r());
      pl.push({ cx: cx, cy: cy, a: a, b: b, c: cos(tk), s: sin(tk), z: z, tx: cos(ta) * tm, ty: sin(ta) * tm, p1: r() * 6, p2: r() * 6, p3: r() * 6, tone: .88 + .22 * r() });
    }
    const at = (x, y) => {
      let best = -1e9, id = -1;
      for (let k = 0; k < pl.length; k++) {
        const q = pl[k], dx = x - q.cx, dy = y - q.cy; if (abs(dx) > q.a * 1.5 + 2 || abs(dy) > q.a * 1.5 + 2) continue;
        const u = dx * q.c + dy * q.s, v = -dx * q.s + dy * q.c, ph = Math.atan2(v / q.b, u / q.a), m = 1 + o.irr * (.5 * sin(2 * ph + q.p1) + .35 * sin(3 * ph + q.p2) + .25 * sin(5 * ph + q.p3));
        const g = Math.pow(abs(u) / (q.a * m), 1.45) + Math.pow(abs(v) / (q.b * m), 1.45); if (g >= 1) continue;
        const z = q.z + q.tx * dx + q.ty * dy - .32 * sstep(.62, 1, g); if (z > best) { best = z; id = k; }
      }
      return { z: best, id: id };
    };
    return { h: (x, y) => { const q = at(x, y); return q.id < 0 ? -1.2 : q.z; }, tone: (x, y) => { const q = at(x, y); return q.id < 0 ? .7 : pl[q.id].tone; } };
  };
  SPEC.form_flat = [V('', '', [S('slope', 'slope', '°', 10, 30, 1, 20), S('elong', 'plate length : width', ': 1', 2, 5, .1, 3), S('irr', 'outline irregularity', '', 0, .6, .01, .35)])];
  SPEC.form_layered = [V('', '', [S('tilt', 'tread tilt', '°', 0, 15, .5, 5), S('irr', 'outline irregularity', '', 0, .6, .01, .3)])];
  SPEC.form_blocky = [V('', '', [S('ang', 'joint angle', '°', 70, 110, 1, 90), S('chips', 'chips per block', '', 0, 4, .1, 2.5)])];
  SPEC.form_sheared = [V('', '', [S('ang', 'layer angle', '°', 25, 45, 1, 28), S('elong', 'blade length : width', ': 1', 4, 10, .1, 6)])];
  SPEC.form_weathered = [V('', '', [S('R', 'rounding radius R', 'mm', 7, 35, .5, 11)])];
  SPEC.form_pitted = [V('', '', [S('ratio', 'width : depth', ': 1', 6, 10, .1, 6)])];
  SPEC.form_cracked = [V('', '', [S('cell', 'plate size', 'mm', 6, 10, .1, 6.4), S('jit', 'irregularity', '', .2, 1, .05, .85)])];
  SPEC.form_columnar = [V('', '', [S('wall', 'wall angle', '°', 80, 90, .1, 83.4)])];
  const formFn = {
    flat: (p) => { const L = lensField({ n: 46, a: 6.2, elong: p.elong, irr: p.irr, th: 35, jt: 9, tilt: 4, seed: 5 }); return { h: (x, y) => L.h(x, y) - tan(rad(p.slope)) * x * .4 + grain(x, y, .05, 2), tone: L.tone }; },
    layered: (p) => { const L = lensField({ n: 36, a: 8.5, elong: 4.4, irr: p.irr, th: -14, jt: 4, tilt: p.tilt, stack: true, rise: .24, seed: 8 }); return { h: (x, y) => L.h(x, y) + grain(x, y, .05, 3), tone: L.tone }; },
    blocky: (p) => {
      const a = rad(8), sh = 1 / tan(rad(p.ang)), uv = (x, y) => { const v = -x * sin(a) + y * cos(a); return [x * cos(a) + y * sin(a) + v * sh, v]; };
      const side = (col, row, k) => { const m = hash2(col, row, 30 + k); return m < .3 ? .22 : m < .68 ? 1.05 : .55; };   /* crisp, worn or broken, edge by edge */
      return { h: (x, y) => {
        const q = uv(x, y), u = q[0], v = q[1], row = fl(v / 7), off = hash2(row, 1, 4) * 8, uu = u + off, col = fl(uu / 8), lu = uu - col * 8, lv = v - row * 7, cx = lu - 4, cz = lv - 3.5, H = (i) => hash2(col, row, i);
        const ca = H(23) * PI, ca2 = ca + 1 + H(25), top = 2.4 * H(8) + (H(21) - .5) * .12 * cx + (H(22) - .5) * .12 * cz + (H(24) < .5 ? -1 : 1) * .1 * abs(cx * cos(ca) + cz * sin(ca)) + (H(26) < .5 ? -1 : 1) * .06 * abs(cx * cos(ca2) + cz * sin(ca2));
        const ds = [lu, 8 - lu, lv, 7 - lv], ed = min(ds[0], ds[1], ds[2], ds[3]); let drop = 0;
        for (let k = 0; k < 4; k++) { const c = side(col, row, k); drop = max(drop, c * (1 - sstep(0, c * 1.25, ds[k])) * (c > .6 ? 1 : 1.3)); }
        let chip = 0; const nc = fl(p.chips + H(40));
        for (let c = 0; c < nc; c++) { const sd = fl(H(41 + c) * 4), t = H(50 + c), px = sd < 2 ? (sd ? 8 : 0) + (sd ? -.3 : .3) : 8 * t, pz = sd < 2 ? 7 * t : (sd === 2 ? .3 : 6.7), dx = lu - px, dz = lv - pz, ph = H(60 + c) * 6, rr = .8 + 1.3 * H(70 + c), d = hyp(dx, dz) * (1 + .3 * sin(3 * Math.atan2(dz, dx) + ph)); chip = max(chip, max(0, (.5 + .9 * H(80 + c)) - d / rr * (.5 + .9 * H(80 + c)))); }
        const tex = .07 * (vn(x * 2.6, y * 2.6, 3) - .5) * 2 + .04 * (vn(x * 5.3, y * 5.3, 4) - .5) * 2 - .12 * max(0, vn(x * 1.8, y * 1.8, 9) - .7) / .3;
        return top - drop - chip - 1.7 * (1 - sstep(0, .42, ed)) + tex;
      }, tone: (x, y) => { const q = uv(x, y), row = fl(q[1] / 7), col = fl((q[0] + hash2(row, 1, 4) * 8) / 8); return .92 + .16 * hash2(col, row, 6); } };
    },
    sheared: (p) => { const L = lensField({ n: 34, a: 9, elong: p.elong, irr: .28, th: p.ang, jt: 3, tilt: 7, stack: true, rise: .22, seed: 12 }); return { h: (x, y) => L.h(x, y) + grain(x, y, .04, 5), tone: L.tone }; },
    weathered: (p) => { const Vo = vor(9.5, 4, .8); return { h: (x, y) => {
      const q = Vo(x, y), dx = x - q.cx, dy = y - q.cy, d = hyp(dx, dy), R = p.R, cap = Math.sqrt(max(0, R * R - d * d)) - R, fa = q.id2 * 6.28, plane = -tan(rad(26 + 10 * q.id3)) * (dx * cos(fa) + dy * sin(fa) - 1.2),
        e = q.e, crack = e < .6 ? 1.3 * (1 - e / .6) ** 1.2 : 0, ox = (q.id3 - .5) * 3, oy = (q.id - .5) * 3, dim = q.id2 > .5 ? .55 * max(0, 1 - hyp(dx - ox, dy - oy) / 1.4) ** 1.6 : 0;
      return min(cap, plane) + (q.id - .5) * .8 - crack - dim + grain(x, y, .05, 4) - .5 * sstep(.2, 1.6, 1.6 - e) * .3;
    }, tone: (x, y) => .92 + .16 * Vo(x, y).id }; },
    pitted: (p) => { const Vo = vor(3.7, 13, .8); return { h: (x, y) => { const q = Vo(x, y), dep = 2.8 / p.ratio * (.6 + .8 * q.id2); return (q.id - .5) * .14 - dep * sstep(.12, 1.0, q.e) ** .8 + grain(x, y, .045, 12); }, tone: (x, y) => .93 + .14 * Vo(x, y).id }; },
    cracked: (p) => { const Vo = vor(p.cell, 6, p.jit); return { h: (x, y) => { const q = Vo(x, y), crack = q.e < .6 ? 1.35 * (1 - q.e / .6) ** 1.1 : 0, lip = .12 * (1 - sstep(.5, 1.3, q.e)); return (q.id - .5) * .4 - lip - crack + (q.id2 - .5) * tan(rad(3.4)) * (x - q.cx) + (q.id3 - .5) * tan(rad(3.4)) * (y - q.cy) + grain(x, y, .035, 4); }, tone: (x, y) => .93 + .14 * Vo(x, y).id }; },
    columnar: (p) => {
      const r = E.rng(7), bx = [], hs = [], br = []; let x = -4; while (x < 28) { x += 2.6 + 2.8 * r(); bx.push(x); hs.push(1.2 + 3.6 * r()); br.push((r() - .5) * .16); }
      const seg = (i, y) => { let yb = -3 - 8 * hash2(i, 1, 3), k = 0; for (;;) { const nx = yb + 5 + 8 * hash2(i, k, 5); if (nx > y) break; yb = nx; k++; } return { k: k, y0: yb }; };
      const w = max(.02, 2.8 / tan(rad(p.wall)));
      return { h: (x, y) => {
        let i = 0; while (i < bx.length - 1 && x > bx[i] + br[i] * (y - 11)) i++;
        const xl = bx[i - 1 < 0 ? 0 : i - 1] + br[i - 1 < 0 ? 0 : i - 1] * (y - 11), xr = bx[i] + br[i] * (y - 11), e = min(x - xl, xr - x), sg = seg(i, y), top = hs[i] + 2.2 * (hash2(i, sg.k, 9) - .5) - .09 * (y - sg.y0) * (hash2(i, sg.k, 11) - .3);
        return top - 3.2 * (1 - sstep(.2, .2 + w, e)) + .14 * (vn(x * 3.1, y * .35, 5) - .5) * 2 + .1 * (vn(x * 1.3, y * 1.3, 8) - .5) * 2 + grain(x, y, .03, 6);
      }, tone: (x, y) => { let i = 0; while (i < bx.length - 1 && x > bx[i] + br[i] * (y - 11)) i++; return .9 + .2 * hash2(i, seg(i, y).k, 4); } };
    }
  };
  ['flat', 'layered', 'blocky', 'sheared', 'weathered', 'pitted', 'cracked', 'columnar'].forEach(k => { MODELS['form_' + k] = (p) => { const f = formFn[k](par('form_' + k, p)); return E.hfPatch({ size: 22, N: 150, h: f.h, tone: f.tone, aoR: 2.3 }); }; });
  MODELS.form_columnar.cam = { az: -12, el: 32, dist: 1 };

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
    network: (p) => ({ N: 170, h: (x, y) => { const xx = x + 1.6 * fbm(x / 7, y / 7, 3, 2), yy = y + 1.6 * fbm(x / 7 + 9, y / 7, 5, 2), q = VN(xx, yy), crack = q.e < .65 ? p.meso * (1 - q.e / .65) ** 1.2 : 0; return p.macro * .8 * fbm(x / 16, y / 16, 6, 2) + (q.id - .5) * .4 - .1 * (1 - sstep(.6, 1.4, q.e)) - crack + (q.id2 - .5) * tan(rad(2.5)) * (x - q.cx) + grain(x, y, p.micro, 4); }, tone: (x, y) => .93 + .14 * VN(x, y).id, aoR: 2.4 })
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

  /* O massing, from the N and O sheet's "Massing and ratios": four layers sharing H as .26 / .26 / .28 / .20 (base to top), a block footprint of width : height 1.6 to 2.4.
     The layers step back, and the blocks get smaller going up, in width and in height (a layer whose blocks would be too tall for its fall-off is cut into two courses), and no course is a row of equal blocks. */
  SPEC.courses = [V('', '', [S('H', 'total height H', 'mm', 30, 43, .5, 38), S('s1', 'layer 1 (base), share of H', '', .2, .32, .01, .26), S('s2', 'layer 2, share of H', '', .2, .32, .01, .26), S('s3', 'layer 3, share of H', '', .22, .34, .01, .28), S('s4', 'layer 4 (top), share of H', '', .14, .26, .01, .2), S('wh', 'block width : height (base)', ': 1', 1.6, 2.4, .05, 2), S('fall', 'size fall-off per layer', '', 0, 1, .05, .6), S('irr', 'irregularity', '', 0, 1, .05, .5), S('set', 'setback per layer', 'of depth', .05, .3, .01, .15)])];
  const coursesLayout = (p) => {
    const r = E.rng(14), tot = p.s1 + p.s2 + p.s3 + p.s4, sh = [p.s1, p.s2, p.s3, p.s4].map(v => v / tot), f = 1 - .3 * p.fall, D = 24, DEP = 18, layers = [];
    const h0 = p.H * sh[0], L0 = Math.max(44, 3 * p.wh * h0); let z = 0;
    for (let i = 0; i < 4; i++) {
      const hi = p.H * sh[i], tb = h0 * Math.pow(f, i), nc = Math.max(1, Math.round(hi / tb)), hb = hi / nc, wbar = p.wh * tb, Li = L0 * (1 - .12 * i), xs0 = L0 * (.035 * i + (r() - .5) * .05) + (L0 - Li) * .25, front = i * p.set * DEP, blocks = [];
      for (let c = 0; c < nc; c++) {
        let x = xs0 - r() * wbar * .5; const zc0 = z + c * hb;
        while (x < xs0 + Li) {
          const end = xs0 + Li, w0 = max(4.5, wbar * (1 + p.irr * (r() - .5) * 1.2)), x1 = (x + w0 + .6 * wbar > end) ? end : x + w0, push = p.irr * r() * 1.6, dz = (c === nc - 1 ? p.irr * (r() - .3) * .7 : 0);
          if (x1 - x > 3) blocks.push({ x0: max(x, xs0 - 1), x1: x1, z0: zc0, z1: zc0 + hb + dz, y0: front + push, course: c });
          x = x1 + .35 + .4 * r();
        }
      }
      layers.push({ z0: z, z1: z + hi, hb: hb, wbar: wbar, blocks: blocks, nc: nc, front: front, x0: xs0, x1: xs0 + Li }); z += hi;
    }
    return { layers: layers, H: p.H, L0: L0, D: D, sh: sh };
  };
  MODELS.courses = (p) => {
    p = par('courses', p);
    const r = E.rng(15), parts = [], L = coursesLayout(p);
    L.layers.forEach((ly, li) => ly.blocks.forEach(bl => {
      const hb = bl.z1 - bl.z0, b = { x0: bl.x0, x1: bl.x1, y0: bl.y0, y1: L.D, z0: bl.z0, z1: bl.z1, lean: { '-y': 2.2, '+x': 2, '-x': 2, '+z': 3 } };
      const pl = E.blockPlanes(b, r), ez = min(2.4, .08 * hb + .9) * (.7 + .6 * r()), vs = r() < .5;
      edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0 || nm.indexOf('+y') >= 0) return null; if (nm === '-y+z') return { kind: 'soft', c: ez * 1.2 }; if (nm.indexOf('+z') >= 0) return { kind: 'soft', c: ez }; return vs ? { kind: 'soft', c: .6 + .5 * r() } : { kind: 'crisp', c: .4 + .3 * r() }; }).forEach(q => pl.push(q));
      bendFace(b, '-y', r, 3).forEach(q => { q.d += .35; pl.push(q); });
      if (r() < .4) pl.push(cornerChip(b, r() < .5 ? 1 : -1, -1, 1, .12 + .1 * r(), r));
      parts.push(solid(pl, .95 + .08 * r()));
    }));
    const xa = Math.min.apply(null, L.layers.map(l => l.x0)), xb = Math.max.apply(null, L.layers.map(l => l.x1));
    parts.push(solid([E.P([0, 0, 1], L.H * .75, 'top'), E.P([0, 0, -1], 0, 'bot'), E.P([-1, 0, 0], -(xa + 9), 'l'), E.P([1, 0, 0], xb - 9, 'r'), E.P([0, -1, 0], -(L.D - 3), 'f'), E.P([0, 1, 0], L.D + 6, 'b')], .82));
    return E.solidsMesh(parts, { ground: [xa - 8, -8, xb + 8, L.D + 10], seed: 6, pitch: 5, facetTone: .1, grey: .8 });
  };
  MODELS.courses.cam = { az: -26, el: 22, dist: 1.1 };
  window.COURSESLAYOUT = (p) => coursesLayout(par('courses', p));

  /* an O block in context: a course of three wide blocks with eased top edges and a corner chip. Whole faces carry long cracks: a crack is a plane cut through the block (it runs edge to edge across a face, or stops on the crack it meets),
     and each side of it is chamfered, so the groove is closed and the mass stays solid. In the middle block one face is split once and the smaller piece is set back and turned (the skin). */
  SPEC.skin = [V('', '', [S('H', 'course height', 'mm', 10, 30, .5, 13), S('W', 'block width', 'mm', 14, 40, .5, 20), S('ease', 'top edge radius', 'mm', .7, 2.8, .05, 1.6), S('cracks', 'cracks per face', '', 0, 3, 1, 2), S('cw', 'crack width', 'mm', .4, .8, .05, .5), S('setback', 'skin setback', 'mm', .1, .3, .01, .2), S('turn', 'skin turn', '°', .2, 1.2, .05, .6)])];
  SPEC.skin.opt = { k: 'view', label: 'view', choices: [['whole', 'whole'], ['cut', 'cutaway']] };
  const clip2 = (poly, n, d) => { const out = []; for (let i = 0; i < poly.length; i++) { const a = poly[i], b = poly[(i + 1) % poly.length], da = n[0] * a[0] + n[1] * a[1] - d, db = n[0] * b[0] + n[1] * b[1] - d; if (da <= 1e-9) out.push(a); if ((da < -1e-9 && db > 1e-9) || (da > 1e-9 && db < -1e-9)) { const t = da / (da - db); out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]); } } return out; };
  const area2 = (q) => { let s = 0; for (let i = 0; i < q.length; i++) { const a = q[i], b = q[(i + 1) % q.length]; s += a[0] * b[1] - b[0] * a[1]; } return abs(s) / 2; };
  const cen2 = (q) => { let x = 0, z = 0; q.forEach(v => { x += v[0]; z += v[1]; }); return [x / q.length, z / q.length]; };
  const skinLayout = (p) => {
    const r = E.rng(31), D = 14, bl = [], ws = [.85, .6, .8].map(f => f * p.W), hz = [0, .4, -.5]; let x = 0;
    ws.forEach((w, i) => {
      const b = { x0: x, x1: x + w, y0: 0, y1: D, z0: 0, z1: p.H + hz[i] }; x += w + .4;
      let regs = [{ poly: [[b.x0, b.z0], [b.x1, b.z0], [b.x1, b.z1], [b.x0, b.z1]], cons: [] }]; const cuts = [];
      for (let c = 0; c < (i === 1 ? 1 : p.cracks); c++) {
        let k = 0; regs.forEach((q, j) => { if (area2(q.poly) > area2(regs[k].poly)) k = j; }); const R = regs[k], ct = cen2(R.poly), ph = rad((r() < .5 ? 20 : 110) + 50 * r()), n = [-sin(ph), cos(ph)];
        const sh = (r() - .5) * .3 * (b.x1 - b.x0), d = n[0] * (ct[0] + sh) + n[1] * ct[1], A = clip2(R.poly, n, d), B = clip2(R.poly, [-n[0], -n[1]], -d);
        if (A.length < 3 || B.length < 3 || area2(A) < 6 || area2(B) < 6) continue;
        const on = A.filter(v => abs(n[0] * v[0] + n[1] * v[1] - d) < 1e-6).sort((u, v) => u[0] - v[0] || u[1] - v[1]), seg = [on[0], on[on.length - 1]];
        const top = on.some(v => abs(v[1] - b.z1) < 1e-6), cid = { n: n, d: d, seg: seg, top: top };
        regs.splice(k, 1, { poly: A, cons: R.cons.concat([{ n: n, d: d, out: 1, cut: cid }]) }, { poly: B, cons: R.cons.concat([{ n: [-n[0], -n[1]], d: -d, out: -1, cut: cid }]) }); cuts.push(cid);
      }
      let skin = -1; if (i === 1 && regs.length > 1) { skin = 0; regs.forEach((q, j) => { if (area2(q.poly) < area2(regs[skin].poly)) skin = j; }); }
      bl.push({ b: b, regs: regs, cuts: cuts, skin: skin });
    });
    return { blocks: bl, D: D, W: x };
  };
  /* a chamfer between a face (normal a, offset da) and a cut face (normal b, offset db): hw wide on the first, dp deep on the second */
  const chamf2 = (a, da, b, db, hw, dp, tag) => { const m = E.add(E.mul(a, hw), E.mul(b, dp)), l = hyp(m[0], m[1], m[2]), n = E.mul(m, 1 / l); return E.P(n, (hw * da + dp * db) / l - hw * dp / hyp(hw, dp), tag); };
  MODELS.skin = (p) => {
    p = par('skin', p);
    const L = skinLayout(p), r = E.rng(33), parts = [], cutX = (() => { const b1 = L.blocks[1], q = b1.regs[b1.skin < 0 ? 0 : b1.skin]; return cen2(q.poly)[0]; })();
    L.blocks.forEach((B, bi) => {
      const b = B.b, base = E.blockPlanes(Object.assign({ lean: { '-y': .7, '+z': .8, '+x': .8, '-x': .8 }, burn: true }, b), r);
      edgePlanes(b, nm => { if (nm.indexOf('-z') >= 0 || nm.indexOf('+y') >= 0) return null; if (nm === '-y+z') return { kind: 'soft', c: p.ease }; if (nm.indexOf('+z') >= 0) return { kind: 'soft', c: p.ease * .75 }; return { kind: 'crisp', c: .4 }; }).forEach(q => base.push(q));
      if (bi === 0) base.push(cornerChip(b, -1, -1, 1, .2, r)); if (bi === 2) base.push(cornerChip(b, 1, -1, 1, .15, r));
      B.regs.forEach((R, ri) => {
        const isSkin = bi === 1 && ri === B.skin, pl = base.filter(q => !(isSkin && q.tag === '-y')), yf = isSkin ? p.setback : 0;
        if (isSkin) { const ct = cen2(R.poly), ax = E.norm(E.cross([0, -1, 0], [.8, 0, .6])), a = rad(p.turn), n = E.norm(E.add(E.mul([0, -1, 0], cos(a)), E.mul(E.cross(ax, [0, -1, 0]), sin(a)))); pl.push(E.P(n, E.dot(n, [ct[0], yf, ct[1]]), '-y')); }
        R.cons.forEach(cn => {
          const nc = [cn.n[0], 0, cn.n[1]]; pl.push(E.P(nc, cn.d, 'crack'));
          const hw = p.cw / 2 * (.7 + .6 * r()), dp = hw * (1.8 + .8 * r());
          pl.push(chamf2([0, -1, 0], -yf, nc, cn.d, hw, dp, 'lip'));
          if (cn.cut.top) pl.push(chamf2([0, 0, 1], b.z1, nc, cn.d, hw, dp, 'lip'));
          cn.cut.seg.forEach(e => { if (abs(e[1] - b.z1) < 1e-6 && r() < .6) pl.push(E.cutAt([e[0], yf, e[1]], E.norm(E.add(E.add(nc, [0, -1, 0]), [0, 0, 1])), .45 + .5 * r(), 'chip')); });
        });
        if (p.view === 'cut') pl.push(E.P([-1, 0, 0], -cutX, 'cutaway'));
        parts.push(solid(pl, .97 + .06 * r()));
      });
    });
    return E.solidsMesh(parts, { ground: [-8, -6, L.W + 8, L.D + 8], seed: 9, pitch: 3, facetTone: .08, grey: .8 });
  };
  MODELS.skin.cam = { az: -22, el: 24, dist: 1.15 };
  window.SKINLAYOUT = (p) => skinLayout(par('skin', p));

  /* the mistakes, as models: even round pockets, and uniform noise (no numbers on the page, so no sliders) */
  MODELS.dish_round = () => { const Sp = []; for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) Sp.push({ x: 5.5 + i * 9.5, y: 5.5 + j * 9.5, r: 3.6, d: .8, ph: 0, el: 1 }); return E.hfPatch({ size: 30, N: 150, h: (x, y) => { let cut = 0; for (const q of Sp) { const rr = hyp(x - q.x, y - q.y) / q.r; if (rr < 1) cut = max(cut, q.d * (1 - rr * rr)); } return -cut + grain(x, y, .03, 3); } }); };
  MODELS.uniform = () => E.hfPatch({ size: 30, N: 170, h: (x, y) => .55 * fbm(x * .9, y * .9, 5, 3) + grain(x, y, .12, 9), aoR: 1.6 });
  MODELS.steps.cam = { az: -12, el: 24, dist: 1.0 };
  G.MODELS = MODELS; G.PROFILES = PROFILES; G.SPEC = SPEC;
})(window);
