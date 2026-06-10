"""
GrassCarpet: embossed grass-carpet texture under the 3D blades.

Two components are produced:

1.  Noise base — Gaussian-filtered white noise added into ``terrain_z`` with
    an optional placement mask.  Gives the compressed-grass background
    texture that shows between upright blades.

2.  Blade tube meshes — one flat tube mesh per blade seed, built with the
    SAME mesh builder as the 3-D grass blades (``FlatGrassGrower.build_mesh``).
    Each blade's spine xy follows the same curl trajectory used in the 3-D
    layer; spine z is sampled from ``terrain_z`` so the blade conforms to
    the noise base.  ``blade_thickness`` is overridden to
    ``noise_top_mm + blade_raise_mm`` so the ridge stands clear of the noise
    envelope.  Meshes are returned in the parts list and unioned with the
    terrain solid by the orchestrator — giving sub-cell smooth blade edges
    rather than the staircased heightmap rasterisation we used to produce.

Seeds are planted with the same Voronoi-group logic as the 3-D grass layer
using the blade geometry parameters on ``GrassUnderlayConfig``.

See docs/design/grass-underlay.md for the original design rationale.
"""
from __future__ import annotations

import dataclasses

import numpy as np
from scipy.ndimage import distance_transform_edt, gaussian_filter

from ..core.config import (GrassConfig as _RuntimeGrassConfig,
                           GrassUnderlayConfig, SpeciesConfig)
from ..core.tile import TileScene
from ..grass._geometry import _blade_step_geometry, _sample_grid
from ..grass.growers.flat import FlatGrassGrower
from ..grass.grow import plant_seeds as _plant_seeds
from ..grass.seed import GrassPath, GrassSeed


class GrassCarpet:
    """Embossed grass carpet: noise base in terrain_z + flat blade tube meshes.

    Pass ``species=SpeciesConfig(...)`` to share blade geometry with a
    companion 3D ``Grass`` instance.  Pass ``underlay=GrassUnderlayConfig(...)``
    to customise carpet-specific settings (noise amplitude, edge fade, etc.);
    if both are given, ``species`` overrides the species inside ``underlay``.
    """

    height_default_mm: float = 5.0

    def __init__(
        self,
        species: SpeciesConfig | None = None,
        *,
        underlay: GrassUnderlayConfig | None = None,
    ) -> None:
        _species = species or SpeciesConfig()
        if underlay is None:
            self.cfg = GrassUnderlayConfig(species=_species)
        elif species is not None:
            # Caller supplied both: use underlay settings, override its species.
            self.cfg = dataclasses.replace(underlay, species=_species)
        else:
            self.cfg = underlay

    def apply(
        self,
        scene: TileScene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list:
        """Apply noise base to terrain_z and return blade tube meshes.

        Ends with a ``terrain_support_z[:] = terrain_z`` sync so any
        subsequent scatter layer sees the updated noise surface.
        """
        cfg = self.cfg
        surface = scene.surface
        rng = np.random.default_rng(surface.seed ^ 0x554E_4445)  # "UNDE"

        # Reset vegetation_support_z to the bare terrain baseline so that
        # plant_seeds' _vegetation_depth check returns 0 for every cell and
        # no carpet seed is spuriously rejected.
        scene.vegetation_support_z = scene.terrain_support_z.copy()

        gh, gw = scene.terrain_z.shape
        noise_field = np.zeros((gh, gw), dtype=float)

        # ── 1. Noise base ─────────────────────────────────────────────────────
        # Two octaves at half and double ``noise_scale_mm`` are summed 50/50
        # before normalisation, giving fine grain on top of broader undulation.
        # Noise peaks land exactly at noise_top_mm; valleys descend noise_amp
        # below that.  Normalise to [-1, 0] (peak-referenced) then scale+shift:
        #   field = noise_top_mm + noise_amp * unit_noise,  unit_noise ∈ [-1, 0]
        if cfg.noise_amp > 0.0:
            sigma_hi = max(1.0, (cfg.noise_scale_mm * 0.5) / surface.cell_w)
            sigma_lo = max(1.0, (cfg.noise_scale_mm * 2.0) / surface.cell_w)
            noise_hi = gaussian_filter(rng.standard_normal((gh, gw)), sigma=sigma_hi)
            noise_lo = gaussian_filter(rng.standard_normal((gh, gw)), sigma=sigma_lo)
            noise = 0.5 * noise_hi + 0.5 * noise_lo
            noise -= float(noise.max())          # shift so peak = 0
            n_min = float(noise.min())
            if abs(n_min) > 1e-12:
                noise /= abs(n_min)              # now ∈ [-1, 0]
            noise_field += cfg.noise_top_mm + cfg.noise_amp * noise

        # ── 2. Edge fade — cosine ramp from 0 at mask boundary to 1 inside ───
        if cfg.edge_fade_mm > 0.0:
            fade = _compute_edge_fade(placement_mask, gh, gw, cfg.edge_fade_mm, surface.cell_w)
            noise_field *= fade

        # ── 3. Apply noise to terrain_z ───────────────────────────────────────
        if placement_mask is None:
            scene.terrain_z += noise_field
        else:
            scene.terrain_z[placement_mask] += noise_field[placement_mask]

        # Keep terrain_support_z in sync; blade meshes will sit on this surface.
        scene.terrain_support_z[:] = scene.terrain_z

        # ── 4. Build blade tube meshes ────────────────────────────────────────
        seeds = _collect_seeds(scene, surface, cfg, rng, placement_mask=placement_mask)
        parts: list = []
        for seed in seeds:
            mesh = _build_carpet_blade_mesh(scene, surface, seed, cfg, placement_mask)
            if mesh is not None:
                parts.append(mesh)
        return parts


# ── Edge fade ─────────────────────────────────────────────────────────────────

def _compute_edge_fade(
    placement_mask: np.ndarray | None,
    gh: int,
    gw: int,
    edge_fade_mm: float,
    cell_w: float,
) -> np.ndarray:
    """Return a [0..1] weight array that tapers toward 0 at every boundary.

    The fade rises smoothly to 1 over ``edge_fade_mm`` inward.  Mask-boundary
    cells (False) genuinely fade to 0 (so the carpet vanishes cleanly into
    soil).  The TILE edge, however, is shifted +1 cell — the zero crossing
    sits one cell outside the grid, so the outermost real cell carries a
    small but nonzero fade weight instead of exactly zero.  That avoids the
    flat "bare-tile" strip you'd otherwise see at the perimeter where the
    outermost cell row is pinned to z=baseline.
    """
    fade_cells = max(edge_fade_mm / cell_w, 1e-6)

    # Distance (in cells) from each tile edge, plus the +1 cell shift so the
    # outermost real cell sits one fade-step in from the virtual zero point.
    ix = np.arange(gw, dtype=float)
    iy = np.arange(gh, dtype=float)
    dx = np.minimum(ix, gw - 1 - ix)
    dy = np.minimum(iy, gh - 1 - iy)
    tile_edge_dist = np.minimum(dx[np.newaxis, :], dy[:, np.newaxis]) + 1.0

    if placement_mask is not None and not placement_mask.all():
        # EDT gives the distance from each True cell to the nearest False cell;
        # this stays unshifted so the carpet really does fade to 0 at the seam.
        mask_dist = distance_transform_edt(placement_mask)
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
    placement_mask=None,
) -> list[GrassSeed]:
    """Plant blade seeds using the same Voronoi-group logic as the 3D layer."""
    grass_cfg = _RuntimeGrassConfig(species=[cfg.species])
    occ_z = scene.terrain_z.copy()
    plant_rng = np.random.default_rng(int(rng.integers(2**31)))
    paths = _plant_seeds(scene, surface, grass_cfg, occ_z, plant_rng,
                         placement_mask=placement_mask)
    return [p.seed for p in paths]


# ── Blade mesh ────────────────────────────────────────────────────────────────

def _build_carpet_blade_mesh(
    scene, surface, seed: GrassSeed, cfg: GrassUnderlayConfig,
    placement_mask: np.ndarray | None = None,
):
    """Build one flat blade tube mesh.

    The xy spine follows the same curl trajectory as the 3-D grower.  The z
    spine sits on the terrain, but with a quarter-sin arc offset so the base
    sinks one blade-thickness below the surface and rises smoothly back to
    terrain level at the tip:

        z_spine(t) = terrain_z(x, y) + thickness * (sin(π·t/2) − 1)

    The base is rooted in the ground (only the centre ridge kisses the
    surface where the cross-section sin arc adds ``thickness`` above the
    spine); the blade emerges and rises clear of the terrain over its
    length, ending in a tapered tip at the surface.

    ``blade_thickness`` is overridden to ``noise_top_mm + blade_raise_mm`` so
    the ridge clears the noise envelope.  Mesh construction is delegated to
    ``FlatGrassGrower.build_mesh`` so the carpet and the 3-D blades share a
    single mesh builder.

    If ``placement_mask`` is provided, the trajectory walk stops the moment a
    step lands outside the mask — blades planted near the region edge taper
    off before crossing into neighbouring regions (e.g. a water pool).
    """
    species = cfg.species
    stamp_hmax = cfg.noise_top_mm + cfg.blade_raise_mm
    species_for_mesh = dataclasses.replace(species, blade_thickness=stamp_hmax)

    def _inside_mask(xi: float, yi: float) -> bool:
        if placement_mask is None:
            return True
        ix = int(xi / surface.cell_w)
        iy = int(yi / surface.cell_w)
        if not (0 <= ix < surface.grid_w and 0 <= iy < surface.grid_h):
            return False
        return bool(placement_mask[iy, ix])

    # Walk the same xy trajectory as the 3-D grower's step().
    xy_pts: list[tuple[float, float]] = []
    x, y = seed.x, seed.y

    # Ring 0 (seed root).
    taper0 = seed.distance_taper(0.0, seed.blade_n_steps * seed.blade_segment_length)
    hw0 = seed.blade_width * taper0 / 2.0
    if not (hw0 <= x <= surface.tile_w - hw0 and hw0 <= y <= surface.tile_h - hw0):
        return None
    if not _inside_mask(x, y):
        return None
    xy_pts.append((x, y))

    for step in range(seed.blade_n_steps):
        tx, ty, _, _taper0, taper1 = _blade_step_geometry(seed, step, x, y)
        hw = seed.blade_width * taper1 / 2.0
        if not (hw <= tx <= surface.tile_w - hw and hw <= ty <= surface.tile_h - hw):
            break
        if not _inside_mask(tx, ty):
            break
        xy_pts.append((tx, ty))
        x, y = tx, ty

    n = len(xy_pts)
    if n < 2:
        return None

    # Quarter-sin arc: −thickness at t=0 (buried base), 0 at t=1 (tip at surface).
    # Derivative is zero at t=1 so the spine flattens smoothly into the tip.
    t_vals = np.linspace(0.0, 1.0, n)
    z_arc = stamp_hmax * (np.sin(np.pi * t_vals / 2.0) - 1.0)

    points: list[tuple[float, float, float]] = []
    for (xi, yi), za in zip(xy_pts, z_arc):
        terrain_zi = float(_sample_grid(scene.terrain_z, surface, xi, yi))
        points.append((xi, yi, terrain_zi + float(za)))

    path = GrassPath(seed=seed, points=points)
    return FlatGrassGrower.build_mesh(path, species_for_mesh, scene, surface)
