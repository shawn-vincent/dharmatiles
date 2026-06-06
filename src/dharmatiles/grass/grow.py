"""Seeding and growth orchestration for grass."""

from __future__ import annotations

import numpy as np

from .config import GrassConfig, SpeciesConfig
from .growers import GROWERS
from .seed import GrassPath, GrassSeed, GrowingPath


def grow_all(scene, surface, cfg: GrassConfig, rng: np.random.Generator, verbose: bool = True) -> list[GrassPath]:
    occ_z = scene.support_z.copy()
    growing = plant_seeds(scene, surface, cfg, occ_z, rng)

    if verbose:
        n_groups = sum(s.groups_per_square * surface.cols * surface.rows for s in cfg.species)
        print(f"  Planted {len(growing)} blades in {n_groups} groups")

    rng.shuffle(growing)
    species_map = {species.name: species for species in cfg.species}
    max_steps = max((path.seed.n_steps for path in growing), default=0)

    for round_idx in range(max_steps):
        grown = 0
        for path in growing:
            if not path.alive or round_idx >= path.seed.n_steps:
                path.alive = False
                continue
            species = species_map[path.seed.species_id]
            grower = GROWERS[species.grower]
            if grower.step(path, occ_z, scene, surface, cfg, species):
                grown += 1

        if verbose:
            alive = sum(1 for path in growing if path.alive)
            print(f"  Round {round_idx + 1:2d}: {grown:3d} segments grown, {alive:3d} blades still alive")
        if grown == 0:
            break

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
) -> list[GrowingPath]:
    paths: list[GrowingPath] = []
    for species in cfg.species:
        n_groups = species.groups_per_square * surface.cols * surface.rows
        margin = species.width_max / 2.0
        for gx, gy in _jittered_group_centers(n_groups, surface, margin, rng):
            group_dir = float(rng.uniform(0.0, 2.0 * np.pi))
            group_curl = _sample_group_curl(species, rng)
            n_seeds = int(rng.integers(species.group_min, species.group_max + 1))

            for _ in range(n_seeds):
                x, y = _sample_seed_xy(gx, gy, species.group_spread_mm, surface, margin, rng)
                ix, iy = _cell_index(surface, x, y)
                if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
                    continue
                if scene.stone_mask is not None and scene.stone_mask[iy, ix]:
                    continue

                terrain_z = float(scene.terrain_z[iy, ix])
                floor_z = float(occ_z[iy, ix])
                if floor_z - terrain_z > cfg.max_stack_height:
                    continue

                seed = _make_seed(x, y, group_dir, group_curl, species, rng)
                z0 = max(terrain_z, floor_z) + cfg.clearance
                paths.append(GrowingPath(seed=seed, points=[(x, y, z0)]))
                _stamp_seed(occ_z, surface, x, y, z0 + species.thickness, seed.width)

    return paths


def _jittered_group_centers(
    n: int,
    surface,
    margin: float,
    rng: np.random.Generator,
) -> list[tuple[float, float]]:
    if n <= 0:
        return []
    cols = max(1, round(np.sqrt(n * surface.tile_w / surface.tile_h)))
    rows = int(np.ceil(n / cols))
    usable_w = max(surface.tile_w - 2.0 * margin, surface.cell_w)
    usable_h = max(surface.tile_h - 2.0 * margin, surface.cell_w)
    cell_w = usable_w / cols
    cell_h = usable_h / rows

    centers = []
    for row in range(rows):
        for col in range(cols):
            x = margin + (col + rng.uniform(0.1, 0.9)) * cell_w
            y = margin + (row + rng.uniform(0.1, 0.9)) * cell_h
            centers.append((float(np.clip(x, margin, surface.tile_w - margin)),
                            float(np.clip(y, margin, surface.tile_h - margin))))
    order = rng.permutation(len(centers))[:n]
    return [centers[int(i)] for i in order]


def _sample_seed_xy(
    gx: float,
    gy: float,
    spread: float,
    surface,
    margin: float,
    rng: np.random.Generator,
) -> tuple[float, float]:
    angle = float(rng.uniform(0.0, 2.0 * np.pi))
    radius = float(spread * np.sqrt(rng.uniform(0.0, 1.0)))
    x = float(np.clip(gx + radius * np.cos(angle), margin, surface.tile_w - margin))
    y = float(np.clip(gy + radius * np.sin(angle), margin, surface.tile_h - margin))
    return x, y


def _sample_group_curl(species: SpeciesConfig, rng: np.random.Generator) -> float:
    return float(rng.uniform(-species.curl_max, species.curl_max))


def _make_seed(
    x: float,
    y: float,
    group_dir: float,
    group_curl: float,
    species: SpeciesConfig,
    rng: np.random.Generator,
) -> GrassSeed:
    width = float(rng.uniform(species.width_min, species.width_max))
    target_length = float(rng.uniform(species.length_min, species.length_max))
    n_steps = max(1, int(round(target_length / species.step_len)))
    curl = float(np.clip(
        group_curl + rng.normal(0.0, species.curl_jitter),
        -species.curl_max,
        species.curl_max,
    ))
    # Store curl as radians per step, not total blade curl.
    curl_per_step = curl / max(n_steps, 1)
    return GrassSeed(
        x=float(x),
        y=float(y),
        direction=float(group_dir + rng.normal(0.0, species.dir_jitter)),
        step_len=float(species.step_len),
        n_steps=n_steps,
        curl=curl_per_step,
        width=width,
        rise_cap=float(species.rise_cap),
        species_id=species.name,
    )


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy


def _stamp_seed(occ_z: np.ndarray, surface, x: float, y: float, z: float, width: float) -> None:
    hw = width / 2.0
    ix0 = max(0, int((x - hw) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((x + hw) / surface.cell_w) + 1)
    iy0 = max(0, int((y - hw) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((y + hw) / surface.cell_w) + 1)
    np.maximum(occ_z[iy0:iy1 + 1, ix0:ix1 + 1], z, out=occ_z[iy0:iy1 + 1, ix0:ix1 + 1])
