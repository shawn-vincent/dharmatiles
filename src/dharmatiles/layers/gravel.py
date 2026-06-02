"""
GravelLayer: batch-vectorised half-ellipsoid stones placed on terrain.

All stone geometry is built with NumPy broadcasting in a single pass.
Stone tops are rasterised into the scene's support_z so that subsequent
layers (grass blades) are forced to sit above the stones.
"""
from __future__ import annotations

from typing import List

import numpy as np
import trimesh

from ..core.config import SurfaceConfig, GravelConfig
from ..core.tile import TileScene
from ..core.grid import sample_grid


class GravelLayer:
    """Place random stones across the tile surface and update support_z."""

    def __init__(self, surface: SurfaceConfig, gravel: GravelConfig) -> None:
        self.surface = surface
        self.gravel  = gravel

    def build(self, scene: TileScene) -> List[trimesh.Trimesh]:
        """Add stone geometry to *scene.support_z* and return mesh list."""
        tile_area = self.surface.tile_cols * self.surface.tile_rows
        n_stones  = self.gravel.gravel_per_tile * tile_area
        rng  = np.random.default_rng(self.surface.seed + 7919)
        mesh = _build_gravel_mesh(self.surface, self.gravel, n_stones,
                                  scene.terrain_z, scene.support_z, rng)
        return [mesh]


# ── Internal implementation ───────────────────────────────────────────────────

def _build_gravel_mesh(surface: SurfaceConfig, gravel: GravelConfig,
                       n_stones: int,
                       terrain_z: np.ndarray, support_z: np.ndarray,
                       rng: np.random.Generator) -> trimesh.Trimesh:
    """Place *n_stones* stones; return a single merged Trimesh."""
    N  = n_stones
    AZ = gravel.az_segs
    EL = gravel.el_segs

    # Power-law size draw: U^power skews toward r_min; power=1 = uniform
    u_x    = rng.uniform(0.0, 1.0, N) ** gravel.size_power
    rx_arr = gravel.r_min + (gravel.r_max - gravel.r_min) * u_x
    # Second axis: aspect ratio bounded to [aspect_min, 1.0] so rocks stay roundish
    aspect = rng.uniform(gravel.aspect_min, 1.0, N)
    ry_arr = rx_arr * aspect
    h_frac  = rng.uniform(gravel.flat_min, gravel.flat_max, N)
    height  = 0.5 * (rx_arr + ry_arr) * h_frac
    angle   = rng.uniform(0, np.pi, N)
    margin  = np.maximum(rx_arr, ry_arr)

    span_x = np.maximum(surface.tile_w - 2 * margin, 0.0)
    span_y = np.maximum(surface.tile_h - 2 * margin, 0.0)
    cx = margin + rng.uniform(0, 1, N) * span_x
    cy = margin + rng.uniform(0, 1, N) * span_y

    ca, sa = np.cos(angle), np.sin(angle)
    tz     = sample_grid(terrain_z, surface, cx, cy)
    base_z = tz - gravel.sink

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
    ch = surface.cell_h
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

    return mesh
