"""
Per-layer configuration dataclasses.

Each layer owns its own config.  No layer reads another layer's config.
Surface dimensions and grid shape live in ``SurfaceConfig``; everything
else is layer-specific.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import numpy as np

from ..dist import D, Sample


# ─────────────────────────────────────────────────────────────────────────────
# Grass species / runtime grass
# ─────────────────────────────────────────────────────────────────────────────


@dataclass
class SpeciesConfig:
    """Template for one plant species.

    Both ``GrassCarpet`` (2D stamps) and ``Grass`` (3D blades) consume a
    ``SpeciesConfig``.  Defining one object and sharing it between the two
    layer instances in a ``.tile.py`` file keeps blade geometry in a single
    place.
    """

    name: str = "floppy-grass"

    # Blade geometry distributions, sampled at seed creation time.
    blade_width: Sample[float] = 1.2
    blade_length: Sample[float] = 10
    blade_segment_length: float = 0.5
    blade_taper: float = 1.0
    blade_base_width: float = 1.0
    blade_base_taper: float | None = 0
    blade_curl: Sample[float] = D[0.2:0.45]  # fraction of π (1.0 = 180° arc)
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

    # Blade direction jitter within a Voronoi group.
    blade_direction_jitter: Sample[float] = D.normal(0.0, 0.1)


@dataclass(frozen=True)
class GrassConfig:
    """Top-level grass config (passed to the 3D grass layer)."""

    species: SpeciesConfig = field(default_factory=SpeciesConfig)
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
    cells_per_square: int   = 128  # heightmap resolution per square (35 mm / 128 = 0.27 mm/cell)
    base_h:         float = 0.0  # mm — extra slab below z=0
    seed:           int   = 377

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
    n_blobs:            Sample[int]   = 277   # primary clumps per square
    blob_sigma:         Sample[float] = D.triangular(0.22, 0.434, 1.026)
    blob_aspect:        Sample[float] = D[0.78:1.0]
    blob_power:         float = 3.5   # super-Gaussian exponent (2=Gaussian, higher=sharper base)
    blob_cutoff:        float = 2.6   # clip at this × sigma
    blob_h_scale:       Sample[float] = D[0.14:1.12]

    # ── Small-bump / surface-grain tier ──────────────────────────────────────
    n_small:            Sample[int]   = 0     # small bumps per square
    small_sigma:        Sample[float] = D[0.20:0.40]
    small_h:            Sample[float] = D[0.004:0.010]

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
    blob_jitter:      Sample[float] = 1.0    # placement jitter: 0=perfect grid, 1=fully random
    blob_cluster_count:  Sample[int] = 30   # number of cluster centres (0 = no clustering)
    blob_cluster_spread_mm: Sample[float] = 6.0  # Gaussian spread around each cluster centre (mm)

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
    instance to both the ``GrassCarpet`` and ``Grass`` in a tile spec so the
    2D stamps and 3D blades share identical *geometry* (width, taper, curl,
    cross-section).  Their *positions* are independently seeded — the carpet
    provides a dense field of flat footprints; the 3D blades stand up through
    it at different locations.
    """

    # ── Noise base ────────────────────────────────────────────────────────────
    # noise_top_mm  — height of noise PEAKS relative to terrain_z.
    # noise_amp     — depth of roughness below noise_top_mm (pure texture depth).
    noise_top_mm:   float = 0.50   # mm — height of noise peaks above terrain_z
    noise_amp:      float = 1.00   # mm — roughness depth below noise_top_mm
    noise_scale_mm: float = 0.2    # mm — Gaussian σ (feature correlation length)

    # ── Stamp rendering ───────────────────────────────────────────────────────
    blade_raise_mm:  float = 0.40  # mm — blade stamps rise this far above noise_top_mm

    # ── Edge fade ─────────────────────────────────────────────────────────────
    # Cosine ramp toward 0 at every boundary, rising to 1 over edge_fade_mm.
    # The tile edge is shifted +1 cell so the outermost real cell sits at a
    # small (≈ 4 %) nonzero fade weight instead of exactly zero — that avoids
    # a flat "bare-tile" strip at the perimeter while still pulling the noise
    # crests way down so the vertical side joins the top cleanly.
    edge_fade_mm: float = 1.0      # mm — 0 disables the fade entirely

    # ── Blade geometry (shared with companion 3D grass layer) ─────────────────
    species: SpeciesConfig = field(default_factory=SpeciesConfig)


# ─────────────────────────────────────────────────────────────────────────────
# Rocks
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class RocksConfig:
    """Rock geometry parameters (shape and size only).

    Placement density is controlled by the ``Uniform`` placement strategy
    on ``Rocks`` -- pass ``placement=Uniform(count_per_square=N)`` there.

    Pass plain numbers for fixed values, or ``D[...]`` distributions for
    sampled values.  For example, ``r=D[0.8:2.2].power(1.5)`` skews toward
    small rocks while still allowing occasional large ones.
    """
    r:             Sample[float] = D[1.82:2.40].power(2.5)
    aspect:        Sample[float] = D[0.65:1.0]
    flat:          Sample[float] = D[0.32:1.20]
    angle:         Sample[float] = D[0.0:np.pi]
    n_cuts:        int   = 4      # random plane cuts per stone
    cut:           Sample[float] = D[0.40:0.75]
    roughness:     float = 0.02   # small residual per-vertex noise
    az_segs:       int   = 32     # azimuth facets per stone
    el_segs:       int   = 12     # elevation rings per stone
    sink:          float = 0.10   # mm — base sunk below terrain


# ─────────────────────────────────────────────────────────────────────────────
# Flowers
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class FlowerConfig:
    """Flower geometry: a thin dome cap on a support column, repeated N times for petals.

    Each petal / centre dome is built in two parts:

    1. **Support column** — a solid oval prism of height ``column_height_mm``
       rising from the terrain.  Petal columns get a 45° conical undercut at
       the base (``petal_undercut_deg``), arcing inward toward the flower
       centre so the outer tip rests on the terrain while the inner edge
       lifts off.

    2. **Dome cap** — a thin half-ellipsoid of height ``dome_thickness_mm``
       (centre dome) or ``petal_thickness_mm`` (petals) sitting on top of the
       column.  "Thickness" is the vertical height of the cap, NOT the wall
       thickness of a hollow shell — the cap is solid.

    ``overlap`` controls how far the petal root extends into the centre dome:
      0.0 → petal base sits at the outer edge of the centre circle
      1.0 → petal base sits at the flower centre (full overlap)
    """
    n_petals:           int   = 5    # number of petals
    center_radius_mm:   float = 1.5  # radius of the centre dome (and its column)
    outer_radius_mm:    float = 2.5  # radial distance from flower centre to petal tip
    column_height_mm:   float = 1.0  # height of support column below each dome cap
    dome_thickness_mm:  float = 1.0  # height of the centre dome cap
    petal_thickness_mm: float = 0.5  # height of each petal dome cap
    overlap:            float = 0.5  # 0 = touch centre edge; 1 = touch centre
    sink:               float = 0.10 # mm below terrain for watertight base seal
    az_segs:            int   = 16   # azimuth facets per dome
    el_segs:            int   = 8    # elevation rings per dome

    @property
    def height_mm(self) -> float:
        """Support-ceiling height used by the grass system (terrain_z + height_mm)."""
        return self.column_height_mm + max(self.dome_thickness_mm, self.petal_thickness_mm)


# ─────────────────────────────────────────────────────────────────────────────
# Base (underside peg / socket)
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class BaseConfig:
    """Tile base (underside socket-peg) parameters.

    ``'db'`` (DungeonBlocks) and ``'ol'`` (OpenLOCK) system exports are generated by
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


