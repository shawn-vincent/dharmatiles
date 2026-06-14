# Design Document: Procedural Deciduous Trees

**Date:** 2026-06-13  
**Status:** Draft — Phase 1 (trunk only) ready for implementation  
**Scope:** 3D-printable tree scatter-things for 28 mm tabletop terrain tiles

---

## 1  Goals

- Add `Trees` as a first-class scatter thing alongside `Rocks`, `Flowers`, and `Grass`.
- Start with **trunk only** (Phase 1). Branches come in Phase 2.
- The trunk should look like a dead or bark-heavy deciduous trunk: slightly bent
  spine, tapered girth, root flare at base, bark ridges running axially.
- All geometry must be **self-supporting** at standard FDM overhang limits (≤ 45°
  from vertical) — no support material should be required to print a trunk.
- The trunk is a **placed object**, not a heightmap stamp. It sits on top of
  `terrain_z` the way a rock does: one call per trunk, mesh returned, footprint
  stamped into `terrain_support_z` and `obstacle_mask`.
- Trees must be scatterable via the existing `Uniform` / `Grouped` placement
  infrastructure, and must be manually placeable at exact coordinates by the
  spec author if desired.

---

## 2  Background Research

### 2.1  Space Colonization Algorithm (Runions, Lane & Prusinkiewicz 2007)

The canonical reference for generating realistic-looking organic trees:
[Modeling Trees with a Space Colonization Algorithm (PDF)](https://algorithmicbotany.org/papers/colonization.egwnp2007.large.pdf)

**How it works:**

1. Fill a **crown volume** (ellipsoid, sphere, or arbitrary shape) with random
   attraction points — these model the resource gradients (light, auxin) a tree
   grows toward.
2. Maintain a set of active **branch tips**, initially just the root.
3. Each step: for every attraction point, find the nearest branch tip within a
   *perception radius* `d_i`. Add an influence vector from tip toward attractor.
4. Each tip moves one *segment length* `D` in the normalised sum of its influence
   vectors (plus a small tropism bias for gravity or wind).
5. Attractions within a *kill distance* `d_k` of any branch node are removed —
   they have been "colonised".
6. Repeat until all attractions are consumed or a maximum step count is reached.
7. Branch radii are assigned by da Vinci's pipe-model rule
   (see §2.3): radius² is conserved at every bifurcation.
8. Mesh: each skeleton segment is a truncated cone; segments sharing a node are
   blended (averaged normals at shared rings).

**Why this algorithm for deciduous trees:**

- Parameters map directly to visible crown shape: `crown_radius`, `crown_height`,
  `crown_offset` (where crown starts on trunk), `D` (segment fineness).
- Produces the characteristic irregular, light-seeking branching of broadleaf species
  automatically, without hand-tuning L-system rules per species.
- Python implementations are compact (~150 lines for the core loop).
- Well-studied; Blender's SpaceTree add-on and multiple GitHub repos provide
  reference implementations.

### 2.2  Weber & Penn (1995)

[Creation and Rendering of Realistic Trees (SIGGRAPH)](https://courses.cs.duke.edu/cps124/fall01/resources/p119-weber.pdf) —
[Arbaro implementation](https://arbaro.sourceforge.net/)

A statistical model with explicit branching-angle, taper, and curve parameters
per "level" (trunk, primary branch, secondary branch, …). Very configurable for
matching real species; the Arbaro XML presets cover dozens of tree species.

**Why we defer it:** Weber & Penn requires ~25 parameters per level and produces
great results for distant rendering but is harder to constrain for FDM printing.
Space colonization naturally produces FDM-friendly shapes because you control the
attraction volume to keep branches close to vertical. Weber & Penn needs per-species
angle fiddling to achieve the same.

We will borrow Weber & Penn's **taper formula** (radius decreases with a power law
along each segment) and its bark **Z-scale / undulation** parameters for texture.

### 2.3  Da Vinci's Pipe Model / Area Preservation

At every branching node in a real tree, the sum of cross-sectional areas of the
child branches equals the parent cross-section:

```
r_parent² = Σ r_child²
```

([Tree Branching: Leonardo da Vinci's Rule versus Biomechanical Models, PLOS One](https://journals.plosone.org/plosone/article?id=10.1371/journal.pone.0093535))

This is the most important formula for making procedural trees look believable.
It gives thick trunks that taper naturally into delicate outer twigs. We assign
radii bottom-up: leaf segments get `r_min`; each parent gets `sqrt(Σ child²)`.

### 2.4  FDM Overhang Constraints

A standard FDM printer without support can handle overhangs up to about 45° from
vertical (i.e., the surface normal tilts no more than 45° away from straight up).
Beyond 45°, filament bridges unsupported air and curls or fails.

For a **trunk** this is easy — a vertical tapered column has no overhang problem
at all. The challenge is in Phase 2 (branches). Broad deciduous branches often
depart the trunk at 50–70° from vertical in real life. Two strategies for Phase 2:

1. **Bend-up constraint**: apply a positive gravitropic tropism bias after the
   space colonization so all outer branch tips curve upward. Keeping terminal
   segments within 45° of vertical is achievable while still having a wide crown.
2. **Stub strategy** (simpler for Phase 1): major branch attachment points are
   rendered as short upward-pointing stubs on the trunk. The tabletop gamer
   reads the shape as "tree" without requiring full canopy geometry.

For Phase 1 (trunk only), the only overhang risk is the **root flare** — wide,
low-angled flanges at the base. These are kept at ≤ 45° from vertical by design
(they taper from trunk radius to floor over a fixed vertical height).

---

## 3  Trunk Geometry (Phase 1)

The trunk is modelled as a **swept cross-section** along a bent spine, analogous
to the rock system but oriented vertically. The bark cross-section, surface detail,
root flare, frame propagation, and mesh stitching rules now live in
[`design-bark.md`](design-bark.md); this section keeps only the Phase 1 trunk-specific
shape notes.

### 3.1  Spine Generation

```
base point  →  N_seg evenly-spaced control points  →  apex
```

The spine starts at `(cx, cy, terrain_z)`. Each step advances `segment_len_mm`
mostly upward, with a small random lateral perturbation (Gaussian, σ = `lean_mm`
per step). The cumulative lean is clamped to `lean_max_mm` from the base to keep
the trunk visually vertical but not robotically straight.

The spine is smoothed with a 1-pass Laplacian smooth (tension = 0.5) to remove
kinks introduced by the random walk. The result is a natural-looking slight lean or
gentle S-curve.

### 3.2  Cross-Section Profiles

At each spine point a **ring** of `az_segs` vertices is generated. See
[`design-bark.md` §3](design-bark.md#3--cross-section-ring) for the shared ring
model: elliptical cross-section, axial bark ridges, fine grain, and horizontal
wrinkles.

**Radius taper** (Weber & Penn power-law):

```
r(t) = r_base * (1 - t)^taper_power
```

where `t ∈ [0, 1]` is the fractional height along the trunk. `taper_power = 0.6`
gives a realistic deciduous trunk (broader at base, tapering quickly in the lower
third, more slowly in the upper two-thirds). This is adjustable.

### 3.3  Root Flare

The bottom of the trunk widens beyond the base taper to simulate buttress roots
and ground contact. The shared flare profile and sunk base cap are specified in
[`design-bark.md` §4](design-bark.md#4--root-flare) and
[`design-bark.md` §6.3](design-bark.md#63--base-cap).

### 3.4  Bark Texture: Horizontal Wrinkles

Horizontal wrinkles are part of the shared bark surface model; see
[`design-bark.md` §3.4](design-bark.md#34--horizontal-wrinkles).

### 3.5  Branch Stubs (optional in Phase 1)

Short outward protrusions can be added at randomly selected ring levels above
`stub_min_height_frac` of the trunk height. Each stub is a small tapered cone
(`stub_r_base`, `stub_r_tip ≈ 0`) pointing in a random lateral direction,
constrained to ≤ 45° from vertical (pointing slightly upward) so it is
self-supporting.

This is the visual hook that reads as "deciduous tree" even without a full
canopy. Stubs are off by default (set `n_stubs = 0`) for the cleanest trunk
silhouette.

### 3.6  Mesh Construction

1. Generate spine as a list of `N_seg + 1` points (including base and apex).
2. Build one ring of `az_segs` vertices at each spine point through the shared
   bark surface pipeline.
3. Stitch adjacent rings with quad pairs (two triangles each), cap the apex, cap
   the base, and call `fix_normals()`.
4. Merge vertices of all optional stubs into the main vertex array.

The current implementation-level mesh rules are specified in
[`design-bark.md` §6](design-bark.md#6--swept-ring-mesh-construction).

Vertex count estimate (trunk only, no stubs):

```
(N_seg + 1) * az_segs  +  2  (apex + base centre)
≈ 20 * 24 + 2  =  482 vertices / trunk
```

---

## 4  Phase 2: Branches (Future — Not Implemented Here)

Described briefly for architectural completeness.

1. After the trunk spine is built, identify the **trunk crown insertion point**
   (height `crown_start_frac` of trunk).
2. Fill an ellipsoidal **crown volume** centred at `(cx, cy, crown_z)` with
   `n_attractors` random points.
3. Run the space colonization algorithm from the trunk tip and any stub tips,
   growing into the crown volume. Apply a weak positive Z-tropism bias to keep
   branches trending upward (FDM constraint).
4. Post-process: prune branch segments whose angle from vertical exceeds
   `max_overhang_deg` (default 45°) — in practice the tropism prevents most
   violations.
5. Assign radii bottom-up via the pipe model (`r² preserved`), starting from
   `r_tip_mm` at terminals.
6. Mesh each branch segment as a truncated cone, blending shared rings at nodes.
7. Union all branch meshes with the trunk mesh.

---

## 5  Configuration

### 5.1  `TreeConfig` dataclass (`core/config.py`)

The original Phase 1 draft mixed trunk-shape fields and bark-surface fields in one
config. The shared bark fields now live in `BarkConfig`; see
[`design-bark.md` §7](design-bark.md#7--barkconfig-reference). The trunk-specific
shape fields are:

```python
@dataclass
class TreeConfig:
    """Deciduous tree trunk geometry.

    Phase 1: trunk only (no branches).
    """

    bark: BarkConfig

    # ── Trunk dimensions ──────────────────────────────────────────────────
    height_mm:          Sample[float] = D[20.0:45.0]   # total trunk height
    taper_power:        float         = 0.6             # radius taper exponent

    # ── Spine curvature ───────────────────────────────────────────────────
    n_seg:              int           = 18              # spine segments
    lean_mm:            float         = 1.2             # σ of per-step lateral noise
    lean_max_mm:        float         = 4.0             # hard clamp on cumulative lean

    # ── Branch stubs (optional) ───────────────────────────────────────────
    n_stubs:            int           = 3               # 0 = no stubs
    stub_min_height_frac: float       = 0.35            # don't stub below this fraction
    stub_length_mm:     Sample[float] = D[2.0:5.0]
    stub_r_base_mm:     Sample[float] = D[0.6:1.2]
    stub_angle_up:      Sample[float] = D[0.1:0.4]     # radians above horizontal (FDM safety)

    @property
    def height_mm_max(self) -> float:
        """Upper bound of the height distribution, used for footprint stamping."""
        from ..dist import bounds
        return float(bounds(self.height_mm)[1])
```

### 5.2  `PlantConfig` dataclass (future)

When Phase 2 branches are added, `TreeConfig` will embed a `BranchConfig` sub-object
controlling the space colonization parameters. This is not designed here.

---

## 6  Integration with dharmatiles

### 6.1  `Trees` scatter thing (`scatter/trees.py`)

```python
class Trees:
    """Scatter deciduous tree trunks into a region.

    Usage in a tile spec::

        from dharmatiles.scatter import Trees, Rocks, Grass
        from dharmatiles.layers import ScatterLayer

        ScatterLayer(
            Trees(height_mm=D[25:40], placement=Uniform(count_per_square=1)),
            Grass(...),
        )
    """
    def __init__(self, *, placement=None, **tree_kwargs): ...
    def footprint_mm(self) -> float: ...
    def scatter(self, scene, *, placement_mask, layer_idx=0) -> list[trimesh.Trimesh]: ...
```

The pattern is identical to `Flowers` and `Rocks`: sample positions from
`scatter_positions()`, build one mesh per position, stamp `terrain_support_z`
and `obstacle_mask`, return merged mesh list.

### 6.2  Stamping

The trunk footprint is an **ellipse** at the base, using the root radius and aspect
from `BarkConfig`, stamped to height `terrain_z + height_mm`. This prevents grass
from growing inside the trunk and forces subsequent `Grass` blades to steer around
the tree.

For visual correctness, the obstacle footprint should cover only the actual base
cross-section, not the full crown radius. The full tree will be tall, but grass
growing around the base of the trunk is physically reasonable (real trees have
grass at their feet).

### 6.3  Material tagging

```python
_tag(mesh, Material.WOOD)   # new material constant in core/color.py
```

`Material.WOOD` can be assigned a brown/grey colour for renderer previews.

### 6.4  Manual placement

For cases where the spec author wants a tree at a specific location:

```python
# In a tile spec — place one tree at the centre of the tile:
from dharmatiles.scatter.trees import _build_tree_mesh, _stamp_tree
...
# Inside a custom layer's apply():
tz = float(sample_grid(scene.terrain_z, surface, cx_arr, cy_arr)[0])
mesh = _build_tree_mesh(cx, cy, tz, angle, cfg, rng)
_stamp_tree(cx, cy, tz, cfg, scene.terrain_support_z, scene.obstacle_mask, surface)
return [mesh]
```

This is the same pattern used internally in `Flowers._build_flower_mesh` + `_stamp_flower`.

### 6.5  Placement strategies

`Trees` accepts `Uniform(count_per_square=N)` only (no `Grouped`). Trees are too
large to cluster meaningfully at the per-tile densities we use (0.5–2 per square).

Because a 35 mm tile is only 35 × 35 mm, a 4 mm base radius trunk already takes
up a significant fraction of one square. Expect `count_per_square = 0.5–2` in
typical specs (on a 2×2 or larger tile).

---

## 7  File Layout

```
src/dharmatiles/
  core/
    config.py          + TreeConfig dataclass
    color.py           + Material.WOOD constant
  scatter/
    trees.py           Trees scatter thing  (new file)
  trees/               tree mesh sub-pipeline  (new package)
    __init__.py        (empty)
    trunk.py           _build_tree_mesh, _stamp_tree, _build_trunk_spine, ...
src/tiles/
  soil+trees.tile.py   example spec  (new file)
```

The `trees/` sub-package mirrors the `grass/` structure: one module for the mesh
builder, one for the scatter thing in `scatter/`. If Phase 2 branches are added,
`trees/branches.py` (space colonization) and `trees/mesh.py` (skeleton-to-mesh)
go here.

---

## 8  Implementation Plan

### Phase 1A — Trunk mesh builder (`trees/trunk.py`)

1. Write `_build_trunk_spine(cx, cy, tz, cfg, rng) → list[Vec3]`  
   Random walk from base to apex; Laplacian smooth.

2. Write `_rings_from_spine(spine, cfg, rng) → list[ndarray(az_segs, 3)]`  
   Delegate cross-section detail, bark texture, flare, and caps to the shared bark
   surface model in [`design-bark.md`](design-bark.md).

3. Write `_stitch_rings(rings) → (vertices, faces)`  
   Quad-strip between adjacent rings; top fan; base disk cap.

4. Write `_build_stubs(spine, cfg, rng) → list[trimesh.Trimesh]`  
   Optional branch stubs; each is a simple tapered cone.

5. Write `_build_tree_mesh(cx, cy, tz, angle, cfg, rng) → trimesh.Trimesh`  
   Orchestrates steps 1–4; concatenates stubs; `fix_normals()`.

6. Write `_stamp_tree(cx, cy, tz, cfg, support_z, obstacle_mask, surface)`  
   Ellipse raster at base radius; sets `support_z` to `tz + height_max`.

### Phase 1B — Trees scatter thing (`scatter/trees.py`)

7. Write `Trees` class following the `Flowers` pattern exactly.  
   Add `TreeConfig` to `core/config.py`.  
   Add `Material.WOOD` to `core/color.py`.  
   Export `Trees` from `scatter/__init__.py`.

### Phase 1C — Example spec and STL regeneration

8. Write `src/tiles/soil+trees.tile.py` with 1–2 trees per 2×2 tile.
9. Regenerate all tile STLs (`for spec in src/tiles/*.tile.py; do dharmatiles-gen --spec "$spec"; done`).
10. Visually inspect trunk silhouette, bark texture, root flare, and base seal in
    PrusaSlicer (check watertight status and no overhang warnings on the trunk).

### Phase 2 — Branches (future, separate doc)

The space colonization loop and skeleton-to-mesh conversion are independent of
Phase 1 trunk geometry and will be added to `trees/branches.py` without changing
the Phase 1 interface.

---

## 9  Open Questions

| Question | Recommendation |
|---|---|
| Should the trunk hollow out for material savings at large scales? | Defer. Solid trunk is simpler and correct for miniature scale (3–5 mm base radius). |
| Should trees stamp a support_z column up to their full height? | **Yes.** This prevents grass from seeding above the trunk footprint, which is correct (grass grows around the base, not through the tree). |
| Root flare: separate mesh piece or part of trunk rings? | Shared bark surface concern; see [`design-bark.md` §4](design-bark.md#4--root-flare). |
| Should phase-drift on bark ridges use Perlin noise or a simple sum-of-sines? | Shared bark surface concern; see [`design-bark.md` §3.2](design-bark.md#32--axial-bark-ridges). |
| How many FDM overhang issues does a straight trunk actually have? | **None.** The trunk itself is essentially a vertical cylinder. Root flare constraints are handled by `BarkConfig`. |
| Can trees be used as region-filling objects (many per tile) or only as accents? | **Accent first.** At 35 mm/square scale, 2–4 trees per 2×2 tile is the right density. A forest tile spec could use higher density but would need thinner trunks. |

---

## 10  Key References

- Runions, Lane & Prusinkiewicz, [*Modeling Trees with a Space Colonization Algorithm*](https://algorithmicbotany.org/papers/colonization.egwnp2007.large.pdf), 2007
- Weber & Penn, [*Creation and Rendering of Realistic Trees*](https://courses.cs.duke.edu/cps124/fall01/resources/p119-weber.pdf), SIGGRAPH 1995
- Eloy, [*Leonardo's Rule, Self-Similarity, and Wind-Induced Stresses in Trees*](https://phys.org/news/2012-01-leonardo-da-vinci-tree.html), 2012
- Shinozaki et al. (pipe model) reviewed in [*Tree Branching: Leonardo da Vinci's Rule versus Biomechanical Models*](https://journals.plosone.org/plosone/article?id=10.1371/journal.pone.0093535), PLOS ONE 2014
- [Arbaro tree generator (Weber & Penn Java implementation)](https://arbaro.sourceforge.net/)
- [Jason Webb: Space Colonization Algorithm in JavaScript](https://medium.com/@jason.webb/modeling-organic-branching-structures-with-the-space-colonization-algorithm-and-javascript-6f683b743dc5)
