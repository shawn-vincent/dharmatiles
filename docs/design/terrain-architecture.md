# Terrain Generator Architecture Specification

**Version:** 0.2  
**Status:** Active — current implementation scope

---

## 1. Tile Model

### 1.1 Base Tile Unit

- One tile unit: **35 mm × 35 mm**
- The working surface is specified as an input: **T_cols × T_rows tile units**
- Example: a 2 × 2 surface = 4 tile units = 70 mm × 70 mm physical

### 1.2 Logical Grid

The grid is **128 × 128 cells per tile unit**, uniform in both axes.

| Surface | Grid dimensions | Cell size |
|---|---|---|
| 1 × 1 tile | 128 × 128 | 0.273 mm |
| 2 × 2 tiles | 256 × 256 | 0.273 mm |
| T × T tiles | 128T × 128T | 0.273 mm |

Cell size is always **35 / 128 ≈ 0.273 mm** regardless of surface size.  
Total grid dimensions scale with the number of tile units.

### 1.3 Grid Cell Properties

```ts
interface Cell {
  terrainType: TerrainType;
  surfaceHeight: number;   // always defined
}
```

### 1.4 Terrain Types (Current Scope)

| Type | Notes |
|---|---|
| `water` | |
| `ground` | |
| `grass` | |
| `constructed_floor` | |
| `wall` | |
| `high_wall` | |
| `highest_wall` | |
| `embedded_stl` | future use |

No additional types are defined at this stage.

---

## 2. Base Mesh Generation

### 2.1 Purpose

The base mesh represents the **undisturbed terrain surface** before any:

- plants
- decorative elements
- scatter
- water effects

### 2.2 Inputs

- Grid of terrain types
- Per-cell surface height

### 2.3 Outputs

- Continuous surface mesh
- Derived entirely from terrain type, height, and transition rules

---

## 3. Height Model

### 3.1 Surface Height Invariant

Every grid cell has a defined surface height. This height is used by all
subsequent systems.

### 3.2 Height Resolution

Heights are resolved during base mesh generation. Terrain transitions
determine how heights blend between adjacent cells.

---

## 4. Terrain Transitions

Transitions between adjacent cells determine vertical shape.

### 4.1 Hard Transitions (Vertical Edges)

| Transition | Result |
|---|---|
| wall → lower terrain | vertical drop, sharp edge, no smoothing |
| ground edge (where appropriate) | same |
| constructed floor edge (where appropriate) | same |

### 4.2 Soft Transitions (Sloped Edges)

| Transition | Result |
|---|---|
| ground → water | smooth S-shaped slope (shoreline) |

No other soft transitions are defined at this stage.

### 4.3 Slope-Normal Placement

The base terrain heightmap stores true world-Z heights, so soft transitions
are already geometrically sloped. Detail systems currently treat local terrain
as world-horizontal unless documented otherwise.

Future systems that place oriented features on a slope must use a terrain
normal derived from the heightmap gradient. See
`docs/design/slope-normal-requirements.md` for the current contract and the
list of layer changes required before grass, stones, vegetation, support
cones, or normal-displaced soil detail are placed in transition zones.

---

## 5. XY Boundary Shape (Planar Domain)

Terrain boundaries define geometry in the XY plane independently of height.

### 5.1 Wall Boundaries

- Prefer straight edges and square corners
- Diagonals are allowed
- Smooth curves are possible but not a focus
- Current behavior: determined directly from grid layout; limited to straight
  or diagonal geometry

### 5.2 Ground Boundaries

- Organic, curved
- Smooth transitions in XY
- Coastline variability achieved by assigning terrain types across cells to
  form organic shapes

### 5.3 Floor Boundaries

- Smooth curves allowed
- More structured than ground but still curved

---

## 6. Terrain Authoring Model

Terrain shape is **not sculpted directly**. Instead:

- The grid is populated with terrain types
- Geometry emerges from terrain type, height, and transition rules

---

## 7. Generation Pipeline

```
semantic terrain grid
  → base mesh (height + transitions)
  → (future layers — see §9)
```

---

## 8. Plant System (Current Scope: Grass Only)

### 8.1 General Model

- Plants are seeded onto terrain
- Growth occurs after base mesh generation

### 8.2 Grass

Grass is the only defined plant type.

#### 8.2.1 Seed Self-Containment

**Each seed carries everything it needs to grow.** The growth algorithm takes
a single seed as input and produces geometry — it requires no external lookup,
no shared mutable state, no reference to other seeds.

This means all variability is baked into the seed at seeding time:

| Parameter | Per-seed | Notes |
|---|---|---|
| position | ✓ | world-space location on terrain |
| direction | ✓ | initial growth direction angle |
| curve | ✓ | lateral curvature (sign + magnitude) |
| length | ✓ | total blade arc-length |
| base_diameter | ✓ | width at base |
| tip_length | ✓ | length of tapering tip section |
| curve_start | ✓ | fraction of length before bend begins |
| power | ✓ | cross-section falloff exponent |

#### 8.2.2 Config and Variability

A **grass config** defines ranges for all seed parameters. At seeding time,
each seed samples from those ranges independently, producing a fully-specified
seed struct. The config is not retained after seeding — the seed is the
authoritative record.

Prefer **copying config into each seed** over shared references. Per-seed
copies are simpler to reason about, trivially serializable, and eliminate
any risk of shared-state mutation during growth. The memory cost (a few dozen
floats per seed) is negligible.

Multiple seeds may be generated from the same config (e.g. a clump), but each
resulting seed is independent.

#### 8.2.3 Growth Behavior

Given a fully-specified seed, grass:

- grows along a directional curve
- generates points along its growth path
- avoids obstacles and other grass
- may spread slightly or grow onto adjacent levels
- final output is reduced to smooth curves consistent with the existing blade
  algorithm (see `scripts/BLADE_DESIGN.md` and `scripts/blade.py`)

---

## 9. Future Systems (Out of Current Scope)

The following are acknowledged but **not part of the current implementation**.
They must not be assumed when building present systems.

### 9.1 Additional Plants

- Examples: vines, broadleaf plants
- Each uses a distinct growth algorithm
- Interaction and ordering rules exist conceptually but are unspecified

### 9.2 Water System

- Planned: procedural ripples, randomness-based variation, disturbance around
  objects
- Current placeholder: a water layer is an explicit blue cap over water cells.
  Terrain top quads inside the water mask are omitted so the water cap replaces
  them instead of duplicating coplanar ground faces.
- Current limitation: a water region has one height value, used as both the
  terrain depression height and the water surface height. A visible dip,
  basin, or depth under water requires separate bed and surface heights.

### 9.3 Object / STL Placement

- Includes embedded STL objects and rocks
- Planned behaviors: occupy space, protrude from terrain, influence nearby
  systems
- Current rocks/stones are opt-in in `.tile` specs. A region must include an
  explicit `rock`, `rocks`, `stone`, or `stones` layer for stone scatter to run
  there. Boundary strips can also declare those layer types. The placement
  region constrains rock centres only; rock geometry may extend across region
  edges, but not off the tile.

### 9.4 Gravel

- Defined conceptually: surface texture on ground, optional individual rocks
- No implementation details included

---

## 10. Core Principles

### Semantic First

The grid defines *what exists*. Geometry is derived, not authored.

### Deterministic Emergence

The mesh is a pure function of terrain types, heights, and transition rules.
Same input → same output.

### Universal Height Field

Surface height is defined everywhere. All systems depend on it.

### Layer Separation

Strict boundaries between:

1. Terrain definition (grid)
2. Mesh generation (base mesh)
3. Growth systems (current: grass)
4. Future detail systems (§9)

Nothing in layer N may depend on implementation details of layer N+1 or later.
