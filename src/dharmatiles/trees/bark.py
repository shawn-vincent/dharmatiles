"""Procedural bark configuration for Tree meshes."""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class BarkConfig:
    enabled: bool = True
    spacing_mm: float = 2.25
    depth_mm: float = 0.42
    width_mm: float = 0.72
    roughness_amplitude_mm: float = 0.05
    roughness_cell_mm: float = 0.90
    wave_amplitude_mm: float = 0.22
    wave_length_mm: float = 7.5
    twist_rotations: float = 1.25
    phase_jitter: float = 1.0
    min_branch_radius_mm: float = 0.58
    foliage_clearance_mm: float = 0.6
