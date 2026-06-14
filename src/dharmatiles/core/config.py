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

from ..dist import D, Sample


def _range_compat(name: str, value, old_min, old_max, default):
    """Coerce old ``*_min``/``*_max`` constructor args into one distribution."""
    if value is not None and (old_min is not None or old_max is not None):
        raise TypeError(f"{name}: pass either {name}=... or {name}_min/{name}_max, not both")
    if value is not None:
        return value
    if old_min is None and old_max is None:
        return default
    if old_min is None or old_max is None:
        raise TypeError(f"{name}: legacy min/max arguments must be passed together")
    if old_min == old_max:
        return old_min
    return D[old_min:old_max]


# ─────────────────────────────────────────────────────────────────────────────
# Grass species / runtime grass
# ─────────────────────────────────────────────────────────────────────────────


@dataclass(frozen=True, init=False)
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

    # Growth behaviour.
    grower: str = "floppy"

    def __init__(
        self,
        name: str = "floppy-grass",
        *,
        blade_width: Sample[float] | None = None,
        blade_length: Sample[float] | None = None,
        blade_segment_length: float = 0.5,
        blade_taper: float = 1.0,
        blade_base_width: float = 1.0,
        blade_base_taper: float | None = 0,
        blade_curl: Sample[float] | None = None,
        blade_smooth: float = 0.9,
        blade_rise_cap: float = 2.0,
        blade_clearance: float = 0.1,
        blade_top_facets: int = 6,
        blade_thickness: float = 0.6,
        keel_fraction: float = 0.6,
        min_printable_width: float = 1.2,
        blade_direction_jitter: Sample[float] | None = None,
        group_dir_jitter: float | None = None,
        grower: str = "floppy",
        blade_width_min: float | None = None,
        blade_width_max: float | None = None,
        blade_length_min: float | None = None,
        blade_length_max: float | None = None,
        blade_curl_min: float | None = None,
        blade_curl_max: float | None = None,
    ) -> None:
        blade_width = _range_compat(
            "blade_width", blade_width, blade_width_min, blade_width_max, 1.2
        )
        blade_length = _range_compat(
            "blade_length", blade_length, blade_length_min, blade_length_max, 10
        )
        blade_curl = _range_compat(
            "blade_curl", blade_curl, blade_curl_min, blade_curl_max, D[0.2:0.45]
        )
        if blade_direction_jitter is not None and group_dir_jitter is not None:
            raise TypeError(
                "blade_direction_jitter: pass either blade_direction_jitter=... "
                "or legacy group_dir_jitter=..., not both"
            )
        if blade_direction_jitter is None:
            blade_direction_jitter = (
                D.normal(0.0, group_dir_jitter)
                if group_dir_jitter is not None
                else D.normal(0.0, 0.1)
            )

        values = {
            "name": name,
            "blade_width": blade_width,
            "blade_length": blade_length,
            "blade_segment_length": blade_segment_length,
            "blade_taper": blade_taper,
            "blade_base_width": blade_base_width,
            "blade_base_taper": blade_base_taper,
            "blade_curl": blade_curl,
            "blade_smooth": blade_smooth,
            "blade_rise_cap": blade_rise_cap,
            "blade_clearance": blade_clearance,
            "blade_top_facets": blade_top_facets,
            "blade_thickness": blade_thickness,
            "keel_fraction": keel_fraction,
            "min_printable_width": min_printable_width,
            "blade_direction_jitter": blade_direction_jitter,
            "grower": grower,
        }
        for field_name, value in values.items():
            object.__setattr__(self, field_name, value)


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

@dataclass(init=False)
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

    def __init__(
        self,
        *,
        n_blobs: Sample[int] = 277,
        blob_sigma: Sample[float] | None = None,
        blob_aspect: Sample[float] | None = None,
        blob_power: float = 3.5,
        blob_cutoff: float = 2.6,
        blob_h_scale: Sample[float] | None = None,
        n_small: Sample[int] = 0,
        small_sigma: Sample[float] | None = None,
        small_h: Sample[float] | None = None,
        blob_warp_str_mm: float = 0.0,
        blob_texture_amp: float = 0.0,
        blob_shape_noise_amp: float = 0.06,
        blob_shape_noise_harmonics: int = 4,
        surface_texture_amp: float = 0.06,
        surface_texture_scale_mm: float = 0.27,
        surface_texture2_amp: float = 0.03,
        surface_texture2_scale_mm: float = 0.12,
        blob_jitter: Sample[float] = 1.0,
        blob_cluster_count: Sample[int] = 30,
        blob_cluster_spread_mm: Sample[float] = 6.0,
        edge_fade_mm: float = 1.0,
        blob_sigma_min_mm: float | None = None,
        blob_sigma_max_mm: float | None = None,
        blob_sigma_mode_mm: float | None = None,
        blob_aspect_min: float | None = None,
        blob_aspect_max: float | None = None,
        blob_h_scale_min: float | None = None,
        blob_h_scale_max: float | None = None,
        blob_h_size_bias: float | None = None,
        small_sigma_min_mm: float | None = None,
        small_sigma_max_mm: float | None = None,
        small_h_min: float | None = None,
        small_h_max: float | None = None,
    ) -> None:
        if blob_sigma is not None and any(
            v is not None for v in (blob_sigma_min_mm, blob_sigma_max_mm, blob_sigma_mode_mm)
        ):
            raise TypeError("blob_sigma: pass either blob_sigma=... or legacy sigma args, not both")
        if blob_sigma is None:
            if blob_sigma_min_mm is None and blob_sigma_max_mm is None and blob_sigma_mode_mm is None:
                blob_sigma = D.triangular(0.22, 0.434, 1.026)
            elif blob_sigma_min_mm is None or blob_sigma_max_mm is None:
                raise TypeError("blob_sigma: legacy min/max arguments must be passed together")
            elif blob_sigma_mode_mm is not None and blob_sigma_mode_mm >= blob_sigma_min_mm:
                blob_sigma = D.triangular(blob_sigma_min_mm, blob_sigma_mode_mm, blob_sigma_max_mm)
            else:
                blob_sigma = D[blob_sigma_min_mm:blob_sigma_max_mm]

        blob_aspect = _range_compat(
            "blob_aspect", blob_aspect, blob_aspect_min, blob_aspect_max, D[0.78:1.0]
        )
        blob_h_scale = _range_compat(
            "blob_h_scale", blob_h_scale, blob_h_scale_min, blob_h_scale_max, D[0.14:1.12]
        )
        small_sigma = _range_compat(
            "small_sigma", small_sigma, small_sigma_min_mm, small_sigma_max_mm, D[0.20:0.40]
        )
        small_h = _range_compat(
            "small_h", small_h, small_h_min, small_h_max, D[0.004:0.010]
        )
        if blob_h_size_bias is not None:
            raise TypeError(
                "blob_h_size_bias was removed; express the desired height "
                "spread directly with blob_h_scale=..."
            )

        values = {
            "n_blobs": n_blobs,
            "blob_sigma": blob_sigma,
            "blob_aspect": blob_aspect,
            "blob_power": blob_power,
            "blob_cutoff": blob_cutoff,
            "blob_h_scale": blob_h_scale,
            "n_small": n_small,
            "small_sigma": small_sigma,
            "small_h": small_h,
            "blob_warp_str_mm": blob_warp_str_mm,
            "blob_texture_amp": blob_texture_amp,
            "blob_shape_noise_amp": blob_shape_noise_amp,
            "blob_shape_noise_harmonics": blob_shape_noise_harmonics,
            "surface_texture_amp": surface_texture_amp,
            "surface_texture_scale_mm": surface_texture_scale_mm,
            "surface_texture2_amp": surface_texture2_amp,
            "surface_texture2_scale_mm": surface_texture2_scale_mm,
            "blob_jitter": blob_jitter,
            "blob_cluster_count": blob_cluster_count,
            "blob_cluster_spread_mm": blob_cluster_spread_mm,
            "edge_fade_mm": edge_fade_mm,
        }
        for field_name, value in values.items():
            setattr(self, field_name, value)


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

@dataclass(init=False)
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

    def __init__(
        self,
        *,
        r: Sample[float] | None = None,
        aspect: Sample[float] | None = None,
        flat: Sample[float] | None = None,
        angle: Sample[float] | None = None,
        n_cuts: int = 4,
        cut: Sample[float] | None = None,
        roughness: float = 0.02,
        az_segs: int = 32,
        el_segs: int = 12,
        sink: float = 0.10,
        r_min: float | None = None,
        r_max: float | None = None,
        size_power: float | None = None,
        aspect_min: float | None = None,
        flat_min: float | None = None,
        flat_max: float | None = None,
        cut_min: float | None = None,
        cut_max: float | None = None,
    ) -> None:
        if size_power is not None:
            if r is not None:
                raise TypeError("r: pass either r=... or legacy r_min/r_max/size_power, not both")
            if r_min is None or r_max is None:
                raise TypeError("r: legacy size_power requires r_min and r_max")
            r = D[r_min:r_max].power(size_power)
        else:
            r = _range_compat("r", r, r_min, r_max, D[1.82:2.40].power(2.5))
        aspect = _range_compat(
            "aspect",
            aspect,
            aspect_min,
            1.0 if aspect_min is not None else None,
            D[0.65:1.0],
        )
        flat = _range_compat("flat", flat, flat_min, flat_max, D[0.32:1.20])
        cut = _range_compat("cut", cut, cut_min, cut_max, D[0.40:0.75])
        angle = D[0.0:np.pi] if angle is None else angle

        values = {
            "r": r,
            "aspect": aspect,
            "flat": flat,
            "angle": angle,
            "n_cuts": n_cuts,
            "cut": cut,
            "roughness": roughness,
            "az_segs": az_segs,
            "el_segs": el_segs,
            "sink": sink,
        }
        for field_name, value in values.items():
            setattr(self, field_name, value)




# ─────────────────────────────────────────────────────────────────────────────
# Trees
# ─────────────────────────────────────────────────────────────────────────────

@dataclass(init=False)
class TreeConfig:
    """Deciduous tree: trunk swept cross-section + optional SCA branch crown.

    All ``Sample[float]`` fields accept plain numbers (fixed geometry) or
    ``D[low:high]`` distributions (per-tree variation).  Distributions are
    sampled once per tree by ``trees/trunk.py`` / ``trees/branches.py``.

    Phase 1 parameters control the trunk (always built).
    Phase 2 parameters control space-colonization branches (``grow_branches``).
    """

    # ── Trunk dimensions ──────────────────────────────────────────────────────
    height_mm:         Sample[float] = D[20.0:40.0]   # total trunk height
    r_base_mm:         Sample[float] = D[2.5:4.5]     # base radius at ground
    taper_power:       float         = 0.6             # radius power-law exponent
    aspect:            Sample[float] = D[0.75:1.0]    # cross-section aspect (minor/major)
    twist_per_seg:     float         = 0.04            # frame yaw per segment (radians)

    # ── Spine curvature ───────────────────────────────────────────────────────
    n_seg:             int           = 18              # spine segments (more = smoother)
    lean_mm:           float         = 1.0             # σ of per-step lateral noise (mm)
    lean_max_mm:       float         = 3.5             # hard clamp on cumulative lean (mm)

    # ── Root flare ────────────────────────────────────────────────────────────
    flare_amp:         float         = 0.55            # boost factor at base (fraction of r_base)
    flare_fraction:    float         = 0.18            # fraction of trunk height for flare zone
    flare_power:       float         = 2.5             # sharpness of flare taper

    # ── Bark ridges (axial harmonics) ─────────────────────────────────────────
    ridge_harmonics:   int           = 5               # number of angular harmonics
    ridge_amp:         float         = 0.10            # amplitude (fraction of radius)
    ridge_drift_mm:    float         = 60.0            # Z-period of ridge phase drift (mm)

    # ── Bark wrinkles (horizontal Z-offset) ──────────────────────────────────
    wrinkle_amp:       Sample[float] = D[0.30:0.60]   # mm amplitude
    wrinkle_period:    Sample[float] = D[4.0:8.0]     # mm period

    # ── Branch stubs (optional) ───────────────────────────────────────────────
    n_stubs:           int           = 3               # 0 = no stubs
    stub_min_height_frac: float      = 0.35            # don't stub below this fraction
    stub_length_mm:    Sample[float] = D[2.0:4.5]     # stub length
    stub_r_base_mm:    Sample[float] = D[0.6:1.1]     # stub base radius
    stub_angle_up:     Sample[float] = D[0.15:0.40]   # radians above horizontal (FDM safe)

    # ── Space colonization (Phase 2) ──────────────────────────────────────────
    grow_branches:        bool          = True
    crown_rx:             Sample[float] = D[8.0:13.0]  # crown ellipsoid X radius (mm)
    crown_ry:             Sample[float] = D[8.0:13.0]  # crown ellipsoid Y radius (mm)
    crown_rz:             Sample[float] = D[5.0:9.0]   # crown ellipsoid Z half-height (mm)
    crown_offset_z:       Sample[float] = D[0.0:3.0]   # extra upward shift of crown centre (mm)
    n_attractors:         int           = 120           # attraction points seeded in crown
    sca_segment_mm:       float         = 2.5           # skeleton segment length (mm)
    sca_perception_r:     float         = 9.0           # attractor influence radius (mm)
    sca_kill_r:           float         = 3.0           # attractor kill radius (mm)
    sca_max_steps:        int           = 60            # maximum SCA iterations
    sca_tropism:          float         = 0.3           # upward bias strength (FDM safe)
    # Branching: when XY spread of visible attractors > threshold, a tip splits into two
    sca_branch_xy_std:    float         = 0.35          # XY direction std-dev threshold
    sca_min_branch_att:   int           = 3             # min attractors per cluster to split
    # Spine fraction at which to seed SCA branch roots (0=base, 1=apex)
    sca_trunk_root_frac:  float         = 0.60          # top (1 - frac) of spine used as roots
    branch_r_tip_mm:      float         = 0.5           # leaf-node radius (mm)
    branch_min_r_mm:      float         = 0.35          # skip edges below this radius (mm)
    branch_az_segs:       int           = 8             # azimuth facets per branch segment

    # ── Mesh quality ──────────────────────────────────────────────────────────
    az_segs:           int           = 24              # azimuth facets per trunk ring
    sink:              float         = 0.15            # mm below terrain for watertight base

    def __init__(
        self,
        *,
        height_mm:           Sample[float] | None = None,
        r_base_mm:           Sample[float] | None = None,
        taper_power:         float                = 0.6,
        aspect:              Sample[float] | None = None,
        twist_per_seg:       float                = 0.04,
        n_seg:               int                  = 18,
        lean_mm:             float                = 1.0,
        lean_max_mm:         float                = 3.5,
        flare_amp:           float                = 0.55,
        flare_fraction:      float                = 0.18,
        flare_power:         float                = 2.5,
        ridge_harmonics:     int                  = 5,
        ridge_amp:           float                = 0.10,
        ridge_drift_mm:      float                = 60.0,
        wrinkle_amp:         Sample[float] | None = None,
        wrinkle_period:      Sample[float] | None = None,
        n_stubs:             int                  = 3,
        stub_min_height_frac: float               = 0.35,
        stub_length_mm:      Sample[float] | None = None,
        stub_r_base_mm:      Sample[float] | None = None,
        stub_angle_up:       Sample[float] | None = None,
        grow_branches:       bool                 = True,
        crown_rx:            Sample[float] | None = None,
        crown_ry:            Sample[float] | None = None,
        crown_rz:            Sample[float] | None = None,
        crown_offset_z:      Sample[float] | None = None,
        n_attractors:        int                  = 120,
        sca_segment_mm:      float                = 2.5,
        sca_perception_r:    float                = 9.0,
        sca_kill_r:          float                = 3.0,
        sca_max_steps:       int                  = 60,
        sca_tropism:         float                = 0.3,
        sca_branch_xy_std:   float                = 0.35,
        sca_min_branch_att:  int                  = 3,
        sca_trunk_root_frac: float                = 0.60,
        branch_r_tip_mm:     float                = 0.5,
        branch_min_r_mm:     float                = 0.35,
        branch_az_segs:      int                  = 8,
        az_segs:             int                  = 24,
        sink:                float                = 0.15,
    ) -> None:
        vals = {
            'height_mm':           height_mm      if height_mm      is not None else D[20.0:40.0],
            'r_base_mm':           r_base_mm      if r_base_mm      is not None else D[2.5:4.5],
            'taper_power':         taper_power,
            'aspect':              aspect         if aspect         is not None else D[0.75:1.0],
            'twist_per_seg':       twist_per_seg,
            'n_seg':               n_seg,
            'lean_mm':             lean_mm,
            'lean_max_mm':         lean_max_mm,
            'flare_amp':           flare_amp,
            'flare_fraction':      flare_fraction,
            'flare_power':         flare_power,
            'ridge_harmonics':     ridge_harmonics,
            'ridge_amp':           ridge_amp,
            'ridge_drift_mm':      ridge_drift_mm,
            'wrinkle_amp':         wrinkle_amp    if wrinkle_amp    is not None else D[0.30:0.60],
            'wrinkle_period':      wrinkle_period if wrinkle_period is not None else D[4.0:8.0],
            'n_stubs':             n_stubs,
            'stub_min_height_frac': stub_min_height_frac,
            'stub_length_mm':      stub_length_mm if stub_length_mm is not None else D[2.0:4.5],
            'stub_r_base_mm':      stub_r_base_mm if stub_r_base_mm is not None else D[0.6:1.1],
            'stub_angle_up':       stub_angle_up  if stub_angle_up  is not None else D[0.15:0.40],
            'grow_branches':       grow_branches,
            'crown_rx':            crown_rx       if crown_rx       is not None else D[8.0:13.0],
            'crown_ry':            crown_ry       if crown_ry       is not None else D[8.0:13.0],
            'crown_rz':            crown_rz       if crown_rz       is not None else D[5.0:9.0],
            'crown_offset_z':      crown_offset_z if crown_offset_z is not None else D[0.0:3.0],
            'n_attractors':        n_attractors,
            'sca_segment_mm':      sca_segment_mm,
            'sca_perception_r':    sca_perception_r,
            'sca_kill_r':          sca_kill_r,
            'sca_max_steps':       sca_max_steps,
            'sca_tropism':         sca_tropism,
            'sca_branch_xy_std':   sca_branch_xy_std,
            'sca_min_branch_att':  sca_min_branch_att,
            'sca_trunk_root_frac': sca_trunk_root_frac,
            'branch_r_tip_mm':     branch_r_tip_mm,
            'branch_min_r_mm':     branch_min_r_mm,
            'branch_az_segs':      branch_az_segs,
            'az_segs':             az_segs,
            'sink':                sink,
        }
        for k, v in vals.items():
            setattr(self, k, v)


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
    soil:             SoilConfig    = field(default_factory=SoilConfig)
    rocks:            RocksConfig   = field(default_factory=RocksConfig)
    base:             BaseConfig    = field(default_factory=BaseConfig)
    max_stack_height: float         = 2.0
    # mm — max occ_z above terrain_z a blade may seed or grow into.
    # Must be ≥ the tallest stone in any grass region so blades can clear
    # rocks; keep small to prevent mid-air blade pileups.
