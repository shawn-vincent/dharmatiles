"""
Grass tile: terrain + gravel + grass blades, exported as a single STL.

This is the primary entry point for building a grass terrain tile.
Import :func:`build_grass_tile` directly, or run the module as a script
(``python -m dharmatiles.terrains.grass_tile``).
"""
from __future__ import annotations

import argparse
import pathlib
import sys

import numpy as np
import trimesh

from ..core.tile import TileConfig, TileScene, make_xy_grids
from ..core.flow import build_flow_field
from ..core.mesh import make_heightmap_solid
from ..layers.gravel import GravelLayer
from ..layers.grass import VegetationLayer


# ── Public API ────────────────────────────────────────────────────────────────

def build_grass_tile(cfg: TileConfig,
                     output_path: pathlib.Path,
                     verbose: bool = True) -> trimesh.Trimesh:
    """Build a complete grass tile and export it to *output_path*.

    Pipeline
    ────────
    1. Terrain heightmap
    2. Flow vector field
    3. Gravel layer  → updates support_z
    4. Grass layer   → updates support_z
    5. Terrain solid mesh
    6. Concatenate → export STL

    Parameters
    ----------
    cfg         : tile configuration (immutable).
    output_path : destination ``.stl`` path.
    verbose     : print progress to stdout.

    Returns
    -------
    The combined :class:`trimesh.Trimesh`.
    """
    if verbose:
        print("=== Building grass tile ===")

    # ── Scene ──────────────────────────────────────────────────────────────────
    scene = TileScene.from_config(cfg)

    # ── Flow field ─────────────────────────────────────────────────────────────
    if verbose:
        print(f"Building flow field  ({cfg.flow_type})...")
    x_grid, y_grid = make_xy_grids(cfg)
    flow_angle, flow_curv = build_flow_field(cfg, x_grid, y_grid)

    parts: list = []

    # ── Gravel layer ───────────────────────────────────────────────────────────
    if cfg.n_gravel > 0:
        if verbose:
            print(f"Building gravel  ({cfg.n_gravel} stones)...")
        gravel = GravelLayer(cfg)
        parts.extend(gravel.build(scene))
        if verbose:
            print("  support_z updated")

    # ── Grass layer ────────────────────────────────────────────────────────────
    if cfg.n_blades + cfg.n_fill > 0:
        if verbose:
            print("Building vegetation...")
        veg = VegetationLayer(cfg)
        parts.extend(veg.build(scene, flow_angle, flow_curv, verbose=verbose))

    # ── Terrain solid (prepended so it renders first / sorts cleanly) ──────────
    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, cfg.tile_w, cfg.tile_h, cfg.base_h, subsample=4
    )
    parts.insert(0, terrain_mesh)

    # ── Concatenate & export ───────────────────────────────────────────────────
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
        description="Generate a grass terrain tile STL.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--output", "-o", type=pathlib.Path, default=pathlib.Path("stl/grass.stl"),
                   help="Output STL path")
    p.add_argument("--seed",     type=int,   default=42)
    p.add_argument("--n-blades", type=int,   default=50, dest="n_blades",
                   help="Number of tuft seeds (each expands to tuft-min..tuft-max blades)")
    p.add_argument("--n-gravel", type=int,   default=6000, dest="n_gravel")
    p.add_argument("--curl-max", type=float, default=0.6, dest="curl_max",
                   help="Maximum per-blade curl magnitude")
    p.add_argument("--curl-min-fraction", type=float, default=0.0,
                   dest="curl_min_fraction",
                   help="Minimum curl magnitude as a fraction of curl-max")
    p.add_argument("--density-candidate-factor", type=float, default=4.0,
                   dest="density_candidate_factor",
                   help="Candidate-grid multiplier used for weighted blade placement")
    p.add_argument("--divergence-density-gain", type=float, default=1.5,
                   dest="divergence_density_gain",
                   help="Extra placement weight in positive-divergence flow areas")
    p.add_argument("--edge-density-margin", type=float, default=5.0,
                   dest="edge_density_margin",
                   help="Distance from tile edge over which placement density fades")
    p.add_argument("--edge-density-min", type=float, default=0.25,
                   dest="edge_density_min",
                   help="Placement density multiplier at the tile edge")
    p.add_argument("--flow-type", type=str,  default="linear",
                   choices=["linear", "swirl", "radial", "drain", "dipole",
                            "random-zones", "curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=0.30, dest="flow_curl_noise")
    p.add_argument("--cross-section", type=str, default="triangle",
                   choices=["triangle", "circle", "diamond"],
                   dest="blade_cross_section",
                   help="Grass blade cross-section shape")
    p.add_argument("--leaf-cross-section", type=str, default="triangle",
                   choices=["triangle", "circle", "diamond"],
                   dest="leaf_cross_section",
                   help="Leaf cross-section shape (default: triangle)")
    p.add_argument("--circle-segs", type=int, default=8,
                   dest="blade_circle_segs",
                   help="Segments for 'circle' cross-section")
    p.add_argument("--diamond-equator", type=float, default=0.75,
                   dest="blade_diamond_equator",
                   help="Diamond equator depth (0=top/sharp .. 1=bottom/flat-top); default 0.75")
    p.add_argument("--tuft-min", type=int, default=1, dest="tuft_min",
                   help="Minimum blades per grass tuft")
    p.add_argument("--tuft-max", type=int, default=3, dest="tuft_max",
                   help="Maximum blades per grass tuft")
    p.add_argument("--tuft-spread", type=float, default=60.0, dest="tuft_spread_deg",
                   help="Total angular fan width of a grass tuft in degrees (default 60)")
    # Grass blade geometry
    p.add_argument("--tall-w-min",  type=float, default=1.5,  dest="tall_w_min",
                   help="Minimum grass blade diameter/width")
    p.add_argument("--tall-w-max",  type=float, default=2.0,  dest="tall_w_max",
                   help="Maximum grass blade diameter/width")
    p.add_argument("--tall-l-min",  type=float, default=4.0,  dest="tall_l_min",
                   help="Minimum grass blade body length")
    p.add_argument("--tall-l-max",  type=float, default=14.4, dest="tall_l_max",
                   help="Maximum grass blade body length")
    p.add_argument("--tall-tl-min", type=float, default=1.2,  dest="tall_tl_min",
                   help="Minimum grass blade tip length")
    p.add_argument("--tall-tl-max", type=float, default=4.8,  dest="tall_tl_max",
                   help="Maximum grass blade tip length")
    # Fill blade geometry
    p.add_argument("--fill-w-min",  type=float, default=0.3, dest="fill_w_min",
                   help="Minimum fill grass blade diameter/width")
    p.add_argument("--fill-w-max",  type=float, default=0.5, dest="fill_w_max",
                   help="Maximum fill grass blade diameter/width")
    p.add_argument("--fill-l-min",  type=float, default=4.0, dest="fill_l_min",
                   help="Minimum fill grass blade body length")
    p.add_argument("--fill-l-max",  type=float, default=7.2, dest="fill_l_max",
                   help="Maximum fill grass blade body length")
    p.add_argument("--fill-tl-min", type=float, default=1.2, dest="fill_tl_min",
                   help="Minimum fill grass blade tip length")
    p.add_argument("--fill-tl-max", type=float, default=2.4, dest="fill_tl_max",
                   help="Maximum fill grass blade tip length")
    # Vegetation mix
    p.add_argument("--grass-ratio", type=int, default=5, dest="grass_ratio",
                   help="Grass seeds per ratio unit (default 5)")
    p.add_argument("--leaf-ratio", type=int, default=1, dest="leaf_ratio",
                   help="Leaf seeds per ratio unit (default 1); 0 = grass only")
    # Leaf geometry
    p.add_argument("--leaf-w-min",  type=float, default=3.5, dest="leaf_w_min")
    p.add_argument("--leaf-w-max",  type=float, default=5.5, dest="leaf_w_max")
    p.add_argument("--leaf-l-min",  type=float, default=12.0, dest="leaf_l_min")
    p.add_argument("--leaf-l-max",  type=float, default=22.0, dest="leaf_l_max")
    p.add_argument("--leaf-peak-t", type=float, default=0.35, dest="leaf_peak_t",
                   help="Normalized position of max leaf width (0=base, 1=tip; default 0.35)")
    p.add_argument("--no-strict", action="store_true",
                   help="Disable strict intersection checking (faster)")
    p.add_argument("--quiet", "-q", action="store_true",
                   help="Suppress progress output")
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)

    cfg = TileConfig(
        seed                  = args.seed,
        n_blades              = args.n_blades,
        n_gravel              = args.n_gravel,
        curl_max              = args.curl_max,
        curl_min_fraction     = args.curl_min_fraction,
        density_candidate_factor = args.density_candidate_factor,
        divergence_density_gain  = args.divergence_density_gain,
        edge_density_margin      = args.edge_density_margin,
        edge_density_min         = args.edge_density_min,
        flow_type             = args.flow_type,
        flow_curl_noise       = args.flow_curl_noise,
        blade_cross_section   = args.blade_cross_section,
        leaf_cross_section    = args.leaf_cross_section,
        blade_circle_segs     = args.blade_circle_segs,
        blade_diamond_equator = args.blade_diamond_equator,
        tuft_min              = args.tuft_min,
        tuft_max              = args.tuft_max,
        tuft_spread           = np.radians(args.tuft_spread_deg),
        tall_w_min            = args.tall_w_min,
        tall_w_max            = args.tall_w_max,
        tall_l_min            = args.tall_l_min,
        tall_l_max            = args.tall_l_max,
        tall_tl_min           = args.tall_tl_min,
        tall_tl_max           = args.tall_tl_max,
        fill_w_min            = args.fill_w_min,
        fill_w_max            = args.fill_w_max,
        fill_l_min            = args.fill_l_min,
        fill_l_max            = args.fill_l_max,
        fill_tl_min           = args.fill_tl_min,
        fill_tl_max           = args.fill_tl_max,
        grass_ratio           = args.grass_ratio,
        leaf_ratio            = args.leaf_ratio,
        leaf_w_min            = args.leaf_w_min,
        leaf_w_max            = args.leaf_w_max,
        leaf_l_min            = args.leaf_l_min,
        leaf_l_max            = args.leaf_l_max,
        leaf_peak_t           = args.leaf_peak_t,
        strict_mode           = not args.no_strict,
    )
    build_grass_tile(cfg, output_path=args.output, verbose=not args.quiet)


if __name__ == "__main__":
    main()
