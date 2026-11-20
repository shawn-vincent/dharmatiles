/* ---------- chapter 7 ---------- */
{
  const mini = (list, sub, o = {}) => still(list.map(stoneMesh), Object.assign({ w: 300, h: 260, plateSize: [1.5, 1.2], centre: [.75, .75, .55], fov: 11, sub }, o));
  const B = (o) => Object.assign({ bevel: .17, taperA: .95, taperD: .96, jit: .02, seed: 3 }, o), dB = 1.2;
  const st = (a0, w, d0, dp, h2, o = {}) => B(Object.assign({ w, dp, h: h2, a: a0 + w / 2, d: dB - d0 - dp / 2, z: 0 }, o));
  $('#n-vocab').append(
    mini([st(.25, .5, 0, .5, 1.15), st(.75, .48, 0, .5, .8, { seed: 5 })], '<b>1 Abut.</b> Upright joint, tops apart.'),
    mini([st(.3, .6, 0, .45, 1.2), st(.25, .5, .45, .32, .42, { seed: 8, taperA: .8, taperD: .85 })], '<b>2 Stand in front.</b> The lower one nearer.'),
    mini([st(.3, .7, 0, .55, .62), st(.3, .58, 0, .45, .5, { z: .62, bevelB: .15, seed: 9 })], '<b>3 Rest on.</b> Level joint, upper set back.'),
    mini([st(.15, .34, 0, .45, .78), st(.95, .36, 0, .45, .78, { seed: 4 }), st(.5, .44, 0, .2, .74, { seed: 6 }), st(.08, .68, 0, .55, .42, { z: .78, bevelB: .08, seed: 10, d: dB - .3 }), st(.76, .62, 0, .5, .34, { z: .78, bevelB: .08, seed: 12, d: dB - .27 })], '<b>4 Bridge.</b> Two caps meet over a recess with a back wall.'),
    mini([st(.4, .6, 0, .3, .7), st(.4, .6, 0, .56, .6, { z: .7, bevelB: .08, seed: 14 })], '<b>5 Oversail.</b> An upper block stands forward of a set-back wall.'),
    mini([st(.25, .45, 0, .45, 1.2), st(.78, .42, 0, .42, 1.0, { seed: 15, leanA: -.22 })], '<b>6 Prop.</b> A leaner against an upright.'));
  const side = PAIRS.filter((p) => !p.stacked);
  hist('n-dtop', side.map((p) => p.dtop), { x: [0, 1.5], bins: 25, xl: 'difference in top level, fh', yl: 'pairs', prop: [.12, 1.5], h: 230 });
  const dec = side.filter((p) => p.lowfront_foot != null), big = dec.filter((p) => p.dtop > .3); $('#n-front-big').textContent = Math.round(100 * big.filter((p) => p.lowfront_foot).length / big.length) + ' percent of ' + big.length;
  chart('n-front', 120, (svg, w) => { const k = dec.filter((p) => p.lowfront_foot).length, f = k / dec.length, x0 = 10, x1 = w - 10;
    svg.append(s('rect', { x: x0, y: 34, width: (x1 - x0) * f, height: 34, fill: 'var(--a-out)' }), s('rect', { x: x0 + (x1 - x0) * f, y: 34, width: (x1 - x0) * (1 - f), height: 34, fill: 'var(--stone2)' }),
      s('text', { x: x0, y: 24, 'font-size': 12.5, text: `lower stone nearer: ${Math.round(f * 100)}% (${k} of ${dec.length})` }), s('text', { x: x1, y: 88, 'font-size': 12.5, 'text-anchor': 'end', class: 'mut', text: `taller stone nearer: ${Math.round((1 - f) * 100)}%` })); });
  for (const [id, a, b] of [['H', 'H3', 'H4'], ['T12', null, null], ['T4', null, null], ['L', 'L3', 'L4']]) $('#n-plat').append(cellView(id, a ? { only: (t) => t.id === a || t.id === b, tops: false } : { tops: false }));
  for (const id of ['A', 'D', 'F', 'L']) $('#n-tallfront').append(cellView(id, { only: (t) => t.id === CELLS[id].footid, tops: false }));
  hist('n-ratio', PAIRS.map((p) => p.ratio), { x: [0, 1], bins: 20, xl: 'smaller ÷ larger', yl: 'pairs', h: 200 });
  const caps = $('#n-caps'); for (const id of ['D', 'K', 'O', 'S9']) caps.append(cellView(id, { only: (t, m) => m.kind === 'cap', tops: false }));
  const gap = (poly) => (svg) => svg.append(s('polygon', { points: poly, fill: 'var(--a-gap)', 'fill-opacity': .35, stroke: 'var(--a-gap)', 'stroke-width': 2.5, 'vector-effect': 'non-scaling-stroke' }));
  $('#n-gaps').append(cellView('H', { stones: false, extra: gap('64,12 66.5,11 65,30 62,34 58,56 55,74 56,55 60,36 63,30') }), cellView('J', { stones: false, extra: gap('44,26 53,26 52,67 46,66') }), cellView('K', { stones: false, extra: gap('32,39 52,39.5 62,36 72,40 77,47 66,56 60,74 46,69 36,68') }), cellView('T15', { stones: false, extra: gap('50,36 62,34 68,40 67,62 62,86 52,84 49,60') }));
  const V = $('#n-valid'), F = (o, fn) => { const f = formation(Object.assign({ seed: 21, cols: 3, front: 2 }, o)); if (fn) f.stones.forEach(fn); return formMeshes(f); };
  const pair = (good, bad, gs, bs) => { V.append(still(good, { w: 300, h: 260, sub: '<b>Fits.</b> ' + gs }), still(bad, { w: 300, h: 260, sub: '<b>Does not.</b> ' + bs })); };
  pair(F({}), F({ step: 0, fall: 0, base: 1, pile: 0 }, (t) => { t.h = 1.3 - t.z; }), 'Tops step, or share a plane in one run.', 'Every top level, every row: reads as a built wall.');
  pair(F({}), F({}, (t) => { t.w -= .09; t.dp -= .09; }), 'Joints closed; bevels make the groove.', 'Stones stand apart: reads as posts.');
  const BL = { seed: 4, base: .55, cols: 3, pile: 4, depth: .7, setback: .08, step: .13, stack: .4, bond: .5 };
  pair(F(BL), F(Object.assign({}, BL, { spread: 0, step: 0, fall: 0, setback: 0 })), 'Joints staggered, sizes unequal.', 'Joints in line both ways, equal stones: reads as crates.');
  pair(F({}), F({}, (t) => { if (t.rank === 0) t.h *= 3.6; }), 'Low stones in front, tall behind.', 'Every front stone tall: the rest is hidden and the stair in depth is gone.');
}
