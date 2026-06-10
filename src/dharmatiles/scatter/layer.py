"""
Scatter: a tile layer that scatters one or more things into a region.

Each "thing" is a ``Rocks`` or ``Grass`` instance with a ``scatter()`` method.
Things are placed in the order given — author writes ``Rocks`` before
``Grass`` to let grass steer around rocks.
"""
from __future__ import annotations

import numpy as np
import trimesh


class Scatter:
    """Scatter a sequence of things into a region in spec order."""

    height_default_mm: float = 5.0

    def __init__(self, *things) -> None:
        self.things = things

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        """Run each thing's ``scatter()`` in spec order and collect parts."""
        parts: list[trimesh.Trimesh] = []
        for layer_idx, thing in enumerate(self.things):
            parts.extend(thing.scatter(
                scene,
                placement_mask = placement_mask,
                layer_idx      = layer_idx,
            ))
        return parts
