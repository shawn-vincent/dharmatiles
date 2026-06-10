"""ScatterConfig: spatial distribution parameters for one scatter prototype."""
from __future__ import annotations
from dataclasses import dataclass


@dataclass(frozen=True)
class ScatterConfig:
    """How to distribute seed positions spatially across a placement region.

    Controls *where* and *how densely* seeds land.  The seed *geometry*
    (shape, size, blade parameters) is the prototype's responsibility.

    Parameters
    ----------
    items_per_square : int
        Hard seed count per tile square.  When > 0, overrides area-based
        density.  When 0, density is derived from ``gap_mm`` and the
        prototype's reported footprint radius.
    groups_per_square : int
        Number of Voronoi clump centres per tile square.  0 = no grouping:
        positions are sampled uniformly at random (rocks default).
        > 0 = items cluster into Voronoi cells, each with an independent
        direction hint passed to ``Prototype.make_seed`` (grass default).
    gap_mm : float
        Approximate clear gap between adjacent item footprint edges (mm).
        Used for area-based density when ``items_per_square == 0``.
    group_dir_mode : str
        How each Voronoi group's direction is assigned.
        ``'random'`` — uniform in [0, 2π).
        ``'none'``   — always 0.0 (for prototypes that ignore direction).
    seed : int
        Per-prototype RNG seed offset, XOR-ed with the surface seed so
        multiple prototypes on the same tile draw independent sequences.
    """
    items_per_square: int  = 0          # > 0: hard count; 0 = area-based
    groups_per_square: int = 0          # 0: uniform random; > 0: Voronoi
    gap_mm:            float = 2.0
    group_dir_mode:    str   = 'random' # 'random' | 'none'
    seed:              int   = 0
