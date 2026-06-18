"""Seeding and growth orchestration for grass."""

from __future__ import annotations

import numpy as np

from ._geometry import _cell_index
from .config import GrassConfig, SpeciesConfig
from .growers import FlatGrassGrower
from .seed import GrassPath, GrassSeed, GrowingPath

# Distribution helpers live in scatter/distribute.py and are shared with
# the rock prototype.  Import them here so the rest of this module can use
# the same names as before without changes.
from ..scatter.config import Grouped as _Grouped
from ..scatter.distribute import (
    scaled_voronoi_group_count  as _scaled_voronoi_group_count,
    scaled_group_seed_count     as _scaled_group_seed_count,
    voronoi_groups              as _voronoi_groups,
    random_spread_sites         as _random_spread_sites,
    nearest_site_labels         as _nearest_site_labels,
    jitter_grid_xy              as _jitter_grid_xy,
)
from ..dist import bounds, sample


def grow_all(
    scene,
    surface,
    cfg: GrassConfig,
    rng: np.random.Generator,
    verbose: bool = True,
    placement_mask=None,
    placement=None,
) -> list[GrassPath]:
    """Plant blades, then fully grow each blade before starting the next one."""
    occ_z = scene.vegetation_support_z.copy()
    growing = plant_seeds(scene, surface, cfg, occ_z, rng,
                          placement_mask=placement_mask, placement=placement)

    if verbose:
        _pl = placement or _Grouped()
        n_groups = _pl.groups_per_square * surface.cols * surface.rows
        print(f"  Planted {len(growing)} blades in ~{n_groups} groups")

    _sort_upstream_first(growing, surface)
    species = cfg.species
    total_segments = 0
    full_length_blades = 0

    for path in growing:
        if _vegetation_depth_at_seed(scene, surface, occ_z, path) > 0.0:
            path.alive = False
            continue

        grower = FlatGrassGrower
        grown_segments = 0

        for _ in range(path.seed.blade_n_steps):
            if not path.alive:
                break
            if not grower.step(path, occ_z, scene, surface, cfg, species):
                break
            grown_segments += 1

        total_segments += grown_segments
        if grown_segments == path.seed.blade_n_steps:
            full_length_blades += 1

    if verbose:
        viable_blades = sum(1 for path in growing if len(path.points) >= 2)
        print(
            f"  Grew {total_segments} segments across {viable_blades} blades "
            f"({full_length_blades} reached full length)"
        )

    return [
        GrassPath(seed=path.seed, points=path.points)
        for path in growing
        if len(path.points) >= 2
    ]


def plant_seeds(
    scene,
    surface,
    cfg: GrassConfig,
    occ_z: np.ndarray,
    rng: np.random.Generator,
    placement_mask=None,
    placement=None,
) -> list[GrowingPath]:
    _placement = placement or _Grouped()
    species = cfg.species
    paths: list[GrowingPath] = []
    n_groups = _scaled_voronoi_group_count(_placement.groups_per_square, placement_mask, surface, rng)
    groups = _voronoi_groups(n_groups, surface, rng, mask=placement_mask)
    for group in groups:
        group_dir = float(rng.uniform(0.0, 2.0 * np.pi))
        n_seeds = _scaled_group_seed_count(
            group,
            _placement.gap_mm,
            float(bounds(species.blade_width)[1]),
            surface.cell_w,
            rng,
        )

        for x, y in _jitter_grid_xy(group, n_seeds, group_dir, surface, rng):
            ix, iy = _cell_index(surface, x, y)
            if placement_mask is not None and not placement_mask[iy, ix]:
                continue
            if scene.obstacle_mask is not None and scene.obstacle_mask[iy, ix]:
                continue

            terrain_z = float(scene.terrain_z[iy, ix])
            terrain_support_z = float(scene.terrain_support_z[iy, ix])
            if _vegetation_depth(scene.vegetation_support_z, terrain_support_z, ix, iy) > 0.0:
                continue

            floor_z = max(terrain_support_z, float(occ_z[iy, ix]))
            if floor_z - terrain_z > cfg.max_stack_height:
                continue

            seed = _make_seed(x, y, group_dir, species, rng, surface)
            z0 = max(terrain_z, floor_z) + seed.blade_clearance - (
                species.blade_thickness + seed.blade_clearance
            )
            paths.append(GrowingPath(seed=seed, points=[(x, y, z0)]))

    return paths


def _vegetation_depth_at_seed(scene, surface, occ_z: np.ndarray, path: GrowingPath) -> float:
    seed = path.seed
    ix, iy = _cell_index(surface, seed.x, seed.y)
    terrain_support_z = float(scene.terrain_support_z[iy, ix])
    return _vegetation_depth(occ_z, terrain_support_z, ix, iy)


def _vegetation_depth(vegetation_support_z: np.ndarray, terrain_support_z: float, ix: int, iy: int) -> float:
    return max(0.0, float(vegetation_support_z[iy, ix]) - terrain_support_z)


def _sample_seed_curl(species: SpeciesConfig, rng: np.random.Generator) -> float:
    """Return total arc sweep in radians for one blade.

    The user-facing ``blade_curl`` parameter is a dimensionless fraction of π
    (180°).  A value of 1.0 means the tip sweeps all the way back — "too much"
    curl — so sensible defaults live in the 0.2–0.5 range (36°–90°).
    """
    magnitude = max(0.0, float(sample(species.blade_curl, rng)))
    if magnitude == 0.0:
        return 0.0
    # Scale from fraction-of-π to radians, then pick random left/right.
    return magnitude * np.pi * float(rng.choice([-1.0, 1.0]))


def _make_seed(
    x: float,
    y: float,
    group_dir: float,
    species: SpeciesConfig,
    rng: np.random.Generator,
    surface=None,
) -> GrassSeed:
    blade_width = float(sample(species.blade_width, rng))
    target_length = float(sample(species.blade_length, rng))
    blade_n_steps = max(1, int(round(target_length / species.blade_segment_length)))
    curl = _sample_seed_curl(species, rng)
    # Store curl as radians per step, not total blade curl.
    curl_per_step   = curl / max(blade_n_steps, 1)
    blade_direction = float(group_dir + sample(species.blade_direction_jitter, rng))
    return GrassSeed(
        x=float(x),
        y=float(y),
        blade_direction=blade_direction,
        blade_segment_length=float(species.blade_segment_length),
        blade_n_steps=blade_n_steps,
        blade_taper=float(species.blade_taper),
        blade_base_width=float(species.blade_base_width),
        blade_base_taper=float(
            species.blade_taper if species.blade_base_taper is None else species.blade_base_taper
        ),
        blade_curl=curl_per_step,
        blade_width=blade_width,
        blade_rise_cap=float(species.blade_rise_cap),
        blade_clearance=float(species.blade_clearance),
        species_id=species.name,
        upstream_dist=_compute_upstream_dist(x, y, blade_direction, surface),
    )


def _compute_upstream_dist(x: float, y: float, direction: float, surface) -> float:
    """Distance from (x, y) to the tile boundary along *direction*.

    This is the "upstream distance" used as the secondary sort key for grass:
    blades closest to the boundary they face grow first so interior blades
    ride on top of them.  Returns 0.0 if surface is None.
    """
    if surface is None:
        return 0.0
    dx = float(np.sin(direction))
    dy = float(np.cos(direction))
    dists: list[float] = []
    if dx > 1e-9:
        dists.append((surface.tile_w - x) / dx)
    elif dx < -1e-9:
        dists.append(x / (-dx))
    if dy > 1e-9:
        dists.append((surface.tile_h - y) / dy)
    elif dy < -1e-9:
        dists.append(y / (-dy))
    return min(dists) if dists else 0.0


def _sort_upstream_first(paths: list[GrowingPath], surface) -> None:
    """Sort paths in-place using GrassSeed.sort_key().

    GrassSeed.sort_key() returns (priority=1, upstream_dist, dir_norm).
    Since all seeds here share priority=1, the effective sort is:
      1. upstream_dist ascending — seeds closest to the boundary they face
         grow first so interior blades ride on top of outer ones.
      2. blade direction [0, 2π) — tiebreaker: groups same-direction blades
         together within each upstream band.
    """
    paths.sort(key=lambda p: p.seed.sort_key())
