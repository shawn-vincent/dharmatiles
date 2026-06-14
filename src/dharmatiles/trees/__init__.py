"""Procedural deciduous tree generation — unified trunk + branch system.

The trunk and branches are one SCA skeleton; all edges share the same
scale-adaptive bark surface.

Public API (imported by ``scatter/trees.py``):

- ``build_tree(cx, cy, tz, cfg, rng)``   → ``(mesh, height_mm)``
- ``stamp_tree(cx, cy, tz, height_mm, cfg, support_z, obstacle_mask, surface)``
"""
from .tree import build_tree, stamp_tree

__all__ = ['build_tree', 'stamp_tree']
