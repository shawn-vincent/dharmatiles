"""
Unified tree orchestrator: skeleton → radii → surface → mesh.

Public API
----------
``build_tree(cx, cy, tz, cfg, rng)``
    Build a complete tree mesh.  Returns ``(mesh, height_mm)``.

``stamp_tree(cx, cy, tz, height_mm, cfg, support_z, obstacle_mask, surface)``
    Rasterise the trunk footprint into scene arrays so grass steers around it.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..dist import sample, bounds as _bounds
from .skeleton import grow_skeleton
from .radii    import assign_radii
from .surface  import build_tree_mesh


def build_tree(
    cx:  float,
    cy:  float,
    tz:  float,
    cfg,
    rng: np.random.Generator,
) -> tuple[trimesh.Trimesh, float]:
    """Grow and mesh a unified tree (trunk + branches as one structure).

    Returns
    -------
    mesh      : complete tree trimesh
    height_mm : ``max(nodes_xyz[:,2]) - tz``, used for footprint stamping
    """
    r_root = float(sample(cfg.r_base_mm, rng))

    nodes_xyz, parents, arc_dists, crown_base_z = grow_skeleton(cx, cy, tz, cfg, rng)

    if len(nodes_xyz) <= 1:
        # SCA produced no growth (degenerate case — no attractors reached)
        return trimesh.Trimesh(process=False), 0.0

    radii = assign_radii(parents, cfg.branch_r_tip_mm, r_root)

    mesh = build_tree_mesh(
        nodes_xyz, parents, radii, arc_dists,
        cfg, rng, tz, crown_base_z,
    )

    height_mm = float(nodes_xyz[:, 2].max()) - tz
    return mesh, height_mm


def stamp_tree(
    cx:           float,
    cy:           float,
    tz:           float,
    height_mm:    float,
    cfg,
    support_z:    np.ndarray,
    obstacle_mask: np.ndarray | None,
    surface,
) -> None:
    """Rasterise the trunk base circle into *support_z* and *obstacle_mask*.

    The stamped radius is ``r_base_max * (1 + flare_amp)`` — large enough to
    cover the widest possible root flare.  Grass seeds inside this radius are
    blocked, so blades grow around the tree base rather than through it.
    """
    r_max   = float(_bounds(cfg.r_base_mm)[1]) * (1.0 + cfg.flare_amp)
    block_z = tz + height_mm

    cw = surface.cell_w
    gw = surface.grid_w
    gh = surface.grid_h

    i_lo = max(0,      int((cx - r_max) / cw))
    i_hi = min(gw - 1, int((cx + r_max) / cw) + 1)
    j_lo = max(0,      int((cy - r_max) / cw))
    j_hi = min(gh - 1, int((cy + r_max) / cw) + 1)
    if i_lo > i_hi or j_lo > j_hi:
        return

    ii = np.arange(i_lo, i_hi + 1)
    jj = np.arange(j_lo, j_hi + 1)
    II, JJ  = np.meshgrid(ii, jj)
    dx      = II * cw - cx
    dy      = JJ * cw - cy
    inside  = (dx ** 2 + dy ** 2) <= r_max ** 2

    if not np.any(inside):
        return

    sl = support_z[j_lo:j_hi + 1, i_lo:i_hi + 1]
    np.maximum(sl, np.where(inside, block_z, -np.inf), out=sl)

    if obstacle_mask is not None:
        obstacle_mask[j_lo:j_hi + 1, i_lo:i_hi + 1] |= inside
