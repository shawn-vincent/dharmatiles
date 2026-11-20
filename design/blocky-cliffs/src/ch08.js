/* ---------- chapters 8 and 9 ---------- */
{
  const F = (o) => formMeshes(formation(o)), pic = (m, o = {}) => still(m, Object.assign({ w: 260, h: 230 }, o));
  const M = [
    ['Foot', 'The foot', 'One or two low stones at the right front, against the base of taller ones. The lowest is a wedge; a second, a little higher, may stand behind or beside it, so the foot is itself a stair of two or three.', 'nearly all; a labelled toe in 27', ['H', 'O', 'T9', 'S14'], F({ preset: 'pillars', seed: 31, nA: 2, rows: 3, frontLen: .4, length: 1.1 })],
    ['Body', 'Rising rank', 'Pillars shoulder to shoulder, tops stepping up toward the cut end, unequal in width.', 'most of the 22 pillar formations', ['F', 'T6', 'T1', 'S1'], F({ seed: 13, cols: 3, front: 2, fall: .6, base: 1, pile: 0 })],
    ['Body', 'Plateau wall', 'Two to four broad stones sharing one top plane, level or nearly so as seen; the far end almost as high as the peak.', '6', ['H', 'T10', 'T12', 'T20'], F({ seed: 46, cols: 3, front: 2, dip: 7, fall: .15, step: .05, base: .9, pile: 0 })],
    ['Body', 'Stair', 'Long slabs or rows of blocks, each set back by about its own height.', '7', ['E', 'I', 'T14', 'S11'], F({ seed: 5, base: .3, cols: 2, pile: 5, depth: 1.1, setback: .2, front: 0, peak: 0, fall: .2, height: 1.6, length: 1.5, stack: .3, bond: .6 })],
    ['Body', 'Tiers', 'Squarish blocks in two to four levels, each level near one height, joints staggered, each tier shorter than the one below.', '10', ['O', 'T4', 'T7', 'S5'], F({ seed: 40, base: .55, cols: 3, pile: 4, depth: .7, setback: .08, step: .13, front: 2, stack: .4, bond: .5 })],
    ['Body', 'Sheaf', 'Leaners side by side, fanning, with an upright stone at the cut end and upright stones in front.', '10', ['P', 'L', 'T9', 'S14'], F({ seed: 39, lean: 20, cols: 4, spread: .12, base: .72, fall: .45, peak: 0, depth: .6, pile: 1, setback: 0, front: 2, height: 1.6, step: .1 })],
    ['Body', 'Tower with skirts', 'One stone far taller than the rest, lower stones gathered round its foot on three sides.', '5', ['C', 'T5', 'T17', 'S13'], F({ seed: 13, cols: 3, front: 3, peak: .35, fall: .75, length: 1.25, base: .95, pile: 0 })],
    ['Body', 'Twin with a cleft', 'Two tall stones with an open joint between them, as tall as they are. (The generator draws the two stones; it does not open the joint.)', '4', ['H', 'S6', 'T13', 'T19'], F({ seed: 3, cols: 2, front: 2, length: 1.05, fall: .2, base: .95, pile: 0 })],
    ['Head', 'Resting stone', 'A smaller block or a thin slab set back on the tops, usually across a joint.', 'about 12', ['A', 'E', 'D', 'T12'], F({ seed: 5, cols: 2, front: 2, base: .65, pile: 1 })],
    ['Head', 'Oversail', 'An upper block standing forward of a set-back wall, as often at the far end as by the peak. (The generator lays blocks on tops; it does not set a wall back under one, except under a gate\u2019s lintels.)', '7', ['D', 'O', 'S3', 'S10'], F({ seed: 9, cols: 3, front: 2, base: .6, pile: 2, setback: 0, fall: .3 })],
    ['Head', 'Gate', 'Caps from two unequal piers meet over a set-back wall.', '2 (T15, S9); K and J are a shelter and a niche', ['T15', 'S9', 'K', 'J'], F({ seed: 26, opening: .34, cols: 3, base: .66, height: 1.45, depth: .72, fall: .3, peak: .15, pile: 2, bond: 1, front: 2, setback: 0 })]];
  const box = $('#motifs');
  for (const [part, name, what, count, cells, mesh] of M) box.append(h('div', { class: 'catrow' }, h('div', { class: 'words' }, h('h3', { text: name }), h('p', { html: `<span class="ev e">Estimated</span> ${part} · ${count} of 52` }), h('p', { text: what }), h('p', { class: 'num', text: cells.join(' ') })), h('div', { class: 'row', style: 'grid-template-columns:repeat(5,minmax(0,1fr))' }, pic(mesh), ...cells.map((c) => cellView(c, { stones: false })))));
  $('#foot-n').textContent = ALL.filter((id) => CELLS[id].footshare < .5).length;
  const tb = $('#combo'); tb.append(h('tr', {}, ['Class', 'Formations', 'With a labelled toe', 'With a cap', 'With an overhang', 'With a cleft or recess'].map((t, i) => h('th', { class: i ? 'num' : null, text: t }))));
  for (const k of CLASSES) { const ids = ALL.filter((id) => CL[id].class === k); tb.append(h('tr', {}, h('td', { text: CLASS_NAME[k] }), ...[ids.length, ids.filter((id) => CELLS[id].kinds.toe).length, ids.filter((id) => CELLS[id].kinds.cap).length, ids.filter((id) => CL[id].overhang).length, ids.filter((id) => CL[id].cleft).length].map((n) => h('td', { class: 'num', text: n })))); }

  let fam = 'columns', pull = .22; const A = $('#hier-a'), B = $('#hier-b');
  const draw = () => { const f = formation(drawParams(12, fam)), ms = formMeshes(f, true);
    const grey = (st) => { const v = Math.round(lerp(0xd2, 0x9a, st.o.rank / 2)); return (v << 16) | (v << 8) | (v + 4); };
    A.replaceChildren(still(ms, { w: 400, h: 360, color: grey, shift: (st) => [0, -(2 - st.o.rank) * pull, 0], centre: [PLATE / 2, PLATE / 2 - pull * .8, .7], fov: 16 }));
    B.replaceChildren(still(ms, { w: 400, h: 360, view: 'top', color: grey, centre: [PLATE / 2, PLATE / 2, 0], span: 2 })); };
  $('#hier-x').addEventListener('input', (e) => { pull = +e.target.value; draw(); });
  const pills = $('#hier-cls'); for (const k of FAMILIES) pills.append(h('button', { type: 'button', 'aria-pressed': k === fam, text: k[0].toUpperCase() + k.slice(1), onclick: (e) => { fam = k; for (const b of pills.children) b.setAttribute('aria-pressed', b === e.target); draw(); } }));
  draw();
}
