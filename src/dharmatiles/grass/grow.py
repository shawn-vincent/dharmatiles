"""Seeding and growth orchestration for grass."""

from __future__ import annotations

import numpy as np

from .config import GrassConfig, SpeciesConfig
from .growers import GROWERS
from .seed import GrassPath, GrassSeed, GrowingPath


def grow_all(scene, surface, cfg: GrassConfig, rng: np.random.Generator, verbose: bool = True) -> list[GrassPath]:
    """Plant blades, then fully grow each blade before starting the next one."""
    occ_z = scene.vegetation_support_z.copy()
    growing = plant_seeds(scene, surface, cfg, occ_z, rng)

    if verbose:
        n_groups = sum(s.groups_per_square * surface.cols * surface.rows for s in cfg.species)
        print(f"  Planted {len(growing)} blades in {n_groups} groups")

    _sort_upstream_first(growing, surface)
    species_map = {species.name: species for species in cfg.species}
    total_segments = 0
    full_length_blades = 0

    for path in growing:
        if _vegetation_depth_at_seed(scene, surface, occ_z, path) > 0.0:
            path.alive = False
            continue

        species = species_map[path.seed.species_id]
        grower = GROWERS[species.grower]
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
) -> list[GrowingPath]:
    paths: list[GrowingPath] = []
    for species in cfg.species:
        n_groups = _scaled_voronoi_group_count(species.groups_per_square, scene, surface, rng)
        groups = _voronoi_groups(n_groups, scene, surface, rng)
        actual_n_groups = len(groups)
        for group in groups:
            group_dir = float(rng.uniform(0.0, 2.0 * np.pi))
            n_seeds = _scaled_group_seed_count(
                group,
                surface.grid_w * surface.grid_h,
                actual_n_groups,
                species.group_min,
                species.group_max,
                rng,
            )

            for x, y in _jitter_grid_xy(group, n_seeds, group_dir, surface, rng):
                ix, iy = _cell_index(surface, x, y)
                if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
                    continue
                if scene.stone_mask is not None and scene.stone_mask[iy, ix]:
                    continue

                terrain_z = float(scene.terrain_z[iy, ix])
                terrain_support_z = float(scene.terrain_support_z[iy, ix])
                if _vegetation_depth(scene.vegetation_support_z, terrain_support_z, ix, iy) > 0.0:
                    continue

                floor_z = max(terrain_support_z, float(occ_z[iy, ix]))
                if floor_z - terrain_z > cfg.max_stack_height:
                    continue

                seed = _make_seed(x, y, group_dir, species, rng)
                z0 = max(terrain_z, floor_z) + seed.blade_clearance - (
                    species.blade_thickness + seed.blade_clearance
                )
                paths.append(GrowingPath(seed=seed, points=[(x, y, z0)]))

    return paths


def _scaled_voronoi_group_count(
    groups_per_square: int,
    scene,
    surface,
    rng: np.random.Generator,
) -> int:
    """Scale Voronoi group count by grass-layer area over total tile area."""
    total_tile_cells = surface.grid_w * surface.grid_h
    if total_tile_cells <= 0:
        return 0

    if scene.grass_mask is None:
        grass_cells = total_tile_cells
    else:
        grass_cells = int(scene.grass_mask.sum())
    if grass_cells <= 0:
        return 0

    full_tile_groups = max(0, int(groups_per_square)) * surface.cols * surface.rows
    expected_groups = full_tile_groups * grass_cells / float(total_tile_cells)
    base_count = int(np.floor(expected_groups))
    if rng.random() < expected_groups - base_count:
        base_count += 1
    return max(1, base_count)


def _scaled_group_seed_count(
    group: dict[str, np.ndarray],
    total_tile_cells: int,
    n_groups: int,
    group_min: int,
    group_max: int,
    rng: np.random.Generator,
) -> int:
    """Sample blade count for a Voronoi group, scaled by its actual tile area."""
    if total_tile_cells <= 0 or n_groups <= 0:
        return 0

    min_count = max(0, int(group_min))
    max_count = max(min_count, int(group_max))
    nominal_count = float(rng.integers(min_count, max_count + 1))

    group_cells = len(group["rows"])
    ideal_group_cells = total_tile_cells / float(n_groups)
    scaled_count = nominal_count * group_cells / ideal_group_cells
    base_count = int(np.floor(scaled_count))
    if rng.random() < scaled_count - base_count:
        base_count += 1
    return base_count


def _vegetation_depth_at_seed(scene, surface, occ_z: np.ndarray, path: GrowingPath) -> float:
    seed = path.seed
    ix, iy = _cell_index(surface, seed.x, seed.y)
    terrain_support_z = float(scene.terrain_support_z[iy, ix])
    return _vegetation_depth(occ_z, terrain_support_z, ix, iy)


def _vegetation_depth(vegetation_support_z: np.ndarray, terrain_support_z: float, ix: int, iy: int) -> float:
    return max(0.0, float(vegetation_support_z[iy, ix]) - terrain_support_z)


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


def _jitter_grid_xy(
    group: dict[str, np.ndarray],
    n_target: int,
    group_dir: float,
    surface,
    rng: np.random.Generator,
) -> list[tuple[float, float]]:
    """Return jitter-grid (x, y) candidates whose cell belongs to this group.

    Spacing is chosen so the number of in-group grid points ≈ n_target.
    Each grid cell gets an independent uniform random jitter so coverage is
    thorough yet avoids the regular pattern of a strict lattice.

    An extra row of seeds is appended along the upstream edge (the group face
    closest to the tile boundary in ``group_dir``) so that there is no bald
    strip at the leading edge caused by the first grid row being offset by up
    to one full ``spacing`` from the boundary.
    """
    if n_target <= 0:
        return []

    group_rows = group["rows"]
    group_cols = group["cols"]
    n_cells = len(group_rows)
    if n_cells == 0:
        return []

    cell_w = surface.cell_w

    # Boolean membership mask — cheaper than a Python set for the final filter.
    group_mask = np.zeros((surface.grid_h, surface.grid_w), dtype=bool)
    group_mask[group_rows, group_cols] = True

    min_col = int(group_cols.min())
    max_col = int(group_cols.max())
    min_row = int(group_rows.min())
    max_row = int(group_rows.max())

    group_area = n_cells * cell_w * cell_w
    # spacing such that group_area / spacing² ≈ n_target; floor at half a cell.
    spacing = max(cell_w * 0.5, np.sqrt(group_area / max(n_target, 1)))

    bbox_w = (max_col - min_col + 1) * cell_w
    bbox_h = (max_row - min_row + 1) * cell_w
    n_x = max(1, int(np.ceil(bbox_w / spacing)))
    n_y = max(1, int(np.ceil(bbox_h / spacing)))

    x_base = min_col * cell_w
    y_base = min_row * cell_w

    # Vectorised jitter grid: each cell gets its own independent offset.
    xi = np.arange(n_x, dtype=float)
    yi = np.arange(n_y, dtype=float)
    xs = x_base + (xi[:, np.newaxis] + rng.uniform(0.0, 1.0, (n_x, n_y))) * spacing
    ys = y_base + (yi[np.newaxis, :] + rng.uniform(0.0, 1.0, (n_x, n_y))) * spacing

    xs_flat = xs.ravel()
    ys_flat = ys.ravel()

    # Cell indices for every candidate point (clipped to grid bounds).
    ixs = np.clip((xs_flat / cell_w).astype(int), 0, surface.grid_w - 1)
    iys = np.clip((ys_flat / cell_w).astype(int), 0, surface.grid_h - 1)

    valid = group_mask[iys, ixs]
    interior = list(zip(xs_flat[valid].tolist(), ys_flat[valid].tolist()))

    # Upstream edge row — fills the gap between the bounding-box edge and the
    # first jitter-grid row, which can be up to one full spacing wide.
    edge = _upstream_edge_row(group_rows, group_cols, group_dir, cell_w, spacing, rng)

    return interior + edge


def _upstream_edge_row(
    group_rows: np.ndarray,
    group_cols: np.ndarray,
    group_dir: float,
    cell_w: float,
    spacing: float,
    rng: np.random.Generator,
) -> list[tuple[float, float]]:
    """Scatter seeds along the upstream face — the face blades grow *away from*.

    The upstream face is the set of group cells whose centres sit within one
    cell width of the *minimum* projection onto the blade direction vector
    ``(sin(group_dir), cos(group_dir))``.  These are the source cells: blades
    rooted here grow across the full depth of the group before stopping.

    The seed count is proportional to the lateral width of the face at the
    jitter-grid density, but the positions are chosen by randomly sampling
    that many cells from the face — not spread on an even lattice.
    """
    dx = float(np.sin(group_dir))
    dy = float(np.cos(group_dir))

    cx = (group_cols + 0.5) * cell_w
    cy = (group_rows + 0.5) * cell_w
    proj_u = cx * dx + cy * dy

    # Source face: cells closest to the *back* of the group in blade direction.
    min_u = float(proj_u.min())
    edge_mask = proj_u <= min_u + cell_w

    edge_rows = group_rows[edge_mask]
    edge_cols = group_cols[edge_mask]

    if len(edge_rows) == 0:
        return []

    # Lateral extent — used only to estimate how many seeds to scatter.
    px = float(-np.cos(group_dir))
    py = float(np.sin(group_dir))
    edge_v = (edge_cols + 0.5) * cell_w * px + (edge_rows + 0.5) * cell_w * py
    lateral_extent = float(edge_v.max() - edge_v.min()) + cell_w
    n_seeds = max(1, int(np.round(lateral_extent / spacing)))

    # Random sample (without replacement, capped at available cells).
    n_pick = min(n_seeds, len(edge_rows))
    chosen = rng.choice(len(edge_rows), size=n_pick, replace=False)

    result: list[tuple[float, float]] = []
    for idx in chosen:
        r = int(edge_rows[idx])
        c = int(edge_cols[idx])
        x = (c + rng.uniform(0.05, 0.95)) * cell_w
        y = (r + rng.uniform(0.05, 0.95)) * cell_w
        result.append((x, y))

    return result


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
    )


def _sort_upstream_first(paths: list[GrowingPath], surface) -> None:
    """Sort paths in-place: upstream (closest to tile edge in travel direction) first.

    Primary key — upstream distance: the minimum distance from the seed to the
    tile boundary along the blade's initial travel direction.  Seeds that are
    physically close to the edge they are heading toward are upstream; they get
    grown and stamped into occ_z first, so interior blades that pass over the
    same area subsequently ride up on top of them.

    Secondary key — blade direction normalised to [0, 2π): groups blades that
    point in the same direction together within each upstream band, so the mesh
    draw order is coherent even when two seeds share the same boundary distance.

    Both keys sort ascending (small upstream-distance first; 0° before 360°).
    """
    TWO_PI = 2.0 * np.pi

    def _key(p: GrowingPath) -> tuple[float, float]:
        seed = p.seed
        dx = float(np.sin(seed.blade_direction))
        dy = float(np.cos(seed.blade_direction))

        dists: list[float] = []
        if dx > 1e-9:
            dists.append((surface.tile_w - seed.x) / dx)
        elif dx < -1e-9:
            dists.append(seed.x / (-dx))
        if dy > 1e-9:
            dists.append((surface.tile_h - seed.y) / dy)
        elif dy < -1e-9:
            dists.append(seed.y / (-dy))

        boundary_dist = min(dists) if dists else 0.0
        dir_norm = seed.blade_direction % TWO_PI
        return (boundary_dist, dir_norm)

    paths.sort(key=_key)


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy
