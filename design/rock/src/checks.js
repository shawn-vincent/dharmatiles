/* checks.js — owns the claims about the stone, each marked for what it is.
   PROVEN BY CONSTRUCTION (the check only confirms the build on this stone): zero identity, nothing outside the region, every piece reaches the seat.
   MEASURED ON EACH BUILD: islands and specks (mesh.js), overhang (here). */
var Stone = Stone || {};
(function (S) {
  const { TIERS, CONTROLS, STAGES, defaults, strengthRow, settingsRows } = S.controls, abs = Math.abs;
  const RED_NZ = Math.sin(65 * Math.PI / 180);     // past 65 degrees from vertical: red; past 55 (convex.OVERHANG_NZ): orange
  const SEAT_Z = 0.15;                             // mm: faces this low are the seat's own and are not overhang

  /* downward area past 55 and past 65 degrees, as the printability gate reads it */
  function overhang(m) {
    const p = m.pos, ix = m.idx; let down = 0, band = 0, red = 0;
    for (let t = 0; t < ix.length; t += 3) {
      const a = ix[t] * 3, b = ix[t + 1] * 3, c = ix[t + 2] * 3;
      const ux = p[b] - p[a], uy = p[b + 1] - p[a + 1], uz = p[b + 2] - p[a + 2], vx = p[c] - p[a], vy = p[c + 1] - p[a + 1], vz = p[c + 2] - p[a + 2];
      const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx, A = 0.5 * Math.hypot(nx, ny, nz);
      if ((p[a + 2] + p[b + 2] + p[c + 2]) / 3 < SEAT_Z) continue;
      const ov = -nz / (2 * A || 1); if (ov <= 0) continue;
      down += A; if (ov > RED_NZ) red += A; else if (ov > S.convex.OVERHANG_NZ) band += A;
    }
    return { down, band, red };
  }
  /* FNV-1a over the float bits of the positions and indices */
  function hashMesh(m) {
    let h = 2166136261; const w = new Uint32Array(m.pos.buffer, m.pos.byteOffset, m.pos.length);
    for (let i = 0; i < w.length; i++) h = Math.imul(h ^ w[i], 16777619);
    for (let i = 0; i < m.idx.length; i++) h = Math.imul(h ^ m.idx[i], 16777619);
    return (h >>> 0).toString(16);
  }
  const base = () => defaults();
  const mid = () => { const P = defaults(); STAGES.forEach((s) => { P[s.id] = strengthRow(s.id).def; }); return P; };

  /* PROVEN: every stage at 0 is the box; each stage at 0 ignores every one of its own settings (at their least and their most)
     while all the other stages are on. Sampled at the check voxel (coarse: the claim is about identity, not looks). */
  function zeroIdentity(seed) {
    const out = { box: false, stages: {} }, P0 = Object.assign(base(), { seed: seed || 1 }), m0 = S.stone.mesh(P0, TIERS.check).mesh;
    const box = new Set(); for (const x of [-12, 12]) for (const y of [-12, 12]) for (const z of [0, 21]) box.add(x + ',' + y + ',' + z);
    let corners = true; for (let i = 0; i < m0.pos.length; i += 3) if (!box.has(m0.pos[i] + ',' + m0.pos[i + 1] + ',' + m0.pos[i + 2])) corners = false;
    out.box = m0.path === 'polytope' && m0.tris === 12 && corners;
    STAGES.forEach((s) => {
      const M = Object.assign(mid(), { seed: seed || 1, [s.id]: 0 }), hs = [];
      for (const which of ['def', 'min', 'max']) { const Q = Object.assign({}, M); settingsRows(s.id).forEach((c) => { Q[c.key] = which === 'def' ? c.def : c[which]; }); hs.push(S.checks.hashMesh(S.stone.mesh(Q, TIERS.check).mesh)); }
      out.stages[s.id] = hs.every((h) => h === hs[0]);
    });
    out.ok = out.box && Object.values(out.stages).every(Boolean);
    return out;
  }
  /* PROVEN, checked here on this build: no piece vertex outside the region; the standing pieces are one body resting on the seat; no void sealed inside */
  function invariants(model) {
    let outside = 0;   // a vertex is outside when it is past a plane of every convex of the cell: the least such excess over the convexes, the most over the vertices
    for (const p of model.standing) for (const v of p.convex.vertices()) { let least = Infinity; for (const rc of model.regions) { let e0 = -Infinity; for (const f of rc.faces) { const e = f.n[0] * v[0] + f.n[1] * v[1] + f.n[2] * v[2] - f.d; if (e > e0) e0 = e; } if (e0 < least) least = e0; } if (least > outside) outside = least; }
    const seen = new Set(), first = model.standing.find((p) => p.held), stack = first ? [first] : []; stack.forEach((p) => seen.add(p));
    while (stack.length) { const p = stack.pop(); for (const o of p.contacts) if (o.state === 'standing' && !seen.has(o)) { seen.add(o); stack.push(o); } }
    const unreached = model.standing.filter((p) => !seen.has(p)).length;
    // outer is one definition: no joint face of a standing piece may have a fallen or absent neighbour
    const stray = S.settle.outerFaces(model.pieces, model.report.minContact).length;
    return { stray, outside, unreached, sealed: model.report.sealed, noHeld: model.report.noHeld, ok: stray === 0 && outside < 1e-6 && unreached === 0 && model.report.sealed === 0 && !model.report.noHeld };
  }
  S.checks = { overhang, hashMesh, zeroIdentity, invariants };
})(Stone);
