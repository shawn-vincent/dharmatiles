/* engine.js: our own procedural geometry for the page. No geometry libraries; three.js only draws.
   Units are mm at the 35 mm square. z is up.
   Two builders, both closed solids by construction:
     hf*      a heightfield patch: a rough top over a flat base, four walls and a floor.
     poly*    convex solids from lists of half-spaces (the way the wall generators build blocks).
   Lighting is baked per vertex as cavity shading (horizon occlusion on a heightfield, hemisphere rays on solids). */
(function (G) {
  'use strict';
  const T = THREE, PI = Math.PI, sin = Math.sin, cos = Math.cos, abs = Math.abs, min = Math.min, max = Math.max, sqrt = Math.sqrt, floor = Math.floor;

  /* ---------- small tools ---------- */
  function rng(a) { return function () { a |= 0; a = a + 0x6D2B79F5 | 0; let t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }
  function hash2(i, j, s) { let h = (Math.imul(i, 374761393) + Math.imul(j, 668265263) + Math.imul(s, 1274126177)) | 0; h = Math.imul(h ^ (h >>> 13), 1274126177); return ((h ^ (h >>> 16)) >>> 0) / 4294967296; }
  const sstep = (a, b, x) => { const t = min(1, max(0, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
  const lerp = (a, b, t) => a + (b - a) * t, clamp = (x, a, b) => min(b, max(a, x));
  function vnoise(x, y, s) { const i = floor(x), j = floor(y), fx = x - i, fy = y - j, u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy); const a = hash2(i, j, s), b = hash2(i + 1, j, s), c = hash2(i, j + 1, s), d = hash2(i + 1, j + 1, s); return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v; }
  function fbm(x, y, s, oct) { let a = 0, n = 0, f = 1, m = 1; for (let o = 0; o < (oct || 3); o++) { a += m * (vnoise(x * f, y * f, s + o * 17) - .5); n += m; m *= .5; f *= 2; } return a / n * 2; }   /* about -1..1 */
  const rad = (d) => d * PI / 180;

  /* Voronoi in the plane: nearest and second nearest of one jittered seed per grid cell */
  function voronoi(cs, seed, jit) {
    jit = jit == null ? .85 : jit;
    return function (x, y) {
      const gi = floor(x / cs), gj = floor(y / cs); let d1 = 1e9, d2 = 1e9, s1 = null;
      for (let a = -1; a <= 1; a++) for (let b = -1; b <= 1; b++) {
        const i = gi + a, j = gj + b, px = (i + .5 + (hash2(i, j, seed) - .5) * jit) * cs, py = (j + .5 + (hash2(i, j, seed + 7) - .5) * jit) * cs;
        const d = Math.hypot(x - px, y - py);
        if (d < d1) { d2 = d1; d1 = d; s1 = { i: i, j: j, x: px, y: py }; } else if (d < d2) d2 = d;
      }
      return { d1: d1, d2: d2, e: (d2 - d1) * .5, id: hash2(s1.i, s1.j, seed + 31), id2: hash2(s1.i, s1.j, seed + 53), id3: hash2(s1.i, s1.j, seed + 97), cx: s1.x, cy: s1.y };
    };
  }
  /* signed distance to a polygon (negative inside), pts = [[x,y],...] */
  function polySD(pts, x, y) {
    let d = 1e9, inside = false;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const ax = pts[j][0], ay = pts[j][1], bx = pts[i][0], by = pts[i][1], ex = bx - ax, ey = by - ay, t = clamp(((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey), 0, 1), px = ax + ex * t - x, py = ay + ey * t - y;
      d = min(d, Math.hypot(px, py));
      if (((ay > y) !== (by > y)) && (x < (bx - ax) * (y - ay) / (by - ay) + ax)) inside = !inside;
    }
    return inside ? -d : d;
  }
  function distSeg(px, py, ax, ay, bx, by) { const ex = bx - ax, ey = by - ay, t = clamp(((px - ax) * ex + (py - ay) * ey) / (ex * ex + ey * ey || 1), 0, 1); return { d: Math.hypot(ax + ex * t - px, ay + ey * t - py), t: t, s: (ex * (py - ay) - ey * (px - ax)) >= 0 ? 1 : -1 }; }
  function distPath(pts, x, y) { let best = { d: 1e9, t: 0, s: 1 }, tot = 0, lens = []; for (let i = 0; i < pts.length - 1; i++) { const l = Math.hypot(pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]); lens.push(l); tot += l; } let acc = 0; for (let i = 0; i < pts.length - 1; i++) { const r = distSeg(x, y, pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1]); if (r.d < best.d) best = { d: r.d, t: (acc + r.t * lens[i]) / tot, s: r.s }; acc += lens[i]; } return best; }
  /* a meandering path from a to b */
  function wander(a, b, n, amp, seed) { const pts = [], dx = b[0] - a[0], dy = b[1] - a[1], L = Math.hypot(dx, dy), nx = -dy / L, ny = dx / L; for (let i = 0; i <= n; i++) { const t = i / n, w = (vnoise(t * 3.2, seed, seed) - .5) * 2 * amp * sin(PI * t) ** .5; pts.push([a[0] + dx * t + nx * w, a[1] + dy * t + ny * w]); } return pts; }
  /* an irregular convex-ish polygon about a centre */
  function jag(cx, cy, rx, ry, n, rot, seed, irr) { const r = rng(seed), pts = []; for (let i = 0; i < n; i++) { const a = rot + 2 * PI * (i + (r() - .5) * .5) / n, k = 1 - irr + irr * 2 * r(); pts.push([cx + cos(a) * rx * k, cy + sin(a) * ry * k]); } return pts; }

  /* ---------- heightfield patch ----------
     spec: { size, N, base, h(x,y) -> z, tone(x,y) -> 0.8..1.15 (optional), ao: strength }
     returns { meshes, center, radius } */
  const MAT = () => new T.MeshLambertMaterial({ vertexColors: true });
  function hfPatch(spec) {
    const size = spec.size || 30, N = spec.N || 150, cell = size / N, n1 = N + 1;
    const H = new Float32Array(n1 * n1), Tn = new Float32Array(n1 * n1);
    for (let j = 0; j <= N; j++) for (let i = 0; i <= N; i++) { const x = i * cell, y = j * cell; H[j * n1 + i] = spec.h(x, y); Tn[j * n1 + i] = spec.tone ? spec.tone(x, y) : 1; }
    let hmin = 1e9, hmax = -1e9; for (let k = 0; k < H.length; k++) { hmin = min(hmin, H[k]); hmax = max(hmax, H[k]); }
    const base = (spec.base == null ? 3 : spec.base) - hmin;   /* the floor sits 3 mm below the lowest point */
    /* horizon occlusion: for 8 directions, the steepest rise seen within R mm */
    const R = spec.aoR || 3.2, dist = [.25, .5, .9, 1.5, 2.3, 3.2].filter(d => d <= R + .01), A = spec.ao == null ? 1 : spec.ao, ao = new Float32Array(n1 * n1);
    const dirs = []; for (let k = 0; k < 8; k++) dirs.push([cos(k * PI / 4), sin(k * PI / 4)]);
    for (let j = 0; j <= N; j++) for (let i = 0; i <= N; i++) {
      const h0 = H[j * n1 + i]; let occ = 0;
      for (const d of dirs) { let ms = 0; for (const r of dist) { const ii = Math.round(i + d[0] * r / cell), jj = Math.round(j + d[1] * r / cell); if (ii < 0 || jj < 0 || ii > N || jj > N) break; const s = (H[jj * n1 + ii] - h0) / r; if (s > ms) ms = s; } occ += Math.atan(ms) / (PI / 2); }
      ao[j * n1 + i] = 1 - A * 1.25 * occ / 8;
    }
    const pos = new Float32Array(n1 * n1 * 3), col = new Float32Array(n1 * n1 * 3), g = spec.grey || .74;
    for (let j = 0; j <= N; j++) for (let i = 0; i <= N; i++) { const k = j * n1 + i; pos[k * 3] = i * cell; pos[k * 3 + 1] = j * cell; pos[k * 3 + 2] = H[k]; const c = g * Tn[k] * clamp(ao[k], .25, 1.05); col[k * 3] = c; col[k * 3 + 1] = c; col[k * 3 + 2] = c * 1.025; }
    const idx = []; for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) { const a = j * n1 + i, b = a + 1, c = a + n1, d = c + 1; idx.push(a, b, d, a, d, c); }
    const top = new T.BufferGeometry(); top.setAttribute('position', new T.BufferAttribute(pos, 3)); top.setAttribute('color', new T.BufferAttribute(col, 3)); top.setIndex(idx); top.computeVertexNormals();
    /* walls and floor, flat colours */
    const wp = [], wc = [], wcol = g * .62;
    function quad(a, b, c, d) { [a, b, c, a, c, d].forEach(p => { wp.push(p[0], p[1], p[2]); wc.push(wcol, wcol, wcol * 1.02); }); }
    for (let i = 0; i < N; i++) {
      const x0 = i * cell, x1 = (i + 1) * cell, h0 = H[i], h1 = H[i + 1], k0 = N * n1 + i, k1 = k0 + 1;
      quad([x0, 0, -base], [x1, 0, -base], [x1, 0, h1], [x0, 0, h0]);
      quad([x1, size, -base], [x0, size, -base], [x0, size, H[k0]], [x1, size, H[k1]]);
      const y0 = i * cell, y1 = (i + 1) * cell, l0 = H[i * n1], l1 = H[(i + 1) * n1], r0 = H[i * n1 + N], r1 = H[(i + 1) * n1 + N];
      quad([0, y1, -base], [0, y0, -base], [0, y0, l0], [0, y1, l1]);
      quad([size, y0, -base], [size, y1, -base], [size, y1, r1], [size, y0, r0]);
    }
    quad([0, size, -base], [size, size, -base], [size, 0, -base], [0, 0, -base]);
    const wg = new T.BufferGeometry(); wg.setAttribute('position', new T.Float32BufferAttribute(wp, 3)); wg.setAttribute('color', new T.Float32BufferAttribute(wc, 3)); wg.computeVertexNormals();
    const m1 = new T.Mesh(top, MAT()), m2 = new T.Mesh(wg, MAT());
    return { meshes: [m1, m2], center: [size / 2, size / 2, (hmin + hmax) / 2], radius: size * .93, hmin: hmin, hmax: hmax, H: H, N: N, size: size };
  }

  /* ---------- convex solids from half-spaces ---------- */
  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2], sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]], add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]], mul = (a, k) => [a[0] * k, a[1] * k, a[2] * k];
  const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]], norm = (a) => { const l = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };
  function clipPoly(poly, n, d) {
    const out = [];
    for (let i = 0; i < poly.length; i++) {
      const a = poly[i], b = poly[(i + 1) % poly.length], da = dot(n, a) - d, db = dot(n, b) - d;
      if (da <= 1e-9) out.push(a);
      if ((da < -1e-9 && db > 1e-9) || (da > 1e-9 && db < -1e-9)) { const t = da / (da - db); out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]); }
    }
    return out;
  }
  /* planes: [{n:[x,y,z] unit, d, tag}] keep n.p <= d. Returns faces [{n, pts, tag}] */
  function solidFromPlanes(planes) {
    const faces = [];
    planes.forEach((pl, i) => {
      const n = pl.n, helper = abs(n[2]) < .9 ? [0, 0, 1] : [1, 0, 0], u = norm(cross(n, helper)), v = cross(n, u), c = mul(n, pl.d), L = 400;
      let poly = [add(c, add(mul(u, L), mul(v, L))), add(c, add(mul(u, -L), mul(v, L))), add(c, add(mul(u, -L), mul(v, -L))), add(c, add(mul(u, L), mul(v, -L)))];
      /* orient so that the polygon normal is n */
      const cn = cross(sub(poly[1], poly[0]), sub(poly[2], poly[0])); if (dot(cn, n) < 0) poly.reverse();
      for (let j = 0; j < planes.length && poly.length >= 3; j++) if (j !== i) poly = clipPoly(poly, planes[j].n, planes[j].d);
      if (poly.length >= 3) { let ar = 0; for (let k = 1; k < poly.length - 1; k++) ar += Math.hypot.apply(null, cross(sub(poly[k], poly[0]), sub(poly[k + 1], poly[0]))) / 2; if (ar > 1e-3) faces.push({ n: n, pts: poly, tag: pl.tag || '', area: ar }); }
    });
    return faces;
  }
  const P = (n, d, tag) => { n = norm(n); return { n: n, d: d, tag: tag }; };
  /* a tilted face plane: nominal normal n0 through point p, leaned by angle (deg) about a random axis */
  function leaned(n0, p, ang, r, tag) {
    const ax = norm(cross(n0, [r() - .5, r() - .5, r() - .5])), a = rad(ang), n = norm(add(mul(n0, cos(a)), mul(cross(ax, n0), sin(a))));
    return P(n, dot(n, p), tag);
  }
  /* an axis aligned box as six planes, optionally leaned, plus chamfer planes on chosen edges, plus extra planes (chips) */
  function blockPlanes(b, r) {
    const x0 = b.x0, x1 = b.x1, y0 = b.y0, y1 = b.y1, z0 = b.z0, z1 = b.z1, lean = b.lean || {}, pl = [];
    const c = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
    const faces = [['-x', [-1, 0, 0], [x0, c[1], c[2]]], ['+x', [1, 0, 0], [x1, c[1], c[2]]], ['-y', [0, -1, 0], [c[0], y0, c[2]]], ['+y', [0, 1, 0], [c[0], y1, c[2]]], ['-z', [0, 0, -1], [c[0], c[1], z0]], ['+z', [0, 0, 1], [c[0], c[1], z1]]];
    faces.forEach(f => { const a = lean[f[0]] || 0; pl.push(a ? leaned(f[1], f[2], a * (r() < .5 ? -1 : 1) * (.4 + .6 * r()), r, f[0]) : P(f[1], dot(f[1], f[2]), f[0])); });
    return pl;
  }
  /* exact: plane through the points E - c*b (on face a) and E - c*a (on face b) */
  function chamferExact(a, b, E, c, tag) { const n = norm(add(a, b)); const p = sub(E, mul(b, c)); return P(n, dot(n, p), tag || 'chamfer'); }
  /* a cut aimed at a point: plane with normal n through (point - depth*n) */
  function cutAt(point, n, depth, tag) { n = norm(n); return P(n, dot(n, point) - depth, tag || 'chip'); }

  /* ---------- solids to a mesh with cavity shading ---------- */
  const AODIR = (function () { const a = [], n = 16; for (let i = 0; i < n; i++) { const u = (i + .5) / n, v = (i * .618034) % 1, r = sqrt(u), ph = 2 * PI * v; a.push([r * cos(ph), r * sin(ph), sqrt(1 - u)]); } return a; })();
  function Soup() { this.tri = []; }  /* triangles to cast against */
  function bakeAO(tris, verts, opts) {
    const CS = 3, mn = [1e9, 1e9, 1e9], mx = [-1e9, -1e9, -1e9];
    tris.forEach(q => { for (let v = 0; v < 3; v++) for (let k = 0; k < 3; k++) { mn[k] = min(mn[k], q[v * 3 + k]); mx[k] = max(mx[k], q[v * 3 + k]); } });
    const D = [0, 1, 2].map(k => Math.ceil((mx[k] - mn[k]) / CS) + 1), grid = new Map(), key = (x, y, z) => (x * D[1] + y) * D[2] + z;
    tris.forEach((q, ti) => { const lo = [0, 1, 2].map(k => floor((min(q[k], q[3 + k], q[6 + k]) - mn[k]) / CS)), hi = [0, 1, 2].map(k => floor((max(q[k], q[3 + k], q[6 + k]) - mn[k]) / CS)); for (let x = lo[0]; x <= hi[0]; x++) for (let y = lo[1]; y <= hi[1]; y++) for (let z = lo[2]; z <= hi[2]; z++) { const kk = key(x, y, z); let l = grid.get(kk); if (!l) { l = []; grid.set(kk, l); } l.push(ti); } });
    const stamp = new Int32Array(tris.length); let sid = 0;
    function hit(o, d, tmax) {
      sid++; let best = 1e9;
      for (let s = 0; s < tmax + CS; s += CS * .5) {
        const ix = floor((o[0] + d[0] * s - mn[0]) / CS), iy = floor((o[1] + d[1] * s - mn[1]) / CS), iz = floor((o[2] + d[2] * s - mn[2]) / CS);
        if (ix < 0 || iy < 0 || iz < 0 || ix >= D[0] || iy >= D[1] || iz >= D[2]) continue;
        const l = grid.get(key(ix, iy, iz)); if (!l) continue;
        for (let a = 0; a < l.length; a++) {
          const ti = l[a]; if (stamp[ti] === sid) continue; stamp[ti] = sid; const q = tris[ti];
          const e1x = q[3] - q[0], e1y = q[4] - q[1], e1z = q[5] - q[2], e2x = q[6] - q[0], e2y = q[7] - q[1], e2z = q[8] - q[2], px = d[1] * e2z - d[2] * e2y, py = d[2] * e2x - d[0] * e2z, pz = d[0] * e2y - d[1] * e2x, det = e1x * px + e1y * py + e1z * pz;
          if (abs(det) < 1e-9) continue; const id = 1 / det, tx = o[0] - q[0], ty = o[1] - q[1], tz = o[2] - q[2], u = (tx * px + ty * py + tz * pz) * id; if (u < 0 || u > 1) continue;
          const qx = ty * e1z - tz * e1y, qy = tz * e1x - tx * e1z, qz = tx * e1y - ty * e1x, v = (d[0] * qx + d[1] * qy + d[2] * qz) * id; if (v < 0 || u + v > 1) continue;
          const tt = (e2x * qx + e2y * qy + e2z * qz) * id; if (tt > .02 && tt < best) best = tt;
        }
        if (best < s + CS) break;
      }
      return best;
    }
    const maxd = opts.maxd || 7, out = new Float32Array(verts.length), cache = new Map();
    verts.forEach((vt, vi) => {
      const p = vt.p, n = vt.n, kk = p.map(x => Math.round(x * 20)).join(',') + '|' + n.map(x => Math.round(x * 8)).join(',');
      let c = cache.get(kk);
      if (c === undefined) {
        const tt = abs(n[2]) < .9 ? [0, 0, 1] : [1, 0, 0], u = norm(cross(n, tt)), v = cross(n, u), o = add(p, mul(n, .06)); let occ = 0;
        for (const a of AODIR) { const d = norm(add(add(mul(u, a[0]), mul(v, a[1])), mul(n, a[2]))), t = hit(o, d, maxd); if (t < maxd) occ += 1 - t / maxd * .55; }
        c = 1 - 1.25 * occ / AODIR.length; cache.set(kk, c);
      }
      out[vi] = c;
    });
    return out;
  }
  /* parts: [{faces, tone, dark}] ; ground: optional z=0 plate [x0,y0,x1,y1] drawn and cast against.
     Faces are tessellated to about 'pitch' mm so the cavity shading has somewhere to vary. */
  function solidsMesh(parts, opts) {
    opts = opts || {}; const pitch = opts.pitch || 3.4, r = rng(opts.seed || 5), tris = [], verts = [], pos = [], nor = [], tone = [];
    function sub3(a, b, c, n, tb) {
      const L = max(Math.hypot.apply(null, sub(b, a)), Math.hypot.apply(null, sub(c, b)), Math.hypot.apply(null, sub(a, c))), m = clamp(Math.ceil(L / pitch), 1, 7);
      const Pt = (i, j) => [a[0] + (b[0] - a[0]) * i / m + (c[0] - a[0]) * j / m, a[1] + (b[1] - a[1]) * i / m + (c[1] - a[1]) * j / m, a[2] + (b[2] - a[2]) * i / m + (c[2] - a[2]) * j / m];
      const emit = (p, q, s) => { [p, q, s].forEach(v => { pos.push(v[0], v[1], v[2]); nor.push(n[0], n[1], n[2]); verts.push({ p: v, n: n }); tone.push(tb); }); tris.push([p[0], p[1], p[2], q[0], q[1], q[2], s[0], s[1], s[2]]); };
      for (let i = 0; i < m; i++) for (let j = 0; j < m - i; j++) { emit(Pt(i, j), Pt(i + 1, j), Pt(i, j + 1)); if (j < m - i - 1) emit(Pt(i + 1, j), Pt(i + 1, j + 1), Pt(i, j + 1)); }
    }
    parts.forEach(pt => pt.faces.forEach(f => {
      const tb = (pt.tone || 1) * (opts.facetTone == null ? .2 : opts.facetTone) * (r() - .5) * 2 + (pt.tone || 1) + (pt.dark ? 0 : 0);
      for (let k = 1; k < f.pts.length - 1; k++) {
        let a = f.pts[0], b = f.pts[k], c = f.pts[k + 1];
        if (dot(cross(sub(b, a), sub(c, a)), f.n) < 0) { const t = b; b = c; c = t; }
        sub3(a, b, c, f.n, tb);
      }
    }));
    let ground = null;
    if (opts.ground) { const g = opts.ground, gv = [[g[0], g[1], 0], [g[2], g[1], 0], [g[2], g[3], 0], [g[0], g[3], 0]]; ground = gv; tris.push([].concat(gv[0], gv[1], gv[2])); tris.push([].concat(gv[0], gv[2], gv[3])); }
    const ao = bakeAO(tris, verts, opts), col = new Float32Array(pos.length), base = opts.grey || .66;
    for (let i = 0; i < verts.length; i++) { const c = base * tone[i] * (.3 + .7 * clamp(ao[i], 0, 1) ** 1.35); col[i * 3] = c; col[i * 3 + 1] = c; col[i * 3 + 2] = c * 1.025; }
    const g = new T.BufferGeometry(); g.setAttribute('position', new T.Float32BufferAttribute(pos, 3)); g.setAttribute('normal', new T.Float32BufferAttribute(nor, 3)); g.setAttribute('color', new T.BufferAttribute(col, 3));
    const meshes = [new T.Mesh(g, MAT())];
    if (ground) {
      /* the plate under it, a closed slab 2 mm thick */
      const g0 = opts.ground, pl = solidFromPlanes([P([0, 0, 1], 0, 'top'), P([0, 0, -1], 2, 'bot'), P([-1, 0, 0], -g0[0], 'a'), P([1, 0, 0], g0[2], 'b'), P([0, -1, 0], -g0[1], 'c'), P([0, 1, 0], g0[3], 'd')]);
      const pp = [], pn = [], pc = [];
      pl.forEach(f => { for (let k = 1; k < f.pts.length - 1; k++) { let a = f.pts[0], b = f.pts[k], c = f.pts[k + 1]; if (dot(cross(sub(b, a), sub(c, a)), f.n) < 0) { const t = b; b = c; c = t; } [a, b, c].forEach(v => { pp.push(v[0], v[1], v[2]); pn.push(f.n[0], f.n[1], f.n[2]); const cc = (f.tag === 'top' ? .62 : .5); pc.push(cc, cc, cc * 1.02); }); } });
      const gg = new T.BufferGeometry(); gg.setAttribute('position', new T.Float32BufferAttribute(pp, 3)); gg.setAttribute('normal', new T.Float32BufferAttribute(pn, 3)); gg.setAttribute('color', new T.Float32BufferAttribute(pc, 3));
      meshes.push(new T.Mesh(gg, MAT()));
    }
    let mn = [1e9, 1e9, 1e9], mx = [-1e9, -1e9, -1e9]; parts.forEach(pt => pt.faces.forEach(f => f.pts.forEach(p => { for (let k = 0; k < 3; k++) { mn[k] = min(mn[k], p[k]); mx[k] = max(mx[k], p[k]); } })));
    const ctr = [(mn[0] + mx[0]) / 2, (mn[1] + mx[1]) / 2, (mn[2] + mx[2]) / 2], rr = Math.hypot(mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]) / 2;
    return { meshes: meshes, center: ctr, radius: rr, tris: tris.length };
  }

  /* ---------- viewer: one shared renderer, a still image per view, orbit on demand ---------- */
  let renderer = null;
  function getRenderer() {
    if (renderer) return renderer;
    renderer = new T.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    renderer.setClearColor(0x000000, 0); return renderer;
  }
  const RELIEF = { k: 1 };
  function makeView(el, model, opt) {
    opt = opt || {};
    const canvas = el.querySelector('canvas'), ctx = canvas.getContext('2d'), scene = new T.Scene(), cam = new T.PerspectiveCamera(opt.fov || 28, 1, .5, 2000);
    cam.up.set(0, 0, 1);
    const grp = new T.Group(); model.meshes.forEach(m => grp.add(m)); scene.add(grp);
    scene.add(new T.HemisphereLight(0xf3f5fa, 0x7d7f88, opt.hemi == null ? .74 : opt.hemi));
    const key = new T.DirectionalLight(0xffffff, opt.key == null ? .5 : opt.key); scene.add(key); scene.add(key.target);
    const st = { az: opt.az == null ? -28 : opt.az, el: opt.el == null ? 38 : opt.el, d: (opt.dist || 1) * model.radius * 3.35, z: 0, flat: !!opt.flat };
    const c0 = model.center, relK = opt.relief !== false;
    function draw() {
      const r = getRenderer(), w = canvas.clientWidth || 400, h = canvas.clientHeight || 300, dpr = min(window.devicePixelRatio || 1, 2);
      if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
      r.setPixelRatio(1); r.setSize(canvas.width, canvas.height, false); cam.aspect = w / h; cam.updateProjectionMatrix();
      const a = rad(st.az), e = rad(st.el), d = st.d / (st.z || 1);
      grp.scale.set(1, 1, relK ? RELIEF.k : 1); grp.position.set(0, 0, 0);
      cam.position.set(c0[0] + d * cos(e) * sin(a), c0[1] - d * cos(e) * cos(a), c0[2] * 1 + d * sin(e)); cam.lookAt(c0[0], c0[1], c0[2]);
      /* key light: soft, from the upper left front of the camera, so relief shows */
      const ka = a - rad(55), ke = rad(opt.keyEl || 42); key.position.set(c0[0] + 100 * cos(ke) * sin(ka), c0[1] - 100 * cos(ke) * cos(ka), c0[2] + 100 * sin(ke)); key.target.position.set(c0[0], c0[1], c0[2]);
      r.render(scene, cam);
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const gr = ctx.createRadialGradient(canvas.width * .5, canvas.height * .42, 10, canvas.width * .5, canvas.height * .5, canvas.width * .75); gr.addColorStop(0, '#9a9ea7'); gr.addColorStop(1, '#6a6e77');
      ctx.fillStyle = gr; ctx.fillRect(0, 0, canvas.width, canvas.height); ctx.drawImage(r.domElement, 0, 0);
    }
    let drag = null, raf = 0; const ask = () => { if (!raf) raf = requestAnimationFrame(() => { raf = 0; draw(); }); };
    canvas.addEventListener('pointerdown', e => { drag = { x: e.clientX, y: e.clientY, az: st.az, el: st.el, t: e.pointerType }; try { canvas.setPointerCapture(e.pointerId); } catch (x) { /* capture is best effort */ } el.classList.add('live'); });
    canvas.addEventListener('pointermove', e => { if (!drag) return; st.az = drag.az - (e.clientX - drag.x) * .45; if (drag.t !== 'touch') st.el = clamp(drag.el + (e.clientY - drag.y) * .35, 4, 86); ask(); });
    const up = () => { drag = null; el.classList.remove('live'); };
    canvas.addEventListener('pointerup', up); canvas.addEventListener('pointercancel', up);
    canvas.addEventListener('dblclick', () => { st.az = opt.az == null ? -28 : opt.az; st.el = opt.el == null ? 38 : opt.el; st.z = 0; ask(); });
    canvas.addEventListener('wheel', e => { if (!e.shiftKey && !e.ctrlKey && !e.altKey) return; e.preventDefault(); st.z = clamp((st.z || 1) * (e.deltaY < 0 ? 1.1 : .9), .5, 4); ask(); }, { passive: false });
    return { draw: ask, now: draw, st: st };
  }

  G.Eng = { rng, hash2, sstep, lerp, clamp, vnoise, fbm, voronoi, polySD, distPath, distSeg, wander, jag, hfPatch, solidFromPlanes, blockPlanes, chamferExact, cutAt, leaned, P, norm, cross, dot, add, sub, mul, solidsMesh, makeView, RELIEF, rad };
})(window);
