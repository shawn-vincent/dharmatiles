/* treat.js — owns stage 2b, what Chunks does to each piece: set-back and tilt of its exposed faces, and a lost corner.
   One operation, `shave`: a surface cut some depth below the piece's own highest point in a direction. It only removes, so a
   piece never leaves the region. A shave that would leave the piece under MIN_PIECE, or under a quarter of its seat, is refused
   and counted. Corners only break off on the region's surface: a corner on a joint would open a hole. */
var Stone = Stone || {};
(function (S) {
  const { norm, cross, perp, rotate, sub } = S.vec, PI = Math.PI;
  const SETBACK_MIN = 0.15;       // a piece is set back 15% to 100% of `setback`
  const CORNER_DEPTH = 0.15;      // a lost corner is this share of the piece's size deep
  const SEAT_KEEP = 0.25;         // a treated piece keeps at least this share of its own seat

  function treat(pieces, cfg, R) {
    let refused = 0;
    const out = pieces.map((p) => {
      let c = p.convex.withGuard({ seatFloor: SEAT_KEEP * p.convex.heldArea() });
      const k = p.key, u = (purpose, i = 0) => R.unit('chunks', purpose, k, i);
      const D = cfg.setback * (SETBACK_MIN + (1 - SETBACK_MIN) * u('setback')), ang = cfg.tilt * PI / 180 * (2 * u('tilt') - 1);
      if (D > 0) c.faces.filter((f) => f.role === 'surface').forEach((f, i) => {
        const ax = rotate(perp(f.n), f.n, u('tilt-axis', i) * 2 * PI), n2 = rotate(f.n, ax, ang);
        const next = c.shave(n2, D); if (next) c = next; else refused++;
      });
      if (R.laid(cfg.corners, 'chunks', 'corner', k)) {
        const seen = []; for (const f of c.faces) if (f.role === 'surface') for (const v of f.pts) seen.push(v);
        if (seen.length) {
          const v = seen[Math.min(seen.length - 1, (u('corner-which') * seen.length) | 0)], size = Math.cbrt(c.volume());
          const next = c.shave(sub(v, c.centroid()), CORNER_DEPTH * size); if (next) c = next; else refused++;
        }
      }
      return Object.assign({}, p, { convex: c });
    });
    return { pieces: out, refused };
  }
  /* A face a fall has exposed is a surface like any other: it is set back and tilted by the same draws as its piece's own exposed faces.
     list: the {p, f} of settle.outerFaces. A shave only moves a face inward, so contacts can only shrink; settle then runs again (it ends: a face is exposed once). */
  function expose(list, cfg, R) {
    let refused = 0; const by = new Map();
    for (const { p, f } of list) { if (!by.has(p)) by.set(p, []); by.get(p).push(f); }
    for (const [p, faces] of by) {
      let c = p.convex.outer(faces.map((f) => f.id)).withGuard({ seatFloor: SEAT_KEEP * p.convex.heldArea() });
      const k = p.key, u = (purpose, i = 0) => R.unit('chunks', purpose, k, i);
      const D = cfg.setback * (SETBACK_MIN + (1 - SETBACK_MIN) * u('setback')), ang = cfg.tilt * PI / 180 * (2 * u('tilt') - 1);
      if (D > 0) for (const f of faces) {
        const ax = rotate(perp(f.n), f.n, u('tilt-axis-fall', S.rand.fnv(String(f.nb))) * 2 * PI), next = c.shave(rotate(f.n, ax, ang), D);
        if (next) c = next; else refused++;
      }
      p.convex = c;
    }
    return { refused };
  }
  S.treat = { treat, expose };
})(Stone);
