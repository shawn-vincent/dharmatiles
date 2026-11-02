# DRY Code Review — 2026-06-13

**Scope:** Full codebase DRY audit. All Python source files under `src/dharmatiles/`
(~10,300 lines). No changes from previous reviews are under review; this is a fresh
DRY-focused sweep.

---

## Findings (ranked by severity)

### 1. `manifold_to_trimesh` conversion — 4 copies, no helper

**Severity: HIGH**

The 5-line pattern that converts a `manifold3d.Manifold` (or its `.to_mesh()` result)
to a `trimesh.Trimesh` appears in four places with zero variation:

| File | Lines |
|---|---|
| `bases/dungeonblocks.py` | 135–140 |
| `bases/openlock.py` | 142–147 |
| `terrains/tile.py` | 274–279 |
| `layers/water.py` | 338–340 (slightly condensed variant) |

```python
msh  = manifold_obj.to_mesh()
mesh = trimesh.Trimesh(
    vertices=np.array(msh.vert_properties, dtype=float)[:, :3],
    faces=np.array(msh.tri_verts, dtype=int),
    process=False,
)
mesh.fix_normals()
```

**Fix:** Extract `_manifold_to_trimesh(m: Manifold) -> trimesh.Trimesh` into
`core/mesh.py` (already the mesh-utility module) and import from the four call sites.

---

### 2. `bases/dungeonblocks.export()` vs `bases/openlock.export()` — body identical

**Severity: HIGH**

The last 8 lines of both `export()` functions are character-for-character identical:

```python
_tag(base_mesh, Material.BASE)
all_meshes = [base_mesh] + list(colored_meshes)

combined = trimesh.util.concatenate(all_meshes)
output_path.parent.mkdir(parents=True, exist_ok=True)
export_color_stl(combined, output_path)

return combined, all_meshes
```

The only divergence is how `base_mesh` is produced (DungeonBlocks: `make_base(surface, peg_h, base_cfg)`; OpenLOCK: `make_base(surface)`).

**Fix:** A shared helper `_export_with_base(base_mesh, colored_meshes, output_path)` 
in `bases/__init__.py` (or a new `bases/_common.py`) handles the common tail. Each
`export()` constructs its own `base_mesh` then delegates.

---

### 3. Config `values`-dict + setattr loop — 3 identical config `__init__` endings

**Severity: HIGH**

`SpeciesConfig`, `SoilConfig`, and `RocksConfig` all end their `__init__` with the
same boilerplate:

```python
values = { "field": value, ... }
for field_name, value in values.items():
    object.__setattr__(self, field_name, value)   # SpeciesConfig (frozen)
    # or setattr(self, field_name, value)          # SoilConfig / RocksConfig
```

This is pure mechanical repetition — the dict exists only because `frozen=True`
prevents normal assignment in SpeciesConfig. The other two use it for consistency
but don't need to.

**Fix:** 
- SpeciesConfig: convert to `@dataclass(frozen=False)` and use a normal `__post_init__`, 
  or add a `_init_frozen(self, **kwargs)` helper that calls `object.__setattr__` in a loop
  (one place instead of three).
- SoilConfig / RocksConfig: drop the `values` dict entirely; assign fields directly.

---

### 4. Step-timing triplet — 8 sites in `terrains/tile.py`, no context manager

**Severity: MEDIUM**

Every timed pipeline step in `_build_tile_mesh` and `build_tile_from_spec` manually
repeats:

```python
reporter.step_begin(label)
t0 = _time.perf_counter()
# ... work ...
elapsed = _time.perf_counter() - t0
reporter.step_end(label, elapsed[, detail])
```

This triplet appears at lines 335–339, 345–349, 352–362, 381–418, 426–434, 476–479,
504–507, and similar (8+ instances in tile.py).

**Fix:**

```python
@contextlib.contextmanager
def _timed(reporter, label, detail_fn=None):
    reporter.step_begin(label)
    t0 = _time.perf_counter()
    yield
    elapsed = _time.perf_counter() - t0
    reporter.step_end(label, elapsed, detail_fn() if detail_fn else "")
```

Usage:
```python
with _timed(reporter, "Terrain solid"):
    terrain_mesh = make_heightmap_solid(...)
```

---

### 5. Cosine edge-fade formula — 7 inlined copies across 4 files

**Severity: MEDIUM**

The Hann/cosine smoothstep `0.5 * (1.0 − cos(π·t))` is written inline at:

| File | Line | Variant |
|---|---|---|
| `layers/grass_carpet.py` | 179 | full clipped form |
| `layers/soil.py` | 103 | full clipped form in `apply()` |
| `layers/soil.py` | 259 | X axis in `_compute_bump_field()` |
| `layers/soil.py` | 260 | Y axis in `_compute_bump_field()` |
| `layers/water.py` | 178 | `make_water_displacement()` |
| `layers/water.py` | 318 | ripple tile-edge fade |
| `layers/water.py` | 326 | ripple mask-boundary fade |

**Fix:** A one-liner `hann_fade(dist, width)` in `core/grid.py` (already a
pure-array utilities module):

```python
def hann_fade(dist: np.ndarray, width: float) -> np.ndarray:
    """Smooth [0,1] weight — 0 at boundary, 1 at width inside."""
    return 0.5 * (1.0 - np.cos(np.pi * np.clip(dist / max(width, 1e-9), 0.0, 1.0)))
```

---

### 6. `.stem.replace('.tile', '')` — 5 inline copies when `_spec_stem()` already exists

**Severity: MEDIUM**

`terrains/tile.py` already has `_spec_stem(spec_path, tiles_root)` which strips the
`.tile.py` suffix. But five call sites skip it and do the cheap inline:

```
line  95:  spec_name = spec_path.stem.replace('.tile', '')
line 1141: name    = args.spec.stem.replace('.tile', '')
line 1192: [sp.stem.replace('.tile', '') for sp in spec_paths]
line 1205: sp.stem.replace('.tile', '')
line 1239: spec_name = sp.stem.replace('.tile', '')
```

**Fix:** Extract `_spec_name(spec_path) -> str` (the first element of `_spec_stem()`
with a dummy tiles root), and use it at all five sites.

---

### 7. Seed-derivation pattern in `Rocks.scatter()` and `Grass.scatter()`

**Severity: MEDIUM**

Both `scatter/prototype.py` methods derive their per-layer seed identically:

```python
# Rocks.scatter lines 59-62:
rng_seed = derive_seed(surface.seed, 'rocks-scatter', layer_idx) ^ self.placement.seed
rng = np.random.default_rng(rng_seed)

# Grass.scatter lines 134-136:
seed = derive_seed(surface.seed, 'grass-scatter', layer_idx) ^ self.placement.seed
```

The only difference is the type-name string. Any third scatter type (`Debris`, `Flowers`)
would duplicate this again.

**Fix:** A module-level helper `_scatter_rng(surface, placement, kind, layer_idx)` or a
mixin method on a base `ScatterThing` class.

---

### 8. `for tile in load_spec(): ... break` anti-pattern — 3 sites

**Severity: LOW**

Three sites load a spec just to extract the first tile, using a `for...break` loop instead
of `next()`:

- `_batch_worker()` line ~131
- `_batch_worker()` line ~138
- `main()` sequential path line ~1269

```python
for tile in load_spec(sp):
    # use tile once
    break
```

**Fix:** `tile = next(iter(load_spec(sp)), None)` is clearer and eliminates the misleading
loop. A `_first_tile(spec_path)` helper wrapping this makes intent explicit.

---

### 9. `blob_sigma` triangular validation duplicates `_range_compat` structure

**Severity: LOW**

`SoilConfig.__init__` handles `blob_sigma` with its own 12-line block (lines 328–340)
that re-implements the conflict-check, both-or-neither check, and coercion logic from
`_range_compat` — but adds triangular-mode support. The validation boilerplate is
identical; only the coercion differs.

**Fix:** Extend `_range_compat` with an optional `mode` parameter:
```python
def _range_compat(name, value, old_min, old_max, default, mode=None):
    ...
    if mode is not None and mode >= old_min:
        return D.triangular(old_min, mode, old_max)
    return D[old_min:old_max]
```

---

### 10. PNG path + render call pairs — 3 near-identical call sites

**Severity: LOW**

```python
out = _png_path_for(spec_path, tile, tiles_root, png_root)
_render_from_meshes(meshes, out, sq_mm, quiet, label=_label_for_png(out, png_root))
```

This 2-line pair appears at lines ~131, ~1161, and ~1270. Each site must remember to
call both helpers in sequence and thread `out` through.

**Fix:** A `_render_png(spec_path, tile, tiles_root, png_root, meshes, sq_mm, quiet)` 
wrapper encapsulates the sequence.

---

## Summary Table

| # | Description | Files | Sites | Severity |
|---|---|---|---|---|
| 1 | `manifold_to_trimesh` conversion | 4 files | 4 | HIGH |
| 2 | `bases/*/export()` body duplication | 2 files | 2 | HIGH |
| 3 | Config `values`-dict + setattr loop | `core/config.py` | 3 | HIGH |
| 4 | Step-timing triplet, no context manager | `terrains/tile.py` | 8 | MEDIUM |
| 5 | Cosine edge-fade formula inlined | 4 files | 7 | MEDIUM |
| 6 | `.stem.replace('.tile', '')` bypass | `terrains/tile.py` | 5 | MEDIUM |
| 7 | Seed-derivation in Rocks/Grass scatter | `scatter/prototype.py` | 2 | MEDIUM |
| 8 | `for tile in load_spec(): break` loop | `terrains/tile.py` | 3 | LOW |
| 9 | `blob_sigma` duplicates `_range_compat` structure | `core/config.py` | 1 | LOW |
| 10 | PNG path + render call pairs | `terrains/tile.py` | 3 | LOW |
