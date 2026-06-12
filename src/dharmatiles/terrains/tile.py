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
import multiprocessing
import pathlib
import time as _time
from concurrent.futures import (
    ProcessPoolExecutor, as_completed,
    wait as _futures_wait, FIRST_COMPLETED,
)

import numpy as np
import trimesh
from scipy.ndimage import distance_transform_edt

from ..core.color import Material, tag as _tag, build_scene
from ..core.config import SurfaceConfig
from ..core.tile import TileScene
from ..core.mesh import make_heightmap_solid
from ..core.region import build_region_mask
from ..spec import Tile, Region, Boundary, load_spec
from .reporter import TileReporter, RichReporter, make_reporter


# ── Parallel batch helpers ────────────────────────────────────────────────────

class _CollectingReporter(TileReporter):
    """Silent reporter that accumulates export results for the parent process."""

    def __init__(self) -> None:
        self._cols: int = 1
        self._rows: int = 1
        self._outputs: list[dict] = []

    def tile_begin(self, name, cols, rows, grid_w, grid_h,
                   region_ids, boundary_ids) -> None:
        self._cols = cols
        self._rows = rows

    def export_done(self, suffix, path, n_verts, n_faces,
                    watertight, elapsed) -> None:
        self._outputs.append(dict(
            suffix=suffix, path=path,
            n_verts=n_verts, n_faces=n_faces,
            watertight=watertight,
        ))

    def to_row(self, name: str, elapsed: float) -> dict:
        return dict(
            name=name, elapsed=elapsed,
            cols=self._cols, rows=self._rows,
            outputs=list(self._outputs),
        )


def _batch_worker(args: tuple) -> dict:
    """Build one tile spec in a worker process; return a summary-row dict.

    This function must be at module level so ProcessPoolExecutor can pickle it.
    The reporter is created inside the worker (not passed) to avoid pickling issues.
    """
    spec_path_str, tiles_root_str, stl_root_str = args

    spec_path  = pathlib.Path(spec_path_str)
    tiles_root = pathlib.Path(tiles_root_str)
    stl_root   = pathlib.Path(stl_root_str)

    reporter  = _CollectingReporter()
    spec_name = spec_path.stem.replace('.tile', '')
    t0        = _time.perf_counter()

    for tile in load_spec(spec_path):
        surface   = tile.surface
        sys_paths = _system_paths_for(spec_path, tile, tiles_root, stl_root)
        reporter.tile_begin(
            name         = spec_name,
            cols         = surface.cols,
            rows         = surface.rows,
            grid_w       = surface.grid_w,
            grid_h       = surface.grid_h,
            region_ids   = [r.id for r in tile.regions],
            boundary_ids = [b.id for b in tile.boundaries],
        )
        build_tile_from_spec(tile, system_paths=sys_paths, reporter=reporter)

    return reporter.to_row(spec_name, _time.perf_counter() - t0)


# ── Terrain face-colour helper ────────────────────────────────────────────────

# Layer class-name → terrain-surface material.
# Uses type-name strings to avoid cross-package imports; we control all names.
# Layers not listed here default to SOIL (bare dirt).
_LAYER_TERRAIN_MATERIAL: dict[str, "Material"] = {
    'GrassCarpet':      Material.GRASS,
    'GrassCarpetLayer': Material.GRASS,
    'SoilCarpet':       Material.SOIL,
    'SoilCarpetLayer':  Material.SOIL,
    'Water':            Material.WATER,
    'WaterLayer':       Material.WATER,
}


def _color_terrain_faces(
    terrain_mesh: trimesh.Trimesh,
    region_mask:  np.ndarray | None,
    regions:      list,           # list[Region]
    surface:      "SurfaceConfig",
) -> None:
    """Apply per-face colours to *terrain_mesh* based on region layer content.

    Faces whose centroid falls inside a grass-carpet region become
    ``Material.GRASS`` (yellow-green); soil-carpet regions become
    ``Material.SOIL`` (reddish-brown); water regions become ``Material.WATER``
    (turquoise).  Side and bottom faces of the slab are coloured by XY
    centroid — they are hidden under the base in practice.

    Modifies *terrain_mesh.visual* in-place; does not change geometry.
    Falls back to uniform SOIL when *region_mask* is None.
    """
    from ..core.color import RGBA

    if region_mask is None or not regions:
        _tag(terrain_mesh, Material.SOIL)
        return

    # Determine the terrain-surface material for each region index.
    # The FIRST recognised layer in a region wins; Scatter has no override.
    region_mat: list[Material] = []
    for region in regions:
        mat = Material.SOIL       # default: bare dirt
        for layer in region.layers:
            override = _LAYER_TERRAIN_MATERIAL.get(type(layer).__name__)
            if override is not None:
                mat = override
                break
        region_mat.append(mat)

    # Per-face centroid → grid cell → region index → colour.
    centroids = terrain_mesh.triangles_center          # (F, 3)
    gh, gw    = region_mask.shape

    gi = np.clip((centroids[:, 0] / surface.tile_w * gw).astype(int), 0, gw - 1)
    gj = np.clip((centroids[:, 1] / surface.tile_h * gh).astype(int), 0, gh - 1)
    ridx = region_mask[gj, gi]          # (F,) — -1=boundary, 0..N=region

    # Default all faces to SOIL.
    n           = len(terrain_mesh.faces)
    face_colors = np.empty((n, 4), dtype=np.uint8)
    face_colors[:] = RGBA[Material.SOIL]

    # Override for each region whose material differs from SOIL.
    for r_idx, mat in enumerate(region_mat):
        if mat != Material.SOIL:
            mask = (ridx == r_idx)
            if mask.any():
                face_colors[mask] = RGBA[mat]

    terrain_mesh.visual = trimesh.visual.ColorVisuals(
        mesh=terrain_mesh,
        face_colors=face_colors,
    )
    terrain_mesh.metadata['material'] = Material.SOIL


# ── Water-from-soil Boolean subtraction ──────────────────────────────────────

def _carve_soil_from_water(
    colored_meshes: list[trimesh.Trimesh],
) -> list[trimesh.Trimesh]:
    """Subtract the soil solid from the full-tile water slab.

    The water layer emits a full-tile slab; subtracting the terrain (soil)
    solid from it naturally confines the water to whatever space the terrain
    does not occupy — i.e. the pool cavity — without needing an explicit
    pool-shaped cutter mesh.

    Returns a new list where the WATER mesh is replaced by the carved result.
    No-op when either SOIL or WATER is absent, or when either mesh is not
    watertight.
    """
    import manifold3d as m3d

    soil_idx  = next((i for i, m in enumerate(colored_meshes)
                      if m.metadata.get('material') == Material.SOIL), None)
    water_idx = next((i for i, m in enumerate(colored_meshes)
                      if m.metadata.get('material') == Material.WATER), None)

    if soil_idx is None or water_idx is None:
        return colored_meshes

    soil_mesh  = colored_meshes[soil_idx]
    water_mesh = colored_meshes[water_idx]

    if not soil_mesh.is_volume or not water_mesh.is_volume:
        return colored_meshes

    def _to_m3d(verts: np.ndarray, faces: np.ndarray) -> m3d.Manifold:
        return m3d.Manifold(mesh=m3d.Mesh(
            vert_properties=verts.astype('f4'),
            tri_verts=faces.astype('u4'),
        ))

    water_m  = _to_m3d(water_mesh.vertices, water_mesh.faces)
    soil_m   = _to_m3d(soil_mesh.vertices,  soil_mesh.faces)
    carved_m = water_m - soil_m

    msh    = carved_m.to_mesh()
    carved = trimesh.Trimesh(
        vertices=np.array(msh.vert_properties, dtype=float)[:, :3],
        faces=np.array(msh.tri_verts, dtype=int),
        process=False,
    )
    carved.fix_normals()
    # Re-apply the water material tag + face colours; the boolean op strips
    # all visual attributes from the trimesh result.
    _tag(carved, Material.WATER)

    result            = list(colored_meshes)
    result[water_idx] = carved
    return result


# ── Internal mesh builder ─────────────────────────────────────────────────────

def _build_tile_mesh(
    tile:        Tile,
    surface:     SurfaceConfig,
    region_mask: np.ndarray | None,
    reporter:    TileReporter,
) -> tuple[list[trimesh.Trimesh], TileScene]:
    """Run all spec layers in ``tile.areas`` order; return (colored_meshes, scene).

    Returns a list of meshes grouped by :class:`~dharmatiles.core.color.Material`
    — one mesh per material that appears in the tile.  Each mesh carries
    uniform ``face_colors`` matching the palette in ``core/color.py``.

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
    _color_terrain_faces(terrain_mesh, region_mask, tile.regions, surface)
    elapsed = _time.perf_counter() - t0
    reporter.step_end("Terrain solid", elapsed,
                      f"{len(terrain_mesh.vertices):,} verts · "
                      f"{len(terrain_mesh.faces):,} faces")
    parts.insert(0, terrain_mesh)

    # ── Per-material grouping ─────────────────────────────────────────────────
    # Group all parts by their material tag, then union solid parts within
    # each material group.  No cross-material boolean ops → face colours survive.
    # GRASS blades are concatenated rather than unioned (they can be numerous
    # and are thin enough that slicers handle overlaps correctly).
    from collections import defaultdict
    _NO_UNION = {Material.GRASS}   # materials where we concatenate, not union

    groups: dict[Material, list[trimesh.Trimesh]] = defaultdict(list)
    for p in parts:
        mat = p.metadata.get('material', Material.SOIL)
        groups[mat].append(p)

    group_label = f"Material grouping  ({len(groups)} group(s))"
    reporter.step_begin(group_label)
    t0 = _time.perf_counter()

    colored_meshes: list[trimesh.Trimesh] = []
    for mat in Material:          # deterministic order: TERRAIN ROCK GRASS WATER BASE
        mesh_list = groups.get(mat)
        if not mesh_list:
            continue

        if mat in _NO_UNION:
            # Fast path: just concatenate (grass blades, etc.)
            group_mesh = (trimesh.util.concatenate(mesh_list)
                          if len(mesh_list) > 1 else mesh_list[0])
        else:
            solid = [m for m in mesh_list if m.is_volume]
            if len(solid) > 1:
                group_mesh = trimesh.boolean.union(
                    solid, engine='manifold', check_volume=False,
                )
            elif solid:
                group_mesh = solid[0]
            else:
                group_mesh = (trimesh.util.concatenate(mesh_list)
                              if len(mesh_list) > 1 else mesh_list[0])

        if mat == Material.SOIL and len(mesh_list) == 1:
            # Terrain already carries per-face region colours from
            # _color_terrain_faces(); just ensure the metadata is set.
            group_mesh.metadata['material'] = Material.SOIL
        else:
            _tag(group_mesh, mat)   # (re-)apply after union strips attributes
        colored_meshes.append(group_mesh)

    elapsed = _time.perf_counter() - t0
    total_v = sum(len(m.vertices) for m in colored_meshes)
    total_f = sum(len(m.faces)    for m in colored_meshes)
    reporter.step_end(group_label, elapsed,
                      f"{total_v:,} verts · {total_f:,} faces total")

    # ── Carve soil from water (if any water present) ──────────────────────────
    has_water = any(m.metadata.get('material') == Material.WATER
                    for m in colored_meshes)
    if has_water:
        carve_label = "Carve soil from water"
        reporter.step_begin(carve_label)
        t0 = _time.perf_counter()
        colored_meshes = _carve_soil_from_water(colored_meshes)
        elapsed = _time.perf_counter() - t0
        water_m = next((m for m in colored_meshes
                        if m.metadata.get('material') == Material.WATER), None)
        wt = ("watertight" if water_m is not None and water_m.is_watertight
              else "NOT watertight")
        reporter.step_end(carve_label, elapsed, wt)

    return colored_meshes, scene


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
    built: dict[float, tuple[list[trimesh.Trimesh], TileScene]] = {}
    first_result: trimesh.Trimesh | None = None

    for system in tile.systems:
        sys_surface = system.surface_for(surface)
        sq_mm       = sys_surface.square_mm

        if sq_mm not in built:
            reporter.rebuild_begin(system.suffix, sq_mm)

            sys_tile = dataclasses.replace(tile, surface=sys_surface)
            colored_meshes, scene = _build_tile_mesh(
                sys_tile, sys_surface, region_mask, reporter,
            )
            built[sq_mm] = (colored_meshes, scene)

        colored_meshes, scene = built[sq_mm]
        out_path = system_paths[system.suffix]

        reporter.step_begin(f"Export {system.suffix}")
        t0 = _time.perf_counter()
        result = system.export(colored_meshes, sys_surface, scene.terrain_z, out_path)
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


def build_meshes_for_render(
    spec_path: pathlib.Path,
    *,
    system: str = "db",
) -> list[trimesh.Trimesh]:
    """Return coloured trimesh parts (including base) without writing any files.

    Intended for off-screen rendering tools.  *system* selects which base
    system to attach (``'db'`` for DungeonBlocks, ``'ol'`` for OpenLOCK).
    Returns a list of properly-indexed trimesh objects, each with ``face_colors``
    set from the material palette in ``core.color``.
    """
    import dataclasses as _dc
    from ..spec import load_spec
    from ..systems import DungeonBlocks, OpenLOCK
    from ..bases import dungeonblocks as _db
    from ..core.color import Material, tag as _tag
    from ..core.config import BaseConfig

    tiles = load_spec(spec_path)
    tile  = tiles[0]

    target = next(
        (s for s in tile.systems if s.suffix == system),
        tile.systems[0] if tile.systems else DungeonBlocks(),
    )
    surface = target.surface_for(tile.surface)

    region_mask = build_region_mask(tile) if tile.areas else None

    sys_tile = _dc.replace(tile, surface=surface)
    colored_meshes, scene = _build_tile_mesh(
        sys_tile, surface, region_mask, TileReporter()
    )

    # Attach the base (same logic as system.export, but no file I/O)
    base_cfg = BaseConfig()
    if isinstance(target, DungeonBlocks) and target.peg_height is not None:
        base_cfg = _dc.replace(base_cfg, peg_height=target.peg_height)

    peg_h     = _db.select_peg_height(scene.terrain_z, base_cfg)
    base_mesh = _db.make_base(surface, peg_h, base_cfg)
    _tag(base_mesh, Material.BASE)

    return [base_mesh] + colored_meshes


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


# ── Parallel live display ─────────────────────────────────────────────────────

def _run_parallel_live(
    spec_names:     list[str],
    future_to_name: dict,
    t_starts:       dict[str, float],
    reporter:       "RichReporter",
) -> None:
    """Drive ``future_to_name`` futures with a Rich Live multi-line display.

    Shows one line per tile:
    * ⠙  name   1.4s   — spinner + live clock while the worker is running
    * ✓  name   3.1s   — green tick + final time once the worker finishes

    When the Live context exits the lines stay on screen permanently.
    Completed rows are recorded into the reporter via ``record_batch_rows``.
    """
    from rich.live    import Live
    from rich.console import Group
    from rich.text    import Text

    _FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    _FPS    = 12.5

    done_rows:   dict[str, dict] = {}   # name → result row
    pending_set: set             = set(future_to_name.keys())

    def _render() -> Group:
        now   = _time.perf_counter()
        frame = _FRAMES[int(now * _FPS) % len(_FRAMES)]
        lines: list[Text] = []
        for name in spec_names:
            if name in done_rows:
                elapsed = done_rows[name]['elapsed']
                tc      = RichReporter._table_time_color(elapsed)
                lines.append(Text.from_markup(
                    f"  [green]✓[/green] {name:<38} [{tc}]{elapsed:.1f}s[/]"
                ))
            else:
                elapsed = now - t_starts[name]
                t_str   = f"{elapsed:.1f}s" if elapsed >= 0.05 else "   …"
                lines.append(Text.from_markup(
                    f"  [cyan]{frame}[/cyan] {name:<38} [cyan]{t_str}[/]"
                ))
        return Group(*lines)

    # Use the reporter's own console so Rich's internal state stays consistent.
    console = reporter._console  # type: ignore[attr-defined]

    with Live(_render(), console=console, refresh_per_second=_FPS) as live:
        while pending_set:
            newly_done, pending_set = _futures_wait(
                pending_set, timeout=1.0 / _FPS,
                return_when=FIRST_COMPLETED,
            )
            for f in newly_done:
                row  = f.result()
                name = future_to_name[f]
                done_rows[name] = row
            live.update(_render())

    # Populate the reporter's batch-table data (no more printing).
    # Emit in spec_names order so the summary table is alphabetically stable.
    reporter.record_batch_rows([done_rows[n] for n in spec_names if n in done_rows])


# ── 3MF pruning ──────────────────────────────────────────────────────────────

def _count_3mf_objects(path: pathlib.Path) -> int:
    """Return the number of ``<object>`` elements in a 3MF archive.

    Uses a fast byte-level count rather than full XML parsing.
    Returns 0 on any read error.
    """
    import zipfile
    try:
        with zipfile.ZipFile(path) as z:
            model = z.read('3D/3dmodel.model')
        return model.count(b'<object ')
    except Exception:
        return 0


def _keep_best_3mf_per_dir(stl_root: pathlib.Path) -> None:
    """Prune 3MF files under *stl_root* to one per directory.

    Within each directory that contains more than one 3MF, keep the file
    whose 3MF scene has the most ``<object>`` elements (most material
    groups).  Ties are broken by keeping the alphabetically-first path so
    the result is deterministic across runs.

    Called after every batch or single-spec generation so the ``stl/``
    tree always contains exactly one representative 3MF per output
    sub-directory.
    """
    from collections import defaultdict
    by_dir: dict[pathlib.Path, list[pathlib.Path]] = defaultdict(list)
    for p in stl_root.rglob('*.3mf'):
        by_dir[p.parent].append(p)

    for parent, files in by_dir.items():
        if len(files) <= 1:
            continue
        # Sort alphabetically for determinism, then rank by object count descending.
        files.sort()
        ranked = sorted(files, key=_count_3mf_objects, reverse=True)
        for loser in ranked[1:]:
            loser.unlink(missing_ok=True)


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
        _keep_best_3mf_per_dir(STL_ROOT)
        if not args.quiet:
            _print_closing_quote()
        return

    # ── Batch ─────────────────────────────────────────────────────────────────
    spec_paths = sorted(TILES_ROOT.rglob("*.tile.py"))
    if not spec_paths:
        print(f"No .tile.py files found under {TILES_ROOT}/  "
              f"(pass --spec FILE to target a specific tile)")
        return

    # Number of parallel workers: half the logical CPU count, capped at 4.
    # We cap at 4 because manifold3d already uses internal threads, so running
    # more than 4 tiles simultaneously tends to thrash rather than help.
    n_workers = min(4, len(spec_paths),
                    max(1, multiprocessing.cpu_count() // 2))

    reporter.batch_begin(len(spec_paths))
    t_batch = _time.perf_counter()

    if n_workers > 1:
        # ── Parallel path: each spec in its own worker process ──────────────
        worker_args = [
            (str(sp), str(TILES_ROOT), str(STL_ROOT))
            for sp in spec_paths
        ]
        spec_names = [sp.stem.replace('.tile', '') for sp in spec_paths]
        t_submit   = _time.perf_counter()

        with ProcessPoolExecutor(max_workers=n_workers) as executor:
            future_to_name = {
                executor.submit(_batch_worker, wa): sp.stem.replace('.tile', '')
                for sp, wa in zip(spec_paths, worker_args)
            }
            t_starts = {name: t_submit for name in spec_names}

            if isinstance(reporter, RichReporter):
                # ── Rich Live display: spinner + live clock per tile ─────────
                _run_parallel_live(
                    spec_names, future_to_name, t_starts, reporter,
                )
            else:
                # ── Plain fallback (pipe / --quiet / no rich) ────────────────
                collected: list[dict] = []
                for future in as_completed(future_to_name):
                    row = future.result()
                    collected.append(row)
                    reporter.inject_batch_row(row)
                # SilentReporter has a no-op record_batch_rows, TextReporter
                # already printed above; nothing more to do here.
    else:
        # ── Sequential fallback (single-core or --quiet) ─────────────────────
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
    _keep_best_3mf_per_dir(STL_ROOT)
    if not args.quiet:
        _print_closing_quote()


if __name__ == "__main__":
    main()
