"""
Tile generator: assemble a Tile spec into STL output(s).

Usage
─────
    generate-tile-stl
        Batch mode: process every ``*.tile.py`` file under ``src/tiles/`` and
        write outputs to ``stl/dungeonblocks/`` and ``stl/openlock/``.

    generate-tile-stl --spec "src/tiles/soil+grass.tile.py"
        Single tile: same naming and directory conventions as batch.

    generate-tile-stl --spec src/tiles/foo.tile.py -o stl/custom.stl
        Single tile, explicit output path.
"""
from __future__ import annotations

import argparse
import dataclasses
import pathlib
import time as _time

import numpy as np
import trimesh
from scipy.ndimage import distance_transform_edt

from ..core.config import (SceneConfig, SurfaceConfig,
                           SoilConfig, RocksConfig, BaseConfig)
from ..core.tile import TileScene
from ..core.mesh import make_heightmap_solid
from ..core.region import build_region_mask
from ..spec import Tile, load_spec
from ..bases import dungeonblocks, openlock


# ── Orchestrator ──────────────────────────────────────────────────────────────

def _build_tile_mesh(tile: Tile,
                     scene_cfg: SceneConfig,
                     region_mask: np.ndarray | None,
                     verbose: bool = True) -> tuple[trimesh.Trimesh, TileScene]:
    """Run all spec layers in spec order; return (final mesh, scene)."""
    terrain_z = _build_spec_terrain(tile, scene_cfg.surface, region_mask)

    scene = TileScene(
        config            = scene_cfg,
        terrain_z         = terrain_z,
        terrain_support_z = terrain_z.copy(),
        rock_mask         = np.zeros(
            (scene_cfg.surface.grid_h, scene_cfg.surface.grid_w), dtype=bool),
    )

    parts: list[trimesh.Trimesh] = []

    if region_mask is None:
        # No regions declared: nothing to do.
        pass
    else:
        for idx, region in enumerate(tile.regions):
            mask = (region_mask == idx)
            for layer in region.layers:
                parts.extend(layer.apply(scene, placement_mask=mask))

        boundary_mask = (region_mask < 0)
        for boundary in tile.boundaries:
            if not boundary.layers:
                continue
            for layer in boundary.layers:
                parts.extend(layer.apply(scene, placement_mask=boundary_mask))

    if verbose:
        print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(
        scene.terrain_z, scene_cfg.surface.tile_w, scene_cfg.surface.tile_h,
        scene_cfg.surface.base_h,
        error_threshold=scene_cfg.surface.terrain_simplify_threshold,
        simplify_stride=scene_cfg.surface.terrain_simplify_stride,
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


# ── Public API ────────────────────────────────────────────────────────────────


def _new_tile_paths(spec_path: pathlib.Path,
                    cols: int, rows: int,
                    tiles_root: pathlib.Path,
                    stl_root: pathlib.Path) -> dict[str, pathlib.Path]:
    """Return ``{system: path}`` for the canonical output hierarchy."""
    try:
        no_py   = spec_path.with_suffix('')
        no_tile = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
        rel     = no_tile.relative_to(tiles_root)
    except ValueError:
        no_py   = pathlib.Path(spec_path.stem)
        rel     = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
    stem   = f"{cols}x{rows}-{rel.name}"
    subdir = rel.parent
    return {
        dungeonblocks.SYSTEM_SUFFIX: stl_root / 'dungeonblocks' / subdir / f"{stem}-db.stl",
        openlock.SYSTEM_SUFFIX:      stl_root / 'openlock'      / subdir / f"{stem}-ol.stl",
    }


def _make_ol_surface(surface: SurfaceConfig) -> SurfaceConfig:
    """Return a copy of *surface* scaled to OpenLOCK 25.4 mm per square."""
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
    """Export one STL per base system from a base-less tile mesh."""
    if cfg.base.style == 'none':
        output_path.parent.mkdir(parents=True, exist_ok=True)
        tile_mesh.export(str(output_path))
        if verbose:
            print(f"Saved -> {output_path}")
        return {'none': tile_mesh}

    if system_paths is None:
        stem = output_path.stem
        if stem.endswith('-db'):
            system_paths = {dungeonblocks.SYSTEM_SUFFIX: output_path}
        elif stem.endswith('-ol'):
            system_paths = {openlock.SYSTEM_SUFFIX: output_path}
        else:
            system_paths = {
                dungeonblocks.SYSTEM_SUFFIX: output_path.with_name(
                    f"{stem}-db{output_path.suffix}"),
                openlock.SYSTEM_SUFFIX: output_path.with_name(
                    f"{stem}-ol{output_path.suffix}"),
            }

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


def build_tile_from_spec(tile: Tile,
                         output_path: pathlib.Path,
                         verbose: bool = True,
                         *,
                         system_paths: dict[str, pathlib.Path] | None = None,
                         ) -> trimesh.Trimesh:
    """Build a tile from a Python ``Tile`` spec and export system STLs."""
    cfg = _scene_config_from_spec(tile)

    if verbose:
        region_ids = [r.id for r in tile.regions]
        bnd_ids    = [b.id for b in tile.boundaries]
        print(f"=== Building tile from spec "
              f"({cfg.surface.cols}×{cfg.surface.rows} squares, "
              f"grid {cfg.surface.grid_w}×{cfg.surface.grid_h}) ===")
        if region_ids:
            print(f"  Regions:    {region_ids}")
        if bnd_ids:
            print(f"  Boundaries: {bnd_ids}")

    region_mask = None
    if tile.regions or tile.boundaries:
        region_mask = build_region_mask(tile)

    db_tile_mesh, db_scene = _build_tile_mesh(tile, cfg, region_mask, verbose=verbose)

    # ── OpenLOCK: rebuild natively at 25.4 mm/square ─────────────────────────
    ol_tile_mesh: trimesh.Trimesh | None = None
    ol_surface:   SurfaceConfig   | None = None
    ol_terrain_z: np.ndarray      | None = None
    if cfg.base.style != 'none':
        if verbose:
            print(f"\n=== Rebuilding scene at OpenLOCK scale "
                  f"({openlock.OPENLOCK_SQUARE_MM} mm/sq) ===")
        ol_surface = _make_ol_surface(cfg.surface)
        ol_cfg     = dataclasses.replace(cfg, surface=ol_surface)
        ol_tile_mesh, ol_scene = _build_tile_mesh(
            tile, ol_cfg, region_mask, verbose=verbose)
        ol_terrain_z = ol_scene.terrain_z

    exports = _export_system_stls(db_tile_mesh, cfg, db_scene.terrain_z,
                                  output_path, verbose=verbose,
                                  ol_tile_mesh=ol_tile_mesh,
                                  ol_surface=ol_surface,
                                  ol_terrain_z=ol_terrain_z,
                                  system_paths=system_paths)
    return exports.get(dungeonblocks.SYSTEM_SUFFIX, db_tile_mesh)


# ── Spec → terrain helpers ────────────────────────────────────────────────────

def _build_spec_terrain(tile: Tile, surface: SurfaceConfig,
                         region_mask: np.ndarray | None) -> np.ndarray:
    """Derive a terrain heightmap from region heights declared in *tile*.

    Algorithm
    ---------
    1. Assign each region cell its exact ``effective_height_mm``.
    2. Boundary cells get an inverse-distance-weighted (IDW) blend of
       neighbouring region heights.
    """
    gh, gw = surface.grid_h, surface.grid_w
    default_h = 5.0

    if not tile.regions or region_mask is None:
        return np.full((gh, gw), default_h, dtype=float)

    heights = [r.effective_height_mm for r in tile.regions]

    if len(set(heights)) <= 1:
        return np.full((gh, gw), heights[0] if heights else default_h, dtype=float)

    z_exact = np.full((gh, gw), default_h, dtype=float)
    for idx, h in enumerate(heights):
        z_exact[region_mask == idx] = h

    z_idw = np.zeros((gh, gw), dtype=float)
    w_sum = np.zeros((gh, gw), dtype=float)
    for idx, h in enumerate(heights):
        dist = distance_transform_edt(region_mask != idx)
        w    = 1.0 / (dist + 0.5)
        z_idw += h * w
        w_sum += w
    z_idw /= np.maximum(w_sum, 1e-12)

    z = z_exact.copy()
    z[region_mask < 0] = z_idw[region_mask < 0]
    return z.astype(float)


# ── Spec → config helpers ─────────────────────────────────────────────────────

def _scene_config_from_spec(tile: Tile) -> SceneConfig:
    """Build a SceneConfig from a Tile spec using defaults for unspecified layers."""
    return SceneConfig(
        surface          = tile.surface,
        soil             = SoilConfig(),
        rocks            = RocksConfig(),
        base             = BaseConfig(),
        max_stack_height = 2.0,
    )


# ── Spec → build helper ───────────────────────────────────────────────────────

def _build_spec(tile: Tile,
                spec_path: pathlib.Path,
                output: pathlib.Path | None,
                tiles_root: pathlib.Path,
                stl_root: pathlib.Path,
                verbose: bool = True) -> None:
    """Build one tile spec at the size declared on its surface config."""
    cols, rows = tile.surface.cols, tile.surface.rows
    if output is None:
        sys_paths = _new_tile_paths(spec_path, cols, rows, tiles_root, stl_root)
        out = sys_paths[dungeonblocks.SYSTEM_SUFFIX]
    else:
        sys_paths = None
        out = output
    build_tile_from_spec(tile, output_path=out, verbose=verbose,
                         system_paths=sys_paths)


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate terrain tile STLs from .tile.py spec files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--spec", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help=".tile.py Python spec.  Omit to process all src/tiles/")
    p.add_argument("--output", "-o", type=pathlib.Path,
                   default=None,
                   help="Override output path (requires --spec).")
    p.add_argument("--quiet", "-q", action="store_true")
    return p


def main(argv=None):
    args    = _build_parser().parse_args(argv)
    verbose = not args.quiet

    TILES_ROOT = pathlib.Path("src/tiles")
    STL_ROOT   = pathlib.Path("stl")

    if args.spec is not None:
        tile = load_spec(args.spec)
        _build_spec(tile, args.spec, args.output,
                    TILES_ROOT, STL_ROOT, verbose=verbose)
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
        tile = load_spec(sp)
        _build_spec(tile, sp, None, TILES_ROOT, STL_ROOT, verbose=verbose)
    elapsed = _time.perf_counter() - t_batch
    n = len(specs)
    print(f"\n{n} spec{'s' if n != 1 else ''} processed in {elapsed:.1f}s  "
          f"({elapsed/n:.1f}s/spec)")


if __name__ == "__main__":
    main()
