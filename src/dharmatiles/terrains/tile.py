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
        Suppress all output.
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
from .reporter import TileReporter, make_reporter


# ── Internal mesh builder ─────────────────────────────────────────────────────

def _build_tile_mesh(
    tile:        Tile,
    surface:     SurfaceConfig,
    region_mask: np.ndarray | None,
    reporter:    TileReporter,
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
        region_idx: dict[int, int] = {
            id(r): i for i, r in enumerate(tile.regions)
        }

        for area in tile.areas:
            if isinstance(area, Region):
                idx  = region_idx[id(area)]
                mask = (region_mask == idx)
                for layer in area.layers:
                    label = f"{area.id}: {type(layer).__name__}"
                    reporter.step_begin(label)
                    t0 = _time.perf_counter()
                    new_parts = layer.apply(scene, placement_mask=mask)
                    elapsed = _time.perf_counter() - t0
                    reporter.step_end(label, elapsed)
                    parts.extend(new_parts)

            elif isinstance(area, Boundary) and area.layers:
                for layer in area.layers:
                    label = f"{area.id}: {type(layer).__name__}"
                    reporter.step_begin(label)
                    t0 = _time.perf_counter()
                    new_parts = layer.apply(scene, placement_mask=boundary_mask)
                    elapsed = _time.perf_counter() - t0
                    reporter.step_end(label, elapsed)
                    parts.extend(new_parts)

    reporter.step_begin("Terrain solid")
    t0 = _time.perf_counter()
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, surface.tile_w, surface.tile_h,
        surface.base_h,
        error_threshold=surface.terrain_simplify_threshold,
        simplify_stride=surface.terrain_simplify_stride,
    )
    elapsed = _time.perf_counter() - t0
    reporter.step_end("Terrain solid", elapsed,
                      f"{len(terrain_mesh.vertices):,} verts · "
                      f"{len(terrain_mesh.faces):,} faces")
    parts.insert(0, terrain_mesh)

    solid_parts = [p for p in parts if p.is_volume]
    union_label = f"Boolean union  ({len(solid_parts)}/{len(parts)} solid)"
    reporter.step_begin(union_label)
    t0 = _time.perf_counter()
    if len(solid_parts) == 0:
        combined = trimesh.util.concatenate(parts)
    elif len(solid_parts) == 1:
        combined = solid_parts[0]
    else:
        combined = trimesh.boolean.union(solid_parts, engine='manifold')
    elapsed = _time.perf_counter() - t0
    wt_label = "watertight" if combined.is_watertight else "NOT watertight"
    reporter.step_end(union_label, elapsed,
                      f"{wt_label}  "
                      f"{len(combined.vertices):,} verts · "
                      f"{len(combined.faces):,} faces")

    return combined, scene


# ── Public build API ──────────────────────────────────────────────────────────

def build_tile_from_spec(
    tile:         Tile,
    *,
    system_paths: dict[str, pathlib.Path],
    reporter:     TileReporter,
) -> trimesh.Trimesh:
    """Build a tile from a Python ``Tile`` spec and export one STL per system.

    *system_paths* maps each system's ``suffix`` to its output path.
    The orchestrator loops over ``tile.systems`` in declaration order,
    builds a mesh at the system's scale (caching equal-scale meshes), and
    calls ``system.export()``.

    Returns the mesh from the first system (typically DungeonBlocks).
    """
    surface = tile.surface

    region_ids   = [r.id for r in tile.regions]
    boundary_ids = [b.id for b in tile.boundaries]

    # ── Region mask (once, at primary scale) ─────────────────────────────────
    if tile.areas:
        reporter.step_begin("Region mask")
        t0 = _time.perf_counter()
        region_mask = build_region_mask(tile)
        reporter.step_end("Region mask", _time.perf_counter() - t0)
    else:
        region_mask = None

    # ── Build each system's mesh (cache equal-scale results) ─────────────────
    built: dict[float, tuple[trimesh.Trimesh, TileScene]] = {}
    first_result: trimesh.Trimesh | None = None

    for system in tile.systems:
        sys_surface = system.surface_for(surface)
        sq_mm       = sys_surface.square_mm

        if sq_mm not in built:
            reporter.rebuild_begin(system.suffix, sq_mm)

            sys_tile = dataclasses.replace(tile, surface=sys_surface)
            mesh, scene = _build_tile_mesh(
                sys_tile, sys_surface, region_mask, reporter,
            )
            built[sq_mm] = (mesh, scene)

        tile_mesh, scene = built[sq_mm]
        out_path = system_paths[system.suffix]

        reporter.step_begin(f"Export {system.suffix}")
        t0 = _time.perf_counter()
        result = system.export(tile_mesh, sys_surface, scene.terrain_z, out_path)
        elapsed = _time.perf_counter() - t0

        reporter.export_done(
            suffix     = system.suffix,
            path       = out_path,
            n_verts    = len(result.vertices),
            n_faces    = len(result.faces),
            watertight = result.is_watertight,
            elapsed    = elapsed,
        )

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
    reporter:   TileReporter,
) -> None:
    """Build one tile spec at the size declared on its surface config."""
    sys_paths = _system_paths_for(spec_path, tile, tiles_root, stl_root)
    build_tile_from_spec(tile, system_paths=sys_paths, reporter=reporter)


# ── Closing quote ─────────────────────────────────────────────────────────────

def _print_closing_quote() -> None:
    """Pick a random Buddhist quote from the bundled data file and print it."""
    import json
    import random
    from pathlib import Path

    try:
        data_path = Path(__file__).parent.parent / "assets" / "quotes.json"
        quotes = json.loads(data_path.read_text(encoding="utf-8"))
        entry = random.choice(quotes)
        quote_text = entry["q"]
        source = entry["s"]
    except Exception:
        return

    try:
        from rich.console import Console
        from rich.rule import Rule
        console = Console(highlight=False)
        console.print()
        console.print(Rule(style="dim"))
        console.print(f"  [italic]{quote_text}[/italic]")
        console.print(f"  [dim]{source}[/dim]")
        console.print(Rule(style="dim"))
        console.print()
    except ImportError:
        print(f"\n{'─'*60}\n{quote_text}\n{source}\n{'─'*60}\n")


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate terrain tile STLs from .tile.py spec files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help=".tile.py Python spec.  Omit to process all src/tiles/")
    p.add_argument("--quiet", "-q", action="store_true",
                   help="Suppress all output.")
    return p


def main(argv=None):
    args     = _build_parser().parse_args(argv)
    reporter = make_reporter(quiet=args.quiet)

    if not args.quiet:
        from ..logo_render import render_header
        render_header()

    TILES_ROOT = pathlib.Path("src/tiles")
    STL_ROOT   = pathlib.Path("stl")

    # ── Single spec ───────────────────────────────────────────────────────────
    if args.spec is not None:
        specs = list(load_spec(args.spec))
        for tile in specs:
            surface = tile.surface
            name    = args.spec.stem.replace('.tile', '')
            reporter.tile_begin(
                name         = name,
                cols         = surface.cols,
                rows         = surface.rows,
                grid_w       = surface.grid_w,
                grid_h       = surface.grid_h,
                region_ids   = [r.id for r in tile.regions],
                boundary_ids = [b.id for b in tile.boundaries],
            )
            t0 = _time.perf_counter()
            _build_spec(tile, args.spec, TILES_ROOT, STL_ROOT, reporter)
            reporter.tile_end(_time.perf_counter() - t0)
        if not args.quiet:
            _print_closing_quote()
        return

    # ── Batch ─────────────────────────────────────────────────────────────────
    spec_paths = sorted(TILES_ROOT.rglob("*.tile.py"))
    if not spec_paths:
        print(f"No .tile.py files found under {TILES_ROOT}/  "
              f"(pass --spec FILE to target a specific tile)")
        return

    reporter.batch_begin(len(spec_paths))
    t_batch = _time.perf_counter()

    for sp in spec_paths:
        spec_name = sp.stem.replace('.tile', '')
        reporter.batch_spec_begin(spec_name)
        t_spec = _time.perf_counter()

        for tile in load_spec(sp):
            surface = tile.surface
            reporter.tile_begin(
                name         = spec_name,
                cols         = surface.cols,
                rows         = surface.rows,
                grid_w       = surface.grid_w,
                grid_h       = surface.grid_h,
                region_ids   = [r.id for r in tile.regions],
                boundary_ids = [b.id for b in tile.boundaries],
            )
            t0 = _time.perf_counter()
            _build_spec(tile, sp, TILES_ROOT, STL_ROOT, reporter)
            reporter.tile_end(_time.perf_counter() - t0)

        reporter.batch_spec_done(spec_name, _time.perf_counter() - t_spec)

    reporter.batch_end(len(spec_paths), _time.perf_counter() - t_batch)
    if not args.quiet:
        _print_closing_quote()


if __name__ == "__main__":
    main()
