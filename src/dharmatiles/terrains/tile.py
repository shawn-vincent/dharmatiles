"""
Grass tile generator: soil + stones + grass blades on a DungeonBlocks base.

Usage
─────
    generate-tile-stl
        Batch mode: process every *.tile file under src/tiles/ and write
        outputs to stl/dungeonblocks/ and stl/openlock/ with names like
        1x1-half-grass-soil-db.stl / 1x1-half-grass-soil-ol.stl.
        Sub-directories under src/tiles/ are mirrored in the output trees.

    generate-tile-stl --spec src/tiles/half-grass-soil.tile
        Single tile: same naming and directory conventions as batch.

    generate-tile-stl --spec src/tiles/foo.tile -o stl/custom.stl
        Single tile, explicit output path.
"""
from __future__ import annotations

import argparse
import dataclasses
import pathlib

import numpy as np
import trimesh

from ..core.config import (SceneConfig, SurfaceConfig, FlowConfig, SolverConfig,
                           GrassConfig, SoilConfig, StonesConfig, BaseConfig)
from ..core.tile import TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import make_heightmap_solid
from ..core.spec import TileSpec, load_spec
from ..core.region import build_region_mask, build_grass_mask
from ..bases import dungeonblocks, openlock
from ..layers.soil import SoilLayer
from ..layers.stones import StonesLayer
from ..layers.grass import GrassLayer
from ..layers.water import make_water_volume



# ── Shared layer pipeline ─────────────────────────────────────────────────────

def _build_mesh(cfg: SceneConfig,
                scene: TileScene,
                flow_angle: np.ndarray,
                flow_curv: np.ndarray,
                grass_cfgs: list[GrassConfig] | None = None,
                stone_layers: list[tuple[StonesConfig, np.ndarray | None]] | None = None,
                verbose: bool = True,
                water_mask: np.ndarray | None = None,
                water_height: float | None = None) -> trimesh.Trimesh:
    """Run all layers on *scene* and return the concatenated mesh.

    *grass_cfgs* is a list of GrassConfig objects — one per seed-packet
    layer in the spec.  Defaults to ``[cfg.grass]`` (single packet).

    *stone_layers* is a list of ``(StonesConfig, placement_mask)`` pairs —
    one per stone-layer zone.  Each pair is one independent stone-placement
    pass with its own size distribution and placement region.  Defaults to
    ``[(cfg.stones, None)]`` (single pass, whole-tile placement).

    *water_mask* / *water_height* — when provided, a flat water-surface mesh
    is placed at *water_height* over the water region.
    """
    if grass_cfgs is None:
        grass_cfgs = [cfg.grass]
    if stone_layers is None:
        stone_layers = [(cfg.stones, None)]

    parts: list[trimesh.Trimesh] = []

    # ── Soil ──────────────────────────────────────────────────────────────────
    if verbose:
        print("Building soil texture...")
    SoilLayer(cfg.surface, cfg.soil).build(scene)
    if water_mask is not None:
        scene.terrain_z[water_mask] = 0.0

    # ── Stones (one independent pass per stone-layer zone) ────────────────────
    n_squares = cfg.surface.cols * cfg.surface.rows
    for layer_idx, (stone_cfg, stone_pmask) in enumerate(stone_layers):
        n_stones = stone_cfg.stones_per_square * n_squares
        if n_stones > 0:
            if verbose:
                print(f"Building stones  ({n_stones} stones = "
                      f"{stone_cfg.stones_per_square}/sq × {n_squares} sq)...")
            stone_parts = StonesLayer(cfg.surface, stone_cfg).build(
                scene, placement_mask=stone_pmask, layer_idx=layer_idx)
            parts.extend(stone_parts)

    # ── Grass (one pass per seed-packet config) ───────────────────────────────
    if grass_cfgs and verbose:
        print("Growing grass...")
    for i, g_cfg in enumerate(grass_cfgs):
        packet_cfg = SceneConfig(
            surface=cfg.surface, flow=cfg.flow, grass=g_cfg,
            solver=cfg.solver,   soil=cfg.soil,  stones=cfg.stones,
            base=cfg.base,
        )
        grown = GrassLayer(packet_cfg)
        grass_parts = grown.build(scene, flow_angle, flow_curv,
                                  verbose=(verbose and i == 0))
        parts.extend(grass_parts)

    # ── Water volume ──────────────────────────────────────────────────────────
    if water_mask is not None and water_height is not None:
        if verbose:
            print("Building water volume...")
        water_mesh = make_water_volume(
            scene.terrain_z, water_mask, water_height,
            cfg.surface.tile_w, cfg.surface.tile_h,
            error_threshold=cfg.surface.terrain_simplify_threshold,
            simplify_stride=cfg.surface.terrain_simplify_stride)
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
    import time as _time
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

def _system_output_path(output_path: pathlib.Path, suffix: str) -> pathlib.Path:
    """Return ``path/to/name-SUFFIX.stl`` for a requested output path."""
    output_path = pathlib.Path(output_path)
    return output_path.with_name(f"{output_path.stem}-{suffix}{output_path.suffix}")


def _new_tile_paths(spec_path: pathlib.Path,
                    cols: int, rows: int,
                    tiles_root: pathlib.Path,
                    stl_root: pathlib.Path) -> dict[str, pathlib.Path]:
    """Return ``{system: path}`` for the canonical output hierarchy.

    Naming: ``stl/{system}/{subdir}/{N}x{M}-{stem}-{db|ol}.stl``
    where *subdir* mirrors any directory nesting under *tiles_root*.
    """
    try:
        rel = spec_path.with_suffix('').relative_to(tiles_root)
    except ValueError:
        rel = pathlib.Path(spec_path.stem)   # spec outside src/tiles/ — no subdir
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
        system_paths = {
            dungeonblocks.SYSTEM_SUFFIX: _system_output_path(
                output_path, dungeonblocks.SYSTEM_SUFFIX),
            openlock.SYSTEM_SUFFIX: _system_output_path(
                output_path, openlock.SYSTEM_SUFFIX),
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
    """Build a tile from a YAML/Python TileSpec and export system-specific STLs."""
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

    # ── Extend bank slope into pool ───────────────────────────────────────────
    # The bank slope visible on the shore is extrapolated downward into the pool
    # zone so the bed is a continuous slope (rather than a flat floor) and the
    # soil texture applied by SoilLayer runs across the whole bed.
    if water_mask is not None and water_height is not None:
        terrain_z = _extend_bank_slope_into_pool(
            terrain_z, water_mask, water_height, cfg.surface)

    scene = TileScene(
        config     = cfg,
        terrain_z  = terrain_z,
        support_z  = terrain_z.copy(),
        stone_mask = np.zeros((cfg.surface.grid_h, cfg.surface.grid_w), dtype=bool),
    )

    if region_mask is not None:
        scene.grass_mask = build_grass_mask(region_mask, spec)
        if verbose and scene.grass_mask is not None:
            n_grass = int(scene.grass_mask.sum())
            n_total = scene.grass_mask.size
            print(f"  Grass coverage: {n_grass}/{n_total} cells "
                  f"({100 * n_grass / n_total:.0f}%)")
    if verbose and water_mask is not None:
        n_water = int(water_mask.sum())
        n_total = water_mask.size
        print(f"  Water coverage: {n_water}/{n_total} cells "
              f"({100 * n_water / n_total:.0f}%)")

    grass_cfgs   = _collect_grass_configs(spec)
    if grass_cfgs:
        if verbose:
            print(f"Building flow field  ({cfg.flow.flow_type})...")
        x_grid, y_grid = make_xy_grids(cfg.surface)
        flow_angle, flow_curv = build_flow_field(cfg.surface, cfg.flow, x_grid, y_grid)
    else:
        flow_angle = flow_curv = np.zeros(
            (cfg.surface.grid_h, cfg.surface.grid_w), dtype=float)
    stone_layers = _collect_stones_layers(spec, region_mask)
    tile_mesh    = _build_mesh(cfg, scene, flow_angle, flow_curv,
                               grass_cfgs=grass_cfgs, stone_layers=stone_layers,
                               verbose=verbose,
                               water_mask=water_mask, water_height=water_height)

    # ── OpenLOCK: regenerate terrain natively at 25.4 mm/square ──────────────
    # The heightmap grid dimensions (grid_w × grid_h) are set by
    # cols × cells_per_square and do not change with square_mm, so the existing
    # region_mask, terrain_z and water_mask are directly reusable.  Only the
    # physical cell/tile sizes differ, which drives correct feature sizing inside
    # the soil, stone and grass layers.
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
            support_z  = terrain_z.copy(),
            stone_mask = np.zeros(
                (ol_cfg.surface.grid_h, ol_cfg.surface.grid_w), dtype=bool),
        )
        if region_mask is not None:
            ol_scene.grass_mask = build_grass_mask(region_mask, spec)
        ol_grass_cfgs   = _collect_grass_configs(spec)
        if ol_grass_cfgs:
            ol_x, ol_y = make_xy_grids(ol_surface)
            ol_flow_angle, ol_flow_curv = build_flow_field(
                ol_surface, ol_cfg.flow, ol_x, ol_y)
        else:
            ol_flow_angle = ol_flow_curv = np.zeros(
                (ol_cfg.surface.grid_h, ol_cfg.surface.grid_w), dtype=float)
        ol_stone_layers = _collect_stones_layers(spec, region_mask)
        ol_tile_mesh = _build_mesh(
            ol_cfg, ol_scene, ol_flow_angle, ol_flow_curv,
            grass_cfgs=ol_grass_cfgs, stone_layers=ol_stone_layers,
            verbose=verbose,
            water_mask=water_mask, water_height=water_height)
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
    from scipy.ndimage import distance_transform_edt

    gh, gw = surface.grid_h, surface.grid_w
    default_h = 5.0

    if not spec.regions or region_mask is None:
        return np.full((gh, gw), default_h, dtype=float)

    heights   = [r.effective_height_mm for r in spec.regions]
    n_regions = len(heights)

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
    from scipy.ndimage import binary_erosion, distance_transform_edt

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

    # ── Apply smoothstepped slope ─────────────────────────────────────────────
    # Rather than a linear ramp (which has sharp corners at top and bottom),
    # we normalise dist → t ∈ [0, 1] over the full slope span and apply a
    # smoothstep curve  t_s = 3t² − 2t³  whose derivative is 0 at both ends.
    # This eases gently out of the bank at the top and eases smoothly into the
    # flat bed at the bottom, removing both hard corners.
    terrain_z_min = 0.0
    slope_dist    = (water_height - terrain_z_min) / max(slope_rate, 1e-6)

    t   = np.clip(dist_from_shore / slope_dist, 0.0, 1.0)   # 0 = shore, 1 = bed
    t_s = t * t * (3.0 - 2.0 * t)                            # smoothstep

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
        surface = spec.surface,
        flow    = FlowConfig(),
        grass   = GrassConfig(),    # overridden per grass_cfgs in build_tile_from_spec
        solver  = SolverConfig(),
        soil    = SoilConfig(),
        stones  = StonesConfig(),
        base    = BaseConfig(),
    )


def _collect_grass_configs(spec: TileSpec) -> list[GrassConfig]:
    """Return one GrassConfig per grass LayerSpec found in any region."""
    defaults = GrassConfig()
    cfgs: list[GrassConfig] = []
    for region in spec.regions:
        for layer in region.layers:
            if layer.type == 'grass':
                d = vars(defaults).copy()
                d.update(layer.params)
                cfgs.append(GrassConfig(**d))
    return cfgs


def _collect_stones_layers(
    spec: TileSpec,
    region_mask: np.ndarray | None,
) -> list[tuple[StonesConfig, np.ndarray | None]]:
    """Return one ``(StonesConfig, placement_mask)`` pair per stone layer.

    Each region or boundary that declares a ``rock``/``rocks``/``stone``/
    ``stones`` layer gets its own independent stone-placement pass.  The
    placement mask restricts stone centres to cells belonging to that zone;
    ``None`` means whole-tile placement.

    Keeping layers independent means a pool region and a shoreline boundary
    can use entirely different size distributions without merging.
    """
    stone_layer_types = {'rock', 'rocks', 'stone', 'stones'}
    result: list[tuple[StonesConfig, np.ndarray | None]] = []

    for idx, region in enumerate(spec.regions):
        for layer in region.layers:
            if layer.type in stone_layer_types:
                cfg = vars(StonesConfig()).copy()
                cfg.update(layer.params)
                mask = (region_mask == idx) if region_mask is not None else None
                result.append((StonesConfig(**cfg), mask))

    for boundary in spec.boundaries:
        for layer in boundary.layers:
            if layer.type in stone_layer_types:
                cfg = vars(StonesConfig()).copy()
                cfg.update(layer.params)
                # boundary cells: region_mask == -1
                mask = (region_mask < 0) if region_mask is not None else None
                result.append((StonesConfig(**cfg), mask))

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
        description="Generate terrain tile STLs from .tile spec files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help="YAML (or .tile.py) tile spec.  Omit to process all src/tiles/")
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=None,
                   help="Override output path (requires --spec); system suffixes "
                        "are inserted before .stl.  Omit to use the canonical "
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
    specs = sorted(TILES_ROOT.rglob("*.tile"))
    if not specs:
        print(f"No .tile files found under {TILES_ROOT}/  "
              f"(pass --spec FILE to target a specific tile)")
        return
    import time as _time
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
