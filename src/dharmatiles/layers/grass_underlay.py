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

from ..core.config import GrassConfig as _RuntimeGrassConfig, GrassUnderlayConfig
from ..core.tile import TileScene
from ..grass._geometry import _stamp_segment
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
        # Noise peaks land exactly at noise_top_mm; valleys descend noise_amp
        # below that.  Normalise to [-1, 0] (peak-referenced) then scale+shift:
        #   field = noise_top_mm + noise_amp * unit_noise,  unit_noise ∈ [-1, 0]
        # Changing noise_top_mm slides the whole envelope; changing noise_amp
        # widens/narrows the roughness — the two are fully independent.
        if cfg.noise_amp > 0.0:
            sigma = max(1.0, cfg.noise_scale_mm / surface.cell_w)
            noise = rng.standard_normal((gh, gw))
            noise = gaussian_filter(noise, sigma=sigma)
            noise -= float(noise.max())          # shift so peak = 0
            n_min = float(noise.min())
            if abs(n_min) > 1e-12:
                noise /= abs(n_min)              # now ∈ [-1, 0]
            field += cfg.noise_top_mm + cfg.noise_amp * noise

        # ── 2. Blade stamp footprints ─────────────────────────────────────────
        # Blade peak = noise_top_mm + blade_raise_mm, always above the noise
        # ceiling (noise_top_mm), so blades win np.maximum cleanly with no
        # noise texture on the blade face.
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
    grass_cfg = _RuntimeGrassConfig(species=[cfg.species])

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
    """Rasterise a blade's swept footprint onto field via _stamp_segment.

    Walks the same spine trajectory as the 3D grower and delegates each
    segment to the shared ``_stamp_segment`` helper (grass._geometry), which
    applies the same sin-arc cross-section profile used by the 3D mesh.

    Stamping into a delta field (z_spine = 0): the resulting height offsets
    are added to terrain_z by the caller, so ``thickness`` here equals the
    desired absolute bump height above terrain.
    """
    total_len  = seed.blade_n_steps * seed.blade_segment_length
    stamp_hmax = cfg.noise_top_mm + cfg.blade_raise_mm

    x, y, direction = seed.x, seed.y, seed.blade_direction

    for step in range(seed.blade_n_steps):
        taper0 = seed.distance_taper(step * seed.blade_segment_length, total_len)
        taper1 = seed.distance_taper((step + 1) * seed.blade_segment_length, total_len)

        tx = x + seed.blade_segment_length * math.sin(direction)
        ty = y + seed.blade_segment_length * math.cos(direction)

        # Mirror the 3D containment rule: blade centre must stay ≥ hw from edges.
        hw = seed.blade_width * taper0 / 2.0
        if x < hw or x > surface.tile_w - hw or y < hw or y > surface.tile_h - hw:
            break

        # Stamp only where taper is above the threshold; always advance position.
        if taper0 >= cfg.stamp_min_taper:
            _stamp_segment(
                field, surface,
                x, y, tx, ty,
                seed.blade_width * taper0, seed.blade_width * taper1,
                0.0, 0.0,                            # z_spine = 0 (delta field)
                stamp_hmax * taper0, stamp_hmax * taper1,
                cfg.species.blade_top_facets,
            )

        x, y = tx, ty
        direction += seed.blade_curl

        if x < 0.0 or x >= surface.tile_w or y < 0.0 or y >= surface.tile_h:
            break
