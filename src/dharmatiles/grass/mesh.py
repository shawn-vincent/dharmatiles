"""Mesh construction and support rasterisation for completed grass paths."""

from __future__ import annotations

import numpy as np
import trimesh

from ._geometry import _sample_grid, _spine_distances, _stamp_segment
from .config import GrassConfig
from .growers import FlatGrassGrower
from .seed import GrassPath
from ..core.color import Material, tag as _tag


def build_meshes(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> list[trimesh.Trimesh]:
    """Build one complete blade mesh at a time, updating vegetation support after each path.

    For each full path (downstream-first order):
    1. Lift every non-root point to max(planned_z, vegetation_support_z).
    2. Build the mesh from the adjusted path.
    3. Update scene.vegetation_support_z from the actual mesh top surface using
       profile-aware, slope-aware rasterisation.
    """
    species = cfg.species
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        grower = FlatGrassGrower

        # Step 1: adjust path points against the current accumulated surface.
        lifted_points = _lift_path_points(path.points, scene.vegetation_support_z, surface)
        lifted_path = GrassPath(seed=path.seed, points=lifted_points)

        # Step 2: build mesh.
        mesh = grower.build_mesh(lifted_path, species, scene, surface)
        if mesh is not None:
            _tag(mesh, Material.GRASS)
            meshes.append(mesh)

        # Step 3: update vegetation support from actual mesh top surface.
        pts_arr = np.asarray(lifted_points, dtype=float)
        path_dists = _spine_distances(pts_arr)
        total_len = float(path_dists[-1])
        point_tapers = path.seed.distance_taper_vec(path_dists, total_len)
        pt_thicknesses = species.blade_thickness * point_tapers
        pt_widths = path.seed.blade_width * point_tapers

        for idx in range(1, len(pts_arr)):
            prev = pts_arr[idx - 1]
            curr = pts_arr[idx]
            _stamp_segment(
                scene.vegetation_support_z, surface,
                float(prev[0]), float(prev[1]),
                float(curr[0]), float(curr[1]),
                float(pt_widths[idx - 1]), float(pt_widths[idx]),
                float(prev[2]), float(curr[2]),
                float(pt_thicknesses[idx - 1]), float(pt_thicknesses[idx]),
                species.blade_top_facets,
            )

    return meshes


# ── Point lifting ─────────────────────────────────────────────────────────────

def _lift_path_points(
    points: list[tuple[float, float, float]],
    support_z: np.ndarray,
    surface,
) -> list[tuple[float, float, float]]:
    """Adjust path points against the current support surface.

    Ring 0 is the intentional terrain-sunk root.  Other points raise only:
    max(planned_z, floor_z).
    """
    lifted = []
    for idx, (x, y, z) in enumerate(points):
        if idx == 0:
            lifted.append((float(x), float(y), float(z)))
            continue
        floor_z = _sample_grid(support_z, surface, x, y)
        lifted.append((float(x), float(y), float(max(z, floor_z))))
    return lifted





