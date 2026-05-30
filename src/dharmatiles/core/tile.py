"""
TileConfig  — all parameters for one terrain tile (immutable after creation).
TileScene   — mutable state accumulated while building a tile.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import List

import numpy as np
import trimesh


# ── Tile type presets ─────────────────────────────────────────────────────────

class TileType:
    """Named base-height presets matching the game's physical tile system."""
    GROUND   = 6.0   # mm
    WATER    = 3.0
    MANMADE  = 9.5


# ── Config ────────────────────────────────────────────────────────────────────

@dataclass
class TileConfig:
    """All tunable parameters for a single terrain tile.

    Immutable by convention — create a new instance rather than mutating.
    Derived grid spacings (``gx``, ``gy``) are computed properties.
    """

    # ── Tile geometry ──────────────────────────────────────────────────────────
    tile_w: float = 35.0       # mm — tile width  (X axis)
    tile_h: float = 35.0       # mm — tile height (Y axis)
    base_h: float = TileType.GROUND  # mm — solid slab depth below terrain surface
    grid_res: int = 256        # support-field resolution (cells per side)

    # ── Terrain ────────────────────────────────────────────────────────────────
    terrain_amp: float  = 1.0   # mm — sinusoidal bump amplitude
    terrain_freq: float = 1.5   # cycles across tile

    # ── Blade population ───────────────────────────────────────────────────────
    n_blades: int  = 200
    n_fill: int    = 0
    seed: int      = 42
    curl_max: float = 0.6

    # Tall blade geometry (mm)
    tall_w_min: float  = 1.5
    tall_w_max: float  = 2.0
    tall_l_min: float  = 4.0
    tall_l_max: float  = 14.4
    tall_tl_min: float = 1.2
    tall_tl_max: float = 4.8

    # Fill blade geometry (mm)
    fill_w_min: float  = 0.3
    fill_w_max: float  = 0.5
    fill_l_min: float  = 4.0
    fill_l_max: float  = 7.2
    fill_tl_min: float = 1.2
    fill_tl_max: float = 2.4

    # Blade cross-section
    grass_thickness: float         = 0.5   # mm — triangular hull depth below spine
    grass_sub_hull_fraction: float = 0.5   # fraction down triangle sides where sub-hull starts

    # Blade lean profile
    base_lean_angle: float = np.radians(8)    # lean at base (blade erupts near-vertically)
    lean_angle: float      = np.radians(80)   # max lean at tip (nearly horizontal)
    n_path: int            = 50               # spine sample count (more = smoother)

    # ── Flow field ─────────────────────────────────────────────────────────────
    # flow_type: 'linear' | 'swirl' | 'radial' | 'drain' | 'dipole' | 'curl'
    flow_type: str        = 'linear'
    flow_curl_noise: float = 0.30           # 0 = pure base field, 1 = all curl noise
    dir_spread: float     = np.radians(15)  # per-blade Gaussian jitter around flow
    curl_from_curv: float = 0.80            # 0 = random curl, 1 = curvature-driven

    # ── Terrain-following / knot-envelope z-solver ─────────────────────────────
    clearance: float               = 0.10   # mm — gap above previous blade tops
    base_sink: float               = 0.05   # mm — base buried below local terrain
    base_obstacle_ignore_t: float  = 0.20   # ignore obstacles over first 20% of blade
    collision_repair_passes: int   = 8      # max per-blade repair attempts
    max_stack_height: float        = 6.0    # mm — hard pile-height cap above terrain

    # ── Strict intersection checking ───────────────────────────────────────────
    strict_mode: bool    = True
    strict_base_t: float = 0.25   # ignore hits at t ≤ this (blade erupting from terrain)

    # ── Gravel / stones ────────────────────────────────────────────────────────
    n_gravel: int            = 6000
    gravel_r_min: float      = 0.048   # mm — minimum horizontal semi-axis
    gravel_r_max: float      = 0.42    # mm — maximum horizontal semi-axis
    gravel_flat_min: float   = 0.40    # height = this × mean_radius (flattest)
    gravel_flat_max: float   = 1.30    # height = this × mean_radius (roundest)
    gravel_az_segs: int      = 7       # azimuth facets per stone
    gravel_el_segs: int      = 3       # elevation rings per stone
    gravel_sink: float       = 0.01    # mm — base sunk below terrain (looks embedded)

    # ── Derived grid spacing ───────────────────────────────────────────────────
    @property
    def gx(self) -> float:
        """mm per grid cell in X."""
        return self.tile_w / (self.grid_res - 1)

    @property
    def gy(self) -> float:
        """mm per grid cell in Y."""
        return self.tile_h / (self.grid_res - 1)


# ── Grid helpers ──────────────────────────────────────────────────────────────

def make_xy_grids(cfg: TileConfig):
    """Return (x_grid, y_grid) world-coordinate arrays (GRID_RES × GRID_RES)."""
    iy, ix = np.mgrid[0:cfg.grid_res, 0:cfg.grid_res]
    return (ix * cfg.gx).astype(float), (iy * cfg.gy).astype(float)


def make_terrain(cfg: TileConfig) -> np.ndarray:
    """Build the sinusoidal terrain heightmap (GRID_RES × GRID_RES)."""
    x_grid, y_grid = make_xy_grids(cfg)
    u_grid = x_grid / cfg.tile_w
    v_grid = y_grid / cfg.tile_h
    edge_envelope = np.sin(np.pi * u_grid) * np.sin(np.pi * v_grid)
    undulation = (
        np.sin(2 * np.pi * cfg.terrain_freq * u_grid) *
        np.cos(2 * np.pi * cfg.terrain_freq * v_grid)
    )
    return (cfg.terrain_amp * edge_envelope * undulation).astype(float)


# ── Scene ─────────────────────────────────────────────────────────────────────

@dataclass
class TileScene:
    """Mutable state accumulated while building a tile.

    ``terrain_z`` is fixed at construction.
    ``support_z`` grows as layers rasterise their geometry onto it.
    ``parts``     is the list of Trimesh objects to combine at export.
    """
    config: TileConfig
    terrain_z: np.ndarray                      # (GRID_RES, GRID_RES) — read-only
    support_z: np.ndarray                      # (GRID_RES, GRID_RES) — mutable
    parts: List[trimesh.Trimesh] = field(default_factory=list)

    @classmethod
    def from_config(cls, cfg: TileConfig) -> "TileScene":
        """Initialise a fresh scene: terrain generated, support_z = terrain_z."""
        terrain_z = make_terrain(cfg)
        return cls(config=cfg, terrain_z=terrain_z, support_z=terrain_z.copy())
