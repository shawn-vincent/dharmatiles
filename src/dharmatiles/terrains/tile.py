"""
Tile generator: assemble a Tile spec into STL output(s).

Usage
─────
    dharmatiles-gen
        Batch mode: process every ``*.tile.py`` file under ``src/tiles/`` and
        write outputs to ``stl/{system_dir}/…``.

    dharmatiles-gen --tile "src/tiles/soil+grass.tile.py"
        Single tile: same naming and directory conventions as batch.

    dharmatiles-gen --quiet
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

from ..core.color import Material, tag as _tag
from ..core.config import SurfaceConfig
from ..core.tile import TileScene
from ..core.mesh import make_heightmap_solid
from ..core.region import build_region_mask
from ..spec import Tile, Region, Boundary, load_tile
from .reporter import TileReporter, RichReporter, make_reporter


# ── Parallel batch helpers ────────────────────────────────────────────────────

class _CollectingReporter(TileReporter):
    """Silent reporter that accumulates export results for the parent process."""

    def __init__(self, phase_cb=None) -> None:
        self._phase_cb  = phase_cb
        self._cols: int = 1
        self._rows: int = 1
        self._outputs: list[dict] = []

    def tile_begin(self, name, cols, rows, grid_w, grid_h,
                   region_ids, boundary_ids) -> None:
        self._cols = cols
        self._rows = rows

    def step_begin(self, label: str) -> None:
        if self._phase_cb is not None:
            phase = label.split(': ', 1)[-1] if ': ' in label else label
            self._phase_cb(phase[:28])

    def rebuild_begin(self, suffix: str, square_mm: float) -> None:
        if self._phase_cb is not None:
            self._phase_cb(f"building {suffix}")

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
    """Build one tile spec (STL + PNG) in a worker process; return a summary-row dict.

    This function must be at module level so ProcessPoolExecutor can pickle it.
    The reporter is created inside the worker (not passed) to avoid pickling issues.
    """
    # Suppress ALL output in worker processes so nothing written to the terminal
    # can displace the cursor and corrupt the parent's Rich Live panel.
    #
    # Two layers of suppression are needed:
    #   1. OS-level fd redirect (catches C/C++ extensions and macOS system
    #      messages, e.g. "ApplePersistenceIgnoreState", which are written
    #      directly to fd 1/2 and bypass Python's sys.stdout/stderr).
    #   2. Python-level redirect (catches print() calls in tile specs or libs).
    #
    # The fd redirect must happen before pyrender/pyglet/AppKit initialises —
    # that's when macOS emits the per-process ApplePersistenceIgnoreState
    # message that was the root cause of the stacked-panel bug.
    import warnings as _warnings, os as _os, sys as _sys
    _warnings.filterwarnings('ignore')
    # ── OS fd level ──────────────────────────────────────────────────────────
    _null_fd = _os.open(_os.devnull, _os.O_WRONLY)
    _os.dup2(_null_fd, 1)   # stdout (fd 1)
    _os.dup2(_null_fd, 2)   # stderr (fd 2)
    _os.close(_null_fd)
    # ── Python stream level ───────────────────────────────────────────────────
    _devnull = open(_os.devnull, 'w')
    _sys.stdout = _devnull
    _sys.stderr = _devnull

    tile_path_str, tiles_root_str, stl_root_str, png_root_str, phase_dict, formats = args

    tile_path  = pathlib.Path(tile_path_str)
    tiles_root = pathlib.Path(tiles_root_str)
    stl_root   = pathlib.Path(stl_root_str)
    png_root   = pathlib.Path(png_root_str)

    tile_name = tile_path.stem.replace('.tile', '')

    def _set_phase(phase: str) -> None:
        if phase_dict is not None:
            try:
                phase_dict[tile_name] = phase
            except Exception:
                pass

    reporter  = _CollectingReporter(phase_cb=_set_phase)
    t0        = _time.perf_counter()

    render_meshes: list[trimesh.Trimesh] | None = None
    dir_to_meshes_all: dict[pathlib.Path, list[trimesh.Trimesh]] = {}
    for tile in load_tile(tile_path):
        _filter_tile_systems(tile, formats)
        surface   = tile.surface
        sys_paths = _system_paths_for(tile_path, tile, tiles_root, stl_root)
        reporter.tile_begin(
            name         = tile_name,
            cols         = surface.cols,
            rows         = surface.rows,
            grid_w       = surface.grid_w,
            grid_h       = surface.grid_h,
            region_ids   = [r.id for r in tile.regions],
            boundary_ids = [b.id for b in tile.boundaries],
        )
        _, render_meshes, dir_to_meshes = build_tile_from_spec(
            tile, system_paths=sys_paths, reporter=reporter,
        )
        dir_to_meshes_all.update(dir_to_meshes)

    # Render PNG from the already-built meshes (no second build pass).
    _set_phase("Render PNG")
    if render_meshes is not None:
        for tile in load_tile(tile_path):
            out = _png_path_for(tile_path, tile, tiles_root, png_root)
            _render_from_meshes(render_meshes, out, tile.surface.square_mm, quiet=True,
                                label=_label_for_png(out, png_root))
            break
    else:
        try:
            from ..render import render as _render
            for tile in load_tile(tile_path):
                out = _png_path_for(tile_path, tile, tiles_root, png_root)
                out.parent.mkdir(parents=True, exist_ok=True)
                meshes = build_meshes_for_render(tile_path)
                _render(meshes, out, quiet=True, grid_square_mm=tile.surface.square_mm,
                        label=_label_for_png(out, png_root))
                break
        except Exception:
            pass

    # Pre-format 3MF XML while still in the worker — runs in parallel with
    # other tiles building.  The main process assembly step then only needs
    # to concatenate strings and write the ZIP.
    #
    # face_xml_cache: db and ol meshes share identical face topology (same
    # connectivity, only vertex positions differ by scale).  By passing one
    # cache dict across all system directories, the second call (ol) reuses
    # face XML from the first call (db) — saving ~half the XML serialisation
    # time for tiles with both scales.
    _set_phase("3MF XML")
    from ..core.export_3mf import tile_xml_parts as _tile_xml_parts
    dir_3mf_parts: dict[str, tuple[str, dict]] = {}
    _face_xml_cache: dict = {}
    for d, ms in dir_to_meshes_all.items():
        expanded = _expand_mixed_materials(ms)
        dir_3mf_parts[str(d)] = (tile_name, _tile_xml_parts(expanded, face_xml_cache=_face_xml_cache))

    row = reporter.to_row(tile_name, _time.perf_counter() - t0)
    row['dir_3mf_parts'] = dir_3mf_parts
    return row


# ── Terrain face-colour helper ────────────────────────────────────────────────

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

    Each layer may declare ``terrain_material: ClassVar[Material]`` to
    indicate which material it paints the terrain surface.  The first layer
    in a region with such an attribute wins; layers without it are skipped.
    """
    from ..core.color import RGBA

    if region_mask is None or not regions:
        _tag(terrain_mesh, Material.SOIL)
        return

    # Determine the terrain-surface material for each region index.
    # The FIRST layer with a terrain_material ClassVar wins; others default to SOIL.
    region_mat: list[Material] = []
    for region in regions:
        mat = Material.SOIL       # default: bare dirt
        for layer in region.layers:
            layer_mat = getattr(type(layer), 'terrain_material', None)
            if layer_mat is not None:
                mat = layer_mat
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

    with np.errstate(invalid='ignore', divide='ignore'):
        both_solid = soil_mesh.is_volume and water_mesh.is_volume
    if not both_solid:
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
    with np.errstate(invalid='ignore', divide='ignore'):
        carved.fix_normals()
    # Re-apply the water material tag + face colours; the boolean op strips
    # all visual attributes from the trimesh result.
    _tag(carved, Material.WATER)

    result            = list(colored_meshes)
    result[water_idx] = carved
    return result


# ── Content pipeline ──────────────────────────────────────────────────────────
# Runs all spec layers, builds the terrain solid, groups meshes by material.
# Produces TileContent — no base geometry, no file I/O.

@dataclasses.dataclass
class TileContent:
    """Output of the content pipeline; input to the export pipeline.

    The content pipeline (``_build_tile_content``) runs all spec layers,
    builds the terrain solid, and groups meshes by material.  It knows
    nothing about base geometry or file output.

    The export pipeline (``system.export()``) receives the meshes and scene
    from this object, attaches the system-specific base, and writes the STL.
    """
    colored_meshes: list  # list[trimesh.Trimesh]
    scene: TileScene


def _build_tile_content(
    tile:        Tile,
    surface:     SurfaceConfig,
    region_mask: np.ndarray | None,
    reporter:    TileReporter,
) -> TileContent:
    """Run all spec layers in ``tile.areas`` order; return a ``TileContent``.

    Returns a :class:`TileContent` whose ``colored_meshes`` are grouped by
    :class:`~dharmatiles.core.color.Material` — one mesh per material that
    appears in the tile, each with uniform ``face_colors`` from ``core/color.py``.

    *surface* may differ from ``tile.surface`` when a system builds at a
    different scale (e.g. OpenLOCK at 25.4 mm/sq).  *region_mask* is
    always the one computed at the primary (spec) surface and is reused
    across scales — fractional positions are scale-invariant.
    """
    terrain_z, water_surface_mm = _build_spec_terrain(tile, surface, region_mask)

    scene = TileScene(
        surface          = surface,
        terrain_z        = terrain_z,
        terrain_support_z= terrain_z.copy(),
        obstacle_mask    = np.zeros((surface.grid_h, surface.grid_w), dtype=bool),
        region_mask      = region_mask,
        water_surface_mm = water_surface_mm,
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
    from ..core.color import DEBUG_COLORS as _DEBUG_COLORS
    # Materials that are concatenated rather than boolean-unioned.
    # GRASS: blades are numerous and thin — unioning them is wasteful.
    # FLOWER + DEBUG_COLOR_*: attractor debug spheres — unioning ~200 small
    # icospheres is very slow; concatenation is correct and fast.
    # WOOD: each tree is already internally unioned into one closed wood mesh.
    _NO_UNION = {Material.GRASS, Material.FLOWER, Material.WOOD, Material.FOLIAGE} | set(_DEBUG_COLORS)

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
            # Fast path: just concatenate (grass blades, attractor spheres, etc.)
            group_mesh = (trimesh.util.concatenate(mesh_list)
                          if len(mesh_list) > 1 else mesh_list[0])
        else:
            with np.errstate(invalid='ignore', divide='ignore'):
                solid = [m for m in mesh_list if m.is_volume]
            solid_ids = {id(m) for m in solid}
            nonsolid = [m for m in mesh_list if id(m) not in solid_ids]
            if len(solid) > 1:
                solid_mesh = trimesh.boolean.union(
                    solid, engine='manifold', check_volume=False,
                )
                group_mesh = (trimesh.util.concatenate([solid_mesh] + nonsolid)
                              if nonsolid else solid_mesh)
            elif solid:
                group_mesh = (trimesh.util.concatenate(solid + nonsolid)
                              if nonsolid else solid[0])
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

    return TileContent(colored_meshes=colored_meshes, scene=scene)


# ── Public build API ──────────────────────────────────────────────────────────

def build_tile_from_spec(
    tile:         Tile,
    *,
    system_paths: dict[str, pathlib.Path],
    reporter:     TileReporter,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh] | None, dict[pathlib.Path, list[trimesh.Trimesh]]]:
    """Build a tile from a Python ``Tile`` spec and export one STL per system.

    Returns ``(main_mesh, render_meshes, dir_to_meshes)`` where:

    * *render_meshes* — ``[base] + colored_meshes`` from the first system,
      used for PNG rendering without a rebuild pass.
    * *dir_to_meshes* — ``{output_dir: all_meshes}`` per system, used by the
      caller to accumulate per-directory 3MF data across tiles.
    """
    surface = tile.surface

    region_ids   = [r.id for r in tile.regions]
    boundary_ids = [b.id for b in tile.boundaries]

    # ── Region mask (computed per distinct grid size) ─────────────────────────
    region_masks: dict[tuple[int, int], np.ndarray | None] = {}

    def _get_region_mask(surf: SurfaceConfig) -> np.ndarray | None:
        key = (surf.grid_h, surf.grid_w)
        if key not in region_masks:
            if tile.areas:
                sys_tile_for_mask = dataclasses.replace(tile, surface=surf)
                region_masks[key] = build_region_mask(sys_tile_for_mask)
            else:
                region_masks[key] = None
        return region_masks[key]

    # Pre-build primary (DB) region mask and report it once.
    if tile.areas:
        reporter.step_begin("Region mask")
        t0 = _time.perf_counter()
        _ = _get_region_mask(surface)
        reporter.step_end("Region mask", _time.perf_counter() - t0)

    # ── Content pipeline (cached per distinct scale) ──────────────────────────
    # Runs all layers, builds terrain solid, groups by material.
    # Produces TileContent — no base geometry, no file I/O.
    built: dict[tuple[float, int, int], TileContent] = {}
    first_result: trimesh.Trimesh | None = None
    first_render_meshes: list[trimesh.Trimesh] | None = None
    dir_to_meshes: dict[pathlib.Path, list[trimesh.Trimesh]] = {}

    for system in tile.systems:
        sys_surface = system.surface_for(surface)
        cache_key   = (sys_surface.square_mm, sys_surface.grid_h, sys_surface.grid_w)

        if cache_key not in built:
            reporter.rebuild_begin(system.suffix, sys_surface.square_mm)

            region_mask = _get_region_mask(sys_surface)
            sys_tile = dataclasses.replace(tile, surface=sys_surface)
            built[cache_key] = _build_tile_content(
                sys_tile, sys_surface, region_mask, reporter,
            )

        # ── Export pipeline ───────────────────────────────────────────────────
        # Attaches system-specific base, writes STL.
        content  = built[cache_key]
        out_path = system_paths[system.suffix]

        reporter.step_begin(f"Export {system.suffix}")
        t0 = _time.perf_counter()
        result, rend_meshes = system.export(
            content.colored_meshes, sys_surface, content.scene.terrain_z, out_path,
        )
        elapsed = _time.perf_counter() - t0

        # Avoid the O(F log F) edge-sort that `is_watertight` triggers on the
        # combined mesh (which may have millions of faces).  The combined STL
        # is a concatenation of many shells and will not be watertight anyway.
        reporter.export_done(
            suffix     = system.suffix,
            path       = out_path,
            n_verts    = len(result.vertices),
            n_faces    = len(result.faces),
            watertight = None,
            elapsed    = elapsed,
        )

        dir_to_meshes[out_path.parent] = rend_meshes

        if first_result is None:
            first_result = result
            first_render_meshes = rend_meshes

    return first_result or trimesh.Trimesh(), first_render_meshes, dir_to_meshes


def build_meshes_for_render(
    tile_path: pathlib.Path,
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
    from ..spec import load_tile
    from ..systems import DungeonBlocks, OpenLOCK
    from ..bases import dungeonblocks as _db
    from ..core.color import Material, tag as _tag
    from ..core.config import BaseConfig

    tiles = load_tile(tile_path)
    tile  = tiles[0]

    target = next(
        (s for s in tile.systems if s.suffix == system),
        tile.systems[0] if tile.systems else DungeonBlocks(),
    )
    surface = target.surface_for(tile.surface)

    region_mask = build_region_mask(tile) if tile.areas else None

    # ── Content pipeline ──────────────────────────────────────────────────────
    sys_tile = _dc.replace(tile, surface=surface)
    content = _build_tile_content(sys_tile, surface, region_mask, TileReporter())

    # ── Export pipeline (no file I/O — base attach only) ─────────────────────
    base_cfg = BaseConfig()
    if isinstance(target, DungeonBlocks) and target.peg_height is not None:
        base_cfg = _dc.replace(base_cfg, peg_height=target.peg_height)

    tile_max_z = max(
        (float(m.bounds[1, 2]) for m in content.colored_meshes if len(m.vertices) > 0),
        default=0.0,
    )
    peg_h     = _db.select_peg_height(content.scene.terrain_z, base_cfg, tile_max_z=tile_max_z)
    base_mesh = _db.make_base(surface, peg_h, base_cfg)
    _tag(base_mesh, Material.BASE)

    return [base_mesh] + content.colored_meshes


# ── Spec → terrain helper ─────────────────────────────────────────────────────

def _build_spec_terrain(
    tile:        Tile,
    surface:     SurfaceConfig,
    region_mask: np.ndarray | None,
) -> tuple[np.ndarray, dict[int, float]]:
    """Derive a terrain heightmap from region heights declared in *tile*.

    Returns ``(terrain_z, water_surface_mm)`` where *water_surface_mm* maps
    region index → water surface height (mm) for every region that contains
    a ``WaterLayer``.

    Algorithm
    ---------
    1. For water regions, the IDW contribution is 0 mm (pool floor), not the
       region's ``height_mm`` (which is the water *surface* level).  This
       confines the shore slope to the boundary strip: the boundary cells blend
       smoothly from 0 mm (pool floor) up to the neighbouring land height, so
       the visible waterline sits near the boundary path centreline rather than
       being pushed into the pool interior.
    2. Assign each non-water region cell its exact ``height_mm``; water
       region cells get 0 mm (pool floor).
    3. Boundary cells get an inverse-distance-weighted (IDW) blend of the above
       floor heights.
    """
    from ..layers.water import Water as _WaterLayer  # local import to avoid circularity

    gh, gw    = surface.grid_h, surface.grid_w
    default_h = 5.0

    if not tile.regions or region_mask is None:
        return np.full((gh, gw), default_h, dtype=float), {}

    # Collect per-region heights; water regions use 0 mm floor for IDW.
    water_surface_mm: dict[int, float] = {}
    idw_heights: list[float] = []
    for idx, region in enumerate(tile.regions):
        h = region.height_mm
        has_water = any(isinstance(layer, _WaterLayer) for layer in region.layers)
        if has_water:
            water_surface_mm[idx] = h  # save surface level for WaterLayer to consume
            idw_heights.append(0.0)    # pool floor drives IDW and z_exact
        else:
            idw_heights.append(h)

    if len(set(idw_heights)) <= 1:
        return np.full((gh, gw), idw_heights[0] if idw_heights else default_h, dtype=float), water_surface_mm

    z_exact = np.full((gh, gw), default_h, dtype=float)
    for idx, h in enumerate(idw_heights):
        z_exact[region_mask == idx] = h

    z_idw = np.zeros((gh, gw), dtype=float)
    w_sum = np.zeros((gh, gw), dtype=float)
    for idx, h in enumerate(idw_heights):
        dist  = distance_transform_edt(region_mask != idx)
        w     = 1.0 / (dist + 0.5)
        z_idw += h * w
        w_sum += w
    z_idw /= np.maximum(w_sum, 1e-12)

    z = z_exact.copy()
    z[region_mask < 0] = z_idw[region_mask < 0]
    return z.astype(float), water_surface_mm


# ── CLI helpers ───────────────────────────────────────────────────────────────

def _tile_stem(tile_path: pathlib.Path, tiles_root: pathlib.Path) -> tuple[str, pathlib.Path]:
    """Return (stem, subdir) for a tile path relative to tiles_root.

    stem is the tile filename without .tile.py (e.g. '1x1-water+grass').
    subdir is the path component between tiles_root and the file (e.g. PosixPath('water')).
    """
    try:
        no_py   = tile_path.with_suffix('')
        no_tile = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
        rel     = no_tile.relative_to(tiles_root)
    except ValueError:
        no_py   = pathlib.Path(tile_path.stem)
        rel     = no_py.with_suffix('') if no_py.suffix == '.tile' else no_py
    return rel.name, rel.parent


def _png_path_for(
    tile_path:  pathlib.Path,
    tile:       Tile,
    tiles_root: pathlib.Path,
    png_root:   pathlib.Path,
) -> pathlib.Path:
    """Return the canonical PNG output path for *tile_path* (no system suffix)."""
    stem, sub = _tile_stem(tile_path, tiles_root)
    return png_root / sub / f"{stem}.png"


def _label_for_png(out: pathlib.Path, png_root: pathlib.Path) -> str:
    """Derive the display label from a PNG path (e.g. 'water/1x1-water+grass')."""
    try:
        return str(out.relative_to(png_root).with_suffix('')).replace('\\', '/')
    except ValueError:
        parent = out.parent.name
        if parent and parent not in ('tmp', ''):
            return f"{parent}/{out.stem}"
        return out.stem


def _render_from_meshes(
    meshes:    list[trimesh.Trimesh],
    out:       pathlib.Path,
    square_mm: float,
    quiet:     bool,
    label:     str | None = None,
) -> None:
    """Render one PNG from already-built meshes — no tile rebuild."""
    try:
        from ..render import render as _render
        out.parent.mkdir(parents=True, exist_ok=True)
        _render(meshes, out, quiet=quiet, grid_square_mm=square_mm, label=label)
    except ImportError:
        if not quiet:
            print("pyrender not available — skipping PNG step")
    except Exception:
        pass


def _meshes_to_serial(meshes: list[trimesh.Trimesh]) -> list[dict]:
    """Serialise trimesh meshes to plain numpy-array dicts for cross-process IPC."""
    serial = []
    for m in meshes:
        try:
            fc: np.ndarray | None = np.asarray(m.visual.face_colors, dtype=np.uint8)
            if fc.shape != (len(m.faces), 4):
                fc = None
        except Exception:
            fc = None
        serial.append({
            'verts':    m.vertices.astype(np.float32),
            'faces':    m.faces.astype(np.int32),
            'colors':   fc,
            'material': m.metadata.get('material'),
        })
    return serial


def _serial_to_meshes(serial: list[dict]) -> list[trimesh.Trimesh]:
    """Reconstruct trimesh meshes from serialised numpy-array dicts."""
    meshes = []
    for d in serial:
        m = trimesh.Trimesh(
            vertices=d['verts'].astype(float),
            faces=d['faces'],
            process=False,
        )
        if d['colors'] is not None:
            m.visual = trimesh.visual.ColorVisuals(mesh=m, face_colors=d['colors'])
        if d['material'] is not None:
            m.metadata['material'] = d['material']
        meshes.append(m)
    return meshes


def _expand_mixed_materials(
    meshes: list[trimesh.Trimesh],
) -> list[trimesh.Trimesh]:
    """Split any mesh whose faces carry mixed material colours into per-material parts.

    The terrain solid is tagged ``Material.SOIL`` in its metadata but holds
    per-face RGBA colours assigned by ``_color_terrain_faces()`` — grass-carpet
    region faces are green, soil faces are brown, etc.  PNG rendering reads
    those face colours directly and looks correct; the 3MF exporter reads
    ``metadata['material']`` for filament-slot assignment and therefore paints
    the whole terrain brown.

    This function detects meshes whose face colours span more than one entry in
    the ``RGBA`` palette, splits them on face-colour boundaries, and tags each
    part with the correct ``Material`` so the 3MF exporter assigns the right
    extruder to every region.  Single-colour and untagged meshes pass through
    unchanged.
    """
    from ..core.color import RGBA, Material

    # Reverse palette: (R, G, B) → Material  (ignores alpha so alpha=255 variants match too)
    _rgb_to_mat: dict[tuple[int, int, int], Material] = {
        tuple(rgba[:3]): mat  # type: ignore[misc]
        for mat, rgba in RGBA.items()
    }

    result: list[trimesh.Trimesh] = []
    for mesh in meshes:
        try:
            fc = mesh.visual.face_colors  # (F, 4) uint8
        except Exception:
            result.append(mesh)
            continue

        if len(fc) == 0:
            result.append(mesh)
            continue

        fc3 = fc[:, :3]  # (F, 3) — drop alpha for comparison

        # Fast path: check first vs last row before committing to an O(F log F)
        # np.unique sort.  Most meshes (tree, grass, base, water) carry a uniform
        # single colour applied by tag(); the terrain solid is the only one with
        # mixed grass/soil colours.  A single np.any(...) scan exits early for
        # the uniform case.
        if np.array_equal(fc3[0], fc3[-1]) and not np.any(fc3 != fc3[0]):
            result.append(mesh)
            continue

        unique_colors = np.unique(fc3, axis=0)

        if len(unique_colors) <= 1:
            result.append(mesh)
            continue

        # Multiple colours → split and retag each part.
        for color in unique_colors:
            key = tuple(int(c) for c in color)
            mat = _rgb_to_mat.get(key, Material.SOIL)  # type: ignore[arg-type]
            face_idx = np.where(np.all(fc3 == color, axis=1))[0]
            sub = mesh.submesh([face_idx], append=True)
            n = len(sub.faces)
            sub.visual = trimesh.visual.ColorVisuals(
                mesh=sub,
                face_colors=np.tile(RGBA[mat], (n, 1)).astype(np.uint8),
            )
            sub.metadata['material'] = mat
            result.append(sub)

    return result


def _write_dir_3mf(
    dir_path: pathlib.Path,
    tiles:    list[tuple[str, list[trimesh.Trimesh]]],
) -> None:
    """Write one ``<dir>.3mf`` from trimesh objects (single-tile / sequential path).

    *tiles* is a list of ``(spec_name, meshes)`` pairs.  Mixed-material terrain
    meshes are split by :func:`_expand_mixed_materials` before export so Bambu
    Studio assigns the correct filament slot to each region.

    For the parallel batch path use :func:`_write_dir_3mf_from_parts` instead —
    it receives XML already formatted in worker processes and skips the expensive
    float→string step here in the main process.
    """
    from ..core.export_3mf import export_3mf_colored
    if not tiles:
        return
    names       = [name for name, _ in tiles]
    meshes_list = [_expand_mixed_materials(ms) for _, ms in tiles]
    dir_path.mkdir(parents=True, exist_ok=True)
    export_3mf_colored(meshes_list, dir_path / f"{dir_path.name}.3mf", names=names)


def _write_dir_3mf_from_parts(
    dir_path:        pathlib.Path,
    tile_parts_list: list[tuple[str, dict]],
) -> None:
    """Write one ``<dir>.3mf`` from pre-formatted XML parts (parallel batch path).

    *tile_parts_list* is a list of ``(name, xml_parts_dict)`` pairs where
    *xml_parts_dict* is the return value of
    :func:`~dharmatiles.core.export_3mf.tile_xml_parts`.  All vertex/triangle
    XML was already generated in the tile-build worker processes; this function
    only concatenates strings and writes the ZIP archive.
    """
    from ..core.export_3mf import export_3mf_from_parts
    if not tile_parts_list:
        return
    dir_path.mkdir(parents=True, exist_ok=True)
    export_3mf_from_parts(tile_parts_list, dir_path / f"{dir_path.name}.3mf")


def _render_all_pngs(
    spec_paths: list[pathlib.Path],
    tiles_root: pathlib.Path,
    png_root:   pathlib.Path,
    quiet:      bool = False,
) -> None:
    """Render a PNG thumbnail for each spec in *spec_paths* into *png_root*.

    Fallback used when pre-built render meshes are unavailable.
    """
    try:
        from ..render import render as _render
    except ImportError:
        if not quiet:
            print("pyrender not available — skipping PNG step")
        return

    for sp in spec_paths:
        tiles = load_tile(sp)
        if not tiles:
            continue
        tile = tiles[0]
        out  = _png_path_for(sp, tile, tiles_root, png_root)
        out.parent.mkdir(parents=True, exist_ok=True)
        meshes = build_meshes_for_render(sp)
        _render(meshes, out, quiet=quiet, grid_square_mm=tile.surface.square_mm,
                label=_label_for_png(out, png_root))


def _collect_png_sections(png_root: pathlib.Path) -> list[tuple[str, list[pathlib.Path]]]:
    """Return immediate PNG subdirectories as catalog sections."""
    if not png_root.exists():
        return []
    sections: list[tuple[str, list[pathlib.Path]]] = []
    for child in sorted(p for p in png_root.iterdir() if p.is_dir()):
        images = sorted(child.glob("*.png"))
        if images:
            sections.append((child.name, images))
    return sections


def _write_tile_catalog_pdf(
    png_root: pathlib.Path,
    out:      pathlib.Path,
    quiet:    bool = False,
) -> None:
    """Write a Letter-sized PDF catalog from PNG thumbnails under *png_root*.

    Each immediate subdirectory becomes a section.  Sections start on fresh
    pages; pages use a fixed two-column grid and as many rows as fit.
    """
    sections = _collect_png_sections(png_root)
    if not sections:
        return

    try:
        from PIL import Image, ImageDraw, ImageOps
        from ..render import _load_label_font, _load_label_font_bold
    except ImportError:
        if not quiet:
            print("Pillow not available — skipping PDF catalog")
        return

    # Render at 3× (216 DPI) so tiles appear crisp; divide all layout constants
    # by SCALE to get the same physical dimensions on a Letter page.
    SCALE          = 3
    DPI            = 72 * SCALE
    page_w, page_h = 612 * SCALE, 792 * SCALE   # Letter at DPI
    margin         = 36  * SCALE
    title_h        = 44  * SCALE
    gap            = 16  * SCALE
    cols           = 2
    rows           = 3
    cell_w         = (page_w - 2 * margin - gap) // cols
    grid_top       = margin + title_h + 14 * SCALE
    cell_h         = (page_h - grid_top - margin - (rows - 1) * gap) // rows

    title_font = _load_label_font_bold(24 * SCALE)
    page_font  = _load_label_font(9  * SCALE)
    pages: list[Image.Image] = []

    def _new_page(section: str, page_num: int, total_pages: int) -> Image.Image:
        # RGBA so alpha-masked tile paste composites cleanly onto white.
        page = Image.new("RGBA", (page_w, page_h), (255, 255, 255, 255))
        draw = ImageDraw.Draw(page)
        draw.text((margin, margin), section.title(), fill=(24, 24, 24), font=title_font)
        if total_pages > 1:
            text = f"{page_num}/{total_pages}"
            bbox = draw.textbbox((0, 0), text, font=page_font)
            draw.text((page_w - margin - (bbox[2] - bbox[0]), margin + 9 * SCALE),
                      text, fill=(100, 100, 100), font=page_font)
        draw.line((margin, margin + title_h, page_w - margin, margin + title_h),
                  fill=(220, 220, 220), width=1)
        return page

    per_page = cols * rows
    for section, image_paths in sections:
        total_pages = (len(image_paths) + per_page - 1) // per_page
        for section_page_idx in range(total_pages):
            chunk = image_paths[
                section_page_idx * per_page:(section_page_idx + 1) * per_page
            ]
            page = _new_page(section, section_page_idx + 1, total_pages)
            for idx, image_path in enumerate(chunk):
                row, col = divmod(idx, cols)
                x = margin + col * (cell_w + gap)
                y = grid_top + row * (cell_h + gap)
                try:
                    with Image.open(image_path) as img:
                        # Keep RGBA so rounded-corner alpha is preserved.
                        thumb = ImageOps.contain(img.convert("RGBA"), (cell_w, cell_h))
                except Exception:
                    continue
                px = x + (cell_w - thumb.width) // 2
                py = y + (cell_h - thumb.height) // 2
                # Use the image's own alpha as the paste mask so transparent
                # corners reveal the white page background (rounded look).
                page.paste(thumb, (px, py), mask=thumb)
            pages.append(page)

    out.parent.mkdir(parents=True, exist_ok=True)
    # Convert to RGB for PDF output (white BG already composited above).
    rgb_pages = [p.convert("RGB") for p in pages]
    first, rest = rgb_pages[0], rgb_pages[1:]
    first.save(str(out), "PDF", resolution=float(DPI), save_all=True, append_images=rest)
    if not quiet:
        print(f"→ {out}")


def _filter_tile_systems(tile: Tile, formats) -> None:
    """Restrict ``tile.systems`` to the requested format suffixes (in place).

    *formats* is an iterable of system suffixes (e.g. ``["db", "ol"]``); a
    falsy value leaves the tile untouched (all systems).  Filtering preserves
    the tile's original system order and is a no-op if it would leave nothing.
    """
    if not formats:
        return
    wanted   = set(formats)
    filtered = [s for s in tile.systems if s.suffix in wanted]
    if filtered:
        tile.systems = filtered


def _system_paths_for(
    tile_path:  pathlib.Path,
    tile:       Tile,
    tiles_root: pathlib.Path,
    stl_root:   pathlib.Path,
) -> dict[str, pathlib.Path]:
    """Return ``{system.suffix: output_path}`` for the canonical hierarchy."""
    stem, sub = _tile_stem(tile_path, tiles_root)
    return {
        system.suffix: stl_root / system.dir_name / sub / f"{stem}-{system.suffix}.stl"
        for system in tile.systems
    }


def _build_tile(
    tile:       Tile,
    tile_path:  pathlib.Path,
    tiles_root: pathlib.Path,
    stl_root:   pathlib.Path,
    reporter:   TileReporter,
) -> tuple[list[trimesh.Trimesh] | None, dict[pathlib.Path, list[trimesh.Trimesh]]]:
    """Build one tile; returns (render_meshes, dir_to_meshes)."""
    sys_paths = _system_paths_for(tile_path, tile, tiles_root, stl_root)
    _, render_meshes, dir_to_meshes = build_tile_from_spec(
        tile, system_paths=sys_paths, reporter=reporter,
    )
    return render_meshes, dir_to_meshes


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
    phase_dict:     object = None,
    n_workers:      int = 4,
    phase_label:    str   = "",
    t_phase:        float = 0.0,
) -> None:
    """Drive ``future_to_name`` futures with a Rich Live fixed-height display.

    The panel is always exactly ``n_workers + 1`` lines tall when
    ``phase_label`` is set (the default in batch mode):
      - 1 phase-spinner line: ``⠹ Building N tiles  7.4s  ·  M queued``
      - n_workers slot lines: one per worker; empty padding lines fill unused slots

    Fixed height means Rich always knows how many lines to erase on each
    refresh, which prevents ghost rows when the active-tile count changes.
    """
    from rich.live    import Live
    from rich.console import Group
    from rich.text    import Text

    _FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    _FPS    = 12.5

    done_rows:   dict[str, dict] = {}
    pending_set: set             = set(future_to_name.keys())

    def _render() -> Group:
        now   = _time.perf_counter()
        frame = _FRAMES[int(now * _FPS) % len(_FRAMES)]

        running: list[str] = []
        n_queued = 0
        for name in spec_names:
            if name in done_rows:
                continue
            try:
                in_phase = phase_dict is not None and bool(phase_dict.get(name))
            except Exception:
                in_phase = False
            if in_phase:
                t_starts.setdefault(name, now)  # record start on first appearance
                running.append(name)
            else:
                n_queued += 1

        n_done_count = len(done_rows)
        lines: list[Text] = []

        # ── Phase spinner (top-level, flush-left) ────────────────────────────
        if phase_label:
            n_total    = len(spec_names)
            dyn_label  = (
                f"Building {n_done_count}/{n_total} tiles"
                if n_done_count > 0 else phase_label
            )
            ph_elapsed = now - t_phase
            ph_t_str   = f"{ph_elapsed:.1f}s" if ph_elapsed >= 0.05 else "  …"
            queued_str = f"  [dim]·  {n_queued} queued[/dim]" if n_queued else ""
            lines.append(Text.from_markup(
                f"[cyan]{frame}[/cyan] [bold]{dyn_label}[/bold]"
                f"  [cyan]{ph_t_str}[/cyan]"
                f"{queued_str}"
            ))

        # ── Worker slots: always exactly n_workers lines ──────────────────────
        for i in range(n_workers):
            if i < len(running):
                name    = running[i]
                elapsed = now - t_starts[name]
                t_str   = f"{elapsed:.1f}s" if elapsed >= 0.05 else "   …"
                phase   = ""
                if phase_dict is not None:
                    try:
                        phase = phase_dict.get(name, "")
                    except Exception:
                        pass
                phase_str = f"  [dim]{phase}[/dim]" if phase else ""
                lines.append(Text.from_markup(
                    f"  [cyan]{frame}[/cyan] {name:<38} [cyan]{t_str}[/]{phase_str}"
                ))
            else:
                lines.append(Text(" "))  # non-empty blank — keeps height constant
                # Text("") can render as 0 segments in some Rich versions and be
                # miscounted as 0 lines; a single space is always 1 line.

        return Group(*lines)

    console = reporter._console  # type: ignore[attr-defined]

    # Use get_renderable= so Rich's auto-refresh thread calls _render() on every
    # tick — the spinner frame advances smoothly without manual live.update() calls.
    # auto_refresh=True (default) starts the background refresh thread; this is the
    # correct pattern for Rich 15 and avoids the "copies scrolling" bug that
    # auto_refresh=False + refresh=True suffers from.
    with Live(
        get_renderable=_render,
        console=console,
        refresh_per_second=_FPS,
        transient=True,   # erase panel on exit so phase_end() prints cleanly below
    ) as live:  # noqa: F841 — live var unused; context manager handles lifecycle
        while pending_set:
            newly_done, pending_set = _futures_wait(
                pending_set, timeout=1.0 / _FPS,
                return_when=FIRST_COMPLETED,
            )
            for f in newly_done:
                row  = f.result()
                name = future_to_name[f]
                done_rows[name] = row

    # Populate the reporter's batch-table data (no more printing).
    # Emit in spec_names order so the summary table is alphabetically stable.
    reporter.record_batch_rows([done_rows[n] for n in spec_names if n in done_rows])


# ── CLI ───────────────────────────────────────────────────────────────────────

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate terrain tile STLs from .tile.py files.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--tile", "-s", type=pathlib.Path, default=None,
                   metavar="FILE",
                   help=".tile.py tile file.  Omit to process all src/tiles/")
    p.add_argument("--formats", "-f", nargs="+", choices=["db", "ol"], default=None,
                   metavar="FMT",
                   help="Base system(s) to generate: db (DungeonBlocks) and/or ol "
                        "(OpenLOCK).  Default: every system the tile defines.")
    p.add_argument("--png-only", action="store_true",
                   help="Re-render PNG thumbnails and PDF catalog only; skip STL generation.")
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
    PNG_ROOT   = pathlib.Path("png")
    CATALOG_PDF = STL_ROOT / "tile-catalog.pdf"

    # ── PNG-only (skip all STL work) ──────────────────────────────────────────
    if args.png_only:
        tile_paths = (
            [args.tile] if args.tile is not None
            else sorted(TILES_ROOT.rglob("*.tile.py"))
        )
        if not tile_paths:
            print(f"No .tile.py files found under {TILES_ROOT}/")
            return
        n_png = len(tile_paths)
        t_png = _time.perf_counter()
        reporter.phase_begin(
            f"Rendering {n_png} PNG{'s' if n_png != 1 else ''}"
        )
        _render_all_pngs(tile_paths, TILES_ROOT, PNG_ROOT, quiet=args.quiet)
        reporter.phase_end(
            f"Render PNG{'s' if n_png != 1 else ''}",
            _time.perf_counter() - t_png,
        )
        t_cat = _time.perf_counter()
        reporter.phase_begin("Writing catalog")
        _write_tile_catalog_pdf(PNG_ROOT, CATALOG_PDF, quiet=args.quiet)
        reporter.phase_end("Write catalog", _time.perf_counter() - t_cat)
        if not args.quiet:
            _print_closing_quote()
        return

    # ── Single tile ───────────────────────────────────────────────────────────
    if args.tile is not None:
        from collections import defaultdict as _defaultdict
        tiles_list = list(load_tile(args.tile))
        render_meshes: list[trimesh.Trimesh] | None = None
        dir_3mf_accum: dict[pathlib.Path, list[tuple[str, list[trimesh.Trimesh]]]] = _defaultdict(list)
        for tile in tiles_list:
            _filter_tile_systems(tile, args.formats)
            surface = tile.surface
            name    = args.tile.stem.replace('.tile', '')
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
            render_meshes, dir_to_meshes = _build_tile(
                tile, args.tile, TILES_ROOT, STL_ROOT, reporter,
            )
            reporter.tile_end(_time.perf_counter() - t0)
            for d, ms in dir_to_meshes.items():
                dir_3mf_accum[d].append((name, ms))
        if dir_3mf_accum:
            t_3mf = _time.perf_counter()
            n_3mf = len(dir_3mf_accum)
            reporter.phase_begin("Assembling 3MF")
            for d, mls in dir_3mf_accum.items():
                _write_dir_3mf(d, mls)
            reporter.phase_end(
                "Assemble 3MF", _time.perf_counter() - t_3mf,
                f"{n_3mf} file{'s' if n_3mf != 1 else ''}",
            )
        t_png = _time.perf_counter()
        reporter.phase_begin("Render PNG")
        if render_meshes is not None and tiles_list:
            out = _png_path_for(args.tile, tiles_list[0], TILES_ROOT, PNG_ROOT)
            _render_from_meshes(render_meshes, out, tiles_list[0].surface.square_mm, args.quiet,
                                label=_label_for_png(out, PNG_ROOT))
        else:
            _render_all_pngs([args.tile], TILES_ROOT, PNG_ROOT, quiet=args.quiet)
        reporter.phase_end("Render PNG", _time.perf_counter() - t_png)
        t_cat = _time.perf_counter()
        reporter.phase_begin("Write catalog")
        _write_tile_catalog_pdf(PNG_ROOT, CATALOG_PDF, quiet=args.quiet)
        reporter.phase_end("Write catalog", _time.perf_counter() - t_cat)
        if not args.quiet:
            _print_closing_quote()
        return

    # ── Batch ─────────────────────────────────────────────────────────────────
    tile_paths = sorted(TILES_ROOT.rglob("*.tile.py"))
    if not tile_paths:
        print(f"No .tile.py files found under {TILES_ROOT}/  "
              f"(pass --tile FILE to target a specific tile)")
        return

    # Number of parallel workers: half the logical CPU count, capped at 4.
    # We cap at 4 because manifold3d already uses internal threads, so running
    # more than 4 tiles simultaneously tends to thrash rather than help.
    n_workers = min(4, len(tile_paths),
                    max(1, multiprocessing.cpu_count() // 2))

    reporter.batch_begin(len(tile_paths))
    t_batch = _time.perf_counter()

    pngs_rendered = False
    from collections import defaultdict as _defaultdict

    n_tiles = len(tile_paths)
    tile_word = f"tile{'s' if n_tiles != 1 else ''}"

    # ── Phase: Building tiles ─────────────────────────────────────────────────
    t_build      = _time.perf_counter()
    _build_label = f"Building {n_tiles} {tile_word}"

    if n_workers > 1:
        # ── Parallel path: each tile in its own worker process ──────────────
        tile_names   = [tp.stem.replace('.tile', '') for tp in tile_paths]
        t_submit     = _time.perf_counter()
        all_par_rows: list[dict] = []   # collected for 3MF assembly

        with multiprocessing.Manager() as mgr:
            phase_dict = mgr.dict()
            worker_args = [
                (str(tp), str(TILES_ROOT), str(STL_ROOT), str(PNG_ROOT), phase_dict, args.formats)
                for tp in tile_paths
            ]

            with ProcessPoolExecutor(max_workers=n_workers) as executor:
                future_to_name = {
                    executor.submit(_batch_worker, wa): tp.stem.replace('.tile', '')
                    for tp, wa in zip(tile_paths, worker_args)
                }
                t_starts = {}  # populated lazily in _render() on first activity

                if isinstance(reporter, RichReporter):
                    # ── Rich Live display: phase spinner embedded in panel ────────
                    _run_parallel_live(
                        tile_names, future_to_name, t_starts, reporter,
                        phase_dict=phase_dict,
                        n_workers=n_workers,
                        phase_label=_build_label,
                        t_phase=t_build,
                    )
                    all_par_rows = [f.result() for f in future_to_name]
                else:
                    # ── Plain fallback (pipe / --quiet / no rich) ────────────────
                    reporter.phase_header(_build_label)
                    for future in as_completed(future_to_name):
                        row = future.result()
                        all_par_rows.append(row)
                        reporter.inject_batch_row(row)

        reporter.phase_end(f"Build {n_tiles} {tile_word}", _time.perf_counter() - t_build)
        pngs_rendered = True

        # ── Phase: Assemble 3MF (one per system output dir, all tiles) ───────
        # XML is already formatted by workers — just concatenate and ZIP.
        dir_3mf_accum: dict[pathlib.Path, list[tuple[str, dict]]] = _defaultdict(list)
        for row in all_par_rows:
            for d_str, (name, xml_parts) in row.get('dir_3mf_parts', {}).items():
                dir_3mf_accum[pathlib.Path(d_str)].append((name, xml_parts))
        if dir_3mf_accum:
            t_3mf = _time.perf_counter()
            n_3mf = len(dir_3mf_accum)
            reporter.phase_begin("Assembling 3MF")
            for d, tile_parts_list in dir_3mf_accum.items():
                _write_dir_3mf_from_parts(d, tile_parts_list)
            reporter.phase_end(
                "Assemble 3MF", _time.perf_counter() - t_3mf,
                f"{n_3mf} file{'s' if n_3mf != 1 else ''}",
            )
    else:
        # ── Sequential fallback (single-core or --quiet) ─────────────────────
        reporter.phase_header(_build_label)
        dir_3mf_accum_seq: dict[pathlib.Path, list[tuple[str, list[trimesh.Trimesh]]]] = _defaultdict(list)

        for tp in tile_paths:
            tile_name = tp.stem.replace('.tile', '')
            reporter.batch_tile_begin(tile_name)
            t_tile = _time.perf_counter()

            sq_mm_seq: float | None = None
            rend_seq:  list[trimesh.Trimesh] | None = None
            for tile in load_tile(tp):
                _filter_tile_systems(tile, args.formats)
                surface = tile.surface
                sq_mm_seq = surface.square_mm
                reporter.tile_begin(
                    name         = tile_name,
                    cols         = surface.cols,
                    rows         = surface.rows,
                    grid_w       = surface.grid_w,
                    grid_h       = surface.grid_h,
                    region_ids   = [r.id for r in tile.regions],
                    boundary_ids = [b.id for b in tile.boundaries],
                )
                t0 = _time.perf_counter()
                rend_seq, dir_to_meshes = _build_tile(
                    tile, tp, TILES_ROOT, STL_ROOT, reporter,
                )
                reporter.tile_end(_time.perf_counter() - t0)
                for d, ms in dir_to_meshes.items():
                    dir_3mf_accum_seq[d].append((tile_name, ms))

            reporter.batch_tile_done(tile_name, _time.perf_counter() - t_tile)

            # Render PNG immediately — no second build pass.
            if rend_seq is not None and sq_mm_seq is not None:
                for tile in load_tile(tp):
                    out = _png_path_for(tp, tile, TILES_ROOT, PNG_ROOT)
                    _render_from_meshes(rend_seq, out, sq_mm_seq, args.quiet,
                                        label=_label_for_png(out, PNG_ROOT))
                    break

        reporter.phase_end(f"Build {n_tiles} {tile_word}", _time.perf_counter() - t_build)
        pngs_rendered = True  # rendered inline above

        # ── Phase: Assemble 3MF ───────────────────────────────────────────────
        if dir_3mf_accum_seq:
            t_3mf = _time.perf_counter()
            n_3mf = len(dir_3mf_accum_seq)
            reporter.phase_begin("Assembling 3MF")
            for d, mls in dir_3mf_accum_seq.items():
                _write_dir_3mf(d, mls)
            reporter.phase_end(
                "Assemble 3MF", _time.perf_counter() - t_3mf,
                f"{n_3mf} file{'s' if n_3mf != 1 else ''}",
            )

    reporter.batch_end(n_tiles, _time.perf_counter() - t_batch)
    if not pngs_rendered:
        _render_all_pngs(tile_paths, TILES_ROOT, PNG_ROOT, quiet=args.quiet)

    # ── Phase: Write catalog ──────────────────────────────────────────────────
    t_cat = _time.perf_counter()
    reporter.phase_begin("Writing catalog")
    _write_tile_catalog_pdf(PNG_ROOT, CATALOG_PDF, quiet=args.quiet)
    reporter.phase_end("Write catalog", _time.perf_counter() - t_cat)

    if not args.quiet:
        _print_closing_quote()


if __name__ == "__main__":
    main()
