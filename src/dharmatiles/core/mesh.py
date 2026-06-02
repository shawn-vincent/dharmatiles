"""
Low-level mesh primitives: frame computation, blade tube, terrain solid,
DungeonBlocks base, and coloured STL export.
"""
from __future__ import annotations

import pathlib
import struct

import numpy as np
import trimesh

from .config import SurfaceConfig, BaseConfig
from .grid import sample_grid


# ── Coloured STL export ───────────────────────────────────────────────────────

def export_coloured_stl(mesh: trimesh.Trimesh, path: pathlib.Path) -> None:
    """Write *mesh* as a binary STL with VisCAM/SolidView per-face colours.

    Each triangle's 2-byte attribute field encodes RGB in 5-5-5 format::

        bit 15      = 1  (marks colour as valid)
        bits 14-10  = R  (0-31)
        bits  9-5   = G  (0-31)
        bits  4-0   = B  (0-31)

    Viewers that support the hack (PrusaSlicer, MeshLab, Windows 3D Builder …)
    will render each face in its assigned colour.  Viewers that ignore it see a
    normal single-colour STL.
    """
    faces   = mesh.faces                     # (N, 3) int
    verts   = mesh.vertices                  # (M, 3) float64
    normals = mesh.face_normals              # (N, 3) float64
    colours = mesh.visual.face_colors        # (N, 4) uint8  (RGBA)
    n       = len(faces)

    # Encode 8-bit channels to 5-bit and pack into uint16
    r5   = (colours[:, 0].astype(np.uint16) * 31 // 255)
    g5   = (colours[:, 1].astype(np.uint16) * 31 // 255)
    b5   = (colours[:, 2].astype(np.uint16) * 31 // 255)
    attr = (np.uint16(0x8000) | (r5 << 10) | (g5 << 5) | b5)

    # Pack into a numpy structured array — one record per triangle (50 bytes)
    dtype = np.dtype([
        ('normal', '<f4', (3,)),
        ('v0',     '<f4', (3,)),
        ('v1',     '<f4', (3,)),
        ('v2',     '<f4', (3,)),
        ('attr',   '<u2'),
    ])
    tri_verts = verts[faces]                 # (N, 3, 3)
    data              = np.empty(n, dtype=dtype)
    data['normal']    = normals.astype('<f4')
    data['v0']        = tri_verts[:, 0].astype('<f4')
    data['v1']        = tri_verts[:, 1].astype('<f4')
    data['v2']        = tri_verts[:, 2].astype('<f4')
    data['attr']      = attr

    header = b'VisCAM coloured STL' + b'\x00' * 61   # must be exactly 80 bytes

    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'wb') as fh:
        fh.write(header)
        fh.write(struct.pack('<I', n))
        fh.write(data.tobytes())


# ── Frame computation ─────────────────────────────────────────────────────────

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


# ── Terrain solid ─────────────────────────────────────────────────────────────

def make_heightmap_solid(z_grid: np.ndarray, tile_w: float, tile_h: float,
                         base_h: float, subsample: int = 1) -> trimesh.Trimesh:
    """Watertight solid: top = *z_grid* surface, bottom = flat at −*base_h*.

    Parameters
    ----------
    z_grid   : (grid_h, grid_w) terrain heights in mm.
    tile_w/h : tile dimensions in mm.
    base_h   : depth of the solid slab below terrain in mm (positive value).
    subsample: take every Nth grid sample for the mesh (1 = full resolution).
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
    """Dungeonblock socket-base mesh: one peg per square.

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
    square_sz = surface.tile_w / surface.cols  # 35.0 mm
    col       = base_cfg.col_size                   # 26.0 mm
    bevel     = base_cfg.col_bevel                  # 1.5 mm
    flare_h   = base_cfg.flare_height               # 5.2 mm
    bevel_col = col - 2.0 * bevel                   # 23.0 mm

    col_inset   = (square_sz - col) / 2.0             # 4.5 mm
    bevel_inset = (square_sz - bevel_col) / 2.0       # 6.0 mm

    z0 = 0.0                                          # flare top = terrain bottom
    z1 = -flare_h                                     # column top / flare bottom
    z2 = -(peg_height - bevel + flare_h)              # bevel top / column bottom
    z3 = -(peg_height + flare_h)                      # peg bottom

    parts: list = []
    for ci in range(surface.cols):
        for ri in range(surface.rows):
            tx = ci * square_sz
            ty = ri * square_sz

            rings = [
                _square_ring(tx, ty, 0.0,        square_sz, z0),  # full square at top
                _square_ring(tx, ty, col_inset,   square_sz, z1),  # column top
                _square_ring(tx, ty, col_inset,   square_sz, z2),  # column bottom
                _square_ring(tx, ty, bevel_inset, square_sz, z3),  # chamfered tip
            ]
            parts.append(_prismatoid_mesh(rings))

    if not parts:
        return trimesh.Trimesh()
    return trimesh.util.concatenate(parts)
