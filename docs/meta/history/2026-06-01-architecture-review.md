# Architecture Review: Current State vs. Target Architecture
**Date:** 2026-06-01  
**Reviewer:** Claude Sonnet 4.6 + Orin  
**Subject:** Full codebase — `src/dharmatiles/` and `scripts/`  
**Goal:** Honest gap analysis between current code and the architecture specified in
`docs/design/terrain-architecture.md` v0.2. Find what to keep, what to cut, what
to rebuild. No invented scope.

---

## 1. Current Codebase Map

```
src/dharmatiles/
  core/
    tile.py        199 lines  TileConfig (God Object) + TileScene + make_terrain
    grid.py         81 lines  bilinear sampling + support_z rasterisation
    flow.py        157 lines  flow field: analytic base + curl noise
    collision.py   178 lines  Möller–Trumbore blade intersection + repair
    mesh.py        363 lines  blade tube, terrain solid, sub-hull, frame utils
  layers/
    grass.py       604 lines  VegetationLayer (all-at-once placement)  ← SUPERSEDED
    grown_grass.py 600 lines  GrownGrassLayer (segment-by-segment growth) ← CANONICAL
    gravel.py      170 lines  GravelLayer (vectorised half-ellipsoids)
  terrains/
    grass_tile.py  252 lines  entry point using VegetationLayer       ← SUPERSEDED
    grown_grass_tile.py 169 lines  entry point using GrownGrassLayer  ← CANONICAL
scripts/
  blade.py         219 lines  2D blade renderer (standalone, archived lineage)
  generate-grass-stl.py  19 lines  thin shim → grown_grass_tile
  archived/        old generations
```

---

## 2. What's Good (Keep As-Is)

These files are clean primitives with tight scopes and clear contracts:

| File | Why it's good |
|---|---|
| `core/grid.py` | Two pure functions: bilinear sample + support rasterisation. No coupling to blade logic. |
| `core/flow.py` | Takes `(cfg, x_grid, y_grid)` → `(angle_field, curv_field)`. Self-contained. Easily replaced with a different field type. |
| `core/collision.py` | Vectorised Möller–Trumbore + repair. No coupling to blade types or config internals. |
| `core/mesh.py` | Low-level primitives: tube mesh, terrain solid, blade frame. Accepts arrays, returns Trimesh. No config leakage. |
| `layers/gravel.py` | Clean layer interface: `.build(scene)` → mesh list. |

The pipeline structure in `grown_grass_tile.py` is also sound:
terrain → flow → gravel → grass → mesh export. That ordering stays.

---

## 3. What's Broken or Vestigial (Fix or Remove)

### 3.1 TileConfig is a God Object

`core/tile.py:TileConfig` is a flat dataclass with **60+ parameters** spanning:
- Tile geometry
- Flow field type
- Grass geometry (tall blades)
- Fill blade geometry (distinct size ranges)
- Cross-section shapes
- Leaf (broadleaf) geometry + lean profile
- Tuft sizes and spreads
- Blade lean profile
- Z-solver clearances
- Collision repair passes
- Gravel geometry

Every layer reads whatever it needs directly from `cfg`. This is the architecture's
most serious violation: **strict layer separation** requires each layer to own its config.

**No layer should need to know about another layer's parameters. Currently all of them do.**

### 3.2 Seeds Are Not Self-Contained

`place_blades()` in `grass.py` returns plain `dict`s with 7 keys:
`base_x, base_y, width, length, tip_len, direction, curl`.

But `make_vegetation_blade()` then reads from `cfg`:
- `n_path`, `base_lean_angle`, `lean_angle`, `leaf_lean_angle`
- `base_obstacle_ignore_t`, `base_sink`, `clearance`
- `max_stack_height`
- `blade_cross_section`, `leaf_cross_section`, `blade_circle_segs`
- `grass_thickness`, `blade_diamond_equator`

**The seed is not self-contained. It cannot grow without external config.**  
This directly violates the architecture's seed model (§8.2.1).

`grown_grass.py` has the same problem: seeds are dicts, growth reads from `GrownGrassLayer`
instance attrs that are set from `cfg`.

### 3.3 Two Competing Grass Systems

`layers/grass.py` (VegetationLayer) and `layers/grown_grass.py` (GrownGrassLayer) both exist.
Only GrownGrassLayer is the current canonical output. VegetationLayer and `grass_tile.py`
are dead code in the shipped pipeline.

The architecturally correct growth model is `grown_grass.py`: segment-by-segment advance
with obstacle avoidance and steering. That matches the spec (§8.2.3: "avoids obstacles,
steers around them"). `grass.py`'s all-at-once LCM envelope is a good internal algorithm
for solving a blade's z-path, but as a placement model it doesn't "grow" — it places.

**`grass.py` (VegetationLayer) and `grass_tile.py` should be retired.**

### 3.4 No Semantic Terrain Grid

The architecture's core concept (§1, §2, §6) is a **grid of TerrainType cells** that
drives all geometry. Current code has no such thing:

- `TerrainType` does not exist
- `make_terrain()` returns a sinusoidal float heightmap — no type system
- `TileType` is three float constants (`GROUND=6.0`, `WATER=3.0`, `MANMADE=9.5`) with
  no dispatch logic; it's vestigial

There is no ground, water, wall, or floor concept in the codebase. The entire semantic
layer (§1.3, §1.4, §4, §5) is unimplemented.

### 3.5 Grid Resolution Wrong

`TileConfig.grid_res = 256` (hardcoded default).  

Architecture spec: **128 cells per tile unit**, derived from surface dimensions.  
A 1×1 tile → 128×128. A 2×2 → 256×256. `grid_res` should not be a free parameter.

Minor correctness note: `gx = tile_w / (grid_res - 1)` uses `grid_res - 1` as the
denominator, making the rightmost cell's right edge sit exactly at `tile_w`. This is
heightmap convention (vertices, not cell centers). Once the semantic grid arrives this
should be revisited: cell-center vs. vertex-sample semantics matter for terrain type
lookup.

### 3.6 Multi-Tile: Not Supported

The architecture defines the working surface as `T_cols × T_rows` tile units. Currently:
- Only 1×1 tile is possible
- `tile_w` and `tile_h` are free floats, not derived from integer tile counts
- No concept of tile-unit boundaries, stitching, or multi-tile seeding

---

## 4. Gap Summary Table

| Architecture Requirement | Current State | Severity |
|---|---|---|
| Semantic `TerrainType` grid | Not implemented | **Critical** |
| Terrain type → height + transitions | Not implemented | **Critical** |
| `GrassSeed` self-contained dataclass | Plain dict, 7 keys, leaks to TileConfig | **High** |
| Per-layer configs (layer separation) | Single God Object `TileConfig` | **High** |
| 128 cells/tile, derived from surface dims | `grid_res=256`, free parameter | **Medium** |
| Multi-tile surface (T_cols × T_rows) | Not supported | **Medium** |
| Single canonical grass system | Two systems (`grass.py` + `grown_grass.py`) | **Medium** |
| Gravel (future) | Fully implemented (OK — no regression, just ahead) | Low / OK |
| Hard/soft terrain transitions | Not implemented | Low (needs terrain types first) |

---

## 5. What the Migration Looks Like

In priority order:

### Phase 1 — Clean up what exists

1. **Retire `layers/grass.py`** (VegetationLayer) and **`terrains/grass_tile.py`**.
   Move them to `scripts/archived/` or delete. `grown_grass.py` is the canonical system.

2. **Split TileConfig** into:
   - `SurfaceConfig`: `tile_cols`, `tile_rows`, `cell_size_mm=35/128` → derives total dims and grid shape
   - `FlowConfig`: flow type + curl noise + dir spread
   - `GrassConfig`: blade geometry ranges, lean profile, cross-section, tuft params
   - `GravelConfig`: stone geometry
   - `SolverConfig`: clearance, base_sink, repair passes, stack height cap

3. **Define `GrassSeed` dataclass** with ALL growth parameters baked in:
   position, direction, curl, length, base_diameter, tip_length, curve_start, power,
   n_path, lean_angle, base_lean_angle, clearance, base_sink, base_obstacle_ignore_t,
   cross_section, thickness. Seeding samples from `GrassConfig` ranges and copies values
   into each seed. Growth algorithm accepts a seed and nothing else.

### Phase 2 — Add the semantic grid

4. **Add `TerrainType` enum**: water, ground, grass, constructed_floor, wall,
   high_wall, highest_wall.

5. **Add `TerrainGrid`**: a 2D array of `(TerrainType, float)` cells — the logical
   grid that authors populate.

6. **Replace `make_terrain()`** with `terrain_grid_to_heightmap()`: walk the grid,
   derive surface heights from terrain type and height field, apply hard/soft
   transitions at boundaries.

7. **Update `SurfaceConfig`** to hold a `TerrainGrid` as its primary input rather
   than sinusoidal parameters.

### Phase 3 — Multi-tile

8. **Add `tile_cols`/`tile_rows` to `SurfaceConfig`**. Grid dims = 128 × tile_cols
   by 128 × tile_rows. Remove `grid_res` as a free parameter.

---

## 6. Orin's Verdict

**The core primitives are excellent.** `grid.py`, `flow.py`, `collision.py`, `mesh.py`
are exactly the right shape: pure functions, array in / array out, no config leakage.
Don't touch them except to update call sites.

**The config layer is the problem.** `TileConfig` expanded organically and now
everything reaches into it. Splitting it is not optional — it's the prerequisite for
every other architecture improvement. Until it's split, no layer is truly independent.

**Kill `grass.py`.** The LCM placement model was an earlier approach. It's dead code in
the current pipeline. Keeping it creates a false sense that there's a choice, introduces
confusion about which system to develop further, and requires maintaining two systems.
The grown_grass segment-by-segment model is correct, matches the architecture intent,
and is what ships. Archive and move on.

**The semantic grid is the foundation.** Everything in the architecture depends on it —
terrain transitions, plant placement zones, height model. It should be the next built
thing after the config split. The sinusoidal terrain is a fine stand-in for testing, but
it's not the architecture.

**Don't add multi-tile until Phase 1 and 2 are done.** The grid resolution and semantic
terrain changes will reshape the core data structures. Adding multi-tile on top of the
current structures means doing it twice.
