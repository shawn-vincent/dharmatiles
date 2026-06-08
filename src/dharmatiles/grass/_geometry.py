"""Shared low-level grid and geometry helpers for the grass package.

Single-source implementations of helpers that were previously duplicated
across ``growers/flat.py``, ``mesh.py``, and ``grow.py``.
"""

from __future__ import annotations

import numpy as np


def _spine_distances(spine: np.ndarray) -> np.ndarray:
    """Cumulative physical distance along a blade spine."""
    if len(spine) == 0:
        return np.array([], dtype=float)
    if len(spine) == 1:
        return np.array([0.0], dtype=float)
    segment_lengths = np.linalg.norm(np.diff(spine, axis=0), axis=1)
    return np.concatenate(([0.0], np.cumsum(segment_lengths)))


def _sample_grid(grid: np.ndarray, surface, x: float, y: float) -> float:
    """Bilinear sample of *grid* at world coordinates (scalar result)."""
    i = np.clip(x / surface.cell_w, 0, surface.grid_w - 1)
    j = np.clip(y / surface.cell_w, 0, surface.grid_h - 1)
    i0 = int(np.floor(i)); i1 = min(i0 + 1, surface.grid_w - 1)
    j0 = int(np.floor(j)); j1 = min(j0 + 1, surface.grid_h - 1)
    fi = i - i0; fj = j - j0
    return float(
        grid[j0, i0] * (1 - fi) * (1 - fj)
        + grid[j0, i1] * fi * (1 - fj)
        + grid[j1, i0] * (1 - fi) * fj
        + grid[j1, i1] * fi * fj
    )


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    """Return the (ix, iy) grid-cell indices for world position (x, y)."""
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy


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

    ix0 = max(0, int(min_x / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int(max_x / surface.cell_w) + 1)
    iy0 = max(0, int(min_y / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int(max_y / surface.cell_w) + 1)

    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    left   = cols * surface.cell_w
    right  = (cols + 1) * surface.cell_w
    bottom = rows * surface.cell_w
    top    = (rows + 1) * surface.cell_w

    X0, Y0 = np.meshgrid(left, bottom)
    X1, Y1 = np.meshgrid(right, bottom)
    X2, Y2 = np.meshgrid(right, top)
    X3, Y3 = np.meshgrid(left, top)
    corner_x = np.stack([X0, X1, X2, X3], axis=0)
    corner_y = np.stack([Y0, Y1, Y2, Y3], axis=0)

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
