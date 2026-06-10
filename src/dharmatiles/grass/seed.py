"""Seed and path data models for grass growth."""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import numpy as np


@dataclass(frozen=True)
class GrassSeed:
    x: float
    y: float
    blade_direction: float
    blade_segment_length: float
    blade_n_steps: int
    blade_taper: float
    blade_base_width: float
    blade_base_taper: float
    blade_curl: float
    blade_width: float
    blade_rise_cap: float
    blade_clearance: float
    species_id: str

    # Precomputed for sorting — distance from seed to tile boundary along
    # blade_direction.  Seeds closer to the boundary they face are grown
    # first (upstream) so interior blades ride on top of outer ones.
    # Stored here so sort_key() is self-contained (no surface needed).
    upstream_dist: float = field(default=0.0)

    def sort_key(self) -> tuple:
        """(priority=1, upstream_dist, direction_norm) — grass after rocks.

        Primary:   upstream distance — seeds closest to the tile boundary they
                   face grow first so interior blades ride on top of outer ones.
        Secondary: blade direction normalised to [0, 2π) — tiebreaker that
                   groups same-direction blades together within each upstream band.
        """
        return (1, self.upstream_dist, self.blade_direction % (2.0 * math.pi))

    # ── Scalar API ───────────────────────────────────────────────────────────

    def point_taper(self, point_idx: int, blade_n_steps: int | None = None) -> float:
        """Width/thickness multiplier for a spine point.

        Width is constrained by two independent taper envelopes:
        a tip taper measured backward from the tip, and a base taper measured
        forward from the root.  Overlap is allowed; the narrower envelope wins.
        """
        n_steps = self.blade_n_steps if blade_n_steps is None else max(0, int(blade_n_steps))
        point_len = min(max(point_idx, 0), n_steps) * self.blade_segment_length
        total_len = n_steps * self.blade_segment_length
        return self.distance_taper(point_len, total_len)

    def distance_taper(self, point_len: float, total_len: float) -> float:
        """Width/thickness multiplier at a physical distance along a blade."""
        total_len = max(0.0, float(total_len))
        point_len = min(max(0.0, float(point_len)), total_len)
        return min(self._tip_taper(point_len, total_len), self._base_taper(point_len))

    def _tip_taper(self, point_len: float, total_len: float) -> float:
        taper_len = max(0.0, float(self.blade_taper))
        if taper_len <= 0.0:
            return 1.0

        dist_from_tip = max(0.0, total_len - point_len)
        if dist_from_tip >= taper_len:
            return 1.0

        t = dist_from_tip / taper_len
        return math.sin(t * math.pi / 2.0)

    def _base_taper(self, point_len: float) -> float:
        base_fraction = max(0.0, float(self.blade_base_width))
        if base_fraction >= 1.0:
            return 1.0

        taper_len = max(0.0, float(self.blade_base_taper))
        if taper_len <= 0.0:
            return 1.0

        dist_from_base = max(0.0, float(point_len))
        if dist_from_base >= taper_len:
            return 1.0

        t = dist_from_base / taper_len
        return base_fraction + (1.0 - base_fraction) * math.sin(t * math.pi / 2.0)

    # ── Vectorised API ───────────────────────────────────────────────────────
    #
    # Equivalent to calling distance_taper() over every element of *path_dists*
    # but uses NumPy operations rather than a Python loop over math.*  functions.
    # Drop-in replacement for the list-comprehension pattern:
    #   [seed.distance_taper(d, total_len) for d in path_dists]

    def distance_taper_vec(self, path_dists: np.ndarray,
                           total_len: float) -> np.ndarray:
        """Vectorised width/thickness multiplier over an array of distances."""
        total_len = max(0.0, float(total_len))
        lens = np.clip(np.asarray(path_dists, dtype=float), 0.0, total_len)
        return np.minimum(self._tip_taper_vec(lens, total_len),
                          self._base_taper_vec(lens))

    def _tip_taper_vec(self, lens: np.ndarray, total_len: float) -> np.ndarray:
        taper_len = max(0.0, float(self.blade_taper))
        if taper_len <= 0.0:
            return np.ones(len(lens))
        dist_from_tip = np.maximum(0.0, total_len - lens)
        t = np.clip(dist_from_tip / taper_len, 0.0, 1.0)
        return np.where(dist_from_tip >= taper_len, 1.0, np.sin(t * (np.pi / 2.0)))

    def _base_taper_vec(self, lens: np.ndarray) -> np.ndarray:
        base_fraction = max(0.0, float(self.blade_base_width))
        if base_fraction >= 1.0:
            return np.ones(len(lens))
        taper_len = max(0.0, float(self.blade_base_taper))
        if taper_len <= 0.0:
            return np.ones(len(lens))
        t = np.clip(lens / taper_len, 0.0, 1.0)
        return np.where(lens >= taper_len, 1.0,
                        base_fraction + (1.0 - base_fraction) * np.sin(t * (np.pi / 2.0)))


@dataclass
class GrowingPath:
    seed: GrassSeed
    points: list[tuple[float, float, float]]
    alive: bool = True


@dataclass
class GrassPath:
    seed: GrassSeed
    points: list[tuple[float, float, float]]
