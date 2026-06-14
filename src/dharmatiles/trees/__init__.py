"""Procedural deciduous tree generation.

Public API (imported by ``scatter/trees.py``):

- ``build_trunk(cx, cy, tz, angle, cfg, rng)``
  → ``(mesh, apex_pos, apex_dir, height_mm)``

- ``stamp_trunk(cx, cy, tz, cfg, height_mm, support_z, obstacle_mask, surface)``

- ``build_branches(apex_pos, apex_dir, cx, cy, tz, height_mm, cfg, rng)``
  → ``trimesh.Trimesh | None``
"""
from .trunk    import build_trunk, stamp_trunk
from .branches import build_branches

__all__ = ['build_trunk', 'stamp_trunk', 'build_branches']
