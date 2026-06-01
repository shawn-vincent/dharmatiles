"""
Low-level mesh primitives: frame computation, blade tube, terrain solid,
and printable support hulls.
"""
from __future__ import annotations

import numpy as np
import trimesh

from .tile import TileConfig
from .grid import sample_grid


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
                    n_segs: int = 8,
                    diamond_equator: float = 0.75) -> trimesh.Trimesh:
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

    cross_section='diamond'
        4 verts / ring forming a rhombus.  V0 is the top apex (spine), V2 is
        the bottom keel apex (*thickness* below).  V1/V3 are the equator
        (widest points) at *diamond_equator* × *thickness* below the spine.
        Larger equator values push the equator toward the bottom, sharpening
        the upper ridge.  Default 0.75 gives a sharp-topped blade.
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

    elif cross_section == 'diamond':
        # 4-vertex rhombus / diamond cross-section.
        # Spine sits at the top apex; the blade widens at the equator then tapers
        # back to a bottom keel apex, giving a ridge-backed, keel-bottomed blade.
        #
        #   V0 (top apex)      — spine
        #   V1 (right equator) — spine + half_w * up  + equator_d * down
        #   V2 (bottom apex)   — spine + thickness * down
        #   V3 (left equator)  — spine − half_w * up  + equator_d * down
        #
        # equator_d = diamond_equator × thickness.  Larger values push the
        # widest point toward the bottom, sharpening the upper ridge.
        n        = 4
        eq_d     = diamond_equator * thickness
        ring_v   = np.stack([
            path,                                                               # V0
            path + half_W[:, None] * up_locs   + eq_d * down_locs,            # V1
            path + thickness * down_locs,                                       # V2
            path - half_W[:, None] * up_locs   + eq_d * down_locs,            # V3
        ], axis=1)                                                              # (n_pts, 4, 3)

    else:
        raise ValueError(
            f"Unknown cross_section {cross_section!r}; "
            "use 'triangle', 'circle', or 'diamond'"
        )

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


# ── Printable support hull ────────────────────────────────────────────────────

def drop_to_support(point, support_z: np.ndarray, cfg: TileConfig) -> np.ndarray:
    """Drop vertically from *point* until hitting support_z at the same XY."""
    start = np.asarray(point, dtype=float)

    def clearance(dist):
        return start[2] - dist - sample_grid(support_z, cfg, start[0], start[1])

    if clearance(0.0) <= 0.0:
        return start

    hi = 0.25
    search_limit = cfg.base_h + cfg.max_stack_height + cfg.grass_thickness + 2.0
    while hi < search_limit and clearance(hi) > 0.0:
        hi *= 2.0
    if clearance(hi) > 0.0:
        return np.array([start[0], start[1], start[2] - hi], dtype=float)

    lo = 0.0
    for _ in range(16):
        mid = 0.5 * (lo + hi)
        if clearance(mid) > 0.0:
            lo = mid
        else:
            hi = mid
    return np.array([start[0], start[1], start[2] - hi], dtype=float)


def build_sub_hull_mesh(cfg: TileConfig, spine_3d: np.ndarray,
                        widths: np.ndarray,
                        support_z: np.ndarray,
                        cross_section: str = 'triangle') -> trimesh.Trimesh:
    """Triangular-prism support hull that bridges under a blade to the terrain.

    Two side vertices attach to the blade's underside; a third is dropped
    vertically until it touches the support surface.  Works for all three
    cross-section modes (triangle / circle / diamond).
    """
    path  = np.asarray(spine_3d, dtype=float)
    W_arr = np.asarray(widths, dtype=float)
    n_pts = len(path)
    n     = 3

    _, up_locs, down_locs = blade_frame(path)
    half_W = (W_arr / 2.0)[:, None]
    frac   = cfg.grass_sub_hull_fraction

    if cross_section == 'triangle':
        apex   = path + cfg.grass_thickness * down_locs
        right  = path + half_W * up_locs
        left   = path - half_W * up_locs
        side_r = right + frac * (apex - right)
        side_l = left  + frac * (apex - left)

    elif cross_section == 'diamond':
        eq_d   = cfg.blade_diamond_equator * cfg.grass_thickness
        side_r = path + half_W * up_locs + eq_d * down_locs
        side_l = path - half_W * up_locs + eq_d * down_locs

    else:  # 'circle'
        theta_r = frac * np.pi / 2
        theta_l = np.pi - theta_r
        side_r  = path + half_W * (np.cos(theta_r) * up_locs + np.sin(theta_r) * down_locs)
        side_l  = path + half_W * (np.cos(theta_l) * up_locs + np.sin(theta_l) * down_locs)

    centers = 0.5 * (side_r + side_l)

    lower = np.empty_like(path)
    for idx in range(n_pts):
        lower[idx] = drop_to_support(centers[idx], support_z, cfg)

    ring_v = np.stack([lower, side_r, side_l], axis=1)   # (n_pts, 3, 3)
    ring_v[:, :, 0] = np.clip(ring_v[:, :, 0], 0.0, cfg.tile_w)
    ring_v[:, :, 1] = np.clip(ring_v[:, :, 1], 0.0, cfg.tile_h)

    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = fi = 0

    for idx in range(n_pts):
        verts[vi:vi + n] = ring_v[idx];  vi += n

    v_base = vi;  verts[vi] = np.mean(ring_v[0],  axis=0);  vi += 1
    v_tip  = vi;  verts[vi] = np.mean(ring_v[-1], axis=0);  vi += 1

    for idx in range(n):
        faces[fi] = [v_base, (idx + 1) % n, idx];  fi += 1

    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for idx in range(n):
            i1 = (idx + 1) % n
            faces[fi] = [ra + idx, rb + idx, ra + i1];  fi += 1
            faces[fi] = [ra + i1, rb + idx, rb + i1];   fi += 1

    rl = (n_pts - 1) * n
    for idx in range(n):
        faces[fi] = [rl + idx, rl + (idx + 1) % n, v_tip];  fi += 1

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
