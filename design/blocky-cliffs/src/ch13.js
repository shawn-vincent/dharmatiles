/* ---------- chapter 13 ---------- */
{
  const live = $('#g-live'), FIG = [1.25, .45];
  const CLS = [['pillars', 'Pillars'], ['blocks', 'Blocks'], ['leaners', 'Leaning fins'], ['ramp', 'Dipping beds'], ['courses', 'Courses'], ['gates', 'Gates']];
  const defs = [['height', 'Height', 1, 2.1, .01], ['length', 'Length', .8, 1.7, .01], ['depth', 'Depth', .5, 1.2, .01], ['nA', 'Stones along', 1, 5, 1], ['rows', 'Rows', 1, 4, 1], ['fall', 'Fall to the far end', 0, .8, .01], ['front', 'Front row height', .15, 1, .01], ['lean', 'Common lean, degrees', 0, 32, 1], ['dip', 'Beds dip, degrees', 0, 35, 1], ['cutEvery', 'Level joint every', 0, .9, .01], ['cap', 'Caps', 0, 2, 1], ['bevel', 'Bevel', 0, .4, .01]];
  let P = { preset: 'pillars', cls: 'pillars', seed: 7 }, cam = { az: 45, el: 20, dist: 9.6 }, F = null, R3 = null, scene = null, W = 0, H = 0;
  const ctl = $('#g-ctl'), inputs = {};
  const render = () => { if (!R3 || !scene || !W) return; R3.render(scene, cameraFor('ref', W, H, { az: cam.az, el: cam.el, dist: cam.dist, fov: 19, centre: [PLATE / 2, PLATE / 2, .78] })); };
  const rebuild = () => {
    F = formation(P); const ms = formMeshes(F);
    if (HAVE3) { if (scene) scene.traverse((m) => { if (m.geometry) m.geometry.dispose(); if (m.material) m.material.dispose(); }); scene = buildScene(ms, { figure: FIG }); render(); }
    const q = F.q, st = $('#g-steps'), o = { w: 220, h: 200 }, flat = { bevel: 0, bevelV: 0, bevelB: 0, taperA: 1, taperD: 1, dipA: 0, dipD: 0, botA: 0, cut: 0, jit: 0, yaw: 0, leanA: 0, leanD: 0 };
    const rowBoxes = F.rows.map((r) => stoneMesh(Object.assign({ w: r.len, dp: r.dp - .01, h: Math.max(...r.cols.map((c) => c.h)), a: r.len / 2, d: r.d0 + r.dp / 2, z: 0 }, flat)));
    const colBoxes = F.rows.flatMap((r) => r.cols.map((c) => stoneMesh(Object.assign({ w: c.w - .01, dp: r.dp * (1 - (c.recess || 0)) - .01, h: c.h, a: c.a0 + c.w / 2, d: r.d0 + r.dp * (c.recess || 0) + r.dp * (1 - (c.recess || 0)) / 2, z: 0 }, flat))));
    const tl = Math.tan(q.lean * Math.PI / 180), sh = Math.max(0, tl * q.height * .8);
    const cutBoxes = F.stones.map((s0) => stoneMesh(Object.assign({}, s0, flat, { a: s0.a + tl * s0.z - sh })));
    const a = still([], o); { const d0 = PLATE - q.depth, e = (aa, t) => q.height * lerp(q.front, 1, t) * (1 - q.fall * Math.max(0, aa - q.peak * q.length) / (q.length * (1 - q.peak))); const fp = [[0, d0, 0], [0, PLATE, 0], [q.length, PLATE, 0], [q.length, d0, 0]].map((p) => a.proj(...p)), en = [[0, d0, e(0, 0)], [0, PLATE, e(0, 1)], [q.length, PLATE, e(q.length, 1)], [q.length, d0, e(q.length, 0)]].map((p) => a.proj(...p));
      a.svg.append(s('polygon', { points: pts(fp), fill: 'var(--a-out)', 'fill-opacity': .25, stroke: 'var(--a-out)', 'stroke-width': 2 }), s('polygon', { points: pts(en), fill: 'var(--a-top)', 'fill-opacity': .2, stroke: 'var(--a-top)', 'stroke-width': 2, 'stroke-dasharray': '5 4' }), ...fp.map((p, i) => s('line', { x1: p[0], y1: p[1], x2: en[i][0], y2: en[i][1], stroke: 'var(--a-top)', 'stroke-width': 1 }))); }
    st.replaceChildren(a, still(rowBoxes, o), still(colBoxes, o), still(cutBoxes, o), still(ms, o));
    [...st.children].forEach((c, i) => c.append(h('span', { class: 'tag', text: ['1 footprint, envelope', '2 rows', '3 columns', '4 cuts, caps', '5 shaped'][i] })));
    const m = HAVE3 ? seen(F) : null; if (m) $('#g-read').innerHTML = `<span class="ev p">Proposed</span> Seed ${P.seed} · ${m.n} stones · as seen ${fmt(m.W)} fh wide, ${fmt(m.H)} fh high (references: 1.3–2.1 and 1.9–2.2).`;
    const bs = $('#g-beside'), refs = ALL.filter((id) => CL[id].class === F.q.cls).slice(0, 4);
    bs.replaceChildren(...[1, 2, 3, 4].map((k) => still(formMeshes(formation(Object.assign({}, P, { seed: P.seed * 13 + k }))), { w: 300, h: 260, figure: FIG })), ...refs.map((id) => cellView(id, { stones: false })));
  };
  const sync = () => { const q = formation(P).q; for (const [k, , , , stp] of defs) { inputs[k][0].value = q[k]; inputs[k][1].textContent = fmt(q[k], stp < 1 ? 2 : 0); } };
  for (const [k, name, a, b, stp] of defs) { const out = h('output', { for: 'g-' + k }), inp = h('input', { type: 'range', id: 'g-' + k, min: a, max: b, step: stp, oninput: () => { P[k] = +inp.value; out.textContent = fmt(P[k], stp < 1 ? 2 : 0); rebuild(); } }); inputs[k] = [inp, out]; ctl.append(h('label', { for: 'g-' + k, text: name }), inp, out); }
  const pills = $('#g-cls'), press = (k) => { for (const b of pills.children) b.setAttribute('aria-pressed', b.dataset.k === k); };
  for (const [k, name] of CLS) pills.append(h('button', { type: 'button', 'data-k': k, text: name, onclick: () => { P = { preset: k, cls: k === 'ramp' ? 'leaners' : k, seed: P.seed }; press(k); sync(); rebuild(); } }));
  $('#g-seed').addEventListener('click', () => { P.seed = 1 + Math.floor(Math.random() * 9999); rebuild(); });
  $('#g-all').addEventListener('click', () => { P = drawParams(1 + Math.floor(Math.random() * 99999)); press(P.preset); sync(); rebuild(); });
  $('#g-cam').addEventListener('click', () => { cam = { az: 45, el: 20, dist: 9.6 }; render(); });
  if (HAVE3) {
    R3 = new THREE.WebGLRenderer({ antialias: true }); R3.setClearColor(STAGE, 1); R3.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2)); const cv = R3.domElement; live.append(cv);
    /* the view's size is its box's width (the box's height follows from its aspect ratio in CSS); nothing is read back from the canvas */
    new ResizeObserver(() => { const w = Math.floor(live.clientWidth); if (!w || w === W) return; W = w; H = Math.round(w * .78); R3.setSize(W, H, false); render(); }).observe(live);
    const ptr = new Map(); let pinch = 0;
    cv.addEventListener('pointerdown', (e) => { ptr.set(e.pointerId, [e.clientX, e.clientY]); cv.setPointerCapture(e.pointerId); if (ptr.size === 2) { const [p, q] = [...ptr.values()]; pinch = Math.hypot(p[0] - q[0], p[1] - q[1]); } });
    cv.addEventListener('pointermove', (e) => { const p = ptr.get(e.pointerId); if (!p) return; const dx = e.clientX - p[0], dy = e.clientY - p[1]; ptr.set(e.pointerId, [e.clientX, e.clientY]);
      if (ptr.size === 2) { const [a, b] = [...ptr.values()], d = Math.hypot(a[0] - b[0], a[1] - b[1]); if (pinch) cam.dist = Math.min(22, Math.max(4, cam.dist * pinch / d)); pinch = d; }
      else { cam.az += dx * .45; if (e.pointerType === 'mouse') cam.el = Math.min(85, Math.max(3, cam.el + dy * .3)); } render(); });
    const up = (e) => { ptr.delete(e.pointerId); pinch = 0; }; cv.addEventListener('pointerup', up); cv.addEventListener('pointercancel', up);
    /* scrolling over the view scrolls the page; a trackpad's pinch arrives as a wheel with ctrl held, and only that zooms */
    cv.addEventListener('wheel', (e) => { if (!e.ctrlKey) return; e.preventDefault(); cam.dist = Math.min(22, Math.max(4, cam.dist * Math.exp(e.deltaY * .01))); render(); }, { passive: false });
  } else live.append(h('p', { style: 'padding:16px;color:#fff', text: 'The 3D view needs three.js, which did not load.' }));
  press('pillars'); sync(); rebuild();

  /* measured like the sheets */
  if (HAVE3) { const S = [], C = []; for (let i = 1; i <= 200; i++) { const m = seen(formation(drawParams(i))); S.push(...m.stones); C.push(m); }
    const two = (id, ref, gen, x, xl) => strip(id, [{ name: 'references', kind: 'pillar', vals: ref }, { name: 'generator', kind: 'cap', vals: gen }], { x, xl, left: 122, rowH: 36 });
    two('g-v1', STONES.map((r) => r.wb), S.map((r) => r.wb), [0, 1.6]); two('g-v2', STONES.map((r) => r.hb), S.map((r) => r.hb), [0, 2]); two('g-v3', STONES.map((r) => r.slender), S.map((r) => r.hb / r.wb), [0, 5]);
    const RC = ALL.map((id) => CELLS[id]); two('g-v4', RC.map((c) => c.H), C.map((c) => c.H), [1, 2.6]); two('g-v5', RC.map((c) => c.W), C.map((c) => c.W), [1, 2.4]); two('g-v6', RC.map((c) => c.n), C.map((c) => c.n), [3, 14]); }
}
