"""
StonesLayer: batch-vectorised half-ellipsoid stones placed on terrain.

All stone geometry is built with NumPy broadcasting in a single pass.
Stone tops are rasterised into the scene's support_z so that subsequent
layers (grass blades) are forced to sit above the stones.

Slope alignment
---------------
Each stone is rotated so its local +Z axis aligns with the terrain normal at
its centre.  The normal is derived from the gradient of ``terrain_z`` via
Rodrigues' formula.  The bottom-cap vertex (the pivot) stays at the terrain
surface; the rest of the stone tilts to follow the slope.  The ``support_z``
rasterisation footprint uses the axis-aligned bounding ellipse of the
un-tilted stone, which is a slight over-estimate on steep slopes but
negligible in practice.
"""
from __future__ import annotations

from typing import List

import numpy as np
import trimesh

from ..core.config import SurfaceConfig, StonesConfig
from ..core.tile import TileScene
from ..core.grid import sample_grid


_UNSET = object()  # sentinel: "fall back to scene.stone_placement_mask"


class StonesLayer:
    """Place random stones across the tile surface and update support_z."""

    def __init__(self, surface: SurfaceConfig, stones: StonesConfig) -> None:
        self.surface = surface
        self.stones  = stones

    def build(self, scene: TileScene, *,
              placement_mask=_UNSET,
              layer_idx: int = 0) -> List[trimesh.Trimesh]:
        """Add stone geometry to *scene.support_z* and return mesh list.

        Parameters
        ----------
        placement_mask
            Restrict stone centres to True cells in this bool array.
            When omitted, falls back to ``scene.stone_placement_mask``.
        layer_idx
            Index among multiple stone-layer passes for the same tile.
            Offsets the RNG seed so each pass produces independent placement.
        """
        if placement_mask is _UNSET:
            placement_mask = scene.stone_placement_mask
        n_squares = self.surface.cols * self.surface.rows
        n_stones  = self.stones.stones_per_square * n_squares
        rng  = np.random.default_rng(self.surface.seed + 7919 + layer_idx * 65537)
        mesh = _build_stones_mesh(self.surface, self.stones, n_stones,
                                  scene.terrain_z, scene.support_z,
                                  scene.stone_mask,
                                  placement_mask,
                                  rng)
        return [mesh]


# ── Internal implementation ───────────────────────────────────────────────────

def _build_stones_mesh(surface: SurfaceConfig, stones: StonesConfig,
                       n_stones: int,
                       terrain_z: np.ndarray, support_z: np.ndarray,
                       stone_mask: np.ndarray | None,
                       placement_mask: np.ndarray | None,
                       rng: np.random.Generator) -> trimesh.Trimesh:
    """Place *n_stones* stones; return a single merged Trimesh."""
    N  = n_stones
    AZ = stones.az_segs
    EL = stones.el_segs
    if N <= 0:
        return trimesh.Trimesh(process=False)

    # Power-law size draw: U^power skews toward r_min; power=1 = uniform
    u_x    = rng.uniform(0.0, 1.0, N) ** stones.size_power
    rx_arr = stones.r_min + (stones.r_max - stones.r_min) * u_x
    # Second axis: aspect ratio bounded to [aspect_min, 1.0] so rocks stay roundish
    aspect = rng.uniform(stones.aspect_min, 1.0, N)
    ry_arr = rx_arr * aspect
    h_frac  = rng.uniform(stones.flat_min, stones.flat_max, N)
    height  = 0.5 * (rx_arr + ry_arr) * h_frac
    angle   = rng.uniform(0, np.pi, N)
    margin  = np.maximum(rx_arr, ry_arr)

    span_x = np.maximum(surface.tile_w - 2 * margin, 0.0)
    span_y = np.maximum(surface.tile_h - 2 * margin, 0.0)
    cx = margin + rng.uniform(0, 1, N) * span_x
    cy = margin + rng.uniform(0, 1, N) * span_y

    if placement_mask is not None:
        # The mask constrains only the stone centre.  The stone footprint may
        # cross region boundaries, but clipping by margin keeps it on the tile.
        allowed = np.argwhere(placement_mask)
        if len(allowed) == 0:
            return trimesh.Trimesh(process=False)
        chosen = allowed[rng.integers(0, len(allowed), N)]
        cy = (chosen[:, 0] + rng.uniform(0.0, 1.0, N)) * surface.cell_w
        cx = (chosen[:, 1] + rng.uniform(0.0, 1.0, N)) * surface.cell_w
        cx = np.clip(cx, margin, surface.tile_w - margin)
        cy = np.clip(cy, margin, surface.tile_h - margin)

    ca, sa = np.cos(angle), np.sin(angle)
    tz     = sample_grid(terrain_z, surface, cx, cy)
    base_z = tz - stones.sink

    # ── Terrain normals for slope alignment ───────────────────────────────────
    # Each stone is rotated so its local +Z aligns with the terrain normal,
    # keeping the base flush on the slope instead of slicing horizontally.
    # We use Rodrigues' formula to build a per-stone rotation R[n] that maps
    # world +Z → terrain normal n:
    #   v = cross([0,0,1], n) = (-n_y, n_x, 0)
    #   R = I + K + K²·(1−nz)/(nx²+ny²)   (K = skew-symmetric of v)
    _cw       = surface.cell_w
    _dzdx_g   = np.gradient(terrain_z, axis=1) / _cw   # dz/dx  (rows, cols)
    _dzdy_g   = np.gradient(terrain_z, axis=0) / _cw   # dz/dy
    _dzdx     = sample_grid(_dzdx_g, surface, cx, cy)   # (N,)
    _dzdy     = sample_grid(_dzdy_g, surface, cx, cy)
    _nlen     = np.sqrt(_dzdx**2 + _dzdy**2 + 1.0)
    _nx       = -_dzdx / _nlen                           # terrain normal x  (N,)
    _ny       = -_dzdy / _nlen                           # terrain normal y
    _nz       =  1.0   / _nlen                           # terrain normal z
    _ns2      = _nx**2 + _ny**2                          # sin²(tilt)
    _fac      = np.where(_ns2 > 1e-12,
                         (1.0 - _nz) / np.maximum(_ns2, 1e-12), 0.0)
    # Rotation matrix components, all shape (N,)
    _R00 = 1.0 - _fac * _nx**2;  _R01 = -_fac * _nx * _ny;  _R02 = _nx
    _R10 = _R01;                  _R11 = 1.0 - _fac * _ny**2; _R12 = _ny
    _R20 = -_nx;                  _R21 = -_ny;                _R22 = _nz

    # ── Vertex buffer ──────────────────────────────────────────────────────────
    vps = 1 + EL * AZ + 1    # verts per stone
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
          height[:, None, None] * z_off[None, :, None] * np.ones((1, 1, AZ)))

    mean_r = 0.5 * (rx_arr + ry_arr)                       # (N,) per-stone scale

    # ── Plane cuts: slice random chunks off each stone ────────────────────────
    # For each cut, project vertices past a random half-plane back onto it.
    # Normals are biased horizontal so cuts create side-face chunks, not
    # slices off the top.
    n_cuts = stones.n_cuts
    if n_cuts > 0:
        # Local coords relative to stone base-centre (so centre = 0,0,0)
        lx_v = wx - cx[:, None, None]   # (N, EL, AZ)
        ly_v = wy - cy[:, None, None]
        lz_v = wz - base_z[:, None, None]
        pts  = np.stack([lx_v, ly_v, lz_v], axis=-1)   # (N, EL, AZ, 3)

        raw = rng.standard_normal((N, n_cuts, 3))
        raw[:, :, 2] = np.abs(raw[:, :, 2]) * 0.3      # mostly horizontal cuts
        norms = raw / (np.linalg.norm(raw, axis=-1, keepdims=True) + 1e-8)

        cut_d = (rng.uniform(stones.cut_min, stones.cut_max, (N, n_cuts))
                 * mean_r[:, None])                     # (N, n_cuts) in mm

        for k in range(n_cuts):
            n_k  = norms[:, k, :][:, None, None, :]    # (N,1,1,3)
            d_k  = cut_d[:, k][:, None, None]          # (N,1,1)
            dot  = (pts * n_k).sum(axis=-1)             # (N, EL, AZ)
            exc  = np.maximum(0.0, dot - d_k)          # only vertices past plane
            pts -= exc[:, :, :, None] * n_k

        pts[..., 2] = np.maximum(pts[..., 2], 0.0)     # don't cut below base
        wx = cx[:, None, None]       + pts[..., 0]
        wy = cy[:, None, None]       + pts[..., 1]
        wz = base_z[:, None, None]   + pts[..., 2]

        # Apply same cuts to the apex vertex so it can't spike above a cut top
        apex_pts = np.zeros((N, 3))
        apex_pts[:, 2] = height                          # local (0, 0, h)
        for k in range(n_cuts):
            n_k = norms[:, k, :]                         # (N, 3)
            d_k = cut_d[:, k]                            # (N,)
            dot = (apex_pts * n_k).sum(axis=-1)          # (N,)
            exc = np.maximum(0.0, dot - d_k)
            apex_pts -= exc[:, None] * n_k
        apex_pts[:, 2] = np.maximum(apex_pts[:, 2], 0.0)
        all_verts[apex_idx, 0] = cx + apex_pts[:, 0]
        all_verts[apex_idx, 1] = cy + apex_pts[:, 1]
        all_verts[apex_idx, 2] = base_z + apex_pts[:, 2]

    # ── Residual roughness: tiny per-vertex noise to break perfect flat faces ─
    if stones.roughness > 0.0:
        scale = (stones.roughness * mean_r)[:, None, None]
        wx += scale * rng.uniform(-1.0, 1.0, wx.shape)
        wy += scale * rng.uniform(-1.0, 1.0, wy.shape)
        wz += scale * 0.4 * rng.uniform(-1.0, 1.0, wz.shape)

    # ── Slope rotation: align stone with terrain normal ───────────────────────
    # Rotate the local offset of every ring vertex and the apex so the stone
    # sits flush on the slope rather than intersecting it.  The base-centre
    # vertex (bot_idx) is the pivot and needs no rotation.
    if np.any(_ns2 > 1e-9):
        dx_ = wx - cx[:, None, None]       # local offsets from base centre
        dy_ = wy - cy[:, None, None]
        dz_ = wz - base_z[:, None, None]
        # Broadcast (N,) rotation components to (N, 1, 1) for ring arrays
        wx = cx[:, None, None]     + (_R00[:, None, None]*dx_ + _R01[:, None, None]*dy_ + _R02[:, None, None]*dz_)
        wy = cy[:, None, None]     + (_R10[:, None, None]*dx_ + _R11[:, None, None]*dy_ + _R12[:, None, None]*dz_)
        wz = base_z[:, None, None] + (_R20[:, None, None]*dx_ + _R21[:, None, None]*dy_ + _R22[:, None, None]*dz_)
        # Rotate apex (written earlier; may have been modified by cuts)
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
    stone_bases = (np.arange(N) * vps).astype(np.int32)
    all_faces   = (canon_faces[None, :, :] +
                   stone_bases[:, None, None]).reshape(-1, 3)

    mesh = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    mesh.fix_normals()

    # ── Rasterise stone tops into support_z ───────────────────────────────────
    cw = surface.cell_w
    ch = surface.cell_w
    gw = surface.grid_w
    gh = surface.grid_h

    for s in range(N):
        _cx, _cy = cx[s], cy[s]
        _rx, _ry = rx_arr[s], ry_arr[s]
        _h       = height[s]
        _ca, _sa = ca[s], sa[s]
        _bz      = base_z[s]
        r_max    = max(_rx, _ry)

        i_lo = max(0,      int((_cx - r_max) / cw))
        i_hi = min(gw - 1, int((_cx + r_max) / cw) + 1)
        j_lo = max(0,      int((_cy - r_max) / ch))
        j_hi = min(gh - 1, int((_cy + r_max) / ch) + 1)
        if i_lo > i_hi or j_lo > j_hi:
            continue

        ii_g = np.arange(i_lo, i_hi + 1)
        jj_g = np.arange(j_lo, j_hi + 1)
        II, JJ = np.meshgrid(ii_g, jj_g)
        dx_g =  II * cw - _cx
        dy_g =  JJ * ch - _cy

        lx_g =  _ca * dx_g + _sa * dy_g
        ly_g = -_sa * dx_g + _ca * dy_g

        d2     = (lx_g / _rx) ** 2 + (ly_g / _ry) ** 2
        inside = d2 <= 1.0
        if not np.any(inside):
            continue

        z_top = np.where(inside, _bz + _h * np.sqrt(np.maximum(0.0, 1.0 - d2)), -np.inf)
        sl = support_z[j_lo:j_hi + 1, i_lo:i_hi + 1]
        np.maximum(sl, z_top, out=sl)

        if stone_mask is not None:
            stone_mask[j_lo:j_hi + 1, i_lo:i_hi + 1] |= inside

    return mesh
