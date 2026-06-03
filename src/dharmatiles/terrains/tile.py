"""
Grass tile generator: soil + stones + grass blades on a DungeonBlocks base.

Default command (no arguments) produces the canonical 1×1 tile:
  • flat 35×35 mm terrain base with organic soil bump texture
  • 15 random stones placed on the surface (grass steers around them)
  • 240 groups of grass blades grown segment-by-segment
  • DungeonBlocks-compatible socket-peg base

Usage
─────
    generate-tile-stl                          # default all-grass tile
    generate-tile-stl --seed 42
    generate-tile-stl --cols 3 --rows 3
    generate-tile-stl --spec tiles/half-grass-soil.tile
    generate-tile-stl --spec tiles/half-grass-soil.tile -o stl/custom.stl
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from ..core.config import (SceneConfig, SurfaceConfig, FlowConfig, SolverConfig,
                           GrassConfig, SoilConfig, StonesConfig, BaseConfig,
                           WaterRippleConfig)
from ..core.tile import TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import (make_heightmap_solid, make_dungeonblock_base,
                         select_peg_height, export_coloured_stl)
from ..core.spec import TileSpec, LayerSpec, load_spec
from ..core.region import build_region_mask, build_grass_mask
from ..layers.soil import SoilLayer
from ..layers.stones import StonesLayer
from ..layers.grass import GrassLayer
from ..layers.water import WaterLayer


# ── VisCAM / SolidView colour constants ───────────────────────────────────────

COLOUR_SOIL  = (101,  67,  33, 255)   # earthy brown   — terrain surface
COLOUR_STONE = (120, 120, 120, 255)   # mid-grey        — rocks
COLOUR_GRASS = ( 50, 120,  30, 255)   # natural green   — blades & supports
COLOUR_WATER = ( 30, 100, 200, 255)   # water blue      — flat water surface


def _paint(mesh_or_list, rgba: tuple) -> None:
    """Set a uniform face colour on a Trimesh or list of Trimeshes (in-place)."""
    colour = np.array(rgba, dtype=np.uint8)
    items  = [mesh_or_list] if isinstance(mesh_or_list, trimesh.Trimesh) else mesh_or_list
    for m in items:
        if m is not None and len(m.faces):
            m.visual.face_colors = colour


def _clear_paint(mesh: trimesh.Trimesh) -> None:
    """Leave every face without an explicit VisCAM/SolidView colour."""
    mesh.visual.face_colors = np.zeros((len(mesh.faces), 4), dtype=np.uint8)


def _paint_terrain_top(mesh: trimesh.Trimesh, rgba: tuple) -> None:
    """Colour only top terrain faces; sides and bottom remain unspecified."""
    n_top = int(mesh.metadata.get('top_face_count', 0))
    if n_top <= 0:
        return
    colours = mesh.visual.face_colors.copy()
    colours[:n_top] = np.array(rgba, dtype=np.uint8)
    mesh.visual.face_colors = colours


# ── Shared layer pipeline ─────────────────────────────────────────────────────

def _build_mesh(cfg: SceneConfig,
                scene: TileScene,
                flow_angle: np.ndarray,
                flow_curv: np.ndarray,
                grass_cfgs: list[GrassConfig] | None = None,
                verbose: bool = True,
                water_mask: np.ndarray | None = None,
                water_height: float | None = None) -> trimesh.Trimesh:
    """Run all layers on *scene* and return the concatenated mesh.

    *grass_cfgs* is a list of GrassConfig objects — one per seed-packet
    layer in the spec.  Defaults to ``[cfg.grass]`` (single packet).

    *water_mask* / *water_height* — when provided, the soil layer's bumps
    are zeroed out in the water region (keeping the pool floor flat), and a
    blue placeholder water-surface mesh is inserted above the terrain solid.
    """
    if grass_cfgs is None:
        grass_cfgs = [cfg.grass]

    parts: list[trimesh.Trimesh] = []

    # ── Soil ──────────────────────────────────────────────────────────────────
    # For dry riverbeds, snapshot which water cells are at the flat floor
    # *before* soil runs so we can strip texture from them afterward.
    # Slope cells (above the floor) keep their texture; the flat bed stays bare.
    DRY_BED_HEIGHT_MM = 0.2
    flat_bed_mask: np.ndarray | None = None
    if water_mask is not None:
        flat_bed_mask = water_mask & (scene.terrain_z <= DRY_BED_HEIGHT_MM + 0.01)

    if verbose:
        print("Building soil texture...")
    SoilLayer(cfg.surface, cfg.soil).build(scene)

    # Restore flat bed to exactly 0.2 mm — no soil texture on the channel floor.
    if flat_bed_mask is not None and np.any(flat_bed_mask):
        scene.terrain_z[flat_bed_mask] = DRY_BED_HEIGHT_MM

    # Dry riverbed: set support_z to the bed floor level so grass/stones don't
    # seed into the channel.  No water clipping or ripple computation needed.
    if water_mask is not None:
        scene.support_z[water_mask] = DRY_BED_HEIGHT_MM

    # No ripple/overflow computation — water surface is not rendered.
    overflow_mask = None

    # ── Stones ────────────────────────────────────────────────────────────────
    n_squares = cfg.surface.cols * cfg.surface.rows
    n_stones  = cfg.stones.stones_per_square * n_squares
    if n_stones > 0:
        if verbose:
            print(f"Building stones  ({n_stones} stones = "
                  f"{cfg.stones.stones_per_square}/sq × {n_squares} sq)...")
        stone_parts = StonesLayer(cfg.surface, cfg.stones).build(scene)
        _paint(stone_parts, COLOUR_STONE)
        parts.extend(stone_parts)

    # ── Grass (one pass per seed-packet config) ───────────────────────────────
    if verbose:
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
        _paint(grass_parts, COLOUR_GRASS)
        parts.extend(grass_parts)

    # ── Water surface — not rendered (dry riverbed mode) ─────────────────────
    # Water surface mesh is intentionally omitted.  The sloped bed is left bare
    # so a separate water layer can be added on top later.

    # ── Terrain solid ─────────────────────────────────────────────────────────
    # omit_top_mask covers only overflow cells (not the full water zone) so that
    # the pool floor retains terrain faces — revealed when ripple troughs dip the
    # water surface below the render_lift threshold.
    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, cfg.surface.tile_w, cfg.surface.tile_h, cfg.surface.base_h,
        omit_top_mask=overflow_mask,   # None if no overflow → all terrain faces kept
    )
    _clear_paint(terrain_mesh)
    _paint_terrain_top(terrain_mesh, COLOUR_SOIL)
    parts.insert(0, terrain_mesh)

    # ── DungeonBlocks base ────────────────────────────────────────────────────
    if cfg.base.style == 'dungeonblock':
        peg_h  = select_peg_height(scene.terrain_z, cfg.base)
        n_pegs = cfg.surface.cols * cfg.surface.rows
        if verbose:
            print(f"Building dungeonblock base  "
                  f"(peg_height={peg_h:.1f} mm, "
                  f"{n_pegs} peg{'s' if n_pegs != 1 else ''})...")
        base_mesh = make_dungeonblock_base(cfg.surface, peg_h, cfg.base)
        _clear_paint(base_mesh)
        parts.insert(0, base_mesh)

    if verbose:
        print("Concatenating...")
    combined = trimesh.util.concatenate(parts)
    if verbose:
        print(f"  vertices: {len(combined.vertices):,}   "
              f"faces: {len(combined.faces):,}   "
              f"watertight: {combined.is_watertight}")
    return combined


# ── Public API ────────────────────────────────────────────────────────────────

def build_tile(cfg: SceneConfig,
               output_path: pathlib.Path,
               verbose: bool = True) -> trimesh.Trimesh:
    """Build a tile from a SceneConfig and export it to *output_path*."""
    if verbose:
        print(f"=== Building tile "
              f"({cfg.surface.cols}×{cfg.surface.rows} squares, "
              f"grid {cfg.surface.grid_w}×{cfg.surface.grid_h}) ===")

    scene = TileScene.from_config(cfg)

    if verbose:
        print(f"Building flow field  ({cfg.flow.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg.surface)
    flow_angle, flow_curv = build_flow_field(cfg.surface, cfg.flow, x_grid, y_grid)

    combined = _build_mesh(cfg, scene, flow_angle, flow_curv, verbose=verbose)

    export_coloured_stl(combined, output_path)
    if verbose:
        print(f"Saved → {output_path}  (VisCAM colours embedded)")
    return combined


def build_tile_from_spec(spec: TileSpec,
                         output_path: pathlib.Path,
                         verbose: bool = True) -> trimesh.Trimesh:
    """Build a tile from a YAML/Python TileSpec and export it to *output_path*."""
    cfg = _scene_config_from_spec(spec)
    cfg.stones = _collect_stones_config(spec)

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
        scene.stone_placement_mask = _build_stone_placement_mask(region_mask, spec)
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

    if verbose:
        print(f"Building flow field  ({cfg.flow.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg.surface)
    flow_angle, flow_curv = build_flow_field(cfg.surface, cfg.flow, x_grid, y_grid)

    grass_cfgs = _collect_grass_configs(spec)
    combined   = _build_mesh(cfg, scene, flow_angle, flow_curv,
                             grass_cfgs=grass_cfgs, verbose=verbose,
                             water_mask=water_mask, water_height=water_height)

    export_coloured_stl(combined, output_path)
    if verbose:
        print(f"Saved → {output_path}  (VisCAM colours embedded)")
    return combined


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

    cell_mm = 0.5 * (surface.cell_w + surface.cell_h)

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
    # Bed floors at 0.2 mm above the tile base (a thin but solid slab).
    # Rather than a linear ramp (which has sharp corners at top and bottom),
    # we normalise dist → t ∈ [0, 1] over the full slope span and apply a
    # smoothstep curve  t_s = 3t² − 2t³  whose derivative is 0 at both ends.
    # This eases gently out of the bank at the top and eases smoothly into the
    # flat bed at the bottom, removing both hard corners.
    terrain_z_min = 0.2
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


def _build_stone_placement_mask(region_mask: np.ndarray,
                                spec: TileSpec) -> np.ndarray | None:
    """Return True where explicitly requested stone centres may be placed.

    In spec mode, stones are opt-in. A region must include a ``rock``,
    ``rocks``, ``stone``, or ``stones`` layer for stone scatter to run there.
    A boundary strip may also include one of those layer types, which enables
    stone centres in boundary cells. Stone geometry may extend past the edge
    of the placement region; only the tile edge clips placement.
    """
    stone_layer_types = {'rock', 'rocks', 'stone', 'stones'}
    stone_region_indices = {
        i for i, r in enumerate(spec.regions)
        if any(layer.type in stone_layer_types for layer in r.layers)
    }
    result = np.zeros(region_mask.shape, dtype=bool)
    for idx in stone_region_indices:
        result |= (region_mask == idx)
    if any(
        layer.type in stone_layer_types
        for boundary in spec.boundaries
        for layer in boundary.layers
    ):
        result |= (region_mask < 0)
    return result


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
    return cfgs or [defaults]


def _collect_stones_config(spec: TileSpec) -> StonesConfig:
    """Return StonesConfig from explicit rock/stone layers, or disabled."""
    defaults = StonesConfig()
    stone_layer_types = {'rock', 'rocks', 'stone', 'stones'}
    cfg = vars(defaults).copy()
    found = False
    for region in spec.regions:
        for layer in region.layers:
            if layer.type in stone_layer_types:
                cfg.update(layer.params)
                found = True
    for boundary in spec.boundaries:
        for layer in boundary.layers:
            if layer.type in stone_layer_types:
                cfg.update(layer.params)
                found = True
    if not found:
        cfg['stones_per_square'] = 0
    return StonesConfig(**cfg)


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    _S = SurfaceConfig()
    _F = FlowConfig()
    _G = GrassConfig()
    _V = StonesConfig()
    _B = BaseConfig()

    p = argparse.ArgumentParser(
        description="Generate a terrain tile STL.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help="YAML (or .tile.py) tile spec; overrides all other flags")
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=pathlib.Path("stl/tile.stl"),
                   help="Output STL path")
    p.add_argument("--seed",       type=int,   default=_S.seed)
    p.add_argument("--cols",       type=int,   default=_S.cols,
                   help="Number of 35 mm squares in X")
    p.add_argument("--rows",       type=int,   default=_S.rows,
                   help="Number of 35 mm squares in Y")
    p.add_argument("--base-h",     type=float, default=_S.base_h, dest="base_h",
                   help="Slab depth below terrain surface (mm)")
    p.add_argument("--stones-per-square", type=int, default=_V.stones_per_square,
                   dest="stones_per_square")
    p.add_argument("--r-max",      type=float, default=_V.r_max,   dest="r_max")
    p.add_argument("--size-power", type=float, default=_V.size_power, dest="size_power")
    p.add_argument("--flow-type",  type=str,   default=_F.flow_type,
                   choices=["linear","swirl","radial","drain","dipole",
                            "random-zones","curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=_F.flow_curl_noise,
                   dest="flow_curl_noise")
    p.add_argument("--cross-section", type=str, default=_G.cross_section,
                   choices=["triangle","circle","diamond"], dest="cross_section")
    p.add_argument("--groups-per-square", type=int, default=_G.groups_per_square,
                   dest="groups_per_square")
    p.add_argument("--group-min",  type=int,   default=_G.group_min,  dest="group_min")
    p.add_argument("--group-max",  type=int,   default=_G.group_max,  dest="group_max")
    p.add_argument("--group-spread", type=float, default=_G.group_spread_mm,
                   dest="group_spread_mm")
    p.add_argument("--max-segs",   type=int,   default=_G.max_segs,   dest="max_segs")
    p.add_argument("--seg-len",    type=float, default=_G.seg_len,     dest="seg_len")
    p.add_argument("--rise-cap",   type=float, default=_G.rise_cap,    dest="rise_cap")
    p.add_argument("--curl-max",   type=float, default=_G.curl_max,    dest="curl_max")
    p.add_argument("--smooth-sigma", type=float, default=_G.smooth_sigma,
                   dest="smooth_sigma")
    p.add_argument("--root-depth", type=float, default=_G.root_depth, dest="root_depth")
    p.add_argument("--max-bridge", type=float, default=_G.max_bridge_mm,
                   dest="max_bridge_mm")
    p.add_argument("--rolling-terrain", action="store_true", dest="rolling_terrain")
    p.add_argument("--no-base", action="store_true", dest="no_base")
    p.add_argument("--peg-height", type=float, default=None, dest="peg_height")
    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)

    # ── Spec mode ─────────────────────────────────────────────────────────────
    if args.spec is not None:
        spec = load_spec(args.spec)
        build_tile_from_spec(spec, output_path=args.output,
                             verbose=not args.quiet)
        return

    # ── Legacy flag mode ──────────────────────────────────────────────────────
    cfg = SceneConfig(
        surface=SurfaceConfig(
            cols        = args.cols,
            rows        = args.rows,
            base_h      = args.base_h,
            seed        = args.seed,
            flat_terrain= not args.rolling_terrain,
        ),
        flow=FlowConfig(
            flow_type       = args.flow_type,
            flow_curl_noise = args.flow_curl_noise,
        ),
        grass=GrassConfig(
            cross_section   = args.cross_section,
            max_segs        = args.max_segs,
            seg_len         = args.seg_len,
            rise_cap        = args.rise_cap,
            curl_max        = args.curl_max,
            smooth_sigma    = args.smooth_sigma,
            root_depth      = args.root_depth,
            groups_per_square = args.groups_per_square,
            group_min       = args.group_min,
            group_max       = args.group_max,
            group_spread_mm = args.group_spread_mm,
            max_bridge_mm   = args.max_bridge_mm,
        ),
        soil=SoilConfig(),
        stones=StonesConfig(
            stones_per_square = args.stones_per_square,
            r_max             = args.r_max,
            size_power        = args.size_power,
        ),
        base=BaseConfig(
            style      = 'none' if args.no_base else 'dungeonblock',
            peg_height = args.peg_height,
        ),
    )
    build_tile(cfg, output_path=args.output, verbose=not args.quiet)


if __name__ == "__main__":
    main()
