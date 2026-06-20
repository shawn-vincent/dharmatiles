"""Profile the 3MF assembly pipeline.

Run from the repo root:
    python src/scripts/profile_3mf.py

Builds the two tree tiles (heaviest tiles), then profiles each phase of
the 3MF assembly so we know where the 36s goes.
"""
from __future__ import annotations

import cProfile
import io
import pathlib
import pstats
import sys
import time

# ── Repo bootstrap ────────────────────────────────────────────────────────────
ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "src"))

import numpy as np
import trimesh

from dharmatiles.spec import load_tile
from dharmatiles.terrains.tile import (
    build_tile_from_spec,
    _system_paths_for,
    _filter_tile_systems,
    _meshes_to_serial,
    _serial_to_meshes,
    _expand_mixed_materials,
    _write_dir_3mf,
)
from dharmatiles.terrains.reporter import TileReporter
from dharmatiles.core.export_3mf import (
    export_3mf_colored,
    tile_xml_parts,
    _vert_lines_xml,
    _face_lines_xml,
)

TILES_ROOT = ROOT / "src" / "tiles"
STL_ROOT   = ROOT / "stl"

# ── Choose which tiles to profile ─────────────────────────────────────────────
PROFILE_SPECS = [
    TILES_ROOT / "ground" / "1x1-grass-tree.tile.py",
    TILES_ROOT / "ground" / "2x2-grass-tree.tile.py",
    TILES_ROOT / "water"  / "1x1-grass-tree+water.tile.py",
]
# Filter to only existing files
PROFILE_SPECS = [p for p in PROFILE_SPECS if p.exists()]

def _fmt(t: float) -> str:
    return f"{t*1000:.1f} ms" if t < 1 else f"{t:.2f} s"

def _mesh_stats(meshes: list[trimesh.Trimesh]) -> str:
    verts  = sum(len(m.vertices) for m in meshes)
    faces  = sum(len(m.faces)    for m in meshes)
    return f"{len(meshes)} parts, {verts:,} verts, {faces:,} faces"

def build_tile_meshes(spec: pathlib.Path) -> tuple[str, list[trimesh.Trimesh]]:
    """Build a tile and return (name, db-meshes)."""
    print(f"\n{'='*60}")
    print(f"Building: {spec.name}")
    t0 = time.perf_counter()
    tile_name = spec.stem.replace('.tile', '')
    tile_meshes: list[trimesh.Trimesh] = []
    for tile in load_tile(spec):
        _filter_tile_systems(tile, {'db'})   # db only to stay fast
        sys_paths = _system_paths_for(spec, tile, TILES_ROOT, STL_ROOT)
        _, _, dir_to_meshes = build_tile_from_spec(tile, system_paths=sys_paths, reporter=TileReporter())
        for _d, ms in dir_to_meshes.items():
            tile_meshes.extend(ms)
        break  # one tile per spec
    t1 = time.perf_counter()
    print(f"  Build:  {_fmt(t1-t0)}   {_mesh_stats(tile_meshes)}")
    return tile_name, tile_meshes


def profile_3mf_phases(name: str, meshes: list[trimesh.Trimesh]) -> None:
    """Time each phase of 3MF assembly for a single tile's meshes."""
    print(f"\n── Profiling 3MF phases for '{name}' ──")
    print(f"   Input: {_mesh_stats(meshes)}")

    # Phase 1: serial round-trip (happens in batch when crossing process boundary)
    t0 = time.perf_counter()
    serial = _meshes_to_serial(meshes)
    t1 = time.perf_counter()
    meshes2 = _serial_to_meshes(serial)
    t2 = time.perf_counter()
    print(f"   _meshes_to_serial:   {_fmt(t1-t0)}")
    print(f"   _serial_to_meshes:   {_fmt(t2-t1)}")

    # Phase 2: expand mixed materials
    t3 = time.perf_counter()
    expanded = _expand_mixed_materials(meshes2)
    t4 = time.perf_counter()
    print(f"   _expand_mixed_mats:  {_fmt(t4-t3)}   → {_mesh_stats(expanded)}")

    # Phase 3: vertex + triangle XML building (innermost bottleneck candidate)
    total_verts = sum(len(m.vertices) for m in expanded)
    total_faces = sum(len(m.faces)    for m in expanded)
    print(f"\n   XML build for {total_verts:,} verts, {total_faces:,} faces:")

    t_vxml = t_fjoin = t_fxml = t_vjoin = 0.0
    for m in expanded:
        v = m.vertices
        f = m.faces
        ts = time.perf_counter()
        vlines = np.char.add(
            '<vertex x="',
            np.char.add(np.char.mod('%.5f', v[:, 0]),
                np.char.add('" y="', np.char.add(np.char.mod('%.5f', v[:, 1]),
                    np.char.add('" z="', np.char.add(np.char.mod('%.5f', v[:, 2]), '"/>'))
                ))
            )
        )
        t_vxml += time.perf_counter() - ts

        ts = time.perf_counter()
        '\n     '.join(vlines.tolist())
        t_vjoin += time.perf_counter() - ts

        ts = time.perf_counter()
        flines = np.char.add(
            '<triangle v1="',
            np.char.add(np.char.mod('%d', f[:, 0]),
                np.char.add('" v2="', np.char.add(np.char.mod('%d', f[:, 1]),
                    np.char.add('" v3="', np.char.add(np.char.mod('%d', f[:, 2]), '"/>'))
                ))
            )
        )
        t_fxml += time.perf_counter() - ts

        ts = time.perf_counter()
        '\n     '.join(flines.tolist())
        t_fjoin += time.perf_counter() - ts

    print(f"     np.char vert lines:  {_fmt(t_vxml)}")
    print(f"     join vert lines:     {_fmt(t_vjoin)}")
    print(f"     np.char face lines:  {_fmt(t_fxml)}")
    print(f"     join face lines:     {_fmt(t_fjoin)}")
    xml_total = t_vxml + t_vjoin + t_fxml + t_fjoin
    print(f"     XML subtotal:        {_fmt(xml_total)}")

    # Phase 4: full export_3mf_colored to a temp path
    import tempfile, os
    tmp = tempfile.mktemp(suffix='.3mf')
    t5 = time.perf_counter()
    export_3mf_colored([[m for m in expanded]], tmp, names=[name])
    t6 = time.perf_counter()
    sz = os.path.getsize(tmp)
    os.unlink(tmp)
    print(f"\n   export_3mf_colored:  {_fmt(t6-t5)}   ({sz/1e6:.1f} MB output)")
    print(f"   Total 3MF phases:   {_fmt((t2-t0) + (t4-t3) + (t6-t5))}")

    # Phase 5: simulate db+ol double-call with face XML cache (worker path)
    t7 = time.perf_counter()
    cache: dict = {}
    tile_xml_parts(expanded, face_xml_cache=cache)   # db (fills cache)
    tile_xml_parts(expanded, face_xml_cache=cache)   # ol (reuses face XML)
    t8 = time.perf_counter()
    uncached_2x = 2 * (t6 - t5)
    print(f"   tile_xml_parts ×2 (cached faces): {_fmt(t8-t7)}  "
          f"vs {_fmt(uncached_2x)} uncached → saves {_fmt(uncached_2x-(t8-t7))}")


def cprofile_export(name: str, meshes: list[trimesh.Trimesh]) -> None:
    """Run cProfile on the full export_3mf_colored for the worst tile."""
    import tempfile, os
    expanded = _expand_mixed_materials(_serial_to_meshes(_meshes_to_serial(meshes)))
    tmp = tempfile.mktemp(suffix='.3mf')

    pr = cProfile.Profile()
    pr.enable()
    export_3mf_colored([[m for m in expanded]], tmp, names=[name])
    pr.disable()
    os.unlink(tmp)

    sio = io.StringIO()
    ps = pstats.Stats(pr, stream=sio).sort_stats('cumulative')
    ps.print_stats(25)
    print(f"\n── cProfile top-25 (cumulative) for '{name}' 3MF export ──")
    print(sio.getvalue())


def main() -> None:
    if not PROFILE_SPECS:
        print("No tree-tile specs found — check paths")
        return

    print(f"Profiling {len(PROFILE_SPECS)} tile(s): "
          + ", ".join(p.stem.replace('.tile', '') for p in PROFILE_SPECS))

    tile_data: list[tuple[str, list[trimesh.Trimesh]]] = []
    for spec in PROFILE_SPECS:
        name, meshes = build_tile_meshes(spec)
        tile_data.append((name, meshes))

    for name, meshes in tile_data:
        profile_3mf_phases(name, meshes)

    # Deep-profile the worst tile (biggest)
    worst = max(tile_data, key=lambda x: sum(len(m.faces) for m in x[1]))
    print(f"\n{'='*60}")
    print(f"cProfile on worst tile: '{worst[0]}'")
    cprofile_export(worst[0], worst[1])

    # Also time the full batch 3MF (all tiles together, db dir)
    print(f"\n{'='*60}")
    print("Full batch 3MF (all profiled tiles into one 3MF):")
    import tempfile, os
    tmp_dir = pathlib.Path(tempfile.mkdtemp())
    try:
        pairs = [(name, _serial_to_meshes(_meshes_to_serial(ms)))
                 for name, ms in tile_data]
        t0 = time.perf_counter()
        _write_dir_3mf(tmp_dir, pairs)
        t1 = time.perf_counter()
        out = tmp_dir / f"{tmp_dir.name}.3mf"
        sz = out.stat().st_size if out.exists() else 0
        print(f"  _write_dir_3mf: {_fmt(t1-t0)}  ({sz/1e6:.1f} MB)")
    finally:
        import shutil
        shutil.rmtree(tmp_dir, ignore_errors=True)


if __name__ == '__main__':
    main()
