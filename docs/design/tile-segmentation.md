# Tile Segmentation Design

*Status: Draft — pre-implementation*

---

## Problem

Every tile currently has uniform terrain coverage: grass everywhere,
soil everywhere.  Real tabletop tiles are mixed — half grass / half
dirt path, a grass patch next to a cobblestone floor, a coast where
grass meets water, a wall running along one edge.

We need a way to divide a tile into named regions, assign content to
each region, and let the boundaries between them look organic rather
than geometrically sharp.

---

## Height model

`height_mm` is the **slab thickness** — the distance from the tile's flat
bottom face to the region's surface.  All regions share the same flat bottom
(so the DungeonBlocks peg base fits), and their surfaces sit at different
heights above it.

**Defaults by layer type** (used when `height_mm` is not specified on the region):

| Type | Default height | Surface relative to ground |
|---|---|---|
| `grass` / `soil` | 5.0 mm | reference level |
| `water` | 3.0 mm | 2 mm below ground |
| `floor` | 10.0 mm | 5 mm above ground |

Set `height_mm` on the region to override, e.g.:

```yaml
regions:
  deep_pool:
    contains: [0.75, 0.5]
    height_mm: 1.5    # 3.5 mm below ground — noticeably deeper than default
    layers:
      - type: water
```

When two adjacent regions have different heights, the boundary's slope
interpolates between them across the boundary strip's `width_mm`.

---

## Core model

A tile contains two kinds of things: **regions** and **boundaries**.

### Regions

A region is a contiguous area of the tile with its own content.
Content is defined by one or more **layer specs** — "packets" of
content sprayed into the region.

```toml
[[region]]
id       = "meadow"
contains = [0.25, 0.5]     # normalised (x, y), must lie inside this region

[[region.layer]]
type              = "grass"
groups_per_square = 200
# ... any GrassConfig param

[[region.layer]]            # second packet — sparse tall grass over the same area
type              = "grass"
groups_per_square = 40
blade_width_min   = 1.2
blade_width_max   = 1.6
blade_length_min  = 14.4
blade_length_max  = 14.4
```

A region with no `[[region.layer]]` entries is bare soil at base
elevation — the implicit default.

**Layer types and what they define**

| `type` | What it does | Key params |
|---|---|---|
| `grass` | Plants grass seeds in the region | All `GrassConfig` fields |
| `vegetation` | Future generalisation of grass (jungle plants, reeds…) | TBD |
| `water` | Sets region base elevation lower; generates water-surface mesh | `depth_mm` |
| `soil` | Explicit bare soil (mainly useful as a boundary layer) | — |
| `wall` | Generates raised solid geometry | `height_mm`, style TBD |

The layer type also determines the region's **base elevation**:
- `grass` / `soil` / `wall` → 0 mm (tile surface level)
- `water` → `−depth_mm` (default −2 mm)

Elevation is implicit in the layer type; you only override it when you
genuinely need a non-standard depth.

### Boundaries

A boundary is a curve that divides the tile into two regions.  It
starts at one point on the tile perimeter and ends at another.

```
  left edge, t=0.5  ──┐
                        ╲  (organic path)
                         ╲
                          └── right edge, t=0.5
```

Anchor points: `{ edge = "top|bottom|left|right", t = 0..1 }` where
`t` is the fractional position along that edge from the tile's
bottom-left origin corner.

**The boundary is itself a region.**  `width_mm` on the boundary gives it
physical extent.  A `layer` spec defines what terrain type fills that strip.
The slope between adjacent region heights is interpolated automatically
across the strip width.

```yaml
boundaries:
  shoreline:
    from: {edge: top,    t: 0.55}
    to:   {edge: bottom, t: 0.45}
    path: organic
    amplitude_mm: 4.0
    wavelength_mm: 12.0
    width_mm: 4.0        # ← width is on the boundary, not the layer
    layer:
      type: soil          # bare ramp, no seeds; height interpolated automatically
```

**`width_mm` defaults to 0** (zero-width dividing line).  Assigning a
`layer` to a zero-width boundary is an error — you cannot put content into
a strip with no extent.  A zero-width boundary between ground and water
produces a sharp cutoff (valid, just no slope).

**Boundary layer possibilities**

| Boundary layer type | Use case |
|---|---|
| *(none)* | Grass→soil, any flat-to-flat same-elevation boundary |
| `soil` | Slope between any two different-elevation regions |
| `water` | River or stream running along the boundary |
| `wall` | Low wall or kerb between two regions |
| `masonry` | Threshold/seam between paving and grass (future) |

---

## Path types

| `path` value | Description |
|---|---|
| `organic` | Straight-line baseline with band-limited perpendicular noise, tapered to zero at both anchors so the path always hits exactly the specified perimeter points |
| `straight` | Exact straight line — use for walls, water-level cuts |
| `waypoints` | Explicit list of interior `[x, y]` (normalised) points; organic noise can be layered on top |

---

## Transitions

### Grass ↔ soil (zero-width boundary)
Stop planting seeds at the boundary.  Blades near the edge follow the
flow field and can lean across — looks natural.  No slope needed
because both regions are at the same elevation.

### Grass/soil ↔ water (soil slope boundary)
The water region sits at `−depth_mm`.  The boundary layer (`type =
"soil"`, `width_mm = N`) generates a continuous slope from soil
elevation to water elevation across its width.  No grass seeds are
planted in the boundary strip.  Existing blades seeded in the grass
region can lean down onto the slope naturally; the rise_cap mechanism
will stop them before they dive underwater.

### Any two different-elevation regions
The slope is always owned by the boundary layer.  Adjacent regions
don't know about each other's elevations — only the boundary does.
This means future combinations (water-to-wall, two water levels, etc.)
work without special-casing.

### Grass ↔ wall
Wall region blocks all other layers (stone mask + region mask logic
run before grass seeding).  Boundary is zero-width unless you want a
visible seam layer.

---

## Tile spec file format (TOML)

```toml
# tiles/water+grass.tile  (formerly coast-left.tile)

[surface]
cols = 1
rows = 1
seed = 7

# ── Regions ────────────────────────────────────────────────────────

[[region]]
id       = "meadow"
contains = [0.25, 0.5]

[[region.layer]]
type              = "grass"
groups_per_square = 240

# ── optional second seed packet (sparse tall blades) ──
[[region.layer]]
type              = "grass"
groups_per_square = 30
blade_width_min   = 1.3
blade_width_max   = 1.7
blade_length_min  = 14.4
blade_length_max  = 14.4


[[region]]
id       = "lake"
contains = [0.75, 0.5]

[[region.layer]]
type     = "water"
depth_mm = 2.0


# ── Boundaries ─────────────────────────────────────────────────────

[[boundary]]
id   = "shoreline"
from = { edge = "top",    t = 0.55 }
to   = { edge = "bottom", t = 0.45 }
path = "organic"
amplitude_mm  = 4.0
wavelength_mm = 12.0

[[boundary.layer]]
type     = "soil"    # bare slope, no grass seeds
width_mm = 4.0       # height interpolates grass-elevation → water-elevation
```

### Backwards compatibility

`generate-tile-stl` with no `--spec` flag keeps the current behaviour
(all defaults, full-coverage grass, no segmentation).  With
`--spec path/to/foo.tile` it reads the spec and applies regions and
boundaries over the base defaults.

---

## More examples

### Half grass / half soil (no elevation change, zero-width boundary)

```toml
[surface]
cols = 1
rows = 1
seed = 42

[[region]]
id       = "meadow"
contains = [0.25, 0.5]

[[region.layer]]
type              = "grass"
groups_per_square = 240

[[region]]
id       = "dirt"
contains = [0.75, 0.5]
# no [[region.layer]] → bare soil

[[boundary]]
id   = "edge"
from = { edge = "top",    t = 0.48 }
to   = { edge = "bottom", t = 0.52 }
path = "organic"
amplitude_mm  = 3.0
wavelength_mm = 10.0
# no [[boundary.layer]] → zero-width line, no slope needed
```

### Corner grass (one quadrant)

```toml
[surface]
seed = 99

[[region]]
id       = "patch"
contains = [0.15, 0.15]    # bottom-left corner

[[region.layer]]
type = "grass"
groups_per_square = 240

[[region]]
id       = "floor"
contains = [0.75, 0.75]
# bare soil — no layer

[[boundary]]
id   = "corner-cut"
from = { edge = "left",   t = 0.5 }
to   = { edge = "bottom", t = 0.5 }
path = "organic"
amplitude_mm  = 2.5
wavelength_mm = 8.0
```

### Three sides grass (all except top-right corner)

```toml
[surface]
seed = 201

[[region]]
id       = "meadow"
contains = [0.2, 0.5]      # large region — left/bottom/most of top

[[region.layer]]
type = "grass"
groups_per_square = 240

[[region]]
id       = "exposed"
contains = [0.9, 0.9]      # top-right corner — bare soil

[[boundary]]
id   = "corner-cut"
from = { edge = "top",   t = 0.65 }
to   = { edge = "right", t = 0.65 }
path = "organic"
amplitude_mm  = 2.0
wavelength_mm = 7.0
```

### Future: river boundary (water running between two grass banks)

```toml
[[region]]
id       = "bank-left"
contains = [0.15, 0.5]

[[region.layer]]
type = "grass"

[[region]]
id       = "bank-right"
contains = [0.85, 0.5]

[[region.layer]]
type = "grass"

[[boundary]]
id   = "river"
from = { edge = "top",    t = 0.5 }
to   = { edge = "bottom", t = 0.5 }
path = "organic"
amplitude_mm  = 5.0
wavelength_mm = 14.0

[[boundary.layer]]
type     = "water"
width_mm = 8.0
depth_mm = 1.5
# slope from each bank down to water level is automatic
```

---

## Implementation order

### Phase 1 — Infrastructure + segmented grass/soil

1. `RegionMask` (`core/region.py`) — per-cell region ID array
2. `BoundaryDef` rasteriser — organic / straight path → cell mask
3. `SegmentationSpec` + TOML loader (`core/spec.py`)
4. `TileScene` gets `region_mask`
5. `GrassLayer.build()` skips seeds outside grass regions
6. `StonesLayer.build()` skips stones outside soil/grass regions
7. `generate-tile-stl --spec foo.tile`

### Phase 2 — Water

8. Boundary layer rasteriser (strips with widths)
9. `terrain_z` pre-step: lower water-region cells; generate slope in
   boundary strip
10. `WaterLayer` — flat mesh at depth elevation, VisCAM colour
    `(30, 80, 160)`

### Phase 3 — Walls

11. `WallLayer` — solid geometry; blocks other layers in its region

### Phase 4 — Polish

12. Multiple seed packets per region (Phase 1 plumbing supports it;
    just needs the loop in `build_tile`)
13. Named pattern shortcuts (`pattern = "half"`, `"corner"`, …)
14. Waypoint path type
15. Multi-tile edge matching

---

## Open questions

**Q1: `contains` units — normalised or mm?**
Using `[0..1, 0..1]` normalised coordinates means specs are
tile-size-independent (a `soil+grass-corner.tile` works for 1×1 and 3×3).
Using mm is more intuitive when hand-authoring coastlines.
*Leaning toward normalised; add a note in the doc that `[0.5, 0.5]`
is tile centre.*

**Q2: Boundary strip width — centred or one-sided?**
`width_mm = 4.0` — is that 2 mm each side of the path, or 4 mm on the
downhill side only?  Centred is simpler; one-sided gives more control
over where the slope starts on the upper region.  Centred for now.

**Q3: Grass leaning across into slope**
Blades seeded in the grass region can grow toward the slope and lean
down it — the rise_cap should stop them from diving into the water.
Confirm with real geometry in Phase 2.

**Q4: Multi-tile (3×3) boundary continuity**
Boundaries are in tile-local coordinates; adjacent tiles in a set need
their edge anchor points to match.  This is an authoring / tooling
problem, not a generation problem.  Defer.

**Q5: Closed-loop boundaries (pond in the middle)**
Requires different rasterisation (flood-fill from outside rather than
from `contains`).  Defer to Phase 4.
