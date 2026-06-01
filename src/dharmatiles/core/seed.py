"""
GrassSeed: a fully self-contained blade growth specification.

Every parameter the growth algorithm needs is stored in the seed.
The growth function takes a seed and nothing else from external config.

Seeding samples from a GrassConfig and copies values into each seed.
After seeding the config is irrelevant; seeds are the durable unit.
"""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from .config import GrassConfig, SolverConfig


@dataclass
class GrassSeed:
    """All information needed to grow one grass blade.

    Created by :func:`make_seeds`; consumed by GrownGrassLayer.
    Never mutated after creation.
    """
    # ── Position and orientation ──────────────────────────────────────────────
    base_x:    float   # world X of base (mm)
    base_y:    float   # world Y of base (mm)
    direction: float   # initial growth azimuth (radians; 0 = +Y)
    curl:      float   # lateral curvature magnitude × sign

    # ── Blade geometry (all mm) ───────────────────────────────────────────────
    width:    float    # blade half-width × 2
    length:   float    # body arc-length
    tip_len:  float    # tip taper arc-length

    # ── Cross-section ─────────────────────────────────────────────────────────
    cross_section:     str    # 'triangle' | 'circle' | 'diamond'
    circle_segs:       int    # segments for 'circle' (ignored for others)
    thickness:         float  # mm — apex depth for 'triangle' / 'diamond'
    diamond_equator:   float  # equator fraction for 'diamond'
    sub_hull_fraction: float  # sub-hull attachment point fraction

    # ── Lean profile ──────────────────────────────────────────────────────────
    base_lean_angle: float   # radians — near-vertical at base
    lean_angle:      float   # radians — lean at tip
    n_path:          int     # spine sample count

    # ── Solver ────────────────────────────────────────────────────────────────
    clearance:              float   # mm — gap above support surface
    base_sink:              float   # mm — base buried below terrain
    base_obstacle_ignore_t: float   # ignore obstacles over first t of blade

    # ── Growth ────────────────────────────────────────────────────────────────
    seg_len:      float   # mm per growth segment
    max_segs:     int     # maximum segments to grow
    rise_cap:     float   # mm: max tolerated rise per step
    smooth_sigma: float   # Gaussian smoothing width (segments)
    root_depth:   float   # mm below terrain for underground anchor


# ─────────────────────────────────────────────────────────────────────────────
# Factory
# ─────────────────────────────────────────────────────────────────────────────

def make_seed(base_x: float, base_y: float,
              direction: float, curl: float,
              grass: GrassConfig, solver: SolverConfig,
              rng: np.random.Generator) -> GrassSeed:
    """Sample blade geometry from *grass* and return a fully-specified seed.

    ``direction`` and ``curl`` are already resolved by the caller (from the
    flow field + per-blade jitter).  All other variable geometry is sampled
    from the config ranges here.
    """
    width   = float(rng.uniform(grass.width_min,   grass.width_max))
    length  = float(rng.uniform(grass.length_min,  grass.length_max))
    tip_len = float(rng.uniform(grass.tip_len_min, grass.tip_len_max))

    return GrassSeed(
        base_x    = base_x,
        base_y    = base_y,
        direction = direction,
        curl      = curl,
        width     = width,
        length    = length,
        tip_len   = tip_len,
        # cross-section — copied from config
        cross_section     = grass.cross_section,
        circle_segs       = grass.circle_segs,
        thickness         = grass.thickness,
        diamond_equator   = grass.diamond_equator,
        sub_hull_fraction = grass.sub_hull_fraction,
        # lean profile
        base_lean_angle = grass.base_lean_angle,
        lean_angle      = grass.lean_angle,
        n_path          = grass.n_path,
        # solver
        clearance               = solver.clearance,
        base_sink               = solver.base_sink,
        base_obstacle_ignore_t  = solver.base_obstacle_ignore_t,
        # growth
        seg_len      = grass.seg_len,
        max_segs     = grass.max_segs,
        rise_cap     = grass.rise_cap,
        smooth_sigma = grass.smooth_sigma,
        root_depth   = grass.root_depth,
    )
