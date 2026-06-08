"""Configuration objects for the grass package."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class SpeciesConfig:
    """Template for one plant species."""

    name: str = "floppy-grass"

    # Blade geometry ranges, sampled at seed creation time.
    blade_width_min: float = 1.0
    blade_width_max: float = 1.0
    blade_length_min: float = 15
    blade_length_max: float = 15
    blade_segment_length: float = 0.5
    blade_taper: float = 1.0
    blade_base_width: float = 1.0
    blade_base_taper: float | None = 0
    blade_curl_min: float = 0.5
    blade_curl_max: float = 0.8
    blade_smooth: float = 0.9
    blade_rise_cap: float = 2.0
    blade_clearance: float = 0.1

    # Cross-section shape.
    # blade_top_facets controls the top profile above the blade equator (spine plane):
    #   1 → flat (blade_thickness ignored; top surface IS the equator)
    #   2 → peaked / leaf (two faces meeting at a centre ridge)
    #   N → round (N faces approximating a half-sine arc)
    # blade_thickness is the distance from the equator to the top profile peak.
    # keel_fraction sets keel_depth = keel_fraction × blade_width.
    # keel_fraction > 0.5 gives a keel angle steeper than 45° for any width.
    blade_top_facets: int = 6
    blade_thickness: float = 0.5
    keel_fraction: float = 0.6

    # FDM printability floor.
    # The blade body is clamped to at least this width (mm) so short or thin blades
    # produce walls thick enough for FDM to render without collapsing.  Only the
    # tip-taper zone (last blade_taper mm) is exempt — that portion still tapers
    # freely to a point.  Set to 0.0 to disable.
    min_printable_width: float = 1.2

    # Placement.
    groups_per_square: int = 3
    group_density_min: float = 1.0   # blades / mm²  (lower bound of per-group uniform draw)
    group_density_max: float = 1.0   # blades / mm²  (upper bound)
    group_dir_jitter: float = 0

    # Growth behavior.
    grower: str = "floppy"


@dataclass(frozen=True)
class GrassConfig:
    """Top-level grass config."""

    species: list[SpeciesConfig] = field(default_factory=lambda: [SpeciesConfig()])
    max_stack_height: float = 2.0
    seed: int = 0
