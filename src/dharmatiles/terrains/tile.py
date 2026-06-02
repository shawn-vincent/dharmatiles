"""
Grass tile generator: soil + stones + grass blades on a DungeonBlocks base.

Default command (no arguments) produces the canonical 1×1 tile:
  • flat 35×35 mm terrain base with organic soil bump texture
  • 15 random stones placed on the surface (grass steers around them)
  • 120 groups of grass blades grown segment-by-segment
  • DungeonBlocks-compatible socket-peg base

Usage
─────
    python -m dharmatiles.terrains.tile
    python -m dharmatiles.terrains.tile --seed 42
    python -m dharmatiles.terrains.tile --cols 3 --rows 3
    python -m dharmatiles.terrains.tile --no-base --groups-per-square 0
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from ..core.config import (SceneConfig, SurfaceConfig, FlowConfig,
                           GrassConfig, SoilConfig, StonesConfig, BaseConfig)
from ..core.tile import TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import (make_heightmap_solid, make_dungeonblock_base,
                         select_peg_height, export_coloured_stl)
from ..layers.soil import SoilLayer
from ..layers.stones import StonesLayer
from ..layers.grass import GrassLayer


# ── VisCAM / SolidView color hack ────────────────────────────────────────────
# Binary STL encodes per-face color in the 2-byte attribute field:
#   bit 15 = 1 (color valid), bits 14-10 = R5, bits 9-5 = G5, bits 4-0 = B5
# trimesh writes this automatically when face_colors are set on the mesh.

COLOUR_SOIL  = (101,  67,  33, 255)   # earthy brown   — terrain surface
COLOUR_STONE = (120, 120, 120, 255)   # mid-grey        — rocks
COLOUR_GRASS = ( 50, 120,  30, 255)   # natural green   — blades & supports
COLOUR_BASE  = ( 70,  45,  20, 255)   # dark brown      — dungeonblock underside


def _paint(mesh_or_list, rgba: tuple) -> None:
    """Set a uniform face colour on a Trimesh or list of Trimeshes (in-place)."""
    import numpy as np
    colour = np.array(rgba, dtype=np.uint8)
    items  = [mesh_or_list] if isinstance(mesh_or_list, trimesh.Trimesh) else mesh_or_list
    for m in items:
        if m is not None and len(m.faces):
            m.visual.face_colors = colour


# ── Public API ────────────────────────────────────────────────────────────────

def build_tile(cfg: SceneConfig,
               output_path: pathlib.Path,
               verbose: bool = True) -> trimesh.Trimesh:
    """Build a grass tile and export it to *output_path*."""
    if verbose:
        print(f"=== Building tile "
              f"({cfg.surface.cols}×{cfg.surface.rows} squares, "
              f"grid {cfg.surface.grid_w}×{cfg.surface.grid_h}) ===")

    scene = TileScene.from_config(cfg)

    if verbose:
        print(f"Building flow field  ({cfg.flow.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg.surface)
    flow_angle, flow_curv = build_flow_field(cfg.surface, cfg.flow,
                                             x_grid, y_grid)

    parts: list = []

    if verbose:
        print("Building soil texture...")
    SoilLayer(cfg.surface, cfg.soil).build(scene)

    n_squares = cfg.surface.cols * cfg.surface.rows
    n_stones  = cfg.stones.stones_per_square * n_squares
    if n_stones > 0:
        if verbose:
            print(f"Building stones  ({n_stones} stones = "
                  f"{cfg.stones.stones_per_square}/square × {n_squares} squares)...")
        stones = StonesLayer(cfg.surface, cfg.stones)
        stone_parts = stones.build(scene)
        _paint(stone_parts, COLOUR_STONE)
        parts.extend(stone_parts)

    if verbose:
        print("Growing grass...")
    grown = GrassLayer(cfg)
    grass_parts = grown.build(scene, flow_angle, flow_curv, verbose=verbose)
    _paint(grass_parts, COLOUR_GRASS)
    parts.extend(grass_parts)

    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z,
        cfg.surface.tile_w,
        cfg.surface.tile_h,
        cfg.surface.base_h,
    )
    _paint(terrain_mesh, COLOUR_SOIL)
    parts.insert(0, terrain_mesh)

    if cfg.base.style == 'dungeonblock':
        peg_h = select_peg_height(scene.terrain_z, cfg.base)
        n_pegs = cfg.surface.cols * cfg.surface.rows
        if verbose:
            print(f"Building dungeonblock base  "
                  f"(peg_height={peg_h:.1f} mm, {n_pegs} peg{'s' if n_pegs != 1 else ''})...")
        base_mesh = make_dungeonblock_base(cfg.surface, peg_h, cfg.base)
        _paint(base_mesh, COLOUR_BASE)
        parts.insert(0, base_mesh)

    if verbose:
        print("Concatenating...")
    combined = trimesh.util.concatenate(parts)
    if verbose:
        print(f"  vertices: {len(combined.vertices):,}   "
              f"faces: {len(combined.faces):,}   "
              f"watertight: {combined.is_watertight}")

    export_coloured_stl(combined, output_path)
    if verbose:
        print(f"Saved → {output_path}  (VisCAM colours embedded)")

    return combined


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    # Pull defaults from the config dataclasses — single source of truth.
    _S = SurfaceConfig()
    _F = FlowConfig()
    _G = GrassConfig()
    _V = StonesConfig()
    _B = BaseConfig()

    p = argparse.ArgumentParser(
        description="Generate a grass terrain tile STL.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=pathlib.Path("stl/tile.stl"),
                   help="Output STL path (default: stl/tile.stl)")
    p.add_argument("--seed",       type=int,   default=_S.seed)
    p.add_argument("--cols",  type=int,   default=_S.cols, dest="cols",
                   help="Number of 35 mm squares in X")
    p.add_argument("--rows",  type=int,   default=_S.rows, dest="rows",
                   help="Number of 35 mm squares in Y")
    p.add_argument("--base-h",     type=float, default=_S.base_h, dest="base_h",
                   help="Slab depth below terrain surface (mm)")
    p.add_argument("--stones-per-square", type=int, default=_V.stones_per_square,
                   dest="stones_per_square",
                   help="Stones per 35mm square (scaled by square count)")
    p.add_argument("--r-max", type=float, default=_V.r_max, dest="r_max",
                   help="Maximum stone radius (mm)")
    p.add_argument("--size-power", type=float, default=_V.size_power, dest="size_power",
                   help="Size distribution skew: 1=uniform, >1=mostly small")
    p.add_argument("--flow-type",  type=str,   default=_F.flow_type,
                   choices=["linear", "swirl", "radial", "drain", "dipole",
                            "random-zones", "curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=_F.flow_curl_noise,
                   dest="flow_curl_noise")
    p.add_argument("--cross-section", type=str, default=_G.cross_section,
                   choices=["triangle", "circle", "diamond"],
                   dest="cross_section")
    p.add_argument("--groups-per-square", type=int, default=_G.groups_per_square,
                   dest="groups_per_square",
                   help="Grass groups per 35mm square (scaled by square count)")
    p.add_argument("--group-min",  type=int,   default=_G.group_min,  dest="group_min")
    p.add_argument("--group-max",  type=int,   default=_G.group_max,  dest="group_max")
    p.add_argument("--group-spread", type=float, default=_G.group_spread_mm,
                   dest="group_spread_mm")
    p.add_argument("--max-segs",   type=int,   default=_G.max_segs,   dest="max_segs")
    p.add_argument("--seg-len",    type=float, default=_G.seg_len,    dest="seg_len")
    p.add_argument("--rise-cap",   type=float, default=_G.rise_cap,   dest="rise_cap")
    p.add_argument("--curl-max",   type=float, default=_G.curl_max,   dest="curl_max")
    p.add_argument("--smooth-sigma", type=float, default=_G.smooth_sigma,
                   dest="smooth_sigma")
    p.add_argument("--root-depth", type=float, default=_G.root_depth, dest="root_depth")
    p.add_argument("--max-bridge", type=float, default=_G.max_bridge_mm,
                   dest="max_bridge_mm")
    p.add_argument("--rolling-terrain", action="store_true", dest="rolling_terrain",
                   help="Enable sinusoidal stand-in terrain undulation (legacy)")
    p.add_argument("--quiet", "-q", action="store_true")

    # ── Base ──────────────────────────────────────────────────────────────────
    p.add_argument("--no-base", action="store_true", dest="no_base",
                   help="Omit the underside socket-peg base")
    p.add_argument("--peg-height", type=float, default=None, dest="peg_height",
                   help=("Override peg column height in mm (default: auto — "
                         f"{_B.short_peg_height} mm if max terrain ≤ "
                         f"{_B.auto_threshold_mm} mm, else "
                         f"{_B.tall_peg_height} mm)"))
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)

    cfg = SceneConfig(
        surface=SurfaceConfig(
            cols         = args.cols,
            rows         = args.rows,
            base_h       = args.base_h,
            seed         = args.seed,
            flat_terrain = not args.rolling_terrain,
        ),
        flow=FlowConfig(
            flow_type       = args.flow_type,
            flow_curl_noise = args.flow_curl_noise,
        ),
        grass=GrassConfig(
            cross_section    = args.cross_section,
            max_segs         = args.max_segs,
            seg_len          = args.seg_len,
            rise_cap         = args.rise_cap,
            curl_max         = args.curl_max,
            smooth_sigma     = args.smooth_sigma,
            root_depth       = args.root_depth,
            groups_per_square  = args.groups_per_square,
            group_min        = args.group_min,
            group_max        = args.group_max,
            group_spread_mm  = args.group_spread_mm,
            max_bridge_mm    = args.max_bridge_mm,
        ),
        soil=SoilConfig(),
        stones=StonesConfig(
            stones_per_square = args.stones_per_square,
            r_max           = args.r_max,
            size_power      = args.size_power,
        ),
        base=BaseConfig(
            style      = 'none' if args.no_base else 'dungeonblock',
            peg_height = args.peg_height,
        ),
    )

    build_tile(cfg, output_path=args.output,
               verbose=not args.quiet)


if __name__ == "__main__":
    main()
