"""Seed and path data models for grass growth."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class GrassSeed:
    x: float
    y: float
    blade_direction: float
    blade_segment_length: float
    blade_n_steps: int
    blade_curl: float
    blade_width: float
    blade_rise_cap: float
    blade_clearance: float
    species_id: str


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
