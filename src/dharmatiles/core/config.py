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


@dataclass
class SurfaceConfig:
    """Physical surface dimensions.

    ``grid_w`` and ``grid_h`` are derived from ``cols``/``rows`` and
    ``cells_per_square``; do not set them directly.

    Parameters
    ----------
    cols, rows : int
        Number of 35 mm squares along X and Y.  A 1×1 tile is one
        standard DungeonBlocks square.  A 3×3 tile is nine squares.
    cells_per_square : int
        Heightmap resolution along each axis per square.  Higher values
        give finer mesh geometry.  Must be a power of two; default 256
        (≈ 0.137 mm/cell).  Use 128 for legacy behaviour (≈ 0.273 mm/cell).
    base_h : float
        Depth of the solid slab below the terrain surface (mm).
    seed : int
        Master seed; layers derive their own seeds by XOR-ing with a
        per-layer constant.
    """
    cols:             int   = 1
    rows:             int   = 1
    cells_per_square: int   = 256  # heightmap resolution per 35 mm square
    base_h:         float = 0.0  # mm — extra slab below z=0
    seed:           int   = 377
    flat_terrain:   bool  = True   # False → sinusoidal stand-in terrain (legacy)

    # ── Derived dimensions ────────────────────────────────────────────────────
    @property
    def tile_w(self) -> float:
        """Total tile width in mm."""
        return self.cols * 35.0

    @property
    def tile_h(self) -> float:
        """Total tile height in mm."""
        return self.rows * 35.0

    @property
    def grid_w(self) -> int:
        """Grid columns (cells_per_square × cols)."""
        return self.cols * self.cells_per_square

    @property
    def grid_h(self) -> int:
        """Grid rows (cells_per_square × rows)."""
        return self.rows * self.cells_per_square

    @property
    def cell_w(self) -> float:
        """mm per grid cell in X."""
        return 35.0 / self.cells_per_square

    @property
    def cell_h(self) -> float:
        """mm per grid cell in Y."""
        return 35.0 / self.cells_per_square


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

    # ── Blade width range (mm) ────────────────────────────────────────────────
    width_min:    float = 0.8015625
    width_max:    float = 1.06875

    # ── Spine ─────────────────────────────────────────────────────────────────
    n_path:          int   = 50                       # spine sample count

    # ── Curl ─────────────────────────────────────────────────────────────────
    curl_max:          float = 0.8
    curl_min_fraction: float = 0.65   # every blade curves at least this fraction of max

    # ── Growth (GrassLayer) ───────────────────────────────────────────────────
    seg_len:         float = 0.8    # mm per growth segment
    max_segs:        int   = 12     # max growth segments
    rise_cap:        float = 0.8    # mm: max tolerated rise per step
    smooth_sigma:    float = 2.0    # Gaussian smoothing width (segment units)
    root_depth:      float = 2.0    # mm — underground anchor depth

    # ── Group placement ───────────────────────────────────────────────────────
    # groups_per_square is a density — the layer multiplies by cols × rows
    # to get the actual group count for the surface.
    # More groups with fewer blades each → uniform coverage; fewer groups with
    # more blades each → visible clumping.  At 120 groups the jittered-grid
    # spacing is ~3 mm, small enough that directional flow sweeps fill the gaps.
    groups_per_square: int   = 240
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
    """Parameters for the Z-path solver."""
    clearance:              float = 0.10   # mm — gap above previous blade tops
    max_stack_height:       float = 6.0    # mm — hard pile-height cap above terrain


# ─────────────────────────────────────────────────────────────────────────────
# Soil
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class SoilConfig:
    """Soil texture: two tiers of random super-Gaussian blobs summed into terrain_z.

    Primary tier: large, well-spaced clumps — the main soil mounds.
    Small tier: dense fine-scale bumps adding surface grain on and between mounds.

    Each blob uses a super-Gaussian profile exp(-d^P / (2σ^P)).  P=2 is a
    standard Gaussian; P=4 gives a flatter top and steeper sides — the
    "smooth but small-radius" edge seen on real soil clods.

    All sigma values are in mm; converted to grid cells at runtime so blob
    physical size is independent of cells_per_square resolution.
    """
    # ── Primary clumps ────────────────────────────────────────────────────────
    # Elliptical blobs with random aspect ratio and orientation give organic,
    # rain-eroded shapes rather than perfect circles.
    n_blobs:            int   = 277   # primary clumps per square (35 × 35 mm)
    blob_sigma_min_mm:  float = 0.22  # mm — smallest primary σ (major axis)
    blob_sigma_max_mm:  float = 1.026 # mm — largest  primary σ (major axis)
    blob_sigma_mode_mm: float = 0.434 # mm — triangular distribution peak (None-like: set < min for uniform)
    blob_aspect_min:    float = 0.78  # min minor/major axis ratio (elongated)
    blob_aspect_max:    float = 1.00  # max ratio (circular)
    blob_power:         float = 3.5   # super-Gaussian exponent (2=Gaussian, higher=sharper base)
    blob_cutoff:        float = 2.6   # clip at this × sigma
    blob_h_min:         float = 0.25  # mm — floor for secondary tier
    blob_h_max:         float = 0.30  # mm — ceiling for secondary tier
    blob_h_scale_min:   float = 0.14  # primary tier: height = this × sigma_mm (min)
    blob_h_scale_max:   float = 1.12  # primary tier: height = this × sigma_mm (max)
    blob_h_size_bias:   float = 0.85  # 0=independent, 1=large blobs always at scale_max

    # ── Small-bump / surface-grain tier ──────────────────────────────────────
    n_small:            int   = 0     # small bumps per square
    small_sigma_min_mm: float = 0.20  # mm (≥ 1.5 cells at 256/tile — resolvable)
    small_sigma_max_mm: float = 0.40  # mm
    small_h_min:        float = 0.004  # mm
    small_h_max:        float = 0.010  # mm

    # ── Per-blob organic perturbation ─────────────────────────────────────────
    # blob_warp_str_mm: displaces blob coordinates before computing distance,
    #   making each blob edge irregular/organic rather than a perfect ellipse.
    # blob_texture_amp: multiplies blob height by (1 + noise), adding surface
    #   grain visible within each clump without raising global grid resolution.
    blob_warp_str_mm:          float = 0.0   # mm displacement — disabled (distorts small blobs)
    blob_texture_amp:          float = 0.0   # surface modulation — disabled
    blob_shape_noise_amp:      float = 0.06  # radial irregularity amplitude (0=perfect ellipse)
    blob_shape_noise_harmonics:int   = 4     # number of angular harmonics (2,3,4...)

    # ── Overall surface texture ───────────────────────────────────────────────
    surface_texture_amp:        float = 0.06  # mm — amplitude of base noise layer
    surface_texture_scale_mm:   float = 0.27  # mm — spatial scale of texture features
    surface_texture2_amp:       float = 0.03  # mm — amplitude of finer noise layer
    surface_texture2_scale_mm:  float = 0.12  # mm — spatial scale of finer texture
    blob_jitter:      float = 1.0    # placement jitter: 0=perfect grid, 1=fully random
    blob_cluster_count:  int   = 30   # number of cluster centres (0 = no clustering)
    blob_cluster_spread_mm: float = 6.0  # Gaussian spread around each cluster centre (mm)



    edge_fade_mm: float = 0.8   # mm — cosine fade to zero at tile edges


# ─────────────────────────────────────────────────────────────────────────────
# Stones
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class StonesConfig:
    """Random stone geometry parameters.

    ``stones_per_square`` is a density — StonesLayer multiplies by
    cols × rows to get the actual stone count for the tile.

    Size distribution
    -----------------
    Radius is sampled as  r = r_min + (r_max − r_min) × U^size_power
    where U ~ Uniform(0, 1).  size_power = 1 gives a flat uniform spread;
    higher values skew strongly toward small rocks while still allowing the
    occasional large one up to r_max.
    """
    stones_per_square: int   = 15
    r_min:         float = 1.82   # mm — minimum horizontal semi-axis
    r_max:         float = 2.40   # mm — maximum horizontal semi-axis
    size_power:    float = 2.5    # distribution skew: >1 = more small rocks
    aspect_min:    float = 0.65   # min ry/rx ratio — prevents razor-thin slivers
    flat_min:      float = 0.32   # height = this × mean_radius (flattest)
    flat_max:      float = 1.20   # height = this × mean_radius (roundest)
    n_cuts:        int   = 4      # random plane cuts per stone
    cut_min:       float = 0.40   # min cut distance as fraction of mean radius
    cut_max:       float = 0.75   # max cut distance as fraction of mean radius
    roughness:     float = 0.06   # small residual per-vertex noise
    az_segs:       int   = 12     # azimuth facets per stone
    el_segs:       int   = 6      # elevation rings per stone
    sink:          float = 0.10   # mm — base sunk below terrain


# ─────────────────────────────────────────────────────────────────────────────
# Water ripple
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class WaterRippleConfig:
    """Concentric ripple displacement on the water surface.

    Ripples radiate inward from the water's region boundaries (shoreline and
    stone contacts) only — tile edges are never treated as sources.  Amplitude
    decays exponentially with distance so ripples naturally die out in open
    water long before reaching the tile perimeter.  A secondary harmonic adds
    natural-looking interference.

    Parameters
    ----------
    amplitude_mm        Peak crest height (mm).
    wavelength_mm       Crest-to-crest spacing (mm).
    decay_mm            Exponential e-fold decay distance (mm).
    secondary_amplitude Relative amplitude of the 2nd harmonic.
    secondary_wavelength Wavelength multiplier for the 2nd harmonic (× primary).
    secondary_phase     Phase offset of the 2nd harmonic (radians).
    """
    amplitude_mm:         float = 0.12
    wavelength_mm:        float = 1.8
    decay_mm:             float = 6.0
    secondary_amplitude:  float = 0.35
    secondary_wavelength: float = 0.7    # shorter → tighter secondary rings
    secondary_phase:      float = 0.8    # offset so crests don't perfectly align


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
    soil:    SoilConfig    = field(default_factory=SoilConfig)
    stones:  StonesConfig  = field(default_factory=StonesConfig)
    base:    BaseConfig    = field(default_factory=BaseConfig)
