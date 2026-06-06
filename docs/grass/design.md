# Grass Layer — Design Document

Design for the grass rewrite.  Describes *how* the system will be built.
References requirements in `requirements.md`.

---

## Overview

Generation is split into two passes with a clean data handoff between them.

```
Species config + RNG ──► Seeding ──► Pass 1: Grow ──► [GrassPath list] ──► Pass 2: Mesh build ──► meshes
                                      │                                         │
                                writes occ_z                            writes support_z
```

The heightmap is only touched during Pass 1.  Mesh geometry is only produced
during Pass 2.  Future post-processing (smoothing, analysis) lives in the gap.

---

## Data model

### `GrassSeed`

Created at planting time.  Immutable.  Carries everything the growth algorithm
needs — the growth loop never reads config or any direction field.

```python
@dataclass(frozen=True)
class GrassSeed:
    # Position
    x: float          # mm
    y: float          # mm

    # Growth
    direction: float  # radians — clump base direction + per-seed jitter
    step_len:  float  # mm — physical step size, >> cell_w
    n_steps:   int    # target step count (= target_length / step_len, ±variation)
    curl:      float  # radians added to direction per step

    # Blade geometry (for mesh build)
    width:      float  # mm — full width at base
    rise_cap:   float  # mm — max allowed rise per step
    species_id: str    # which SpeciesConfig to use for mesh building
```

### `GrassPath`

Output of Pass 1.  Input to Pass 2.  The handoff object.

```python
@dataclass
class GrassPath:
    seed:   GrassSeed
    points: list[tuple[float, float, float]]  # (x, y, z) spine positions, mm
    # points[0] is the root; points[-1] is the tip
```

A path with fewer than 2 points is discarded (blade never got off the ground).

### `SpeciesConfig`

Per-species template for seed creation and mesh building.

```python
@dataclass
class SpeciesConfig:
    name: str

    # Blade geometry ranges — sampled at seed creation time
    width_min:    float  # mm
    width_max:    float  # mm
    step_len:     float  # mm — fixed per species; must be >> cell_w
    length_min:   float  # mm
    length_max:   float  # mm
    curl_max:     float  # radians
    rise_cap:     float  # mm per step

    # Cross-section (for mesh build)
    cross_section: str   # 'flat' | 'leaf' | 'diamond' | ...
    thickness:     float # mm

    # Placement
    groups_per_square: int
    group_min:         int
    group_max:         int
    dir_jitter:        float  # radians σ — per-blade direction noise within group
    curl_jitter:       float  # radians σ — per-blade curl noise within group

    # Growth behaviour (pluggable — see Species section)
    grower: type   # class implementing the step() interface
```

### `GrassConfig`

Top-level config passed to `GrassLayer`.

```python
@dataclass
class GrassConfig:
    species:         list[SpeciesConfig]
    clearance:       float = 0.01   # mm — spine above floor on open terrain
    max_stack_height:float = 2.0    # mm — blade stops if occ_z - terrain_z > this
    seed:            int   = 0      # RNG seed
```

---

## Pass 1 — Growth

### Seeding

For each species, random group sites are placed inside the grass mask and the
mask is partitioned into nearest-site Voronoi-style clump cells.  Each group
samples one base direction and one base curl.  For each group cell,
`rng.integers(group_min, group_max+1)` seeds are planted:

1. Pick a random cell from the group's clump cell and jitter within that cell.
2. Reject if outside `grass_mask`, inside `stone_mask`, or `occ_z - terrain_z >
   max_stack_height`.
3. Create a `GrassSeed`:
   - `direction` = group direction + `rng.normal(0, dir_jitter)`
   - `curl` = group curl + `rng.normal(0, curl_jitter)`, clamped to the species
     curl range
   - `width`, `n_steps` = sampled from species ranges
   - `step_len` = species.step_len (fixed, not sampled)
4. Compute initial z: `z0 = max(terrain_z[ix,iy], occ_z[ix,iy]) + clearance`
5. Stamp the seed's footprint into `occ_z` immediately so subsequently planted
   seeds stack correctly.

### Growth loop

```
occ_z = scene.support_z.copy()

all_paths = [GrowingPath(seed, z0) for each planted seed]

for round in range(max_n_steps_across_all_seeds):
    for path in shuffle(all_paths):          # fixed order per run, shuffled once
        if path.alive and round < path.seed.n_steps:
            path.step(occ_z, scene)

return [p.to_grass_path() for p in all_paths if len(p.points) >= 2]
```

Processing order is shuffled once before the loop and reused every round —
stacking precedence is consistent across all rounds (REQ-OBS-2).
During growth, each path also carries only its most recent footprint stamp.
When the next footprint sample overlaps that last stamp and no other blade has
raised the same cells higher, sampling falls back to `scene.support_z`.  This
prevents a blade from climbing its immediately previous segment while still
allowing it to rise over stones and other blades.

### Per-step logic (flat grass species)

```
advance direction by seed.curl
tx, ty = current_pos + step_len * (sin(dir), cos(dir))

stop if (tx, ty) outside tile boundary  →  alive = False
stop if (tx, ty) outside grass_mask     →  alive = False

floor_z = sample_footprint_max(occ_z, tx, ty, seed.width, dir)
tz      = terrain_z_at(tx, ty)
nz      = max(tz, floor_z) + clearance

stop if nz > prev_z + seed.rise_cap     →  alive = False

append (tx, ty, nz) to path.points
stamp_footprint(occ_z, tx, ty, nz + blade_top_offset, seed.width, dir)
```

### Heightmap sampling and stamping

**Sampling** (`sample_footprint_max`):
Read the maximum `occ_z` value over a rectangle of width `seed.width` centered
at `(tx, ty)`, aligned perpendicular to the growth direction.  The rectangle
extends ±`seed.width/2` laterally.  In the forward direction, a point sample at
`(tx, ty)` is sufficient — the blade does not look backward.

This is conceptually a thin cross-section at the destination; full swept-area
sampling is unnecessary because the blade's leading-edge cross-section is the
only new territory being entered.

**Stamping** (`stamp_footprint`):
Write `nz + blade_top_offset` to all grid cells covered by the swept rectangle
from the previous spine position to `(tx, ty)` — width `seed.width`, length
`step_len`, aligned with the growth direction.  This records the full physical
extent of the newly placed segment so subsequent blades detect it correctly.

**Last-stamp self handling** (REQ-OBS-3):
Because the sampled footprint can overlap the immediately previous swept stamp,
each growing path tracks its last stamp only.  If cells in the next sample are
raised only by that last stamp, the sampler ignores them and uses the pre-grass
`scene.support_z` value.  Older self-crossings are treated like obstacles and
will usually stop the blade through the rise cap.

---

## Pass 2 — Mesh build

### V1 (no post-processing)

```python
def build_meshes(paths: list[GrassPath],
                 species_map: dict[str, SpeciesConfig]) -> list[trimesh.Trimesh]:
    meshes = []
    for path in paths:
        species = species_map[path.seed.species_id]
        mesh    = species.grower.build_mesh(path, species)
        if mesh is not None:
            meshes.append(mesh)
    return meshes
```

### Flat ribbon mesh (default species)

Given a `GrassPath` with N spine points:

1. **Embedded root** — prepend one point at `(x0, y0, terrain_z - root_depth)`
   and lower the first blade ring so its top face is at or below raw terrain.
   All four root-ring corners are therefore coincident with or below the terrain
   surface before the blade emerges upward.

2. **Width taper** — compute per-point width: full width for the first 81.25% of
   points, then taper to a sub-nozzle point at the tip using a cosine curve.
   The tip is not allowed to collapse to exact zero width because each blade
   must remain a closed solid for boolean union.

3. **Build ribbon** — for each consecutive pair of spine points, emit a quad
   (two triangles) for each face: top, bottom, left side, right side.  Cap the
   root and tip ends.

4. **Clamp XY** — clip all vertices to the tile footprint.

The ribbon is `blade_top_offset` thick (default 0.06 mm), matching the stamp
height so the occupancy grid and mesh are consistent.

### Future post-processing slot

Between `grow_all()` and `build_meshes()`, insert any path operations here:

```python
paths = grow_all(scene, cfg, rng)
# ── future: paths = smooth_paths(paths) ──
# ── future: paths = resample_paths(paths, n_points=50) ──
meshes = build_meshes(paths, species_map)
```

V1: this block is empty.

---

## Species and pluggable growth

Each `SpeciesConfig` references a **grower class** that implements two methods:

```python
class Grower:
    @staticmethod
    def step(path: GrowingPath, occ_z: np.ndarray, scene: TileScene) -> bool:
        """Advance path by one step.  Return True if alive, False if stopped."""
        ...

    @staticmethod
    def build_mesh(path: GrassPath, species: SpeciesConfig) -> trimesh.Trimesh | None:
        """Build the mesh for a completed path."""
        ...
```

**V1 ships one grower: `FlatGrassGrower`** — the step logic described above,
producing a flat horizontal ribbon.

Future growers could implement: branching reeds, curling fronds, standing sedge.
All share the same seeding, growth loop, and heightmap infrastructure.

---

## Integration into the dharmatile pipeline

### Interface

`GrassLayer` keeps the same external interface as today:

```python
class GrassLayer:
    def __init__(self, cfg: SceneConfig): ...

    def build(self,
              scene: TileScene,
              verbose: bool = True) -> list[trimesh.Trimesh]:
        ...
```

`terrains/tile.py` calls `grass_layer.build(scene, ...)` after earlier layers
have populated terrain, stone masks, grass masks, and `scene.support_z`.

### Internal build

```python
def build(self, scene, verbose):
    rng = np.random.default_rng(self.cfg.seed ^ GRASS_SALT)

    # Pass 1
    occ_z = scene.support_z.copy()
    paths = grow_all(scene, occ_z, self.cfg, rng)

    # (future post-processing here)

    # Pass 2
    meshes = build_meshes(paths, self.species_map)

    # Write final occupancy back to scene
    for path in paths:
        rasterise_path_into_support(scene.support_z, path)

    return meshes
```

### What changes vs the current implementation

| | Current | New |
|---|---|---|
| Step size | `cell_w` (0.14 mm) | `species.step_len` (e.g. 0.4 mm) |
| Self-trail | full `own_stamps` dict per blade | last-stamp tracking only |
| Stamping | Leading-edge transverse strip only | Full swept rectangle |
| Jitter | Period-2 Z oscillation from stamp aliasing | Eliminated by larger step |
| Mesh build | Interleaved with growth | Separate pass |
| Species | Single flat config | List of `SpeciesConfig` |
| Path smoothing | Gaussian on final path array | Dedicated slot between passes |
| Growable behaviors | Fixed flat ribbon only | Pluggable grower class |

---

## File layout

```
src/dharmatiles/
  grass/
    __init__.py
    config.py        GrassConfig, SpeciesConfig
    seed.py          GrassSeed, GrowingPath, GrassPath
    grow.py          grow_all(), plant_seeds(), stamp_footprint()
    mesh.py          build_meshes(), rasterise_path_into_support()
    growers/
      __init__.py
      flat.py        FlatGrassGrower (default)
    layer.py         GrassLayer — the .build() entry point
```

Replaces:
- `core/seed.py` → `grass/seed.py`
- `layers/grass.py` → `grass/layer.py` + `grass/grow.py` + `grass/mesh.py`

`core/seed.py` and `layers/grass.py` are deleted once the rewrite is complete.
`core/config.py` loses `GrassConfig`; the new one lives in `grass/config.py`.

---

## V1 scope

**In:**
- Two-pass pipeline (grow → mesh)
- `GrassSeed`, `GrassPath`, `GrowingPath` dataclasses
- `GrassConfig` with species list
- `FlatGrassGrower` — flat ribbon, same visual output as today
- Full swept-area stamping
- Last-stamp self-trail tracking only
- Single default species (flat grass)

**Out (future):**
- Path smoothing / resampling
- Additional grower types (reed, frond, etc.)
- Default species library
- Per-group species mixing
