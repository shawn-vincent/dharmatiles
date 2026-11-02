# Full Code Review — 2026-06-09

**Scope:** Complete re-read of the entire production codebase (5,445 lines across
`core/`, `grass/`, `layers/`, `bases/`, `terrains/tile.py`, and all `.tile.py` specs)
following the 2026-06-08 review.  Previous open items are confirmed resolved; new
findings documented below.

---

## Status of 2026-06-08 Open Items

All four items from the previous review are resolved:

| Item | Status |
|---|---|
| Grass DRY — 7 helpers duplicated across 3 modules | ✅ `grass/_geometry.py` created; all copies deleted |
| `TerrainGrid` orphaned from pipeline | ✅ `TerrainCell`, `TerrainGrid`, `terrain_grid_to_heightmap`, `TileScene.from_terrain_grid` deleted; `core/terrain.py` retains only `TerrainType` enum and height defaults |
| Grass species region-unaware | ✅ `_collect_grass_configs` now returns `list[tuple[SpeciesConfig, mask]]`; region mask threaded into `_build_mesh` per-packet |
| `terrain_z` mutated inside `_build_mesh` | ✅ Water-floor zero-out moved to `build_tile_from_spec` before scene construction |

The CLAUDE.md "Known Open Items" section was rewritten to reflect these; it now
lists four accurate items.

---

## New Findings

### 1. Flow field is architecturally present but completely unwired

`core/flow.py::build_flow_field()` is a complete implementation of a spatial
direction field (7 analytic types: swirl, linear, radial, drain, dipole,
random-zones, curl; plus curl-noise blending and a curvature derivative).  It is
exported from `core/__init__.py` and `FlowConfig` lives in `SceneConfig`.
`_scene_config_from_spec` instantiates `FlowConfig()` every build.

**None of this is ever called.**  Grepping the entire `src/` tree for
`build_flow_field`, `angle_field`, or `curv_field` produces only hits inside
`core/flow.py` itself.

The grass pipeline uses uniformly random group directions (`group_dir = rng.uniform(0, 2π)`).
`FlowConfig.dir_spread` (per-blade Gaussian direction jitter) is also unused —
`_make_seed` reads `species.group_dir_jitter`, not `FlowConfig.dir_spread`.
`FlowConfig.curl_from_curv` (curvature-driven blade curl weight) is likewise dead.

**Impact:** Current grass has purely random directions with no spatial coherence.
Real-looking wind-swept or radial patterns are unachievable with current wiring.

**Fix path:** In `_make_seed` (or `plant_seeds`), accept an `angle_field` sampled at
the seed's position and bias `group_dir` toward it.  The `curv_field` can then
weight `blade_curl_min/max` to follow flow curvature.  This is a one-function
change; the infrastructure already exists.

---

### 2. `_jitter_grid_xy` allocates a full-grid bool array per Voronoi group

`grow.py:277–278`:
```python
group_mask = np.zeros((surface.grid_h, surface.grid_w), dtype=bool)
group_mask[group_rows, group_cols] = True
```
A `(grid_h × grid_w)` bool array is allocated and written for every Voronoi group,
just to support the single membership test on line 341 (`valid = group_mask[iys, ixs]`).

At 256 cells/sq: each array = 65,536 bytes.  For `soil+grass.tile.py`'s carpet pass
(`groups_per_square=240`): 240 groups × 65,536 = ~15 MB of bool array churn per call,
plus 240 full-grid write passes to populate them.

The group cells are already available as `group["rows"]` / `group["cols"]`.
A sparse set lookup eliminates the full-grid allocation entirely:

```python
group_set = set(zip(group_rows.tolist(), group_cols.tolist()))
valid_xy = [(xs[k], ys[k]) for k in range(len(xs))
            if (iys[k], ixs[k]) in group_set]
```

Or pass the `group_mask` into `_jitter_grid_xy` from the caller (`_voronoi_groups`
already has the per-group cell lists) to build it once and reuse.

---

### 3. `_collect_seeds` in `grass_carpet.py` silently mutates `scene.vegetation_support_z`

`grass_carpet.py:148`:
```python
scene.vegetation_support_z = scene.terrain_support_z.copy()
```
This fires inside `_collect_seeds`, a function whose name implies a read-only query.
The intent is correct (sync the support surface to the post-soil state before
planting carpet seeds), but the side effect is invisible at the call site in
`GrassCarpetLayer.build`.

No practical bug today because `_build_mesh` resets `vegetation_support_z` again
right before the 3D grass pass (`scene.vegetation_support_z = scene.terrain_support_z.copy()`
at `tile.py:128`).  But the mutation is surprising for future maintainers.

**Fix:** Either rename `_collect_seeds` to something that signals the side effect,
or lift the reset into `GrassCarpetLayer.build` before calling `_collect_seeds`.

---

### 4. `import time as _time` defined twice in `tile.py`

Appears at line 206 (inside `_build_mesh`) and again at line 829 (inside `main`).
Should be a single module-level import.

---

### 5. Stale docstring: `build_tile_from_spec` mentions YAML

`tile.py:321`:
```python
def build_tile_from_spec(spec: TileSpec, ...):
    """Build a tile from a YAML/Python TileSpec ..."""
```
YAML specs were retired in commit c6b10a6.  Only `.tile.py` Python specs exist.

---

### 6. `soil+grass.tile.py`: carpet and 3D grass density differ 10×

```python
LayerSpec(type='grass_carpet', params=dict(groups_per_square=240)),
LayerSpec(type='grass',        params=dict(groups_per_square=24)),
```
The carpet plants 10× as many seed groups as the 3D layer.  This is probably
intentional — a dense field of flat stamps under sparsely distributed upright
blades — but there is no comment documenting the design choice.  Future maintainers
who try to "match" carpet stamps to 3D blades will be confused.

**Note:** Even with equal `groups_per_square`, carpet and 3D blade positions will
not align — they use different RNG seeds (`0x554E_4445` vs. `0x47524F57`).  The
CLAUDE.md / GrassUnderlayConfig docstring claim "2D stamps exactly match the 3D
blades" refers to *blade geometry*, not *position*.  The word "match" is misleading.

---

## Positive Observations

The codebase is in excellent shape overall.  Specific things done right:

- **`grass/_geometry.py`** is a clean shared-helper hub.  No duplicates found.
  `_spine_distances`, `_cell_index`, `_contained_segment_cells`, and `_stamp_segment`
  each have exactly one implementation, cross-imported cleanly.

- **`_collect_grass_configs` / `_collect_grass_carpet_layers`**: Both collectors
  correctly handle three call forms (`species=SpeciesConfig(...)`, flat kwargs, or a
  mix).  The `dataclasses.replace` override chain is elegant and correct.

- **`_build_mesh` parameter wiring** is clean after the grass-region-awareness fix:
  each layer type has its own `list[tuple[config, mask]]` parameter, all defaulting
  sensibly.

- **Terrain pipeline ordering** (`soil → carpet → terrain_support_z sync → rocks → grass`)
  is now correct.  The sync at `tile.py:101` ensures rocks and grass see the post-soil
  terrain.

- **Water pool pipeline** (`_extend_bank_slope_into_pool` → `terrain_z[water_mask] = 0.0`
  before scene creation) correctly honours the `terrain_z` read-only contract.

- **Adaptive terrain mesh** (`_make_heightmap_solid_adaptive`): the RDP boundary ring
  ensures no T-junctions between the top surface Delaunay and side walls.  The
  combined `lap + bg_grid` interior vertex selection is sound.

- **Rock slope alignment**: The Rodrigues rotation (deriving terrain-normal R matrix per
  rock, applying to all ring+apex vertices vectorised) is correctly implemented.

- **`_sort_upstream_first`**: Semantically correct for occlusion layering — blades
  close to their target edge get grown and stamped first, so interior blades stack
  over them rather than under.

- **`GrassSeed.distance_taper_vec`**: Vectorised version matches the scalar
  `distance_taper` exactly.  Both the tip-taper and base-taper paths handle edge
  cases (zero `taper_len`, `base_fraction >= 1`) identically.

---

## Documentation Drift Summary

| Location | Drift |
|---|---|
| `build_tile_from_spec` docstring | "YAML/Python" — YAML retired |
| `FlowConfig` docstring / comments | Describes feature as if wired; not called in pipeline |
| `GrassUnderlayConfig` docstring | "exactly match the 3D blades" — true for geometry, misleading for position |
| `soil+grass.tile.py` | 10× groups density gap between carpet and 3D grass is undocumented |

---

## Actions Taken Same Session

All four items resolved in the same session as this review:

1. **Flow field deleted** — `core/flow.py` removed; `FlowConfig` removed from
   `core/config.py` and `SceneConfig`; `build_flow_field` export removed from
   `core/__init__.py`; `FlowConfig` import and use removed from `terrains/tile.py`.

2. **`_jitter_grid_xy` fixed** — full `(grid_h × grid_w)` bool allocation replaced
   with a `frozenset(zip(group_rows, group_cols))` lookup. `np.fromiter` constructs
   the valid mask without any grid-sized allocation.

3. **Carpet/3D docs updated** — `GrassUnderlayConfig` docstring now says "same
   *geometry*, not same *position*"; `soil+grass.tile.py` has a comment explaining
   the intentional 10× density mismatch.

4. **`_collect_seeds` side effect made explicit** — vegetation_support_z reset
   lifted from `_collect_seeds` into `GrassCarpetLayer.build()` with a
   documenting comment; `_collect_seeds` is now a pure query.

5. **Minor cleanup** — `import time as _time` consolidated to one module-level
   import in `tile.py`; `build_tile_from_spec` docstring "YAML/Python" → "Python".

6. **CLAUDE.md fully resynchronised** — stale CLI flags, wrong module names,
   deleted flow-field pipeline step, wrong TileScene field names, stale colour
   encoding section, wrong extras filename, and missing grass/bases layout all
   corrected.

---

## CLAUDE.md "Known Open Items" — Suggested Update

Replace the current four items with:

1. **Flow field unwired** — `core/flow.py::build_flow_field()` exists and is
   complete but never called.  `FlowConfig` is dead config.  Wiring it through
   `plant_seeds` / `_make_seed` would add spatial grass directionality.

2. **`_jitter_grid_xy` full-grid bool allocation per group** — performance
   bottleneck for high-density grass.  Replace with a sparse set lookup on
   `(row, col)` pairs.

3. **Carpet / 3D grass RNG seeds differ** — `grass_carpet` seeds are planted with
   a different RNG stream than 3D grass.  The "exactly match" docstring claim
   applies to geometry only.  Positions are independently random.

4. **`_collect_seeds` side effect undocumented** — resets `scene.vegetation_support_z`
   inside a function named "collect".

Items 3 and 4 are low priority; items 1 and 2 are medium priority for visual quality
and performance respectively.
