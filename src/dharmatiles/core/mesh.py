"""
Low-level mesh primitives: frame computation, blade tube, terrain solid.
"""
from __future__ import annotations

import numpy as np
import trimesh


# ── Frame computation ─────────────────────────────────────────────────────────

def compute_up_locs(path_xyz: np.ndarray) -> np.ndarray:
    """World-horizontal perpendicular-to-spine unit vectors.

    Returns (n_pts, 3) array; Z component is always 0.
    Matches the horizontal basis used in :func:`build_tube_mesh`.
    """
    path = np.asarray(path_xyz, dtype=float)
    tangs = np.empty_like(path)
    tangs[:-1] = path[1:] - path[:-1]
    tangs[-1]  = path[-1] - path[-2]
    txy_norm   = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2) + 1e-9
    has_xy     = txy_norm > 1e-6
    up         = np.zeros_like(path)
    up[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up[~has_xy]   = [1.0, 0.0, 0.0]
    return up


def blade_frame(path: np.ndarray):
    """Return (tangents, up_locs, down_locs) unit-vector arrays for each ring.

    ``down_locs`` points downward (away from the upper face), perpendicular to
    both the tangent and the world-horizontal up_loc.
    """
    path = np.asarray(path, dtype=float)
    tangs = np.empty_like(path)
    tangs[:-1] = path[1:] - path[:-1]
    tangs[-1]  = path[-1] - path[-2]
    t_norms    = np.linalg.norm(tangs, axis=1, keepdims=True) + 1e-9
    tangs     /= t_norms

    txy_norm = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2)
    has_xy   = txy_norm > 1e-6
    up_locs  = np.zeros_like(path)
    up_locs[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up_locs[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up_locs[~has_xy]   = [1.0, 0.0, 0.0]

    down_locs = np.cross(up_locs, tangs)
    down_norms = np.linalg.norm(down_locs, axis=1, keepdims=True) + 1e-9
    down_locs /= down_norms
    down_locs[down_locs[:, 2] > 0.0] *= -1.0   # always point downward
    return tangs, up_locs, down_locs


# ── Blade tube mesh ───────────────────────────────────────────────────────────

def build_tube_mesh(spine_3d: np.ndarray, widths: np.ndarray,
                    thickness: float,
                    cross_section: str = 'triangle',
                    n_segs: int = 8) -> trimesh.Trimesh:
    """Watertight tube mesh following *spine_3d*.

    cross_section='triangle' (default)
        3 verts / ring:
          V0 — lower hull apex  (spine + thickness * down_loc)
          V1 — right top edge   (spine + half_w * up_loc)
          V2 — left  top edge   (spine − half_w * up_loc)
        The top face (V1–V2 strip) sits on the support curve; the apex hangs
        *thickness* mm below it.

    cross_section='circle'
        n_segs verts / ring, uniformly distributed around the spine.
        Radius = half_width at each ring.  Vertex 0 is at the *up_loc* side
        (top), vertex n_segs//4 is at the *down_loc* side (bottom).
        The spine point is the tube centre, not the top surface.
    """
    path  = np.asarray(spine_3d, dtype=float)   # (n_pts, 3)
    W_arr = np.asarray(widths,   dtype=float)    # (n_pts,)
    n_pts = len(path)

    _, up_locs, down_locs = blade_frame(path)
    half_W = W_arr / 2.0                         # (n_pts,)

    if cross_section == 'triangle':
        n = 3
        ring_v = np.stack([
            path + thickness * down_locs,          # V0: lower apex
            path + half_W[:, None] * up_locs,      # V1: right edge
            path - half_W[:, None] * up_locs,      # V2: left  edge
        ], axis=1)                                 # (n_pts, 3, 3)

    elif cross_section == 'circle':
        n = max(3, int(n_segs))
        thetas = 2 * np.pi * np.arange(n) / n    # (n,) — vertex 0 at up_loc ("top")
        cos_t  = np.cos(thetas)                   # (n,)
        sin_t  = np.sin(thetas)                   # (n,)
        # ring_v[p, i] = spine[p] + R[p] * (cos_t[i]*up[p] + sin_t[i]*down[p])
        # Broadcasting: (n_pts,1,3) + (n_pts,1,1)*(1,n,1)*(n_pts,1,3)
        ring_v = (path[:, None, :] +
                  half_W[:, None, None] *
                  (cos_t[None, :, None] * up_locs[:, None, :] +
                   sin_t[None, :, None] * down_locs[:, None, :]))  # (n_pts, n, 3)

    else:
        raise ValueError(f"Unknown cross_section {cross_section!r}; use 'triangle' or 'circle'")

    # Pre-allocate vertex and face buffers now that n is known
    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = fi = 0

    for i in range(n_pts):
        verts[vi:vi + n] = ring_v[i];  vi += n

    v_base = vi;  verts[vi] = path[0];   vi += 1
    v_tip  = vi;  verts[vi] = path[-1];  vi += 1

    # Base cap
    for i in range(n):
        faces[fi] = [v_base, i, (i + 1) % n];  fi += 1  # inward-facing

    # Side quads — two triangles per quad
    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for i in range(n):
            i1 = (i + 1) % n
            faces[fi]   = [ra + i,  rb + i,  ra + i1];  fi += 1
            faces[fi]   = [ra + i1, rb + i,  rb + i1];  fi += 1

    # Tip cap
    rl = (n_pts - 1) * n
    for i in range(n):
        faces[fi] = [rl + i, rl + (i + 1) % n, v_tip];  fi += 1

    mesh = trimesh.Trimesh(vertices=verts[:vi],
                           faces=faces[:fi].astype(int),
                           process=False)
    mesh.fix_normals()
    return mesh


# ── Terrain solid ─────────────────────────────────────────────────────────────

def make_heightmap_solid(z_grid: np.ndarray, tile_w: float, tile_h: float,
                         base_h: float, subsample: int = 4) -> trimesh.Trimesh:
    """Watertight solid: top = *z_grid* surface, bottom = flat at −*base_h*.

    Parameters
    ----------
    z_grid   : (GRID_RES, GRID_RES) terrain heights in mm.
    tile_w/h : tile dimensions in mm.
    base_h   : depth of the solid slab below terrain in mm (positive value).
    subsample: take every Nth grid sample for the mesh (reduces triangle count).
    """
    res = z_grid.shape[0]
    sr = list(range(0, res, subsample))
    if sr[-1] != res - 1:
        sr.append(res - 1)
    ns  = len(sr)
    gx  = tile_w / (res - 1)
    gy  = tile_h / (res - 1)

    verts: list = []
    faces: list = []

    # ── Top surface ────────────────────────────────────────────────────────────
    top_idx: dict = {}
    for jj, j in enumerate(sr):
        for ii, i in enumerate(sr):
            top_idx[(ii, jj)] = len(verts)
            verts.append([i * gx, j * gy, z_grid[j, i]])

    # ── Bottom surface (flat) ──────────────────────────────────────────────────
    bot_z   = -base_h
    bot_off = len(verts)
    for jj, j in enumerate(sr):
        for ii, i in enumerate(sr):
            verts.append([i * gx, j * gy, bot_z])

    def top(ii, jj): return top_idx[(ii, jj)]
    def bot(ii, jj): return bot_off + jj * ns + ii

    # Top quads (CCW from above)
    for jj in range(ns - 1):
        for ii in range(ns - 1):
            a, b = top(ii, jj), top(ii + 1, jj)
            c, d = top(ii, jj + 1), top(ii + 1, jj + 1)
            faces += [[a, b, d], [a, d, c]]

    # Bottom quads (CW from above = CCW from below)
    for jj in range(ns - 1):
        for ii in range(ns - 1):
            a, b = bot(ii, jj), bot(ii + 1, jj)
            c, d = bot(ii, jj + 1), bot(ii + 1, jj + 1)
            faces += [[a, d, b], [a, c, d]]

    # Side walls
    for ii in range(ns - 1):
        faces += [[top(ii, 0),      bot(ii, 0),      top(ii + 1, 0)],
                  [top(ii + 1, 0),  bot(ii, 0),      bot(ii + 1, 0)]]
        faces += [[top(ii, ns-1),   top(ii+1, ns-1), bot(ii, ns-1)],
                  [top(ii+1, ns-1), bot(ii+1, ns-1), bot(ii, ns-1)]]
    for jj in range(ns - 1):
        faces += [[top(0, jj),      top(0, jj+1),    bot(0, jj)],
                  [top(0, jj+1),    bot(0, jj+1),    bot(0, jj)]]
        faces += [[top(ns-1, jj),   bot(ns-1, jj),   top(ns-1, jj+1)],
                  [top(ns-1, jj+1), bot(ns-1, jj),   bot(ns-1, jj+1)]]

    mesh = trimesh.Trimesh(vertices=np.array(verts, dtype=float),
                           faces=np.array(faces, dtype=int),
                           process=False)
    mesh.fix_normals()
    return mesh
