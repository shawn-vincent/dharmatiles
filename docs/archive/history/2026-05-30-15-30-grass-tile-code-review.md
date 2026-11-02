# Grass Tile Generator — Code Review & Rewrite Brief
**Date:** 2026-05-30  
**Reviewer:** Claude Sonnet 4.6  
**Subject:** `scripts/generate-grass-stl.py` (promoted from `generate-grass-stl-knots.py`)  
**Goal:** Document the current state, identify what to clean up, and lay out a primitives-first architecture for complex multi-terrain tiles.

---

## 1. What the Script Does (Executive Summary)

Generates a 3D-printable STL of a 35 × 35 mm terrain tile covered with grass blades and embedded gravel stones.

**Pipeline (in order):**
1. Build a sinusoidal **terrain heightmap** (`terrain_z`, 256 × 256 grid).
2. Build a **flow vector field** (`flow_angle_field`, `flow_curv_field`) — controls which way blades lean and curl.
3. **Place blade seeds** on a jittered grid; sort them _downstream-first_ so upstream blades arch over already-placed ones.
4. **Place gravel stones** (vectorised half-ellipsoid geometry); rasterise stone tops into `support_z`.
5. For each blade seed, run the **terrain-following blade solver**:
   - XY path: chord-preserving 2D arc with variable lean.
   - Z path: _least-concave-majorant_ (LCM) envelope that rides the current support field.
   - Strict intersection repair loop (Möller–Trumbore); up to 8 repair passes, 32 direction retries.
   - Rasterise accepted blade top into `support_z`.
6. Build a **watertight terrain solid** (top = heightmap, bottom = flat at −BASE_H).
7. Concatenate all meshes → export STL.

---

## 2. Evolution History

| Era | Script | Z-Solver | Notes |
|-----|--------|----------|-------|
| 0 | `generate-grass-heightmap*.py` | 2D heightmap renders | Exploratory; PIL/image output only |
| 0 | `blade.py` | — | 2D analytical blade SDF renderer; still useful for previewing |
| 1 | `generate-grass-stl.py` (archived) | Gaussian smooth + re-floor passes | First 3D STL. Simple but blades drift high on dense piles |
| 2 | `generate-grass-support-stl.py` (archived) | `scipy.optimize.minimize` curve fit | Added keel, optimizer for z-profile; ~5-10 s/blade; slow |
| 3 | `generate-grass-stl-knots.py` → **current** | Least-concave-majorant + PCHIP | Keel removed; LCM is O(n) and exact; fast and printable |

The **key insight** in the current version: the correct spine height is the _least concave majorant_ of the support obstacle heights. This is the tightest curve that:
- Passes through the base pinned at terrain
- Monotonically can rise above previous blade tops (with clearance)
- Never exceeds `MAX_STACK_HEIGHT` above local terrain

PCHIP interpolation through the LCM control points gives a C1 smooth curve without overshoot.

---

## 3. Strengths (Keep These)

### 3.1 Least-Concave-Majorant Z-Solver
`_fit_sample_envelope_spine` / `_upper_concave_envelope` / `_smooth_contact_curve`  
**Excellent.** O(n) exact algorithm; replaces expensive optimizer. Produces the geometrically "most lying-down" blade that physically clears its neighbours — exactly what you want for printability.

### 3.2 Vectorised Gravel Generation
`add_gravel()` builds 6 000 stones with full numpy broadcasting in one pass.  
**Excellent.** Clean, fast, the rasterise-into-support loop is the only serial part.

### 3.3 Chord-Preserving XY Arc
In `make_grass_blade()`, the XY path is built as a cumsum of lean×azimuth increments, then rotated so the base→tip chord aligns with `azimuth`.  
**Correct and elegant.** Preserves the visual "lean direction" regardless of curl.

### 3.4 Flow Field System
`build_flow_field()` with swirl / linear / radial / drain / dipole / curl modes + curl-noise blend.  
**Strong.** The curvature-driven blade curl (`CURL_FROM_CURV`) is particularly good — blades sweep with the flow instead of randomly.

### 3.5 Möller–Trumbore Strict Intersection Check
`_seg_tri_batch` + `_blade_top_intersections` — batched, vectorised MT test on the blade top-strip quads.  
**Solid correctness tool.** The 8-pass repair loop is pragmatic.

### 3.6 Downstream-First Sort
Blades sorted so downstream blades (already leaning in the flow direction) are placed first; upstream blades naturally arch over them.  
**Physically motivated, visually correct.**

---

## 4. Issues & Smells (Fix in Rewrite)

### 4.1 Module-Level Global State (Major)
`terrain_z`, `support_z`, `flow_angle_field`, `flow_curv_field`, `rng`, `blades`, `GX`, `GY` are **bare module globals**.  
- `terrain_z` is referenced inside `make_grass_blade()` and `add_gravel()` without being passed as an argument.
- `support_z` is shared mutation — the script uses a local copy (`local_support_z`) only inside `build_scene()` but the gravel still mutates the global.
- Makes the code untestable and impossible to call as a library (every import re-runs the setup code).

**Fix:** Encapsulate state in a `TileConfig` dataclass and a `TileScene` object. Pass `terrain_z` explicitly.

### 4.2 Script-at-Import-Time Execution (Major)
All setup code — `print("Building terrain...")`, flow field construction, blade placement — runs at module import time.  
- `from generate_grass_stl import make_grass_blade` triggers the entire pipeline.
- The `build_scene(OUTPUT)` call at the bottom runs unconditionally.

**Fix:** Wrap everything in `if __name__ == "__main__":` and proper functions.

### 4.3 Duplicated Code Between `generate-grass-stl.py` and `generate-grass-stl.py` (Archived)
`sample_grid`, `rasterise_into_support`, `terrain_normal_at`, `add_gravel`, `_build_tube_mesh`, `blade_footprint_inside_tile`, `_compute_up_locs`, `_seg_tri_batch`, `_blade_top_intersections`, `collect_strict_hits`, `strict_check`, `add_collision_repairs`, `make_heightmap_solid` are **copy-pasted verbatim** across the archived scripts.

**Fix (already done by promotion):** Now there's one canonical file. For a rewrite, split into modules (see §6).

### 4.4 `_build_tube_mesh` Has an Unused Variable
```python
K_arr = path  # keel = spine (no downward extension)
```
`K_arr` is set but the actual ring construction uses `path + thickness * down_locs` in the *knots* version, and uses `K_arr` in the archived version. The knots version correctly uses `down_locs`. The variable is a vestige from the keel era.

**Fix:** Remove the dead `K_arr` alias.

### 4.5 `terrain_normal_at` Is Defined but Never Called
The function exists in both current and archived scripts; it was used in an earlier keel implementation. No call site remains.

**Fix:** Delete it.

### 4.6 `sub_hull_mesh` / `_build_sub_hull_mesh` — Purpose Unclear
`make_grass_blade()` returns a `sub_hull_mesh` alongside the blade mesh. In `build_scene()` both are appended to `parts`. The docstring says "separate printable support hull under the blade." 

**Issue:** The hull's `_drop_to_support()` uses a bisection search on a 1D ray — sequential, not vectorised. With 200 blades × 50 path points = 10 000 bisection calls.  
Also: is this sub-hull actually needed for the knot-solver approach? The LCM already ensures the blade rests on its supports. The sub-hull was designed to bridge gaps in the earlier smoothing-based Z-solver.

**Fix:** Audit whether the sub-hull is still needed. If yes, vectorise the drop. If no, remove it.

### 4.7 `build_flow_field()` Uses Seeded-But-Fixed Random for Direction in `'linear'` Mode
```python
angle = frng.uniform(0, 2 * np.pi)
```
The linear angle is deterministic per `SEED` (because `frng` is seeded by `SEED ^ 0x464C4F57`). So changing `SEED` changes the linear direction. This may be intentional, but it's surprising — users expect `FLOW_TYPE='linear'` to have a stable direction and `SEED` to randomise blade placement only.

**Consider:** Add a `FLOW_LINEAR_ANGLE` parameter for explicit control.

### 4.8 Gravel Rasterisation Is O(N) Serial Loop
```python
for s in range(N):  # N = 6000
    ...
    np.maximum(sl, z_top, out=sl)
```
The stone geometry is built vectorised but the `support_z` rasterisation is a Python for-loop over 6 000 stones. Each stone touches a small bounding box, so scatter-add approaches won't directly apply, but a C-extension (or scipy.ndimage) could help.

**Minor issue:** 6 000 stones is fast enough (~0.5 s) but this is the obvious bottleneck for N_GRAVEL > 20 000.

### 4.9 `GRASS_THICKNESS = 0.5` but Cross-Section Comment Is Inconsistent
The knots version docstring says "inverted triangular hull depth" but the tube mesh now builds a flat-top (V1/V2 on the support curve, V0 below via `down_locs`). The `GRASS_SUB_HULL_FRACTION` parameter is used only in `_build_sub_hull_mesh`, not in the main tube. The config block comment could be clearer.

### 4.10 Config Block Has Hardcoded Tile-Type Comments That Aren't Enforced
```python
BASE_H = 6.0  # mm — solid slab below terrain (GROUND)
               #       other tile types: 3.0 (WATER), 9.5 (MANMADE)
```
These magic numbers are foreshadowing a tile-type system that doesn't exist yet. Good signal for what's coming, but needs to become actual code.

### 4.11 Output Path Is Hardcoded
```python
OUTPUT = pathlib.Path("stl/grass.stl")
```
Relative path, no CLI argument parsing. For a library primitive this is wrong.

---

## 5. Critical Algorithm Notes for Rewriter

### 5.1 LCM Contact-Point Stack
`_upper_concave_envelope` implements a **convex hull upper envelope** (Jarvis-march style). The termination condition `slope(b, c) > slope(a, b)` pops intermediate points that would create a concavity. This is correct but tricky: the "upper" concave majorant is the same as the **upper convex hull** when read in parameter order. The code works; just make sure any rewrite preserves the slope comparison direction.

### 5.2 `BASE_OBSTACLE_IGNORE_T = 0.20`
The first 20% of the blade is free to pass through existing geometry — this prevents the base from being pinned up by nearby blades when it erupts from the terrain. This parameter is critical for visual quality; too low → blades can't emerge; too high → visible intersection at base.

### 5.3 `STRICT_BASE_T = 0.25`
Intersections at t ≤ 0.25 are suppressed in the intersection checker. Should be **≥ BASE_OBSTACLE_IGNORE_T**. If you reduce `BASE_OBSTACLE_IGNORE_T`, also reduce `STRICT_BASE_T`.

### 5.4 Downstream Sort Direction
```python
_mfx = float(np.mean(np.sin(flow_angle_field)))
_mfy = float(np.mean(np.cos(flow_angle_field)))
blades.sort(key=lambda b: -(_mfx * b['base_x'] + _mfy * b['base_y']))
```
This sorts blades by their projection onto the mean flow vector, descending — so the "most downstream" blade is placed first. If `FLOW_CURL_NOISE` is very high (≈1), the mean flow is near-zero and the sort order degenerates to arbitrary. In practice FLOW_CURL_NOISE=0.30 keeps the mean well-defined.

### 5.5 Fancy-Index `support_z` Assignment
```python
support_z[jj, ii] = np.maximum(support_z[jj, ii], z)
```
This was a bug in earlier versions (fancy-index reads a copy; must assign back). The current code correctly assigns the result back. Don't "simplify" this with `np.maximum(..., out=support_z[jj, ii])` — that would silently fail with fancy indexing.

---

## 6. Proposed Rewrite Architecture

The goal: **grass, stone, water, mixed** terrains as composable primitives over shared infrastructure.

```
dharmatiles/
├── core/
│   ├── tile.py           # TileConfig, TileScene, terrain heightmap
│   ├── grid.py           # GridField: sample_grid, rasterise_into_support
│   ├── flow.py           # build_flow_field (all FLOW_TYPE modes)
│   ├── mesh.py           # make_heightmap_solid, _build_tube_mesh
│   └── collision.py      # strict intersection: _seg_tri_batch, collect_strict_hits
│
├── layers/
│   ├── gravel.py         # add_gravel(scene) → [Trimesh]
│   ├── grass.py          # GrassLayer: place_blades, make_grass_blade, build_scene
│   ├── water.py          # (future) WaterLayer: animated-looking surface ripples
│   └── stone.py          # (future) StoneFace: flat-faced rock tiles
│
├── terrains/
│   ├── grass_tile.py     # compose: terrain + gravel + grass
│   ├── water_tile.py     # (future) flat water surface + shore rocks
│   └── mixed_tile.py     # (future) zone masks, multi-layer composition
│
├── scripts/
│   ├── generate-grass-stl.py   # thin CLI wrapper over terrains/grass_tile.py
│   └── archived/               # historical scripts
│
└── meta/history/               # this document, future reviews
```

### TileConfig Dataclass Sketch
```python
@dataclass
class TileConfig:
    tile_w: float = 35.0
    tile_h: float = 35.0
    base_h: float = 6.0      # GROUND=6, WATER=3, MANMADE=9.5
    grid_res: int = 256
    seed: int = 42
    terrain_amp: float = 1.0
    terrain_freq: float = 1.5
    flow_type: str = 'linear'
    flow_curl_noise: float = 0.30
    # ... etc.
    
    @property
    def gx(self): return self.tile_w / (self.grid_res - 1)
    @property
    def gy(self): return self.tile_h / (self.grid_res - 1)
```

### Layer Protocol
```python
class Layer(Protocol):
    def build(self, scene: TileScene) -> list[trimesh.Trimesh]:
        """Add geometry and update scene.support_z. Return mesh list."""
        ...
```

### Terrain Zone Masks (for mixed tiles)
```python
# A zone mask is a [GRID_RES, GRID_RES] float array in [0, 1].
# Layers can use the mask to scale their density or skip regions.
# E.g. grass_mask + water_mask = 1.0 everywhere on a shore tile.
```

---

## 7. Specific Rewrite Tasks (Prioritised)

### P0 — Immediate (Correctness & Cleanliness)
1. **Wrap in `if __name__ == '__main__':`** and proper `def main(cfg, output_path):`.
2. **Remove `terrain_normal_at`** (dead code).
3. **Remove `K_arr = path`** vestige in `_build_tube_mesh`.
4. **Pass `terrain_z` explicitly** to `make_grass_blade()` and `add_gravel()`; remove global references.
5. **Audit `_build_sub_hull_mesh`** — is the support hull still needed? If yes, vectorise `_drop_to_support`. If no, remove.

### P1 — Refactor (Library-Ready)
6. **Extract `grid.py`** with `GridField` wrapping `terrain_z`/`support_z` and providing `sample`, `rasterise` methods.
7. **Extract `flow.py`** — pure function, no globals.
8. **Extract `collision.py`** — pure functions on arrays, no globals.
9. **Add `TileConfig` dataclass** — centralise all constants; pass around instead of using module-level names.
10. **CLI argument parsing** with `argparse`: `--output`, `--seed`, `--n-blades`, `--flow-type`, `--no-strict`.

### P2 — Architecture (Terrain Primitives)
11. **Extract `GrassLayer`** class: `__init__(cfg)`, `place_blades()`, `build(scene)`.
12. **Extract `GravelLayer`** class: thin wrapper over existing vectorised code.
13. **`TerrainBuilder`** / `TileScene`: holds `terrain_z`, `support_z`, accumulated meshes; layers mutate it.
14. **`TerrainConfig` enum**: `GROUND`, `WATER`, `MANMADE` → sets `base_h` and default terrain parameters.

### P3 — New Terrain Types
15. **Water tile**: flat or gently rippled surface; no grass; wave-like surface normals baked into geometry.
16. **Stone tile**: large flat-faced rock panels on terrain.
17. **Mixed (grass + water)**: shore tile with zone masks blending grass density and water region.

---

## 8. Parameter Tuning Reference

| Parameter | Current Value | Effect | Tuning Notes |
|-----------|--------------|--------|--------------|
| `N_BLADES` | 200 | density | 300 is printable but slow; 150 looks sparse |
| `TALL_L_MAX` | 14.4 mm | max blade length | ~= 6 mm tile height × 2.4 looks natural |
| `LEAN_ANGLE` | 80° | max lay angle | 80° = nearly horizontal; 60° = more upright |
| `MAX_STACK_HEIGHT` | 6 mm | pile height cap | = BASE_H; prevents runaway stacking |
| `CLEARANCE` | 0.10 mm | gap above previous blade | 0.10 mm is tight; 0.15–0.25 safer for FDM |
| `BASE_OBSTACLE_IGNORE_T` | 0.20 | eruption zone | keep ≤ STRICT_BASE_T |
| `FLOW_CURL_NOISE` | 0.30 | organic variety | 0.15=neat rows, 0.50=chaotic |
| `CURL_FROM_CURV` | 0.80 | streamline-driven curl | 1.0=fully correlated, 0=random |
| `N_GRAVEL` | 6000 | stone density | 6000 covers ~35% of tile area visually |
| `STRICT_MODE` | True | collision checking | set False for 5-10× faster prototyping |

---

## 9. Verdict

The `generate-grass-stl-knots.py` / `generate-grass-stl.py` codebase is **algorithmically solid**. The LCM z-solver is the right approach; the flow field system is well-designed; the intersection repair is pragmatic and effective.

The main technical debt is **structural**: global state, script-at-import execution, and no separation of concerns between geometry primitives, placement logic, and configuration. These are not bugs — they were the right trade-offs during rapid iteration. Now that the algorithm is stable, the natural next step is the modular rewrite described in §6, which will unlock multi-terrain composition without rewriting any of the core math.

**Recommended immediate action:** P0 tasks (§7) take 1-2 hours and make the code importable as a library. P1 tasks take a day and unlock scripted parameter sweeps. P2 sets up the terrain-primitive architecture. P3 can then be tackled terrain type by terrain type.
