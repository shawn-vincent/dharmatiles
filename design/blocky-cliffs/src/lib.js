/* ---------- small tools ---------- */
const NS = 'http://www.w3.org/2000/svg';
const $ = (s, r = document) => r.querySelector(s);
const put = (e, attrs, kids) => { for (const k in attrs) { const v = attrs[k]; if (v == null || v === false) continue; if (k === 'text') e.textContent = v; else if (k === 'html') e.innerHTML = v; else if (k.startsWith('on')) e.addEventListener(k.slice(2), v); else e.setAttribute(k, v); } for (const c of kids.flat()) if (c != null) e.append(c); return e; };
const h = (tag, attrs = {}, ...kids) => put(document.createElement(tag), attrs, kids);
const s = (tag, attrs = {}, ...kids) => put(document.createElementNS(NS, tag), attrs, kids);
const fmt = (v, d = 2) => (v == null || !isFinite(v)) ? '–' : (+v).toFixed(d);
const pts = (p) => p.map((q) => q[0] + ',' + q[1]).join(' ');
const quant = (arr, q) => { const a = arr.filter((v) => v != null && isFinite(v)).sort((x, y) => x - y); if (!a.length) return NaN; const i = (a.length - 1) * q, lo = Math.floor(i), hi = Math.ceil(i); return a[lo] + (a[hi] - a[lo]) * (i - lo); };
const stats = (arr) => ({ n: arr.filter((v) => v != null && isFinite(v)).length, min: quant(arr, 0), p10: quant(arr, .1), q1: quant(arr, .25), med: quant(arr, .5), q3: quant(arr, .75), p90: quant(arr, .9), max: quant(arr, 1) });
const range = (st, d = 2) => fmt(st.p10, d) + '–' + fmt(st.p90, d);
const AP = 'ABCDEFGHIJKLMNOP'.split(''), TT = Array.from({ length: 20 }, (_, i) => 'T' + (i + 1)), SS = Array.from({ length: 16 }, (_, i) => 'S' + (i + 1));
const ALL = [...AP, ...TT, ...SS];
const imgOf = (id) => 'img/' + (id.length === 1 ? 'ap-' : '') + id.toLowerCase() + '.jpg';
const KINDS = ['slab', 'block', 'pillar', 'lean', 'cap', 'toe'];
const CLASSES = ['pillars', 'blocks', 'leaners', 'courses', 'gates'];
const CLASS_NAME = { pillars: 'Pillars', blocks: 'Blocks', leaners: 'Leaners', courses: 'Courses', gates: 'Gates' };

/* ---------- data: the traces and what was measured from them ---------- */
const [TR, ME, CL] = await Promise.all(['data/traces.json', 'data/measures.json', 'data/classes.json'].map((u) => fetch(u).then((r) => { if (!r.ok) throw new Error(u + ': ' + r.status); return r.json(); })));
const STONES = ME.stones, PAIRS = ME.pairs, CELLS = ME.cells;
const stoneById = Object.fromEntries(STONES.map((x) => [x.id, x]));
for (const st of STONES) st.cls = CL[st.cell].class;

/* ---------- a reference cell with its traced stones over it ---------- */
function cellView(id, o = {}) {
  const c = TR[id];
  const svg = s('svg', { viewBox: '0 0 100 100', preserveAspectRatio: 'none', style: 'position:absolute;inset:0;width:100%;height:100%' });
  if (o.mass) svg.append(s('polygon', { points: pts(CELLS[id].mass), fill: 'none', stroke: 'var(--a-dim)', 'stroke-width': 2, 'vector-effect': 'non-scaling-stroke' }));
  if (o.stones !== false) for (const st of c.stones) {
    const want = !o.only || o.only(st, stoneById[st.id]);
    const g = s('g', { 'data-stone': st.id, class: 'tr' + (want ? '' : ' dim') });
    g.append(s('polygon', { points: pts(st.out), class: 'o' }));
    if (st.top && o.tops !== false) g.append(s('polygon', { points: pts(st.top), class: 't' }));
    if (o.onstone) { g.addEventListener('pointerenter', () => o.onstone(st.id)); g.addEventListener('click', () => o.onstone(st.id)); }
    svg.append(g);
  }
  if (o.frame && c.plate) { svg.append(s('polyline', { points: pts(c.plate), fill: 'none', stroke: 'var(--a-dim)', 'stroke-width': 2, 'vector-effect': 'non-scaling-stroke' })); const b = c.fig.base; svg.append(s('ellipse', { cx: b[0], cy: b[1], rx: b[2], ry: b[3], fill: 'none', stroke: 'var(--a-dim)', 'stroke-width': 2, 'vector-effect': 'non-scaling-stroke' }), s('line', { x1: c.fig.foot[0], y1: c.fig.foot[1], x2: c.fig.head[0], y2: c.fig.head[1], stroke: 'var(--a-dim)', 'stroke-width': 2, 'vector-effect': 'non-scaling-stroke' })); }
  if (o.extra) o.extra(svg, c);
  const kids = [h('img', { src: o.img || imgOf(id), alt: 'Reference formation ' + id, style: o.fade ? 'opacity:' + o.fade : null }), svg, o.tag === false ? null : h('span', { class: 'tag', text: o.label || id })];
  const d = h('div', { class: 'cell stage', 'data-cell': id }, kids);
  if (o.onpick) { d.style.cursor = 'pointer'; d.tabIndex = 0; d.setAttribute('role', 'button'); d.addEventListener('click', () => o.onpick(id)); d.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); o.onpick(id); } }); }
  return o.sub ? h('div', { style: 'min-width:0' }, d, h('div', { class: 'sub', html: o.sub })) : d;
}

/* ---------- charts: drawn as SVG at the width of the box they sit in, drawn again when that width changes ---------- */
const DRAWS = [];
function chart(target, height, draw) {
  const box = typeof target === 'string' ? document.getElementById(target) : target; if (!box) return;
  box.classList.add('chart'); let last = 0;
  const run = () => { const w = Math.floor(box.clientWidth); if (!w || w === last) return; last = w; const svg = s('svg', { width: w, height, viewBox: `0 0 ${w} ${height}`, role: 'img' }); draw(svg, w, height); box.replaceChildren(svg); };
  new ResizeObserver(run).observe(box); DRAWS.push(() => { last = 0; run(); }); run();
}
const scale = (d0, d1, r0, r1) => (v) => r0 + (v - d0) / (d1 - d0) * (r1 - r0);
const ticks = (d0, d1, n = 5) => { const span = d1 - d0, step0 = Math.pow(10, Math.floor(Math.log10(span / n))), err = span / n / step0; const step = step0 * (err >= 7.5 ? 10 : err >= 3.5 ? 5 : err >= 1.5 ? 2 : 1); const out = []; for (let v = Math.ceil(d0 / step - 1e-9) * step; v <= d1 + 1e-9; v += step) out.push(+v.toFixed(6)); return out; };
function axes(svg, w, hgt, o) {
  const m = Object.assign({ l: 40, r: 10, t: 10, b: 34 }, o.m || {});
  const x = scale(o.x[0], o.x[1], m.l, w - m.r), y = scale(o.y[0], o.y[1], hgt - m.b, m.t);
  const g = s('g', { 'font-size': 11 });
  for (const v of (o.xt || ticks(o.x[0], o.x[1], w < 420 ? 4 : 7))) g.append(s('line', { x1: x(v), x2: x(v), y1: m.t, y2: hgt - m.b, stroke: 'var(--rule)' }), s('text', { x: x(v), y: hgt - m.b + 14, 'text-anchor': 'middle', class: 'mut', text: o.xf ? o.xf(v) : v }));
  for (const v of (o.yt || ticks(o.y[0], o.y[1], 4))) g.append(s('line', { x1: m.l, x2: w - m.r, y1: y(v), y2: y(v), stroke: 'var(--rule)' }), s('text', { x: m.l - 5, y: y(v) + 4, 'text-anchor': 'end', class: 'mut', text: o.yf ? o.yf(v) : v }));
  if (o.xl) g.append(s('text', { x: (m.l + w - m.r) / 2, y: hgt - 4, 'text-anchor': 'middle', text: o.xl }));
  if (o.yl) g.append(s('text', { x: 4, y: m.t + 2, 'dominant-baseline': 'hanging', text: o.yl }));
  svg.append(g); return { x, y, m };
}
/* a histogram of measured values, with an optional band for a proposed range */
function hist(target, vals, o) {
  chart(target, o.h || 210, (svg, w, hgt) => {
    const v = vals.filter((q) => q != null && isFinite(q)), n = o.bins || 14, [a, b] = o.x;
    const bins = Array(n).fill(0); for (const q of v) { const i = Math.floor((Math.min(Math.max(q, a), b - 1e-9) - a) / (b - a) * n); bins[i]++; }
    const top = Math.max(...bins) * 1.12;
    const ax = axes(svg, w, hgt, { x: o.x, y: [0, top], xl: o.xl, yl: o.yl || 'stones', xf: o.xf, m: { t: 18 } });
    if (o.prop) svg.append(s('rect', { x: ax.x(o.prop[0]), y: ax.m.t, width: ax.x(o.prop[1]) - ax.x(o.prop[0]), height: hgt - ax.m.b - ax.m.t, fill: 'var(--accent-soft)', opacity: .55 }), s('line', { x1: ax.x(o.prop[0]), x2: ax.x(o.prop[1]), y1: ax.m.t, y2: ax.m.t, stroke: 'var(--accent)', 'stroke-width': 2, 'stroke-dasharray': '5 3' }));
    bins.forEach((c, i) => { if (!c) return; const x0 = ax.x(a + i / n * (b - a)), x1 = ax.x(a + (i + 1) / n * (b - a)); svg.append(s('rect', { x: x0 + 1, y: ax.y(c), width: Math.max(1, x1 - x0 - 2), height: ax.y(0) - ax.y(c), fill: o.fill || 'var(--a-out)', rx: 1 })); });
    const st = stats(v); const my = ax.m.t + 2;
    svg.append(s('line', { x1: ax.x(Math.max(a, st.p10)), x2: ax.x(Math.min(b, st.p90)), y1: my + 6, y2: my + 6, stroke: 'var(--fg)', 'stroke-width': 1.5 }), s('line', { x1: ax.x(Math.max(a, st.q1)), x2: ax.x(Math.min(b, st.q3)), y1: my + 6, y2: my + 6, stroke: 'var(--fg)', 'stroke-width': 5 }), s('circle', { cx: ax.x(st.med), cy: my + 6, r: 4, fill: 'var(--panel)', stroke: 'var(--fg)', 'stroke-width': 2 }));
  });
}
/* a scatter of measured values: groups get the page's annotation colours and their own mark */
const MARK = { pillars: ['var(--a-out)', 'c'], blocks: ['var(--a-dim)', 'q'], leaners: ['var(--a-ax)', 'd'], courses: ['var(--a-top)', 't'], gates: ['var(--a-gap)', 'x'],
  pillar: ['var(--a-out)', 'c'], block: ['var(--a-dim)', 'q'], lean: ['var(--a-ax)', 'd'], slab: ['var(--a-top)', 't'], cap: ['var(--a-gap)', 'x'], toe: ['var(--muted)', 'p'] };
function mark(kind, x, y, r = 4, extra = {}) {
  const [col, sh] = MARK[kind] || ['var(--fg)', 'c']; const a = Object.assign({ fill: col, 'fill-opacity': .72, stroke: col, 'stroke-width': 1 }, extra);
  if (sh === 'c') return s('circle', Object.assign({ cx: x, cy: y, r }, a));
  if (sh === 'q') return s('rect', Object.assign({ x: x - r * .85, y: y - r * .85, width: r * 1.7, height: r * 1.7 }, a));
  if (sh === 'd') return s('polygon', Object.assign({ points: `${x},${y - r * 1.2} ${x + r * 1.1},${y} ${x},${y + r * 1.2} ${x - r * 1.1},${y}` }, a));
  if (sh === 't') return s('polygon', Object.assign({ points: `${x},${y - r * 1.15} ${x + r * 1.15},${y + r * .9} ${x - r * 1.15},${y + r * .9}` }, a));
  if (sh === 'p') return s('polygon', Object.assign({ points: `${x},${y + r * 1.15} ${x + r * 1.15},${y - r * .9} ${x - r * 1.15},${y - r * .9}` }, a));
  return s('path', Object.assign({ d: `M${x - r},${y - r}L${x + r},${y + r}M${x - r},${y + r}L${x + r},${y - r}` }, a, { 'stroke-width': 2.2, fill: 'none' }));
}
function legend(kinds, names) { return h('div', { class: 'legend' }, kinds.map((k) => { const sv = s('svg', { width: 14, height: 14, viewBox: '0 0 14 14' }, mark(k, 7, 7, 4.2)); return h('span', {}, sv, (names && names[k]) || k); })); }
function scatter(target, rows, o) {
  chart(target, o.h || 260, (svg, w, hgt) => {
    const ax = axes(svg, w, hgt, o);
    if (o.under) o.under(svg, ax, w, hgt);
    for (const r of rows) { const X = o.fx(r), Y = o.fy(r); if (X == null || Y == null || !isFinite(X) || !isFinite(Y)) continue; const m = mark(o.fk ? o.fk(r) : 'pillar', ax.x(Math.min(Math.max(X, o.x[0]), o.x[1])), ax.y(Math.min(Math.max(Y, o.y[0]), o.y[1])), o.r || 3.6); if (o.title) m.append(s('title', { text: o.title(r) })); svg.append(m); }
    if (o.over) o.over(svg, ax, w, hgt);
  });
}
/* rows of dots: one row per group, every measured stone a dot, a tick at each row's median and a bar from its 10th to 90th percentile */
function strip(target, groups, o) {
  chart(target, (o.rowH || 30) * groups.length + 44, (svg, w, hgt) => {
    const ax = axes(svg, w, hgt, { x: o.x, y: [0, 1], yt: [], xl: o.xl, xt: o.xt, m: { l: o.left || 80, t: 6 } }), rh = (hgt - ax.m.b - ax.m.t) / groups.length;
    if (o.prop) svg.append(s('rect', { x: ax.x(o.prop[0]), y: ax.m.t, width: ax.x(o.prop[1]) - ax.x(o.prop[0]), height: hgt - ax.m.b - ax.m.t, fill: 'var(--accent-soft)', opacity: .5 }));
    if (o.ref != null) svg.append(s('line', { x1: ax.x(o.ref), x2: ax.x(o.ref), y1: ax.m.t, y2: hgt - ax.m.b, stroke: 'var(--fg)', 'stroke-dasharray': '3 3' }));
    groups.forEach((g, i) => { const y = ax.m.t + rh * (i + .5), r = rng(i + 3), v = g.vals.filter((q) => q != null && isFinite(q)), st = stats(v);
      svg.append(s('text', { x: 4, y: y + 4, 'font-size': 12, text: g.name }), s('text', { x: ax.m.l - 6, y: y + 4, 'font-size': 10.5, 'text-anchor': 'end', class: 'mut', text: 'n ' + v.length }));
      for (const q of v) svg.append(mark(g.kind || 'pillar', ax.x(Math.min(Math.max(q, o.x[0]), o.x[1])), y + (r() - .5) * rh * .62, 2.4, { 'fill-opacity': .5, 'stroke-width': 0 }));
      if (v.length) svg.append(s('line', { x1: ax.x(Math.max(o.x[0], st.p10)), x2: ax.x(Math.min(o.x[1], st.p90)), y1: y, y2: y, stroke: 'var(--fg)', 'stroke-width': 1.5 }), s('line', { x1: ax.x(st.med), x2: ax.x(st.med), y1: y - rh * .36, y2: y + rh * .36, stroke: 'var(--fg)', 'stroke-width': 3 })); });
  });
}
const kindGroups = (f, kinds = KINDS) => kinds.map((k) => ({ name: k, kind: k, vals: STONES.filter((x) => x.kind === k).map(f) }));
const classGroups = (f) => CLASSES.map((k) => ({ name: CLASS_NAME[k], kind: k, vals: ALL.filter((id) => CL[id].class === k).map((id) => f(CELLS[id])) }));
const pointIn = (p, poly) => { let c = false; for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) { const a = poly[i], b = poly[j]; if ((a[1] > p[1]) !== (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]) c = !c; } return c; };
/* a formation's outline in fh, with its lowest right corner at (0, 0), x to the left negative, y up */
const massFh = (id) => { const c = CELLS[id], [W, H] = CROPS[TR[id].img].size, f = c.fh * H / 100, P = c.mass.map((p) => [p[0] * W / 100, p[1] * H / 100]); const x1 = Math.max(...P.map((p) => p[0])), y1 = Math.max(...P.map((p) => p[1])); return P.map((p) => [(p[0] - x1) / f, (y1 - p[1]) / f]); };
/* a small table of what was measured: each row a name, the values, and how many decimals */
function statTable(target, rows) {
  const t = typeof target === 'string' ? document.getElementById(target) : target;
  t.append(h('tr', {}, ['', 'Typical', 'Usual range', 'Extremes', 'Counted'].map((x, i) => h('th', { class: i ? 'num' : null, text: x }))), ...rows.map(([name, vals, d = 2, unit = '']) => { const st = stats(vals); return h('tr', {}, h('td', { html: name }), h('td', { class: 'num', text: fmt(st.med, d) + unit }), h('td', { class: 'num', text: fmt(st.p10, d) + '–' + fmt(st.p90, d) }), h('td', { class: 'num', text: fmt(st.min, d) + '–' + fmt(st.max, d) }), h('td', { class: 'num', text: st.n })); }));
}
