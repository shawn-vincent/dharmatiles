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
# Grass species / runtime grass
# ─────────────────────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class SpeciesConfig:
    """Template for one plant species.

    Both the 2D grass-underlay layer and the 3D grass-blade layer consume a
    ``SpeciesConfig``.  Defining one object and sharing it between the two
    ``LayerSpec`` entries in a ``.tile.py`` file keeps blade geometry in a
    single place.
    """

    name: str = "floppy-grass"

    # Blade geometry ranges, sampled at seed creation time.
    blade_width_min: float = 1.2
    blade_width_max: float = 1.2
    blade_length_min: float = 10
    blade_length_max: float = 10
    blade_segment_length: float = 0.5
    blade_taper: float = 1.0
    blade_base_width: float = 1.0
    blade_base_taper: float | None = 0
    blade_curl_min: float = 0.2    # fraction of π (0 = straight, 1.0 = 180° arc)
    blade_curl_max: float = 0.45   # gives 36°–81° arc range by default
    blade_smooth: float = 0.9
    blade_rise_cap: float = 2.0
    blade_clearance: float = 0.1

    # Cross-section shape.
    # blade_top_facets: 1=flat, 2=peaked/leaf, N=round arc.
    # blade_thickness: distance from equator to top profile peak.
    # keel_fraction > 0.5 gives a keel steeper than 45° for any width.
    blade_top_facets: int = 6
    blade_thickness: float = 0.6
    keel_fraction: float = 0.6

    # FDM printability floor — blade body clamped to at least this width.
    # Set to 0.0 to disable.
    min_printable_width: float = 1.2

    # Placement.
    groups_per_square: int = 3
    gap_mm: float = 0.3    # average clear gap between adjacent blade edges (mm)
    group_dir_jitter: float = 0.1

    # Growth behaviour.
    grower: str = "floppy"


@dataclass(frozen=True)
class GrassConfig:
    """Top-level grass config (passed to the 3D grass layer)."""

    species: list[SpeciesConfig] = field(default_factory=lambda: [SpeciesConfig()])
    max_stack_height: float = 2.0
    seed: int = 0


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
        Number of squares along X and Y.
    square_mm : float
        Physical size of one square in mm.  Default 35.0 for DungeonBlocks;
        use 25.4 for canonical OpenLOCK (1-inch imperial) or 25.0 for metric
        OpenLOCK.  Changing this rescales all physical dimensions uniformly
        while keeping per-square density counts and feature sizes (blade widths,
        stone radii) unchanged.
    cells_per_square : int
        Heightmap resolution along each axis per square.  Higher values give
        finer mesh geometry.  Default 256 (≈ square_mm/256 mm/cell).
    base_h : float
        Depth of the solid slab below the terrain surface (mm).
    seed : int
        Master seed; layers derive their own seeds by XOR-ing with a
        per-layer constant.
    """
    cols:             int   = 1
    rows:             int   = 1
    square_mm:      float = 35.0  # mm per square — 35 DB, 25.4 OL imperial, 25.0 OL metric
    cells_per_square: int   = 256  # heightmap resolution per square
    base_h:         float = 0.0  # mm — extra slab below z=0
    seed:           int   = 377
    flat_terrain:   bool  = True   # False → sinusoidal stand-in terrain (legacy)

    # ── Adaptive terrain mesh ─────────────────────────────────────────────────
    # Laplacian threshold for adaptive top-surface triangulation (mm).
    # Interior heightmap vertices are kept only where |∇²z| > this value;
    # a coarse background grid (terrain_simplify_stride cells apart) fills flat
    # areas so triangles don't grow unboundedly large.
    # None = uniform full-resolution grid (legacy behaviour).
    terrain_simplify_threshold: float | None = 0.02
    terrain_simplify_stride:    int          = 16

    # ── Derived dimensions ────────────────────────────────────────────────────
    @property
    def tile_w(self) -> float:
        """Total tile width in mm."""
        return self.cols * self.square_mm

    @property
    def tile_h(self) -> float:
        """Total tile height in mm."""
        return self.rows * self.square_mm

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
        return self.square_mm / self.cells_per_square


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
    blob_h_min:         float = 0.25  # mm — fallback height floor for primary tier if perturb=False (unused in default pipeline)
    blob_h_max:         float = 0.30  # mm — fallback height ceiling for primary tier if perturb=False (unused in default pipeline)
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



    edge_fade_mm: float = 1.0   # mm — cosine fade to zero at tile edges and mask boundary


# ─────────────────────────────────────────────────────────────────────────────
# Grass underlay
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class GrassUnderlayConfig:
    """Embossed 2D grass-carpet texture stamped into terrain_z under 3D blades.

    Two components are composited onto a scratch field, then applied to
    terrain_z (with an optional placement mask):

    1. **Noise base** — smooth Gaussian-filtered white noise that creates a
       low-amplitude bumpy background resembling compressed, matted grass.

    2. **Blade stamp footprints** — the top-profile silhouette of each blade
       rasterised flat onto the ground, using the same Voronoi-group seeding
       logic as the companion 3D grass layer.

    ``species`` holds all blade geometry.  Pass the *same* ``SpeciesConfig``
    instance to both the ``grass_underlay`` and ``grass`` layers in a tile
    spec to guarantee the 2D stamps exactly match the 3D blades.
    """

    # ── Noise base ────────────────────────────────────────────────────────────
    # noise_top_mm  — height of noise PEAKS relative to terrain_z.
    # noise_amp     — depth of roughness below noise_top_mm (pure texture depth).
    noise_top_mm:   float = 0.50   # mm — height of noise peaks above terrain_z
    noise_amp:      float = 1.00   # mm — roughness depth below noise_top_mm
    noise_scale_mm: float = 2.0    # mm — Gaussian σ (feature correlation length)

    # ── Stamp rendering ───────────────────────────────────────────────────────
    blade_raise_mm:  float = 0.40  # mm — blade stamps rise this far above noise_top_mm
    stamp_min_taper: float = 0.40  # skip stamp steps below this taper fraction [0..1]

    # ── Edge fade ─────────────────────────────────────────────────────────────
    edge_fade_mm: float = 1.0      # cosine ramp to 0 at mask boundary; 0 = disabled

    # ── Blade geometry (shared with companion 3D grass layer) ─────────────────
    species: SpeciesConfig = field(default_factory=SpeciesConfig)


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
    roughness:     float = 0.02   # small residual per-vertex noise
    az_segs:       int   = 32     # azimuth facets per stone
    el_segs:       int   = 12     # elevation rings per stone
    sink:          float = 0.10   # mm — base sunk below terrain




# ─────────────────────────────────────────────────────────────────────────────
# Base (underside peg / socket)
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class BaseConfig:
    """Tile base (underside socket-peg) parameters.

    ``'dungeonblock'`` and ``'openlock'`` system exports are generated by
    default. Set ``style = 'none'`` to skip system-base generation entirely.

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
    surface:          SurfaceConfig = field(default_factory=SurfaceConfig)
    flow:             FlowConfig    = field(default_factory=FlowConfig)
    soil:             SoilConfig    = field(default_factory=SoilConfig)
    stones:           StonesConfig  = field(default_factory=StonesConfig)
    base:             BaseConfig    = field(default_factory=BaseConfig)
    max_stack_height: float         = 2.0
    # mm — max occ_z above terrain_z a blade may seed or grow into.
    # Must be ≥ the tallest stone in any grass region so blades can clear
    # rocks; keep small to prevent mid-air blade pileups.
