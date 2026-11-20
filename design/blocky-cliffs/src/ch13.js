/* ---------- chapter 13 ---------- */
{
  const live = $('#g-live'), FIG = [1.25, .45];
  const CLS = [['pillars', 'Pillars'], ['wall', 'Plateau wall'], ['blocks', 'Blocks in tiers'], ['leaners', 'Leaning set'], ['ramp', 'Dipping beds'], ['courses', 'Courses'], ['gates', 'Gates']];
  const defs = [['height', 'Height', 1, 2.1, .01], ['length', 'Length', .8, 1.7, .01], ['depth', 'Depth', .5, 1.2, .01], ['nA', 'Stones along', 1, 4, 1], ['rows', 'Rows', 1, 5, 1], ['fall', 'Fall to the far end', 0, .8, .01], ['peak', 'Peak along the cliff', 0, .5, .01], ['front', 'Buttress height, share of the wall', .15, 1, .01], ['sizeVar', 'Widths unequal', 0, .7, .01], ['plateau', 'Plateau', 0, 1, 1], ['cleft', 'Cleft', 0, 1, 1], ['caps', 'Stones piled on', 0, 3, 1], ['applied', 'Applied pillar', 0, 1, 1], ['tuck', 'Tucked stone', 0, 1, 1], ['oversail', 'Oversail at the far end', 0, 1, 1], ['tiers', 'Tiers', 0, 4, 1], ['lean', 'Lean of the slabs, degrees', 0, 32, 1], ['dip', 'Beds dip, degrees', 0, 35, 1], ['bevel', 'Bevel', 0, .4, .01]];
  let P = { preset: 'pillars', cls: 'pillars', seed: 7 }, cam = { az: 45, el: 20, dist: 9.6 }, F = null, R3 = null, scene = null, W = 0, H = 0;
  const ctl = $('#g-ctl'), inputs = {};
  const render = () => { if (!R3 || !scene || !W) return; R3.render(scene, cameraFor('ref', W, H, { az: cam.az, el: cam.el, dist: cam.dist, fov: 19, centre: [PLATE / 2, PLATE / 2, .78] })); };
  const rebuild = () => {
    F = formation(P); const ms = formMeshes(F);
    if (HAVE3) { if (scene) scene.traverse((m) => { if (m.geometry) m.geometry.dispose(); if (m.material) m.material.dispose(); }); scene = buildScene(ms, { figure: FIG }); render(); }
    const q = F.q, st = $('#g-steps'), o = { w: 220, h: 200 }, flat = { bevel: 0, bevelV: 0, bevelB: 0, taperA: 1, taperD: 1, dipA: 0, dipD: 0, botA: 0, cut: 0, jit: 0, yaw: 0, leanA: 0, leanD: 0 }, flat2 = { bevel: 0, bevelV: 0, bevelB: 0, dipD: 0, cut: 0, jit: 0, yaw: 0 };
    const rowBoxes = F.rows.map((r) => stoneMesh(Object.assign({ w: r.len, dp: r.dp - .01, h: Math.max(...r.cols.map((c) => c.h)), a: r.len / 2, d: r.d0 + r.dp / 2, z: 0 }, flat)));
    const colBoxes = F.rows.flatMap((r) => r.cols.map((c) => stoneMesh(Object.assign({ w: c.w - .01, dp: r.dp * (1 - (c.recess || 0)) - .01, h: c.h, a: c.a0 + c.w / 2, d: r.d0 + r.dp * (c.recess || 0) + r.dp * (1 - (c.recess || 0)) / 2, z: 0 }, flat))));
    const cutBoxes = F.stones.map((s0) => stoneMesh(Object.assign({}, s0, flat2)));
    const a = still([], o); { const d0 = PLATE - q.depth, e = (aa, t) => q.height * (1 - q.fall * Math.max(0, aa - q.peak * q.length) / (q.length * (1 - q.peak))); const fp = [[0, d0, 0], [0, PLATE, 0], [q.length, PLATE, 0], [q.length, d0, 0]].map((p) => a.proj(...p)), en = [[0, d0, e(0, 0)], [0, PLATE, e(0, 1)], [q.length, PLATE, e(q.length, 1)], [q.length, d0, e(q.length, 0)]].map((p) => a.proj(...p));
      a.svg.append(s('polygon', { points: pts(fp), fill: 'var(--a-out)', 'fill-opacity': .25, stroke: 'var(--a-out)', 'stroke-width': 2 }), s('polygon', { points: pts(en), fill: 'var(--a-top)', 'fill-opacity': .2, stroke: 'var(--a-top)', 'stroke-width': 2, 'stroke-dasharray': '5 4' }), ...fp.map((p, i) => s('line', { x1: p[0], y1: p[1], x2: en[i][0], y2: en[i][1], stroke: 'var(--a-top)', 'stroke-width': 1 }))); }
    st.replaceChildren(a, still(rowBoxes, o), still(colBoxes, o), still(cutBoxes, o), still(ms, o));
    [...st.children].forEach((c, i) => c.append(h('span', { class: 'tag', text: ['1 footprint, envelope', '2 rows', '3 ground stones', '4 the pile, plain', '5 shaped'][i] })));
    const m = HAVE3 ? seen(F) : null; if (m) $('#g-read').innerHTML = `<span class="ev p">Proposed</span> Seed ${P.seed} · ${m.n} stones · as seen ${fmt(m.W)} fh wide, ${fmt(m.H)} fh high (references: 1.3–2.1 and 1.8–2.3).`;
    const bs = $('#g-beside'), refs = P.preset === 'wall' ? ['H', 'T10', 'T12', 'T20'] : P.preset === 'ramp' ? ['N', 'G', 'S14', 'S7'] : P.preset === 'leaners' ? ['P', 'L', 'T9', 'T19'] : ALL.filter((id) => CL[id].class === F.q.cls).slice(0, 4);
    bs.replaceChildren(...[1, 2, 3, 4].map((k) => still(formMeshes(formation(Object.assign({}, P, { seed: P.seed * 13 + k }))), { w: 300, h: 260, figure: FIG })), ...refs.map((id) => cellView(id, { stones: false })));
  };
  const sync = () => { const q = formation(P).q; for (const [k, , , , stp] of defs) { inputs[k][0].value = q[k]; inputs[k][1].textContent = fmt(q[k], stp < 1 ? 2 : 0); } };
  for (const [k, name, a, b, stp] of defs) { const out = h('output', { for: 'g-' + k }), inp = h('input', { type: 'range', id: 'g-' + k, min: a, max: b, step: stp, oninput: () => { P[k] = +inp.value; out.textContent = fmt(P[k], stp < 1 ? 2 : 0); rebuild(); } }); inputs[k] = [inp, out]; ctl.append(h('label', { for: 'g-' + k, text: name }), inp, out); }
  const pills = $('#g-cls'), press = (k) => { for (const b of pills.children) b.setAttribute('aria-pressed', b.dataset.k === k); };
  for (const [k, name] of CLS) pills.append(h('button', { type: 'button', 'data-k': k, text: name, onclick: () => { P = { preset: k, cls: k === 'ramp' ? 'leaners' : k === 'wall' ? 'pillars' : k, seed: P.seed }; press(k); sync(); rebuild(); } }));
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

  /* measured like the sheets: sizes, then the relations between touching stones */
  if (HAVE3) { const S = [], C = [], GP = []; for (let i = 1; i <= 200; i++) { const m = seen(formation(drawParams(i))); S.push(...m.stones); C.push(m); GP.push(...m.pairs); }
    const two = (id, ref, gen, x, xl) => strip(id, [{ name: 'references', kind: 'pillar', vals: ref }, { name: 'generator', kind: 'cap', vals: gen }], { x, xl, left: 122, rowH: 36 });
    two('g-v1', STONES.map((r) => r.wb), S.map((r) => r.wb), [0, 1.6]); two('g-v2', STONES.map((r) => r.hb), S.map((r) => r.hb), [0, 2]); two('g-v3', STONES.map((r) => r.slender), S.map((r) => r.hb / r.wb), [0, 5]);
    const RC = ALL.map((id) => CELLS[id]); two('g-v4', RC.map((c) => c.H), C.map((c) => c.H), [1, 2.6]); two('g-v5', RC.map((c) => c.W), C.map((c) => c.W), [1, 2.4]); two('g-v6', RC.map((c) => c.n), C.map((c) => c.n), [3, 14]);
    const side = PAIRS.filter((p) => !p.stacked), pc = (a, n) => Math.round(100 * a / n) + '%', rf = side.filter((p) => p.lowfront_foot != null), gf = GP.filter((p) => p.lowfront != null), md = (v) => fmt(quant(v, .5));
    const rows = [['Touching pairs whose tops are within 0.05 fh as seen', pc(side.filter((p) => p.dtop < .05).length, side.length), pc(GP.filter((p) => p.dtop < .05).length, GP.length)],
      ['… within 0.12 fh', pc(side.filter((p) => p.dtop < .12).length, side.length), pc(GP.filter((p) => p.dtop < .12).length, GP.length)],
      ['Median step between touching tops, fh', md(side.map((p) => p.dtop)), md(GP.map((p) => p.dtop))],
      ['The lower stone is the one whose foot is nearer', pc(rf.filter((p) => p.lowfront_foot).length, rf.length), pc(gf.filter((p) => p.lowfront).length, gf.length)],
      ['Nearest stone tops out under half the height', pc(RC.filter((c) => c.footshare < .5).length, 52), pc(C.filter((c) => c.footshare < .5).length, 200)],
      ['Highest point left of the middle', pc(RC.filter((c) => c.peak < .5).length, 52), pc(C.filter((c) => c.peak < .5).length, 200)],
      ['Highest point, share of the way across (median)', md(RC.map((c) => c.peak)), md(C.map((c) => c.peak))],
      ['Far end ÷ highest point, as seen (median)', md(RC.map((c) => c.farend)), md(C.map((c) => c.farend))],
      ['Widest ÷ narrowest stone in a formation (median)', md(RC.map((c) => c.wratio)), md(C.map((c) => c.wratio))],
      ['Formations with a cap or oversailing block', pc(RC.filter((c) => c.kinds.cap).length, 52), pc(C.filter((c) => c.jut).length, 200)],
      ['Most stones in a formation', Math.max(...RC.map((c) => c.n)), Math.max(...C.map((c) => c.n))],
      ['Stones resting on another stone', pc(RC.reduce((a, c) => a + c.resting, 0), STONES.length), pc(C.reduce((a, c) => a + c.rest, 0), C.reduce((a, c) => a + c.real, 0))],
      ['Stones bearing on two or more', pc(RC.reduce((a, c) => a + c.ontwo, 0), STONES.length), pc(C.reduce((a, c) => a + c.two, 0), C.reduce((a, c) => a + c.real, 0))],
      ...[0, 1, 2, 3, 4].map((k) => ['Formations stacked ' + (k ? k + ' deep at most' : 'not at all'), pc(RC.filter((c) => c.deep === k).length, 52), pc(C.filter((c) => c.deep === k).length, 200)])];
    $('#g-rel').append(h('tr', {}, ['Relation, measured the same way on both', 'References', 'Generator'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))), ...rows.map((r) => h('tr', {}, r.map((t, i) => h('td', { class: i ? 'num' : null, text: t }))))); }
}
