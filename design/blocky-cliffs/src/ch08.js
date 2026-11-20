/* ---------- chapters 8 and 9 ---------- */
{
  const F = (o) => formMeshes(formation(o)), pic = (m, o = {}) => still(m, Object.assign({ w: 260, h: 230 }, o));
  const M = [
    ['Foot', 'The foot', 'One or two low stones at the right front, against the base of taller ones. The lowest is a wedge; a second, a little higher, may stand behind or beside it, so the foot is itself a stair of two or three.', 'nearly all; a labelled toe in 27', ['H', 'O', 'T9', 'S14'], F({ preset: 'pillars', seed: 31, nA: 2, rows: 3, frontLen: .4, length: 1.1 })],
    ['Body', 'Rising rank', 'Pillars shoulder to shoulder, each top a step above the last, toward the cut end.', '22 (the pillar class)', ['F', 'T6', 'T12', 'S1'], F({ preset: 'pillars', seed: 8, nA: 4, rows: 1, fall: .5, toe: false })],
    ['Body', 'Stair', 'Long slabs or rows of blocks, each set back by about its own height.', '7', ['E', 'I', 'T14', 'S11'], F({ preset: 'courses', seed: 5 })],
    ['Body', 'Tiers', 'Squarish blocks in two or three levels, joints staggered.', '10', ['O', 'T4', 'T7', 'S5'], F({ preset: 'blocks', seed: 4 })],
    ['Body', 'Sheaf', 'Parallel leaners, each behind and above the last.', '10', ['P', 'L', 'T9', 'S14'], F({ preset: 'leaners', seed: 6 })],
    ['Body', 'Tower with skirts', 'One stone far taller than the rest, lower stones gathered round its foot on three sides.', '5', ['C', 'T5', 'T17', 'S13'], F({ preset: 'pillars', seed: 13, nA: 3, rows: 3, peak: .35, fall: .75, length: 1.25 })],
    ['Body', 'Twin with a cleft', 'Two tall stones with an open joint between them, as tall as they are.', '4', ['H', 'S6', 'T13', 'T19'], (() => { const f = formation({ preset: 'pillars', seed: 3, nA: 2, rows: 2, length: 1.05, fall: .15, taper: .9 }); return formMeshes(f); })()],
    ['Head', 'Head on shoulders', 'A cap on one or two stones. It may jut toward the figure, or two caps may meet over a recess.', '17 with a cap; 3 gates', ['B', 'K', 'S9', 'T15'], F({ preset: 'gates', seed: 8 })]];
  const box = $('#motifs');
  for (const [part, name, what, count, cells, mesh] of M) box.append(h('div', { class: 'catrow' }, h('div', { class: 'words' }, h('h3', { text: name }), h('p', { html: `<span class="ev e">Estimated</span> ${part} · ${count} of 52` }), h('p', { text: what }), h('p', { class: 'num', text: cells.join(' ') })), h('div', { class: 'row', style: 'grid-template-columns:repeat(5,minmax(0,1fr))' }, pic(mesh), ...cells.map((c) => cellView(c, { stones: false })))));
  const first = ALL.filter((id) => { const st = STONES.find((x) => x.cell === id); return st.topy < .5 * CELLS[id].H; }).length; $('#foot-n').textContent = first;
  const tb = $('#combo'); tb.append(h('tr', {}, ['Class', 'Formations', 'With a labelled toe', 'With a cap', 'With an overhang', 'With a cleft or recess'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))));
  for (const k of CLASSES) { const ids = ALL.filter((id) => CL[id].class === k); tb.append(h('tr', {}, h('td', { text: CLASS_NAME[k] }), ...[ids.length, ids.filter((id) => CELLS[id].kinds.toe).length, ids.filter((id) => CELLS[id].kinds.cap).length, ids.filter((id) => CL[id].overhang).length, ids.filter((id) => CL[id].cleft).length].map((n) => h('td', { class: 'num', text: n })))); }

  let cls = 'pillars', pull = .22; const A = $('#hier-a'), B = $('#hier-b');
  const draw = () => { const f = formation({ preset: cls, seed: 12, rows: cls === 'courses' ? 4 : 3, nA: cls === 'courses' ? 1 : 3 }), ms = formMeshes(f), n = f.rows.length;
    const grey = (st) => { const t = n > 1 ? st.o.col.row.i / (n - 1) : 1, v = Math.round(lerp(0xd2, 0x9a, t)); return (v << 16) | (v << 8) | (v + 4); };
    A.replaceChildren(still(ms, { w: 400, h: 360, color: grey, shift: (st) => [0, -(n - 1 - st.o.col.row.i) * pull, 0], centre: [PLATE / 2, PLATE / 2 - pull * .8, .7], fov: 16 }));
    B.replaceChildren(still(ms, { w: 400, h: 360, view: 'top', color: grey, centre: [PLATE / 2, PLATE / 2, 0], span: 2 })); };
  $('#hier-x').addEventListener('input', (e) => { pull = +e.target.value; draw(); });
  const pills = $('#hier-cls'); for (const k of ['pillars', 'blocks', 'courses', 'leaners']) pills.append(h('button', { type: 'button', 'aria-pressed': k === cls, text: CLASS_NAME[k], onclick: (e) => { cls = k; for (const b of pills.children) b.setAttribute('aria-pressed', b === e.target); draw(); } }));
  draw();
}
