/* ---------- chapter 2 ---------- */
{
  let pick = 'all'; const pills = $('#sil-pills');
  const chosen = () => ALL.filter((id) => pick === 'all' || CL[id].class === pick);
  const redo = []; 
  [['all', 'All 52'], ...CLASSES.map((k) => [k, CLASS_NAME[k]])].forEach(([k, name], i) => pills.append(h('button', { type: 'button', 'aria-pressed': i === 0, text: name, onclick: (e) => { pick = k; for (const b of pills.children) b.setAttribute('aria-pressed', b === e.target); redo.forEach((f) => f()); } })));
  const M = Object.fromEntries(ALL.map((id) => [id, massFh(id)]));
  const frame = (w, hgt) => { const k = Math.min((w - 20) / 2.4, (hgt - 24) / 2.5); return (p) => [w - 12 + p[0] * k, hgt - 14 - p[1] * k]; };
  chart('sil-all', 330, (svg, w, hgt) => { const X = frame(w, hgt);
    svg.append(s('line', { x1: X([-2.3, 0])[0], x2: w - 6, y1: X([0, 0])[1], y2: X([0, 0])[1], stroke: 'var(--rule)' }));
    for (const id of chosen()) svg.append(s('polygon', { points: pts(M[id].map(X)), fill: 'var(--stone3)', 'fill-opacity': .05, stroke: 'var(--a-out)', 'stroke-opacity': .5, 'stroke-width': 1 }, s('title', { text: id })));
    const a = X([-2.25, 0]), b = X([-2.25, 1]); svg.append(s('line', { x1: a[0], y1: a[1], x2: b[0], y2: b[1], stroke: 'var(--a-dim)', 'stroke-width': 3 }), s('text', { x: a[0] + 6, y: b[1] + 10, 'font-size': 11, text: '1 fh' }));
  });
  chart('sil-heat', 330, (svg, w, hgt) => { const X = frame(w, hgt), ids = chosen(), N = 44;
    for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) { const p = [-(i + .5) / N * 2.4, (j + .5) / N * 2.5]; let c = 0; for (const id of ids) if (pointIn(p, M[id])) c++; if (!c) continue; const a = X([-(i + 1) / N * 2.4, (j + 1) / N * 2.5]), b = X([-i / N * 2.4, j / N * 2.5]); svg.append(s('rect', { x: a[0], y: a[1], width: b[0] - a[0] + .5, height: b[1] - a[1] + .5, fill: 'var(--a-out)', 'fill-opacity': (c / ids.length) * .9 })); }
    const a = X([-2.25, 0]), b = X([-2.25, 1]); svg.append(s('line', { x1: a[0], y1: a[1], x2: b[0], y2: b[1], stroke: 'var(--a-dim)', 'stroke-width': 3 }));
  });
  redo.push(...DRAWS.slice(-2));
  chart('sky', 280, (svg, w, hgt) => { const ax = axes(svg, w, hgt, { x: [0, 1], y: [0, 2.5], xl: 'across the outline, left to right', yl: 'fh', xt: [0, .25, .5, .75, 1] });
    const xs = Array.from({ length: 48 }, (_, i) => (i + .5) / 48), line = (v) => v.map((y, i) => ax.x(xs[i]) + ',' + ax.y(y)).join(' ');
    const q = (p) => xs.map((_, i) => quant(ALL.map((id) => CELLS[id].sky[i]), p));
    svg.append(s('polygon', { points: line(q(.25)) + ' ' + q(.75).map((y, i) => ax.x(xs[i]) + ',' + ax.y(y)).reverse().join(' '), fill: 'var(--a-top)', 'fill-opacity': .22 }));
    for (const id of ALL) svg.append(s('polyline', { points: line(CELLS[id].sky), fill: 'none', stroke: 'var(--a-out)', 'stroke-opacity': .28, 'stroke-width': 1 }));
    svg.append(s('polyline', { points: line(q(.5)), fill: 'none', stroke: 'var(--a-top)', 'stroke-width': 3 }));
  });
  strip('sky-peak', classGroups((c) => c.peak), { x: [0, 1], xl: 'highest point, share of the way across', left: 78, xt: [0, .25, .5, .75, 1] });
  strip('sky-fill', classGroups((c) => c.fill), { x: [.5, 1], xl: 'stone ÷ bounding box', left: 78, xt: [.5, .6, .7, .8, .9, 1] });

  /* the envelope, on a generated formation */
  const f = formation({ preset: 'pillars', seed: 11, nA: 4, rows: 3, fall: .45 }), ms = formMeshes(f), row = $('#env-row'), q = f.q;
  const lv = (box, view) => { for (const st of f.stones) { const c = st.col; if (st.z + st.h < c.h - 1e-6 && st.kind !== 'cap') continue; const top = st.z + st.h; const A = view === 'side' ? box.proj(c.a0 + c.w / 2, c.row.d0, top) : box.proj(c.a0, c.row.d0, top), B = view === 'side' ? box.proj(c.a0 + c.w / 2, c.row.d0 + c.row.dp, top) : box.proj(c.a0 + c.w, c.row.d0, top); box.svg.append(s('line', { x1: A[0], y1: A[1], x2: B[0], y2: B[1], stroke: 'var(--a-top)', 'stroke-width': 2.5 })); } };
  const a = still(ms, { tag: 'as the references see it', w: 360, h: 320 });
  { const back = PLATE, e = (aa, t) => q.height * lerp(q.front, 1, t) * (1 - q.fall * Math.max(0, aa - q.peak * q.length) / (q.length * (1 - q.peak))); const P = [[0, back - q.depth, e(0, 0)], [0, back, e(0, 1)], [q.length, back, e(q.length, 1)], [q.length, back - q.depth, e(q.length, 0)]].map((p) => a.proj(...p)); a.svg.append(s('polygon', { points: pts(P), fill: 'var(--a-top)', 'fill-opacity': .18, stroke: 'var(--a-top)', 'stroke-width': 2, 'stroke-dasharray': '6 4' })); }
  const b = still(ms, { tag: 'front', view: 'front', w: 360, h: 320, span: 2.3, centre: [PLATE / 2, PLATE / 2, .9] }); lv(b, 'front');
  const c = still(ms, { tag: 'from the cut end', view: 'side', w: 360, h: 320, span: 2.3, centre: [PLATE / 2, PLATE / 2, .9] }); lv(c, 'side');
  row.append(a, b, c);

  const fr = $('#foot-row');
  const cut = (poly) => (svg) => svg.append(s('polygon', { points: poly, fill: 'var(--a-gap)', 'fill-opacity': .3, stroke: 'var(--a-gap)', 'stroke-width': 2.5, 'vector-effect': 'non-scaling-stroke' }));
  fr.append(cellView('E', { stones: false, extra: cut('79.5,52 94.5,49 95,79 90,82 79.5,83.5') }), cellView('I', { stones: false, extra: cut('93,22 94,45 93,78 90,82 88,61 88,42') }), cellView('H', { only: (st) => st.id === 'H1', tops: false }), cellView('S2', { only: (st, m) => m.kind === 'toe', tops: false }));

  /* one set of stones, three ways */
  const base = formation({ preset: 'pillars', seed: 5, nA: 4, rows: 3 });
  const noEnv = formation({ preset: 'pillars', seed: 5, nA: 4, rows: 3, fall: 0, front: 1, rough: 5, stepMin: 0 });
  const pile = formation({ preset: 'pillars', seed: 5, nA: 4, rows: 3 }); { const r = rng(77); for (const st of pile.stones) { st.a += (r() - .5) * .5 + .15; st.d += (r() - .5) * .45 - .15; st.yaw = (r() - .5) * 1.6; st.leanA = (r() - .5) * .5; st.leanD = (r() - .5) * .4; } }
  $('#pile-row').append(still(formMeshes(base), { sub: '<b>A cliff.</b> Envelope, tight joints, one upright, low stones in front.' }), still(formMeshes(noEnv), { sub: '<b>A wall of posts.</b> Heights drawn with no envelope: the front hides the back and the skyline is noise. <i>Leaves the style.</i>' }), still(formMeshes(pile), { sub: '<b>A pile.</b> Same stones turned, tilted and spread each on its own. <i>Leaves the style.</i>' }));
}
