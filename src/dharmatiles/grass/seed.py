"""Seed and path data models for grass growth."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class GrassSeed:
    x: float
    y: float
    direction: float
    blade_segment_length: float
    n_steps: int
    curl: float
    blade_width: float
    rise_cap: float
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
