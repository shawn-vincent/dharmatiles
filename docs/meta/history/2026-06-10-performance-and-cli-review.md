# Performance & CLI review — 2026-06-10

A focused review after the A–Q architectural refactor sprint.  The code is
cleaner than it has ever been — but some of the refactors traded performance
for clarity, and the command-line output is stuck in 2019.  This document
covers both.

---

## Part 1 — Performance

### Where time actually goes

The pipeline runs in this order; the heavy items are annotated.

```
load_spec            — negligible
build_region_mask    — moderate (Bresenham; see §P3)
_build_spec_terrain  — fast (distance_transform_edt × N regions)
  for each area layer:
    GrassCarpet.apply      ← §P1  CRITICAL
    SoilCarpet.apply       — moderate (soil blobs; §P4 helps here)
    Scatter(Rocks)         — moderate (vectorised; §P5 loop)
    Scatter(Grass)         — moderate-heavy (segment growth loop; §P6)
make_heightmap_solid       — fast (adaptive Delaunay)
trimesh.boolean.union      ← §P1 feeds the part count here
system.export              — fast
```

---

### P1 — GrassCarpet builds one Trimesh per blade (CRITICAL)

**Likely the primary regression from the recent refactor.**

`GrassCarpet` was rewritten from a heightmap-stamping layer to a tube-mesh
layer in order to share `FlatGrassGrower.build_mesh()` with the 3-D grass
pass.  The new path does, for every carpet blade seed:

```python
# layers/grass_carpet.py line 124
for seed in seeds:
    mesh = _build_carpet_blade_mesh(scene, surface, seed, cfg, placement_mask)
    if mesh is not None:
        parts.append(mesh)
```

Each `_build_carpet_blade_mesh` calls `FlatGrassGrower.build_mesh()`, which
calls `_build_blade_mesh()`, which ends with:

```python
mesh.merge_vertices()                           # vertex hash + dedup
mesh.update_faces(mesh.area_faces > 1e-12)      # area per face
mesh.remove_unreferenced_vertices()
mesh.fix_normals()
```

At `groups_per_square=240` on a 1×1 tile, you get ~200–400 carpet blade seeds.
That's 200–400 round-trips through four trimesh housekeeping methods, producing
200–400 individual `Trimesh` objects that are then passed to
`trimesh.boolean.union()`.  Each additional part given to the boolean
union multiplies its internal split-and-repair cost.

The old `GrassCarpetLayer` stamped blade heights into `terrain_z` with
`np.maximum()` slices — O(cells per blade), no trimesh objects, no union
pressure.

**Fix options (pick one or combine):**

**Option A — Batch-concatenate before returning.**
Carpet blades don't need to be watertight volumes; they're embossed texture.
Concatenate all blade meshes into a single non-boolean mesh before returning:

```python
if parts:
    return [trimesh.util.concatenate(parts)]
return []
```

The union step passes non-volume parts straight through
(`trimesh.util.concatenate`) and skips the expensive manifold boolean for them.
This alone should cut union time dramatically.

**Option B — Drop the per-blade trimesh cleanup.**
Each blade is already created with `process=False` by `_build_blade_mesh`.
Skip `merge_vertices` and its siblings for carpet blades — they are texture, not
printable standalone parts; a few duplicate vertices don't matter.

**Option C — Revert carpet to heightmap stamping.**
Return an empty parts list from `GrassCarpet.apply()` and stamp everything into
`terrain_z` (as the old layer did).  The tube meshes were added to get
better blade edge quality; if Option A is fast enough visually, the extra mesh
quality isn't worth the cost.

**Recommendation:** Try Option A first — it's a one-liner that keeps the
quality gain.  If the union is still slow after batching, add Option B.

---

### P2 — `jitter_grid_xy` — Python generator for group membership (significant)

In `scatter/distribute.py` lines 308 and 362:

```python
group_set = frozenset(zip(group_rows.tolist(), group_cols.tolist()))
...
valid = np.fromiter(
    ((int(iy), int(ix)) in group_set
     for iy, ix in zip(iys.tolist(), ixs.tolist())),
    dtype=bool, count=len(iys),
)
```

The `frozenset` lookup is O(1) per query but the iteration is a Python
generator: for a group of 500 cells generating 200 candidates, that's 200
Python iterations with tuple construction.  Vectorise it:

```python
# Build a boolean membership grid once per group
in_group = np.zeros((surface.grid_h, surface.grid_w), dtype=bool)
in_group[group_rows, group_cols] = True

# Test all candidates at once
iys_clipped = np.clip(iys.astype(int), 0, surface.grid_h - 1)
ixs_clipped = np.clip(ixs.astype(int), 0, surface.grid_w - 1)
valid = in_group[iys_clipped, ixs_clipped]
```

No Python loop, no frozenset construction.  For a tile with many small Voronoi
groups this may not dominate, but it removes the only remaining pure-Python
loop in the hot scatter path.

---

### P3 — Bresenham rasterisation — 4000 Python iterations per boundary (significant)

`core/region.py` `_rasterise` loops over consecutive path samples:

```python
for i in range(len(cols) - 1):
    _bresenham(mask, r0, c0, r1, c1)   # itself a Python while loop
```

With `n_samples=4000` (the default), that's 3999 calls to `_bresenham`, each
running a Python while loop.  For a 256×256 grid where consecutive samples are
~0.1 cells apart, `_bresenham` almost always steps zero or one cell — yet each
call still pays the full Python call overhead and loop setup.

**Fix:** Since we already have dense samples (4000 per boundary), consecutive
sample pairs are guaranteed to be ≤ 1 cell apart.  Skip Bresenham entirely:

```python
def _rasterise(path_mm, surface, mask):
    cols = np.clip((path_mm[:, 0] / surface.cell_w).astype(int),
                   0, surface.grid_w - 1)
    rows = np.clip((path_mm[:, 1] / surface.cell_w).astype(int),
                   0, surface.grid_h - 1)
    mask[rows, cols] = _BOUNDARY   # one vectorised write; no loop at all
```

If you later want to support very coarse grids (few cells per boundary) where
gaps could form, add a check: if `max(|Δrow|, |Δcol|) <= 1` everywhere, use
the vectorised form; otherwise fall back to Bresenham.  At typical resolutions
(256+ cells per tile, 4000 path samples) the condition is always satisfied.

---

### P4 — `dist.sample()` allocates a `Constant` wrapper on every plain-float call (moderate)

`dist.py` `sample()`:

```python
def sample(value, rng, size=None):
    return as_distribution(value).sample(rng, size)

def as_distribution(value):
    if hasattr(value, 'sample') and hasattr(value, 'bounds'):
        return value
    return Constant(value)          # allocates a new dataclass every time
```

For a plain `float` (the most common case), every `sample()` call:
1. Calls `hasattr(float, 'sample')` → False
2. Calls `hasattr(float, 'bounds')` → False
3. Allocates `Constant(value)` (a frozen dataclass)
4. Calls `Constant.sample()` → `return self.value`

In tight loops — `_compute_bump_field` calling `sample()` for sigma and height
of each blob; `_make_seed` sampling six parameters per grass blade; `_make_seed`
sampling four parameters per rock — this wastes Python object allocation budget.

**Fix: one-line short-circuit:**

```python
def sample(value, rng, size=None):
    if isinstance(value, (int, float)):
        return value if size is None else np.full(size, value, dtype=float)
    return as_distribution(value).sample(rng, size)
```

`isinstance(float, (int, float))` is free (C-level type check).  No allocation.
No change in semantics.  The `bounds()` free function benefits from the same
pattern.

---

### P5 — Rock support_z rasterisation — N Python loop iterations (moderate)

`layers/rocks.py` lines 274–309 stamp each rock's elliptical footprint into
`terrain_support_z` with:

```python
for s in range(N):
    ...
    II, JJ = np.meshgrid(ii_g, jj_g)   # per-rock allocation
    d2 = (lx_g / _rx)**2 + (ly_g / _ry)**2
    ...
    np.maximum(sl, z_top, out=sl)
```

This is inherently serial (each rock's bounding box overlaps a different region
of `support_z`) so full vectorisation is hard.  But the allocations inside the
loop — `meshgrid`, the intermediate arrays — can be reduced by preallocating
scratch buffers at the maximum rock bounding box size once per call.

For N = 15 rocks per square on a 3×3 tile (135 rocks), the current approach is
fine.  If someone cranks rocks to 100+ per square on a large tile, revisit.

---

### P6 — Grass segment growth is inherently sequential (known; limited fix)

`grow_all` in `grass/grow.py` grows one segment at a time per blade, in
upstream order.  Each `grower.step()` reads and writes `occ_z` (a shared
occupancy grid), so blade N's segments must finish before blade N+1 starts.
This cannot be fully vectorised.

What *can* be done:
- The `_sample_footprint_max` → `_leading_edge_cells` path inside each step
  constructs small NumPy arrays.  These are already fast relative to the
  per-blade overhead; no change needed.
- The Python loop over blades (hundreds of iterations) is the irreducible cost.
  If this becomes a bottleneck, consider reducing `groups_per_square` for 3D
  grass (it's already at 24 in the default spec; carpet at 240 is the heavier
  layer).

---

### P7 — `_build_blade_mesh` cleanup on every 3D grass blade (minor)

Same cleanup as §P1, but for 3D grass blades via `FloppyGrassLayer`.  With a
typical 3D grass density (~50–150 blades per 1×1 tile), the overhead is smaller
than for the 200–400 carpet blades, but still present.  The `merge_vertices` /
`update_faces` calls exist to clean up collapsed tip rings (which produce
duplicate vertices).  Could be made conditional:

```python
if np.any(widths < 1e-6):   # only if a tip ring actually collapsed
    mesh.merge_vertices()
    mesh.update_faces(mesh.area_faces > 1e-12)
    mesh.remove_unreferenced_vertices()
mesh.fix_normals()
```

Since most blades taper to a point (all of them, by design), `any(widths < 1e-6)`
is almost always True — this doesn't save much for grass blades.  It would
help for carpet blades if Option A (batching) from §P1 is adopted, since the
concatenated mesh doesn't need per-blade cleanup.

---

### Suggested implementation order

| Priority | Item | Expected gain |
|---|---|---|
| **1** | §P1 Option A — batch-concatenate carpet blades | Large — cuts union input from N parts to 1 |
| **2** | §P4 — short-circuit `sample()` for plain floats | Small but free; one line |
| **3** | §P3 — vectorise `_rasterise` | Moderate; removes the Bresenham loop |
| **4** | §P2 — vectorise `jitter_grid_xy` membership | Small-moderate |
| **5** | §P1 Option B — skip per-blade trimesh cleanup | Small; makes §P1 complete |

---

## Part 2 — CLI

### Where things stand

The current output is functional but spartan:

```
=== Building tile from spec (1x1 squares, grid 256x256) ===
  Regions:    ['meadow', 'dirt']
  Boundaries: ['margin']
Building terrain solid...
Computing union  (45/45 solid parts)...
  vertices: 12,345   faces: 23,456   watertight   1.2s
Building db base and exporting...
Saved -> stl/dungeonblocks/1x1-soil+grass-db.stl
```

Problems:
- No color — no way to distinguish info, success, and warnings at a glance.
- No live progress — the terminal is frozen for 30–90 seconds with no feedback.
- No per-step timing — you can't tell if the 60-second wait is in the union or
  the grass.
- The batch summary is a single line; you have to scroll to find per-tile stats.
- `===` and `─` headings are inconsistent.
- Mesh stats (vertices, faces, watertight) appear on the same line as timing,
  which is hard to scan.

---

### Recommendation: adopt `rich`

`rich` (pip: `rich`) is the de-facto standard for beautiful Python CLIs in 2026.
It's already a transitive dependency of many common Python tools, and adds ~2 MB
to the install.  Add it to `pyproject.toml`:

```toml
dependencies = [
    "numpy",
    "trimesh",
    "scipy",
    "pyyaml",
    "manifold3d",
    "rich",
]
```

`rich` gives you panels, spinners, progress bars, colored tables, and markup —
all without touching terminal escape codes by hand.

---

### What the output should look like

#### Single tile (`generate-tile-stl --spec src/tiles/soil+grass.tile.py`)

```
┌─────────────────────────────────────────────────────────────────┐
│  soil+grass  ·  1×1  ·  256×256 grid                           │
│  regions: meadow, dirt   ·   boundaries: margin                 │
└─────────────────────────────────────────────────────────────────┘
  ✓ Region mask            0.08s
  ✓ Terrain heightmap      0.04s
  ✓ meadow: GrassCarpet    1.23s   (312 blades)
  ✓ meadow: Scatter        2.11s   (22 rocks, 48 blades)
  ✓ dirt: SoilCarpet       0.51s
  ✓ Terrain solid          0.34s   62 K verts · 118 K faces
  ✓ Union  [db]            4.82s   ● watertight   71 K · 137 K
  ✓ Base + export [db]     0.12s   → stl/dungeonblocks/1x1-soil+grass-db.stl
  ✓ Union  [ol]            3.91s   ● watertight   58 K · 112 K
  ✓ Base + export [ol]     0.09s   → stl/openlock/1x1-soil+grass-ol.stl
────────────────────────────────────────────────────────────────────
  Total  9.3s
```

- Each step shows a green `✓` on completion (spinner while running).
- Timing in grey; fast = grey, slow (>5s) = yellow.
- Mesh stats in a compact single line.
- Watertight in green `●`; not-watertight in red `✗`.

#### Batch (`generate-tile-stl`)

```
  Batch: 7 specs  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 3/7  soil+grass-corner [db union]

  ┌──────────────────────┬─────┬────────┬────────┬──────────┬─────────┐
  │ Spec                 │  s  │ Verts  │ Faces  │ Wtight   │ Files   │
  ├──────────────────────┼─────┼────────┼────────┼──────────┼─────────┤
  │ grass                │  7s │  48 K  │  93 K  │ ✓ ✓      │ db / ol │
  │ grass-carpet         │  5s │  41 K  │  79 K  │ ✓ ✓      │ db / ol │
  │ soil+grass           │ 12s │  71 K  │ 137 K  │ ✓ ✓      │ db / ol │
  │ soil+grass-corner    │ 11s │  68 K  │ 131 K  │ ✓ ✓      │ db / ol │
  └──────────────────────┴─────┴────────┴────────┴──────────┴─────────┘
```

The progress bar shows the spec-level position, plus the current step within
the active tile in the label.  The table accumulates rows as each spec finishes.

---

### Implementation sketch

The orchestrator needs a lightweight "reporter" protocol that the rest of the
pipeline calls to emit progress:

```python
class TileReporter:
    """Receives pipeline events; default implementation uses rich."""
    def tile_begin(self, name: str, surface, regions, boundaries): ...
    def step_begin(self, label: str): ...
    def step_end(self, label: str, elapsed: float, detail: str = ""): ...
    def export_done(self, suffix: str, path: Path,
                    mesh: trimesh.Trimesh, elapsed: float): ...
    def tile_done(self, elapsed: float): ...
    def batch_done(self, n: int, elapsed: float): ...

class SilentReporter(TileReporter):
    """Does nothing — for --quiet."""
    ...

class RichReporter(TileReporter):
    """Uses rich.live + rich.progress + rich.table."""
    ...
```

The current `verbose: bool` flag throughout the codebase gets replaced by
passing a reporter instance.  The existing `print()` calls move into the
reporter's methods, so the core logic never touches string formatting directly.

Keeping the `--quiet` flag: `--quiet` selects `SilentReporter`.
A new `--verbose` flag (or remove `--quiet` and invert) could expose a third
`VerboseReporter` that dumps more detail per step.

---

### Color/icon scheme

| Meaning | Icon | Color |
|---|---|---|
| Spinner (in-progress) | `⠙` (animated) | cyan |
| Success | `✓` | green |
| Warning | `⚠` | yellow |
| Error / not watertight | `✗` | red |
| Info / dim detail | — | grey / dim |
| File path | — | blue underline |
| Step timing, fast (<2s) | — | dim white |
| Step timing, medium (2–10s) | — | yellow |
| Step timing, slow (>10s) | — | red |

---

### What *not* to do

- **Don't add a full TUI** (textual, urwid, etc.) — `generate-tile-stl` is a
  batch tool piped into other tools; it should stay line-oriented and log-safe.
  `rich` with `force_terminal=False` gracefully degrades to plain text when
  stdout is not a TTY.
- **Don't log to a file by default** — STL generation is fast enough that a
  post-hoc log is rarely needed.  A `--log FILE` flag can be added later.
- **Don't hide the numbers** — vertex and face counts are useful for diagnosing
  mesh quality regressions.  Keep them visible in non-quiet mode.

---

## Suggested rollout order

1. **§P4** (`sample()` short-circuit) — one line, zero risk, ship immediately.
2. **§P3** (`_rasterise` vectorisation) — replaces a function, easy to test by
   comparing region masks before/after.
3. **§P1** (batch-concatenate carpet blades) — the highest-value performance
   change; measure union time before and after.
4. **§P2** (`jitter_grid_xy` vectorisation) — clean NumPy rewrite of one
   function.
5. **CLI rewrite** — add `rich` dependency, implement the reporter protocol,
   replace all `print()` calls in `terrains/tile.py` and layer `.apply()` paths.
   Largest change surface, but entirely isolated to output code.
6. **§P7** (conditional per-blade cleanup) — polish; do after §P1 proves out.
