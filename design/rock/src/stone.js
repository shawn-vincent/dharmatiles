/* stone.js — owns the pipeline: a stone is a list of the stages that are on, each handed only its own settings.
     params -> region (Outline cuts) -> pieces (Chunks: layout, treat, settle) -> field/marks (Chips, Pockets, Grain, Wear) -> mesh
   A stage at 0 is not in the list, so it cannot read anything: zero returns its input exactly, by construction.
   build(P) is the model (pieces and their states); mesh(P, vox) draws it. The model is cached on what it reads, so a Pockets,
   Grain or Wear slider never rebuilds the pieces. */
var Stone = Stone || {};
(function (S) {
  const { STAGES, TIERS } = S.controls;
  const layoutCache = new Map();   // key -> the model, the last few

  function stagesOf(P) { const st = {}; STAGES.forEach((s) => { if (P[s.id] > 0) st[s.id] = S.settingsOf(P, s.id); }); return st; }
  function contextOf(P, R, size, bedding) { const g = S.stoneOf(P); return { R, size: size || g.size, patch: g.patch, bedding: bedding || { dip: g.dip, facing: g.facing } }; }   // the strata lean: a cell's (its bed's), else the stone's own dip and facing
  /* the convexes the stone fills: a cell's, or the box of its own width, breadth and height */
  const regionsOf = (P, cell) => { const c = cell || S.cell.box(S.stoneOf(P).size); return { convexes: c.convexes(), size: c.size, key: c.key, bedding: c.bedding }; };
  const wearOf = (st, ctx) => (st.wear ? S.field.wearRadius(st.wear, ctx) : null);

  const now = () => (typeof performance !== 'undefined' && performance.now ? performance.now() : Date.now());
  /* build(P, onstep): onstep({step, phase: 'start' | 'end', ms, cached}) is told as each stage that makes the pieces (cuts, chunks) starts and ends. A model built before is
     kept, with the time it took, and its steps are told again with cached: true. */
  function build(P, onstep, cell) {
    const st = stagesOf(P), g = S.stoneOf(P), R = S.rand.stream(g.seed), rg = regionsOf(P, cell), ctx = contextOf(P, R, rg.size, rg.bedding);
    const minContact = S.settle.contactFor(st.wear ? st.wear.strength : 0);
    const key = JSON.stringify([g, st.cuts || 0, st.chunks || 0, minContact, rg.key, rg.size, rg.bedding]);
    const emit = (step, phase, ms, cached) => { if (onstep) onstep({ step, phase, ms, cached }); };
    const hit = layoutCache.get(key);
    if (hit) { for (const id of ['cuts', 'chunks']) if (hit.report.steps[id]) { emit(id, 'start'); emit(id, 'end', hit.report.steps[id].ms, true); } return hit; }
    const report = { refused: 0, strata: 0, tiled: 1, steps: {} };
    const timed = (id, fn) => { emit(id, 'start'); const t = now(), r = fn(); report.steps[id] = { ms: now() - t }; emit(id, 'end', report.steps[id].ms, false); return r; };
    /* a bed that carries a bear outline is always cut to it, even with the cuts stage at 0: the bed rule wins over everything (a stone with no bear is unchanged) */
    const cutCfg = st.cuts || (rg.convexes.some((c) => c.faces.some((f) => f.bear)) ? { strength: 0 } : null);
    const cutAll = () => { const o = rg.convexes.map((c) => S.region.outlineCuts(c, cutCfg, R)); return { convexes: o.map((x) => x.convex), refused: o.reduce((a, x) => a + x.refused, 0) }; };
    const region = cutCfg ? timed('cuts', cutAll) : { convexes: rg.convexes, refused: 0 };
    report.refused += region.refused;
    const regionVol = region.convexes.reduce((a, c) => a + c.volume(), 0);
    const frame = S.layout.frame(ctx.bedding, R), fall = st.chunks ? st.chunks.fall : 0;
    let pieces, s;
    const settleAll = () => {
      if (st.chunks) {
        pieces = []; report.strata = 0; let tiledVol = 0;
        region.convexes.forEach((c, ci) => { const lay = S.layout.layout(c, st.chunks, ctx.bedding, R, ci ? 'c' + ci + ':' : ''), tr = S.treat.treat(lay.pieces, st.chunks, R); report.refused += tr.refused; report.strata = Math.max(report.strata, lay.strata); tiledVol += lay.tiled * c.volume(); for (const p of tr.pieces) pieces.push(p); });
        pieces.forEach((p, i) => { p.id = i; }); report.tiled = tiledVol / regionVol;
      } else pieces = region.convexes.map((c, ci) => ({ id: ci, key: ci, convex: c, stratum: 0 }));
      s = S.settle.settle(pieces, fall, minContact);
      // Outer is one definition: the solid across a face is not stone. After settle, the joints that a fall left bare become surface, set back and tilted
      // like the rest (treat.expose), and settle runs again, since a shave can only shrink contacts. Marks, field, chips and the views read the roles after this.
      if (st.chunks) for (let ex = S.settle.outerFaces(pieces, minContact); ex.length; ex = S.settle.outerFaces(pieces, minContact)) { report.refused += S.treat.expose(ex, st.chunks, R).refused; s = S.settle.settle(pieces, fall, minContact); }
    };
    if (st.chunks) timed('chunks', settleAll); else settleAll();
    const standing = pieces.filter((p) => p.state === 'standing'), vol = standing.reduce((a, p) => a + p.convex.volume(), 0);
    Object.assign(report, { nPieces: standing.length, fellSize: pieces.filter((p) => p.state === 'fell:size').length, fellFloat: pieces.filter((p) => p.state === 'fell:float').length, sealed: s.sealed, noHeld: !s.anyHeld, volumeRatio: vol / regionVol, minContact });
    const model = { pieces, standing, region: region.convexes[0], regions: region.convexes, frame, report, size: rg.size, bedding: ctx.bedding };
    layoutCache.set(key, model); if (layoutCache.size > 6) layoutCache.delete(layoutCache.keys().next().value);
    return model;
  }

  /* the sampled field of the stone, for the mesher and the Lines view */
  function fieldOf(P, model) {
    const m = model || build(P), st = stagesOf(P), ctx = contextOf(P, S.rand.stream(P.seed | 0), m.size, m.bedding);
    ctx.frame = m.frame;
    return S.field.makeField(m.standing, wearOf(st, ctx), S.marks.active(P, ctx));
  }
  const sampled = (P) => P.wear > 0 || P.chips > 0 || P.pockets > 0 || P.grain > 0;

  /* draw the stone. vox is the voxel in mm (a tier), used only on the sampled path. Returns {mesh, report}. */
  function mesh(P, vox, onp, onstep, cell) {
    const t0 = Date.now(), model = build(P, onstep, cell), report = Object.assign({}, model.report, { steps: Object.assign({}, model.report.steps) });
    let m;
    if (sampled(P)) {
      // Chips, Pockets, Grain and Wear are one pass: the field is evaluated at every sample of the grid and every vertex, with each stage's work folded into the same
      // evaluation, so they have one time between them, not four.
      if (onstep) onstep({ step: 'sampled', phase: 'start' });
      const t = now(); m = S.mesh.sampledMesh(fieldOf(P, model), vox, onp, !!(cell && cell.manifold));
      const p = S.mesh.parts(m, vox); report.islands = p.islands; report.specks = p.specks;
      report.steps.sampled = { ms: now() - t, stages: S.controls.STAGES.map((x) => x.id).filter((id) => ['chips', 'pockets', 'grain', 'wear'].includes(id) && P[id] > 0) };
      if (onstep) onstep({ step: 'sampled', phase: 'end', ms: report.steps.sampled.ms });
    } else { m = S.mesh.polytopeMesh(model.pieces, false); report.islands = []; report.specks = []; }
    report.overhang = S.checks.overhang(m); report.tris = m.tris; report.path = m.path; report.vox = m.vox || 0; report.ms = Date.now() - t0;
    return { mesh: m, report };
  }
  /* the pieces themselves, fallen ones too: the Chunks view, whatever else is on */
  const chunkMesh = (P, onstep) => { const model = build(P, onstep), m = S.mesh.polytopeMesh(model.pieces, true); m.pieceInfo = model.pieces.map((p) => ({ state: p.state, size: p.size, centre: p.convex.centroid(), held: p.held })); return m; };

  S.stone = { build, fieldOf, mesh, chunkMesh, sampled, stagesOf };
})(Stone);
