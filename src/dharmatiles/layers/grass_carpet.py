"""
GrassCarpetLayer: embossed 2D grass-carpet texture stamped into terrain_z.

The layer runs after SoilCarpetLayer and before any ScatterLayer (rocks +
3D grass).  It modifies terrain_z in-place and adds no Trimesh geometry to
the scene.

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

import dataclasses

import numpy as np
from scipy.ndimage import distance_transform_edt, gaussian_filter

from ..core.config import (GrassConfig as _RuntimeGrassConfig,
                           GrassUnderlayConfig, SpeciesConfig)
from ..core.tile import TileScene
from ..grass._geometry import _blade_step_geometry, _stamp_segment
from ..grass.grow import plant_seeds as _plant_seeds
from ..grass.seed import GrassSeed


_UNDERLAY_FIELDS = {f.name for f in dataclasses.fields(GrassUnderlayConfig)
                    if f.name != 'species'}
_SPECIES_FIELDS  = {f.name for f in dataclasses.fields(SpeciesConfig)}


class GrassCarpetLayer:
    """Emboss a 2D grass-carpet texture into scene.terrain_z.

    Flat kwargs are split between ``GrassUnderlayConfig`` and
    ``SpeciesConfig``; pass ``species=SpeciesConfig(...)`` to share blade
    geometry with a companion 3D ``Grass`` instance.
    """

    height_default_mm: float = 5.0

    def __init__(self, species: SpeciesConfig | None = None, **kwargs) -> None:
        base_species = species or SpeciesConfig()
        species_over = {k: v for k, v in kwargs.items() if k in _SPECIES_FIELDS}
        carpet_over  = {k: v for k, v in kwargs.items() if k in _UNDERLAY_FIELDS}
        unknown = set(kwargs) - _SPECIES_FIELDS - _UNDERLAY_FIELDS
        if unknown:
            raise TypeError(f"GrassCarpetLayer: unknown kwargs {sorted(unknown)!r}")
        final_species = (dataclasses.replace(base_species, **species_over)
                         if species_over else base_species)
        self.cfg = GrassUnderlayConfig(species=final_species, **carpet_over)

    def apply(
        self,
        scene: TileScene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list:
        """Compute and apply the underlay heightmap bump to terrain_z.

        Ends with a ``terrain_support_z[:] = terrain_z`` sync so any
        subsequent scatter layer sees the updated surface.
        """
        cfg = self.cfg
        surface = scene.config.surface
        rng = np.random.default_rng(surface.seed ^ 0x554E_4445)  # "UNDE"

        # Reset vegetation_support_z to the bare terrain baseline so that
        # plant_seeds' _vegetation_depth check returns 0 for every cell and
        # no carpet seed is spuriously rejected.  Nothing has elevated
        # vegetation_support_z yet at this point in the pipeline; this is a
        # defensive guard against future reordering.
        scene.vegetation_support_z = scene.terrain_support_z.copy()

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

        # Keep terrain_support_z in sync with the modified terrain_z.
        scene.terrain_support_z[:] = scene.terrain_z
        return []


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
    stamp_hmax = cfg.noise_top_mm + cfg.blade_raise_mm
    x, y = seed.x, seed.y

    for step in range(seed.blade_n_steps):
        tx, ty, _, taper0, taper1 = _blade_step_geometry(seed, step, x, y)

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

        if x < 0.0 or x >= surface.tile_w or y < 0.0 or y >= surface.tile_h:
            break
