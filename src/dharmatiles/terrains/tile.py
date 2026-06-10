"""
Grass tile generator: soil + stones + grass blades on a DungeonBlocks base.

Usage
─────
    generate-tile-stl
        Batch mode: process every *.tile.py file under src/tiles/ and write
        outputs to stl/dungeonblocks/ and stl/openlock/ with names like
        1x1-soil+grass-db.stl / 1x1-soil+grass-ol.stl.
        Sub-directories under src/tiles/ are mirrored in the output trees.

    generate-tile-stl --spec "src/tiles/soil+grass.tile.py"
        Single tile: same naming and directory conventions as batch.

    generate-tile-stl --spec src/tiles/foo.tile.py -o stl/custom.stl
        Single tile, explicit output path.
"""
from __future__ import annotations

import argparse
import dataclasses
import pathlib
import time as _time

import numpy as np
import trimesh
from scipy.ndimage import binary_dilation, binary_erosion, distance_transform_edt

from ..core.config import (SceneConfig, SurfaceConfig,
                           SoilConfig, RocksConfig, BaseConfig,
                           GrassUnderlayConfig, SpeciesConfig, GrassConfig as RuntimeGrassConfig)
from ..core.tile import TileScene
from ..core.mesh import make_heightmap_solid
from ..core.spec import TileSpec, load_spec
from ..core.region import build_region_mask, build_grass_mask
from ..bases import dungeonblocks, openlock
from ..layers.soil import SoilCarpetLayer
from ..layers.grass_carpet import GrassCarpetLayer
from ..layers.water import make_water_displacement, make_water_ripple_displacement, make_water_volume, WATER_RENDER_LIFT_MM
from ..scatter.config import ScatterConfig
from ..scatter.prototype import RockPrototype, GrassPrototype
from ..scatter.layer import ScatterLayer



# ── Shared layer pipeline ─────────────────────────────────────────────────────

def _build_mesh(cfg: SceneConfig,
                scene: TileScene,
                scatter_pairs: list[tuple[object, np.ndarray | None]] | None = None,
                soil_carpet_layers: list[tuple[SoilConfig, np.ndarray | None]] | None = None,
                grass_carpet_layers: list[tuple[GrassUnderlayConfig, np.ndarray | None]] | None = None,
                verbose: bool = True,
                water_mask: np.ndarray | None = None,
                water_height: float | None = None,
                water_embed_mm: float = 2.0) -> trimesh.Trimesh:
    """Run all layers on *scene* and return the concatenated mesh.

    *scatter_pairs* is a list of ``(prototype, placement_mask)`` pairs processed
    by ``ScatterLayer`` in priority order (rocks first, grass second).  Each
    prototype is a ``RockPrototype`` or ``GrassPrototype`` instance.  Defaults to
    one ``RockPrototype`` + one ``GrassPrototype`` over the whole tile.

    *grass_carpet_layers* is a list of ``(GrassUnderlayConfig, placement_mask)``
    pairs — one per ``grass_carpet`` layer in the spec.  Each modifies
    terrain_z in its region (embossed 2D texture) before stones and 3D grass.

    *water_mask* / *water_height* — when provided, a flat water-surface mesh
    is placed at *water_height* over the water region.  The pool floor must
    already be zeroed in *scene.terrain_z* before this call (done by
    ``build_tile_from_spec`` before scene construction).
    """
    if scatter_pairs is None:
        scatter_pairs = [
            (RockPrototype(cfg.rocks), None),
            (GrassPrototype(SpeciesConfig()), None),
        ]
    if soil_carpet_layers is None:
        soil_carpet_layers = []
    if grass_carpet_layers is None:
        grass_carpet_layers = []

    parts: list[trimesh.Trimesh] = []

    # ── Soil carpet (terrain_z texture, bare-ground regions) ──────────────────
    for soil_cfg, soil_mask in soil_carpet_layers:
        if verbose:
            print("Building soil texture...")
        SoilCarpetLayer(cfg.surface, soil_cfg).build(scene, placement_mask=soil_mask)

    # ── Grass carpet (terrain_z texture, grass regions) ───────────────────────
    for carpet_cfg, carpet_mask in grass_carpet_layers:
        if verbose:
            print("Building grass carpet...")
        GrassCarpetLayer(carpet_cfg).build(scene, placement_mask=carpet_mask)

    # Sync terrain_support_z to include soil + underlay baked into terrain_z.
    scene.terrain_support_z[:] = scene.terrain_z

    # ── Scatter (rocks → grass, in priority order) ────────────────────────────
    scatter_layer = ScatterLayer(scatter_pairs)
    scatter_parts = scatter_layer.build(scene, verbose=verbose,
                                        max_stack_height=cfg.max_stack_height)
    parts.extend(scatter_parts)

    # ── Water volume ──────────────────────────────────────────────────────────
    if water_mask is not None and water_height is not None:
        if verbose:
            print("Building water volume...")
        # Dilate the pool mask to the far side of the boundary strip so the
        # displacement and ripple textures extend through the full shore zone.
        embed_cells_full = max(1, round(water_embed_mm / cfg.surface.cell_w))
        wm_disp_full = binary_dilation(water_mask, iterations=embed_cells_full)

        z_disp = make_water_displacement(wm_disp_full, cfg.surface)
        # Downsample to 128 cells/square (~0.27 mm/cell for DB scale).
        s = max(1, cfg.surface.cells_per_square // 128)
        if s > 1:
            gh, gw = scene.terrain_z.shape
            hn, wn = gh // s, gw // s
            tz = scene.terrain_z[:hn*s, :wn*s].reshape(hn, s, wn, s).mean(axis=(1, 3))
            wm = water_mask[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
            wm_disp = wm_disp_full[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
            zd = z_disp[:hn*s, :wn*s].reshape(hn, s, wn, s).mean(axis=(1, 3))
            sm = (scene.rock_mask[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
                  if scene.rock_mask is not None else None)
            ds_cell_w = cfg.surface.cell_w * s
        else:
            tz, wm, zd = scene.terrain_z, water_mask, z_disp
            wm_disp = wm_disp_full
            sm = scene.rock_mask
            ds_cell_w = cfg.surface.cell_w
        zd = zd + make_water_ripple_displacement(
            wm_disp, sm, ds_cell_w, seed=cfg.surface.seed ^ 0xC4F7)
        # Outside the pool, raise the water surface to follow terrain_z so the
        # top of the full-tile slab merges with the shore slope — no flat shelf
        # at the waterline where the pool mask ends.
        h_base = water_height + WATER_RENDER_LIFT_MM
        zd = np.where(wm, zd, np.maximum(tz - h_base, zd))

        # Full-tile water slab: the terrain solid sculpts it naturally through
        # the union — no perimeter walls or embed dilation needed.
        hn, wn = wm.shape
        wm_full = np.ones((hn, wn), dtype=bool)
        water_mesh = make_water_volume(
            tz, wm_full, water_height,
            cfg.surface.tile_w, cfg.surface.tile_h,
            z_disp=zd)
        parts.append(water_mesh)

    # ── Terrain solid ─────────────────────────────────────────────────────────
    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, cfg.surface.tile_w, cfg.surface.tile_h, cfg.surface.base_h,
        error_threshold=cfg.surface.terrain_simplify_threshold,
        simplify_stride=cfg.surface.terrain_simplify_stride,
    )
    parts.insert(0, terrain_mesh)

    solid_parts = [p for p in parts if p.is_volume]
    if verbose:
        print(f"Computing union  ({len(solid_parts)}/{len(parts)} solid parts)...")
    _t0 = _time.perf_counter()
    if len(solid_parts) == 0:
        combined = trimesh.util.concatenate(parts)
    elif len(solid_parts) == 1:
        combined = solid_parts[0]
    else:
        combined = trimesh.boolean.union(solid_parts, engine='manifold')
    _t1 = _time.perf_counter()
    if verbose:
        wt_label = "watertight" if combined.is_watertight else "NOT watertight"
        print(f"  vertices: {len(combined.vertices):,}   "
              f"faces: {len(combined.faces):,}   "
              f"{wt_label}   {_t1 - _t0:.1f}s")
    return combined


# ── Public API ────────────────────────────────────────────────────────────────


def _new_tile_paths(spec_path: pathlib.Path,
                    cols: int, rows: int,
                    tiles_root: pathlib.Path,
                    stl_root: pathlib.Path) -> dict[str, pathlib.Path]:
    """Return ``{system: path}`` for the canonical output hierarchy.

    Naming: ``stl/{system}/{subdir}/{N}x{M}-{stem}-{db|ol}.stl``
    where *subdir* mirrors any directory nesting under *tiles_root*.
    """
    try:
        no_py   = spec_path.with_suffix('')                              # foo.tile.py → foo.tile
        no_tile = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
        rel     = no_tile.relative_to(tiles_root)
    except ValueError:
        no_py   = pathlib.Path(spec_path.stem)                           # spec outside src/tiles/
        rel     = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
    stem   = f"{cols}x{rows}-{rel.name}"
    subdir = rel.parent                       # Path('.') when file is at tiles root
    return {
        dungeonblocks.SYSTEM_SUFFIX: stl_root / 'dungeonblocks' / subdir / f"{stem}-db.stl",
        openlock.SYSTEM_SUFFIX:      stl_root / 'openlock'      / subdir / f"{stem}-ol.stl",
    }


def _make_ol_surface(surface: SurfaceConfig) -> SurfaceConfig:
    """Return a copy of *surface* scaled to the OpenLOCK 25.4 mm per square standard."""
    return dataclasses.replace(surface, square_mm=openlock.OPENLOCK_SQUARE_MM)


def _export_system_stls(tile_mesh: trimesh.Trimesh,
                        cfg: SceneConfig,
                        terrain_z: np.ndarray,
                        output_path: pathlib.Path,
                        verbose: bool = True,
                        *,
                        ol_tile_mesh: trimesh.Trimesh | None = None,
                        ol_surface: SurfaceConfig | None = None,
                        ol_terrain_z: np.ndarray | None = None,
                        system_paths: dict[str, pathlib.Path] | None = None,
                        ) -> dict[str, trimesh.Trimesh]:
    """Export one STL per base system from a base-less tile mesh.

    *system_paths* — when provided, a ``{system_suffix: output_path}`` dict
    that overrides the paths derived from *output_path*.  Use this to place
    outputs in the canonical ``stl/{system}/…`` hierarchy.

    *ol_tile_mesh* / *ol_surface* / *ol_terrain_z* — when supplied, the
    OpenLOCK export uses these instead of the DungeonBlocks-scale values.
    """
    if cfg.base.style == 'none':
        output_path.parent.mkdir(parents=True, exist_ok=True)
        tile_mesh.export(str(output_path))
        if verbose:
            print(f"Saved -> {output_path}")
        return {'none': tile_mesh}

    if system_paths is None:
        stem = output_path.stem
        if stem.endswith('-db'):
            system_paths = {dungeonblocks.SYSTEM_SUFFIX: output_path}
        elif stem.endswith('-ol'):
            system_paths = {openlock.SYSTEM_SUFFIX: output_path}
        else:
            system_paths = {
                dungeonblocks.SYSTEM_SUFFIX: output_path.with_name(
                    f"{stem}-db{output_path.suffix}"),
                openlock.SYSTEM_SUFFIX: output_path.with_name(
                    f"{stem}-ol{output_path.suffix}"),
            }

    # Ensure output directories exist
    for path in system_paths.values():
        path.parent.mkdir(parents=True, exist_ok=True)

    result: dict[str, trimesh.Trimesh] = {}
    for system, path in system_paths.items():
        if verbose:
            print(f"Building {system} base and exporting...")
        if system == dungeonblocks.SYSTEM_SUFFIX:
            result[system] = dungeonblocks.export(
                tile_mesh, cfg.surface, cfg.base, terrain_z, path)
        elif system == openlock.SYSTEM_SUFFIX:
            _mesh     = ol_tile_mesh if ol_tile_mesh is not None else tile_mesh
            _surface  = ol_surface   if ol_surface   is not None else cfg.surface
            _terrain  = ol_terrain_z if ol_terrain_z is not None else terrain_z
            result[system] = openlock.export(
                _mesh, _surface, cfg.base, _terrain, path)
        if verbose:
            print(f"Saved -> {path}")
    return result


def build_tile_from_spec(spec: TileSpec,
                         output_path: pathlib.Path,
                         verbose: bool = True,
                         *,
                         system_paths: dict[str, pathlib.Path] | None = None,
                         ) -> trimesh.Trimesh:
    """Build a tile from a Python TileSpec and export system-specific STLs."""
    cfg = _scene_config_from_spec(spec)

    if verbose:
        region_ids = [r.id for r in spec.regions]
        bnd_ids    = [b.id for b in spec.boundaries]
        print(f"=== Building tile from spec "
              f"({cfg.surface.cols}×{cfg.surface.rows} squares, "
              f"grid {cfg.surface.grid_w}×{cfg.surface.grid_h}) ===")
        if region_ids:
            print(f"  Regions:    {region_ids}")
        if bnd_ids:
            print(f"  Boundaries: {bnd_ids}")

    # ── Region mask ───────────────────────────────────────────────────────────
    region_mask: np.ndarray | None = None
    if spec.regions or spec.boundaries:
        region_mask = build_region_mask(spec)

    # ── Height-aware terrain ──────────────────────────────────────────────────
    # Build terrain_z from region heights *before* creating the scene so the
    # slope between different-height regions is present from the start.
    terrain_z = _build_spec_terrain(spec, cfg.surface, region_mask)

    # ── Water region (early, before scene creation) ───────────────────────────
    water_mask, water_height = _collect_water_info(spec, region_mask)

    # ── Boundary strip width: how far to extend displacement into shore ────────
    embed_mm = max(
        (b.width_mm for b in spec.boundaries if b.width_mm > 0),
        default=2.0,
    )

    # ── Extend bank slope into pool, then zero-out pool floor ────────────────
    # First, extrapolate the bank slope into the pool so soil can texture the
    # sloped bed.  Then clamp all pool cells to 0.0 so the pool floor is flat
    # at the tile base — this must happen before the scene is constructed so
    # that terrain_z is truly read-only after init (no mutation inside helpers).
    if water_mask is not None and water_height is not None:
        terrain_z = _extend_bank_slope_into_pool(
            terrain_z, water_mask, water_height, cfg.surface)
        terrain_z[water_mask] = 0.0  # flatten pool floor to tile base

    scene = TileScene(
        config     = cfg,
        terrain_z  = terrain_z,
        terrain_support_z = terrain_z.copy(),
        rock_mask = np.zeros((cfg.surface.grid_h, cfg.surface.grid_w), dtype=bool),
    )

    # Compute once; both DB and OL scenes use the same region_mask and spec.
    grass_mask          = build_grass_mask(region_mask, spec) if region_mask is not None else None
    scatter_pairs       = _collect_scatter_pairs(spec, region_mask)
    soil_carpet_layers  = _collect_soil_carpet_layers(spec, region_mask)
    grass_carpet_layers = _collect_grass_carpet_layers(spec, region_mask)

    scene.grass_mask = grass_mask
    if verbose and grass_mask is not None:
        n_grass = int(grass_mask.sum())
        n_total = grass_mask.size
        print(f"  Grass coverage: {n_grass}/{n_total} cells "
              f"({100 * n_grass / n_total:.0f}%)")
    if verbose and water_mask is not None:
        n_water = int(water_mask.sum())
        n_total = water_mask.size
        print(f"  Water coverage: {n_water}/{n_total} cells "
              f"({100 * n_water / n_total:.0f}%)")

    tile_mesh = _build_mesh(cfg, scene,
                            scatter_pairs=scatter_pairs,
                            soil_carpet_layers=soil_carpet_layers,
                            grass_carpet_layers=grass_carpet_layers,
                            verbose=verbose,
                            water_mask=water_mask, water_height=water_height,
                            water_embed_mm=embed_mm)

    # ── OpenLOCK: regenerate terrain natively at 25.4 mm/square ──────────────
    # The heightmap grid dimensions (grid_w × grid_h) are set by
    # cols × cells_per_square and do not change with square_mm, so the existing
    # region_mask, terrain_z and water_mask are directly reusable.  Only the
    # physical cell/tile sizes differ, which drives correct feature sizing inside
    # the soil, rock and grass layers.
    ol_tile_mesh: trimesh.Trimesh | None = None
    ol_surface:   SurfaceConfig   | None = None
    ol_terrain_z: np.ndarray      | None = None
    if cfg.base.style != 'none':
        if verbose:
            print(f"\n=== Rebuilding scene at OpenLOCK scale "
                  f"({openlock.OPENLOCK_SQUARE_MM} mm/sq) ===")
        ol_surface = _make_ol_surface(cfg.surface)
        ol_cfg     = dataclasses.replace(cfg, surface=ol_surface)
        ol_scene   = TileScene(
            config     = ol_cfg,
            terrain_z  = terrain_z.copy(),
            terrain_support_z = terrain_z.copy(),
            rock_mask = np.zeros(
                (ol_cfg.surface.grid_h, ol_cfg.surface.grid_w), dtype=bool),
        )
        ol_scene.grass_mask = grass_mask   # same result; region_mask unchanged
        ol_tile_mesh = _build_mesh(
            ol_cfg, ol_scene,
            scatter_pairs=scatter_pairs,    # reuse — same spec + region_mask
            soil_carpet_layers=soil_carpet_layers,
            grass_carpet_layers=grass_carpet_layers,
            verbose=verbose,
            water_mask=water_mask, water_height=water_height,
            water_embed_mm=embed_mm)
        ol_terrain_z = ol_scene.terrain_z

    exports = _export_system_stls(tile_mesh, cfg, scene.terrain_z,
                                  output_path, verbose=verbose,
                                  ol_tile_mesh=ol_tile_mesh,
                                  ol_surface=ol_surface,
                                  ol_terrain_z=ol_terrain_z,
                                  system_paths=system_paths)
    return exports.get(dungeonblocks.SYSTEM_SUFFIX, tile_mesh)


# ── Spec → terrain helpers ────────────────────────────────────────────────────

def _build_spec_terrain(spec: TileSpec, surface: SurfaceConfig,
                         region_mask: np.ndarray | None) -> np.ndarray:
    """Derive a terrain heightmap from the region heights declared in *spec*.

    Each region occupies a flat area at its ``effective_height_mm``.  Boundary
    strips occupy their own cells; where adjacent regions differ in height the
    strip receives a blended slope.

    Algorithm
    ---------
    1. Assign each region cell its exact height.
    2. At boundary cells, compute an inverse-distance-weighted (IDW) blend
       of the neighbouring region heights.
    3. Region cells keep their exact region height. Boundary cells use the
       IDW height, so the slope belongs to the boundary strip rather than
       overlapping the neighbouring regions.

    Note: the slope is a vertical (z) ramp; features on the slope (soil
    blobs, stones) are placed at the correct z but their orientation is still
    world-horizontal.  Full slope-normal orientation for placed geometry is a
    future enhancement.
    """
    gh, gw = surface.grid_h, surface.grid_w
    default_h = 5.0

    if not spec.regions or region_mask is None:
        return np.full((gh, gw), default_h, dtype=float)

    heights   = [r.effective_height_mm for r in spec.regions]

    # Fast path: all regions at the same height
    if len(set(heights)) <= 1:
        return np.full((gh, gw), heights[0] if heights else default_h, dtype=float)

    # Step 1: exact heights at region cells
    z_exact = np.full((gh, gw), default_h, dtype=float)
    for idx, h in enumerate(heights):
        z_exact[region_mask == idx] = h

    # Step 2: IDW-blended height for boundary / unassigned cells
    z_idw = np.zeros((gh, gw), dtype=float)
    w_sum  = np.zeros((gh, gw), dtype=float)
    for idx, h in enumerate(heights):
        dist = distance_transform_edt(region_mask != idx)
        w    = 1.0 / (dist + 0.5)
        z_idw += h * w
        w_sum += w
    z_idw /= np.maximum(w_sum, 1e-12)

    # Step 3: boundary strip gets the blended height; regions stay flat.
    z = z_exact.copy()
    z[region_mask < 0] = z_idw[region_mask < 0]
    return z.astype(float)


def _extend_bank_slope_into_pool(terrain_z: np.ndarray,
                                  water_mask: np.ndarray,
                                  water_height: float,
                                  surface: SurfaceConfig) -> np.ndarray:
    """Extrapolate the bank slope downward into the pool zone.

    The slope gradient is measured from the actual terrain_z values in the
    boundary strip just outside the water_mask, then continued inward so the
    pool bed is a natural extension of the bank rather than a flat floor.

    The bed is clamped at ``terrain_z_min`` (0.5 mm above the tile base) so
    the solid slab never becomes paper-thin.

    Returns a copy of *terrain_z* with pool cells updated.
    """
    cell_mm = surface.cell_w

    # ── Measure bank slope from terrain just outside the water boundary ───────
    # scipy's distance_transform_edt gives, for each TRUE cell, the distance to
    # the nearest FALSE cell (and 0 for FALSE cells).  To get the distance from
    # non-water cells to the nearest water cell we therefore invert the mask.
    dist_land_to_water = distance_transform_edt(~water_mask)   # True=land → dist to water
    band = ~water_mask & (dist_land_to_water >= 1) & (dist_land_to_water <= 5)
    if band.any():
        band_z_mean    = float(terrain_z[band].mean())
        band_dist_mean = float(dist_land_to_water[band].mean()) * cell_mm   # mm
        slope_rate     = (band_z_mean - water_height) / max(band_dist_mean, 1e-6)
        slope_rate     = float(np.clip(slope_rate, 0.1, 8.0))
    else:
        slope_rate = 0.8   # fallback: 0.8 mm drop per mm into pool

    # ── Distance from shore inner ring into pool (mm) ─────────────────────────
    # inner_ring: water cells adjacent to non-water (i.e., where bank meets pool)
    water_eroded = binary_erosion(water_mask, border_value=1)
    inner_ring   = water_mask & ~water_eroded

    # For each water cell, distance to the nearest shore cell.
    # EDT gives 0 for False cells and distance to nearest False for True cells,
    # so invert inner_ring: shore cells become False, all others become True.
    dist_from_shore = distance_transform_edt(~inner_ring) * cell_mm   # mm
    dist_from_shore[~water_mask] = 0.0

    # ── Apply quadratic ease-out slope ───────────────────────────────────────
    # t² starts with a non-zero derivative at t=0 (the waterline), so the slope
    # begins immediately with no flat shelf, then eases smoothly into the flat bed.
    terrain_z_min = 0.0
    slope_dist    = (water_height - terrain_z_min) / max(slope_rate, 1e-6)

    t   = np.clip(dist_from_shore / slope_dist, 0.0, 1.0)   # 0 = shore, 1 = bed
    t_s = t * t                                               # quadratic ease-out

    sloped = water_height - (water_height - terrain_z_min) * t_s

    out = terrain_z.copy()
    out[water_mask] = sloped[water_mask]
    return out



def _collect_water_info(spec: TileSpec,
                         region_mask: np.ndarray | None,
                         ) -> tuple[np.ndarray | None, float | None]:
    """Return (water_mask, water_height) or (None, None) if no water regions."""
    if region_mask is None:
        return None, None

    water_indices = [
        i for i, r in enumerate(spec.regions)
        if any(layer.type == 'water' for layer in r.layers)
    ]
    if not water_indices:
        return None, None

    water_height = spec.regions[water_indices[0]].effective_height_mm
    water_mask   = np.zeros(region_mask.shape, dtype=bool)
    for idx in water_indices:
        water_mask |= (region_mask == idx)
    return water_mask, water_height


# ── Spec → config helpers ─────────────────────────────────────────────────────

def _scene_config_from_spec(spec: TileSpec) -> SceneConfig:
    """Build a SceneConfig from a TileSpec using defaults for unspecified layers."""
    return SceneConfig(
        surface          = spec.surface,
        soil             = SoilConfig(),
        rocks            = RocksConfig(),
        base             = BaseConfig(),
        max_stack_height = 2.0,
    )


def _collect_layers(
    spec: TileSpec,
    region_mask: np.ndarray | None,
    layer_types: set[str],
    cfg_class,
    include_boundaries: bool = True,
) -> list[tuple[object, np.ndarray | None]]:
    """Generic helper: collect ``(config, placement_mask)`` pairs from a spec.

    Iterates all regions (and optionally boundaries) in *spec*.  For each
    layer whose ``type`` is in *layer_types*, builds a *cfg_class* instance
    from the class defaults overridden by ``layer.params``, and pairs it with
    a boolean placement mask restricting placement to that zone's cells.

    Region masks:   ``region_mask == idx``  (None when no region_mask)
    Boundary masks: ``region_mask < 0``     (boundary strip cells)
    """
    defaults = vars(cfg_class())
    result: list[tuple[object, np.ndarray | None]] = []

    for idx, region in enumerate(spec.regions):
        for layer in region.layers:
            if layer.type in layer_types:
                cfg = {**defaults, **layer.params}
                mask = (region_mask == idx) if region_mask is not None else None
                result.append((cfg_class(**cfg), mask))

    if include_boundaries:
        for boundary in spec.boundaries:
            for layer in boundary.layers:
                if layer.type in layer_types:
                    cfg = {**defaults, **layer.params}
                    mask = (region_mask < 0) if region_mask is not None else None
                    result.append((cfg_class(**cfg), mask))

    return result


def _collect_scatter_pairs(
    spec: TileSpec,
    region_mask: np.ndarray | None,
) -> list[tuple[object, np.ndarray | None]]:
    """Return ``(prototype, placement_mask)`` pairs for all scatter layers.

    Collects both ``rocks`` and ``grass`` layer types from all regions and
    boundaries, wrapping each in the appropriate prototype class.  The returned
    list is ordered rocks-first (priority 0), grass-second (priority 1) within
    each region; ``ScatterLayer.build()`` preserves this ordering.

    ``rocks`` layer params
    ~~~~~~~~~~~~~~~~~~~~~~
    All ``RocksConfig`` fields are accepted as flat kwargs.  An optional
    ``scatter=ScatterConfig(...)`` key supplies grouping / density overrides;
    without it, a default ``ScatterConfig`` matching the current
    ``rocks_per_square`` count and no Voronoi grouping is used.

    ``grass`` layer params
    ~~~~~~~~~~~~~~~~~~~~~~
    Blade geometry can be supplied as:

    * a ``SpeciesConfig`` instance via ``params=dict(species=my_species)``.
    * flat ``blade_*`` / placement kwargs folded into a ``SpeciesConfig``.
    * any mix: ``species=`` sets the base; flat kwargs override specific fields.

    An optional ``scatter=ScatterConfig(...)`` key overrides distribution
    parameters; otherwise defaults are derived from the ``SpeciesConfig``.

    Grass is not collected from boundary specs (rocks in boundaries are
    supported, matching the previous behaviour).
    """
    _rock_defaults  = vars(RocksConfig())
    _rock_fields    = set(_rock_defaults)
    _spec_default   = SpeciesConfig()
    _spec_fields    = {f.name for f in dataclasses.fields(SpeciesConfig)}

    result: list[tuple[object, np.ndarray | None]] = []

    # Helper: build a ScatterConfig from params (pop 'scatter' key if present)
    def _pop_scatter(params: dict, fallback_fn) -> ScatterConfig:
        sc = params.pop('scatter', None)
        if sc is not None:
            if not isinstance(sc, ScatterConfig):
                raise TypeError(f"'scatter' must be a ScatterConfig, got {type(sc)!r}")
            return sc
        return fallback_fn()

    # ── rocks ──────────────────────────────────────────────────────────────────
    for idx, region in enumerate(spec.regions):
        for layer in region.layers:
            if layer.type == 'rocks':
                mask   = (region_mask == idx) if region_mask is not None else None
                params = dict(layer.params)
                scatter = _pop_scatter(params, lambda: None)  # resolved below
                rock_kwargs = {k: v for k, v in params.items() if k in _rock_fields}
                rocks_cfg   = RocksConfig(**{**_rock_defaults, **rock_kwargs})
                if scatter is None:
                    scatter = ScatterConfig(
                        items_per_square  = rocks_cfg.rocks_per_square,
                        groups_per_square = 0,
                        group_dir_mode    = 'none',
                    )
                result.append((RockPrototype(rocks=rocks_cfg, scatter=scatter), mask))

    for boundary in spec.boundaries:
        for layer in boundary.layers:
            if layer.type == 'rocks':
                mask   = (region_mask < 0) if region_mask is not None else None
                params = dict(layer.params)
                scatter = _pop_scatter(params, lambda: None)
                rock_kwargs = {k: v for k, v in params.items() if k in _rock_fields}
                rocks_cfg   = RocksConfig(**{**_rock_defaults, **rock_kwargs})
                if scatter is None:
                    scatter = ScatterConfig(
                        items_per_square  = rocks_cfg.rocks_per_square,
                        groups_per_square = 0,
                        group_dir_mode    = 'none',
                    )
                result.append((RockPrototype(rocks=rocks_cfg, scatter=scatter), mask))

    # ── grass ──────────────────────────────────────────────────────────────────
    for idx, region in enumerate(spec.regions):
        for layer in region.layers:
            if layer.type == 'grass':
                mask   = (region_mask == idx) if region_mask is not None else None
                params = dict(layer.params)
                scatter = _pop_scatter(params, lambda: None)
                base    = params.pop('species', _spec_default)
                if not isinstance(base, SpeciesConfig):
                    raise TypeError(
                        f"grass layer 'species' must be a SpeciesConfig, got {type(base)!r}"
                    )
                overrides = {k: v for k, v in params.items() if k in _spec_fields}
                species   = dataclasses.replace(base, **overrides) if overrides else base
                if scatter is None:
                    scatter = ScatterConfig(
                        groups_per_square = species.groups_per_square,
                        gap_mm            = species.gap_mm,
                        group_dir_mode    = 'random',
                    )
                result.append((GrassPrototype(species=species, scatter=scatter), mask))

    return result


def _collect_soil_carpet_layers(
    spec: TileSpec,
    region_mask: np.ndarray | None,
) -> list[tuple[SoilConfig, np.ndarray | None]]:
    """Return one ``(SoilConfig, placement_mask)`` pair per soil_carpet layer."""
    return _collect_layers(           # type: ignore[return-value]
        spec, region_mask,
        layer_types={'soil_carpet'},
        cfg_class=SoilConfig,
    )


def _collect_grass_carpet_layers(
    spec: TileSpec,
    region_mask: np.ndarray | None,
) -> list[tuple[GrassUnderlayConfig, np.ndarray | None]]:
    """Return one ``(GrassUnderlayConfig, placement_mask)`` pair per grass_carpet layer.

    Blade geometry can be supplied in three ways (identical to
    ``_collect_grass_configs``):

    * ``species=SpeciesConfig(...)`` — pass the species object directly.
      Share the same instance with the companion ``grass`` layer to guarantee
      that 2D stamps and 3D blades use identical geometry.
    * Flat ``blade_*`` / placement kwargs — folded into a ``SpeciesConfig``.
    * Mix: ``species=`` sets the base; flat kwargs override specific fields.

    Carpet-specific params (``noise_*``, ``blade_raise_mm``,
    ``stamp_min_taper``, ``edge_fade_mm``) are always flat kwargs on
    ``GrassUnderlayConfig``.
    """
    _u_fields = {f.name for f in dataclasses.fields(GrassUnderlayConfig)
                 if f.name != 'species'}
    _s_fields = {f.name for f in dataclasses.fields(SpeciesConfig)}
    _default_u = GrassUnderlayConfig()
    _default_s = SpeciesConfig()
    result: list[tuple[GrassUnderlayConfig, np.ndarray | None]] = []

    for idx, region in enumerate(spec.regions):
        for layer in region.layers:
            if layer.type == 'grass_carpet':
                mask   = (region_mask == idx) if region_mask is not None else None
                params = dict(layer.params)

                # Partition params: carpet-specific vs species (blade geometry).
                u_over = {k: v for k, v in params.items() if k in _u_fields}
                s_over = {k: v for k, v in params.items() if k in _s_fields}

                base_s = params.get('species', _default_s)
                if not isinstance(base_s, SpeciesConfig):
                    raise TypeError(
                        f"grass_carpet layer 'species' must be a SpeciesConfig, "
                        f"got {type(base_s)!r}"
                    )
                species = dataclasses.replace(base_s, **s_over) if s_over else base_s
                cfg     = dataclasses.replace(_default_u, species=species, **u_over)
                result.append((cfg, mask))
    return result



# ── Multi-size helpers ────────────────────────────────────────────────────────

def _sized_spec(spec: TileSpec, cols: int, rows: int) -> TileSpec:
    """Return a copy of *spec* with surface.cols/rows replaced."""
    sized_surface = dataclasses.replace(spec.surface, cols=cols, rows=rows)
    return dataclasses.replace(spec, surface=sized_surface)


def _build_spec_all_sizes(spec: TileSpec,
                           spec_path: pathlib.Path,
                           output: pathlib.Path | None,
                           tiles_root: pathlib.Path,
                           stl_root: pathlib.Path,
                           verbose: bool = True) -> None:
    """Build every size declared in *spec.sizes* and export system STLs.

    Naming: ``{cols}x{rows}-{stem}-{db|ol}.stl`` under *stl_root*.

    When *output* is given explicitly (``--output``):
    - Single-size spec: uses *output* as-is (backward-compatible).
    - Multi-size spec: inserts the size prefix into the output stem so each
      size gets a distinct file.
    """
    for cols, rows in spec.sizes:
        if len(spec.sizes) > 1 and verbose:
            print(f"\n{'━'*60}")
            print(f"  Size: {cols}×{rows}")
            print(f"{'━'*60}")
        sized = _sized_spec(spec, cols, rows)
        if output is None:
            sys_paths = _new_tile_paths(spec_path, cols, rows, tiles_root, stl_root)
            out = sys_paths[dungeonblocks.SYSTEM_SUFFIX]
        elif len(spec.sizes) == 1:
            sys_paths = None
            out = output
        else:
            # Multiple sizes with explicit --output: insert size prefix into stem.
            sys_paths = None
            out = output.with_name(f"{cols}x{rows}-{output.name}")
        build_tile_from_spec(sized, output_path=out, verbose=verbose,
                             system_paths=sys_paths)


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate terrain tile STLs from .tile.py spec files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help=".tile.py Python spec.  Omit to process all src/tiles/")
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=None,
                   help="Override output path (requires --spec).  "
                        "If the path ends with -db.stl or -ol.stl, only that "
                        "format is generated and the path is used as-is.  "
                        "Otherwise -db.stl and -ol.stl variants are written "
                        "alongside the given path.  Omit to use the canonical "
                        "stl/{system}/{NxM}-{name}-{db|ol}.stl hierarchy.")
    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args    = _build_parser().parse_args(argv)
    verbose = not args.quiet

    TILES_ROOT = pathlib.Path("src/tiles")
    STL_ROOT   = pathlib.Path("stl")

    # ── Single spec mode ──────────────────────────────────────────────────────
    if args.spec is not None:
        spec = load_spec(args.spec)
        _build_spec_all_sizes(spec, args.spec, args.output,
                              TILES_ROOT, STL_ROOT, verbose=verbose)
        return

    # ── Batch mode ────────────────────────────────────────────────────────────
    specs = sorted(TILES_ROOT.rglob("*.tile.py"))
    if not specs:
        print(f"No .tile.py files found under {TILES_ROOT}/  "
              f"(pass --spec FILE to target a specific tile)")
        return
    t_batch = _time.perf_counter()
    for sp in specs:
        if verbose:
            print(f"\n{'─'*60}")
            print(f"  {sp}")
            print(f"{'─'*60}")
        spec = load_spec(sp)
        _build_spec_all_sizes(spec, sp, None, TILES_ROOT, STL_ROOT, verbose=verbose)
    elapsed = _time.perf_counter() - t_batch
    n = len(specs)
    print(f"\n{n} spec{'s' if n != 1 else ''} processed in {elapsed:.1f}s  "
          f"({elapsed/n:.1f}s/spec)")


if __name__ == "__main__":
    main()
