"""Seeding and growth orchestration for grass."""

from __future__ import annotations

import numpy as np

from ._geometry import _cell_index
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
                species.gap_mm,
                species.blade_width_max,
                surface.cell_w,
                rng,
            )

            for x, y in _jitter_grid_xy(group, n_seeds, group_dir, surface, rng):
                ix, iy = _cell_index(surface, x, y)
                if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
                    continue
                if scene.rock_mask is not None and scene.rock_mask[iy, ix]:
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
    gap_mm: float,
    blade_width_max: float,
    cell_w: float,
    rng: np.random.Generator,
) -> int:
    """Compute blade count for a Voronoi group from a gap-between-blades distance.

    Centre-to-centre spacing = blade_width_max + gap_mm.
    density = 1 / spacing²  (blades per mm²).
    gap_mm = 0 → blades packed edge-to-edge; gap_mm = 2 (default) → one full
    blade-width of clear space between neighbours on average.
    """
    spacing = max(float(blade_width_max) + max(0.0, float(gap_mm)), 1e-3)
    density = 1.0 / (spacing * spacing)
    group_area_mm2 = len(group["rows"]) * cell_w * cell_w
    scaled_count = density * group_area_mm2
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
    """Jitter-grid candidates with source-edge-biased density.

    The grid is laid out in a rotated (u, v) frame:
      u — along the blade direction (sin θ, cos θ); the source edge (face
          blades grow *away from*) sits at minimum u.
      v — lateral, perpendicular to u.

    The v axis uses uniform spacing.  The u axis is warped by a power
    function so rows pile up near u_min and thin out toward u_max — more
    seeds close to the source edge, fewer far away.  Seeds can start right
    at the source edge (u_lo = u_min − ½ cell_w).

    Total grid cells ≈ n_target; actual in-group count is proportional to
    the group's fill fraction of its (u, v) bounding rectangle.
    """
    if n_target <= 0:
        return []

    group_rows = group["rows"]
    group_cols = group["cols"]
    n_cells = len(group_rows)
    if n_cells == 0:
        return []

    cell_w = surface.cell_w

    group_set = frozenset(zip(group_rows.tolist(), group_cols.tolist()))

    # Rotated frame aligned with the blade direction.
    dx = float(np.sin(group_dir))
    dy = float(np.cos(group_dir))
    px = float(-np.cos(group_dir))   # lateral direction
    py = float(np.sin(group_dir))

    cx = (group_cols + 0.5) * cell_w
    cy = (group_rows + 0.5) * cell_w
    proj_u = cx * dx + cy * dy
    proj_v = cx * px + cy * py

    u_lo = float(proj_u.min()) - 0.5 * cell_w   # source edge — grid starts here
    u_hi = float(proj_u.max()) + 0.5 * cell_w
    v_lo = float(proj_v.min()) - 0.5 * cell_w
    v_hi = float(proj_v.max()) + 0.5 * cell_w
    u_span = u_hi - u_lo
    v_span = v_hi - v_lo

    group_area = n_cells * cell_w * cell_w
    spacing = max(cell_w * 0.5, np.sqrt(group_area / max(n_target, 1)))

    n_u = max(1, int(np.ceil(u_span / spacing)))
    n_v = max(1, int(np.ceil(v_span / spacing)))

    # Both position and count use a (1-t)² power law so density falls off
    # quadratically from the source edge toward the far side.
    #
    # Position warp: u-row i occupies the physical range
    #   [u_lo + (i/n_u)² · u_span,  u_lo + ((i+1)/n_u)² · u_span]
    # so rows are compressed toward u_lo.
    #
    # Count weight: row i gets n_v_i ≈ n_v · (1 − i/n_u)² · scale seeds,
    # where scale normalises the total count to n_u · n_v ≈ n_target.
    t_row = np.arange(n_u, dtype=float) / n_u              # 0 at source → 1 at far
    count_w_raw = 0.5 * (1.0 - t_row) ** 2 + 0.5          # half-strength quadratic
    count_w = count_w_raw * n_u / count_w_raw.sum()        # mean weight = 1

    all_us: list[np.ndarray] = []
    all_vs: list[np.ndarray] = []

    for i in range(n_u):
        n_v_i = max(1, int(round(float(n_v * count_w[i]))))
        t_lo_i = float((i / n_u) ** 2)
        t_hi_i = float(((i + 1.0) / n_u) ** 2)

        j_u = rng.uniform(0.0, 1.0, n_v_i)
        j_v = rng.uniform(0.0, 1.0, n_v_i)

        all_us.append(u_lo + (t_lo_i + j_u * (t_hi_i - t_lo_i)) * u_span)
        all_vs.append(v_lo + (np.arange(n_v_i) + j_v) * (v_span / n_v_i))

    us = np.concatenate(all_us)
    vs = np.concatenate(all_vs)

    # Back to world (x, y).
    xs = us * dx + vs * px
    ys = us * dy + vs * py

    ixs = np.clip((xs / cell_w).astype(int), 0, surface.grid_w - 1)
    iys = np.clip((ys / cell_w).astype(int), 0, surface.grid_h - 1)

    valid = np.fromiter(
        ((int(iy), int(ix)) in group_set for iy, ix in zip(iys.tolist(), ixs.tolist())),
        dtype=bool, count=len(iys),
    )
    return list(zip(xs[valid].tolist(), ys[valid].tolist()))


def _sample_seed_curl(species: SpeciesConfig, rng: np.random.Generator) -> float:
    """Return total arc sweep in radians for one blade.

    The user-facing ``blade_curl`` parameters are dimensionless fractions of π
    (180°).  A value of 1.0 means the tip sweeps all the way back — "too much"
    curl — so sensible defaults live in the 0.2–0.5 range (36°–90°).
    """
    blade_curl_min = max(0.0, float(species.blade_curl_min))
    blade_curl_max = max(blade_curl_min, float(species.blade_curl_max))
    magnitude = float(rng.uniform(blade_curl_min, blade_curl_max))
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


