"""Low-level mesh primitives: terrain solid and heightmap export."""
from __future__ import annotations

import pathlib

import numpy as np
import trimesh


# ── Terrain solid ─────────────────────────────────────────────────────────────

def _rdp_edge(z_arr: np.ndarray, threshold: float) -> list[int]:
    """Ramer-Douglas-Peucker simplification of a 1-D z-profile.

    Returns sorted indices into *z_arr* to keep.  Always includes 0 and
    ``len(z_arr)-1``.  Intermediate points are kept only when the perpendicular
    deviation (in z) from the chord between the current endpoints exceeds
    *threshold* mm.
    """
    n = len(z_arr)
    if n <= 2:
        return list(range(n))
    keep: set[int] = {0, n - 1}
    stack = [(0, n - 1)]
    while stack:
        i, j = stack.pop()
        if j - i <= 1:
            continue
        idxs  = np.arange(i + 1, j)
        t     = (idxs - i) / (j - i)
        z_lin = z_arr[i] + (z_arr[j] - z_arr[i]) * t
        dev   = np.abs(z_arr[i + 1 : j] - z_lin)
        m     = int(np.argmax(dev)) + i + 1
        if dev[m - i - 1] > threshold:
            keep.add(m)
            stack.append((i, m))
            stack.append((m, j))
    return sorted(keep)


def _make_heightmap_solid_adaptive(
    z_grid: np.ndarray,
    tile_w: float, tile_h: float,
    base_h: float,
    threshold: float,
    stride: int = 16,
) -> trimesh.Trimesh:
    """Adaptive terrain mesh: dense at high-curvature areas, sparse on flat ground.

    Boundary ring
    -------------
    Each of the 4 tile edges is simplified with Ramer-Douglas-Peucker at
    ``threshold * 8``.  At 35 mm / 256 cells, soil bumps reaching the tile edge
    are attenuated to ≈ 0.05-0.10 mm; the coarser threshold collapses them so
    each near-flat edge section becomes a single trapezoid on the side wall (a
    few triangles per face instead of 255).  The same simplified ring is used
    for both the top-surface Delaunay and the side walls so no T-junctions form.

    Interior vertex selection
    -------------------------
    1. Coarse background grid (every *stride* cells) — bounds max triangle size.
    2. Interior grid points where |∇²z| > *threshold* — soil bumps and breaks.

    The bottom is a fan from one centre vertex to the simplified ring.
    """
    from scipy.spatial import Delaunay

    nrows, ncols = z_grid.shape
    gx = tile_w / max(ncols - 1, 1)
    gy = tile_h / max(nrows - 1, 1)
    side_thr = threshold * 8   # aggressive: collapses edge bumps < ~0.16 mm

    # ── RDP-simplified boundary ring (shared by top Delaunay + side walls) ────
    sk = _rdp_edge(z_grid[0,  :],         side_thr)  # south: col 0 → ncols-1
    ek = _rdp_edge(z_grid[:,  ncols - 1], side_thr)  # east:  row 0 → nrows-1
    nk = _rdp_edge(z_grid[nrows-1, ::-1], side_thr)  # north: col ncols-1 → 0
    wk = _rdp_edge(z_grid[::-1, 0],       side_thr)  # west:  row nrows-1 → 0

    south = [(0,        c)         for c in sk]
    east  = [(r,        ncols - 1) for r in ek]
    north = [(nrows-1,  ncols-1-k) for k in nk]
    west  = [(nrows-1-k, 0)        for k in wk]

    bdr: list[tuple[int, int]] = south[:-1] + east[:-1] + north[:-1] + west[:-1]
    n_bdr   = len(bdr)
    bdr_set = set(bdr)

    # ── Interior points ───────────────────────────────────────────────────────
    bg_pts: list[tuple[int, int]] = [
        (int(r), int(c))
        for r in range(stride, nrows - 1, stride)
        for c in range(stride, ncols - 1, stride)
    ]
    if nrows > 2 and ncols > 2:
        lap = np.abs(
            z_grid[2:,  1:-1] + z_grid[:-2,  1:-1] +
            z_grid[1:-1, 2:] + z_grid[1:-1, :-2] -
            4.0 * z_grid[1:-1, 1:-1]
        )
        ir, ic  = np.where(lap > threshold)
        lap_pts = [(int(r) + 1, int(c) + 1) for r, c in zip(ir, ic)]
    else:
        lap_pts = []

    interior_set = (set(bg_pts) | set(lap_pts)) - bdr_set
    pts: list[tuple[int, int]] = bdr + sorted(interior_set)
    n_top = len(pts)

    xy  = np.array([(c * gx, r * gy) for r, c in pts])
    z_t = np.array([float(z_grid[r, c]) for r, c in pts])

    # ── 2-D Delaunay → top surface ────────────────────────────────────────────
    top_faces   = Delaunay(xy).simplices.astype(np.int32)
    n_top_faces = len(top_faces)

    # ── Vertex buffer ─────────────────────────────────────────────────────────
    # [0 .. n_top-1]          top (simplified bdr + interior)
    # [n_top .. n_top+n_bdr-1]  bottom ring (same XY as bdr, z = -base_h)
    # [n_top+n_bdr]             bottom centre
    xy_bdr  = xy[:n_bdr]
    bot_ctr = n_top + n_bdr

    verts = np.empty((n_top + n_bdr + 1, 3))
    verts[:n_top,              :2] = xy
    verts[:n_top,               2] = z_t
    verts[n_top:bot_ctr,       :2] = xy_bdr
    verts[n_top:bot_ctr,        2] = -base_h
    verts[bot_ctr]                  = [tile_w / 2.0, tile_h / 2.0, -base_h]

    # ── Side walls (vectorised) ───────────────────────────────────────────────
    ks  = np.arange(n_bdr, dtype=np.int32)
    ks1 = (ks + 1) % n_bdr
    side_arr = np.empty((2 * n_bdr, 3), dtype=np.int32)
    side_arr[0::2] = np.stack([ks,       n_top + ks,  ks1      ], axis=1)
    side_arr[1::2] = np.stack([ks1,      n_top + ks,  n_top + ks1], axis=1)

    # ── Bottom fan (centre → simplified ring) ─────────────────────────────────
    bot_arr = np.empty((n_bdr, 3), dtype=np.int32)
    bot_arr[:, 0] = bot_ctr
    bot_arr[:, 1] = n_top + ks1
    bot_arr[:, 2] = n_top + ks

    all_faces = np.vstack([top_faces, side_arr, bot_arr])
    mesh = trimesh.Trimesh(vertices=verts, faces=all_faces.astype(int),
                           process=False)
    mesh.metadata['top_face_count'] = n_top_faces
    mesh.fix_normals()
    return mesh


def make_heightmap_solid(z_grid: np.ndarray, tile_w: float, tile_h: float,
                         base_h: float, subsample: int = 1,
                         omit_top_mask: np.ndarray | None = None,
                         error_threshold: float | None = None,
                         simplify_stride: int = 16,
                         ) -> trimesh.Trimesh:
    """Watertight solid: top = *z_grid* surface, bottom = flat at −*base_h*.

    Parameters
    ----------
    z_grid   : (grid_h, grid_w) terrain heights in mm.
    tile_w/h : tile dimensions in mm.
    base_h   : depth of the solid slab below terrain in mm (positive value).
    subsample: take every Nth grid sample for the mesh (1 = full resolution).
    omit_top_mask: optional bool grid. Top quads whose four sampled corners are
        True are omitted so another explicit layer, such as water, can cap that
        region instead of duplicating coplanar terrain faces.
    error_threshold : float | None
        When set, use adaptive Laplacian-based triangulation instead of the
        uniform grid.  Interior vertices are kept only where |∇²z| exceeds
        this value (mm).  Incompatible with omit_top_mask (falls back to
        uniform grid when a mask is supplied).
    simplify_stride : int
        Coarse background grid spacing in cells for the adaptive path.
    """
    # ── Adaptive path ─────────────────────────────────────────────────────────
    if error_threshold is not None and omit_top_mask is None:
        return _make_heightmap_solid_adaptive(
            z_grid, tile_w, tile_h, base_h,
            threshold=error_threshold,
            stride=simplify_stride,
        )

    nrows, ncols = z_grid.shape
    sr_arr = np.arange(0, ncols, subsample)
    if sr_arr[-1] != ncols - 1:
        sr_arr = np.append(sr_arr, ncols - 1)
    sc_arr = np.arange(0, nrows, subsample)
    if sc_arr[-1] != nrows - 1:
        sc_arr = np.append(sc_arr, nrows - 1)
    ns_c = len(sr_arr)   # sampled cols
    ns_r = len(sc_arr)   # sampled rows
    gx   = tile_w / max(ncols - 1, 1)
    gy   = tile_h / max(nrows - 1, 1)

    # ── Build all vertices in one shot ────────────────────────────────────────
    # Top: ns_r × ns_c vertices.  Bottom: same layout at z = -base_h.
    # Flat index: top[jj, ii] = jj*ns_c + ii;  bot[jj, ii] = ns_r*ns_c + jj*ns_c + ii
    n_top_v  = ns_r * ns_c
    n_verts  = 2 * n_top_v
    verts    = np.empty((n_verts, 3), dtype=float)

    JJ, II   = np.mgrid[0:ns_r, 0:ns_c]
    I_idx    = sr_arr[II]    # actual column indices into z_grid
    J_idx    = sc_arr[JJ]    # actual row indices

    verts[:n_top_v, 0] = (I_idx * gx).ravel()
    verts[:n_top_v, 1] = (J_idx * gy).ravel()
    verts[:n_top_v, 2] = z_grid[J_idx, I_idx].ravel()

    verts[n_top_v:, 0] = verts[:n_top_v, 0]
    verts[n_top_v:, 1] = verts[:n_top_v, 1]
    verts[n_top_v:, 2] = -base_h

    # Flat vertex index helpers
    def top_v(ii, jj): return jj * ns_c + ii
    def bot_v(ii, jj): return n_top_v + jj * ns_c + ii

    # ── Build all faces vectorised ────────────────────────────────────────────
    # Grid of quad corners: (ns_r-1) × (ns_c-1) quads
    q_r, q_c = np.mgrid[0:ns_r - 1, 0:ns_c - 1]   # quad row/col indices

    # Flat vertex indices for quad corners (broadcast over all quads at once)
    a_top = top_v(q_c,     q_r    )   # (ns_r-1, ns_c-1) each
    b_top = top_v(q_c + 1, q_r    )
    c_top = top_v(q_c,     q_r + 1)
    d_top = top_v(q_c + 1, q_r + 1)

    # ── Top surface ───────────────────────────────────────────────────────────
    if omit_top_mask is not None:
        # Omit quads where all four sampled corners are masked
        j0 = sc_arr[q_r];      j1 = sc_arr[q_r + 1]
        i0 = sr_arr[q_c];      i1 = sr_arr[q_c + 1]
        omit = (omit_top_mask[j0, i0] & omit_top_mask[j0, i1] &
                omit_top_mask[j1, i0] & omit_top_mask[j1, i1])
        keep = ~omit
        a_k, b_k, c_k, d_k = (a_top[keep], b_top[keep],
                                c_top[keep], d_top[keep])
    else:
        a_k, b_k, c_k, d_k = (a_top.ravel(), b_top.ravel(),
                                c_top.ravel(), d_top.ravel())

    n_top_quads = len(a_k)
    top_faces = np.empty((n_top_quads * 2, 3), dtype=np.int32)
    top_faces[0::2] = np.stack([a_k, b_k, d_k], axis=1)
    top_faces[1::2] = np.stack([a_k, d_k, c_k], axis=1)

    # ── Bottom surface ────────────────────────────────────────────────────────
    a_bot = bot_v(q_c,     q_r    ).ravel()
    b_bot = bot_v(q_c + 1, q_r    ).ravel()
    c_bot = bot_v(q_c,     q_r + 1).ravel()
    d_bot = bot_v(q_c + 1, q_r + 1).ravel()
    n_bot_quads = len(a_bot)
    bot_faces = np.empty((n_bot_quads * 2, 3), dtype=np.int32)
    bot_faces[0::2] = np.stack([a_bot, d_bot, b_bot], axis=1)
    bot_faces[1::2] = np.stack([a_bot, c_bot, d_bot], axis=1)

    # ── Side walls ────────────────────────────────────────────────────────────
    ii_sw = np.arange(ns_c - 1)
    jj_sw = np.arange(ns_r - 1)

    # South wall (jj=0): 2 triangles per column segment
    sw_s = np.empty((2 * (ns_c - 1), 3), dtype=np.int32)
    sw_s[0::2] = np.stack([top_v(ii_sw, 0),     bot_v(ii_sw, 0),     top_v(ii_sw + 1, 0)], axis=1)
    sw_s[1::2] = np.stack([top_v(ii_sw + 1, 0), bot_v(ii_sw, 0),     bot_v(ii_sw + 1, 0)], axis=1)

    # North wall (jj=ns_r-1)
    sw_n = np.empty((2 * (ns_c - 1), 3), dtype=np.int32)
    sw_n[0::2] = np.stack([top_v(ii_sw, ns_r-1), top_v(ii_sw+1, ns_r-1), bot_v(ii_sw, ns_r-1)],   axis=1)
    sw_n[1::2] = np.stack([top_v(ii_sw+1, ns_r-1),bot_v(ii_sw+1, ns_r-1),bot_v(ii_sw, ns_r-1)],   axis=1)

    # West wall (ii=0)
    sw_w = np.empty((2 * (ns_r - 1), 3), dtype=np.int32)
    sw_w[0::2] = np.stack([top_v(0, jj_sw),     top_v(0, jj_sw + 1), bot_v(0, jj_sw)],     axis=1)
    sw_w[1::2] = np.stack([top_v(0, jj_sw + 1), bot_v(0, jj_sw + 1), bot_v(0, jj_sw)],     axis=1)

    # East wall (ii=ns_c-1)
    sw_e = np.empty((2 * (ns_r - 1), 3), dtype=np.int32)
    sw_e[0::2] = np.stack([top_v(ns_c-1, jj_sw),  bot_v(ns_c-1, jj_sw),  top_v(ns_c-1, jj_sw+1)], axis=1)
    sw_e[1::2] = np.stack([top_v(ns_c-1, jj_sw+1),bot_v(ns_c-1, jj_sw),  bot_v(ns_c-1, jj_sw+1)], axis=1)

    faces = np.concatenate([top_faces, bot_faces, sw_s, sw_n, sw_w, sw_e])

    mesh = trimesh.Trimesh(vertices=verts,
                           faces=faces.astype(int),
                           process=False)
    mesh.metadata['top_face_count'] = n_top_quads * 2
    mesh.fix_normals()
    return mesh

