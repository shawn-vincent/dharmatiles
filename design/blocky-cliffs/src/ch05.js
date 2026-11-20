/* ---------- chapter 5 ---------- */
{
  chart('p-plan', 230, (svg, w, hgt) => { const cx = w / 2, cy = 92, k = 62, c = Math.SQRT1_2;
    const R = (a, d) => [cx + (a * c - d * c) * k, cy - (a * c + d * c) * k * 1 + 20];   /* plan, turned 45 degrees so the camera is at the bottom */
    const W = 1.3, D = .9, P = [R(0, 0), R(W, 0), R(W, D), R(0, D)];
    svg.append(s('polygon', { points: pts(P), fill: 'var(--stone)', stroke: 'var(--stone3)', 'stroke-width': 1.5 }));
    const xs = P.map((p) => p[0]), x0 = Math.min(...xs), x1 = Math.max(...xs), yb = hgt - 30;
    for (const x of [x0, x1]) svg.append(s('line', { x1: x, x2: x, y1: 20, y2: yb, stroke: 'var(--a-dim)', 'stroke-dasharray': '4 4' }));
    svg.append(s('line', { x1: x0, x2: x1, y1: yb, y2: yb, stroke: 'var(--a-dim)', 'stroke-width': 2.5 }), s('text', { x: (x0 + x1) / 2, y: yb + 16, 'text-anchor': 'middle', 'font-size': 12, text: 'width seen = 0.71 × (length + depth)' }));
    const mid = (a, b) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]; const m1 = mid(P[0], P[1]), m2 = mid(P[0], P[3]);
    svg.append(s('text', { x: m1[0] + 8, y: m1[1] + 14, 'font-size': 12, text: 'length' }), s('text', { x: m2[0] - 8, y: m2[1] + 14, 'font-size': 12, 'text-anchor': 'end', text: 'depth' }), s('text', { x: cx, y: hgt - 2, 'text-anchor': 'middle', 'font-size': 11, class: 'mut', text: 'camera' }));
  });
  $('#p-spec').append(cellView('M', {}));
  scatter('p-wh', STONES, { x: [0, 1.6], y: [0, 2], xl: 'width, fh', yl: 'face height, fh', fx: (r) => r.wb, fy: (r) => r.hb, fk: (r) => r.kind, title: (r) => r.id, h: 340,
    under: (svg, ax) => { for (const k of [.5, 1, 2, 4]) { const xe = Math.min(1.6, 2 / k); svg.append(s('line', { x1: ax.x(0), y1: ax.y(0), x2: ax.x(xe), y2: ax.y(xe * k), stroke: 'var(--muted)', 'stroke-dasharray': '2 4' }), s('text', { x: ax.x(xe) - 4, y: ax.y(xe * k) + 12, 'font-size': 10.5, 'text-anchor': 'end', class: 'mut', text: '×' + k })); } } });
  $('#p-wh-leg').append(legend(KINDS));
  hist('p-w', STONES.map((r) => r.wb), { x: [0, 1.6], bins: 16, xl: 'width, fh' });
  hist('p-h', STONES.map((r) => r.hb), { x: [0, 2], bins: 16, xl: 'face height, fh' });
  strip('p-sl', kindGroups((r) => r.slender), { x: [0, 5], xl: 'face height ÷ width, as seen', ref: 1 });
  chart('p-rank', 250, (svg, w, hgt) => { const ax = axes(svg, w, hgt, { x: [.5, 11.5], y: [0, .6], xl: 'rank by size', yl: 'share', xt: [1, 3, 5, 7, 9, 11] });
    const by = {}; for (const r of STONES) (by[r.cell] = by[r.cell] || []).push(r.share); const all = Object.values(by).map((v) => { const t = v.reduce((a, b) => a + b, 0); return v.map((q) => q / t).sort((a, b) => b - a); });
    for (const v of all) svg.append(s('polyline', { points: v.map((q, i) => ax.x(i + 1) + ',' + ax.y(Math.min(q, .6))).join(' '), fill: 'none', stroke: 'var(--a-out)', 'stroke-opacity': .3 }));
    const med = []; for (let i = 0; i < 8; i++) { const col = all.filter((v) => v.length > i).map((v) => v[i]); if (col.length > 8) med.push(ax.x(i + 1) + ',' + ax.y(quant(col, .5))); }
    svg.append(s('polyline', { points: med.join(' '), fill: 'none', stroke: 'var(--a-top)', 'stroke-width': 3 })); });
  hist('p-share', STONES.map((r) => r.share), { x: [0, .6], bins: 15, xl: 'share of the formation' });
  strip('p-top', kindGroups((r) => r.topshare), { x: [0, .7], xl: 'top face ÷ outline' });
  hist('p-dip', STONES.map((r) => r.topdip), { x: [-15, 25], bins: 16, xl: 'degrees' });
  const C = ALL.map((id) => CELLS[id]);
  statTable('p-table', [['Stone: width', STONES.map((r) => r.wb)], ['Stone: face height', STONES.map((r) => r.hb)], ['Stone: face height ÷ width', STONES.map((r) => r.slender)],
    ['… pillars only', STONES.filter((r) => r.kind === 'pillar').map((r) => r.slender)], ['… blocks only', STONES.filter((r) => r.kind === 'block').map((r) => r.slender)], ['… slabs only', STONES.filter((r) => r.kind === 'slab').map((r) => r.slender)], ['… toes only', STONES.filter((r) => r.kind === 'toe').map((r) => r.slender)],
    ['Stone: share of its formation', STONES.map((r) => r.share)], ['Formation: largest stone’s share', C.map((c) => c.big)], ['Formation: second largest ÷ largest', C.map((c) => c.big2)], ['Formation: stones', C.map((c) => c.n), 0],
    ['Formation: height', C.map((c) => c.H)], ['Formation: width', C.map((c) => c.W)], ['Formation: stone ÷ bounding box', C.map((c) => c.fill)], ['Formation: highest point, way across', C.map((c) => c.peak)]]);
}
