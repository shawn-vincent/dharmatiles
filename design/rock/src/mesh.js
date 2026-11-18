/* mesh.js — owns the two ways a stone becomes triangles.
   TWO DRAWING PATHS, one stated reason each. POLYTOPE: when nothing is rounded or marked, the pieces are drawn as they are, flat faces
   exactly, no sampling. SAMPLED: when wear or a mark is on, the surface is implicit (rounding, dishes and grain need a field), so it is
   sampled on a grid of `vox` mm and meshed by surface nets. Both read the same pieces. Never a third path.
   Mesh { pos, nrm, idx, tris, ... }. Nothing is ever dropped: the report lists islands (parts of at least one voxel^3: a failure) and
   specks (smaller, with their size). */
var Stone = Stone || {};
(function (S) {
  const abs = Math.abs, min = Math.min, max = Math.max, sqrt = Math.sqrt;
  const NEWTON_STEPS = 1;           // one step of position refinement per vertex: the grid already put it within a voxel
  const QEF_PULL = 0.04;            // how hard a vertex is held to the mean of its crossings, against the planes: small, so a crease can pull it onto the ridge
  const QEF_OUT = 0.25;             // a vertex may leave its cell by this much (of a voxel) toward a crease
  const GRAD_EPS = 0.5;             // gradient taken this fraction of a voxel away
  const TETRA = [[1, 1, 1], [1, -1, -1], [-1, 1, -1], [-1, -1, 1]];   // four points whose directions give the gradient: sum e_i f_i / (4 eps)

  /* every face of every piece, as it is. all: also the fallen pieces (the Chunks view). Per vertex: the piece it belongs to. */
  function polytopeMesh(pieces, all) {
    const pos = [], nrm = [], idx = [], pid = [];
    for (const p of pieces) {
      if (!all && p.state !== 'standing') continue;
      for (const f of p.convex.faces) {
        const base = pos.length / 3;
        for (const q of f.pts) { pos.push(q[0], q[1], q[2]); nrm.push(f.n[0], f.n[1], f.n[2]); pid.push(p.id); }
        for (let i = 1; i + 1 < f.pts.length; i++) idx.push(base, base + i, base + i + 1);
      }
    }
    return { pos: new Float32Array(pos), nrm: new Float32Array(nrm), idx: new Uint32Array(idx), pieceOf: new Int32Array(pid), tris: idx.length / 3, path: 'polytope', grid: null };
  }

  /* surface nets over field f at voxel vox. Returns the mesh and its grid (kept for the viewer's occlusion) */
  function sampledMesh(f, vox, onp, manifold) {
    const pad = f.reach + 2 * vox, lo = f.bounds.lo.map((v) => v - pad), hi = f.bounds.hi.map((v) => v + pad);
    const nx = Math.ceil((hi[0] - lo[0]) / vox) + 1, ny = Math.ceil((hi[1] - lo[1]) / vox) + 1, nz = Math.ceil((hi[2] - lo[2]) / vox) + 1;
    const ox = lo[0], oy = lo[1], oz = lo[2], N = nx * ny * nz, V = new Float32Array(N);
    const BS = 4, half = BS * vox * 0.866, band = half + f.reach + 0.5;       // a block farther than this from the surface is all one sign
    for (let bz = 0; bz < nz; bz += BS) for (let by = 0; by < ny; by += BS) for (let bx = 0; bx < nx; bx += BS) {
      if (onp && bx === 0 && by === 0) onp(0.6 * bz / nz);
      const fc = f.coarse(ox + (bx + BS / 2) * vox, oy + (by + BS / 2) * vox, oz + (bz + BS / 2) * vox), far = abs(fc) > band;
      for (let z = bz; z < min(bz + BS, nz); z++) for (let y = by; y < min(by + BS, ny); y++) for (let x = bx; x < min(bx + BS, nx); x++)
        V[x + nx * (y + ny * z)] = far ? (fc > 0 ? band : -band) : f(ox + x * vox, oy + y * vox, oz + z * vox);
    }
    // A CELL'S SURFACE MUST BE A MANIFOLD (the wall's union reads it as one; `manifold` is true when the stone fills a cell): a grid face whose four corners alternate in sign would join two quads' vertices by one edge four times. Such a face is
    // resolved by taking its two outside corners in (a value just inside the surface), and again until none is left, so every edge is shared by two faces and a mesh is a closed manifold.
    const FILL = -1e-4 * vox, at3 = (x, y, z) => x + nx * (y + ny * z);
    for (let pass = 0; pass < (manifold ? 6 : 0); pass++) {
      let fixed = 0;
      const tryFace = (i0, i1, i2, i3) => { const a = V[i0] < 0, b = V[i1] < 0, c = V[i2] < 0, d = V[i3] < 0; if (a === c && b === d && a !== b) { const out = a ? [i1, i3] : [i0, i2]; V[out[0]] = FILL; V[out[1]] = FILL; fixed++; } };
      for (let z = 0; z < nz; z++) for (let y = 0; y < ny; y++) for (let x = 0; x < nx; x++) {
        if (x + 1 < nx && y + 1 < ny) tryFace(at3(x, y, z), at3(x + 1, y, z), at3(x + 1, y + 1, z), at3(x, y + 1, z));
        if (x + 1 < nx && z + 1 < nz) tryFace(at3(x, y, z), at3(x + 1, y, z), at3(x + 1, y, z + 1), at3(x, y, z + 1));
        if (y + 1 < ny && z + 1 < nz) tryFace(at3(x, y, z), at3(x, y + 1, z), at3(x, y + 1, z + 1), at3(x, y, z + 1));
      }
      if (!fixed) break;
    }
    const vid = new Int32Array(N).fill(-1), pos = [];
    const ng = [0, 0, 0], clampI = (i, n) => (i < 0 ? 0 : i >= n ? n - 1 : i), Vat = (i, j, k) => V[clampI(i, nx) + nx * (clampI(j, ny) + ny * clampI(k, nz))];
    const nodeGrad = (i, j, k) => { ng[0] = Vat(i + 1, j, k) - Vat(i - 1, j, k); ng[1] = Vat(i, j + 1, k) - Vat(i, j - 1, k); ng[2] = Vat(i, j, k + 1) - Vat(i, j, k - 1); return ng; };   // the grid's gradient at a node (reused array)
    const C = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
    const E = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
    for (let z = 0; z < nz - 1; z++) for (let y = 0; y < ny - 1; y++) for (let x = 0; x < nx - 1; x++) {
      const v = []; let neg = 0;
      for (let c = 0; c < 8; c++) { const o = C[c], val = V[(x + o[0]) + nx * ((y + o[1]) + ny * (z + o[2]))]; v.push(val); if (val < 0) neg++; }
      if (neg === 0 || neg === 8) continue;
      // The vertex is the point that best lies on the planes through the edge crossings, each with the surface's direction there (read off the grid: the corners' central differences,
      // blended along the edge), pulled gently toward the crossings' mean: a smooth surface gives about the mean; at a crease, where two walls meet, the planes of both walls
      // meet in the crease line and the vertex lands on it, so a ridge comes out straight and sharp, not as teeth. One placement for every cell, creases included.
      let cxs = 0, cys = 0, czs = 0, cnt = 0; const A = [0, 0, 0, 0, 0, 0], B = [0, 0, 0];     // A: xx xy xz yy yz zz
      for (const e of E) {
        const a = v[e[0]], b = v[e[1]]; if ((a < 0) === (b < 0)) continue;
        const t = a / (a - b), oa = C[e[0]], ob = C[e[1]], px = oa[0] + (ob[0] - oa[0]) * t, py = oa[1] + (ob[1] - oa[1]) * t, pz = oa[2] + (ob[2] - oa[2]) * t;
        const ga = nodeGrad(x + oa[0], y + oa[1], z + oa[2]), gb = nodeGrad(x + ob[0], y + ob[1], z + ob[2]);
        let nx_ = ga[0] + (gb[0] - ga[0]) * t, ny_ = ga[1] + (gb[1] - ga[1]) * t, nz_ = ga[2] + (gb[2] - ga[2]) * t; const nl = sqrt(nx_ * nx_ + ny_ * ny_ + nz_ * nz_) || 1; nx_ /= nl; ny_ /= nl; nz_ /= nl;
        const d = nx_ * px + ny_ * py + nz_ * pz;
        A[0] += nx_ * nx_; A[1] += nx_ * ny_; A[2] += nx_ * nz_; A[3] += ny_ * ny_; A[4] += ny_ * nz_; A[5] += nz_ * nz_; B[0] += nx_ * d; B[1] += ny_ * d; B[2] += nz_ * d;
        cxs += px; cys += py; czs += pz; cnt++;
      }
      const mx = cxs / cnt, my = cys / cnt, mz = czs / cnt, mu = QEF_PULL * cnt;
      A[0] += mu; A[3] += mu; A[5] += mu; B[0] += mu * mx; B[1] += mu * my; B[2] += mu * mz;
      const det = A[0] * (A[3] * A[5] - A[4] * A[4]) - A[1] * (A[1] * A[5] - A[4] * A[2]) + A[2] * (A[1] * A[4] - A[3] * A[2]);
      let sx = mx, sy = my, sz = mz;
      if (Math.abs(det) > 1e-9) {
        sx = (B[0] * (A[3] * A[5] - A[4] * A[4]) - A[1] * (B[1] * A[5] - A[4] * B[2]) + A[2] * (B[1] * A[4] - A[3] * B[2])) / det;
        sy = (A[0] * (B[1] * A[5] - A[4] * B[2]) - B[0] * (A[1] * A[5] - A[4] * A[2]) + A[2] * (A[1] * B[2] - B[1] * A[2])) / det;
        sz = (A[0] * (A[3] * B[2] - B[1] * A[4]) - A[1] * (A[1] * B[2] - B[1] * A[2]) + B[0] * (A[1] * A[4] - A[3] * A[2])) / det;
      }
      sx = max(-QEF_OUT, min(1 + QEF_OUT, sx)); sy = max(-QEF_OUT, min(1 + QEF_OUT, sy)); sz = max(-QEF_OUT, min(1 + QEF_OUT, sz));
      vid[x + nx * (y + ny * z)] = pos.length / 3;
      pos.push(ox + (x + sx) * vox, oy + (y + sy) * vox, oz + (z + sz) * vox);
    }
    const idx = [], at = (x, y, z) => vid[x + nx * (y + ny * z)];
    const quad = (a, b, c, d, flip) => { if (a < 0 || b < 0 || c < 0 || d < 0) return; if (flip) idx.push(a, c, b, a, d, c); else idx.push(a, b, c, a, c, d); };
    for (let z = 1; z < nz - 1; z++) for (let y = 1; y < ny - 1; y++) for (let x = 1; x < nx - 1; x++) {
      const v0 = V[x + nx * (y + ny * z)] < 0;
      if (v0 !== (V[(x + 1) + nx * (y + ny * z)] < 0)) quad(at(x, y - 1, z - 1), at(x, y, z - 1), at(x, y, z), at(x, y - 1, z), v0);
      if (v0 !== (V[x + nx * ((y + 1) + ny * z)] < 0)) quad(at(x - 1, y, z - 1), at(x - 1, y, z), at(x, y, z), at(x, y, z - 1), v0);
      if (v0 !== (V[x + nx * (y + ny * (z + 1))] < 0)) quad(at(x - 1, y - 1, z), at(x, y - 1, z), at(x, y, z), at(x - 1, y, z), v0);
    }
    // refine: one Newton step toward f = 0, and the normal from the tetrahedral gradient (5 evaluations a vertex)
    const P3 = new Float32Array(pos), nv = P3.length / 3, nrm = new Float32Array(P3.length), eps = vox * GRAD_EPS, id = [0], pieceOf = new Int32Array(nv);
    for (let i = 0; i < nv; i++) {
      if (onp && (i & 2047) === 0) onp(0.62 + 0.36 * i / nv);
      let x = P3[3 * i], y = P3[3 * i + 1], z = P3[3 * i + 2];
      for (let it = 0; it < NEWTON_STEPS; it++) {
        const f0 = f(x, y, z, id); let gx = 0, gy = 0, gz = 0;
        for (const e of TETRA) { const fv = f(x + e[0] * eps, y + e[1] * eps, z + e[2] * eps); gx += e[0] * fv; gy += e[1] * fv; gz += e[2] * fv; }
        const gl = sqrt(gx * gx + gy * gy + gz * gz) || 1, step = max(-vox, min(vox, f0 * 4 * eps / gl));   // |grad| = gl / (4 eps)
        nrm[3 * i] = gx / gl; nrm[3 * i + 1] = gy / gl; nrm[3 * i + 2] = gz / gl;
        x -= nrm[3 * i] * step; y -= nrm[3 * i + 1] * step; z -= nrm[3 * i + 2] * step;
      }
      P3[3 * i] = x; P3[3 * i + 1] = y; P3[3 * i + 2] = z; pieceOf[i] = id[0];
    }
    const idxM = new Uint32Array(idx);                    // the winding the quads were made with: one orientation all over, the mesh a union reads
    for (let t = 0; t < idx.length; t += 3) {            // wind each triangle to agree with its vertex normals
      const a = idx[t] * 3, b = idx[t + 1] * 3, c = idx[t + 2] * 3;
      const ux = P3[b] - P3[a], uy = P3[b + 1] - P3[a + 1], uz = P3[b + 2] - P3[a + 2], vx = P3[c] - P3[a], vy = P3[c + 1] - P3[a + 1], vz = P3[c + 2] - P3[a + 2];
      const cx = uy * vz - uz * vy, cy = uz * vx - ux * vz, cz = ux * vy - uy * vx;
      if (cx * (nrm[a] + nrm[b] + nrm[c]) + cy * (nrm[a + 1] + nrm[b + 1] + nrm[c + 1]) + cz * (nrm[a + 2] + nrm[b + 2] + nrm[c + 2]) < 0) { const q = idx[t + 1]; idx[t + 1] = idx[t + 2]; idx[t + 2] = q; }
    }
    { let vol = 0; for (let t = 0; t < idxM.length; t += 3) { const a = idxM[t] * 3, b = idxM[t + 1] * 3, c = idxM[t + 2] * 3; vol += P3[a] * (P3[b + 1] * P3[c + 2] - P3[b + 2] * P3[c + 1]) - P3[a + 1] * (P3[b] * P3[c + 2] - P3[b + 2] * P3[c]) + P3[a + 2] * (P3[b] * P3[c + 1] - P3[b + 1] * P3[c]); }
      if (vol < 0) for (let t = 0; t < idxM.length; t += 3) { const q = idxM[t + 1]; idxM[t + 1] = idxM[t + 2]; idxM[t + 2] = q; } }   // outward: positive volume
    return { pos: P3, nrm, idx: new Uint32Array(idx), idxM, pieceOf, tris: idx.length / 3, path: 'sampled', vox, grid: { V, nx, ny, nz, ox, oy, oz, vox, band } };
  }

  /* the connected parts of a mesh. The largest is the stone; every other is an island (size >= one voxel across) or a speck (smaller). Reports, never removes. */
  function parts(mesh, vox) {
    const idx = mesh.idx, nv = mesh.pos.length / 3, par = new Int32Array(nv); for (let i = 0; i < nv; i++) par[i] = i;
    const find = (a) => { while (par[a] !== a) { par[a] = par[par[a]]; a = par[a]; } return a; };
    for (let t = 0; t < idx.length; t += 3) { const a = find(idx[t]); par[find(idx[t + 1])] = a; par[find(idx[t + 2])] = a; }
    const info = new Map();
    for (let t = 0; t < idx.length; t += 3) {
      const r = find(idx[t]); let c = info.get(r); if (!c) info.set(r, c = { tris: 0, lo: [1e9, 1e9, 1e9], hi: [-1e9, -1e9, -1e9] });
      c.tris++; for (let j = 0; j < 3; j++) for (let a = 0; a < 3; a++) { const v = mesh.pos[idx[t + j] * 3 + a]; if (v < c.lo[a]) c.lo[a] = v; if (v > c.hi[a]) c.hi[a] = v; }
    }
    let big = null; for (const [r, c] of info) if (!big || c.tris > info.get(big).tris) big = r;
    const islands = [], specks = [];
    for (const [r, c] of info) if (r !== big) {
      const size = Math.cbrt(max(0, (c.hi[0] - c.lo[0]) * (c.hi[1] - c.lo[1]) * (c.hi[2] - c.lo[2]))), rec = { tris: c.tris, size, at: [0, 1, 2].map((a) => (c.lo[a] + c.hi[a]) / 2) };
      (size >= vox ? islands : specks).push(rec);
    }
    return { islands, specks };
  }
  S.mesh = { polytopeMesh, sampledMesh, parts };
})(Stone);
