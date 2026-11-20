/* The generator page: a live view of one formation and the algorithm step by step, each with the picture it leaves on this seed and what it costs. */
const live = $('#live'), seedIn = $('#seed'), clsIn = $('#cls'), sheet = $('#sheet'), read = $('#read');
if (!HAVE3) { live.append(h('p', { style: 'padding:16px;color:#fff', text: 'The 3D view needs three.js, which did not load.' })); return; }
let seed = 42;   /* a seed that shows a stone built taller and a fused joint */
try { const sv = +localStorage.getItem('bcg-seed'); if (sv > 0) seed = sv; } catch (e) { /* no storage in this frame: start from a random seed */ }
let joined = true;   /* merged: the stones on a fused joint grown toward each other; separate: the stones as built */
try { if (localStorage.getItem('bcg-joined') === '0') joined = false; } catch (e) { /* no storage in this frame: start joined */ }
const make = (sd) => { const p = drawParams(sd, clsIn.value), f = formation(p); return { p, f, ms: formMeshes(f, !joined) }; };
/* the middle of a formation, so every view turns about the stones and not about an absent plate */
const middle = (ms) => { let lo = [1e9, 1e9, 1e9], hi = [-1e9, -1e9, -1e9]; for (const m of ms) for (const q of m.P) for (let i = 0; i < 3; i++) { lo[i] = Math.min(lo[i], q[i]); hi[i] = Math.max(hi[i], q[i]); } return [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2]; };
const R3 = new THREE.WebGLRenderer({ antialias: true }); R3.setClearColor(STAGE, 1); R3.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2)); const cv = R3.domElement; live.append(cv);
const cam = new THREE.PerspectiveCamera(17, 4 / 3, .1, 100);
/* scrolling over the view scrolls the page: this listener is added before the controls', and stops theirs unless it is a pinch (ctrl + wheel) */
cv.addEventListener('wheel', (e) => { if (!e.ctrlKey) e.stopImmediatePropagation(); });
const ctl = new THREE.OrbitControls(cam, cv); ctl.enablePan = false; ctl.minDistance = 4; ctl.maxDistance = 22; ctl.maxPolarAngle = Math.PI * .56;
let scene = null, W = 0;
const render = () => { if (scene && W) R3.render(scene, cam); }; ctl.addEventListener('change', render);
/* the view's size is its box's width; its height is three quarters of that, as the CSS says; nothing is read back from the canvas */
new ResizeObserver(() => { const w = Math.floor(live.clientWidth); if (!w || w === W) return; W = w; R3.setSize(W, Math.round(W * .75), false); render(); }).observe(live);
const refView = (c) => { const R = 8.6, el = 20 * Math.PI / 180, az = 45 * Math.PI / 180; ctl.target.set(c[0], c[2], c[1]); cam.position.set(c[0] - R * Math.cos(el) * Math.sin(az), c[2] + R * Math.sin(el), c[1] - R * Math.cos(el) * Math.cos(az)); ctl.update(); };
const BLUE = 0x7f9ee6;
/* a spec as a plain rounded box, for the pictures of the steps before the shapes */
const plain = (sp) => stoneMesh({ a: sp.a0 + sp.w / 2, d: sp.d0 + sp.dp / 2, w: sp.w, dp: sp.dp, h: sp.h, z: sp.z, level: sp.level, dipA: sp.dipA || 0, botA: sp.botA || 0 });
const step = (id, ms, mark, o = {}) => { const box = $('#' + id); box.replaceChildren(still(ms, Object.assign({ w: 300, h: 250, px: 1.5, figure: [.42, .5], centre: [PLATE / 2, PLATE / 2, .75], dist: 10.5, color: mark ? (st, i) => (mark.has(i) ? BLUE : GREY) : undefined }, o)), h('figcaption', { text: o.cap || '' })); };
const ms1 = (x) => (x < 10 ? x.toFixed(2) : x.toFixed(1));
const say = (id, v) => { const e = $('#' + id); if (e) e.textContent = v; };
/* the stage times: every stage run `reps` times on this seed */
const timeStages = (sd, reps) => {
  const T = { numbers: 0, ground: 0, front: 0, pile: 0, shape: 0, bear: 0, fuse: 0, mesh: 0, draw: 0 }, t = () => performance.now();
  for (let i = 0; i < reps; i++) { let a = t(); const p = drawParams(sd, clsIn.value); T.numbers += t() - a; const f = formation(p), m = f.marks; for (let k = 1; k < m.length; k++) T[m[k].name] += m[k].t - m[k - 1].t;
    a = t(); const S = grownStones(f); T.fuse += t() - a; a = t(); const ms = S.map(stoneMesh); T.mesh += t() - a; a = t(); still(ms, { w: 300, h: 250, px: 1.5, figure: [.42, .5], centre: [PLATE / 2, PLATE / 2, .75], dist: 10.5 }); T.draw += t() - a; }
  for (const k in T) T[k] /= reps; return T;
};
const times = () => { const T = timeStages(seed, 40), tot = Object.values(T).reduce((a, b) => a + b, 0), tb = $('#times tbody'), rows = [['1 The numbers', T.numbers], ['2 Ground', T.ground], ['3 The front cluster', T.front], ['4 The pile', T.pile], ['5 Shapes', T.shape], ['6 Bearing', T.bear], ['7 Fusing', T.fuse], ['8 Meshes', T.mesh], ['9 Drawing', T.draw]];
  tb.replaceChildren(...rows.map(([n, v]) => h('tr', {}, h('td', { text: n }), h('td', { class: 'num', text: ms1(v) }), h('td', { class: 'num', text: Math.round(100 * v / tot) + '%' }))), h('tr', {}, h('th', { text: 'Whole' }), h('th', { class: 'num', text: ms1(tot) }), h('th', { class: 'num', text: '' })));
  ['numbers', 'ground', 'front', 'pile', 'shape', 'bear', 'fuse', 'mesh', 'draw'].forEach((k, i) => say('t' + (i + 1), ms1(T[k]) + ' ms'));
  const a = performance.now(); for (let i = 0; i < 24; i++) { const f = formation(drawParams(i + 1)); still(formMeshes(f), { w: 160, h: 160, px: 1, figure: [.42, .5], centre: [PLATE / 2, PLATE / 2, .75], dist: 10.5 }); } say('t24', Math.round(performance.now() - a) + ' ms'); };
const pictures = (c) => {
  const { p, f } = c, q = f.q, M = Object.fromEntries(f.marks.map((m) => [m.name, m.n])), specs = (n) => f.specs.slice(0, n).map(plain);
  say('f1-kind', `${p.family}, ${lookOf(q)}`); say('f1-hl', `${fmt(q.height, 2)}, ${fmt(q.length, 2)} fh`); say('f1-fp', `${fmt(q.fall, 2)}, ${fmt(q.peak, 2)}`); say('f1-nr', `${q.cols} on the ground, ${q.front} in front, up to ${q.pile} laid on`);
  { const svg = $('#p1'), L = q.length, env = (a) => { const pk = q.peak * L, far = a > pk ? (a - pk) / Math.max(L - pk, .01) : (pk - a) / Math.max(L, .01) * .5; return q.height * (1 - q.fall * far); }, X = (a) => 20 + a / L * 280, Y = (z) => 160 - z / 2.1 * 140, pts = [];
    for (let i = 0; i <= 40; i++) pts.push(X(L * i / 40) + ',' + Y(env(L * i / 40)));
    svg.replaceChildren(s('line', { x1: 20, x2: 300, y1: 160, y2: 160, stroke: 'currentColor', 'stroke-opacity': .5 }), s('polyline', { points: pts.join(' '), fill: 'none', stroke: '#7f9ee6', 'stroke-width': 2.5 }),
      ...f.specs.filter((sp) => sp.level === 0 && !sp.front).map((sp) => s('rect', { x: X(sp.a0), y: Y(sp.z + sp.h), width: Math.max(1, X(sp.a0 + sp.w) - X(sp.a0) - 1.5), height: Math.max(1, Y(sp.z) - Y(sp.z + sp.h)), fill: '#bcbfc5', 'fill-opacity': .85 })),
      s('text', { x: 22, y: 14, 'font-size': 11, fill: 'currentColor', text: 'height, fh' }), s('text', { x: 300, y: 176, 'font-size': 11, 'text-anchor': 'end', fill: 'currentColor', text: 'along the cliff, fh' })); svg.style.color = '#1c2129'; }
  say('f2-n', String(M.ground)); say('f3-n', String(M.front - M.ground)); say('f4-n', String(M.pile - M.front)); say('f5-n', String(M.shape));
  step('v2', specs(M.ground), null, { cap: 'The ground stones, as plain boxes.' }); step('v3', specs(M.front), null, { cap: 'With the front cluster.' }); step('v4', specs(M.pile), null, { cap: 'With the pile laid on the tops.' });
  const built = f.stones, taller = new Set(); built.forEach((s, i) => { if (s.h - f.specs[i].h > 1e-6) taller.add(i); });
  step('v5', built.map((s, i) => stoneMesh(Object.assign({}, s, { z: f.specs[i].z, h: f.specs[i].h }))), null, { cap: 'Each spec shaped: bevels, batter, dips, cuts; before bearing.' });
  say('f6-n', taller.size ? `${taller.size} of ${built.filter((s) => s.z > .02).length} raised stones, by ${[...taller].map((i) => fmt(built[i].h - f.specs[i].h, 2)).join(', ')} fh` : 'none on this seed');
  step('v6', built.map(stoneMesh), taller, { cap: 'The stones as built, with those made taller marked.' });
  const grown = grownStones(f), moved = new Set(); grown.forEach((s, i) => { if (s.w !== built[i].w || s.h !== built[i].h) moved.add(i); }); say('f7-c', String(contactsOf(f).length)); say('f7-n', `${fusedContacts(f).length}, ${moved.size} stones grown`);
  step('v7', grown.map(stoneMesh), moved, { cap: 'The stones after the fused joints are grown. The marked stones grew.' });
  const one = built[0], mo = stoneMesh(one); say('f8-vt', `${f.stones.reduce((a, s) => a + stoneMesh(s).P.length, 0)} and ${f.stones.reduce((a, s) => a + stoneMesh(s).idx.length / 3, 0)} in all; ${mo.P.length} and ${mo.idx.length / 3} in one stone`);
  step('v8', [mo], null, { plate: false, w: 300, h: 250, centre: [one.a, one.d, one.z + one.h / 2], dist: 9, fov: 11, figure: null, cap: 'One stone: a lattice pushed out to a rounded box, then shaped.' });
};
let cur = null;
const show = (sd, keepView) => { seed = Math.max(1, Math.min(999999, Math.round(sd) || 1)); seedIn.value = seed; try { localStorage.setItem('bcg-seed', seed); } catch (e) { /* not kept: nothing depends on it */ }
  cur = make(seed); if (scene) scene.traverse((m) => { if (m.userData.own) m.geometry.dispose(); });
  scene = buildScene(cur.ms, { plate: false }); const c = middle(cur.ms); if (!keepView) refView(c); else { ctl.target.set(c[0], c[2], c[1]); ctl.update(); } render();
  const q = cur.f.q, extra = [q.opening ? 'an opening' : '', q.lean ? `leaning ${Math.round(q.lean)}°` : '', q.dip ? `dipping ${Math.round(q.dip)}°` : ''].filter(Boolean);
  read.innerHTML = `Seed <b>${seed}</b>: ${cur.p.family}, read as ${lookOf(q)}, <b>${cur.f.stones.length}</b> stones, ${fmt(q.height, 2)} high, ${fmt(q.length, 2)} long${extra.length ? '; ' + extra.join(', ') : ''}.`;
  for (const d of sheet.children) d.classList.toggle('sel', +d.dataset.seed === seed);
  pictures(cur); times(); };
const fill = (from) => { sheet.replaceChildren(); for (let i = 0; i < 12; i++) { const sd = from + i, m = make(sd), c = middle(m.ms); const e = still(m.ms, { w: 260, h: 230, px: 1.5, plate: false, centre: c, dist: 8.6, fov: 17, tag: String(sd) }); e.dataset.seed = sd; e.tabIndex = 0; e.setAttribute('role', 'button'); e.setAttribute('aria-label', 'Load seed ' + sd);
    const go = () => show(sd); e.addEventListener('click', go); e.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); go(); } }); sheet.append(e); } };
let from = seed; const go = (sd, refill) => { if (refill || sd < from || sd > from + 11) { from = sd; fill(from); } show(sd); };
$('#new').addEventListener('click', () => go(1 + Math.floor(Math.random() * 99000), true));
$('#prev').addEventListener('click', () => go(Math.max(1, seed - 1)));
$('#next').addEventListener('click', () => go(seed + 1));
$('#view').addEventListener('click', () => { refView(middle(cur.ms)); render(); });
seedIn.addEventListener('change', () => go(+seedIn.value, true));
const jOn = $('#j-on'), jOff = $('#j-off');
const setJoin = (on) => { joined = on; jOn.setAttribute('aria-pressed', on); jOff.setAttribute('aria-pressed', !on); try { localStorage.setItem('bcg-joined', on ? '1' : '0'); } catch (e) { /* not kept: nothing depends on it */ } fill(from); show(seed, true); };
jOn.setAttribute('aria-pressed', joined); jOff.setAttribute('aria-pressed', !joined);
jOn.addEventListener('click', () => setJoin(true)); jOff.addEventListener('click', () => setJoin(false));
clsIn.addEventListener('change', () => { fill(from); show(seed); });
fill(from); show(seed);
