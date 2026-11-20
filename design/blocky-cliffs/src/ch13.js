/* test */
{ const t = $('#gen-test'); if (t) { for (const [p, seed] of [['pillars', 1], ['pillars', 2], ['blocks', 3], ['blocks', 4], ['courses', 5], ['leaners', 6], ['ramp', 7], ['gates', 8]]) { const f = formation({ preset: p, cls: p, seed }); t.append(still(formMeshes(f), { tag: p + ' ' + seed, figure: [1.25, .45] })); } } }
