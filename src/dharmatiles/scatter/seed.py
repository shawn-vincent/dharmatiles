"""RockSeed: a fully-resolved rock instance ready for mesh construction.

Seed sort keys use a two-element tuple ``(priority, secondary_key)``:

  priority 0 — rocks  → secondary: −mean_radius (big rocks first)
  priority 1 — grass  → secondary: upstream_dist (GrassSeed, in grass/seed.py)

The Scatter layer realises all priority-0 seeds before priority-1 seeds so that
rock ``terrain_support_z`` stamps are complete when grass seeds are planted.
"""
from __future__ import annotations
from dataclasses import dataclass


@dataclass(frozen=True)
class RockSeed:
    """One fully-resolved rock instance.

    All size and shape parameters are sampled from the ``RocksConfig``
    distribution at seed-creation time.  The mesh builder receives a sorted
    list of ``RockSeed`` objects and realises them in a single vectorised pass.

    Sort key: ``(0, −mean_radius)`` — priority 0 ensures rocks are placed
    before grass (priority 1); descending mean radius puts large rocks first
    so they win the ``terrain_support_z`` maximum in any overlap zone.
    """

    x:      float   # footprint centre X (mm)
    y:      float   # footprint centre Y (mm)
    rx:     float   # horizontal semi-axis along local +X (mm)
    ry:     float   # horizontal semi-axis along local +Y (mm)
    height: float   # distance from base to highest point (mm)
    angle:  float   # yaw rotation around Z (radians)

    def sort_key(self) -> tuple:
        """(priority=0, −mean_radius) — rocks before grass, big first."""
        return (0, -(self.rx + self.ry) * 0.5)
