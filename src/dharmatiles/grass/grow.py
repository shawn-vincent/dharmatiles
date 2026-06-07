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

    _sort_downstream_first(growing)
    species_map = {species.name: species for species in cfg.species}
    max_steps = max((path.seed.blade_n_steps for path in growing), default=0)

    for round_idx in range(max_steps):
        grown = 0
        for path in growing:
            if not path.alive or round_idx >= path.seed.blade_n_steps:
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
        groups = _voronoi_groups(n_groups, scene, surface, rng)
        for group in groups:
            group_dir = float(rng.uniform(0.0, 2.0 * np.pi))
            n_seeds = int(rng.integers(species.group_min, species.group_max + 1))

            for _ in range(n_seeds):
                x, y = _sample_seed_xy(group, surface, rng)
                ix, iy = _cell_index(surface, x, y)
                if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
                    continue
                if scene.stone_mask is not None and scene.stone_mask[iy, ix]:
                    continue

                terrain_z = float(scene.terrain_z[iy, ix])
                floor_z = float(occ_z[iy, ix])
                if floor_z - terrain_z > cfg.max_stack_height:
                    continue

                seed = _make_seed(x, y, group_dir, species, rng)
                z0 = max(terrain_z, floor_z) + seed.blade_clearance
                # For n=1 (flat) thickness has no effect above the equator.
                effective_top = 0.0 if species.n_top_facets == 1 else species.thickness
                last_stamp = _stamp_seed(occ_z, surface, x, y, z0, seed.blade_width)
                paths.append(GrowingPath(seed=seed, points=[(x, y, z0)], last_stamp=last_stamp))

    return paths


def _voronoi_groups(
    n: int,
    scene,
    surface,
    rng: np.random.Generator,
) -> list[dict[str, np.ndarray]]:
    """Partition the grass mask into random Voronoi-style clump cells."""
    if n <= 0:
        return []

    if scene.grass_mask is None:
        valid = np.ones((surface.grid_h, surface.grid_w), dtype=bool)
    else:
        valid = scene.grass_mask
    rows, cols = np.where(valid)
    if len(rows) == 0:
        return []

    n = min(n, len(rows))
    centers = _random_spread_sites(rows, cols, n, rng)
    labels = _nearest_site_labels(rows, cols, centers)

    groups: list[dict[str, np.ndarray]] = []
    for label in range(len(centers)):
        member_idx = np.flatnonzero(labels == label)
        if len(member_idx) == 0:
            continue
        groups.append({
            "rows": rows[member_idx],
            "cols": cols[member_idx],
        })
    return groups


def _random_spread_sites(
    rows: np.ndarray,
    cols: np.ndarray,
    n: int,
    rng: np.random.Generator,
) -> np.ndarray:
    """Choose random sites with a mild farthest-point bias for coverage."""
    first = int(rng.integers(0, len(rows)))
    chosen = [first]
    min_d2 = (rows - rows[first]) ** 2 + (cols - cols[first]) ** 2

    for _ in range(1, n):
        sample_size = min(len(rows), max(64, n * 8))
        candidates = rng.choice(len(rows), size=sample_size, replace=False)
        next_idx = int(candidates[np.argmax(min_d2[candidates])])
        chosen.append(next_idx)
        d2 = (rows - rows[next_idx]) ** 2 + (cols - cols[next_idx]) ** 2
        min_d2 = np.minimum(min_d2, d2)

    return np.column_stack([rows[chosen], cols[chosen]])


def _nearest_site_labels(
    rows: np.ndarray,
    cols: np.ndarray,
    centers: np.ndarray,
    chunk_size: int = 8192,
) -> np.ndarray:
    labels = np.empty(len(rows), dtype=np.int32)
    center_rows = centers[:, 0]
    center_cols = centers[:, 1]
    for start in range(0, len(rows), chunk_size):
        stop = min(start + chunk_size, len(rows))
        d2 = (
            (rows[start:stop, None] - center_rows[None, :]) ** 2
            + (cols[start:stop, None] - center_cols[None, :]) ** 2
        )
        labels[start:stop] = np.argmin(d2, axis=1)
    return labels


def _sample_seed_xy(
    group: dict[str, np.ndarray],
    surface,
    rng: np.random.Generator,
) -> tuple[float, float]:
    idx = int(rng.integers(0, len(group["rows"])))
    row = int(group["rows"][idx])
    col = int(group["cols"][idx])
    x = float((col + rng.uniform(0.05, 0.95)) * surface.cell_w)
    y = float((row + rng.uniform(0.05, 0.95)) * surface.cell_w)
    return x, y


def _sample_seed_curl(species: SpeciesConfig, rng: np.random.Generator) -> float:
    blade_curl_min = max(0.0, float(species.blade_curl_min))
    blade_curl_max = max(blade_curl_min, float(species.blade_curl_max))
    magnitude = float(rng.uniform(blade_curl_min, blade_curl_max))
    if magnitude == 0.0:
        return 0.0
    return magnitude * float(rng.choice([-1.0, 1.0]))


def _make_seed(
    x: float,
    y: float,
    group_dir: float,
    species: SpeciesConfig,
    rng: np.random.Generator,
) -> GrassSeed:
    blade_width = float(rng.uniform(species.blade_width_min, species.blade_width_max))
    target_length = float(rng.uniform(species.blade_length_min, species.blade_length_max))
    blade_n_steps = max(1, int(round(target_length / species.blade_segment_length)))
    curl = _sample_seed_curl(species, rng)
    # Store curl as radians per step, not total blade curl.
    curl_per_step = curl / max(blade_n_steps, 1)
    return GrassSeed(
        x=float(x),
        y=float(y),
        blade_direction=float(group_dir + rng.normal(0.0, species.group_dir_jitter)),
        blade_segment_length=float(species.blade_segment_length),
        blade_n_steps=blade_n_steps,
        blade_curl=curl_per_step,
        blade_width=blade_width,
        blade_rise_cap=float(species.blade_rise_cap),
        blade_clearance=float(species.blade_clearance),
        species_id=species.name,
    )


def _sort_downstream_first(paths: list[GrowingPath]) -> None:
    """Sort paths in-place so seeds furthest downstream come first.

    Sort key: projection of the seed (x, y) onto its own initial growth direction
    unit vector ``(sin(direction), cos(direction))``.  A higher value means the
    seed sits further in the direction this blade is already heading — i.e. it is
    downstream.

    Processing downstream blades before upstream blades in every round means their
    occ_z stamps are already present when upstream blades grow through the same
    area, so upstream blades rise to cross downstream ones rather than the reverse.
    The returned GrassPath list from grow_all preserves this order, so the mesh
    build phase automatically sees blades in the same downstream-first sequence.
    """
    paths.sort(
        key=lambda p: p.seed.x * np.sin(p.seed.blade_direction) + p.seed.y * np.cos(p.seed.blade_direction),
        reverse=True,
    )


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy


def _stamp_seed(occ_z: np.ndarray, surface, x: float, y: float, z: float, width: float) -> dict[tuple[int, int], float]:
    hw = width / 2.0
    ix0 = max(0, int((x - hw) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((x + hw) / surface.cell_w) + 1)
    iy0 = max(0, int((y - hw) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((y + hw) / surface.cell_w) + 1)
    np.maximum(occ_z[iy0:iy1 + 1, ix0:ix1 + 1], z, out=occ_z[iy0:iy1 + 1, ix0:ix1 + 1])
    return {
        (iy, ix): z
        for iy in range(iy0, iy1 + 1)
        for ix in range(ix0, ix1 + 1)
    }
