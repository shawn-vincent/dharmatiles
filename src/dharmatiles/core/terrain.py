"""
Semantic terrain types: TerrainType enum, default heights, and transition styles.

TerrainType drives region height defaults and boundary-blend behaviour in
``terrains/tile.py``.  The heightmap itself is computed via IDW in
``_build_spec_terrain``; ``TerrainGrid`` (the old object-per-cell model) has
been removed as it was never wired into the live pipeline.
"""
from __future__ import annotations

from enum import Enum, auto

import numpy as np


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


