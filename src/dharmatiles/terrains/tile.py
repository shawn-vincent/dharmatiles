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
    if verbose:
        print("Building soil texture...")
    SoilLayer(cfg.surface, cfg.soil).build(scene)

    # Soil bumps must not deform the water surface. Boundary strips are
    # separate cells, so water_mask should contain only the flat pool region.
    if water_mask is not None and water_height is not None:
        scene.terrain_z[water_mask] = water_height
        scene.support_z[water_mask] = water_height

    # ── Ripple overflow: expand water boundary where crest escapes ────────────
    # Computed here (before terrain solid) so omit_top_mask and terrain_z can
    # be updated before the solid is built.
    ripple_cfg       = WaterRippleConfig()
    overflow_mask    = None
    z_disp_pre       = None
    effective_water  = water_mask   # may be expanded below

    if water_mask is not None and water_height is not None:
        from scipy.ndimage import binary_dilation
        from ..layers.water import _build_ripple_displacement as _compute_disp

        extend_cells  = max(1, int(ripple_cfg.extend_mm / cfg.surface.cell_w))
        extended_mask = binary_dilation(water_mask, iterations=extend_cells)

        z_disp_pre = _compute_disp(
            cfg.surface, water_mask,
            scene.stone_mask, scene.grass_mask,
            ripple_cfg,
            compute_mask=extended_mask,
        )

        # Overflow: border cells where ripple crest is positive
        overflow_mask = extended_mask & ~water_mask & (z_disp_pre > 0)

        if np.any(overflow_mask):
            # Flatten overflow cells to water level so the water mesh caps them
            scene.terrain_z[overflow_mask] = water_height
            scene.support_z[overflow_mask] = water_height
            effective_water = water_mask | overflow_mask

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

    # ── Water surface (with ripples, possibly overflowing boundary) ──────────
    if water_mask is not None and water_height is not None:
        n_overflow = int(np.sum(overflow_mask)) if overflow_mask is not None else 0
        if verbose:
            print(f"Building water surface (ripples, +{n_overflow} overflow cells)...")
        water_layer = WaterLayer(cfg.surface, water_height, ripple_cfg=ripple_cfg)
        water_parts = water_layer.build(
            water_mask,
            stone_mask    = scene.stone_mask,
            grass_mask    = scene.grass_mask,
            effective_mask = effective_water,
            z_disp_pre    = z_disp_pre,
        )
        _paint(water_parts, COLOUR_WATER)
        parts.extend(water_parts)

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

    # ── Water region ──────────────────────────────────────────────────────────
    water_mask, water_height = _collect_water_info(spec, region_mask)
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
