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

    Created by :func:`make_seed`; consumed by GrassLayer.
    Never mutated after creation.
    """
    # ── Blade geometry ────────────────────────────────────────────────────────
    curl:      float   # lateral curvature magnitude × sign
    width:     float   # blade width (mm)

    # ── Cross-section ─────────────────────────────────────────────────────────
    cross_section:   str    # 'triangle' | 'circle' | 'diamond'
    circle_segs:     int    # segments for 'circle' (ignored for others)
    thickness:       float  # mm — apex depth for 'triangle' / 'diamond'
    diamond_equator: float  # equator fraction for 'diamond'

    # ── Spine ─────────────────────────────────────────────────────────────────
    n_path:      int    # spine sample count

    # ── Solver ────────────────────────────────────────────────────────────────
    clearance:   float  # mm — gap above support surface

    # ── Growth ────────────────────────────────────────────────────────────────
    seg_len:             float   # mm per growth segment
    rise_cap:            float   # mm: max tolerated rise per step
    smooth_sigma:        float   # Gaussian smoothing width (segments)
    root_depth:          float   # mm below terrain for underground anchor
    spine_sink_fraction: float   # fraction of width to sink spine below support


# ─────────────────────────────────────────────────────────────────────────────
# Factory
# ─────────────────────────────────────────────────────────────────────────────

def make_seed(curl: float,
              grass: GrassConfig, solver: SolverConfig,
              rng: np.random.Generator) -> GrassSeed:
    """Sample blade geometry from *grass* and return a fully-specified seed.

    ``curl`` is already resolved by the caller (from the flow field +
    per-blade jitter).  All other variable geometry is sampled here.
    """
    width = float(rng.uniform(grass.width_min, grass.width_max))

    return GrassSeed(
        curl             = curl,
        width            = width,
        cross_section    = grass.cross_section,
        circle_segs      = grass.circle_segs,
        thickness        = grass.thickness,
        diamond_equator  = grass.diamond_equator,
        n_path           = grass.n_path,
        clearance        = solver.clearance,
        seg_len          = grass.seg_len,
        rise_cap         = grass.rise_cap,
        smooth_sigma     = grass.smooth_sigma,
        root_depth       = grass.root_depth,
        spine_sink_fraction = grass.spine_sink_fraction,
    )
