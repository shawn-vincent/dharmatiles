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
    blade_taper: float = 3.0
    blade_base_width: float = 1.0
    blade_base_taper: float | None = None
    blade_curl_min: float = 0.0
    blade_curl_max: float = 0.8
    blade_smooth: float = 0.0
    blade_rise_cap: float = 2.0
    blade_clearance: float = 0.2

    # Cross-section shape.
    # blade_top_facets controls the top profile above the blade equator (spine plane):
    #   1 → flat (blade_thickness ignored; top surface IS the equator)
    #   2 → peaked / leaf (two faces meeting at a centre ridge)
    #   N → round (N faces approximating a half-sine arc)
    # blade_thickness is the distance from the equator to the top profile peak.
    # keel_fraction sets keel_depth = keel_fraction × blade_width.
    # keel_fraction > 0.5 gives a keel angle steeper than 45° for any width.
    blade_top_facets: int = 2
    blade_thickness: float = 0.3
    keel_fraction: float = 0.6

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
    max_stack_height: float = 2.0
    seed: int = 0
