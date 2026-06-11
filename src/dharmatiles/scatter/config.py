"""Placement strategy dataclasses for the scatter system.

Two strategies:

``Uniform``  — positions sampled uniformly at random within the placement
               mask.  Rocks use this by default.

``Grouped``  — positions clustered into Voronoi groups with a jitter grid
               inside each cell.  Grass uses this by default.

Each strategy carries only the parameters that are meaningful for it:
``Uniform`` has a gap and a count; ``Grouped`` adds group count and
direction mode.  There is no sentinel-value overloading.
"""
from __future__ import annotations
from dataclasses import dataclass


@dataclass(frozen=True)
class Uniform:
    """Uniform random placement across the region.

    Parameters
    ----------
    count_per_square : int | None
        Hard seed count per tile square.  ``None`` (default) → count is
        derived from the item footprint radius and ``gap_mm``.
    gap_mm : float
        Average clear gap between adjacent item footprint edges (mm).
        Used for area-based density when ``count_per_square is None``.
    seed : int
        Per-prototype RNG seed offset, XOR-ed with the surface seed.
    """
    count_per_square: int | None = None
    gap_mm:           float      = 2.0
    seed:             int        = 0


@dataclass(frozen=True)
class Grouped:
    """Voronoi-grouped placement with a jitter grid inside each cell.

    Parameters
    ----------
    groups_per_square : int
        Number of Voronoi clump centres per tile square.
    count_per_square : int | None
        Hard seed count per tile square.  ``None`` → density-derived from
        ``gap_mm`` and the item footprint.
    gap_mm : float
        Average clear gap between adjacent item footprint edges (mm).
    group_dir_mode : str
        Direction assigned to each Voronoi group.
        ``'random'`` — uniform in [0, 2π); ``'none'`` — always 0.0.
    seed : int
        Per-prototype RNG seed offset, XOR-ed with the surface seed.
    """
    groups_per_square: int   = 3
    count_per_square:  int | None = None
    gap_mm:            float = 0.3
    group_dir_mode:    str   = 'random'
    seed:              int   = 0
