"""
Low-level mesh primitives: frame computation, blade tube, terrain solid,
and printable support hulls.
"""
from __future__ import annotations

import numpy as np
import trimesh

from .config import SurfaceConfig, GrassConfig, SolverConfig, BaseConfig
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
        4-vertex rhombus / diamond cross-section.
        Spine sits at the top apex; the blade widens at the equator then tapers
        back to a bottom keel apex, giving a ridge-backed, keel-bottomed blade.
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
        thetas = 2 * np.pi * np.arange(n) / n
        cos_t  = np.cos(thetas)
        sin_t  = np.sin(thetas)
        ring_v = (path[:, None, :] +
                  half_W[:, None, None] *
                  (cos_t[None, :, None] * up_locs[:, None, :] +
                   sin_t[None, :, None] * down_locs[:, None, :]))

    elif cross_section == 'diamond':
        n      = 4
        eq_d   = diamond_equator * thickness
        ring_v = np.stack([
            path,
            path + half_W[:, None] * up_locs   + eq_d * down_locs,
            path + thickness * down_locs,
            path - half_W[:, None] * up_locs   + eq_d * down_locs,
        ], axis=1)

    else:
        raise ValueError(
            f"Unknown cross_section {cross_section!r}; "
            "use 'triangle', 'circle', or 'diamond'"
        )

    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = fi = 0

    for i in range(n_pts):
        verts[vi:vi + n] = ring_v[i];  vi += n

    v_base = vi;  verts[vi] = path[0];   vi += 1
    v_tip  = vi;  verts[vi] = path[-1];  vi += 1

    for i in range(n):
        faces[fi] = [v_base, i, (i + 1) % n];  fi += 1

    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for i in range(n):
            i1 = (i + 1) % n
            faces[fi]   = [ra + i,  rb + i,  ra + i1];  fi += 1
            faces[fi]   = [ra + i1, rb + i,  rb + i1];  fi += 1

    rl = (n_pts - 1) * n
    for i in range(n):
        faces[fi] = [rl + i, rl + (i + 1) % n, v_tip];  fi += 1

    mesh = trimesh.Trimesh(vertices=verts[:vi],
                           faces=faces[:fi].astype(int),
                           process=False)
    mesh.fix_normals()
    return mesh


# ── Printable support hull ────────────────────────────────────────────────────

def drop_to_support(point, support_z: np.ndarray,
                    surface: SurfaceConfig,
                    search_limit: float = 30.0) -> np.ndarray:
    """Drop vertically from *point* until hitting support_z at the same XY."""
    start = np.asarray(point, dtype=float)

    def gap(dist):
        return start[2] - dist - sample_grid(support_z, surface, start[0], start[1])

    if gap(0.0) <= 0.0:
        return start

    hi = 0.25
    while hi < search_limit and gap(hi) > 0.0:
        hi *= 2.0
    if gap(hi) > 0.0:
        return np.array([start[0], start[1], start[2] - hi], dtype=float)

    lo = 0.0
    for _ in range(16):
        mid = 0.5 * (lo + hi)
        if gap(mid) > 0.0:
            lo = mid
        else:
            hi = mid
    return np.array([start[0], start[1], start[2] - hi], dtype=float)


def build_sub_hull_mesh(surface: SurfaceConfig,
                        grass: GrassConfig,
                        spine_3d: np.ndarray,
                        widths: np.ndarray,
                        support_z: np.ndarray,
                        cross_section: str = 'triangle') -> trimesh.Trimesh:
    """Triangular-prism support hull that bridges under a blade to the terrain.

    Two side vertices attach to the blade's underside; a third is dropped
    vertically until it touches the support surface.
    """
    path  = np.asarray(spine_3d, dtype=float)
    W_arr = np.asarray(widths, dtype=float)
    n_pts = len(path)
    n     = 3

    _, up_locs, down_locs = blade_frame(path)
    half_W = (W_arr / 2.0)[:, None]
    frac   = grass.sub_hull_fraction
    thickness = grass.thickness
    diamond_equator = grass.diamond_equator

    if cross_section == 'triangle':
        apex   = path + thickness * down_locs
        right  = path + half_W * up_locs
        left   = path - half_W * up_locs
        side_r = right + frac * (apex - right)
        side_l = left  + frac * (apex - left)

    elif cross_section == 'diamond':
        eq_d   = diamond_equator * thickness
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
        lower[idx] = drop_to_support(centers[idx], support_z, surface)

    ring_v = np.stack([lower, side_r, side_l], axis=1)
    ring_v[:, :, 0] = np.clip(ring_v[:, :, 0], 0.0, surface.tile_w)
    ring_v[:, :, 1] = np.clip(ring_v[:, :, 1], 0.0, surface.tile_h)

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
    z_grid   : (grid_h, grid_w) terrain heights in mm.
    tile_w/h : tile dimensions in mm.
    base_h   : depth of the solid slab below terrain in mm (positive value).
    subsample: take every Nth grid sample for the mesh (reduces triangle count).
    """
    nrows, ncols = z_grid.shape
    sr = list(range(0, ncols, subsample))
    if sr[-1] != ncols - 1:
        sr.append(ncols - 1)
    sc = list(range(0, nrows, subsample))
    if sc[-1] != nrows - 1:
        sc.append(nrows - 1)
    ns_c = len(sr)   # sampled cols
    ns_r = len(sc)   # sampled rows
    gx   = tile_w / max(ncols - 1, 1)
    gy   = tile_h / max(nrows - 1, 1)

    verts: list = []
    faces: list = []

    # ── Top surface ────────────────────────────────────────────────────────────
    top_idx: dict = {}
    for jj, j in enumerate(sc):
        for ii, i in enumerate(sr):
            top_idx[(ii, jj)] = len(verts)
            verts.append([i * gx, j * gy, z_grid[j, i]])

    # ── Bottom surface (flat) ──────────────────────────────────────────────────
    bot_z   = -base_h
    bot_off = len(verts)
    for jj, j in enumerate(sc):
        for ii, i in enumerate(sr):
            verts.append([i * gx, j * gy, bot_z])

    def top(ii, jj): return top_idx[(ii, jj)]
    def bot(ii, jj): return bot_off + jj * ns_c + ii

    # Top quads (CCW from above)
    for jj in range(ns_r - 1):
        for ii in range(ns_c - 1):
            a, b = top(ii, jj), top(ii + 1, jj)
            c, d = top(ii, jj + 1), top(ii + 1, jj + 1)
            faces += [[a, b, d], [a, d, c]]

    # Bottom quads (CW from above = CCW from below)
    for jj in range(ns_r - 1):
        for ii in range(ns_c - 1):
            a, b = bot(ii, jj), bot(ii + 1, jj)
            c, d = bot(ii, jj + 1), bot(ii + 1, jj + 1)
            faces += [[a, d, b], [a, c, d]]

    # Side walls
    for ii in range(ns_c - 1):
        faces += [[top(ii, 0),       bot(ii, 0),       top(ii + 1, 0)],
                  [top(ii + 1, 0),   bot(ii, 0),       bot(ii + 1, 0)]]
        faces += [[top(ii, ns_r-1),  top(ii+1, ns_r-1), bot(ii, ns_r-1)],
                  [top(ii+1, ns_r-1),bot(ii+1, ns_r-1), bot(ii, ns_r-1)]]
    for jj in range(ns_r - 1):
        faces += [[top(0, jj),       top(0, jj+1),     bot(0, jj)],
                  [top(0, jj+1),     bot(0, jj+1),     bot(0, jj)]]
        faces += [[top(ns_c-1, jj),  bot(ns_c-1, jj),  top(ns_c-1, jj+1)],
                  [top(ns_c-1, jj+1),bot(ns_c-1, jj),  bot(ns_c-1, jj+1)]]

    mesh = trimesh.Trimesh(vertices=np.array(verts, dtype=float),
                           faces=np.array(faces, dtype=int),
                           process=False)
    mesh.fix_normals()
    return mesh


# ── DungeonBlocks socket base ─────────────────────────────────────────────────

def select_peg_height(terrain_z: np.ndarray,
                      base_cfg: BaseConfig) -> float:
    """Return peg column height (mm) for *terrain_z*.

    Uses ``base_cfg.peg_height`` when set; otherwise auto-selects
    ``tall_peg_height`` when the max terrain height exceeds
    ``auto_threshold_mm``, else ``short_peg_height``.
    """
    if base_cfg.peg_height is not None:
        return base_cfg.peg_height
    max_h = float(terrain_z.max())
    return (base_cfg.tall_peg_height
            if max_h > base_cfg.auto_threshold_mm
            else base_cfg.short_peg_height)


def _square_ring(tx: float, ty: float,
                  inset: float, tile_sz: float, z: float) -> np.ndarray:
    """Four CCW corner vertices of a square ring at the given z level.

    Vertices are ordered counterclockwise when viewed from above (+Z):
      0: front-left   (tx+inset,       ty+inset,       z)
      1: front-right  (tx+tile_sz−inset, ty+inset,     z)
      2: back-right   (tx+tile_sz−inset, ty+tile_sz−inset, z)
      3: back-left    (tx+inset,       ty+tile_sz−inset, z)
    """
    i = inset
    s = tile_sz
    return np.array([
        [tx + i,     ty + i,     z],
        [tx + s - i, ty + i,     z],
        [tx + s - i, ty + s - i, z],
        [tx + i,     ty + s - i, z],
    ], dtype=float)


def _prismatoid_mesh(rings: list) -> trimesh.Trimesh:
    """Closed watertight mesh from a list of 4-vertex rectangular rings.

    Rings must be ordered from top (z = 0) to bottom (most negative z).
    Each ring is a (4, 3) float array with CCW vertex order from above.

    Face winding is determined analytically and verified by fix_normals():
      - Top cap     : CCW from above  → outward normal = +Z
      - Bottom cap  : CW  from above  → outward normal = −Z
      - Side strips : CCW from outside → outward normals point away from centre
    """
    n = len(rings)
    verts = np.vstack(rings)    # (4*n, 3)
    faces: list = []

    # Top cap (ring 0) — CCW from above
    faces += [[0, 1, 2], [0, 2, 3]]

    # Bottom cap (last ring) — CW from above (= CCW from below)
    b = 4 * (n - 1)
    faces += [[b, b + 2, b + 1], [b, b + 3, b + 2]]

    # Side strips between adjacent rings
    for i in range(n - 1):
        a = 4 * i        # base index of upper ring
        c = 4 * (i + 1)  # base index of lower ring
        for j in range(4):
            j1 = (j + 1) % 4
            # Each quad a+j, a+j1, c+j1, c+j split into two CCW triangles
            # (CCW when viewed from the outside of the frustum).
            faces += [[a + j, c + j,  c + j1],
                      [a + j, c + j1, a + j1]]

    mesh = trimesh.Trimesh(vertices=verts,
                           faces=np.array(faces, dtype=np.int32),
                           process=False)
    mesh.fix_normals()
    return mesh


def make_dungeonblock_base(surface: SurfaceConfig,
                            peg_height: float,
                            base_cfg: BaseConfig) -> trimesh.Trimesh:
    """Dungeonblock socket-base mesh: one peg per tile unit.

    The mesh top sits at z = 0 (bottom of the terrain slab).
    The peg tip reaches z = −(peg_height + flare_height).

    Geometry (ported from ``floor-wall-tile.scad``):
      Ring 0 — z = 0           : full tile footprint (top of flare)
      Ring 1 — z = −flare_h    : column footprint (flare bottom / column top)
      Ring 2 — z = −(peg_h−bevel+flare_h) : column bottom / bevel top
      Ring 3 — z = −(peg_h+flare_h)       : chamfered peg tip

    Each peg is built as an explicit closed polyhedron (prismatoid) so that
    all four sections (flare, column body, bevel entry, caps) are represented
    exactly — no convex-hull approximation that can collapse interior rings.
    """
    tile_sz   = surface.tile_w / surface.tile_cols  # 35.0 mm
    col       = base_cfg.col_size                   # 26.0 mm
    bevel     = base_cfg.col_bevel                  # 1.5 mm
    flare_h   = base_cfg.flare_height               # 5.2 mm
    bevel_col = col - 2.0 * bevel                   # 23.0 mm

    col_inset   = (tile_sz - col) / 2.0             # 4.5 mm
    bevel_inset = (tile_sz - bevel_col) / 2.0       # 6.0 mm

    z0 = 0.0                                        # flare top = terrain bottom
    z1 = -flare_h                                   # column top / flare bottom
    z2 = -(peg_height - bevel + flare_h)            # bevel top / column bottom
    z3 = -(peg_height + flare_h)                    # peg bottom

    parts: list = []
    for ci in range(surface.tile_cols):
        for ri in range(surface.tile_rows):
            tx = ci * tile_sz
            ty = ri * tile_sz

            rings = [
                _square_ring(tx, ty, 0.0,        tile_sz, z0),  # full tile at top
                _square_ring(tx, ty, col_inset,   tile_sz, z1),  # column top
                _square_ring(tx, ty, col_inset,   tile_sz, z2),  # column bottom
                _square_ring(tx, ty, bevel_inset, tile_sz, z3),  # chamfered tip
            ]
            parts.append(_prismatoid_mesh(rings))

    if not parts:
        return trimesh.Trimesh()
    return trimesh.util.concatenate(parts)
