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
from ..layers.grass import GrassLayer


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
            print("Building grass blades...")
        grass = GrassLayer(cfg)
        parts.extend(grass.build(scene, flow_angle, flow_curv, verbose=verbose))

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
    p.add_argument("--n-blades", type=int,   default=200, dest="n_blades")
    p.add_argument("--n-gravel", type=int,   default=6000, dest="n_gravel")
    p.add_argument("--flow-type", type=str,  default="linear",
                   choices=["linear", "swirl", "radial", "drain", "dipole", "curl"],
                   dest="flow_type")
    p.add_argument("--flow-curl-noise", type=float, default=0.30, dest="flow_curl_noise")
    p.add_argument("--no-strict", action="store_true",
                   help="Disable strict intersection checking (faster)")
    p.add_argument("--quiet", "-q", action="store_true",
                   help="Suppress progress output")
    return p


def main(argv=None):
    args = _build_parser().parse_args(argv)

    cfg = TileConfig(
        seed            = args.seed,
        n_blades        = args.n_blades,
        n_gravel        = args.n_gravel,
        flow_type       = args.flow_type,
        flow_curl_noise = args.flow_curl_noise,
        strict_mode     = not args.no_strict,
    )
    build_grass_tile(cfg, output_path=args.output, verbose=not args.quiet)


if __name__ == "__main__":
    main()
