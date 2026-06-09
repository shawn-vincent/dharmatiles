"""
GrassUnderlayLayer: embossed 2D grass-carpet texture stamped into terrain_z.

The layer runs after SoilLayer and before StonesLayer / GrassLayer (3D).
It modifies terrain_z in-place and adds no Trimesh geometry to the scene.

Two components are composited via np.maximum onto a scratch field, then
added to terrain_z (with an optional placement mask):

1.  Noise base — Gaussian-filtered white noise clipped to ≥ 0.  Gives the
    compressed-grass background texture that shows between upright blades.

2.  Blade stamp footprints — each blade seed's top-profile silhouette is
    rasterised step-by-step onto the scratch field.  Seeds are planted with
    the same Voronoi-group logic as the 3D grass layer using the blade
    geometry parameters on GrassUnderlayConfig.

See docs/design/grass-underlay.md for the full design rationale.
"""
from __future__ import annotations

import math

import numpy as np
from scipy.ndimage import distance_transform_edt, gaussian_filter

from ..core.config import GrassUnderlayConfig
from ..core.tile import TileScene
from ..grass.config import GrassConfig as _RuntimeGrassConfig, SpeciesConfig as _SpeciesConfig
from ..grass.grow import plant_seeds as _plant_seeds
from ..grass.seed import GrassSeed


class GrassUnderlayLayer:
    """Emboss a 2D grass-carpet texture into scene.terrain_z."""

    def __init__(self, cfg: GrassUnderlayConfig) -> None:
        self.cfg = cfg

    def build(
        self,
        scene: TileScene,
        placement_mask: np.ndarray | None = None,
    ) -> None:
        """Compute and apply the underlay heightmap bump to terrain_z.

        Must be called after SoilLayer (terrain_z already has soil bumps) and
        before the terrain_support_z sync, StonesLayer, and GrassLayer.
        """
        cfg = self.cfg
        surface = scene.config.surface
        rng = np.random.default_rng(surface.seed ^ 0x554E_4445)  # "UNDE"

        gh, gw = scene.terrain_z.shape
        field = np.zeros((gh, gw), dtype=float)

        # ── 1. Noise base ─────────────────────────────────────────────────────
        if cfg.noise_amp > 0.0:
            sigma = max(1.0, cfg.noise_scale_mm / surface.cell_w)
            noise = rng.standard_normal((gh, gw))
            noise = gaussian_filter(noise, sigma=sigma)
            s = float(noise.std())
            if s > 1e-12:
                noise /= s
            np.maximum(field, noise * cfg.noise_amp, out=field)

        # ── 2. Blade stamp footprints ─────────────────────────────────────────
        seeds = _collect_seeds(scene, surface, cfg, rng)
        for seed in seeds:
            _stamp_blade(field, surface, seed, cfg)

        # ── 3. Edge fade — cosine ramp from 0 at mask boundary to 1 inside ───
        # Keeps the texture from cutting hard at the grass/soil border.
        if cfg.edge_fade_mm > 0.0:
            fade = _compute_edge_fade(placement_mask, gh, gw, cfg.edge_fade_mm, surface.cell_w)
            field *= fade

        # ── 4. Apply with mask ────────────────────────────────────────────────
        if placement_mask is None:
            scene.terrain_z += field
        else:
            scene.terrain_z[placement_mask] += field[placement_mask]


# ── Edge fade ─────────────────────────────────────────────────────────────────

def _compute_edge_fade(
    placement_mask: np.ndarray | None,
    gh: int,
    gw: int,
    edge_fade_mm: float,
    cell_w: float,
) -> np.ndarray:
    """Return a [0..1] weight array that tapers to 0 at every boundary.

    Two distance fields are combined by taking their minimum so that the
    fade goes to zero at whichever boundary is closest — the placement-mask
    edge OR the physical tile edge.  This is necessary because when the
    grass mask extends all the way to a tile edge, EDT only sees the
    interior grass/soil boundary as the nearest False cell and gives those
    tile-edge cells a large distance, leaving the texture non-zero there.

    Tile-edge distance is always included so the texture is guaranteed to
    be zero at the outermost grid cell on all four sides of the tile.
    """
    fade_cells = max(edge_fade_mm / cell_w, 1e-6)

    # Distance (in cells) from each tile edge — zero at columns 0 / gw-1
    # and rows 0 / gh-1, increasing inward.
    ix = np.arange(gw, dtype=float)
    iy = np.arange(gh, dtype=float)
    dx = np.minimum(ix, gw - 1 - ix)                        # (gw,)
    dy = np.minimum(iy, gh - 1 - iy)                        # (gh,)
    tile_edge_dist = np.minimum(dx[np.newaxis, :], dy[:, np.newaxis])  # (gh, gw)

    if placement_mask is not None:
        # EDT gives the distance to the nearest False cell (mask boundary).
        # False cells get 0, so outside-mask cells already resolve to 0.
        mask_dist = distance_transform_edt(placement_mask)
        # Tightest constraint wins: zero wherever either boundary is within range.
        dist = np.minimum(mask_dist, tile_edge_dist)
    else:
        dist = tile_edge_dist

    return 0.5 * (1.0 - np.cos(np.pi * np.clip(dist / fade_cells, 0.0, 1.0)))


# ── Seeding ───────────────────────────────────────────────────────────────────

def _collect_seeds(
    scene: TileScene,
    surface,
    cfg: GrassUnderlayConfig,
    rng: np.random.Generator,
) -> list[GrassSeed]:
    """Plant blade seeds using the same Voronoi-group logic as the 3D layer."""
    # Build a minimal SpeciesConfig from the underlay blade-geometry fields.
    # Fields not relevant to 2D stamping (blade_top_facets, min_printable_width,
    # blade_rise_cap, blade_clearance, grower) are left at their SpeciesConfig
    # defaults — they don't affect seed placement or stamp shape.
    species = _SpeciesConfig(
        blade_width_min=cfg.blade_width_min,
        blade_width_max=cfg.blade_width_max,
        blade_length_min=cfg.blade_length_min,
        blade_length_max=cfg.blade_length_max,
        blade_segment_length=cfg.blade_segment_length,
        blade_taper=cfg.blade_taper,
        blade_base_width=cfg.blade_base_width,
        blade_base_taper=cfg.blade_base_taper,
        blade_curl_min=cfg.blade_curl_min,
        blade_curl_max=cfg.blade_curl_max,
        blade_thickness=cfg.blade_thickness,
        groups_per_square=cfg.groups_per_square,
        gap_mm=cfg.gap_mm,
    )
    grass_cfg = _RuntimeGrassConfig(
        species=[species],
        seed=int(rng.integers(2**31)),
    )

    # Ensure vegetation_support_z is current so planting respects the post-soil
    # terrain height (no prior vegetation stacked above it yet).
    scene.vegetation_support_z = scene.terrain_support_z.copy()

    occ_z = scene.terrain_z.copy()
    plant_rng = np.random.default_rng(int(rng.integers(2**31)))
    paths = _plant_seeds(scene, surface, grass_cfg, occ_z, plant_rng)
    return [p.seed for p in paths]


# ── Stamp ─────────────────────────────────────────────────────────────────────

def _stamp_blade(
    field: np.ndarray,
    surface,
    seed: GrassSeed,
    cfg: GrassUnderlayConfig,
) -> None:
    """Rasterise a blade's 2D top-profile footprint onto field via np.maximum.

    Traces the blade trajectory (same curl / taper as the 3D seed) and stamps
    a sin-arc cross-section at each step.  Each cross-section is confined to
    ±(blade_segment_length/2 + cell_w) in the along-blade direction so adjacent
    steps tile cleanly with no gaps or double-counting.

    Profile: h = peak_h × sin(π × norm_x)  where norm_x ∈ [0, 1] across the
    blade width (0 = left edge, 1 = right edge) — zero at edges, peak at centre.
    """
    total_len = seed.blade_n_steps * seed.blade_segment_length
    stamp_h_max = cfg.blade_thickness * cfg.stamp_height_scale
    cell_w = surface.cell_w
    half_seg = seed.blade_segment_length / 2.0

    x = seed.x
    y = seed.y
    direction = seed.blade_direction

    for step in range(seed.blade_n_steps):
        dist_along = step * seed.blade_segment_length
        taper = seed.distance_taper(dist_along, total_len)

        width = seed.blade_width * taper
        peak_h = stamp_h_max * taper

        # Stop once the blade has tapered away (width < one cell, or effectively flat)
        if width < cell_w or peak_h < 1e-4:
            break

        hw = width / 2.0

        # Mirror the 3D blade containment rule: the blade centre must stay at
        # least hw from every tile edge so no partial (half-stamped) blades
        # appear at the boundary.  Stop the whole blade, not just this step.
        if (x < hw or x > surface.tile_w - hw or
                y < hw or y > surface.tile_h - hw):
            break

        # Blade tangent and perpendicular (in XY; Z is always up)
        # direction convention: tangent = (sin θ, cos θ) — matches grow.py
        tx = math.sin(direction)
        ty = math.cos(direction)
        px =  math.cos(direction)   # perpendicular (90° CCW from tangent)
        py = -math.sin(direction)

        # Bounding box in grid indices
        r_cells = int((hw + half_seg + cell_w) / cell_w) + 1
        ix_c = int(x / cell_w)
        iy_c = int(y / cell_w)
        ix0 = max(0, ix_c - r_cells)
        ix1 = min(surface.grid_w - 1, ix_c + r_cells)
        iy0 = max(0, iy_c - r_cells)
        iy1 = min(surface.grid_h - 1, iy_c + r_cells)

        if ix0 > ix1 or iy0 > iy1:
            break

        # Cell-centre world coordinates
        ix_arr = np.arange(ix0, ix1 + 1)
        iy_arr = np.arange(iy0, iy1 + 1)
        cx_arr = (ix_arr + 0.5) * cell_w   # (nx,)
        cy_arr = (iy_arr + 0.5) * cell_w   # (ny,)

        # Offsets from current step position
        dx = cx_arr[np.newaxis, :] - x    # (1, nx)
        dy = cy_arr[:, np.newaxis] - y    # (ny, 1)

        # Lateral distance (perpendicular to blade)
        lat   = dx * px + dy * py         # (ny, nx)
        along = dx * tx + dy * ty         # (ny, nx) — along-blade component

        lat_frac = lat / hw               # −1 (left edge) → +1 (right edge)

        # Accept cells within blade width AND within this step's along-blade strip
        in_range = (
            (lat_frac >= -1.0) &
            (lat_frac <=  1.0) &
            (np.abs(along) <= half_seg + cell_w)
        )

        if not np.any(in_range):
            # Advance and continue — don't break; the blade may re-enter range
            x += seed.blade_segment_length * tx
            y += seed.blade_segment_length * ty
            direction += seed.blade_curl
            continue

        # sin(π × norm_x) profile: 0 at both edges, peak_h at centre
        norm_x = (lat_frac + 1.0) / 2.0   # 0 … 1 across blade width
        h = np.zeros((iy1 - iy0 + 1, ix1 - ix0 + 1), dtype=float)
        h[in_range] = peak_h * np.sin(np.pi * norm_x[in_range])

        np.maximum(
            field[iy0:iy1 + 1, ix0:ix1 + 1],
            h,
            out=field[iy0:iy1 + 1, ix0:ix1 + 1],
        )

        # Advance along blade trajectory
        x += seed.blade_segment_length * tx
        y += seed.blade_segment_length * ty
        direction += seed.blade_curl   # seed.blade_curl is already per-step (radians/step)

        # Stop if we've walked off the tile
        if x < 0.0 or x >= surface.tile_w or y < 0.0 or y >= surface.tile_h:
            break
