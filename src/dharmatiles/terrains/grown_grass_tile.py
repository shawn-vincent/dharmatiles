"""
Grown-grass tile: terrain + gravel + grown-grass blades, exported as a single STL.

Blades are grown segment-by-segment from zone-placed seeds rather than being
placed all at once.  See layers/grown_grass.py for the growth algorithm.

Usage
─────
    python -m dharmatiles.terrains.grown_grass_tile
    python -m dharmatiles.terrains.grown_grass_tile --output stl/grown.stl --seed 42
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from ..core.tile import TileConfig, TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import make_heightmap_solid
from ..layers.gravel import GravelLayer
from ..layers.grown_grass import GrownGrassLayer


# ── Public API ────────────────────────────────────────────────────────────────

def build_grown_grass_tile(cfg: TileConfig,
                            output_path: pathlib.Path,
                            grown_kwargs: dict | None = None,
                            verbose: bool = True) -> trimesh.Trimesh:
    """Build a grown-grass tile and export it to *output_path*.

    Parameters
    ----------
    grown_kwargs : optional dict of GrownGrassLayer attribute overrides,
                  e.g. {'n_groups': 41, 'group_min': 10, 'group_max': 15}.
    """
    if verbose:
        print("=== Building grown-grass tile ===")

    scene = TileScene.from_config(cfg)

    if verbose:
        print(f"Building flow field  ({cfg.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg)
    flow_angle, flow_curv = build_flow_field(cfg, x_grid, y_grid)

    parts: list = []

    if cfg.n_gravel > 0:
        if verbose:
            print(f"Building gravel  ({cfg.n_gravel} stones)...")
        gravel = GravelLayer(cfg)
        parts.extend(gravel.build(scene))

    if verbose:
        print("Growing grass...")
    grown = GrownGrassLayer(cfg)
    for k, v in (grown_kwargs or {}).items():
        setattr(grown, k, v)
    parts.extend(grown.build(scene, flow_angle, flow_curv, verbose=verbose))

    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, cfg.tile_w, cfg.tile_h, cfg.base_h, subsample=4
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
    g = GrownGrassLayer    # class-level defaults
    p = argparse.ArgumentParser(
        description="Generate a grown-grass terrain tile STL.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=pathlib.Path("stl/grass.stl"))
    p.add_argument("--seed",     type=int, default=377)
    p.add_argument("--n-gravel", type=int, default=6000, dest="n_gravel")
    p.add_argument("--flow-type", type=str, default="random-zones",
                   choices=["linear", "swirl", "radial", "drain", "dipole",
                            "random-zones", "curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=0.0,
                   dest="flow_curl_noise")
    p.add_argument("--cross-section", type=str, default="circle",
                   choices=["triangle", "circle", "diamond"],
                   dest="blade_cross_section")

    # Grown-grass parameters (defaults pulled from GrownGrassLayer class)
    p.add_argument("--n-groups",       type=int,   default=g.n_groups,
                   dest="n_groups",    help="Number of grass groups")
    p.add_argument("--group-min",      type=int,   default=g.group_min,
                   dest="group_min",   help="Min blades per group")
    p.add_argument("--group-max",      type=int,   default=g.group_max,
                   dest="group_max",   help="Max blades per group")
    p.add_argument("--group-spread",   type=float, default=g.group_spread_mm,
                   dest="group_spread_mm", help="Blade scatter radius per group (mm)")
    p.add_argument("--max-segs",       type=int,   default=g.max_segs,
                   dest="max_segs",    help="Max growth segments per blade")
    p.add_argument("--seg-len",        type=float, default=g.seg_len,
                   dest="seg_len",     help="Segment step length (mm)")
    p.add_argument("--rise-cap",       type=float, default=g.rise_cap,
                   dest="rise_cap",    help="Max tolerated rise per step before turning (mm)")
    p.add_argument("--curl-max",       type=float, default=g.curl_max,
                   dest="curl_max",    help="Max blade curl magnitude")
    p.add_argument("--curl-min-frac",  type=float, default=g.curl_min_fraction,
                   dest="curl_min_fraction", help="Min curl as fraction of max")
    p.add_argument("--smooth-sigma",   type=float, default=g.smooth_sigma,
                   dest="smooth_sigma", help="Gaussian smoothing width (segments)")
    p.add_argument("--root-depth",     type=float, default=g.root_depth,
                   dest="root_depth",  help="Underground anchor depth (mm)")

    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)
    cfg = TileConfig(
        seed                = args.seed,
        n_gravel            = args.n_gravel,
        flow_type           = args.flow_type,
        flow_curl_noise     = args.flow_curl_noise,
        blade_cross_section = args.blade_cross_section,
    )
    grown_kwargs = dict(
        n_groups          = args.n_groups,
        group_min         = args.group_min,
        group_max         = args.group_max,
        group_spread_mm   = args.group_spread_mm,
        max_segs          = args.max_segs,
        seg_len           = args.seg_len,
        rise_cap          = args.rise_cap,
        curl_max          = args.curl_max,
        curl_min_fraction = args.curl_min_fraction,
        smooth_sigma      = args.smooth_sigma,
        root_depth        = args.root_depth,
    )
    build_grown_grass_tile(cfg, output_path=args.output,
                           grown_kwargs=grown_kwargs, verbose=not args.quiet)


if __name__ == "__main__":
    main()
