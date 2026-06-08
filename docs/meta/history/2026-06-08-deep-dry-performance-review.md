# Deep DRY / Performance / Complexity Review — 2026-06-08

Follows on from `2026-06-08-full-code-review.md`.  That session fixed the four
open items (DRY consolidation, TerrainGrid deletion, grass region masks,
`terrain_z` immutability).  This session goes deeper: every file in
`src/dharmatiles/` was read in full before writing these findings.

---

## 1 · Dead Code (safe to delete; no output change)

### 1.1 `core/mesh.py` — `build_tube_mesh` + `blade_frame` (~180 lines)

`build_tube_mesh` and its helper `blade_frame` exist in `core/mesh.py` and are
re-exported from `core/__init__.py`.  The live grass pipeline does not use them:
`grass/growers/flat.py` has its own `_build_blade_mesh` / `_make_ring_verts`
path.  The only callers are in `src/scripts/archived/`.  These two functions are
dead in the production pipeline.

**Action:** delete both from `core/mesh.py`; remove from `core/__init__.py`.
Archived scripts can keep their own copies or be left broken (they are archived).

### 1.2 `terrains/tile.py` — `_system_output_path`

Defined at line 201 but never called anywhere in live code.

**Action:** delete.

### 1.3 `grass/seed.py` — `GrowingPath.last_stamp`

```python
last_stamp: dict[tuple[int, int], float] | None = None
```

Never assigned (always `None`) and never read.  Looks like scaffolding for a
stamping optimisation that was not implemented.

**Action:** delete the field.

### 1.4 `core/tile.py` — Stale docstring in `TileScene.from_config`

The docstring says _"scripts that have not yet been migrated to TerrainGrid"_.
`TerrainGrid` was deleted in the previous session.

**Action:** replace with _"sinusoidal stand-in for non-spec initialisation"_.

---

## 2 · DRY Violations

### 2.1 `grass/_geometry._sample_grid` duplicates `core/grid.sample_grid`

`grass/_geometry.py` has a scalar-only bilinear sampler:

```python
def _sample_grid(grid, surface, x: float, y: float) -> float: ...
```

`core/grid.sample_grid` is identical but already handles both scalars and arrays
(it returns `float(result)` when given scalar inputs).  The grass package imports
`_sample_grid` from `_geometry` in three places (`growers/flat.py`, `mesh.py`).

**Action:** delete `_geometry._sample_grid`; update the three import sites to
`from ..core.grid import sample_grid as _sample_grid` (drop-in replacement since
the signature is the same for scalar inputs).

### 2.2 `_stamp_swept_footprint` (flat.py) ≈ `_stamp_segment_profile` (mesh.py)

Both functions implement _exactly_ the same algorithm:

1. Call `_contained_segment_cells`
2. Compute `z_spine = z0 + (z1 - z0) * along_norm`
3. Compute `z_field = z_spine [ + thickness * sin(π * lateral_frac) ]` for n≥2
4. `np.maximum(block, np.where(mask, z_field, block), out=block)`

The only differences are argument naming and which array is stamped (`occ_z` vs
`support_z`).  Both arrays have the same shape and the operation is identical.

**Action:** move a single canonical `_stamp_segment(support, surface, p0, p1,
width0, width1, z0, z1, thickness0, thickness1, n_top_facets)` into
`_geometry.py`.  Both call sites become one-liners.

### 2.3 Three `_collect_*` functions share identical control flow

`_collect_grass_configs`, `_collect_soil_layers`, and `_collect_stones_layers`
in `terrains/tile.py` all follow:

```
for idx, region in enumerate(spec.regions):
    for layer in region.layers:
        if layer.type in TYPES:
            cfg = vars(DefaultClass()).copy()
            cfg.update(layer.params)
            mask = (region_mask == idx) if region_mask is not None else None
            result.append((DefaultClass(**cfg), mask))
for boundary in spec.boundaries:
    for layer in boundary.layers:
        if layer.type in TYPES:
            ... same pattern, mask = (region_mask < 0) ...
```

The only differences are `TYPES`, `DefaultClass`, and whether boundaries are
included.

**Action:** add a generic helper:

```python
def _collect_layers(spec, region_mask, layer_types, cfg_class, include_boundaries=True):
    ...
```

The three public functions become one-liners calling it.  Removes ~60 lines.

### 2.4 Six redundant calls to the three `_collect_*` helpers

`build_tile_from_spec` calls all three helpers twice: once for DB (lines 365-367)
and again for OL (lines 399-401).  Both calls pass the same `spec` and
`region_mask`, so the results are identical.

**Action:** assign to locals once, reuse for the OL rebuild:

```python
grass_cfgs   = _collect_grass_configs(spec, region_mask)
soil_layers  = _collect_soil_layers(spec, region_mask)
stone_layers = _collect_stones_layers(spec, region_mask)
# ...
# OL block:
ol_tile_mesh = _build_mesh(ol_cfg, ol_scene,
                           grass_cfgs=grass_cfgs,     # reuse
                           soil_layers=soil_layers,   # reuse
                           stone_layers=stone_layers, # reuse
                           ...)
```

### 2.5 `build_grass_mask(region_mask, spec)` called twice

Lines 353 and 398 in `build_tile_from_spec` both call
`build_grass_mask(region_mask, spec)` with identical arguments (pure function).

**Action:** cache the result:

```python
grass_mask = build_grass_mask(region_mask, spec)
scene.grass_mask = grass_mask
# ...
ol_scene.grass_mask = grass_mask
```

---

## 3 · Redundant Computations

### 3.1 `g_R` and `norm` recomputed for every blob

In `soil.py::_accumulate_blob`:

```python
g_R  = float(np.exp(-(cutoff ** power) * 0.5))
norm = 1.0 - g_R
```

`cutoff` and `power` come from `soil.blob_cutoff` and `soil.blob_power` — tile
constants that are the same for every blob in a tier.  These two values are
recomputed inside the inner loop `for i in range(n): _accumulate_blob(...)` — up
to 277 × n_squares times per tier.

**Action:** pre-compute in `_compute_bump_field` before the blob loop and pass as
arguments to `_accumulate_blob`.

### 3.2 `np.gradient(terrain_z)` computed twice across layers

`SoilLayer.build` calls `np.gradient(scene.terrain_z, ...)` for arc-length
reparameterisation.  `_build_stones_mesh` calls `np.gradient(terrain_z, axis=...)` 
for terrain-normal slope alignment.  Both compute the full
`(grid_h, grid_w)` gradient of the terrain heightmap.

Soil modifies `terrain_z` before stones run, so the arrays are slightly different
— but the structure is identical and the gradient is recomputed.  For a
256-cell/square 1×1 tile that is a 256×256 gradient twice.

**Action:** store the gradient in `TileScene` alongside `terrain_z`:

```python
terrain_gz_x: np.ndarray | None = None   # dz/dx  (populated on first demand)
terrain_gz_y: np.ndarray | None = None   # dz/dy
```

Both layers call a `scene.terrain_gradient()` accessor that lazily computes and
caches.  Alternatively, pre-compute in `build_tile_from_spec` after soil and
store as fields before stones runs.

### 3.3 `xs_flat = xs; ys_flat = ys` identity assignments

`grass/grow.py::_jitter_grid_xy` lines 333-334:

```python
xs_flat = xs
ys_flat = ys
```

`xs` and `ys` are already 1-D arrays from `np.concatenate`.  The `_flat` aliases
are immediately used on lines 336-340 and add nothing.

**Action:** delete both lines; use `xs` and `ys` directly.

### 3.4 `wz` broadcast multiplied by unnecessary `np.ones` in stones.py

```python
wz = (base_z[:, None, None] +
      height[:, None, None] * z_off[None, :, None] * np.ones((1, 1, AZ)))
```

`height[:, None, None] * z_off[None, :, None]` already broadcasts to
`(N, EL, AZ)` without help.  The trailing `* np.ones((1, 1, AZ))` allocates a
new float64 array of shape `(1, 1, AZ)` for no benefit.

**Action:** remove the `* np.ones(...)` factor.

---

## 4 · Micro-Performance (Python hot paths)

These are not correctness issues but show up in profiling when generating dense
grass.

### 4.1 `_build_blade_mesh` face list uses Python appends (flat.py)

```python
faces: list[list[int]] = []
for i in range(n_rings - 1):
    a = i * nvr; b = (i + 1) * nvr
    for j in range(nvr):
        j1 = (j + 1) % nvr
        faces.append([a + j,  b + j,  b + j1])
        faces.append([a + j,  b + j1, a + j1])
```

For a 30-step blade with 8 facets: 30×8×2 = 480 Python list appends.  For 1,000
blades: 480,000 pure-Python list operations.  Fully vectorisable with `np.arange`
+ broadcasting (see the equivalent vectorised pattern already used in
`core/mesh.py` and `layers/stones.py`).

### 4.2 `_make_ring_verts` loops over `n_top_facets + 1` in Python

```python
for i in range(n_top_facets + 1):
    x_frac = i / n_top_facets
    lat = -half_w + width * x_frac
    h = thickness * np.sin(np.pi * x_frac)
    top_verts.append(center + perp * lat + up * h)
```

This is called for every ring of every blade (n_rings × n_blades = up to
30,000+ times).  With 8 facets that is 270,000 Python iterations.  Trivially
vectorised: precompute `x_fracs = np.linspace(0, 1, n+1)` and use broadcasting.

### 4.3 `distance_taper` called in a Python list comprehension per blade

In `grass/mesh.py::build_meshes`:

```python
point_tapers = np.array(
    [path.seed.distance_taper(d, total_len) for d in path_dists], dtype=float
)
```

`distance_taper` calls two `math.*` functions per point.  For 1,000 blades × 30
points this is 60,000 Python `math.*` calls.  The taper function is expressible
as a vectorised `np.where` / `np.clip` / `np.sin` chain over the whole
`path_dists` array at once.

### 4.4 Stone support_z rasterisation is a Python loop over N stones

`stones.py` lines 282-316: `for s in range(N): ...`.  The mesh generation above
it is fully vectorised.  The rasterisation loop is lower priority (runs once vs
many blade steps) but could be vectorised for large stone counts.

---

## 5 · Unnecessary Complexity

### 5.1 `SolverConfig` is a single-field dataclass

```python
@dataclass
class SolverConfig:
    max_stack_height: float = 2.0
```

The field is accessed as `cfg.solver.max_stack_height` throughout the codebase.
There is no benefit to wrapping one float in its own dataclass.  The field could
move to `SurfaceConfig` (physical limits belong alongside grid dimensions) or
simply become a top-level parameter on `GrassConfig`.

### 5.2 `TileScene.support_z` alias property

```python
@property
def support_z(self): return self.terrain_support_z
@support_z.setter
def support_z(self, value): self.terrain_support_z = value
```

The only live caller using `.support_z` (not `.terrain_support_z`) is
`stones.py` line 60.  The alias is pure compatibility noise with one call site.

**Action:** rename the one call site in `stones.py` to `.terrain_support_z`;
delete the property.

### 5.3 `SurfaceConfig.flat_terrain` is dead in the production pipeline

`flat_terrain` is only read by `TileScene.from_config` (the legacy sinusoidal
stand-in path).  `build_tile_from_spec` builds terrain via `_build_spec_terrain`
and never consults this flag.  It's spec-loaders and `TileScene.from_config` that
read it; anything using the spec pipeline ignores it entirely.

Unless `TileScene.from_config` is deleted (the legacy entry point), the flag
should at minimum be documented as _"legacy only; ignored by build_tile_from_spec"_.

### 5.4 `_collect_grass_configs` creates a fresh `SpeciesConfig()` every call

```python
defaults = SpeciesConfig()
...
d = vars(defaults).copy()
d.update(layer.params)
```

`SpeciesConfig` is a frozen dataclass.  `vars()` allocates a new dict each call
and `d.update(layer.params)` a second one.  With the proposed generic helper
(§2.3) this pattern appears only once, but a direct `dataclasses.replace` or
`SpeciesConfig(**{**vars(SpeciesConfig()), **layer.params})` would be cleaner.

### 5.5 `wz` axis-3 collapse via `np.ones` (already noted in §3.4)

The pattern `z_off[None, :, None] * np.ones((1, 1, AZ))` forces a broadcast
that NumPy would apply automatically.  Explicitly multiplying by `np.ones`
signals to readers that the developer was unsure of broadcasting rules — a
readability issue as well as a micro-waste.

---

## 6 · Borderline (Noted, Not Acted On)

| Item | Reason to leave |
|------|----------------|
| `_build_spec_terrain` runs N EDT calls (one per region) | N is always ≤ 4 in practice; multi-label EDT requires custom code |
| `_sort_upstream_first` uses Python `min()` on a short list | Lists have at most 2 elements (x and y boundary); not a hotspot |
| `_fit_quadratic_arc` recomputes `np.diff` that `_spine_distances` also computes | Pre/post-smoothing spines differ; the recomputation is intentional |
| `_random_spread_sites` farthest-point heuristic in `grow.py` | Intentional approximate algorithm; already bounded |

---

## Priority Order for Fixes

| Priority | Item |
|----------|------|
| **High** | §1.1 Delete `build_tube_mesh` + `blade_frame` (~180 dead lines) |
| **High** | §2.4 Stop calling `_collect_*` helpers twice; reuse results for OL |
| **High** | §2.5 Cache `build_grass_mask` result |
| **High** | §2.1 Route `grass/_geometry._sample_grid` through `core/grid.sample_grid` |
| **High** | §3.1 Pre-compute `g_R` / `norm` constants outside the blob loop |
| **Medium** | §2.2 Merge the two identical stamp-segment functions into `_geometry.py` |
| **Medium** | §2.3 Unify three `_collect_*` functions via generic helper |
| **Medium** | §3.3 Delete `xs_flat`/`ys_flat` no-op aliases |
| **Medium** | §3.4 Remove `* np.ones((1, 1, AZ))` from `wz` expression |
| **Medium** | §1.2 Delete `_system_output_path` (dead function) |
| **Medium** | §1.3 Delete `GrowingPath.last_stamp` (dead field) |
| **Medium** | §5.2 Delete `TileScene.support_z` alias, update stones.py call site |
| **Low** | §4.1 Vectorise `_build_blade_mesh` face construction |
| **Low** | §4.2 Vectorise `_make_ring_verts` ring construction |
| **Low** | §4.3 Vectorise `distance_taper` over whole path array |
| **Low** | §3.2 Cache terrain gradient in `TileScene` |
| **Low** | §5.1 Flatten `SolverConfig` into parent config |
| **Low** | §1.4 Fix stale `TileScene.from_config` docstring |
