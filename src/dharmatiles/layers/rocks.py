"""
Vectorised half-ellipsoid rock mesh kernel.

All rock geometry is built with NumPy broadcasting in a single pass.
Rock tops are rasterised into the scene's terrain_support_z so that
subsequent layers (grass blades) are forced to sit above the rocks.

Entry points
------------
``_build_rocks_mesh_from_seeds(seeds, ...)``
    Public path: accepts a pre-sorted list of ``RockSeed`` objects
    (geometry already baked in) and delegates to ``_build_rocks_mesh_core``.
    Called by ``scatter.prototype.Rocks.scatter()``.

``_build_rocks_mesh_core(cx, cy, rx_arr, ry_arr, height, angle, ...)``
    Shared kernel: given arrays of positions and geometry, builds the
    half-ellipsoid meshes, applies cuts, roughness, and slope rotation,
    and rasterises tops into support_z / obstacle_mask.

Slope alignment
---------------
Each rock is rotated so its local +Z axis aligns with the terrain normal at
its centre.  The normal is derived from the gradient of ``terrain_z`` via
Rodrigues' formula.  The bottom-cap vertex (the pivot) stays at the terrain
surface; the rest of the rock tilts to follow the slope.  The ``support_z``
rasterisation footprint uses the axis-aligned bounding ellipse of the
un-tilted rock, which is a slight over-estimate on steep slopes but
negligible in practice.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig, RocksConfig
from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import sample


# ── Scatter entry point ───────────────────────────────────────────────────────

def _build_rocks_mesh_from_seeds(
    seeds,                         # list[RockSeed] — pre-sorted big→small
    rocks: RocksConfig,
    surface: SurfaceConfig,
    terrain_z: np.ndarray,
    support_z: np.ndarray,
    obstacle_mask: np.ndarray | None,
    *,
    layer_idx:    int                  = 0,
    terrain_gz_x: np.ndarray | None   = None,
    terrain_gz_y: np.ndarray | None   = None,
) -> trimesh.Trimesh:
    """Build a mesh from pre-generated ``RockSeed`` objects.

    Called by ``scatter.prototype.Rocks.scatter()``.  Positions and sizes are
    already baked into the seeds; this function handles terrain sampling,
    slope rotation, plane cuts, roughness, and support_z rasterisation.
    """
    N = len(seeds)
    if N == 0:
        return trimesh.Trimesh(process=False)

    cx     = np.array([s.x      for s in seeds], dtype=float)
    cy     = np.array([s.y      for s in seeds], dtype=float)
    rx_arr = np.array([s.rx     for s in seeds], dtype=float)
    ry_arr = np.array([s.ry     for s in seeds], dtype=float)
    height = np.array([s.height for s in seeds], dtype=float)
    angle  = np.array([s.angle  for s in seeds], dtype=float)

    # Independent RNG for plane cuts and roughness.  Positions and sizes
    # are already determined by the seeds; this stream only drives the
    # stochastic detail passes so each layer_idx gets a different texture.
    rng = np.random.default_rng(derive_seed(surface.seed, 'rocks-detail', layer_idx))

    return _build_rocks_mesh_core(
        cx, cy, rx_arr, ry_arr, height, angle,
        rocks, surface, terrain_z, support_z, obstacle_mask, rng,
        terrain_gz_x=terrain_gz_x,
        terrain_gz_y=terrain_gz_y,
    )


# ── Shared mesh kernel ────────────────────────────────────────────────────────

def _build_rocks_mesh_core(
    cx:     np.ndarray,   # (N,) centre X
    cy:     np.ndarray,   # (N,) centre Y
    rx_arr: np.ndarray,   # (N,) semi-axis X
    ry_arr: np.ndarray,   # (N,) semi-axis Y
    height: np.ndarray,   # (N,) height above base_z
    angle:  np.ndarray,   # (N,) yaw rotation (radians)
    rocks:  RocksConfig,
    surface: SurfaceConfig,
    terrain_z: np.ndarray,
    support_z: np.ndarray,
    obstacle_mask: np.ndarray | None,
    rng:    np.random.Generator,
    *,
    terrain_gz_x: np.ndarray | None = None,
    terrain_gz_y: np.ndarray | None = None,
) -> trimesh.Trimesh:
    """Build half-ellipsoid rock meshes from pre-computed geometry arrays.

    Performs terrain sampling, slope alignment, plane cuts, roughness, face
    assembly, and support_z rasterisation.
    """
    N  = len(cx)
    AZ = rocks.az_segs
    EL = rocks.el_segs

    ca, sa = np.cos(angle), np.sin(angle)
    tz     = sample_grid(terrain_z, surface, cx, cy)
    base_z = tz - rocks.sink

    # ── Terrain normals for slope alignment ───────────────────────────────────
    # Each rock is rotated so its local +Z aligns with the terrain normal,
    # keeping the base flush on the slope instead of slicing horizontally.
    # We use Rodrigues' formula to build a per-rock rotation R[n] that maps
    # world +Z → terrain normal n:
    #   v = cross([0,0,1], n) = (-n_y, n_x, 0)
    #   R = I + K + K²·(1−nz)/(nx²+ny²)   (K = skew-symmetric of v)
    _cw = surface.cell_w
    if terrain_gz_x is None:
        terrain_gz_x = np.gradient(terrain_z, axis=1) / _cw   # dz/dx
        terrain_gz_y = np.gradient(terrain_z, axis=0) / _cw   # dz/dy
    _dzdx = sample_grid(terrain_gz_x, surface, cx, cy)
    _dzdy = sample_grid(terrain_gz_y, surface, cx, cy)
    _nlen     = np.sqrt(_dzdx**2 + _dzdy**2 + 1.0)
    _nx       = -_dzdx / _nlen
    _ny       = -_dzdy / _nlen
    _nz       =  1.0   / _nlen
    _ns2      = _nx**2 + _ny**2
    _fac      = np.where(_ns2 > 1e-12,
                         (1.0 - _nz) / np.maximum(_ns2, 1e-12), 0.0)
    _R00 = 1.0 - _fac * _nx**2;  _R01 = -_fac * _nx * _ny;  _R02 = _nx
    _R10 = _R01;                  _R11 = 1.0 - _fac * _ny**2; _R12 = _ny
    _R20 = -_nx;                  _R21 = -_ny;                _R22 = _nz

    # ── Vertex buffer ──────────────────────────────────────────────────────────
    vps = 1 + EL * AZ + 1    # verts per rock: apex + rings + base-centre
    fps = AZ + AZ * (EL - 1) * 2 + AZ

    all_verts = np.empty((N * vps, 3), dtype=float)

    apex_idx = np.arange(N) * vps
    all_verts[apex_idx, 0] = cx
    all_verts[apex_idx, 1] = cy
    all_verts[apex_idx, 2] = base_z + height

    ei_arr  = np.arange(1, EL + 1)
    r_frac  = np.sin(ei_arr / EL * np.pi / 2)
    z_off   = np.cos(ei_arr / EL * np.pi / 2)

    ai_arr  = np.arange(AZ)
    theta   = 2 * np.pi * ai_arr / AZ
    cos_th  = np.cos(theta);  sin_th = np.sin(theta)

    lx = rx_arr[:, None, None] * r_frac[None, :, None] * cos_th[None, None, :]
    ly = ry_arr[:, None, None] * r_frac[None, :, None] * sin_th[None, None, :]

    wx = cx[:, None, None] + ca[:, None, None] * lx - sa[:, None, None] * ly
    wy = cy[:, None, None] + sa[:, None, None] * lx + ca[:, None, None] * ly
    wz = (base_z[:, None, None] +
          height[:, None, None] * z_off[None, :, None]) + np.zeros((1, 1, AZ))

    mean_r = 0.5 * (rx_arr + ry_arr)

    # ── Plane cuts: slice random chunks off each rock ─────────────────────────
    n_cuts = rocks.n_cuts
    if n_cuts > 0:
        lx_v = wx - cx[:, None, None]
        ly_v = wy - cy[:, None, None]
        lz_v = wz - base_z[:, None, None]
        pts  = np.stack([lx_v, ly_v, lz_v], axis=-1)

        raw = rng.standard_normal((N, n_cuts, 3))
        raw[:, :, 2] = np.abs(raw[:, :, 2]) * 0.3
        norms = raw / (np.linalg.norm(raw, axis=-1, keepdims=True) + 1e-8)

        cut_d = sample(rocks.cut, rng, (N, n_cuts)) * mean_r[:, None]

        for k in range(n_cuts):
            n_k  = norms[:, k, :][:, None, None, :]
            d_k  = cut_d[:, k][:, None, None]
            dot  = (pts * n_k).sum(axis=-1)
            exc  = np.maximum(0.0, dot - d_k)
            pts -= exc[:, :, :, None] * n_k

        pts[..., 2] = np.maximum(pts[..., 2], 0.0)
        wx = cx[:, None, None]       + pts[..., 0]
        wy = cy[:, None, None]       + pts[..., 1]
        wz = base_z[:, None, None]   + pts[..., 2]

        apex_pts = np.zeros((N, 3))
        apex_pts[:, 2] = height
        for k in range(n_cuts):
            n_k = norms[:, k, :]
            d_k = cut_d[:, k]
            dot = (apex_pts * n_k).sum(axis=-1)
            exc = np.maximum(0.0, dot - d_k)
            apex_pts -= exc[:, None] * n_k
        apex_pts[:, 2] = np.maximum(apex_pts[:, 2], 0.0)
        all_verts[apex_idx, 0] = cx + apex_pts[:, 0]
        all_verts[apex_idx, 1] = cy + apex_pts[:, 1]
        all_verts[apex_idx, 2] = base_z + apex_pts[:, 2]

    # ── Residual roughness: tiny per-vertex noise ──────────────────────────────
    if rocks.roughness > 0.0:
        scale = (rocks.roughness * mean_r)[:, None, None]
        wx += scale * rng.uniform(-1.0, 1.0, wx.shape)
        wy += scale * rng.uniform(-1.0, 1.0, wy.shape)
        wz += scale * 0.4 * rng.uniform(-1.0, 1.0, wz.shape)

    # ── Slope rotation: align rock with terrain normal ────────────────────────
    if np.any(_ns2 > 1e-9):
        dx_ = wx - cx[:, None, None]
        dy_ = wy - cy[:, None, None]
        dz_ = wz - base_z[:, None, None]
        wx = cx[:, None, None]     + (_R00[:, None, None]*dx_ + _R01[:, None, None]*dy_ + _R02[:, None, None]*dz_)
        wy = cy[:, None, None]     + (_R10[:, None, None]*dx_ + _R11[:, None, None]*dy_ + _R12[:, None, None]*dz_)
        wz = base_z[:, None, None] + (_R20[:, None, None]*dx_ + _R21[:, None, None]*dy_ + _R22[:, None, None]*dz_)
        a_dx = all_verts[apex_idx, 0] - cx
        a_dy = all_verts[apex_idx, 1] - cy
        a_dz = all_verts[apex_idx, 2] - base_z
        all_verts[apex_idx, 0] = cx     + _R00*a_dx + _R01*a_dy + _R02*a_dz
        all_verts[apex_idx, 1] = cy     + _R10*a_dx + _R11*a_dy + _R12*a_dz
        all_verts[apex_idx, 2] = base_z + _R20*a_dx + _R21*a_dy + _R22*a_dz

    ring_base = (np.arange(N) * vps + 1)[:, None, None]
    ei_off    = (np.arange(EL) * AZ)[None, :, None]
    ai_off    = np.arange(AZ)[None, None, :]
    ring_idx  = ring_base + ei_off + ai_off

    all_verts[ring_idx.ravel(), 0] = wx.ravel()
    all_verts[ring_idx.ravel(), 1] = wy.ravel()
    all_verts[ring_idx.ravel(), 2] = wz.ravel()

    bot_idx = np.arange(N) * vps + vps - 1
    all_verts[bot_idx, 0] = cx
    all_verts[bot_idx, 1] = cy
    all_verts[bot_idx, 2] = base_z

    # ── Face buffer ────────────────────────────────────────────────────────────
    canon: list = []
    for ai in range(AZ):
        canon.append([0, 1 + ai, 1 + (ai + 1) % AZ])
    for ei in range(1, EL):
        ra = 1 + (ei - 1) * AZ;  rb = 1 + ei * AZ
        for ai in range(AZ):
            a0 = ra + ai;         a1 = ra + (ai + 1) % AZ
            b0 = rb + ai;         b1 = rb + (ai + 1) % AZ
            canon += [[a0, b0, a1], [a1, b0, b1]]
    last_ring = 1 + (EL - 1) * AZ
    bot_local = vps - 1
    for ai in range(AZ):
        canon.append([last_ring + ai, bot_local, last_ring + (ai + 1) % AZ])

    canon_faces = np.array(canon, dtype=np.int32)
    rock_bases  = (np.arange(N) * vps).astype(np.int32)
    all_faces   = (canon_faces[None, :, :] +
                   rock_bases[:, None, None]).reshape(-1, 3)

    mesh = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    mesh.fix_normals()

    # ── Rasterise rock tops into support_z (batched) ─────────────────────────
    #
    # Instead of calling np.meshgrid + rotation per rock (N separate allocations),
    # we pre-compute a shared bounding box of size (box, box) and batch all heavy
    # math into (N, box, box) arrays in one pass.  Only the per-rock scatter-
    # accumulate writes remain in the loop — cheap indexed writes with no math.
    cw = surface.cell_w          # cell width = cell height (square cells)
    gw = surface.grid_w
    gh = surface.grid_h

    r_max_all = max(float(rx_arr.max()), float(ry_arr.max()))
    half      = int(r_max_all / cw) + 2   # half-width in cells; +2 for rounding margin
    box       = 2 * half + 1

    # 1-D offset arrays from the integer centre cell — broadcast into (N, box, box).
    d_xy = np.arange(-half, half + 1, dtype=float) * cw   # (box,)

    # Integer centre cell for each rock + sub-cell correction.
    i_c   = np.round(cx / cw).astype(int)    # (N,)
    j_c   = np.round(cy / cw).astype(int)    # (N,)
    off_x = i_c * cw - cx                     # sub-cell correction in x (N,)
    off_y = j_c * cw - cy                     # sub-cell correction in y (N,)

    # World offsets from each rock centre:
    #   DX_r[s, j, i] = d_xy[i] + off_x[s]   — only varies along axis 2 (cols)
    #   DY_r[s, j, i] = d_xy[j] + off_y[s]   — only varies along axis 1 (rows)
    # Shapes (N,1,box) and (N,box,1) broadcast to (N,box,box) in the products below.
    DX_r = d_xy[np.newaxis, np.newaxis, :] + off_x[:, np.newaxis, np.newaxis]  # (N,1,box)
    DY_r = d_xy[np.newaxis, :, np.newaxis] + off_y[:, np.newaxis, np.newaxis]  # (N,box,1)

    # Rotate into each rock's local ellipse frame — one batched step for all N.
    LX   =  ca[:, None, None] * DX_r + sa[:, None, None] * DY_r   # (N,box,box)
    LY   = -sa[:, None, None] * DX_r + ca[:, None, None] * DY_r   # (N,box,box)

    D2     = (LX / rx_arr[:, None, None]) ** 2 + (LY / ry_arr[:, None, None]) ** 2
    INSIDE = D2 <= 1.0                                              # (N,box,box)
    ZTOP   = np.where(
        INSIDE,
        base_z[:, None, None] + height[:, None, None] * np.sqrt(np.maximum(0.0, 1.0 - D2)),
        -np.inf,
    )                                                               # (N,box,box)

    # Per-rock scatter-accumulate: only clipped slice writes remain in the loop.
    for s in range(N):
        i0 = i_c[s] - half;  j0 = j_c[s] - half
        i0c = max(0, i0);    i1c = min(gw, i0 + box)
        j0c = max(0, j0);    j1c = min(gh, j0 + box)
        if i0c >= i1c or j0c >= j1c:
            continue
        di0 = i0c - i0;  di1 = di0 + (i1c - i0c)
        dj0 = j0c - j0;  dj1 = dj0 + (j1c - j0c)

        sl = support_z[j0c:j1c, i0c:i1c]
        np.maximum(sl, ZTOP[s, dj0:dj1, di0:di1], out=sl)

        if obstacle_mask is not None:
            obstacle_mask[j0c:j1c, i0c:i1c] |= INSIDE[s, dj0:dj1, di0:di1]

    return mesh
