/* view.js: turns the solids of wall.js into a lit mesh (hidden faces dropped, cavity shading baked per vertex) and draws it.
   Lighting as the page specifies: grey, a hemisphere fill (0.6), one soft key from above front left, cavity darkening (rays over about 7 mm). */
(function (G) {
  'use strict';
  const E = G.Eng, T = THREE, dot = E.dot, sub = E.sub, add = E.add, mul = E.mul, cross = E.cross, norm = E.norm, rad = E.rad;
  const min = Math.min, max = Math.max;

  function inside(part, p) {
    if (p[0] < part.mn[0] || p[1] < part.mn[1] || p[2] < part.mn[2] || p[0] > part.mx[0] || p[1] > part.mx[1] || p[2] > part.mx[2]) return false;
    const pl = part.planes; for (let i = 0; i < pl.length; i++) if (dot(pl[i].n, p) > pl[i].d + 1e-6) return false; return true;
  }
  function buildMesh(w, opt) {
    opt = opt || {}; const pitch = opt.pitch || 2.6, parts = w.parts, tris = [], verts = [], pos = [], nor = [], tone = [], tint = [];
    parts.forEach((pt, pi) => pt.faces.forEach(f => {
      for (let k = 1; k < f.pts.length - 1; k++) {
        let a = f.pts[0], b = f.pts[k], c = f.pts[k + 1];
        if (dot(cross(sub(b, a), sub(c, a)), f.n) < 0) { const t = b; b = c; c = t; }
        const L = max(Math.hypot.apply(null, sub(b, a)), Math.hypot.apply(null, sub(c, b)), Math.hypot.apply(null, sub(a, c))), m = E.clamp(Math.ceil(L / pitch), 1, 9);
        const Pt = (i, j) => [a[0] + (b[0] - a[0]) * i / m + (c[0] - a[0]) * j / m, a[1] + (b[1] - a[1]) * i / m + (c[1] - a[1]) * j / m, a[2] + (b[2] - a[2]) * i / m + (c[2] - a[2]) * j / m];
        const emit = (p, q, s) => {
          const ce = [(p[0] + q[0] + s[0]) / 3, (p[1] + q[1] + s[1]) / 3, (p[2] + q[2] + s[2]) / 3], pp = add(ce, mul(f.n, .05));
          for (let o = 0; o < parts.length; o++) if (o !== pi && inside(parts[o], pp)) return;
          [p, q, s].forEach(v => { pos.push(v[0], v[1], v[2]); nor.push(f.n[0], f.n[1], f.n[2]); verts.push({ p: v, n: f.n }); tone.push(pt.tone); tint.push(opt.debug ? (pt.tag.startsWith('front') ? [1,1,1] : pt.tag.startsWith('top') ? [1.5,.5,.5] : pt.tag.startsWith('end') ? [.5,.5,1.5] : pt.tag==='core' ? [.5,1.5,.5] : [1.4,1.2,.4]) : [1,1,1]); });
          tris.push([p[0], p[1], p[2], q[0], q[1], q[2], s[0], s[1], s[2]]);
        };
        for (let i = 0; i < m; i++) for (let j = 0; j < m - i; j++) { emit(Pt(i, j), Pt(i + 1, j), Pt(i, j + 1)); if (j < m - i - 1) emit(Pt(i + 1, j), Pt(i + 1, j + 1), Pt(i, j + 1)); }
      }
    }));
    const g0 = w.ground, gv = [[g0[0], g0[1], 0], [g0[2], g0[1], 0], [g0[2], g0[3], 0], [g0[0], g0[3], 0]];
    const wallTris = tris.length;
    tris.push([].concat(gv[0], gv[1], gv[2])); tris.push([].concat(gv[0], gv[2], gv[3]));
    const ao = E.bakeAO(tris, verts, { maxd: 7 }), col = new Float32Array(pos.length), base = opt.grey || .7;
    for (let i = 0; i < verts.length; i++) { const c = base * tone[i] * (.32 + .68 * E.clamp(ao[i], 0, 1) ** 1.35); col[i * 3] = c * tint[i][0]; col[i * 3 + 1] = c * tint[i][1]; col[i * 3 + 2] = c * 1.02 * tint[i][2]; }
    const g = new T.BufferGeometry(); g.setAttribute('position', new T.Float32BufferAttribute(pos, 3)); g.setAttribute('normal', new T.Float32BufferAttribute(nor, 3)); g.setAttribute('color', new T.BufferAttribute(col, 3));
    const meshes = [new T.Mesh(g, E.MAT())];
    /* the 35 mm plate, a closed slab 2.5 mm thick, with the wall's footprint darkened by the same cavity rays */
    const plate = E.solidFromPlanes([E.P([0, 0, 1], 0, 'top'), E.P([0, 0, -1], 2.5, 'bot'), E.P([-1, 0, 0], 0, 'a'), E.P([1, 0, 0], 35, 'b'), E.P([0, -1, 0], 0, 'c'), E.P([0, 1, 0], 35, 'd')]);
    const pp = [], pn = [], pc = [], N = 14, gt = [];
    plate.forEach(f => { if (f.tag !== 'top') { for (let k = 1; k < f.pts.length - 1; k++) [f.pts[0], f.pts[k], f.pts[k + 1]].forEach(v => { pp.push(v[0], v[1], v[2]); pn.push(f.n[0], f.n[1], f.n[2]); pc.push(.46, .46, .47); }); } });
    /* the top of the plate, tessellated so the shading can vary */
    const topv = [], ptris = [];
    for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) { const x0 = i * 35 / N, x1 = (i + 1) * 35 / N, y0 = j * 35 / N, y1 = (j + 1) * 35 / N; [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]].forEach(q => { topv.push({ p: [q[0], q[1], 0], n: [0, 0, 1] }); }); }
    const tao = E.bakeAO(tris, topv, { maxd: 7 });
    topv.forEach((v, i) => { pp.push(v.p[0], v.p[1], 0); pn.push(0, 0, 1); const c = .66 * (.32 + .68 * E.clamp(tao[i], 0, 1) ** 1.35); pc.push(c, c, c * 1.02); });
    const gg = new T.BufferGeometry(); gg.setAttribute('position', new T.Float32BufferAttribute(pp, 3)); gg.setAttribute('normal', new T.Float32BufferAttribute(pn, 3)); gg.setAttribute('color', new T.Float32BufferAttribute(pc, 3));
    meshes.push(new T.Mesh(gg, E.MAT()));
    /* the 28 mm figure stand-in: a round base, a cylinder, a ball */
    if (opt.figure !== false) {
      const fm = new T.MeshLambertMaterial({ color: 0x8a8d94 }), fg = new T.Group();
      const base = new T.Mesh(new T.CylinderGeometry(7.5, 8.2, 2.2, 40), fm); base.rotation.x = Math.PI / 2; base.position.set(0, 0, 1.1);
      const body = new T.Mesh(new T.CylinderGeometry(3.4, 4.3, 20, 28), fm); body.rotation.x = Math.PI / 2; body.position.set(0, 0, 2.2 + 10);
      const head = new T.Mesh(new T.SphereGeometry(3.1, 24, 16), fm); head.position.set(0, 0, 2.2 + 20 + 3.4 - 2.6);
      fg.add(base, body, head); const fp = opt.figAt || [8, 9]; fg.position.set(fp[0], fp[1], 0); meshes.push(fg);
    }
    return { meshes: meshes, tris: wallTris, center: [17.5, 20, 15], radius: 28 };
  }

  let renderer = null;
  function getRenderer() { if (!renderer) { renderer = new T.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true }); renderer.setClearColor(0, 0); } return renderer; }
  /* camera: az (deg, + moves the eye toward +x), el, dist (mm from the target), target, fov */
  function drawTo(canvas, model, cam, light) {
    const r = getRenderer(), scene = new T.Scene(), camera = new T.PerspectiveCamera(cam.fov || 28, 1, .5, 2000); camera.up.set(0, 0, 1);
    const grp = new T.Group(); model.meshes.forEach(m => grp.add(m)); scene.add(grp);
    light = light || {};
    scene.add(new T.HemisphereLight(0xf3f5fa, 0x80828a, light.hemi == null ? .62 : light.hemi));
    const key = new T.DirectionalLight(0xffffff, light.key == null ? .55 : light.key); scene.add(key); scene.add(key.target);
    const w = canvas.width, h = canvas.height; r.setPixelRatio(1); r.setSize(w, h, false); camera.aspect = w / h; camera.updateProjectionMatrix();
    const a = rad(cam.az), e = rad(cam.el), d = cam.dist, t = cam.target || model.center;
    camera.position.set(t[0] + d * Math.cos(e) * Math.sin(a), t[1] - d * Math.cos(e) * Math.cos(a), t[2] + d * Math.sin(e)); camera.lookAt(t[0], t[1], t[2]);
    const ka = a - rad(55), ke = rad(light.keyEl || 42); key.position.set(t[0] + 100 * Math.cos(ke) * Math.sin(ka), t[1] - 100 * Math.cos(ke) * Math.cos(ka), t[2] + 100 * Math.sin(ke)); key.target.position.set(t[0], t[1], t[2]);
    r.render(scene, camera);
    const ctx = canvas.getContext('2d'); ctx.clearRect(0, 0, w, h);
    const gr = ctx.createRadialGradient(w * .5, h * .42, 10, w * .5, h * .5, w * .8); gr.addColorStop(0, '#86898f'); gr.addColorStop(1, '#4c4e54');
    ctx.fillStyle = gr; ctx.fillRect(0, 0, w, h); ctx.drawImage(r.domElement, 0, 0);
  }
  const cache = {};
  function modelFor(seed, cfgName, stage, opt) {
    const k = [seed, cfgName, stage].join('/'); if (!cache[k]) { const w = G.genN(seed, G.WALLCFG[cfgName], stage); cache[k] = buildMesh(w, opt); cache[k].stats = w.stats; } return cache[k];
  }
  G.WV = { buildMesh: buildMesh, drawTo: drawTo, modelFor: modelFor };
})(window);
