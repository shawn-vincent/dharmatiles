"""Mesh construction and support rasterisation for completed grass paths."""

from __future__ import annotations

import numpy as np
import trimesh

from ._geometry import _contained_segment_cells, _sample_grid, _spine_distances
from .config import GrassConfig
from .growers import GROWERS
from .seed import GrassPath


def build_meshes(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> list[trimesh.Trimesh]:
    """Build one complete blade mesh at a time, updating vegetation support after each path.

    For each full path (downstream-first order):
    1. Lift every non-root point to max(planned_z, vegetation_support_z).
    2. Build the mesh from the adjusted path.
    3. Update scene.vegetation_support_z from the actual mesh top surface using
       profile-aware, slope-aware rasterisation.
    """
    species_map = {species.name: species for species in cfg.species}
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        species = species_map[path.seed.species_id]
        grower = GROWERS[species.grower]

        # Step 1: adjust path points against the current accumulated surface.
        lifted_points = _lift_path_points(path.points, scene.vegetation_support_z, surface)
        lifted_path = GrassPath(seed=path.seed, points=lifted_points)

        # Step 2: build mesh.
        mesh = grower.build_mesh(lifted_path, species, scene, surface)
        if mesh is not None:
            meshes.append(mesh)

        # Step 3: update vegetation support from actual mesh top surface.
        n_pts = len(lifted_points)
        path_dists = _spine_distances(np.asarray(lifted_points, dtype=float))
        total_len = float(path_dists[-1])
        point_tapers = np.array([path.seed.distance_taper(d, total_len) for d in path_dists], dtype=float)
        pt_thicknesses = species.blade_thickness * point_tapers
        pt_widths = path.seed.blade_width * point_tapers

        _rasterise_sloped_path(
            scene.vegetation_support_z,
            surface,
            np.asarray(lifted_points, dtype=float),
            pt_widths,
            pt_thicknesses,
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



# ── Profile-aware, slope-aware support rasterisation ─────────────────────────

def _rasterise_sloped_path(
    support_z: np.ndarray,
    surface,
    path: np.ndarray,           # (n, 3) spine positions
    widths: np.ndarray,         # (n,) blade widths per point
    thicknesses: np.ndarray,    # (n,) top-profile peak heights per point
    n_top_facets: int,
) -> None:
    """Stamp the blade top surface into support_z.

    Each segment is stamped with z values that:
    - interpolate linearly along the segment between the two endpoint spine z
      values (slope-aware), and
    - vary laterally across the blade width according to the top-profile shape
      (profile-aware): flat at equator for n=1, sine arc for n≥2.
    """
    if len(path) == 0:
        return
    for idx in range(1, len(path)):
        prev = path[idx - 1]
        curr = path[idx]
        _stamp_segment_profile(
            support_z,
            surface,
            prev,
            curr,
            float(widths[idx - 1]),
            float(widths[idx]),
            float(prev[2]),
            float(curr[2]),
            float(thicknesses[idx - 1]),
            float(thicknesses[idx]),
            n_top_facets,
        )


def _stamp_segment_profile(
    support_z: np.ndarray,
    surface,
    p0: np.ndarray,
    p1: np.ndarray,
    width0: float,
    width1: float,
    z0: float,          # spine z at p0
    z1: float,          # spine z at p1
    t0: float,          # top-profile peak height at p0
    t1: float,          # top-profile peak height at p1
    n_top_facets: int,
) -> None:
    """Stamp the swept footprint of one segment with slope + profile awareness.

    For n=1 (flat): every cell is stamped at the interpolated spine z (top IS
    the equator; thickness contributes nothing).
    For n≥2: the stamp height also rises laterally toward the blade centre
    following thickness × sin(π × x_frac), where x_frac ∈ [0, 1] across width.
    """
    x0, y0 = float(p0[0]), float(p0[1])
    x1, y1 = float(p1[0]), float(p1[1])
    footprint = _contained_segment_cells(surface, x0, y0, x1, y1, width0 / 2.0, width1 / 2.0)
    if footprint is None:
        return
    ix0g, ix1g, iy0g, iy1g, mask, along_norm, lateral_frac = footprint
    if not np.any(mask):
        return
    z_spine  = z0 + (z1 - z0) * along_norm          # slope along segment
    t_interp = t0 + (t1 - t0) * along_norm          # profile height along segment

    if n_top_facets == 1:
        z_field = z_spine                            # flat: top IS equator
    else:
        z_field = z_spine + t_interp * np.sin(np.pi * lateral_frac)

    block = support_z[iy0g:iy1g + 1, ix0g:ix1g + 1]
    np.maximum(block, np.where(mask, z_field, block), out=block)


