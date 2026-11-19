/* page.js: mounts the 3D views lazily (a still image each, orbit on drag), draws the section profiles from the same height functions
   as the models, and draws the static diagrams. */
(function () {
  'use strict';
  const NS = 'http://www.w3.org/2000/svg';
  const f1 = (x) => (Math.round(x * 10) / 10).toString();
  const f2 = (x) => (Math.round(x * 100) / 100).toString();

  /* ---------- 3D views, on demand, with the sliders of each model ---------- */
  const views = [];
  const SPEC = window.SPEC || {};
  const digits = (step) => step >= 1 ? 0 : step >= .1 ? 1 : step >= .01 ? 2 : 3;
  const fmt = (v, s) => s.unit === '1 :' ? '1 : ' + (+v).toFixed(digits(s.step)) : (+v).toFixed(digits(s.step)) + (s.unit ? (s.unit === '°' || s.unit === '×' ? s.unit : ' ' + s.unit) : '');
  const params = (st) => Object.assign({}, st.spec[st.vi].fix, st.vals[st.vi]);
  function h(tag, cls, txt) { const e = document.createElement(tag); if (cls) e.className = cls; if (txt != null) e.textContent = txt; return e; }
  /* the controls of one model: a sheet switch above the view (when the page's sheets give two ranges), and one slider per number under it */
  function setupCtl(el) {
    const name = el.dataset.model, spec = SPEC[name]; if (!spec) return;
    const st = { name: name, spec: spec, vi: 0, vals: spec.map(v => { const o = {}; v.sl.forEach(s => { o[s.k] = s.def; }); return o; }), dirty: false, slow: false, t: 0 };
    el._st = st;
    const wrap = h('div', 'vw'); el.replaceWith(wrap);
    if (spec.length > 1) {
      const sw = h('div', 'seg'); sw.setAttribute('role', 'group'); sw.setAttribute('aria-label', 'Sheet whose ranges and defaults the model uses'); sw.appendChild(h('span', '', 'sheet'));
      spec.forEach((v, i) => { const b = h('button', '', v.label); b.type = 'button'; b.setAttribute('aria-pressed', String(i === 0)); b.addEventListener('click', () => { st.vi = i; [].forEach.call(sw.querySelectorAll('button'), (x, j) => x.setAttribute('aria-pressed', String(j === i))); draw(); change(el); }); sw.appendChild(b); });
      wrap.appendChild(sw);
    }
    wrap.appendChild(el);
    const ctl = h('div', 'ctl'), grid = h('div', 'ctl-g'), head = h('div', 'ctl-h'); head.appendChild(h('span', '', 'parameters'));
    const rs = h('button', 'ctl-reset', 'reset'); rs.type = 'button'; rs.addEventListener('click', () => { st.vals[st.vi] = {}; st.spec[st.vi].sl.forEach(s => { st.vals[st.vi][s.k] = s.def; }); draw(); change(el); }); head.appendChild(rs);
    ctl.appendChild(head); ctl.appendChild(grid); wrap.appendChild(ctl);
    function draw() {
      grid.textContent = '';
      st.spec[st.vi].sl.forEach(s => {
        const id = 'sl-' + name + '-' + st.vi + '-' + s.k, row = h('div', 'ctl-r'), lab = h('label'), out = h('output'), inp = h('input');
        lab.htmlFor = id; lab.appendChild(h('span', 'k', s.label)); out.id = id + '-o'; out.textContent = fmt(st.vals[st.vi][s.k], s); lab.appendChild(out);
        inp.type = 'range'; inp.id = id; inp.min = s.min; inp.max = s.max; inp.step = s.step; inp.value = st.vals[st.vi][s.k]; inp.setAttribute('aria-describedby', id + '-o');
        const rg = h('div', 'rg'); rg.appendChild(h('span', '', fmt(s.min, s))); rg.appendChild(h('span', '', fmt(s.max, s)));
        inp.addEventListener('input', () => {
          st.vals[st.vi][s.k] = +inp.value; out.textContent = fmt(inp.value, s); st.dirty = true;
          if (!st.slow) { clearTimeout(st.t); st.t = setTimeout(() => change(el), 40); }
        });
        inp.addEventListener('change', () => { clearTimeout(st.t); if (st.dirty) change(el); });
        row.appendChild(lab); row.appendChild(inp); row.appendChild(rg); grid.appendChild(row);
      });
    }
    draw();
  }
  /* a slider moved, or the sheet changed: build the model again from the same parameters, and the section beside it */
  function change(el) {
    const st = el._st; st.dirty = false; if (!el._v) { showMe(el); return; }
    const t0 = performance.now(), cap = el.querySelector('figcaption');
    try { el._v.setModel(window.MODELS[st.name](params(st))); cap.textContent = captionFor(el); } catch (e) { cap.textContent = 'model failed: ' + e.message; console.error(st.name, e); }
    st.slow = performance.now() - t0 > 150;   /* a slow model rebuilds when the slider is released, not while it moves */
    profiles(el); showMe(el);
  }
  /* the printed caption describes the first sheet at its defaults; say so only while that is what is shown */
  function atDefaults(st) { return st.spec[st.vi].sl.every(s => +st.vals[st.vi][s.k] === s.def); }
  function captionFor(el) {
    const st = el._st, v = st.spec[st.vi].label;
    if (st.vi === 0 && atDefaults(st)) return el._cap;
    return (v ? v.charAt(0).toUpperCase() + v.slice(1) + ', ' : 'Model, ') + (atDefaults(st) ? 'at its defaults.' : 'your settings (reset returns the defaults).');
  }
  function profiles(el) { const st = el._st, art = el.closest('.prim'), host = art && art.querySelector('[data-prof="' + st.name + '"]'); if (host && window.PROFILES[st.name]) el._facts = profile(host, params(st)); }
  function showMe(el) {
    const st = el._st, art = el.closest('.prim'), me = art && art.querySelector('.me'); if (!me) return;
    const p = params(st), sum = st.spec[st.vi].sl.map(s => s.label + ' ' + fmt(p[s.k], s)).join(', '), v = st.spec[st.vi].label;
    me.textContent = (v ? v + ': ' : '') + (el._facts || sum);
  }
  function mount(el) {
    if (el._v) return;
    const name = el.dataset.model, mk = window.MODELS[name], cap = el.querySelector('figcaption');
    if (!mk) { cap.textContent = 'no model ' + name; return; }
    el._cap = cap.textContent; let m;
    try { m = mk(el._st ? params(el._st) : undefined); } catch (e) { cap.textContent = 'model failed: ' + e.message; console.error(name, e); return; }
    const c = mk.cam || {};
    const v = Eng.makeView(el, m, { az: el.dataset.az != null ? +el.dataset.az : (c.az != null ? c.az : -28), el: el.dataset.el != null ? +el.dataset.el : (c.el != null ? c.el : 36), dist: el.dataset.dist != null ? +el.dataset.dist : (c.dist != null ? c.dist : 1) });
    el._v = v; views.push(v); v.now();
    if (el._st) { cap.textContent = captionFor(el); profiles(el); showMe(el); }
  }
  function setupViews() {
    const els = [].slice.call(document.querySelectorAll('.v3d'));
    els.forEach(setupCtl);
    if (!window.THREE || !window.MODELS) { els.forEach(e => { e.querySelector('figcaption').textContent = '3D needs three.js (blocked or offline)'; }); return; }
    if ('IntersectionObserver' in window) {
      const io = new IntersectionObserver((es) => { es.forEach(en => { if (en.isIntersecting) { io.unobserve(en.target); setTimeout(() => mount(en.target), 0); } }); }, { rootMargin: '400px 0px' });
      els.forEach(e => io.observe(e));
    } else els.forEach(mount);
    window.addEventListener('resize', () => { views.forEach(v => v.draw()); });
  }

  /* ---------- section profiles ---------- */
  const KIND = { facet: 'crease', steps: 'steps', chip: 'dip', dish: 'dip', pits: 'dips', ridge: 'bump', crack: 'dip', undul: 'wave', grain: 'noise' };
  function median(a) { const b = a.slice().sort((x, y) => x - y); return b[Math.floor(b.length / 2)]; }
  function pct(a, q) { const b = a.slice().sort((x, y) => x - y); return b[Math.floor((b.length - 1) * q)]; }
  function el(tag, attrs, txt) { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); if (txt != null) e.textContent = txt; return e; }
  function profile(host, p) {
    host.textContent = '';
    const id = host.dataset.prof, P = window.PROFILES[id](p);
    const n = 420, xs = [], ys = []; for (let i = 0; i <= n; i++) { const x = P.w * i / n; xs.push(x); ys.push(P.fn(x)); }
    const W = 380, L = 34, R = 14, T = 44, sx = (W - L - R) / P.w;
    const ymin = Math.min.apply(null, ys), ymax = Math.max.apply(null, ys);
    const sy = sx, hpx = (ymax - ymin) * sy, base = 16, H = T + hpx + base + 36;
    const X = (x) => L + x * sx, Y = (y) => T + (ymax - y) * sy;
    const svg = el('svg', { viewBox: '0 0 ' + W + ' ' + Math.round(H), class: 'prof', role: 'img', 'aria-label': 'Section profile of the ' + id + ' model' });
    /* scale bar and ground */
    let d = 'M' + X(0) + ',' + (T + hpx + base); xs.forEach((x, i) => { d += ' L' + f2(X(x)) + ',' + f2(Y(ys[i])); }); d += ' L' + X(P.w) + ',' + (T + hpx + base) + ' Z';
    svg.appendChild(el('path', { d: d, class: 'base' }));
    let line = ''; xs.forEach((x, i) => { line += (i ? ' L' : 'M') + f2(X(x)) + ',' + f2(Y(ys[i])); });
    svg.appendChild(el('path', { d: line, class: 'land', fill: 'none' }));
    const kind = KIND[id], med = (kind === 'dip' ? pct(ys, .88) : kind === 'bump' ? pct(ys, .12) : median(ys)), dims = [];
    const dimV = (x, ya, yb, txt, side) => { const xp = X(x), a = Y(ya), b = Y(yb); svg.appendChild(el('path', { d: 'M' + xp + ',' + a + ' V' + b + ' M' + (xp - 3) + ',' + a + ' h6 M' + (xp - 3) + ',' + b + ' h6', class: 'dimline' })); svg.appendChild(el('text', { x: xp + (side === 'l' ? -6 : 7), y: (a + b) / 2 + 3, class: 'dimtxt', 'text-anchor': side === 'l' ? 'end' : 'start' }, txt)); };
    const dimH = (x0, x1, y, txt) => { const a = X(x0), b = X(x1); svg.appendChild(el('path', { d: 'M' + a + ',' + y + ' H' + b + ' M' + a + ',' + (y - 3) + ' v6 M' + b + ',' + (y - 3) + ' v6', class: 'dimline' })); svg.appendChild(el('text', { x: (a + b) / 2, y: y - 6, class: 'dimtxt', 'text-anchor': 'middle' }, txt)); };
    let facts = '';
    if (kind === 'dip' || kind === 'bump') {
      const sg = kind === 'dip' ? -1 : 1, rel = ys.map(y => sg * (y - med)), pk = Math.max.apply(null, rel), im = rel.indexOf(pk), thr = Math.max(.06, pk * .12);
      let a = im, b = im; while (a > 0 && rel[a] > thr) a--; while (b < n && rel[b] > thr) b++;
      const wmm = xs[b] - xs[a];
      dimH(xs[a], xs[b], Y(med) - (kind === 'bump' ? 6 : 8) - (kind === 'bump' ? hpx * 0 : 0) + (kind === 'dip' ? -4 : 0) - 8 * 0, f1(wmm) + ' mm wide');
      dimV(xs[im], med, ys[im], f2(pk) + ' mm ' + (kind === 'dip' ? 'deep' : 'high'), 'r');
      facts = (kind === 'dip' ? 'depth ' : 'height ') + f2(pk) + ' mm, width ' + f1(wmm) + ' mm';
    } else if (kind === 'dips') {
      facts = (P.count || 0) + ' pits, ' + f1(p.dmin) + ' to ' + f1(Math.max(p.dmin, p.dmax)) + ' mm across, up to ' + f2(p.depth) + ' mm deep';
      const rel = ys.map(y => med - y); let k = 0; for (let i = 1; i < n && k < 2; i++) if (rel[i] > .12 && rel[i - 1] <= .12) { let j = i; while (j < n && rel[j] > .12) j++; const pk = Math.max.apply(null, rel.slice(i, j)); dimH(xs[i], xs[j], Y(med) - 6 - k * 0, f1(xs[j] - xs[i]) + ' mm'); k++; i = j; }
    } else if (kind === 'steps') {
      const dy = ys.map((y, i) => i ? (y - ys[i - 1]) : 0); let ris = [], i = 0; while (i < n) { if (dy[i] > .03) { let j = i; let tot = 0; while (j < n && dy[j] > .03) { tot += dy[j]; j++; } ris.push({ x: xs[i], x1: xs[j], h: tot, y0: ys[i - 1], y1: ys[j] }); i = j; } else i++; }
      ris.forEach((r, k) => { dimV(r.x1 + .8, r.y0, r.y0 + r.h, f1(r.h) + ' mm', 'r'); if (k < ris.length - 1) dimH(r.x1, ris[k + 1].x, Y(ymax) - 10 - (ris.length - 1 - k) * 0 + 0, f1(ris[k + 1].x - r.x1) + ' mm tread'); });
      const rh = ris.reduce((a, r) => a + r.h, 0) / Math.max(1, ris.length), td = ris.length > 1 ? ris[1].x - ris[0].x1 : 0;
      facts = (td ? 'riser : tread 1 : ' + f1(td / rh) + ' (risers ' : '(one riser ') + f1(rh) + ' mm' + (td ? ', treads about ' + f1(td) + ' mm)' : ')');
    } else if (kind === 'crease') {
      const im = ys.indexOf(ymax), ci = (i) => Math.min(n, Math.max(0, i)), sl = (ys[ci(im - 40)] - ys[ci(im - 60)]) / ((xs[ci(im - 40)] - xs[ci(im - 60)]) || 1), sr = (ys[ci(im + 60)] - ys[ci(im + 40)]) / ((xs[ci(im + 60)] - xs[ci(im + 40)]) || 1);
      const ang = Math.atan(sl) * 180 / Math.PI + Math.atan(-sr) * 180 / Math.PI;
      const late = X(xs[im]) > W * .5; svg.appendChild(el('text', { x: X(xs[im]) + (late ? -8 : 8), y: Y(ymax) - 6, class: 'dimtxt', 'text-anchor': late ? 'end' : 'start' }, 'crease, planes differ by ' + f1(Math.abs(ang)) + '°'));
      svg.appendChild(el('circle', { cx: X(xs[im]), cy: Y(ymax), r: 3, fill: 'none', stroke: 'var(--dim)' }));
      facts = 'plane change ' + f1(Math.abs(ang)) + ' degrees across a span of ' + f1(P.w * .8) + ' mm';
    } else if (kind === 'wave') {
      dimV(xs[ys.indexOf(ymax)], ymax, ymin, f2(ymax - ymin) + ' mm peak to low', 'r'); facts = 'peak to low ' + f2(ymax - ymin) + ' mm over ' + P.w + ' mm';
    } else {
      dimV(P.w * .9, ymax, ymin, f2(ymax - ymin) + ' mm', 'l'); facts = 'texture amplitude ' + f2(ymax - ymin) + ' mm';
    }
    /* scale: 5 mm bar; the section is drawn at true scale */
    const by = T + hpx + base + 18; svg.appendChild(el('path', { d: 'M' + L + ',' + by + ' h' + 5 * sx + ' m0,-3 v6 m' + (-5 * sx) + ',-3 v6', class: 'dimline' })); svg.appendChild(el('text', { x: L + 5 * sx + 6, y: by + 3 }, '5 mm'));
    svg.appendChild(el('text', { x: W - R, y: by + 3, 'text-anchor': 'end' }, 'true scale, both ways'));
    svg.appendChild(el('text', { x: L, y: 14, class: 'dimtxt' }, 'Section through the model'));
    host.appendChild(svg);
    return facts;
  }

  /* ---------- static diagrams ---------- */
  const D = {};
  const svgOpen = (w, h, label) => '<svg viewBox="0 0 ' + w + ' ' + h + '" class="prof" role="img" aria-label="' + label + '">';
  /* the angle families on one cliff section */
  D.angles = () => {
    let s = svgOpen(640, 330, 'The angle families of a cliff section');
    s += '<path class="base" d="M20,300 H620 V312 H20Z"/>';
    s += '<path class="land fill1" d="M30,300 L30,258 L96,258 L96,214 L150,214 L150,168 L214,162 L330,64 L396,64 L396,28 L470,28 L470,300Z"/>';
    s += '<path class="thin" d="M120,300 L196,198 M290,300 L384,214 M214,162 L262,300"/>';
    const t = (x, y, a) => '<text x="' + x + '" y="' + y + '" class="dimtxt">' + a + '</text>';
    const ld = (x0, y0, x1, y1) => '<path class="dimline" d="M' + x0 + ',' + y0 + ' L' + x1 + ',' + y1 + '"/><circle cx="' + x1 + '" cy="' + y1 + '" r="2" fill="var(--dim)"/>';
    s += t(486, 140, 'vertical faces 85\u201390\u00b0') + ld(484, 137, 470, 137);
    s += t(24, 112, 'shelf / ledge 0\u201310\u00b0') + ld(96, 116, 123, 214);
    s += t(24, 150, 'riser 70\u201390\u00b0') + ld(60, 154, 96, 236);
    s += t(150, 62, 'sloped face 25\u201360\u00b0') + ld(262, 66, 272, 113);
    s += t(236, 290, 'diagonal cleavage 30\u201360\u00b0') + ld(330, 280, 330, 262).replace('330,280 L330,262', '330,280 L340,262');
    s += t(300, 196, 'cross joints 60\u2013120\u00b0');
    s += '<path class="dimline" d="M322,72 A40,40 0 0 1 337,100"/><text x="620" y="326" text-anchor="end">section, not to scale</text></svg>';
    return s;
  };
  /* fracture sets in plan: 1 to 3 sets, 20 to 60 degrees apart, carry, cracks that stop */
  D.sets = () => {
    const r = Eng.rng(12); let s = svgOpen(560, 300, 'Fracture sets in plan');
    s += '<rect class="fill2" x="20" y="14" width="520" height="250" stroke="var(--rule)"/>';
    const sets = [[0, 1, 'set A, carry 1'], [38, .4, 'set B, carry about .35'], [100, .3, 'set C (cross), carry about .3']];
    sets.forEach((st, k) => {
      const a = st[0] * Math.PI / 180, dx = Math.cos(a), dy = Math.sin(a), n = k === 0 ? 4 : 9;
      for (let i = 0; i < n; i++) {
        const cx = 40 + r() * 480, cy = 30 + (k === 0 ? (i + .5) * 250 / n * .9 : r() * 220), len = k === 0 ? 480 : 70 + r() * 150, x0 = cx - dx * len / 2, y0 = cy - dy * len / 2, x1 = cx + dx * len / 2, y1 = cy + dy * len / 2;
        const cl = (x, y) => [Math.max(22, Math.min(538, x)), Math.max(16, Math.min(262, y))];
        const p = cl(x0, y0), q = cl(x1, y1);
        s += '<path d="M' + f1(p[0]) + ',' + f1(p[1]) + ' L' + f1(q[0]) + ',' + f1(q[1]) + '" stroke="' + ['var(--f1)', 'var(--f2)', 'var(--f3)'][k] + '" stroke-width="' + (k === 0 ? 2.2 : 1.6) + '" fill="none"/>';
        if (k) s += '<circle cx="' + f1(q[0]) + '" cy="' + f1(q[1]) + '" r="2.6" fill="var(--surface)" stroke="' + ['var(--f1)', 'var(--f2)', 'var(--f3)'][k] + '"/>';
      }
    });
    s += '<text x="26" y="284">' + sets.map(x => x[2]).join('   ·   ') + '</text>';
    s += '<text x="534" y="30" text-anchor="end" class="dimtxt">sets 20–60° apart; a crack that stops ends in a ring</text></svg>';
    return s;
  };
  /* one mass broken at three sizes: plan of nested cells with the section of offsets under it */
  D.levels = () => {
    let s = svgOpen(560, 270, 'One mass broken at three levels');
    const A = [[24, 20, 190, 90], [214, 20, 170, 90], [384, 20, 152, 90], [24, 110, 130, 90], [154, 110, 230, 90], [384, 110, 152, 90]];
    A.forEach(a => { s += '<rect x="' + a[0] + '" y="' + a[1] + '" width="' + a[2] + '" height="' + a[3] + '" fill="var(--surface2)" stroke="var(--ink)" stroke-width="2.2"/>'; });
    const B = [[24, 20, 190, 90, .5], [214, 20, 170, 90, .35], [154, 110, 230, 90, .6], [384, 110, 152, 90, .45], [24, 110, 130, 90, .4], [384, 20, 152, 90, .55]], r = Eng.rng(6);
    B.forEach(b => { const cx = b[0] + b[2] * (.3 + .4 * r()), cy = b[1] + b[3] * (.3 + .4 * r()); s += '<path d="M' + f1(cx) + ',' + b[1] + ' V' + f1(b[1] + b[3] * b[4]) + ' H' + (b[0] + b[2]) + ' M' + b[0] + ',' + f1(cy) + ' H' + f1(cx + 20) + ' V' + (b[1] + b[3]) + '" stroke="var(--f2)" stroke-width="1.5" fill="none"/>'; s += '<circle cx="' + f1(cx) + '" cy="' + f1(b[1] + b[3] * b[4]) + '" r="2.4" fill="var(--surface)" stroke="var(--f2)"/>'; });
    s += '<path d="M60,40 h50 v24 M300,60 h30 M440,150 h30 v20 M80,160 h24 M230,140 h40 v22" stroke="var(--f3)" stroke-width="1" fill="none"/>';
    s += '<text x="24" y="232" class="dimtxt">level 1 (black): 12–40 mm cells, offset up to 1.7</text><text x="24" y="246" class="dimtxt">level 2 (teal): cracks stop inside their parent; offset 0.4 of level 1</text><text x="24" y="260" class="dimtxt">level 3 (amber): on 50–80% of cells; offset 0.4 of level 2</text></svg>';
    return s;
  };
  /* edges: profile of a sharp edge, a crisp 0.45 chamfer, a 3-facet round, and the 70/30 split */
  D.edges = () => {
    let s = svgOpen(560, 230, 'Edge profiles');
    const e = (x, kind, label, size) => {
      const x0 = x, y0 = 40, w = 100, h = 100; let p;
      if (kind === 'sharp') p = 'M' + x0 + ',' + (y0 + h) + ' V' + y0 + ' H' + (x0 + w) + ' V' + (y0 + h);
      if (kind === 'crisp') p = 'M' + x0 + ',' + (y0 + h) + ' V' + (y0 + 5) + ' L' + (x0 + 5) + ',' + y0 + ' H' + (x0 + w - 5) + ' L' + (x0 + w) + ',' + (y0 + 5) + ' V' + (y0 + h);
      if (kind === 'soft') { const c = 24; p = 'M' + x0 + ',' + (y0 + h) + ' V' + (y0 + c); [[.0, 0], [22.5, 0], [45, 0], [67.5, 0], [90, 0]].forEach(() => { }); const pts = []; for (let i = 0; i <= 12; i++) { const a = Math.PI / 2 * i / 12; pts.push([x0 + c - Math.cos(a) * c, y0 + c - Math.sin(a) * c]); } p += pts.map(q => ' L' + f1(q[0]) + ',' + f1(q[1])).join(''); p += ' H' + (x0 + w - c); const pts2 = []; for (let i = 0; i <= 12; i++) { const a = Math.PI / 2 * i / 12; pts2.push([x0 + w - c + Math.sin(a) * c, y0 + c - Math.cos(a) * c]); } p += pts2.map(q => ' L' + f1(q[0]) + ',' + f1(q[1])).join('') + ' V' + (y0 + h); }
      if (kind === 'chip') { p = 'M' + x0 + ',' + (y0 + h) + ' V' + (y0 + 8) + ' L' + (x0 + 6) + ',' + y0 + ' H' + (x0 + 50) + ' L' + (x0 + 80) + ',' + (y0 + 26) + ' L' + (x0 + 74) + ',' + (y0 + 38) + ' L' + (x0 + w) + ',' + (y0 + 44) + ' V' + (y0 + h); }
      return '<path class="land fill1" d="' + p + ' Z"/><text x="' + (x0 + w / 2) + '" y="168" text-anchor="middle" class="dimtxt">' + label + '</text><text x="' + (x0 + w / 2) + '" y="182" text-anchor="middle">' + size + '</text>';
    };
    s += e(20, 'sharp', 'sharp', '90°, 0 mm') + e(150, 'crisp', 'crisp (30%)', 'chamfer 0 to 0.5 mm') + e(280, 'soft', 'soft (70%)', 'eased 0.7 to 2.8 mm') + e(410, 'chip', 'broken', 'chip 1.4 to 7 mm');
    s += '<text x="20" y="216">Section through a block corner. 100 px = 14 mm.</text></svg>';
    return s;
  };
  /* skin over core: section. 12 px = 1 mm */
  D.skin = () => {
    let s = svgOpen(560, 240, 'A skin over a core, in section');
    s += '<path class="base" d="M20,214 H540 V226 H20Z"/><rect x="150" y="20" width="390" height="194" class="fill2" stroke="var(--ink)" stroke-width="1.3"/>';
    s += '<path d="M107,20 V214" stroke="var(--ink3)" stroke-dasharray="3 3"/>';
    const sb = [5, 14, 2, 23, 10], lean = [2, -3, 4, -2, 3], ys = [22, 62, 100, 140, 176, 214];
    for (let i = 0; i < 5; i++) { const xf = 107 + sb[i], y0 = ys[i] + 1, y1 = ys[i + 1] - 1; s += '<path class="land fill1" d="M150,' + y0 + ' H' + (xf + lean[i]) + ' L' + (xf - lean[i]) + ',' + y1 + ' H150Z"/>'; }
    s += '<text x="160" y="16" class="dimtxt">core: one body, never cut through</text>';
    s += '<path class="dimline" d="M107,226 H150 M107,222 v8 M150,222 v8"/><text x="128" y="240" text-anchor="middle" class="dimtxt">skin 3.6</text>';
    s += '<text x="170" y="236">pieces 5&#8211;9 wide; setback 0.2&#8211;1.9; lean 1&#8211;9&#176;; 12 px = 1 mm</text>';
    s += '<text x="20" y="40" class="dimtxt">unbroken face</text><path class="dimline" d="M70,44 L105,60"/></svg>';
    return s;
  };
  D.pitwrong = () => {
    let s = svgOpen(420, 210, 'A pit as a cell inside a cell, against a pit as a plane cut');
    s += '<rect x="10" y="30" width="190" height="150" class="fill2" stroke="var(--ink)" stroke-width="1.3"/>';
    s += '<path class="e" d="M10,30 L80,80 M200,30 L140,80 M10,180 L80,130 M200,180 L140,130 M80,80 H140 V130 H80 Z M80,80 V130 M140,80 V130"/>';
    s += '<rect x="80" y="80" width="60" height="50" fill="var(--surface)" stroke="var(--err)" stroke-width="1.5"/>';
    s += '<text x="105" y="20" text-anchor="middle" class="dimtxt" style="fill:var(--err)">cell in a cell: slivers</text>';
    s += '<rect x="220" y="30" width="190" height="150" class="fill2" stroke="var(--ink)" stroke-width="1.3"/>';
    s += '<path d="M290,70 L350,64 L366,110 L330,140 L284,120 Z" fill="var(--surface)" stroke="var(--good)" stroke-width="1.6"/><path d="M290,70 L330,140 M350,64 L330,140" stroke="var(--good)" stroke-width="1" fill="none"/>';
    s += '<text x="315" y="20" text-anchor="middle" class="dimtxt" style="fill:var(--good)">a plane cut: one piece</text></svg>';
    return s;
  };
  /* N and O sections with the sheet's ratios */
  D.secN = () => {
    let s = svgOpen(560, 312, 'Section of N, the slanted slab');
    s += '<path class="base" d="M20,260 H540 V274 H20Z"/>';
    s += '<path class="land fill1" d="M110,260 L400,86 L430,66 V260Z"/>';
    s += '<path class="thin" d="M110,260 L400,86 M146,260 L416,90 M190,260 L424,86 M236,260 L428,80"/>';
    s += '<path class="dimline" d="M110,236 A24,24 0 0 1 128,252"/><text x="140" y="246" class="dimtxt">dip 50° in section (the panel); sheets say 32–38°</text>';
    s += '<path class="dimline" d="M444,260 V66 M440,66 h8 M440,260 h8"/><text x="452" y="168" class="dimtxt">H 38–43</text>';
    s += '<text x="120" y="40">sheet: layers .38 / .28 / .22 / .12 of the face width,</text><text x="120" y="54">crest 13 to 43 mm up the slope, slabs 2.6 to 5 mm thick</text>';
    s += '<path class="dimline" d="M20,282 H430 M20,278 v8 M430,278 v8"/><text x="225" y="296" text-anchor="middle" class="dimtxt">wall run about 0.52 L = 18 mm (sheet); the panel reads wider</text></svg>';
    return s;
  };
  D.secO = () => {
    let s = svgOpen(560, 312, 'Section of O, the stacked blocks');
    s += '<path class="base" d="M20,262 H540 V276 H20Z"/>';
    s += '<path class="land fill1" d="M110,262 V196 H170 V148 H260 V92 H360 V262Z"/>';
    s += '<path class="thin" d="M170,196 V262 M260,148 V262 M110,229 H170"/>';
    s += '<path class="dimline" d="M380,262 V92 M376,92 h8 M376,262 h8 M376,196 h8 M376,148 h8"/>';
    s += '<text x="388" y="96" class="dimtxt">.80 H</text><text x="388" y="152" class="dimtxt">.52 H</text><text x="388" y="200" class="dimtxt">.26 H</text>';
    s += '<text x="118" y="188" class="dimtxt">foot</text><text x="178" y="140" class="dimtxt">tier 2</text><text x="268" y="84" class="dimtxt">cap</text>';
    s += '<path class="dimline" d="M110,284 H360 M110,280 v8 M360,280 v8"/><text x="235" y="298" text-anchor="middle" class="dimtxt">footprint .20 / .60 / .20 of L; H about 38 mm; courses 11.5 to 14.5</text></svg>';
    return s;
  };
  function diagrams() { [].slice.call(document.querySelectorAll('[data-diagram]')).forEach(h => { const f = D[h.dataset.diagram]; if (f) h.innerHTML = f(); }); }

  function init() {
    diagrams(); setupViews();
    [].slice.call(document.querySelectorAll('[data-prof]')).forEach(host => { const el = host.closest('.prim').querySelector('.v3d'); if (el && el._st) { profiles(el); showMe(el); } });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
