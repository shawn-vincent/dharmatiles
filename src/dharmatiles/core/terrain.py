"""
Semantic terrain model: TerrainType enum, TerrainGrid, and heightmap derivation.

The grid is the authoritative description of what exists.  Geometry is derived
from it — not sculpted directly.

Pipeline
--------
    author a TerrainGrid
        → terrain_grid_to_heightmap()   →  z_grid (float array)
        → terrain_grid_to_type_map()    →  type_grid (TerrainType array, same shape)
"""
from __future__ import annotations

from enum import Enum, auto
from dataclasses import dataclass, field

import numpy as np

from .config import SurfaceConfig


# ─────────────────────────────────────────────────────────────────────────────
# Terrain types
# ─────────────────────────────────────────────────────────────────────────────

class TerrainType(Enum):
    """Semantic terrain types.

    Each type implies a surface height and transition behaviour.
    Heights are relative to the tile's base_h slab.
    """
    WATER            = auto()   # below-ground surface; shoreline transitions
    GROUND           = auto()   # natural earth; organic XY boundaries
    GRASS            = auto()   # same height as GROUND; plant growth enabled
    CONSTRUCTED_FLOOR= auto()   # flat manmade surface; curved XY boundaries
    WALL             = auto()   # vertical rise from floor or ground
    HIGH_WALL        = auto()   # taller vertical rise
    HIGHEST_WALL     = auto()   # tallest vertical rise
    EMBEDDED_STL     = auto()   # future: object placed from external STL


# Reference surface heights in mm above the tile base (z=0 = bottom of terrain slab).
# These represent total floor-slab thickness for flat features, and total height
# above the slab bottom for walls.  Per-cell height can override them.
_DEFAULT_HEIGHT: dict[TerrainType, float] = {
    TerrainType.WATER:              3.0,   # shallow pool floor
    TerrainType.GROUND:             5.0,   # natural earth floor
    TerrainType.GRASS:              5.0,   # same level as GROUND
    TerrainType.CONSTRUCTED_FLOOR: 10.0,   # raised manmade floor
    TerrainType.WALL:              33.0,   # full-height wall
    TerrainType.HIGH_WALL:         33.0,   # (alias — separate height TBD)
    TerrainType.HIGHEST_WALL:      33.0,   # (alias — separate height TBD)
    TerrainType.EMBEDDED_STL:       5.0,   # same level as GROUND
}

# Transition style between adjacent cell pairs (unordered).
# 'hard' → vertical drop/rise; 'soft' → S-shaped slope.
_SOFT_TRANSITIONS: frozenset = frozenset({
    frozenset({TerrainType.GROUND, TerrainType.WATER}),
    frozenset({TerrainType.GRASS,  TerrainType.WATER}),
})


def transition_style(a: TerrainType, b: TerrainType) -> str:
    """Return 'soft' or 'hard' for the transition between terrain types *a* and *b*."""
    if frozenset({a, b}) in _SOFT_TRANSITIONS:
        return 'soft'
    return 'hard'


def default_height(t: TerrainType) -> float:
    """Return the default surface height (mm) for a terrain type."""
    return _DEFAULT_HEIGHT[t]


# ─────────────────────────────────────────────────────────────────────────────
# Terrain cell
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class TerrainCell:
    """One grid cell: a terrain type and a surface height.

    ``height`` defaults to the type's standard height.  Override it for
    localised variation (e.g. a slightly raised patch of ground).
    """
    terrain_type:   TerrainType
    height:         float = field(default=None)   # type: ignore[assignment]

    def __post_init__(self) -> None:
        if self.height is None:
            self.height = default_height(self.terrain_type)


# ─────────────────────────────────────────────────────────────────────────────
# Terrain grid
# ─────────────────────────────────────────────────────────────────────────────

class TerrainGrid:
    """2D grid of TerrainCell objects sized to a SurfaceConfig.

    Indexed [row, col] → [y, x].  Defaults to all-GRASS at height 0.

    Parameters
    ----------
    surface : SurfaceConfig
        Determines grid dimensions (grid_h rows × grid_w cols).
    default_type : TerrainType
        Fill type for newly created cells.
    """

    def __init__(self, surface: SurfaceConfig,
                 default_type: TerrainType = TerrainType.GRASS) -> None:
        self.surface = surface
        self._cells: list[list[TerrainCell]] = [
            [TerrainCell(default_type) for _ in range(surface.grid_w)]
            for _ in range(surface.grid_h)
        ]

    # ── Access ────────────────────────────────────────────────────────────────

    def __getitem__(self, idx) -> TerrainCell:
        row, col = idx
        return self._cells[row][col]

    def __setitem__(self, idx, cell: TerrainCell) -> None:
        row, col = idx
        self._cells[row][col] = cell

    @property
    def shape(self) -> tuple[int, int]:
        return (self.surface.grid_h, self.surface.grid_w)

    # ── Bulk fill helpers ─────────────────────────────────────────────────────

    def fill(self, terrain_type: TerrainType, height: float | None = None,
             rows: slice = slice(None), cols: slice = slice(None)) -> None:
        """Fill a rectangular region with one terrain type."""
        h = height if height is not None else default_height(terrain_type)
        for row in self._cells[rows]:
            for j, _ in enumerate(row[cols]):
                col_start = cols.start or 0
                row[col_start + j] = TerrainCell(terrain_type, h)

    # ── World-coordinate helpers ──────────────────────────────────────────────

    def world_to_cell(self, x_mm: float, y_mm: float) -> tuple[int, int]:
        """Convert world mm coords to (row, col).  Clamps to grid bounds."""
        col = int(np.clip(x_mm / self.surface.cell_w, 0, self.surface.grid_w - 1))
        row = int(np.clip(y_mm / self.surface.cell_h, 0, self.surface.grid_h - 1))
        return row, col

    def cell_to_world(self, row: int, col: int) -> tuple[float, float]:
        """Return world mm coords of cell (row, col) center."""
        x = (col + 0.5) * self.surface.cell_w
        y = (row + 0.5) * self.surface.cell_h
        return x, y


# ─────────────────────────────────────────────────────────────────────────────
# Grid → heightmap
# ─────────────────────────────────────────────────────────────────────────────

def terrain_grid_to_heightmap(grid: TerrainGrid,
                               smooth_soft: bool = True) -> np.ndarray:
    """Derive a float heightmap from a TerrainGrid.

    Returns
    -------
    z_grid : (grid_h, grid_w) float array
        Surface height in mm at each cell, with soft transition blending
        applied between ground/grass and water cells.

    Algorithm
    ---------
    1. Populate raw heights from each cell's ``height`` field.
    2. Identify soft-transition boundary pairs.
    3. For each soft boundary, blend heights across a 3-cell transition
       zone using an S-curve (smoothstep).  Hard transitions remain as
       sharp steps.
    """
    nrows, ncols = grid.shape

    # Stage 1: raw heights
    z = np.zeros((nrows, ncols), dtype=float)
    types = np.empty((nrows, ncols), dtype=object)
    for r in range(nrows):
        for c in range(ncols):
            cell = grid[r, c]
            z[r, c]     = cell.height
            types[r, c] = cell.terrain_type

    if not smooth_soft:
        return z

    # Stage 2: soft blending at ground↔water edges.
    # For every cell pair (horizontal or vertical) that is a soft transition,
    # we run a 1D smoothstep blend across the boundary into both cells.
    # Blend radius: 3 cells (~0.82 mm), enough for a visible shoreline slope.
    BLEND_RADIUS = 3

    z_blend = z.copy()

    def _blend_pair(r0, c0, r1, c1):
        """Blend z heights at a soft boundary between cells (r0,c0) and (r1,c1)."""
        if transition_style(types[r0, c0], types[r1, c1]) != 'soft':
            return
        za = float(z[r0, c0])
        zb = float(z[r1, c1])
        if abs(za - zb) < 1e-6:
            return

        # Determine axis and direction of blend
        if r0 == r1:       # horizontal boundary (same row)
            axis, sign = 1, 1 if c1 > c0 else -1
            c_hi, c_lo = (c1, c0) if c1 > c0 else (c0, c1)
        else:              # vertical boundary (same col)
            axis, sign = 0, 1 if r1 > r0 else -1
            c_hi, c_lo = (r1, r0) if r1 > r0 else (r0, r1)

        z_low  = min(za, zb)
        z_high = max(za, zb)

        # Paint blend on both sides of the boundary
        for offset in range(1, BLEND_RADIUS + 1):
            t = 1.0 - (offset - 1) / BLEND_RADIUS   # 1 at boundary → 0 at radius
            t_s = t * t * (3.0 - 2.0 * t)           # smoothstep

            # Side toward higher cell
            if axis == 1:
                c_near = c_hi - offset + 1
                c_far  = c_lo + offset - 1
            else:
                c_near = c_hi - offset + 1
                c_far  = c_lo + offset - 1

            # Clamp and apply — only lower cells toward the lower terrain
            if axis == 1:
                rr, cc_n = r0, c_hi - offset + 1
                rr, cc_f = r0, c_lo + offset - 1
            else:
                rr, cc_n = c_hi - offset + 1, c0
                rr, cc_f = c_lo + offset - 1, c0

            if 0 <= (cc_n if axis == 1 else rr) < (ncols if axis == 1 else nrows):
                if axis == 1:
                    idx = (r0, cc_n)
                else:
                    idx = (cc_n, c0)
                z_blend[idx] = z_low + t_s * (z_high - z_low)

    for r in range(nrows):
        for c in range(ncols - 1):
            _blend_pair(r, c, r, c + 1)
    for r in range(nrows - 1):
        for c in range(ncols):
            _blend_pair(r, c, r + 1, c)

    return z_blend


def terrain_grid_to_type_array(grid: TerrainGrid) -> np.ndarray:
    """Return an object array of TerrainType values matching the grid shape."""
    nrows, ncols = grid.shape
    out = np.empty((nrows, ncols), dtype=object)
    for r in range(nrows):
        for c in range(ncols):
            out[r, c] = grid[r, c].terrain_type
    return out
