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
    blade_segment_length: float  # mm — physical step size, >> cell_w
    n_steps:   int    # target step count (= target_length / blade_segment_length)
    curl:      float  # radians added to direction per step

    # Blade geometry (for mesh build)
    blade_width: float  # mm — full width at base
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
    blade_width_min:      float  # mm
    blade_width_max:      float  # mm
    blade_length_min:     float  # mm
    blade_length_max:     float  # mm
    blade_segment_length: float  # mm — fixed per species; must be >> cell_w
    blade_curl_min:       float  # radians, magnitude
    blade_curl_max:       float  # radians, magnitude
    blade_smooth:         float  # 0=grown path, 1=best-fit base-to-tip arc
    rise_cap:             float  # mm per step

    # Cross-section (for mesh build)
    cross_section: str   # 'flat' | 'leaf' | 'diamond' | ...
    thickness:     float # mm

    # Placement
    groups_per_square: int
    group_min:         int
    group_max:         int
    group_dir_jitter:  float  # radians σ — per-blade direction noise within group

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
samples one base direction.  For each group cell, `rng.integers(group_min,
group_max+1)` seeds are planted:

1. Pick a random cell from the group's clump cell and jitter within that cell.
2. Reject if outside `grass_mask`, inside `stone_mask`, or `occ_z - terrain_z >
   max_stack_height`.
3. Create a `GrassSeed`:
   - `direction` = group direction + `rng.normal(0, group_dir_jitter)`
   - `curl` = sampled magnitude from `blade_curl_min` to `blade_curl_max`, with random sign,
     divided across the derived step count
   - `blade_width` = sampled from `blade_width_min` to `blade_width_max`
   - `n_steps` = sampled target length divided by `blade_segment_length`
   - `blade_segment_length` = species.blade_segment_length (fixed, not sampled)
4. Compute initial z: `z0 = max(terrain_z[ix,iy], occ_z[ix,iy]) + clearance`
5. Stamp the seed's footprint into `occ_z` immediately so subsequently planted
   seeds stack correctly.

### Growth loop

```
occ_z = scene.support_z.copy()

all_paths = [GrowingPath(seed, z0) for each planted seed]

# Sort downstream-first: highest projection onto own direction comes first.
all_paths.sort(key=lambda p: p.seed.x * sin(p.seed.direction)
                              + p.seed.y * cos(p.seed.direction),
               reverse=True)

for round in range(max_n_steps_across_all_seeds):
    for path in all_paths:          # same downstream-first order every round
        if path.alive and round < path.seed.n_steps:
            path.step(occ_z, scene)

return [p.to_grass_path() for p in all_paths if len(p.points) >= 2]
```

**Processing order** is sorted once before the loop and reused every round —
stacking precedence is consistent across all rounds (REQ-OBS-2).

**Why downstream-first?**  The sort key projects each seed's `(x, y)` position
onto its own initial growth direction unit vector `(sin(dir), cos(dir))`.  Seeds
with a higher projection sit further "downstream" in the direction this blade is
already heading.  Growing them first means their occ_z stamps exist when upstream
blades later grow into the same area, so upstream blades rise to cross over
downstream ones rather than the reverse.

The returned `GrassPath` list preserves this sorted order, so `build_meshes`
automatically processes blades in the same downstream-first sequence.  A single
sort therefore governs both the growth phase and the mesh-lift phase, keeping
the two passes consistent.

During growth, each path also carries only its most recent footprint stamp.
When the next footprint sample overlaps that last stamp and no other blade has
raised the same cells higher, sampling falls back to `scene.support_z`.  This
prevents a blade from climbing its immediately previous segment while still
allowing it to rise over stones and other blades.

### Per-step logic (flat grass species)

```
advance direction by seed.curl
tx, ty = current_pos + blade_segment_length * (sin(dir), cos(dir))

stop if (tx, ty) outside tile boundary  →  alive = False
stop if (tx, ty) outside grass_mask     →  alive = False

floor_z = sample_footprint_max(occ_z, tx, ty, seed.blade_width, dir)
tz      = terrain_z_at(tx, ty)
nz      = max(tz, floor_z) + clearance

stop if nz > prev_z + seed.rise_cap     →  alive = False

append (tx, ty, nz) to path.points
stamp_footprint(occ_z, tx, ty, nz + blade_top_offset, seed.blade_width, dir)
```

### Heightmap sampling and stamping

**Sampling** (`sample_footprint_max`):
Read the maximum `occ_z` value over a rectangle of width `seed.blade_width` centered
at `(tx, ty)`, aligned perpendicular to the growth direction.  The rectangle
extends ±`seed.blade_width/2` laterally.  In the forward direction, a point sample at
`(tx, ty)` is sufficient — the blade does not look backward.

This is conceptually a thin cross-section at the destination; full swept-area
sampling is unnecessary because the blade's leading-edge cross-section is the
only new territory being entered.

**Stamping** (`stamp_footprint`):
Write the blade-top height to all grid cells covered by the swept rectangle from
the previous spine position to `(tx, ty)` — width `seed.blade_width`, length
`blade_segment_length`, aligned with the growth direction.  The height is
**slope-aware**: each cell receives a value linearly interpolated between
`prev_z + blade_top_offset` and `nz + blade_top_offset` based on the cell's
position along the segment, so `occ_z` reflects the actual sloped surface of the
proposed path rather than a flat stamp at the endpoint height.  The per-cell z
values are stored in `last_stamp` so own-trail detection continues to work correctly.

**Last-stamp self handling** (REQ-OBS-3):
Because the sampled footprint can overlap the immediately previous swept stamp,
each growing path tracks its last stamp only.  If cells in the next sample are
raised only by that last stamp, the sampler ignores them and uses the pre-grass
`scene.support_z` value.  Older self-crossings are treated like obstacles and
will usually stop the blade through the rise cap.

---

## Pass 2 — Mesh build

Meshes are built in downstream-first order (the same order established by the
growth sort).  For each path the loop does three things:

```python
for path in paths:                              # downstream-first order
    # 1. Adjust every point against the current accumulated surface.
    #    Interior points: raise only.  Tip: snap to surface (up or down).
    floor = support_z_at(x, y)
    lifted = [(x, y, max(z, floor))  for x,y,z in path.points[:-1]]
            + [(tip_x, tip_y, support_z_at(tip_x, tip_y))]
    lifted_path = GrassPath(seed=path.seed, points=lifted)

    # 2. Build the mesh from the adjusted path.
    mesh = grower.build_mesh(lifted_path, species, scene, surface)

    # 3. Update support_z from the actual mesh contours (slope-aware).
    rasterise_sloped(scene.support_z, lifted_path, species.thickness)
```

**Interior points — raise only.**  `max(planned_z, floor_z)` ensures a blade
always lies at or above whatever has already been meshed beneath it.

**Tip — snap to surface.**  The tip is pinned to `support_z` at its XY
regardless of the planned z — up if something sits beneath it, down if the blade
was planned to float above empty terrain.  This prevents tips from hovering in
mid-air and eliminates the upward hook at the end of blades that grew into open
space.

**Why downstream-first order?**  Downstream blades are stamped into `support_z`
before upstream blades are meshed.  When an upstream blade's points are adjusted,
downstream surfaces are already present, so the blade rises smoothly to cross
them rather than lying flat and then spiking up later.

**Why slope-aware rasterisation?**  Each segment of the blade top surface has a
slope in Z.  Stamping a constant (endpoint) Z over the entire footprint of a sloped
segment over-estimates the floor for blades that cross the low end of the segment.
The slope-aware stamp interpolates linearly between the two endpoint blade-top heights
so subsequent geometry interacts with a faithful surface representation.

### Flat ribbon mesh (default species)

Given a `GrassPath` with N spine points:

1. **Optional smoothing** — blend the grown path toward a best-fit quadratic
   arc through the path points.  `blade_smooth = 0` leaves the path unchanged;
   `blade_smooth = 1` uses the fitted arc.  The base and tip positions remain
   pinned.

2. **Embedded root** — prepend one point just below terrain based on blade
   thickness and lower the first blade ring so its top face is at or below raw terrain.
   All four root-ring corners are therefore coincident with or below the terrain
   surface before the blade emerges upward.

3. **Width taper** — compute per-point width: full width for the first 81.25% of
   points, then taper to a sub-nozzle point at the tip using a cosine curve.
   The tip is not allowed to collapse to exact zero width because each blade
   must remain a closed solid for boolean union.

4. **Build ribbon** — for each consecutive pair of spine points, emit a quad
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
| Step size | `cell_w` (0.14 mm) | `species.blade_segment_length` (e.g. 0.8 mm) |
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
