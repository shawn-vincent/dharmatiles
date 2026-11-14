// The overlay's geometry: a tile's map (lines, regions, points; docs/design/editor.html, "The stack and the map") as
// triangles and anchors.  Pure: no DOM, no THREE, no state.  Points are in squares, results in mm.  The editor's overlay
// (overlay.js) and the site's flat renderer (overlay-render.js) both draw from these, and the marks and their states
// (overlay-marks.js) say which to draw.

/**
 * A region's lattice cells as flat triangles (x, y in mm, z 0: the caller sets the height): one quad per run of set nodes in a row, clipped to the tile.
 * Returns a Float32Array of 9 numbers per triangle; `sq` is mm per square.
 */
export function regionTriangles(r, m, sq) {
  const out = [];
  const step = 1 / m.cells;
  for (let j = 0; j < r.ny; j++) {
    let i = 0;
    while (i < r.nx) {
      if (r.bits[j * r.nx + i] !== '1') { i++; continue; }
      const a = i;
      while (i < r.nx && r.bits[j * r.nx + i] === '1') i++;
      const x0 = Math.max(0, (a - 0.5) * step) * sq, x1 = Math.min(m.cols, (i - 0.5) * step) * sq;
      const y0 = Math.max(0, (j - 0.5) * step) * sq, y1 = Math.min(m.rows, (j + 0.5) * step) * sq;
      if (x1 <= x0 || y1 <= y0) continue;
      out.push(x0, y0, 0, x1, y0, 0, x1, y1, 0, x0, y0, 0, x1, y1, 0, x0, y1, 0);
    }
  }
  return new Float32Array(out);
}


/**
 * A polyline (points in squares) as a ribbon in the 3D scene: a flat strip 2*`half` mm wide at z 0
 * and a curtain from -`half` to `half` along it, so it reads from above and from the side.
 * Returns a Float32Array of triangles (9 numbers each); `closed` joins the last point to the first;
 * `curtain` false leaves out the upright strip (a flat band).
 */
export function ribbonTriangles(pts, sq, half = 0.7, closed = false, curtain = true) {
  const out = [];
  const n = pts.length;
  for (let k = 0; k < (closed ? n : n - 1); k++) {
    const a = pts[k], b = pts[(k + 1) % n];
    let ax = a[0] * sq, ay = a[1] * sq, bx = b[0] * sq, by = b[1] * sq;
    const len = Math.hypot(bx - ax, by - ay);
    if (!len) continue;
    const ux = (bx - ax) / len, uy = (by - ay) / len;
    ax -= ux * half; ay -= uy * half; bx += ux * half; by += uy * half; // butt joints meet
    const nx = -uy * half, ny = ux * half;
    out.push(ax - nx, ay - ny, 0, bx - nx, by - ny, 0, bx + nx, by + ny, 0, ax - nx, ay - ny, 0, bx + nx, by + ny, 0, ax + nx, ay + ny, 0);
    if (curtain) out.push(ax, ay, -half, bx, by, -half, bx, by, half, ax, ay, -half, bx, by, half, ax, ay, half);
  }
  return new Float32Array(out);
}

/** The outline of a region's lattice cells as ribbons (mm; not along the tile's own border). */
export function outlineTriangles(r, m, sq, half = 0.5) {
  const step = 1 / m.cells, segs = [];
  const set = (i, j) => i >= 0 && j >= 0 && i < r.nx && j < r.ny && r.bits[j * r.nx + i] === '1';
  const clampX = (v) => Math.min(m.cols, Math.max(0, v)), clampY = (v) => Math.min(m.rows, Math.max(0, v));
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) {
    if (!set(i, j)) continue;
    const x0 = clampX((i - 0.5) * step), x1 = clampX((i + 0.5) * step), y0 = clampY((j - 0.5) * step), y1 = clampY((j + 0.5) * step);
    if (x1 <= x0 || y1 <= y0) continue;
    if (i > 0 && !set(i - 1, j)) segs.push([[x0, y0], [x0, y1]]);
    if (i < r.nx - 1 && !set(i + 1, j)) segs.push([[x1, y0], [x1, y1]]);
    if (j > 0 && !set(i, j - 1)) segs.push([[x0, y0], [x1, y0]]);
    if (j < r.ny - 1 && !set(i, j + 1)) segs.push([[x0, y1], [x1, y1]]);
  }
  const parts = segs.map((g) => ribbonTriangles(g, sq, half));
  const all = new Float32Array(parts.reduce((a, p) => a + p.length, 0));
  let at = 0;
  for (const p of parts) { all.set(p, at); at += p.length; }
  return all;
}

/** A point (squares) as a flat disc of radius `rad` mm, with a ring round it as a ribbon's worth of height. */
export function discTriangles(q, sq, rad = 1.6, sides = 20) {
  const out = [];
  const cx = q[0] * sq, cy = q[1] * sq;
  for (let k = 0; k < sides; k++) {
    const a = (k / sides) * Math.PI * 2, b = ((k + 1) / sides) * Math.PI * 2;
    out.push(cx, cy, 0, cx + rad * Math.cos(a), cy + rad * Math.sin(a), 0, cx + rad * Math.cos(b), cy + rad * Math.sin(b), 0);
  }
  return new Float32Array(out);
}

/** Where to hang a label on a place (squares): the middle of a line, the point itself, the set cell nearest a region's centre. */
export function refAnchor(ref, map) {
  if (ref.at) return ref.at;
  if (ref.kind === 'region') {
    const r = map.regions.find((x) => x.name === ref.name);
    return r ? regionAnchor(r, map) : null;
  }
  const pts = ref.pts || [];
  if (pts.length < 2) return pts[0] || null;
  // a loop (all four edges, the tile) is labelled on its first side
  const closed = pts.length > 2 && pts[0][0] === pts[pts.length - 1][0] && pts[0][1] === pts[pts.length - 1][1];
  const use = closed ? pts.slice(0, 2) : pts;
  const seg = use.slice(1).map((p, k) => Math.hypot(p[0] - use[k][0], p[1] - use[k][1]));
  let left = seg.reduce((a, b) => a + b, 0) / 2;
  for (let k = 0; k < seg.length; k++) {
    if (left <= seg[k] || k === seg.length - 1) {
      const t = seg[k] ? Math.min(1, left / seg[k]) : 0;
      return [use[k][0] + (use[k + 1][0] - use[k][0]) * t, use[k][1] + (use[k + 1][1] - use[k][1]) * t];
    }
    left -= seg[k];
  }
  return use[0];
}

/** The set cell of a region nearest its centre of mass, in squares (null for an empty region). */
export function regionAnchor(r, m) {
  const step = 1 / m.cells;
  let sx = 0, sy = 0, n = 0;
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) if (r.bits[j * r.nx + i] === '1') { sx += i; sy += j; n++; }
  if (!n) return null;
  const cx = sx / n, cy = sy / n;
  let best = null, d = Infinity;
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) {
    if (r.bits[j * r.nx + i] !== '1') continue;
    const e = (i - cx) ** 2 + (j - cy) ** 2;
    if (e < d) { d = e; best = [i, j]; }
  }
  return [Math.min(m.cols, Math.max(0, best[0] * step)), Math.min(m.rows, Math.max(0, best[1] * step))];
}

/** Half the width of a drawn line, mm: thin on a small tile, a little wider on a big one, so about the same on the screen. */
export function lineHalf(map, sq) {
  return Math.min(1, Math.max(0.3, 0.012 * Math.max(map.cols, map.rows) * sq));
}

/**
 * The scene items for the places a region is made from (its `refs`): lines and edges as ribbons, a region
 * as a faint fill inside an outline (its points are markers, drawn with their labels).  Each {positions, color, role: 'line'|'fill', dz}.
 */
export function refItems(map, region, sq, color) {
  const items = [];
  // thin on a small tile, a little wider on a big one: about the same on the screen
  const half = lineHalf(map, sq);
  for (const ref of region.refs || []) {
    // a point is a marker with its label (Overlay.draw), not a disc
    if (ref.kind === 'point') continue;
    if (ref.kind === 'region') {
      const r = map.regions.find((x) => x.name === ref.name);
      if (!r) continue;
      items.push({ positions: regionTriangles(r, map, sq), color, role: 'fill', dz: 0.04 });
      items.push({ positions: outlineTriangles(r, map, sq, 0.7 * half), color, role: 'line', dz: 0.08 });
    } else if (ref.pts) {
      const loop = ref.kind === 'tile' || ref.name === 'edges';
      items.push({ positions: ribbonTriangles(ref.pts, sq, half), color, role: 'line', dz: 0.08, loop });
    }
  }
  return items.filter((i) => i.positions.length);
}


/**
 * A height for every mm of a tile of `footprint` ([w, h] mm) from the triangles `keep(t)` accepts (`positions`: 9 numbers a
 * triangle): the highest corner in each cell, the empty cells filled from their neighbours.  Null when nothing is left to read.
 * What the overlay lays lines and points on, so they lie on the ground and not in the air.
 * @param {ArrayLike<number>} P @param {number} triangles @param {number[]} footprint @param {(t: number) => boolean} [keep]
 */
export function heightGrid(P, triangles, footprint, keep = () => true) {
  const [w, h] = footprint;
  const nx = Math.max(2, Math.ceil(w)), ny = Math.max(2, Math.ceil(h));
  const z = new Float32Array(nx * ny).fill(-Infinity);
  for (let t = 0; t < triangles; t++) {
    if (!keep(t)) continue;
    for (let v = 0; v < 3; v++) {
      const o = 9 * t + 3 * v;
      const i = Math.min(nx - 1, Math.max(0, Math.floor((P[o] / w) * nx)));
      const j = Math.min(ny - 1, Math.max(0, Math.floor((P[o + 1] / h) * ny)));
      if (P[o + 2] > z[j * nx + i]) z[j * nx + i] = P[o + 2];
    }
  }
  // fill the holes a few cells at a time from their neighbours
  for (let pass = 0, holes = true; holes && pass < 64; pass++) {
    holes = false;
    const next = z.slice();
    for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) {
      if (z[j * nx + i] > -Infinity) continue;
      let s = 0, n = 0;
      for (const [di, dj] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const a = i + di, b = j + dj;
        if (a >= 0 && b >= 0 && a < nx && b < ny && z[b * nx + a] > -Infinity) { s += z[b * nx + a]; n++; }
      }
      if (n) next[j * nx + i] = s / n; else holes = true;
    }
    z.set(next);
  }
  return z[0] > -Infinity ? { z, nx, ny, w, h } : null;
}

/** The height of a `heightGrid` at (x, y) mm, or `fallback` without one. */
export function heightAtGrid(H, x, y, fallback) {
  if (!H) return fallback;
  const i = Math.min(H.nx - 1, Math.max(0, Math.floor((x / H.w) * H.nx)));
  const j = Math.min(H.ny - 1, Math.max(0, Math.floor((y / H.h) * H.ny)));
  return H.z[j * H.nx + i];
}
