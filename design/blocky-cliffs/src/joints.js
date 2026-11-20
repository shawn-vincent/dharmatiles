/* ---------- joints and fusion ---------- */
{
  const JN = await fetch('data/joints.json').then((r) => { if (!r.ok) throw new Error('data/joints.json: ' + r.status); return r.json(); });
  const J = JN.joints, byCell = {}; for (const j of J) (byCell[j.cell] = byCell[j.cell] || []).push(j);
  const dying = (j) => j.state === 'one' || j.state === 'both', notOpen = (j) => j.state !== 'open';
  const pct = (a, b) => b ? Math.round(100 * a / b) + '%' : '–';
  const STATE = { open: 'open end to end', one: 'dies at one end', both: 'dies at both ends', fused: 'no line at all' };

  /* the joints of a cell drawn over it: an open joint in the joint colour, a dying joint's line in the axis colour, the stretch with no line in the level colour, dashed */
  const layer = (id, o = {}) => (svg) => {
    for (const j of byCell[id] || []) {
      if (o.only && !o.only(j)) continue;
      const g = s('g', { 'data-joint': j.id, class: 'jn ' + (j.state === 'open' ? 'op' : 'dy') });
      for (const p of j.parts) g.append(s('polyline', { points: pts(p.p), class: p.s }));
      if (j.state !== 'open') for (let i = 0; i < j.parts.length - 1; i++) if ((j.parts[i].s === 'f') !== (j.parts[i + 1].s === 'f')) { const q = j.parts[i].p[j.parts[i].p.length - 1]; g.append(s('circle', { cx: q[0], cy: q[1], r: o.dot || 1.3, class: 'end' })); }
      g.append(s('title', { text: `${j.id}: ${j.kind}, ${STATE[j.state]}` })); svg.append(g);
    }
  };
  const plate = (id, o = {}) => cellView(id, Object.assign({ stones: false, extra: layer(id, o) }, o));
  $('#jn-legend').append(h('div', { class: 'legend' },
    [['op', 'v', 'a line from end to end'], ['op', 'w', 'the same, faint'], ['dy', 'v', 'the line of a joint that dies'], ['dy', 'f', 'no line: one surface']].map(([a, b, t]) => h('span', {}, s('svg', { width: 34, height: 12, viewBox: '0 0 34 12' }, s('g', { class: 'jn ' + a }, s('polyline', { points: '2,6 32,6', class: b }))), t))));

  /* headline numbers */
  const n = J.length, cnt = (f) => J.filter(f).length, set = (k, v) => { for (const e of document.querySelectorAll(`[data-jn="${k}"]`)) e.textContent = v; };
  const len = (f) => J.filter(f).reduce((a, j) => a + j.len, 0), merged = J.reduce((a, j) => a + j.len * (1 - j.shown), 0);
  set('n', n); set('open', cnt((j) => j.state === 'open')); set('one', cnt((j) => j.state === 'one')); set('both', cnt((j) => j.state === 'both')); set('fused', cnt((j) => j.state === 'fused'));
  set('openp', pct(cnt((j) => j.state === 'open'), n)); set('dyp', pct(cnt(dying), n)); set('dy', cnt(dying));
  set('cells', new Set(J.filter(notOpen).map((j) => j.cell)).size); set('mergedlen', pct(merged, len(() => true)));
  set('sure', cnt((j) => j.conf !== 'low')); set('suredy', cnt((j) => j.conf !== 'low' && notOpen(j))); set('surep', pct(cnt((j) => j.conf !== 'low' && notOpen(j)), cnt((j) => j.conf !== 'low')));
  set('inner', cnt((j) => /[a-z]$/.test(j.a))); set('faintopen', pct(cnt((j) => j.state === 'open' && j.faint > 0), cnt((j) => j.state === 'open')));
  const agree = cnt((j) => (j.state === 'open') === (j.mshown >= .85)); set('agree', pct(agree, n)); set('eyeopen-mno', cnt((j) => j.state === 'open' && j.mshown < .85)); set('eyedy-myes', cnt((j) => j.state !== 'open' && j.mshown >= .85));
  chart('jn-bar', 96, (svg, w) => { const x0 = 10, x1 = w - 10; let x = x0;
    for (const [k, col, lab] of [['open', 'var(--a-gap)', 'open'], ['one', 'var(--a-ax)', 'one end'], ['both', 'var(--a-ax)', 'both'], ['fused', 'var(--a-top)', 'fused']]) { const c = cnt((j) => j.state === k), wd = (x1 - x0) * c / n; svg.append(s('rect', { x, y: 30, width: Math.max(1, wd - 1), height: 30, fill: col, 'fill-opacity': k === 'both' ? .55 : 1 })); if (wd > 46) svg.append(s('text', { x: x + 4, y: 22, 'font-size': 12, text: `${lab} ${c}` })); x += wd; }
    svg.append(s('text', { x: x1, y: 78, 'text-anchor': 'end', 'font-size': 12, class: 'mut', text: `dies at both ends ${cnt((j) => j.state === 'both')} · no line at all ${cnt((j) => j.state === 'fused')}` })); });

  /* look at any cell */
  const sheets = [['A–P', AP], ['T1–T20', TT], ['S1–S16', SS]]; let sheet = 2, cur = 'S1';
  const big = $('#jn-big'), table = $('#jn-table'), grid = $('#jn-grid'), note = $('#jn-note');
  const hi = (jid) => { for (const g of big.querySelectorAll('.jn')) g.classList.toggle('on', g.dataset.joint === jid); for (const r of table.querySelectorAll('tr[data-joint]')) r.classList.toggle('on', r.dataset.joint === jid); };
  const show = (id) => { cur = id; const js = byCell[id] || [];
    big.replaceChildren(plate(id, { dot: 1.1 })); for (const g of big.querySelectorAll('.jn')) g.addEventListener('pointerenter', () => hi(g.dataset.joint));
    note.innerHTML = `<b>${id}</b> · ${CLASS_NAME[CL[id].class]} · ${js.length} contacts, ${js.filter(notOpen).length} not open end to end.`;
    table.replaceChildren(h('tr', {}, ['Between', 'Kind', 'State', 'Line shows', 'Dies at', 'Fades', 'Two-stone outline', 'Sure', 'Note'].map((t, i) => h('th', { class: i === 3 ? 'num' : null, text: t }))),
      ...js.map((j) => h('tr', { 'data-joint': j.id, onpointerenter: () => hi(j.id) }, h('td', { text: j.a.slice(id.length) + ' | ' + j.b.slice(id.length) }), h('td', { text: j.kind }), h('td', { class: j.state === 'open' ? 'no' : 'yes', text: STATE[j.state] }), h('td', { class: 'num', text: Math.round(j.shown * 100) + '%' }), h('td', { text: j.dies || '' }), h('td', { text: j.fade || '' }), h('td', { text: j.two == null ? '' : j.two ? 'yes' : 'no' }), h('td', { text: j.conf }), h('td', { text: j.note }))));
    for (const d of grid.children) d.classList.toggle('sel', d.dataset.cell === id); };
  const fill = () => { const names = sheets[sheet][1]; grid.replaceChildren(...names.map((id) => plate(id, { onpick: show, dot: 1.6 }))); if (!names.includes(cur)) cur = names[0]; show(cur); };
  const pills = $('#jn-sheet'); sheets.forEach(([name], i) => pills.append(h('button', { type: 'button', 'aria-pressed': i === sheet, text: name, onclick: (e) => { sheet = i; for (const b of pills.children) b.setAttribute('aria-pressed', b === e.target); fill(); } }))); fill();

  /* plates: the cells where joints die */
  for (const id of ['S1', 'S2', 'S4', 'S6', 'T8', 'T3', 'T12', 'E', 'C', 'S5', 'S12', 'S15']) $('#jn-plates').append(plate(id, { dot: 1.5 }));
  for (const id of ['B', 'M', 'O', 'S9', 'T7', 'T14', 'S14', 'K']) $('#jn-open').append(plate(id, { dot: 1.5 }));

  /* close-ups: the crop plain, the same crop with the joint drawn, and the measured strength of the line along the contact */
  const zoom = (id, box, draw) => { const [W, H] = CROPS[TR[id].img].size, [x0, y0, x1, y1] = box;
    const svg = s('svg', { viewBox: `${x0} ${y0} ${x1 - x0} ${y1 - y0}`, preserveAspectRatio: 'none', style: `display:block;width:100%;aspect-ratio:${(x1 - x0) * W}/${(y1 - y0) * H}` }, s('image', { href: imgOf(id), x: 0, y: 0, width: 100, height: 100, preserveAspectRatio: 'none' }));
    if (draw) draw(svg); return h('div', { class: 'cell stage' }, svg); };
  const strip = (j) => { const w = 16 * 9, svg = s('svg', { viewBox: `0 0 ${w} 34`, class: 'jstrip', role: 'img', 'aria-label': 'Measured line strength along the contact' }); const k = (v) => Math.min(v, 6) / 6 * 28;
    j.prof.forEach((v, i) => svg.append(s('rect', { x: i * 9 + 1, y: 32 - k(v), width: 7, height: Math.max(.6, k(v)), fill: v >= 1.2 ? 'var(--a-ax)' : 'var(--a-top)' })));
    svg.append(s('line', { x1: 0, x2: w, y1: 32 - k(1.2), y2: 32 - k(1.2), stroke: 'var(--fg)', 'stroke-width': .6, 'stroke-dasharray': '3 2' })); return svg; };
  const close = [['S1', 'S1-j2', [38, 10, 58, 82], 'Two pillars, one foot. The groove runs from the skyline to a dimple at half height; below it there is one face to the plate.'],
    ['S4', 'S4-j2', [40, 10, 60, 82], 'A crooked crack from the skyline to a notch under shoulder height, then nothing.'],
    ['T8', 'T8-j3', [50, 30, 70, 84], 'A crack from the head of a slab that runs a third of the way down its face.'],
    ['T3', 'T3-j7', [60, 12, 80, 86], 'A crack from the skyline, fainter down the slab, gone before the foot.'],
    ['T12', 'T12-j3', [62, 8, 80, 52], 'A skyline crack that fades out a third of the way down a block.'],
    ['S11', 'S11-j9', [54, 54, 78, 90], 'Between two front blocks: a crease that widens, softens and is gone before the plate.'],
    ['E', 'E-j1', [56, 42, 98, 76], 'Crisp along the front of the stair. Round the corner, on the cut end, the two courses are one face.'],
    ['S12', 'S12-j4', [40, 30, 70, 50], 'A bed that is crisp by the pillar and gone toward the far end.'],
    ['S15', 'S15-j1', [60, 2, 92, 28], 'The one contact with no line at all: the top block runs into the slab behind it through a rounded neck.']];
  for (const [id, jid, box, text] of close) { const j = J.find((q) => q.id === jid);
    $('#jn-close').append(h('figure', { class: 'jclose' }, h('div', { class: 'pair' }, zoom(id, box), zoom(id, box, (svg) => layer(id, { only: (q) => q.id === jid, dot: .7 })(svg))), strip(j),
      h('figcaption', { html: `<b>${id}</b> ${text} <span class="mut">Line shows on ${Math.round(j.shown * 100)}% of the contact.</span>` }))); }

  /* tables of measures */
  const tab = (target, head, groups) => { const t = $(target);
    t.append(h('tr', {}, [head, 'Contacts', 'Open end to end', 'Dies at one end', 'Dies at both', 'No line', 'Not open'].map((x, i) => h('th', { class: i ? 'num' : null, text: x }))),
      ...groups.map(([name, f]) => { const g = J.filter(f), c = (k) => g.filter((j) => j.state === k).length; return h('tr', {}, h('td', { html: name }), h('td', { class: 'num', text: g.length }), h('td', { class: 'num', text: pct(c('open'), g.length) }), h('td', { class: 'num', text: c('one') }), h('td', { class: 'num', text: c('both') }), h('td', { class: 'num', text: c('fused') }), h('td', { class: 'num', html: `<b>${pct(g.filter(notOpen).length, g.length)}</b>` })); })); };
  const KN = ['column–column', 'course–course', 'leaner–leaner', 'cap–support', 'other'], KL = { 'column–column': 'Column–column: an upright joint between neighbours', 'course–course': 'Course–course: a bed between courses, or across a pillar', 'leaner–leaner': 'Leaner–leaner', 'cap–support': 'Cap–support: the bed under a resting stone', other: 'Other: a front stone or toe against what stands behind' };
  tab('#jn-kind', 'By kind', KN.map((k) => [KL[k], (j) => j.kind === k]));
  tab('#jn-class', 'By class of formation', CLASSES.map((k) => [CLASS_NAME[k], (j) => CL[j.cell].class === k]));
  const sh = (j) => TR[j.cell].sheet;
  tab('#jn-pos', 'By place on the formation', [['Front: reaches the plate, or in the lowest two fifths', (j) => j.rank !== 'back'], ['Back: higher up', (j) => j.rank === 'back'],
    ['Far third of the length', (j) => j.along <= 1 / 3], ['Middle third', (j) => j.along > 1 / 3 && j.along <= 2 / 3], ['Third by the cut end', (j) => j.along > 2 / 3],
    ['Lowest third of the height', (j) => j.up <= 1 / 3], ['Middle third', (j) => j.up > 1 / 3 && j.up <= 2 / 3], ['Top third', (j) => j.up > 2 / 3],
    ['Column–column, front, far third', (j) => j.kind === 'column–column' && j.rank !== 'back' && j.along <= 1 / 3], ['Column–column, back, by the cut end', (j) => j.kind === 'column–column' && j.rank === 'back' && j.along > 2 / 3]]);
  tab('#jn-sheetb', 'By sheet and by how it was found', [['A–P', (j) => sh(j) === 'smooth'], ['T1–T20', (j) => sh(j) === 'twenty'], ['S1–S16', (j) => sh(j) === 'sixteen'],
    ['Between two traced stones', (j) => !/[a-z]$/.test(j.a)], ['A crack inside one traced mass', (j) => /[a-z]$/.test(j.a)], ['Called with high or medium confidence', (j) => j.conf !== 'low'], ['Called with low confidence', (j) => j.conf === 'low']]);

  /* how far a dying line runs, where it dies, how it fades */
  const D = J.filter(dying);
  hist('jn-shown', D.map((j) => j.shown), { x: [0, 1], bins: 10, xl: 'share of the contact on which a line shows', yl: 'joints', h: 210, fill: 'var(--a-ax)', prop: [.3, .75] });
  const count = (target, rows, total) => chart(target, rows.length * 26 + 16, (svg, w) => { const l = 150, mx = Math.max(...rows.map((r) => r[1]));
    rows.forEach(([name, c, col], i) => { const y = 8 + i * 26; svg.append(s('text', { x: 0, y: y + 14, 'font-size': 12.5, text: name }), s('rect', { x: l, y: y + 2, width: Math.max(1, (w - l - 70) * c / mx), height: 16, fill: col || 'var(--a-ax)' }), s('text', { x: l + (w - l - 70) * c / mx + 6, y: y + 14, 'font-size': 12, class: 'mut', text: `${c} · ${pct(c, total)}` })); }); });
  const by = (f, order) => order.map((k) => [k[1], D.filter((j) => f(j) === k[0]).length, k[2]]);
  count('jn-dies', by((j) => j.dies, [['foot', 'toward the foot'], ['mid-face', 'part way across a face'], ['top', 'toward the top'], ['back', 'toward the back'], ['cut end', 'at the cut end']]), D.length);
  count('jn-fade', [...by((j) => j.fade, [['gradual', 'fades gradually'], ['stops', 'stops, often in a notch']]), ...by((j) => j.two, [[true, 'outline still shows two', 'var(--a-top)'], [false, 'outline shows one', 'var(--a-top)']])], D.length);
  statTable('jn-lens', [['Line shows, share of the contact', D.map((j) => j.shown)], ['Stretch with no line, fh', D.map((j) => j.len * (1 - j.shown))], ['Faint stretch before it, where the line fades gradually, fh', D.filter((j) => j.fade === 'gradual').map((j) => j.len * j.faint)], ['Whole contact of a joint that dies, fh', D.map((j) => j.len)], ['Whole contact of an open joint, fh', J.filter((j) => j.state === 'open').map((j) => j.len)]]);
  /* the line's measured strength down upright joints that reach the plate */
  const G = J.filter((j) => j.ends[1] === 'ground' && (j.kind === 'column–column' || j.kind === 'leaner–leaner')), GO = G.filter((j) => j.state === 'open');
  set('g-n', G.length); set('g-open', GO.length); set('g-faint', GO.filter((j) => j.parts[j.parts.length - 1].s === 'w').length);
  const mean = (g, i) => g.reduce((a, j) => a + Math.min(j.prof[i], 6), 0) / g.length; set('g-ratio', fmt((mean(GO, 13) + mean(GO, 14) + mean(GO, 15)) / (mean(GO, 0) + mean(GO, 1) + mean(GO, 2)), 2));
  chart('jn-down', 220, (svg, w, hgt) => { const ax = axes(svg, w, hgt, { x: [0, 1], y: [0, 6], xl: 'along the joint: head at 0, the plate at 1', yl: 'line strength', xt: [0, .25, .5, .75, 1] });
    svg.append(s('rect', { x: ax.x(0), y: ax.y(1.2), width: ax.x(1) - ax.x(0), height: ax.y(0) - ax.y(1.2), fill: 'var(--a-top)', opacity: .18 }));
    for (const [g, col, dash] of [[GO, 'var(--a-gap)', null], [G.filter(dying), 'var(--a-ax)', null]]) svg.append(s('polyline', { points: pts(g[0].prof.map((_, i) => [ax.x((i + .5) / 16), ax.y(mean(g, i))])), fill: 'none', stroke: col, 'stroke-width': 2.5, 'stroke-dasharray': dash }));
    svg.append(s('text', { x: ax.x(.03), y: ax.y(5.55), 'font-size': 11.5, style: 'fill:var(--a-gap)', text: `open joints (${GO.length})` }), s('text', { x: ax.x(.03), y: ax.y(.25), 'font-size': 11.5, style: 'fill:var(--a-ax)', text: `joints that die (${G.filter(dying).length})` })); });

}
