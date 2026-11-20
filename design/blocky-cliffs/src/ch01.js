/* ---------- chapter 1 ---------- */
{
  const sheets = [['A–P smooth', AP, null], ['A–P textured', AP, 'sq-'], ['T1–T20', TT, null], ['S1–S16', SS, null]];
  let sheet = 0, cur = 'H';
  const big = $('#insp-big'), table = $('#insp-table'), grid = $('#insp-grid'), note = $('#insp-note');
  const hi = (sid) => { for (const g of big.querySelectorAll('.tr')) g.classList.toggle('on', g.dataset.stone === sid); for (const r of table.querySelectorAll('tr[data-stone]')) r.classList.toggle('on', r.dataset.stone === sid); };
  const show = (id) => {
    cur = id; const pre = sheets[sheet][2];
    big.replaceChildren(cellView(id, { img: pre ? 'img/' + pre + id.toLowerCase() + '.jpg' : null, mass: true, onstone: hi }));
    const c = CELLS[id];
    note.innerHTML = `<b>${id}</b> · ${CLASS_NAME[CL[id].class]} · ${c.n} stones · ${fmt(c.W)} fh wide, ${fmt(c.H)} fh high as seen. ${CL[id].note}`;
    table.replaceChildren(h('tr', {}, ['Stone', 'Kind', 'Width', 'Face height', 'Height ÷ width', 'Lean', 'Share'].map((t, i) => h('th', { class: i > 1 ? 'num' : null, text: t }))),
      ...STONES.filter((x) => x.cell === id).map((x) => h('tr', { 'data-stone': x.id, onpointerenter: () => hi(x.id) }, h('td', { text: x.id }), h('td', { text: x.kind + (x.ground ? '' : ' ↑') }), h('td', { class: 'num', text: fmt(x.wb) }), h('td', { class: 'num', text: fmt(x.hb) }), h('td', { class: 'num', text: fmt(x.slender) }), h('td', { class: 'num', text: fmt(x.lean, 0) + '°' }), h('td', { class: 'num', text: fmt(x.share * 100, 0) + '%' }))));
    for (const d of grid.children) d.classList.toggle('sel', d.dataset.cell === id);
  };
  const fill = () => {
    const [, names, pre] = sheets[sheet];
    grid.replaceChildren(...names.map((id) => cellView(id, { img: pre ? 'img/' + pre + id.toLowerCase() + '.jpg' : null, onpick: show, tops: false })));
    if (!names.includes(cur)) cur = names[0]; show(cur);
  };
  const pills = $('#insp-sheet');
  sheets.forEach(([name], i) => pills.append(h('button', { type: 'button', 'aria-pressed': i === 0, text: name, onclick: (e) => { sheet = i; for (const b of [...pills.children].slice(0, sheets.length)) b.setAttribute('aria-pressed', b === e.target); fill(); } })));
  const rd = h('button', { type: 'button', 'aria-pressed': false, text: 'Redrawn, no photo', style: 'margin-left:10px', onclick: () => { const on = rd.getAttribute('aria-pressed') !== 'true'; rd.setAttribute('aria-pressed', on); big.classList.toggle('redrawn', on); grid.classList.toggle('redrawn', on); } }); pills.append(rd);
  fill();

  /* the scale: the base's rim as a ruler on the plate */
  $('#scale-cell').append(cellView('H', { stones: false, frame: true }));
  const flat = (id, sphi) => { const c = TR[id], [W, H] = CROPS[c.img].size, b = c.fig.base, rx = b[2] * W / 100; const P = c.plate.map((p) => [p[0] * W / 100, p[1] * H / 100]); const f = (p) => [(p[0] - P[1][0]) / (2 * rx), -(p[1] - P[1][1]) / sphi / (2 * rx)]; return { L: f(P[0]), R: f(P[2]), fig: f([b[0] * W / 100, b[1] * H / 100]), s: (b[3] * H) / (b[2] * W) }; };
  chart('scale-plan', 250, (svg, w, hgt) => {
    /* drawn square-on: the right edge runs across, the left edge runs up, the near corner at bottom left; an arrow there is the camera's direction */
    const f = flat('H', .31), a = Math.hypot(...f.L), b = Math.hypot(...f.R);
    const det = f.R[0] * f.L[1] - f.R[1] * f.L[0], fx = (f.fig[0] * f.L[1] - f.fig[1] * f.L[0]) / det, fy = (f.R[0] * f.fig[1] - f.R[1] * f.fig[0]) / det;
    const k = Math.min((w - 90) / b, (hgt - 50) / a), ox = (w - b * k) / 2 + 20, oy = hgt - 30;
    const X = (p) => [ox + p[0] * k, oy - p[1] * k];
    svg.append(s('polygon', { points: pts([[0, 0], [0, a], [b, a], [b, 0]].map(X)), fill: 'var(--stone)', stroke: 'var(--stone3)' }));
    svg.append(s('polygon', { points: pts([[0, 0], [0, 1.4], [1.4, 1.4], [1.4, 0]].map(X)), fill: 'none', stroke: 'var(--a-dim)', 'stroke-dasharray': '5 4', 'stroke-width': 1.5 }));
    const c = X([fx * b, fy * a]); svg.append(s('circle', { cx: c[0], cy: c[1], r: k / 2, fill: 'var(--stone2)', stroke: 'var(--a-dim)', 'stroke-width': 2 }), s('text', { x: c[0], y: c[1] + 4, 'text-anchor': 'middle', 'font-size': 11, text: '1' }));
    const lab = (p, t, anchor) => { const q = X(p); svg.append(s('text', { x: q[0], y: q[1], 'text-anchor': anchor, 'font-size': 12, 'font-weight': 600, style: 'fill:var(--a-dim)', text: t })); };
    lab([b / 2, -.25], fmt(b, 1), 'middle'); lab([-.12, a / 2], fmt(a, 1), 'end'); lab([.7, .7], '1.4', 'middle');
    const o = X([0, 0]), d = 26; svg.append(s('line', { x1: o[0] - d, y1: o[1] + d * .6, x2: o[0] - 4, y2: o[1] + 2, stroke: 'var(--fg)', 'stroke-width': 1.5 }), s('polygon', { points: pts([[o[0] - 3, o[1] + 1], [o[0] - 12, o[1] + 1], [o[0] - 8, o[1] + 8]]), style: 'fill:var(--fg)' }), s('text', { x: o[0] - d - 2, y: o[1] + d * .6 + 12, 'text-anchor': 'middle', 'font-size': 11, class: 'mut', text: 'camera' }));
  });
  const rows = [['A–P (16 cells)', AP], ['T1–T20', TT], ['S1–S16', SS]].map(([name, ids]) => { const lo = ids.map((id) => flat(id, .36)), hi2 = ids.map((id) => flat(id, .26)); const m = (a, k) => quant(a.map((q) => Math.hypot(...q[k])), .5); return [name, fmt(quant(ids.map((id) => flat(id, .3).s), .5), 2), fmt(m(lo, 'L'), 1) + '–' + fmt(m(hi2, 'L'), 1), fmt(m(lo, 'R'), 1) + '–' + fmt(m(hi2, 'R'), 1), fmt(quant(ids.map((id) => CELLS[id].figratio), .5), 2)]; });
  $('#scale-table').append(h('tr', {}, ['Sheet', 'Rim: height ÷ width', 'Left edge, diameters', 'Right edge, diameters', 'Figure height ÷ diameter, as seen'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))), ...rows.map((r) => h('tr', {}, r.map((t, i) => h('td', { class: i ? 'num' : null, text: t })))),
    h('tr', {}, h('td', { colspan: 5, html: '<span class="ev m">Measured</span> Medians over each sheet. The two ends of each range lay the plate flat with the camera 21° and 15° above the horizon, the spread the rim allows. A 35 mm square under a 25 mm base would read 1.4.' })));

  /* taxonomy */
  const tax = $('#tax');
  for (const k of CLASSES) { const ids = ALL.filter((id) => CL[id].class === k);
    const what = { pillars: 'Upright stones side by side; the joints that show are upright; tops step.', blocks: 'Squarish stones in tiers; upright and level joints both show.', leaners: 'Slabs or pillars tilted one common way, 10 to 35 degrees.', courses: 'Level slabs or rows of blocks, each set back from the one below: a stair.', gates: 'Cap stones meet over a recess.' }[k];
    tax.append(h('div', { class: 'taxrow' }, h('div', {}, h('h3', { text: CLASS_NAME[k] }), h('p', { html: `<b>${ids.length}</b> of 52 · ${what}` }), h('p', { class: 'ids', text: ids.join(' ') })), h('div', { class: 'row', style: 'grid-template-columns:repeat(auto-fill,minmax(78px,1fr));gap:4px' }, ids.map((id) => cellView(id, { stones: false }))))); }
  scatter('tax-grain', ALL.map((id) => CELLS[id]), { x: [0, 90], y: [0, 1], xl: 'grain, degrees from upright', yl: 'agreement', fx: (c) => Math.abs(c.grain), fy: (c) => c.graincoh, fk: (c) => CL[c.id].class, title: (c) => c.id, r: 4.5, xt: [0, 15, 30, 45, 60, 75, 90] });
  $('#tax-legend').append(legend(CLASSES, CLASS_NAME));
  chart('tax-count', 240, (svg, w, hgt) => {
    const ax = axes(svg, w, hgt, { x: [3.5, 11.5], y: [0, 16], xl: 'major stones in the formation', yl: 'formations', xt: [4, 5, 6, 7, 8, 9, 10, 11] });
    const cols = { smooth: 'var(--a-out)', twenty: 'var(--a-top)', sixteen: 'var(--a-gap)' }; const bw = (ax.x(5) - ax.x(4)) * .8;
    for (let n = 4; n <= 11; n++) { let y0 = 0; for (const sh of ['smooth', 'twenty', 'sixteen']) { const c = ALL.filter((id) => CELLS[id].n === n && CELLS[id].sheet === sh).length; if (!c) continue; svg.append(s('rect', { x: ax.x(n) - bw / 2, y: ax.y(y0 + c), width: bw, height: ax.y(y0) - ax.y(y0 + c) - 1, fill: cols[sh] })); y0 += c; } }
    [['A–P', 'smooth'], ['T', 'twenty'], ['S', 'sixteen']].forEach(([t, k], i) => svg.append(s('rect', { x: w - 120 + i * 40, y: 6, width: 10, height: 10, fill: cols[k] }), s('text', { x: w - 106 + i * 40, y: 15, 'font-size': 11, text: t })));
  });
  const dr = $('#diff-row'); dr.append(cellView('L', { stones: false }), cellView('L', { stones: false, img: 'img/sq-l.jpg', label: 'L textured' }), cellView('P', { stones: false }), cellView('P', { stones: false, img: 'img/sq-p.jpg', label: 'P textured' }));
}
