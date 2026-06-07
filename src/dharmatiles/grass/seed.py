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
    base_width_percent: float
    base_taper_length: float
    blade_curl: float
    blade_width: float
    blade_rise_cap: float
    blade_clearance: float
    species_id: str

    def point_taper(self, point_idx: int, blade_n_steps: int | None = None) -> float:
        """Width/thickness multiplier for a spine point.

        Width is constrained by two independent taper envelopes:
        a tip taper measured backward from the tip, and a base taper measured
        forward from the root.  Overlap is allowed; the narrower envelope wins.
        """
        n_steps = self.blade_n_steps if blade_n_steps is None else max(0, int(blade_n_steps))
        return min(self._tip_taper(point_idx, n_steps), self._base_taper(point_idx, n_steps))

    def _tip_taper(self, point_idx: int, blade_n_steps: int) -> float:
        taper_len = max(0.0, float(self.blade_taper))
        if taper_len <= 0.0:
            return 1.0

        total_len = blade_n_steps * self.blade_segment_length
        clamped_idx = min(max(point_idx, 0), blade_n_steps)
        point_len = clamped_idx * self.blade_segment_length
        dist_from_tip = max(0.0, total_len - point_len)
        if dist_from_tip >= taper_len:
            return 1.0

        t = dist_from_tip / taper_len
        return math.sin(t * math.pi / 2.0)

    def _base_taper(self, point_idx: int, blade_n_steps: int) -> float:
        base_fraction = max(0.0, float(self.base_width_percent)) / 100.0
        if base_fraction >= 1.0:
            return 1.0

        taper_len = max(0.0, float(self.base_taper_length))
        if taper_len <= 0.0:
            return 1.0

        clamped_idx = min(max(point_idx, 0), blade_n_steps)
        dist_from_base = clamped_idx * self.blade_segment_length
        if dist_from_base >= taper_len:
            return 1.0

        t = dist_from_base / taper_len
        return base_fraction + (1.0 - base_fraction) * math.sin(t * math.pi / 2.0)


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
