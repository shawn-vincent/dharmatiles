"""
Grown-grass tile: terrain + gravel + grown-grass blades, exported as a single STL.

Blades are grown segment-by-segment from zone-placed seeds.
See layers/grown_grass.py for the growth algorithm.

Usage
─────
    python -m dharmatiles.terrains.grown_grass_tile
    python -m dharmatiles.terrains.grown_grass_tile --output stl/grown.stl --seed 42
    python -m dharmatiles.terrains.grown_grass_tile --tile-cols 2 --tile-rows 2
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from ..core.config import SceneConfig, SurfaceConfig, FlowConfig, GrassConfig, GravelConfig
from ..core.tile import TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import make_heightmap_solid
from ..layers.gravel import GravelLayer
from ..layers.grown_grass import GrownGrassLayer


# ── Public API ────────────────────────────────────────────────────────────────

def build_grown_grass_tile(cfg: SceneConfig,
                            output_path: pathlib.Path,
                            verbose: bool = True) -> trimesh.Trimesh:
    """Build a grown-grass tile and export it to *output_path*."""
    if verbose:
        print(f"=== Building grown-grass tile "
              f"({cfg.surface.tile_cols}×{cfg.surface.tile_rows} tiles, "
              f"grid {cfg.surface.grid_w}×{cfg.surface.grid_h}) ===")

    scene = TileScene.from_config(cfg)

    if verbose:
        print(f"Building flow field  ({cfg.flow.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg.surface)
    flow_angle, flow_curv = build_flow_field(cfg.surface, cfg.flow,
                                             x_grid, y_grid)

    parts: list = []

    if cfg.gravel.n_gravel > 0:
        if verbose:
            print(f"Building gravel  ({cfg.gravel.n_gravel} stones)...")
        gravel = GravelLayer(cfg.surface, cfg.gravel)
        parts.extend(gravel.build(scene))

    if verbose:
        print("Growing grass...")
    grown = GrownGrassLayer(cfg)
    parts.extend(grown.build(scene, flow_angle, flow_curv, verbose=verbose))

    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z,
        cfg.surface.tile_w,
        cfg.surface.tile_h,
        cfg.surface.base_h,
        subsample=4,
    )
    parts.insert(0, terrain_mesh)

    if verbose:
        print("Concatenating...")
    combined = trimesh.util.concatenate(parts)
    if verbose:
        print(f"  vertices: {len(combined.vertices):,}   "
              f"faces: {len(combined.faces):,}   "
              f"watertight: {combined.is_watertight}")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    combined.export(str(output_path))
    if verbose:
        print(f"Saved → {output_path}")

    return combined


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate a grown-grass terrain tile STL.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=pathlib.Path("stl/grass.stl"))
    p.add_argument("--seed",       type=int,   default=377)
    p.add_argument("--tile-cols",  type=int,   default=1, dest="tile_cols",
                   help="Number of 35 mm tile units in X")
    p.add_argument("--tile-rows",  type=int,   default=1, dest="tile_rows",
                   help="Number of 35 mm tile units in Y")
    p.add_argument("--base-h",     type=float, default=6.0, dest="base_h",
                   help="Slab depth below terrain surface (mm)")
    p.add_argument("--n-gravel",   type=int,   default=6000, dest="n_gravel")
    p.add_argument("--flow-type",  type=str,   default="random-zones",
                   choices=["linear", "swirl", "radial", "drain", "dipole",
                            "random-zones", "curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=0.0,
                   dest="flow_curl_noise")
    p.add_argument("--cross-section", type=str, default="circle",
                   choices=["triangle", "circle", "diamond"],
                   dest="cross_section")
    p.add_argument("--n-groups",   type=int,   default=41, dest="n_groups")
    p.add_argument("--group-min",  type=int,   default=10, dest="group_min")
    p.add_argument("--group-max",  type=int,   default=15, dest="group_max")
    p.add_argument("--group-spread", type=float, default=2.5, dest="group_spread_mm")
    p.add_argument("--max-segs",   type=int,   default=12, dest="max_segs")
    p.add_argument("--seg-len",    type=float, default=0.8, dest="seg_len")
    p.add_argument("--rise-cap",   type=float, default=0.8, dest="rise_cap")
    p.add_argument("--curl-max",   type=float, default=0.8, dest="curl_max")
    p.add_argument("--smooth-sigma", type=float, default=2.0, dest="smooth_sigma")
    p.add_argument("--root-depth", type=float, default=2.0, dest="root_depth")
    p.add_argument("--max-bridge", type=float, default=10.0, dest="max_bridge_mm")
    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)

    cfg = SceneConfig(
        surface=SurfaceConfig(
            tile_cols = args.tile_cols,
            tile_rows = args.tile_rows,
            base_h    = args.base_h,
            seed      = args.seed,
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
            n_groups         = args.n_groups,
            group_min        = args.group_min,
            group_max        = args.group_max,
            group_spread_mm  = args.group_spread_mm,
            max_bridge_mm    = args.max_bridge_mm,
        ),
        gravel=GravelConfig(
            n_gravel = args.n_gravel,
        ),
    )

    build_grown_grass_tile(cfg, output_path=args.output,
                           verbose=not args.quiet)


if __name__ == "__main__":
    main()
