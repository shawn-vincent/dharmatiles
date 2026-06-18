"""
Spatial distribution helpers for the scatter placement system.

These functions are shared by the rocks prototype (optional Voronoi grouping
or simple uniform-random placement) and the grass pipeline (Voronoi groups
with a jitter grid inside each cell).

The key exported function is ``scatter_positions``: given a ScatterConfig it
returns a list of ``(x, y, group_dir)`` tuples for seed placement.  Callers
then invoke ``prototype.make_seed(x, y, group_dir, rng)`` for each.

Most helpers were moved here from ``grass/grow.py``; ``grass/grow.py`` now
imports them from this module.
"""
from __future__ import annotations

import numpy as np

from ..dist import sample
from .config import Uniform, Grouped


# ── Public entry point ────────────────────────────────────────────────────────

def scatter_positions(
    placement,                 # Uniform | Grouped
    n_squares: int,
    footprint_mm: float,
    placement_mask,            # bool ndarray | None
    scene,
    surface,
    rng: np.random.Generator,
) -> list[tuple[float, float, float]]:
    """Return ``(x, y, group_dir)`` tuples for seed placement.

    *placement* is a ``Uniform`` (random) or ``Grouped`` (Voronoi)
    placement strategy.  ``group_dir`` is always ``0.0`` for ``Uniform``
    and when ``Grouped.group_dir_mode == 'none'``.
    """
    if isinstance(placement, Grouped):
        return _voronoi_positions(placement, n_squares, footprint_mm,
                                   placement_mask, scene, surface, rng)
    else:
        return _uniform_positions(placement, n_squares, footprint_mm,
                                   placement_mask, surface, rng)


# ── Uniform random placement (rocks default) ──────────────────────────────────

def _uniform_positions(
    cfg: Uniform,
    n_squares: int,
    footprint_mm: float,
    placement_mask,
    surface,
    rng: np.random.Generator,
) -> list[tuple[float, float, float]]:
    """Sample N positions uniformly within *placement_mask*.

    N = ``cfg.count_per_square * n_squares`` when ``count_per_square`` is set,
    otherwise derived from tile area divided by ``(footprint_mm + gap_mm)²``.
    """
    if cfg.count_per_square is not None:
        n = max(0, int(round(float(sample(cfg.count_per_square, rng)) * n_squares)))
    else:
        gap_mm = float(sample(cfg.gap_mm, rng))
        spacing = max(footprint_mm + max(0.0, gap_mm), 1e-3)
        area_mm2 = surface.tile_w * surface.tile_h
        if placement_mask is not None:
            total_cells = surface.grid_w * surface.grid_h
            area_mm2 *= float(placement_mask.sum()) / max(total_cells, 1)
        n = max(0, int(round(area_mm2 / (spacing * spacing))))

    if n <= 0:
        return []

    cw     = surface.cell_w
    margin = footprint_mm

    if placement_mask is not None:
        allowed = np.argwhere(placement_mask)   # (K, 2): [row, col]
        if len(allowed) == 0:
            return []
        chosen = allowed[rng.integers(0, len(allowed), n)]
        cy = (chosen[:, 0] + rng.uniform(0.0, 1.0, n)) * cw
        cx = (chosen[:, 1] + rng.uniform(0.0, 1.0, n)) * cw
        cx = np.clip(cx, margin, surface.tile_w - margin)
        cy = np.clip(cy, margin, surface.tile_h - margin)
    else:
        span_x = max(surface.tile_w - 2 * margin, 0.0)
        span_y = max(surface.tile_h - 2 * margin, 0.0)
        cx = margin + rng.uniform(0.0, 1.0, n) * span_x
        cy = margin + rng.uniform(0.0, 1.0, n) * span_y

    return list(zip(cx.tolist(), cy.tolist(), [0.0] * n))


# ── Voronoi grouped placement (grass default, optional for rocks) ─────────────

def _voronoi_positions(
    cfg: Grouped,
    n_squares: int,
    footprint_mm: float,
    placement_mask,
    scene,
    surface,
    rng: np.random.Generator,
) -> list[tuple[float, float, float]]:
    """Voronoi-grouped placement with jitter grid.

    Returns ``(x, y, group_dir)`` tuples.
    """
    n_groups = scaled_voronoi_group_count(cfg.groups_per_square, placement_mask, surface, rng)
    groups   = voronoi_groups(n_groups, surface, rng, mask=placement_mask)
    count_per_square = (
        max(0, int(round(sample(cfg.count_per_square, rng))))
        if cfg.count_per_square is not None
        else None
    )

    positions: list[tuple[float, float, float]] = []
    for group in groups:
        if cfg.group_dir_mode == 'random':
            group_dir = float(rng.uniform(0.0, 2.0 * np.pi))
        else:
            group_dir = 0.0

        if count_per_square is not None:
            # Distribute the hard count proportionally across groups.
            group_frac = len(group['rows']) / max(surface.grid_w * surface.grid_h, 1)
            n_group = max(1, int(round(count_per_square * n_squares * group_frac)))
        else:
            n_group = scaled_group_seed_count(
                group, cfg.gap_mm, footprint_mm, surface.cell_w, rng)

        for x, y in jitter_grid_xy(group, n_group, group_dir, surface, rng):
            positions.append((x, y, group_dir))

    return positions


# ── Voronoi helpers (also imported by grass/grow.py) ─────────────────────────

def scaled_voronoi_group_count(
    groups_per_square,
    mask,              # bool ndarray | None — placement mask for this layer
    surface,
    rng: np.random.Generator,
) -> int:
    """Scale Voronoi group count by the fraction of the tile covered by *mask*.

    Keeps group density proportional to the actual placement area so sparse
    regions don't get as many groups as full-tile regions.
    """
    total_tile_cells = surface.grid_w * surface.grid_h
    if total_tile_cells <= 0:
        return 0
    if mask is None:
        masked_cells = total_tile_cells
    else:
        masked_cells = int(mask.sum())
    if masked_cells <= 0:
        return 0

    groups_per_square = max(0, int(round(sample(groups_per_square, rng))))
    full_tile_groups = groups_per_square * surface.cols * surface.rows
    expected_groups  = full_tile_groups * masked_cells / float(total_tile_cells)
    base_count       = int(np.floor(expected_groups))
    if rng.random() < expected_groups - base_count:
        base_count += 1
    return max(1, base_count)


def scaled_group_seed_count(
    group: dict,
    gap_mm,
    footprint_mm: float,
    cell_w: float,
    rng: np.random.Generator,
) -> int:
    """Compute item count for a Voronoi group from gap distance.

    Spacing = ``footprint_mm + gap_mm`` (centre-to-centre, mm).
    Density = 1 / spacing²  (items per mm²).
    ``gap_mm = 0`` → items packed edge-to-edge.
    """
    gap_mm      = float(sample(gap_mm, rng))
    spacing     = max(footprint_mm + max(0.0, gap_mm), 1e-3)
    density     = 1.0 / (spacing * spacing)
    group_area  = len(group["rows"]) * cell_w * cell_w
    scaled      = density * group_area
    base_count  = int(np.floor(scaled))
    if rng.random() < scaled - base_count:
        base_count += 1
    return base_count


def voronoi_groups(
    n: int,
    surface,
    rng: np.random.Generator,
    mask=None,
) -> list[dict]:
    """Partition valid cells into n random Voronoi clump cells.

    *mask* restricts candidate cells; ``None`` means the full tile.
    """
    if n <= 0:
        return []

    valid = (mask if mask is not None
             else np.ones((surface.grid_h, surface.grid_w), dtype=bool))

    rows, cols = np.where(valid)
    if len(rows) == 0:
        return []

    n = min(n, len(rows))
    centers = random_spread_sites(rows, cols, n, rng)
    labels  = nearest_site_labels(rows, cols, centers)

    groups: list[dict] = []
    for label in range(len(centers)):
        member_idx = np.flatnonzero(labels == label)
        if len(member_idx) == 0:
            continue
        groups.append({
            "rows": rows[member_idx],
            "cols": cols[member_idx],
        })
    return groups


def random_spread_sites(
    rows: np.ndarray,
    cols: np.ndarray,
    n: int,
    rng: np.random.Generator,
) -> np.ndarray:
    """Choose n random sites with a mild farthest-point bias for good coverage."""
    first  = int(rng.integers(0, len(rows)))
    chosen = [first]
    min_d2 = (rows - rows[first]) ** 2 + (cols - cols[first]) ** 2

    for _ in range(1, n):
        sample_size = min(len(rows), max(64, n * 8))
        candidates  = rng.choice(len(rows), size=sample_size, replace=False)
        next_idx    = int(candidates[np.argmax(min_d2[candidates])])
        chosen.append(next_idx)
        d2     = (rows - rows[next_idx]) ** 2 + (cols - cols[next_idx]) ** 2
        min_d2 = np.minimum(min_d2, d2)

    return np.column_stack([rows[chosen], cols[chosen]])


def nearest_site_labels(
    rows: np.ndarray,
    cols: np.ndarray,
    centers: np.ndarray,
    chunk_size: int = 8192,
) -> np.ndarray:
    """Assign each cell to its nearest Voronoi centre."""
    labels       = np.empty(len(rows), dtype=np.int32)
    center_rows  = centers[:, 0]
    center_cols  = centers[:, 1]
    for start in range(0, len(rows), chunk_size):
        stop   = min(start + chunk_size, len(rows))
        d2     = (
            (rows[start:stop, None] - center_rows[None, :]) ** 2
            + (cols[start:stop, None] - center_cols[None, :]) ** 2
        )
        labels[start:stop] = np.argmin(d2, axis=1)
    return labels


def jitter_grid_xy(
    group: dict,
    n_target: int,
    group_dir: float,
    surface,
    rng: np.random.Generator,
) -> list[tuple[float, float]]:
    """Jitter-grid candidates with source-edge-biased density.

    The grid is laid out in a rotated (u, v) frame:
      u — along the blade direction (sin θ, cos θ); the source edge (face
          items grow *away from*) sits at minimum u.
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
    n_cells    = len(group_rows)
    if n_cells == 0:
        return []

    cell_w    = surface.cell_w
    # Vectorised membership grid — O(1) NumPy index vs O(n) Python generator.
    in_group = np.zeros((surface.grid_h, surface.grid_w), dtype=bool)
    in_group[group_rows, group_cols] = True

    # Rotated frame aligned with the item direction.
    dx = float(np.sin(group_dir))
    dy = float(np.cos(group_dir))
    px = float(-np.cos(group_dir))   # lateral direction
    py = float(np.sin(group_dir))

    cx = (group_cols + 0.5) * cell_w
    cy = (group_rows + 0.5) * cell_w
    proj_u = cx * dx + cy * dy
    proj_v = cx * px + cy * py

    u_lo  = float(proj_u.min()) - 0.5 * cell_w
    u_hi  = float(proj_u.max()) + 0.5 * cell_w
    v_lo  = float(proj_v.min()) - 0.5 * cell_w
    v_hi  = float(proj_v.max()) + 0.5 * cell_w
    u_span = u_hi - u_lo
    v_span = v_hi - v_lo

    group_area = n_cells * cell_w * cell_w
    spacing    = max(cell_w * 0.5, np.sqrt(group_area / max(n_target, 1)))

    n_u = max(1, int(np.ceil(u_span / spacing)))
    n_v = max(1, int(np.ceil(v_span / spacing)))

    # Both position and count use a (1−t)² power law so density falls off
    # quadratically from the source edge toward the far side.
    t_row        = np.arange(n_u, dtype=float) / n_u
    count_w_raw  = 0.5 * (1.0 - t_row) ** 2 + 0.5
    count_w      = count_w_raw * n_u / count_w_raw.sum()

    # Pre-compute row sizes and batch all RNG calls (2 calls instead of 2×n_u).
    row_nvs = [max(1, int(round(float(n_v * count_w[i])))) for i in range(n_u)]
    total   = sum(row_nvs)
    j_u_all = rng.uniform(0.0, 1.0, total)
    j_v_all = rng.uniform(0.0, 1.0, total)

    all_us: list[np.ndarray] = []
    all_vs: list[np.ndarray] = []
    offset = 0

    for i in range(n_u):
        n_v_i  = row_nvs[i]
        t_lo_i = float((i / n_u) ** 2)
        t_hi_i = float(((i + 1.0) / n_u) ** 2)

        j_u = j_u_all[offset:offset + n_v_i]
        j_v = j_v_all[offset:offset + n_v_i]
        offset += n_v_i

        all_us.append(u_lo + (t_lo_i + j_u * (t_hi_i - t_lo_i)) * u_span)
        all_vs.append(v_lo + (np.arange(n_v_i) + j_v) * (v_span / n_v_i))

    us = np.concatenate(all_us)
    vs = np.concatenate(all_vs)

    xs  = us * dx + vs * px
    ys  = us * dy + vs * py
    ixs = np.clip((xs / cell_w).astype(int), 0, surface.grid_w - 1)
    iys = np.clip((ys / cell_w).astype(int), 0, surface.grid_h - 1)

    valid = in_group[iys, ixs]
    return list(zip(xs[valid].tolist(), ys[valid].tolist()))
