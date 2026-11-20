/* ---------- chapter 12 ---------- */
{
  const base = { preset: 'pillars', seed: 7, nA: 3, rows: 2 }, blk = { preset: 'blocks', seed: 4, nA: 3, rows: 2, cap: 0 };
  const rows = [
    ['Stones taller or shorter', 'height', base, [.7, 1.1, 1.5, 1.8, 2.5], (v) => ({ height: v }), [1.1, 1.95], ' fh', 'Below 1.4 there is only A (1.15 as seen): in the family, but one in 52. Above 2 the stones become masts and the figure no longer reads against them.'],
    ['Stones wider or narrower', 'nA', base, [1, 2, 3, 4, 6], (v) => ({ nA: v }), [2, 4], ' along', 'One stone along is a monolith here (it is right for a stair, where rows do the work). Six gives staves: width under 0.25 fh has no counterpart in the sheets.'],
    ['Stacking in line or staggered', 'stagger', blk, [0, .25, .5, .75, 1], (v) => ({ stagger: v, sizeVar: v === 0 ? 0 : .25 }), [.25, .8], '', 'In line with equal stones is a crate stack. Any stagger over a quarter of a stone is enough.'],
    ['More or less batter', 'taper', base, [1.1, 1, .94, .85, .7], (v) => ({ taper: v }), [.88, 1.0], ' top ÷ foot', 'Wider at the top than the foot never occurs except on a cap. Below 0.85 every stone is a toe and the joints gape.'],
    ['Larger or smaller offsets', 'offset', base, [0, .03, .08, .15, .3], (v) => ({ offset: v, gap: v > .1 ? v * .3 : 0 }), [0, .06], ' fh', 'Zero is acceptable but stiff. Past 0.08 stones turn visibly against each other and joints open.'],
    ['Sizes more or less alike', 'sizeVar', Object.assign({}, base, { nA: 4 }), [0, .15, .3, .5, .85], (v) => ({ sizeVar: v }), [.15, .5], '', 'Equal widths read as a fence. At 0.85 one stone is a sliver, and slivers are not in the sheets.'],
    ['More or fewer stones', 'count', base, [[1, 1], [2, 2], [3, 2], [4, 3], [6, 4]], (v) => ({ nA: v[0], rows: v[1] }), null, '', 'Two to three stones is a boulder group; over about twelve the stones fall under a twentieth of the whole and read as texture.', (v) => v[0] + ' × ' + v[1], (v) => v[0] * v[1] >= 4 && v[0] * v[1] <= 12],
    ['Overhang smaller or larger', 'over', Object.assign({}, base, { cap: 1 }), [0, .1, .2, .35, .6], (v) => ({ over: v }), [0, .2], ' fh', 'Up to 0.2 fh the cap juts as in D and O. Past it the cap is a mushroom; the sheets never show one, and a printed one would need support.'],
    ['More or less lopsided', 'fall', base, [0, .2, .42, .65, .9], (v) => ({ fall: v }), [.1, .6], '', 'Zero is a wall with a ruled top. At 0.9 the far end is at the ground: the sheets always end in a stone 0.4 fh or more high.'],
    ['Softer or sharper', 'bevel', base, [0, .05, .16, .25, .42], (v) => ({ bevel: v }), [.1, .23], '', 'Sharp is a cut block; too soft is a pebble with no top face.'],
    ['Common lean', 'lean', base, [0, 8, 16, 26, 42], (v) => ({ lean: v }), [0, 30], '°', 'Shared by every stone it stays in the style to about 30 degrees (P is 24). Past that the stones read as toppled.']];
  const box = $('#sens');
  for (const [name, key, b, vals, f, ok, unit, note, lab, test] of rows) {
    const cells = vals.map((v) => { const inside = test ? test(v) : (v >= ok[0] && v <= ok[1]); const e = still(formMeshes(formation(Object.assign({}, b, f(v)))), { w: 220, h: 196, sub: `<span class="num">${lab ? lab(v) : v}${unit}</span>` }); if (!inside) e.classList.add('dimmed'); return e; });
    box.append(h('div', { class: 'catrow' }, h('div', { class: 'words' }, h('h3', { text: name }), h('p', { html: `<code>${key}</code>` + (ok ? ` · in the style <b>${ok[0]} to ${ok[1]}</b>${unit}` : ' · in the style <b>4 to 11 stones</b>') }), h('p', { text: note })), h('div', { class: 'row', style: 'grid-template-columns:repeat(5,minmax(0,1fr));gap:6px' }, cells)));
  }
}
