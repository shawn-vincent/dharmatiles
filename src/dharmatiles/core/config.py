"""
Per-layer configuration dataclasses.

Each layer owns its own config.  No layer reads another layer's config.
The top-level ``SceneConfig`` bundles them for convenience.

Surface dimensions and grid shape live in ``SurfaceConfig``; everything
else is layer-specific.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import numpy as np


# ─────────────────────────────────────────────────────────────────────────────
# Surface / grid
# ─────────────────────────────────────────────────────────────────────────────

CELL_SIZE_MM: float = 35.0 / 128.0   # ≈ 0.273 mm — fixed physical constant


@dataclass
class SurfaceConfig:
    """Physical surface dimensions.

    The grid is always 128 cells per tile unit in each axis.
    ``grid_w`` and ``grid_h`` are derived; do not set them directly.

    Parameters
    ----------
    tile_cols, tile_rows : int
        Number of 35 mm tile units along X and Y.  A 1×1 surface is one
        standard DungeonBlocks tile.  A 2×2 surface is four tiles.
    base_h : float
        Depth of the solid slab below the terrain surface (mm).
    seed : int
        Master seed; layers derive their own seeds by XOR-ing with a
        per-layer constant.
    """
    tile_cols: int   = 1
    tile_rows: int   = 1
    base_h:    float = 0.0      # mm — extra slab below z=0 (terrain heights are
                                 #      total floor thicknesses; 0 = no extra slab)
    seed:      int   = 377

    # ── Derived dimensions ────────────────────────────────────────────────────
    @property
    def tile_w(self) -> float:
        """Total surface width in mm."""
        return self.tile_cols * 35.0

    @property
    def tile_h(self) -> float:
        """Total surface height in mm."""
        return self.tile_rows * 35.0

    @property
    def grid_w(self) -> int:
        """Grid columns (128 × tile_cols)."""
        return self.tile_cols * 128

    @property
    def grid_h(self) -> int:
        """Grid rows (128 × tile_rows)."""
        return self.tile_rows * 128

    @property
    def cell_w(self) -> float:
        """mm per grid cell in X — always CELL_SIZE_MM."""
        return CELL_SIZE_MM

    @property
    def cell_h(self) -> float:
        """mm per grid cell in Y — always CELL_SIZE_MM."""
        return CELL_SIZE_MM


# ─────────────────────────────────────────────────────────────────────────────
# Flow field
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class FlowConfig:
    """Flow vector field parameters.

    The field drives blade lean direction and lateral curl across the surface.

    flow_type : 'linear' | 'swirl' | 'radial' | 'drain' | 'dipole' |
                'random-zones' | 'curl'
    """
    flow_type:       str   = 'random-zones'
    flow_curl_noise: float = 0.0           # 0 = pure base field, 1 = all curl noise
    dir_spread:      float = float(np.radians(5))  # per-blade Gaussian jitter (rad)
    curl_from_curv:  float = 0.80          # 0 = random curl, 1 = curvature-driven


# ─────────────────────────────────────────────────────────────────────────────
# Grass blade geometry
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class GrassConfig:
    """Grass blade appearance and placement parameters.

    All geometry values are in mm.  Ranges are sampled per-seed at
    seeding time and baked into each ``GrassSeed``; they are not used
    during growth.
    """
    # ── Cross-section ─────────────────────────────────────────────────────────
    # 'triangle' — flat ribbon with apex below (printable, fast)
    # 'circle'   — cylindrical tube (reed / rush look)
    # 'diamond'  — 4-vertex rhombus: ridge top, keel bottom
    cross_section:     str   = 'circle'
    circle_segs:       int   = 12      # segments for 'circle' cross-section (≥3)
    thickness:         float = 0.5     # mm — 'triangle' apex depth below spine
    diamond_equator:   float = 0.75    # equator position for 'diamond'
    sub_hull_fraction: float = 0.5     # fraction down triangle sides where sub-hull starts

    # ── Blade geometry ranges (mm) ────────────────────────────────────────────
    width_min:    float = 0.8015625
    width_max:    float = 1.06875
    length_min:   float = 7.425
    length_max:   float = 13.275
    tip_len_min:  float = 2.3625
    tip_len_max:  float = 4.3875

    # ── Lean profile ──────────────────────────────────────────────────────────
    base_lean_angle: float = float(np.radians(8))    # near-vertical at base
    lean_angle:      float = float(np.radians(80))   # nearly horizontal at tip
    n_path:          int   = 50                       # spine sample count

    # ── Curl ─────────────────────────────────────────────────────────────────
    curl_max:          float = 0.8
    curl_min_fraction: float = 0.65   # every blade curves at least this fraction of max

    # ── Growth (GrownGrassLayer) ──────────────────────────────────────────────
    seg_len:         float = 0.8    # mm per growth segment
    max_segs:        int   = 12     # max growth segments
    rise_cap:        float = 0.8    # mm: max tolerated rise per step
    smooth_sigma:    float = 2.0    # Gaussian smoothing width (segment units)
    root_depth:      float = 2.0    # mm — underground anchor depth

    # ── Group placement ───────────────────────────────────────────────────────
    # groups_per_tile is a density — the layer multiplies by tile_cols × tile_rows
    # to get the actual group count for the surface.
    # More groups with fewer blades each → uniform coverage; fewer groups with
    # more blades each → visible clumping.  At 120 groups the jittered-grid
    # spacing is ~3 mm, small enough that directional flow sweeps fill the gaps.
    groups_per_tile: int   = 120
    group_min:       int   = 3
    group_max:       int   = 5
    group_spread_mm: float = 1.5
    group_dir_jitter:float = 0.14   # per-blade direction jitter within group (rad σ)

    # ── Spine sink ────────────────────────────────────────────────────────────
    # Fraction of blade width to sink the spine below the support surface.
    # 0.0 = spine sits on top of support (old behaviour)
    # 0.5 = blade centre at support level; bottom half intersects support
    # 1.0 = top edge of blade flush with support surface
    spine_sink_fraction: float = 0.5

    # ── Support posts ─────────────────────────────────────────────────────────
    max_bridge_mm:   float = 10.0   # max unsupported span before a post is added


# ─────────────────────────────────────────────────────────────────────────────
# Solver / Z-envelope
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class SolverConfig:
    """Parameters for the Z-path solver and collision repair."""
    clearance:              float = 0.10   # mm — gap above previous blade tops
    base_sink:              float = 0.05   # mm — base buried below local terrain
    base_obstacle_ignore_t: float = 0.20   # ignore obstacles over first 20% of blade
    collision_repair_passes:int   = 8      # max per-blade repair attempts
    max_stack_height:       float = 6.0    # mm — hard pile-height cap above terrain
    strict_mode:            bool  = True
    strict_base_t:          float = 0.25   # ignore hits at t ≤ this


# ─────────────────────────────────────────────────────────────────────────────
# Gravel
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class GravelConfig:
    """Random stone geometry parameters.

    ``gravel_per_tile`` is a density — GravelLayer multiplies by
    tile_cols × tile_rows to get the actual stone count for the surface.

    Size distribution
    -----------------
    Radius is sampled as  r = r_min + (r_max − r_min) × U^size_power
    where U ~ Uniform(0, 1).  size_power = 1 gives a flat uniform spread;
    higher values skew strongly toward small rocks while still allowing the
    occasional large one up to r_max.
    """
    gravel_per_tile: int   = 6000
    r_min:         float = 0.05    # mm — minimum horizontal semi-axis
    r_max:         float = 1.5     # mm — maximum horizontal semi-axis
    size_power:    float = 12.0    # distribution skew: >1 = mostly small rocks
    aspect_min:    float = 0.65   # min ry/rx ratio — prevents razor-thin slivers
    flat_min:      float = 0.40    # height = this × mean_radius (flattest)
    flat_max:      float = 1.50    # height = this × mean_radius (roundest)
    n_cuts:        int   = 5       # random plane cuts per stone (0 = smooth dome)
    cut_min:       float = 0.30    # min cut distance as fraction of mean radius
    cut_max:       float = 0.75    # max cut distance as fraction of mean radius
    roughness:     float = 0.06    # small residual per-vertex noise (breaks flat faces slightly)
    az_segs:       int   = 10      # azimuth facets per stone
    el_segs:       int   = 5       # elevation rings per stone
    sink:          float = 0.01    # mm — base sunk below terrain


# ─────────────────────────────────────────────────────────────────────────────
# Base (underside peg / socket)
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class BaseConfig:
    """Tile base (underside socket-peg) parameters.

    Only ``'dungeonblock'`` style is currently supported.
    Set ``style = 'none'`` to skip base generation entirely.

    The socket geometry matches the DungeonBlocks open standard:
    a chamfered column that flares out to the full tile footprint.

    peg_height : float | None
        Override column height in mm.  ``None`` → auto-select:
        tall (11.4 mm) when the max terrain height exceeds
        *auto_threshold_mm*, short (5.7 mm) otherwise.
    """
    style:             str         = 'dungeonblock'
    peg_height:        float|None  = None   # mm — None = auto
    short_peg_height:  float       = 5.7    # mm — short base column
    tall_peg_height:   float       = 11.4   # mm — tall  base column
    auto_threshold_mm: float       = 15.0   # max terrain > this → tall
    # DungeonBlocks geometry constants (keep for DB-compatible output)
    flare_height:      float       = 5.2    # mm — always 5.2 for DB tiles
    col_size:          float       = 26.0   # mm — column cross-section
    col_bevel:         float       = 1.5    # mm — chamfer at peg entry


# ─────────────────────────────────────────────────────────────────────────────
# Scene bundle
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class SceneConfig:
    """All configuration for one terrain scene.

    Layers receive only the sub-config they need; none reads across
    layer boundaries.
    """
    surface: SurfaceConfig = field(default_factory=SurfaceConfig)
    flow:    FlowConfig    = field(default_factory=FlowConfig)
    grass:   GrassConfig   = field(default_factory=GrassConfig)
    solver:  SolverConfig  = field(default_factory=SolverConfig)
    gravel:  GravelConfig  = field(default_factory=GravelConfig)
    base:    BaseConfig    = field(default_factory=BaseConfig)
