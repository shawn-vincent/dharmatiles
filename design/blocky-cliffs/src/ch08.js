/* ---------- chapters 8 and 9 ---------- */
{
  const F = (o) => formMeshes(formation(o)), pic = (m, o = {}) => still(m, Object.assign({ w: 260, h: 230 }, o));
  const M = [
    ['Foot', 'The foot', 'One or two low stones at the right front, against the base of taller ones. The lowest is a wedge; a second, a little higher, may stand behind or beside it, so the foot is itself a stair of two or three.', 'nearly all; a labelled toe in 27', ['H', 'O', 'T9', 'S14'], F({ preset: 'pillars', seed: 31, nA: 2, rows: 3, frontLen: .4, length: 1.1 })],
    ['Body', 'Rising rank', 'Pillars shoulder to shoulder, tops stepping up toward the cut end, unequal in width.', 'most of the 22 pillar formations', ['F', 'T6', 'T1', 'S1'], F({ preset: 'pillars', seed: 13, nA: 3, rows: 2, fall: .6 })],
    ['Body', 'Plateau wall', 'Two to four broad stones sharing one top plane, level or nearly so as seen; the far end almost as high as the peak.', '6', ['H', 'T10', 'T12', 'T20'], F({ preset: 'wall', seed: 46, nA: 3, rows: 2 })],
    ['Body', 'Stair', 'Long slabs or rows of blocks, each set back by about its own height.', '7', ['E', 'I', 'T14', 'S11'], F({ preset: 'courses', seed: 5 })],
    ['Body', 'Tiers', 'Squarish blocks in two to four levels, each level near one height, joints staggered, each tier shorter than the one below.', '10', ['O', 'T4', 'T7', 'S5'], F({ preset: 'blocks', seed: 40, tiers: 3 })],
    ['Body', 'Sheaf', 'Leaners side by side, fanning, with an upright stone at the cut end and upright stones in front.', '10', ['P', 'L', 'T9', 'S14'], F({ preset: 'leaners', seed: 39 })],
    ['Body', 'Tower with skirts', 'One stone far taller than the rest, lower stones gathered round its foot on three sides.', '5', ['C', 'T5', 'T17', 'S13'], F({ preset: 'pillars', seed: 13, nA: 3, rows: 3, peak: .35, fall: .75, length: 1.25 })],
    ['Body', 'Twin with a cleft', 'Two tall stones with an open joint between them, as tall as they are.', '4', ['H', 'S6', 'T13', 'T19'], F({ preset: 'pillars', seed: 3, nA: 2, rows: 2, length: 1.05, fall: .2, cleft: 1 })],
    ['Head', 'Resting stone', 'A smaller block or a thin slab set back on the tops, usually across a joint.', 'about 12', ['A', 'E', 'D', 'T12'], F({ preset: 'wall', seed: 5, nA: 2, rows: 2, rest: 1 })],
    ['Head', 'Oversail', 'An upper block standing forward of a set-back wall, as often at the far end as by the peak.', '7', ['D', 'O', 'S3', 'S10'], F({ preset: 'pillars', seed: 9, nA: 3, rows: 2, oversail: 1, fall: .3 })],
    ['Head', 'Gate', 'Caps from two unequal piers meet over a set-back wall.', '2 (T15, S9); K and J are a shelter and a niche', ['T15', 'S9', 'K', 'J'], F({ preset: 'gates', seed: 26 })]];
  const box = $('#motifs');
  for (const [part, name, what, count, cells, mesh] of M) box.append(h('div', { class: 'catrow' }, h('div', { class: 'words' }, h('h3', { text: name }), h('p', { html: `<span class="ev e">Estimated</span> ${part} · ${count} of 52` }), h('p', { text: what }), h('p', { class: 'num', text: cells.join(' ') })), h('div', { class: 'row', style: 'grid-template-columns:repeat(5,minmax(0,1fr))' }, pic(mesh), ...cells.map((c) => cellView(c, { stones: false })))));
  $('#foot-n').textContent = ALL.filter((id) => CELLS[id].footshare < .5).length;
  const tb = $('#combo'); tb.append(h('tr', {}, ['Class', 'Formations', 'With a labelled toe', 'With a cap', 'With an overhang', 'With a cleft or recess'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))));
  for (const k of CLASSES) { const ids = ALL.filter((id) => CL[id].class === k); tb.append(h('tr', {}, h('td', { text: CLASS_NAME[k] }), ...[ids.length, ids.filter((id) => CELLS[id].kinds.toe).length, ids.filter((id) => CELLS[id].kinds.cap).length, ids.filter((id) => CL[id].overhang).length, ids.filter((id) => CL[id].cleft).length].map((n) => h('td', { class: 'num', text: n })))); }

  let cls = 'pillars', pull = .22; const A = $('#hier-a'), B = $('#hier-b');
  const draw = () => { const f = formation({ preset: cls, seed: 12, rows: cls === 'courses' ? 4 : 3, nA: cls === 'courses' ? 1 : 3, tiers: 0 }), ms = formMeshes(f), n = f.rows.length;
    const grey = (st) => { const t = n > 1 ? st.o.col.row.i / (n - 1) : 1, v = Math.round(lerp(0xd2, 0x9a, t)); return (v << 16) | (v << 8) | (v + 4); };
    A.replaceChildren(still(ms, { w: 400, h: 360, color: grey, shift: (st) => [0, -(n - 1 - st.o.col.row.i) * pull, 0], centre: [PLATE / 2, PLATE / 2 - pull * .8, .7], fov: 16 }));
    B.replaceChildren(still(ms, { w: 400, h: 360, view: 'top', color: grey, centre: [PLATE / 2, PLATE / 2, 0], span: 2 })); };
  $('#hier-x').addEventListener('input', (e) => { pull = +e.target.value; draw(); });
  const pills = $('#hier-cls'); for (const k of ['pillars', 'blocks', 'courses', 'leaners']) pills.append(h('button', { type: 'button', 'aria-pressed': k === cls, text: CLASS_NAME[k], onclick: (e) => { cls = k; for (const b of pills.children) b.setAttribute('aria-pressed', b === e.target); draw(); } }));
  draw();
}
