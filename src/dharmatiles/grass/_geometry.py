"""Shared low-level grid and geometry helpers for the grass package.

Single-source implementations of helpers that were previously duplicated
across ``growers/flat.py``, ``mesh.py``, and ``grow.py``.
"""

from __future__ import annotations

import numpy as np

# Re-export core sampler so callers inside the grass package have a single
# import point.  core.grid.sample_grid handles both scalar and array inputs
# and is the canonical implementation — no duplicate needed here.
from ..core.grid import sample_grid as _sample_grid


def _blade_step_geometry(
    seed,
    step_idx: int,
    x: float,
    y: float,
) -> tuple[float, float, float, float, float]:
    """Direction, next position, and taper pair for one blade spine step.

    Returns ``(tx, ty, direction, taper0, taper1)`` where *direction* is the
    travel angle (radians) at this step, *(tx, ty)* is the next spine point,
    *taper0* is the width/thickness multiplier at *(x, y)* and *taper1* at
    *(tx, ty)*.

    Canonical implementation shared by the 2-D grass-carpet stamper
    (``layers/grass_carpet._stamp_blade``) and the 3-D flat grower
    (``growers/flat.FlatGrassGrower.step``).
    """
    seg_len   = seed.blade_segment_length
    total_len = seed.blade_n_steps * seg_len
    direction = seed.blade_direction + seed.blade_curl * step_idx
    tx = float(x + seg_len * np.sin(direction))
    ty = float(y + seg_len * np.cos(direction))
    taper0 = seed.distance_taper(step_idx * seg_len, total_len)
    taper1 = seed.distance_taper((step_idx + 1) * seg_len, total_len)
    return tx, ty, direction, taper0, taper1


def _spine_distances(spine: np.ndarray) -> np.ndarray:
    """Cumulative physical distance along a blade spine."""
    if len(spine) == 0:
        return np.array([], dtype=float)
    if len(spine) == 1:
        return np.array([0.0], dtype=float)
    segment_lengths = np.linalg.norm(np.diff(spine, axis=0), axis=1)
    return np.concatenate(([0.0], np.cumsum(segment_lengths)))


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    """Return the (ix, iy) grid-cell indices for world position (x, y)."""
    ix = max(0, min(int(x / surface.cell_w), surface.grid_w - 1))
    iy = max(0, min(int(y / surface.cell_w), surface.grid_h - 1))
    return ix, iy


def _cell_range(
    surface,
    min_x: float,
    max_x: float,
    min_y: float,
    max_y: float,
) -> tuple[int, int, int, int, np.ndarray, np.ndarray]:
    """Cell index range + coordinate arrays for an axis-aligned bounding box.

    Returns ``(ix0, ix1, iy0, iy1, cols, rows)`` where *cols* and *rows* are
    integer index arrays covering the cells that intersect the bbox, padded by
    one cell on each side.

    Shared by ``_contained_segment_cells`` and ``_leading_edge_cells`` (in
    ``grower.py``) — single-source implementation of the identical bbox setup.
    """
    ix0 = max(0, int(min_x / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int(max_x / surface.cell_w) + 1)
    iy0 = max(0, int(min_y / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int(max_y / surface.cell_w) + 1)
    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    return ix0, ix1, iy0, iy1, cols, rows


def _contained_segment_cells(
    surface,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    hw0: float,
    hw1: float,
) -> tuple[int, int, int, int, np.ndarray, np.ndarray, np.ndarray] | None:
    """Return cells whose full square is inside the tapered swept segment.

    The swept region is a quadrilateral with half-widths *hw0* at (x0, y0)
    and *hw1* at (x1, y1).

    Returns
    -------
    ``(ix0, ix1, iy0, iy1, mask, along_norm, lateral_frac)``
    where

    * *mask*         — bool (iy1-iy0+1, ix1-ix0+1): True when the cell's four
                       corners are all inside the swept footprint.
    * *along_norm*   — float array, centre t value along segment [0, 1].
    * *lateral_frac* — float array, normalised lateral offset across the
                       half-width [0, 1] (0 = one edge, 0.5 = centre, 1 = other edge).

    Returns ``None`` when the segment is degenerate or entirely off-tile.
    """
    dx = x1 - x0
    dy = y1 - y0
    segment_length = float(np.hypot(dx, dy))
    if segment_length < 1e-9:
        return None

    ux, uy = dx / segment_length, dy / segment_length
    px, py = -uy, ux

    corners = np.array([
        [x0 + px * hw0, y0 + py * hw0],
        [x1 + px * hw1, y1 + py * hw1],
        [x1 - px * hw1, y1 - py * hw1],
        [x0 - px * hw0, y0 - py * hw0],
    ])
    min_x = max(0.0, float(corners[:, 0].min()))
    max_x = min(surface.tile_w, float(corners[:, 0].max()))
    min_y = max(0.0, float(corners[:, 1].min()))
    max_y = min(surface.tile_h, float(corners[:, 1].max()))
    if min_x >= max_x or min_y >= max_y:
        return None

    ix0, ix1, iy0, iy1, cols, rows = _cell_range(surface, min_x, max_x, min_y, max_y)
    left   = cols * surface.cell_w
    right  = (cols + 1) * surface.cell_w
    bottom = rows * surface.cell_w
    top    = (rows + 1) * surface.cell_w

    # Build corner arrays via broadcasting instead of 4×meshgrid:
    # corner i of cell (row,col): x-coords are [left,right,right,left][i][col]
    #                              y-coords are [bot,bot,top,top][i][row]
    # Shape: (4, n_rows, n_cols) via (4,1,n_cols) × (4,n_rows,1) broadcast.
    corner_x = np.array([left, right, right, left])[:, None, :]   # (4,1,n_cols)
    corner_y = np.array([bottom, bottom, top, top])[:, :, None]   # (4,n_rows,1)

    rel_x = corner_x - x0
    rel_y = corner_y - y0
    corner_along   = rel_x * ux + rel_y * uy
    corner_lateral = rel_x * px + rel_y * py
    corner_t       = np.clip(corner_along / segment_length, 0.0, 1.0)
    corner_hw      = hw0 + (hw1 - hw0) * corner_t
    eps = 1e-9
    mask = (
        (corner_along >= -eps)
        & (corner_along <= segment_length + eps)
        & (np.abs(corner_lateral) <= corner_hw + eps)
    ).all(axis=0)

    center_x       = ((cols + 0.5) * surface.cell_w)[None, :]
    center_y       = ((rows + 0.5) * surface.cell_w)[:, None]
    rel_cx         = center_x - x0
    rel_cy         = center_y - y0
    center_along   = rel_cx * ux + rel_cy * uy
    center_lateral = rel_cx * px + rel_cy * py
    along_norm     = np.clip(center_along / segment_length, 0.0, 1.0)
    center_hw      = hw0 + (hw1 - hw0) * along_norm
    lateral_frac   = np.clip(
        (center_lateral + center_hw) / np.maximum(2.0 * center_hw, 1e-9),
        0.0, 1.0,
    )

    return ix0, ix1, iy0, iy1, mask, along_norm, lateral_frac


def _stamp_segment(
    support: np.ndarray,
    surface,
    x0: float, y0: float,
    x1: float, y1: float,
    width0: float, width1: float,
    z0: float, z1: float,
    thickness0: float, thickness1: float,
    n_top_facets: int,
) -> None:
    """Stamp the swept footprint of one segment into *support* (in-place max).

    Shared implementation used by both the grow-time occ_z stamping
    (``growers/flat.py``) and the mesh-time vegetation support rasterisation
    (``mesh.py``).

    The stamp height at each fully-contained cell is:
      * ``n_top_facets == 1`` (flat): interpolated spine z (thickness ignored).
      * ``n_top_facets >= 2`` (peaked/round): spine z + thickness × sin(π × x_frac),
        where x_frac ∈ [0, 1] across the blade width.

    Both spine z and thickness are interpolated linearly along the segment.
    """
    footprint = _contained_segment_cells(
        surface, x0, y0, x1, y1, width0 / 2.0, width1 / 2.0
    )
    if footprint is None:
        return
    ix0g, ix1g, iy0g, iy1g, mask, along_norm, lateral_frac = footprint
    if not np.any(mask):
        return

    z_spine   = z0 + (z1 - z0) * along_norm
    thickness = thickness0 + (thickness1 - thickness0) * along_norm

    if n_top_facets == 1:
        z_field = z_spine
    else:
        z_field = z_spine + thickness * np.sin(np.pi * lateral_frac)

    block = support[iy0g:iy1g + 1, ix0g:ix1g + 1]
    np.maximum(block, np.where(mask, z_field, block), out=block)
