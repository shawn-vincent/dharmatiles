"""Diagnostic script: trace why overlapping grass blades pass through each other.

Run with:
    python debug_grass_overlap.py

This patches the grass growth and mesh build code to emit per-blade, per-step
trace data showing:
  1. During growth  — when a blade's floor_z exceeds terrain_z (i.e., it is
                      resting on another blade's occ_z stamp), and by how much.
  2. After growth   — the raw (un-smoothed) path z values at the point where
                      one blade's footprint overlaps another's.
  3. During meshing — how blade_smooth reshapes z along the spine, comparing
                      the raw (lifted) z vs. the post-smooth z at each point.
  4. Final crossing check — at the cell where two blades' XY paths are
                      closest, report the z of each blade's mesh spine.

Usage:
    python debug_grass_overlap.py 2>&1 | tee /tmp/grass_debug.txt
"""

from __future__ import annotations

import sys
import numpy as np
from pathlib import Path

# ── Import project code ───────────────────────────────────────────────────────
sys.path.insert(0, str(Path(__file__).parent / "src"))

import dharmatiles.grass.growers.flat as flat_mod
from dharmatiles.grass.seed import GrowingPath, GrassPath
from dharmatiles.grass.config import GrassConfig, SpeciesConfig
import dharmatiles.grass.grow as grow_mod
import dharmatiles.grass.mesh as mesh_mod
from dharmatiles.terrains.tile import build_tile_from_spec
from dharmatiles.spec import load_spec


# ──────────────────────────────────────────────────────────────────────────────
# PATCH 1: instrument FlatGrassGrower.step to report rises
# ──────────────────────────────────────────────────────────────────────────────

_orig_step = flat_mod.FlatGrassGrower.step  # staticmethod

_step_rises: dict[int, list[tuple[int, float, float, float]]] = {}  # blade_id → [(step, floor_z, terrain_z, nz)]

def _patched_step(path, occ_z, scene, surface, cfg, species):
    if not path.alive or len(path.points) == 0:
        return False

    seed = path.seed
    cx, cy, cz = path.points[-1]
    step_idx = len(path.points) - 1
    direction = seed.direction + seed.curl * step_idx
    tx = cx + seed.blade_segment_length * np.sin(direction)
    ty = cy + seed.blade_segment_length * np.cos(direction)

    # Compute floor_z and terrain_z without advancing the path
    terrain_z = flat_mod._sample_grid(scene.terrain_z, surface, tx, ty)
    floor_z_val = flat_mod._sample_footprint_max(
        occ_z,
        scene.support_z,
        path.last_stamp,
        surface,
        tx, ty,
        seed.blade_width,
        direction,
        x0=cx, y0=cy,
    )
    nz = max(terrain_z, floor_z_val) + cfg.clearance
    rise = floor_z_val - terrain_z

    blade_id = id(path)
    if rise > 0.05:   # anything more than 0.05mm above terrain
        if blade_id not in _step_rises:
            _step_rises[blade_id] = []
        _step_rises[blade_id].append((step_idx, floor_z_val, terrain_z, nz, tx, ty))

    result = _orig_step(path, occ_z, scene, surface, cfg, species)
    return result


flat_mod.FlatGrassGrower.step = staticmethod(_patched_step)


# ──────────────────────────────────────────────────────────────────────────────
# PATCH 2: instrument build_mesh to compare raw vs. smoothed z
# ──────────────────────────────────────────────────────────────────────────────

_orig_build_mesh = flat_mod.FlatGrassGrower.build_mesh  # staticmethod
_smoothing_deltas: dict[int, np.ndarray] = {}   # path_id → z_raw - z_smooth per point

def _patched_build_mesh(path, species, scene, surface):
    raw_spine = np.asarray(path.points, dtype=float)
    smoothed_spine = flat_mod._smooth_blade_spine(raw_spine.copy(), species.blade_smooth)

    path_id = id(path)
    z_delta = raw_spine[:, 2] - smoothed_spine[:, 2]
    _smoothing_deltas[path_id] = z_delta

    if np.any(np.abs(z_delta) > 0.1):
        max_drop = float(np.max(z_delta))     # positive = smoothing LOWERED z
        max_rise = float(np.max(-z_delta))    # positive = smoothing RAISED z
        step_of_max_drop = int(np.argmax(z_delta))
        print(f"  [smooth] blade@({path.points[0][0]:.1f},{path.points[0][1]:.1f})"
              f"  blade_smooth={species.blade_smooth:.2f}"
              f"  max_smooth_drop={max_drop:.3f}mm at step {step_of_max_drop}"
              f"  max_smooth_rise={max_rise:.3f}mm")
        # Print per-step z comparison around the max drop
        lo = max(0, step_of_max_drop - 3)
        hi = min(len(raw_spine), step_of_max_drop + 4)
        print(f"    step  raw_z  smooth_z  delta")
        for i in range(lo, hi):
            marker = " <--" if i == step_of_max_drop else ""
            print(f"    {i:3d}  {raw_spine[i, 2]:6.3f}  {smoothed_spine[i, 2]:6.3f}"
                  f"  {z_delta[i]:+6.3f}{marker}")

    return _orig_build_mesh(path, species, scene, surface)

flat_mod.FlatGrassGrower.build_mesh = staticmethod(_patched_build_mesh)


# ──────────────────────────────────────────────────────────────────────────────
# PATCH 3: instrument _lift_path_points to show pre/post lift z
# ──────────────────────────────────────────────────────────────────────────────

_orig_lift = mesh_mod._lift_path_points
_lift_records: list[dict] = []

def _patched_lift(points, support_z, surface):
    lifted = _orig_lift(points, support_z, surface)
    # Find max lift applied
    raw_z   = np.array([p[2] for p in points])
    lifted_z = np.array([p[2] for p in lifted])
    delta = lifted_z - raw_z
    if np.any(delta > 0.05):
        max_lift = float(np.max(delta))
        step_of_lift = int(np.argmax(delta))
        print(f"  [lift] blade@({points[0][0]:.1f},{points[0][1]:.1f})"
              f"  max_lift={max_lift:.3f}mm at step {step_of_lift}"
              f"  raw_z={raw_z[step_of_lift]:.3f}  lifted_z={lifted_z[step_of_lift]:.3f}")
    _lift_records.append({
        "xy": (points[0][0], points[0][1]),
        "raw_z": raw_z,
        "lifted_z": lifted_z,
    })
    return lifted

mesh_mod._lift_path_points = _patched_lift


# ──────────────────────────────────────────────────────────────────────────────
# PATCH 4: capture grow_all's occ_z and final paths for crossing analysis
# ──────────────────────────────────────────────────────────────────────────────

_orig_grow_all = grow_mod.grow_all
_captured_growing: list = []
_captured_occ_z = [None]

def _patched_grow_all(scene, surface, cfg, rng, verbose=True):
    occ_z = scene.support_z.copy()
    growing = grow_mod.plant_seeds(scene, surface, cfg, occ_z, rng)

    if verbose:
        n_groups = sum(s.groups_per_square * surface.cols * surface.rows for s in cfg.species)
        print(f"  Planted {len(growing)} blades in {n_groups} groups")
        for i, g in enumerate(growing):
            print(f"    Blade {i}: seed=({g.seed.x:.1f},{g.seed.y:.1f})"
                  f"  dir={np.degrees(g.seed.direction):.1f}°"
                  f"  width={g.seed.blade_width:.2f}mm"
                  f"  n_steps={g.seed.n_steps}")

    grow_mod._sort_downstream_first(growing)
    species_map = {species.name: species for species in cfg.species}
    max_steps = max((path.seed.n_steps for path in growing), default=0)

    for round_idx in range(max_steps):
        grown = 0
        for path in growing:
            if not path.alive or round_idx >= path.seed.n_steps:
                path.alive = False
                continue
            species = species_map[path.seed.species_id]
            grower = grow_mod.GROWERS[species.grower]
            if grower.step(path, occ_z, scene, surface, cfg, species):
                grown += 1

        if verbose:
            alive = sum(1 for path in growing if path.alive)
            print(f"  Round {round_idx + 1:2d}: {grown:3d} segments grown, {alive:3d} blades still alive")
        if grown == 0:
            break

    _captured_growing.extend(growing)
    _captured_occ_z[0] = occ_z.copy()

    return [
        GrassPath(seed=path.seed, points=path.points)
        for path in growing
        if len(path.points) >= 2
    ]

grow_mod.grow_all = _patched_grow_all


# ──────────────────────────────────────────────────────────────────────────────
# Run the tile build
# ──────────────────────────────────────────────────────────────────────────────

SPEC_PATH = Path("src/tiles/grass-only.tile")
print(f"=== Loading spec: {SPEC_PATH} ===")
spec = load_spec(SPEC_PATH)
print()

print("=== Building tile ===")
print()

# Monkey-patch the grass layer to also capture paths after build
import dharmatiles.grass.layer as layer_mod
_orig_build = layer_mod.GrassLayer.build

_final_paths: list[GrassPath] = []

def _patched_build(self, scene, verbose=True):
    surface = scene.config.surface
    rng = np.random.default_rng(self.cfg.seed)
    paths = grow_mod.grow_all(scene, surface, self.cfg, rng, verbose=verbose)
    _final_paths.extend(paths)
    meshes = mesh_mod.build_meshes(paths, self.cfg, scene, surface)
    if verbose:
        segs = [len(path.points) - 1 for path in paths]
        if segs:
            avg_len = np.mean([segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths))])
            max_len = max(segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths)))
            print(f"  Built {len(paths)} blades — avg {avg_len:.1f} mm, max {max_len:.1f} mm")
    return meshes

layer_mod.GrassLayer.build = _patched_build

from dharmatiles.terrains.tile import build_tile_from_spec
import pathlib
try:
    build_tile_from_spec(spec, output_path=pathlib.Path("/tmp/debug_tile.stl"), verbose=True)
except Exception as e:
    print(f"\n[EXCEPTION during build]: {e}")
    import traceback; traceback.print_exc()


# ──────────────────────────────────────────────────────────────────────────────
# Post-build analysis
# ──────────────────────────────────────────────────────────────────────────────

print()
print("=" * 70)
print("POST-BUILD ANALYSIS")
print("=" * 70)

# A. Report all rise events during growth
print()
print("A. RISE EVENTS DURING GROWTH (floor_z > terrain_z + 0.05mm):")
if not _step_rises:
    print("  *** NONE — no blade ever rose above terrain during growth! ***")
    print("  This means blades are NOT reading each other's occ_z stamps.")
else:
    blade_ids = list(_step_rises.keys())
    for bid in blade_ids:
        rises = _step_rises[bid]
        print(f"  Blade id={bid}: {len(rises)} rise events")
        for step, floor_z, terrain_z, nz, tx, ty in rises[:5]:
            print(f"    step {step:2d}: floor_z={floor_z:.3f}  terrain_z={terrain_z:.3f}"
                  f"  rise={floor_z-terrain_z:.3f}mm  nz={nz:.3f}  pos=({tx:.1f},{ty:.1f})")
        if len(rises) > 5:
            print(f"    ... ({len(rises)-5} more)")

# B. Check path crossing points — find pairs of blades whose XY paths cross
print()
print("B. PATH CROSSING ANALYSIS:")
paths = _final_paths
n = len(paths)
print(f"  Total blades: {n}")

crossings_found = 0
for i in range(n):
    for j in range(i + 1, n):
        pa = paths[i].points
        pb = paths[j].points
        # Find closest XY approach between the two paths
        min_dist = float("inf")
        best = None
        for si, (ax, ay, az) in enumerate(pa):
            for sj, (bx, by, bz) in enumerate(pb):
                d = np.hypot(ax - bx, ay - by)
                if d < min_dist:
                    min_dist = d
                    best = (si, sj, ax, ay, az, bx, by, bz)
        if min_dist < paths[i].seed.blade_width + paths[j].seed.blade_width:
            si, sj, ax, ay, az, bx, by, bz = best
            crossings_found += 1
            z_diff = az - bz
            print(f"  Blades {i}↔{j}: min XY dist={min_dist:.2f}mm"
                  f"  blade_widths=({paths[i].seed.blade_width:.1f},{paths[j].seed.blade_width:.1f})")
            print(f"    At closest approach:")
            print(f"      Blade {i} step {si}: z={az:.3f}mm  pos=({ax:.1f},{ay:.1f})")
            print(f"      Blade {j} step {sj}: z={bz:.3f}mm  pos=({bx:.1f},{by:.1f})")
            print(f"      z diff = {z_diff:+.3f}mm  (+ means blade {i} is higher)")
            if abs(z_diff) < 0.5:
                print(f"      *** WARNING: blades are nearly at the SAME Z — intersection likely! ***")

if crossings_found == 0:
    print("  No blade pairs found close enough to cross. Check blade placement.")

# C. Smoothing impact summary
print()
print("C. SMOOTHING IMPACT SUMMARY (from patch 2):")
if not _smoothing_deltas:
    print("  No smoothing delta data captured.")
else:
    all_max_drops = [(pid, float(np.max(d))) for pid, d in _smoothing_deltas.items()]
    all_max_drops.sort(key=lambda x: -x[1])
    print(f"  {len(all_max_drops)} blades processed. Worst z-drop from smoothing:")
    for pid, drop in all_max_drops[:5]:
        print(f"    blade id={pid}: smoothing LOWERED z by up to {drop:.3f}mm")

print()
print("=== Diagnosis complete ===")
