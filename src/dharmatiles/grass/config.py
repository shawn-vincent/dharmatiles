"""Configuration objects for the grass package."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class SpeciesConfig:
    """Template for one plant species."""

    name: str = "floppy-grass"

    # Blade geometry ranges, sampled at seed creation time.
    width_min: float = 0.75
    width_max: float = 2.0
    step_len: float = 0.8
    length_min: float = 8.0
    length_max: float = 14.4
    curl_max: float = 0.8
    rise_cap: float = 2.0
    root_depth: float = 0.5

    # Flat ribbon mesh.
    cross_section: str = "flat"
    thickness: float = 0.06

    # Placement.
    groups_per_square: int = 50
    group_min: int = 20
    group_max: int = 30
    group_spread_mm: float = 1.5
    dir_jitter: float = 0.14
    curl_jitter: float = 0.064

    # Growth behavior.
    grower: str = "floppy"


@dataclass(frozen=True)
class GrassConfig:
    """Top-level grass config."""

    species: list[SpeciesConfig] = field(default_factory=lambda: [SpeciesConfig()])
    clearance: float = 0.01
    max_stack_height: float = 2.0
    seed: int = 0


def from_legacy_config(cfg, *, seed: int, max_stack_height: float) -> GrassConfig:
    """Translate the old spec-facing config into the new grass package config."""
    step_len = float(getattr(cfg, "seg_len", 0.8))
    max_segs = int(getattr(cfg, "max_segs", 18))
    length = max(step_len, step_len * max_segs)
    curl_max = float(getattr(cfg, "curl_max", 0.8))
    species = SpeciesConfig(
        width_min=float(getattr(cfg, "width_min", 0.75)),
        width_max=float(getattr(cfg, "width_max", 2.0)),
        step_len=step_len,
        length_min=length,
        length_max=length,
        curl_max=curl_max,
        rise_cap=float(getattr(cfg, "rise_cap", 2.0)),
        root_depth=float(getattr(cfg, "root_depth", 0.5)),
        thickness=0.06,
        groups_per_square=int(getattr(cfg, "groups_per_square", 50)),
        group_min=int(getattr(cfg, "group_min", 20)),
        group_max=int(getattr(cfg, "group_max", 30)),
        group_spread_mm=float(getattr(cfg, "group_spread_mm", 1.5)),
        dir_jitter=float(getattr(cfg, "group_dir_jitter", 0.14)),
        curl_jitter=max(curl_max * 0.08, 1e-9),
    )
    return GrassConfig(
        species=[species],
        clearance=0.01,
        max_stack_height=float(max_stack_height),
        seed=int(seed),
    )
