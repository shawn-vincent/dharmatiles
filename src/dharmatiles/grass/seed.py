"""Seed and path data models for grass growth."""

from __future__ import annotations

import math
from dataclasses import dataclass


@dataclass(frozen=True)
class GrassSeed:
    x: float
    y: float
    blade_direction: float
    blade_segment_length: float
    blade_n_steps: int
    blade_taper: float
    blade_curl: float
    blade_width: float
    blade_rise_cap: float
    blade_clearance: float
    species_id: str

    def point_taper(self, point_idx: int) -> float:
        """Width/thickness multiplier for a spine point.

        ``blade_taper`` is a physical distance from the tip over which the blade
        grows from zero width to full width.  If the blade is shorter than that
        distance, even the base remains narrower than ``blade_width``.
        """
        taper_len = max(0.0, float(self.blade_taper))
        if taper_len <= 0.0:
            return 1.0

        total_len = self.blade_n_steps * self.blade_segment_length
        clamped_idx = min(max(point_idx, 0), self.blade_n_steps)
        point_len = clamped_idx * self.blade_segment_length
        dist_from_tip = max(0.0, total_len - point_len)
        if dist_from_tip >= taper_len:
            return 1.0

        t = dist_from_tip / taper_len
        return math.sin(t * math.pi / 2.0)


@dataclass
class GrowingPath:
    seed: GrassSeed
    points: list[tuple[float, float, float]]
    alive: bool = True
    last_stamp: dict[tuple[int, int], float] | None = None


@dataclass
class GrassPath:
    seed: GrassSeed
    points: list[tuple[float, float, float]]
