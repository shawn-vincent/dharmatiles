"""
Tile generator: assemble a Tile spec into STL output(s).

Usage
─────
    generate-tile-stl
        Batch mode: process every ``*.tile.py`` file under ``src/tiles/`` and
        write outputs to ``stl/{system_dir}/…``.

    generate-tile-stl --spec "src/tiles/soil+grass.tile.py"
        Single tile: same naming and directory conventions as batch.

    generate-tile-stl --quiet
        Suppress progress output.
"""
from __future__ import annotations

import argparse
import dataclasses
import pathlib
import time as _time

import numpy as np
import trimesh
from scipy.ndimage import distance_transform_edt

from ..core.config import SurfaceConfig
from ..core.tile import TileScene
from ..core.mesh import make_heightmap_solid
from ..core.region import build_region_mask
from ..spec import Tile, Region, Boundary, load_spec


# ── Internal mesh builder ─────────────────────────────────────────────────────

def _build_tile_mesh(
    tile: Tile,
    surface: SurfaceConfig,
    region_mask: np.ndarray | None,
    verbose: bool = True,
) -> tuple[trimesh.Trimesh, TileScene]:
    """Run all spec layers in ``tile.areas`` order; return (mesh, scene).

    *surface* may differ from ``tile.surface`` when a system builds at a
    different scale (e.g. OpenLOCK at 25.4 mm/sq).  *region_mask* is
    always the one computed at the primary (spec) surface and is reused
    across scales — fractional positions are scale-invariant.
    """
    terrain_z = _build_spec_terrain(tile, surface, region_mask)

    scene = TileScene(
        surface           = surface,
        terrain_z         = terrain_z,
        terrain_support_z = terrain_z.copy(),
        rock_mask         = np.zeros((surface.grid_h, surface.grid_w), dtype=bool),
    )

    parts: list[trimesh.Trimesh] = []

    if region_mask is not None:
        boundary_mask = (region_mask < 0)
        # Build a fast id → index lookup so we can map each Region to its mask
        # value.  The index matches enumerate(tile.regions) order.
        region_idx: dict[int, int] = {
            id(r): i for i, r in enumerate(tile.regions)
        }

        for area in tile.areas:
            if isinstance(area, Region):
                idx  = region_idx[id(area)]
                mask = (region_mask == idx)
                for layer in area.layers:
                    parts.extend(layer.apply(scene, placement_mask=mask))
            elif isinstance(area, Boundary) and area.layers:
                for layer in area.layers:
                    parts.extend(layer.apply(scene, placement_mask=boundary_mask))

    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, surface.tile_w, surface.tile_h,
        surface.base_h,
        error_threshold=surface.terrain_simplify_threshold,
        simplify_stride=surface.terrain_simplify_stride,
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
    return combined, scene


# ── Public build API ──────────────────────────────────────────────────────────

def build_tile_from_spec(
    tile:         Tile,
    *,
    system_paths: dict[str, pathlib.Path],
    verbose:      bool = True,
) -> trimesh.Trimesh:
    """Build a tile from a Python ``Tile`` spec and export one STL per system.

    *system_paths* maps each system's ``suffix`` to its output path.
    The orchestrator loops over ``tile.systems`` in declaration order,
    builds a mesh at the system's scale (caching equal-scale meshes), and
    calls ``system.export()``.

    Returns the mesh from the first system (typically DungeonBlocks).
    """
    surface = tile.surface

    if verbose:
        region_ids = [r.id for r in tile.regions]
        bnd_ids    = [b.id for b in tile.boundaries]
        print(f"=== Building tile from spec "
              f"({surface.cols}x{surface.rows} squares, "
              f"grid {surface.grid_w}x{surface.grid_h}) ===")
        if region_ids:
            print(f"  Regions:    {region_ids}")
        if bnd_ids:
            print(f"  Boundaries: {bnd_ids}")

    region_mask = build_region_mask(tile) if tile.areas else None

    # Cache (tile_mesh, scene) keyed by square_mm so we don't rebuild
    # at the same scale twice (e.g. two DB-scale systems would share one build).
    built: dict[float, tuple[trimesh.Trimesh, TileScene]] = {}
    first_result: trimesh.Trimesh | None = None

    for system in tile.systems:
        sys_surface = system.surface_for(surface)
        sq_mm       = sys_surface.square_mm

        if sq_mm not in built:
            is_primary = (sq_mm == surface.square_mm and not built)
            if verbose and not is_primary:
                print(f"\n=== Rebuilding scene at {sys_surface.square_mm} mm/sq ===")
            # Build with this system's surface scale; reuse region_mask.
            sys_tile = dataclasses.replace(tile, surface=sys_surface)
            mesh, scene = _build_tile_mesh(
                sys_tile, sys_surface, region_mask,
                verbose=verbose,
            )
            built[sq_mm] = (mesh, scene)

        tile_mesh, scene = built[sq_mm]
        out_path = system_paths[system.suffix]

        if verbose:
            print(f"Building {system.suffix} base and exporting...")
        result = system.export(tile_mesh, sys_surface, scene.terrain_z, out_path)
        if verbose:
            print(f"Saved -> {out_path}")

        if first_result is None:
            first_result = result

    return first_result or trimesh.Trimesh()


# ── Spec → terrain helper ─────────────────────────────────────────────────────

def _build_spec_terrain(
    tile:        Tile,
    surface:     SurfaceConfig,
    region_mask: np.ndarray | None,
) -> np.ndarray:
    """Derive a terrain heightmap from region heights declared in *tile*.

    Algorithm
    ---------
    1. Assign each region cell its exact ``effective_height_mm``.
    2. Boundary cells get an inverse-distance-weighted (IDW) blend of
       neighbouring region heights.
    """
    gh, gw    = surface.grid_h, surface.grid_w
    default_h = 5.0

    if not tile.regions or region_mask is None:
        return np.full((gh, gw), default_h, dtype=float)

    heights = [r.terrain.height_mm for r in tile.regions]

    if len(set(heights)) <= 1:
        return np.full((gh, gw), heights[0] if heights else default_h, dtype=float)

    z_exact = np.full((gh, gw), default_h, dtype=float)
    for idx, h in enumerate(heights):
        z_exact[region_mask == idx] = h

    z_idw = np.zeros((gh, gw), dtype=float)
    w_sum = np.zeros((gh, gw), dtype=float)
    for idx, h in enumerate(heights):
        dist  = distance_transform_edt(region_mask != idx)
        w     = 1.0 / (dist + 0.5)
        z_idw += h * w
        w_sum += w
    z_idw /= np.maximum(w_sum, 1e-12)

    z = z_exact.copy()
    z[region_mask < 0] = z_idw[region_mask < 0]
    return z.astype(float)


# ── CLI helpers ───────────────────────────────────────────────────────────────

def _system_paths_for(
    spec_path:  pathlib.Path,
    tile:       Tile,
    tiles_root: pathlib.Path,
    stl_root:   pathlib.Path,
) -> dict[str, pathlib.Path]:
    """Return ``{system.suffix: output_path}`` for the canonical hierarchy."""
    cols, rows = tile.surface.cols, tile.surface.rows
    try:
        no_py   = spec_path.with_suffix('')
        no_tile = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
        rel     = no_tile.relative_to(tiles_root)
    except ValueError:
        no_py   = pathlib.Path(spec_path.stem)
        rel     = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
    stem = f"{cols}x{rows}-{rel.name}"
    sub  = rel.parent
    return {
        system.suffix: stl_root / system.dir_name / sub / f"{stem}-{system.suffix}.stl"
        for system in tile.systems
    }


def _build_spec(
    tile:       Tile,
    spec_path:  pathlib.Path,
    tiles_root: pathlib.Path,
    stl_root:   pathlib.Path,
    verbose:    bool = True,
) -> None:
    """Build one tile spec at the size declared on its surface config."""
    sys_paths = _system_paths_for(spec_path, tile, tiles_root, stl_root)
    build_tile_from_spec(tile, system_paths=sys_paths, verbose=verbose)


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate terrain tile STLs from .tile.py spec files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help=".tile.py Python spec.  Omit to process all src/tiles/")
    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args    = _build_parser().parse_args(argv)
    verbose = not args.quiet

    TILES_ROOT = pathlib.Path("src/tiles")
    STL_ROOT   = pathlib.Path("stl")

    if args.spec is not None:
        for tile in load_spec(args.spec):
            _build_spec(tile, args.spec, TILES_ROOT, STL_ROOT, verbose=verbose)
        return

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
        for tile in load_spec(sp):
            _build_spec(tile, sp, TILES_ROOT, STL_ROOT, verbose=verbose)
    elapsed = _time.perf_counter() - t_batch
    n = len(specs)
    print(f"\n{n} spec{'s' if n != 1 else ''} processed in {elapsed:.1f}s  "
          f"({elapsed/n:.1f}s/spec)")


if __name__ == "__main__":
    main()
