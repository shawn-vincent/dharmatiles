/* ---------- chapter 6 ---------- */
{
  const bv = $('#e-bevel'); for (const b of [0, .04, .1, .16, .22, .32, .45]) { const e = one({ w: .5, dp: .46, h: .8, bevel: b, bevelV: b, taperA: .95, taperD: .96, dipD: .05, jit: .015, seed: 3 }, { w: 200, h: 220, centre: [.6, .6, .4], sub: `<span class="num">${fmt(b)}</span>` }); if (b < .08 || b > .25) e.classList.add('out'); bv.append(e); }
  strip('e-lean', kindGroups((r) => r.lean), { x: [-15, 30], xl: 'degrees', ref: 0 });
  strip('e-close', kindGroups((r) => r.close), { x: [-30, 50], xl: 'degrees', ref: 0 });
  hist('e-nv', STONES.map((r) => r.nv), { x: [4.5, 19.5], bins: 15, xl: 'corners' });
  strip('e-cvx', kindGroups((r) => r.convex), { x: [.5, 1], xl: 'outline ÷ hull', xt: [.5, .6, .7, .8, .9, 1] });
  const base = { w: .46, dp: .44, h: 1.0, bevel: .14, bevelV: .14, taperA: .95, taperD: .96, jit: .012, seed: 6 };
  $('#e-tops').append(one(Object.assign({}, base, { dipD: .03 }), { sub: '<b>Level.</b>' }), one(Object.assign({}, base, { dipD: .16, dipA: -.08 }), { sub: '<b>Tipped.</b> Rises to the back.' }), one(Object.assign({}, base, { dipD: .62 }), { sub: '<b>Shed.</b> Down toward the front.' }), one(Object.assign({}, base, { leanA: -.36, dipA: -.2, a: .78 }), { sub: '<b>Carried.</b> Up the way it leans.' }));
  const fx = { w: .62, dp: .56, h: .8 };
  $('#e-faces').append(...[[.58, '30 degrees to the top: a flat bevel'], [1, '45 degrees: the usual'], [1.73, '60 degrees: steep, nearly upright']].map(([k, t]) => one(Object.assign({}, base, fx, { bevelT: [.08, .02, .1, .02], bevelK: k }), { sub: 'Chamfer at ' + t + '.', w: 280, h: 250, centre: [.6, .6, .4] })));
}
