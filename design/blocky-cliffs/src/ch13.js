/* ---------- chapter 13 ---------- */
{
  const live = $('#g-live'), FIG = [1.25, .45];
  const CLS = FAMILIES.map((k) => [k, k[0].toUpperCase() + k.slice(1)]), OF = { pillars: 'columns', gates: 'columns', blocks: 'tiers', courses: 'tiers', leaners: 'slabs' };
  const defs = [['height', 'Height', 1, 2.1, .01], ['length', 'Length', .8, 1.7, .01], ['depth', 'Depth', .5, 1.2, .01], ['fall', 'Fall to the far end', 0, .8, .01], ['peak', 'Peak along the cliff', 0, .5, .01], ['cols', 'Stones on the ground', 1, 4, 1], ['spread', 'Widths unequal', 0, .7, .01], ['base', 'Ground height, share of the envelope', .25, 1, .01], ['step', 'Least step between tops', 0, .25, .01], ['pile', 'Stones laid on', 0, 5, 1], ['bond', 'Chance across a joint', 0, 1, .01], ['stack', 'Stack: lowest top to highest', 0, 1, .01], ['setback', 'Setback of a laid stone', 0, .3, .01], ['lean', 'Lean of the slabs, degrees', 0, 32, 1], ['dip', 'Beds dip, degrees', 0, 35, 1], ['opening', 'Opening between piers', 0, .45, .01], ['front', 'Stones in front', 0, 3, 1], ['bevel', 'Bevel', 0, .4, .01]];
  let P = drawParams(7, 'columns'), cam = { az: 45, el: 20, dist: 9.6 }, F = null, R3 = null, scene = null, W = 0, H = 0;
  const ctl = $('#g-ctl'), inputs = {};
  const render = () => { if (!R3 || !scene || !W) return; R3.render(scene, cameraFor('ref', W, H, { az: cam.az, el: cam.el, dist: cam.dist, fov: 19, centre: [PLATE / 2, PLATE / 2, .78] })); };
  const rebuild = () => {
    F = formation(P); const ms = formMeshes(F);
    if (HAVE3) { if (scene) scene.traverse((m) => { if (m.userData.own) m.geometry.dispose(); }); scene = buildScene(ms, { figure: FIG }); render(); }
    const q = F.q, st = $('#g-steps'), o = { w: 220, h: 200 }, flat = { cushion: 0, bevel: 0 };
    const M = Object.fromEntries(F.marks.map((m) => [m.name, m.n])), plain = (n) => F.specs.slice(0, n).map((sp) => stoneMesh(Object.assign({ w: sp.w - .01, dp: sp.dp - .01, h: sp.h, a: sp.a0 + sp.w / 2, d: sp.d0 + sp.dp / 2, z: sp.z, level: sp.level }, flat)));
    const a = still([], o); { const d0 = PLATE - q.depth, e = (aa, t) => q.height * (1 - q.fall * Math.max(0, aa - q.peak * q.length) / (q.length * (1 - q.peak))); const fp = [[0, d0, 0], [0, PLATE, 0], [q.length, PLATE, 0], [q.length, d0, 0]].map((p) => a.proj(...p)), en = [[0, d0, e(0, 0)], [0, PLATE, e(0, 1)], [q.length, PLATE, e(q.length, 1)], [q.length, d0, e(q.length, 0)]].map((p) => a.proj(...p));
      a.svg.append(s('polygon', { points: pts(fp), fill: 'var(--a-out)', 'fill-opacity': .25, stroke: 'var(--a-out)', 'stroke-width': 2 }), s('polygon', { points: pts(en), fill: 'var(--a-top)', 'fill-opacity': .2, stroke: 'var(--a-top)', 'stroke-width': 2, 'stroke-dasharray': '5 4' }), ...fp.map((p, i) => s('line', { x1: p[0], y1: p[1], x2: en[i][0], y2: en[i][1], stroke: 'var(--a-top)', 'stroke-width': 1 }))); }
    st.replaceChildren(a, still(plain(M.ground), o), still(plain(M.front), o), still(plain(M.pile), o), still(ms, o));
    [...st.children].forEach((c, i) => c.append(h('span', { class: 'tag', text: ['1 footprint, envelope', '2 ground', '3 front', '4 the pile, plain', '5 shaped'][i] })));
    const m = HAVE3 ? seen(F) : null; if (m) $('#g-read').innerHTML = `<span class="ev p">Proposed</span> Seed ${P.seed} · ${m.n} stones · as seen ${fmt(m.W)} fh wide, ${fmt(m.H)} fh high (references: 1.3–2.1 and 1.8–2.3).`;
    const bs = $('#g-beside'), refs = ALL.filter((id) => OF[CL[id].class] === P.family).filter((id, i) => i % 3 === 0).slice(0, 4);
    bs.replaceChildren(...[1, 2, 3, 4].map((k) => still(formMeshes(formation(Object.assign({}, P, { seed: P.seed * 13 + k }))), { w: 300, h: 260, figure: FIG })), ...refs.map((id) => cellView(id, { stones: false })));
  };
  const sync = () => { const q = formation(P).q; for (const [k, , , , stp] of defs) { inputs[k][0].value = q[k]; inputs[k][1].textContent = fmt(q[k], stp < 1 ? 2 : 0); } };
  for (const [k, name, a, b, stp] of defs) { const out = h('output', { for: 'g-' + k }), inp = h('input', { type: 'range', id: 'g-' + k, min: a, max: b, step: stp, oninput: () => { P[k] = +inp.value; out.textContent = fmt(P[k], stp < 1 ? 2 : 0); rebuild(); } }); inputs[k] = [inp, out]; ctl.append(h('label', { for: 'g-' + k, text: name }), inp, out); }
  const pills = $('#g-cls'), press = (k) => { for (const b of pills.children) b.setAttribute('aria-pressed', b.dataset.k === k); };
  for (const [k, name] of CLS) pills.append(h('button', { type: 'button', 'data-k': k, text: name, onclick: () => { P = drawParams(P.seed, k); press(k); sync(); rebuild(); } }));
  $('#g-seed').addEventListener('click', () => { P.seed = 1 + Math.floor(Math.random() * 9999); rebuild(); });
  $('#g-all').addEventListener('click', () => { P = drawParams(1 + Math.floor(Math.random() * 99999)); press(P.family); sync(); rebuild(); });
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
  press('columns'); sync(); rebuild();

  /* measured like the sheets: sizes, then the relations between touching stones */
  if (HAVE3) { const S = [], C = [], GP = [], S2 = [], C2 = [], GP2 = []; for (let i = 1; i <= 200; i++) { const f0 = formation(drawParams(i)), m = seen(f0), m2 = seen(f0, 25); S.push(...m.stones); C.push(m); GP.push(...m.pairs); S2.push(...m2.stones); C2.push(m2); GP2.push(...m2.pairs); }
    const two = (id, ref, gen, x, xl) => strip(id, [{ name: 'references', kind: 'pillar', vals: ref }, { name: 'generator', kind: 'cap', vals: gen }], { x, xl, left: 122, rowH: 36 });
    two('g-v1', STONES.map((r) => r.wb), S.map((r) => r.wb), [0, 1.6]); two('g-v2', STONES.map((r) => r.hb), S.map((r) => r.hb), [0, 2]); two('g-v3', STONES.map((r) => r.slender), S.map((r) => r.hb / r.wb), [0, 5]);
    const RC = ALL.map((id) => CELLS[id]); two('g-v4', RC.map((c) => c.H), C.map((c) => c.H), [1, 2.6]); two('g-v5', RC.map((c) => c.W), C.map((c) => c.W), [1, 2.4]); two('g-v6', RC.map((c) => c.n), C.map((c) => c.n), [3, 14]);
    const side = PAIRS.filter((p) => !p.stacked), pc = (a, n) => Math.round(100 * a / n) + '%', rf = side.filter((p) => p.lowfront_foot != null), md = (v) => fmt(quant(v, .5)), tri = (v) => [.1, .5, .9].map((p) => fmt(quant(v, p))).join(' / ');
    const refG = STONES.filter((r) => r.ground), refR = STONES.filter((r) => !r.ground), g80 = RC.map((c) => refG.filter((r) => r.cell === c.id && r.topy >= .8 * c.H).length);
    const gen = (St, Cs, Ps) => { const gf = Ps.filter((p) => p.lowfront != null), G = St.filter((x) => x.ground), Rs = St.filter((x) => !x.ground), real = Cs.reduce((a, c) => a + c.real, 0);
      return [pc(Ps.filter((p) => p.dtop < .05).length, Ps.length), pc(Ps.filter((p) => p.dtop < .12).length, Ps.length), md(Ps.map((p) => p.dtop)), pc(gf.filter((p) => p.lowfront).length, gf.length), pc(Cs.filter((c) => c.footshare < .5).length, 200), pc(Cs.filter((c) => c.peak < .5).length, 200), md(Cs.map((c) => c.peak)), md(Cs.map((c) => c.farend)), md(Cs.map((c) => c.wratio)), pc(Cs.filter((c) => c.jut).length, 200), Math.max(...Cs.map((c) => c.n)),
        pc(Cs.reduce((a, c) => a + c.rest, 0), real), pc(Cs.reduce((a, c) => a + c.two, 0), real), [0, 1, 2, 3, 4].map((k) => Math.round(100 * Cs.filter((c) => c.deep === k).length / 200)).join(' / ') + ' %', pc(Cs.filter((c) => c.topRests).length, 200),
        tri(G.map((x) => x.hb)), tri(G.map((x) => x.hb / x.wb)), tri(Rs.map((x) => x.hb)), tri(Cs.map((c) => c.g80)), tri(St.map((x) => x.wb)), tri(St.filter((x) => x.wv > .05).map((x) => x.wv)), tri(G.filter((x) => x.wv > .05).map((x) => x.hb / x.wv)), md(Cs.map((c) => c.wratioV)), tri(St.map((x) => x.hb)), tri(Cs.map((c) => c.H)), pc(Cs.filter((c) => c.H > 2.29).length, 200), tri(Cs.map((c) => c.W))]; };
    const names = ['Touching pairs whose tops are within 0.05 fh as seen', '… within 0.12 fh', 'Median step between touching tops, fh', 'The lower stone is the one whose foot is nearer', 'Nearest stone tops out under half the height', 'Highest point left of the middle', 'Highest point, share of the way across (median)', 'Far end ÷ highest point, as seen (median)', 'Widest ÷ narrowest stone in a formation (median)', 'Formations with a cap or oversailing block', 'Most stones in a formation',
      'Stones resting on another stone', 'Stones bearing on two or more', 'Formations stacked 0 / 1 / 2 / 3 / 4 deep', 'The highest stone is a resting one',
      'Ground-standing stone: face height, fh', 'Ground-standing stone: face height ÷ width', 'Resting stone: face height, fh', 'Ground stones reaching 0.8 of the height, per formation', 'Any stone: width of its whole box, fh', 'Any stone: width of what shows, fh', 'Ground-standing stone: face height ÷ width of what shows', 'Widest ÷ narrowest of what shows (median)', 'Any stone: face height, fh', 'Formation height, fh', 'Formations over 2.29 fh', 'Formation width, fh'];
    const topRef = RC.filter((c) => { const st = STONES.filter((r) => r.cell === c.id); return !st.reduce((u, v) => (v.topy > u.topy ? v : u)).ground; }).length;
    const ref = [pc(side.filter((p) => p.dtop < .05).length, side.length), pc(side.filter((p) => p.dtop < .12).length, side.length), md(side.map((p) => p.dtop)), pc(rf.filter((p) => p.lowfront_foot).length, rf.length), pc(RC.filter((c) => c.footshare < .5).length, 52), pc(RC.filter((c) => c.peak < .5).length, 52), md(RC.map((c) => c.peak)), md(RC.map((c) => c.farend)), md(RC.map((c) => c.wratio)), pc(RC.filter((c) => c.kinds.cap).length, 52), Math.max(...RC.map((c) => c.n)),
      pc(RC.reduce((a, c) => a + c.resting, 0), STONES.length), pc(RC.reduce((a, c) => a + c.ontwo, 0), STONES.length), [0, 1, 2, 3, 4].map((k) => Math.round(100 * RC.filter((c) => c.deep === k).length / 52)).join(' / ') + ' %', pc(topRef, 52),
      tri(refG.map((r) => r.hb)), tri(refG.map((r) => r.slender)), tri(refR.map((r) => r.hb)), tri(g80), '—', tri(STONES.map((r) => r.wb)), tri(refG.map((r) => r.slender)), md(RC.map((c) => c.wratio)), tri(STONES.map((r) => r.hb)), tri(RC.map((c) => c.H)), pc(RC.filter((c) => c.H > 2.29).length, 52), tri(RC.map((c) => c.W))];
    const g45 = gen(S, C, GP), g25 = gen(S2, C2, GP2), rows = names.map((nm, i) => [nm, ref[i], g45[i], g25[i]]);
    $('#g-rel').append(h('tr', {}, ['Measured the same way on both', 'References', 'Generator, from 45°', 'Generator, from 25°'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))), ...rows.map((r) => h('tr', {}, r.map((t, i) => h('td', { class: i ? 'num' : null, text: t }))))); }
}
