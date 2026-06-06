"""Configuration objects for the grass package."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class SpeciesConfig:
    """Template for one plant species."""

    name: str = "floppy-grass"

    # Blade geometry ranges, sampled at seed creation time.
    blade_width_min: float = 0.75
    blade_width_max: float = 2.0
    blade_length_min: float = 8.0
    blade_length_max: float = 14.4
    blade_segment_length: float = 0.8
    curl_min: float = 0.0
    curl_max: float = 0.8
    rise_cap: float = 2.0

    # Flat ribbon mesh.
    cross_section: str = "flat"
    thickness: float = 0.06

    # Placement.
    groups_per_square: int = 50
    group_min: int = 20
    group_max: int = 30
    group_dir_jitter: float = 0.14

    # Growth behavior.
    grower: str = "floppy"


@dataclass(frozen=True)
class GrassConfig:
    """Top-level grass config."""

    species: list[SpeciesConfig] = field(default_factory=lambda: [SpeciesConfig()])
    clearance: float = 0.01
    max_stack_height: float = 2.0
    seed: int = 0
