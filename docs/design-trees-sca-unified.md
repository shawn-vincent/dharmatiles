# Design Document: Unified Branching Tree (Follow-up to design-trees.md)

**Date:** 2026-06-14  
**Status:** Exploratory — pre-implementation design  
**Scope:** Rethinking the trunk/branch split as a single recursive branching structure  
**Predecessor:** `docs/design-trees.md`

---

## 1  The Problem With Two Systems

The current implementation (`trees/trunk.py` + `trees/branches.py`) has a hard
conceptual split:

| | Trunk | Branches |
|---|---|---|
| Algorithm | Parametric swept cross-section | SCA skeleton |
| Surface | Rich: taper, flare, bark ridges, wrinkles | Plain: frustum cones only |
| Radius model | Power-law formula | da Vinci pipe model |
| Mesh topology | Continuous ring-stitched solid | Concatenated overlapping cones |
| Input | `(cx, cy, tz, angle, cfg)` | `(apex_pos, apex_dir, trunk_spine)` |

The trunk is built first; branches are then attached to it. The junction between
them is an arbitrary cut at `sca_trunk_root_frac` of the trunk spine — a seam
where a sculpted parametric surface hands off to a pile of cones.

This split is philosophically wrong. A real tree has **no such distinction** in
its self-similar structure. The trunk is simply the oldest, thickest branch — the
one that grew first and accumulated the most da Vinci pipe-model cross-section. If
the bark ridges, taper, and surface character work at trunk scale, they should
still work (at reduced size) on primary branches, and — below FDM resolution — be
automatically suppressed on the twigs.

**Goal of this document:** design a single branching system where the trunk
emerges naturally as the fattest member of a unified skeleton, all segments share
the same surface model, and there is no code distinction between "trunk" and
"branch."

---

## 2  Core Insight: SCA Already Produces a Trunk

The Space Colonization Algorithm needs no help to grow a trunk. When attractors
are placed *only inside the crown ellipsoid* (above some minimum Z), the skeleton
grows a nearly-vertical path from the ground root upward toward the attractor
cloud before any branching happens. This single path **is** the trunk; it emerged
from the algorithm rather than being hand-built separately.

The key behaviour:

1. Root node sits at `(cx, cy, terrain_z)`.
2. All attractors are above `crown_base_z` — nothing below draws the skeleton
   sideways near the ground.
3. The single root tip marches upward (vertical tropism + no lateral competition)
   until it enters the perception radius of the crown.
4. Once inside the crown, attractors appear on multiple sides simultaneously,
   causing the first branching events.
5. The da Vinci pipe model then assigns the pre-crown single path a radius equal
   to `sqrt(Σ r_child²)` over all downstream branches — naturally the fattest part
   of the skeleton.

**The trunk is the inevitable result of SCA starting below the crown.** We do not
need to build it separately.

---

## 3  What the Trunk Module Contributes That SCA Loses

The current trunk has three things the pure SCA skeleton lacks:

### 3.1  Surface character

Bark ridges, wrinkles, elliptical cross-section, slow twist. These are properties
of the *surface swept along the skeleton*, not of the skeleton itself. The SCA
produces only a node graph; the trunk code converts that graph to a mesh using a
rich swept-ring model. The branch code converts the same graph to plain frustum
cones.

**Fix:** make the swept-ring model universal. Every skeleton edge, regardless of
whether it is "trunk" or "branch," gets the same surface treatment — just scaled
to its radius.

### 3.2  Root flare

The low-radius buttress flare at ground level. This is a *surface property*
applied near terrain contact, not a branching-growth rule. The exact implemented
profile lives in [`design-bark.md` §4](design-bark.md#4--root-flare).

**Fix:** flare belongs in the shared bark surface builder, not in a separate trunk
module.

### 3.3  Spine lean and curvature

The trunk has a random-walk spine with Laplacian smoothing, giving a gentle S-bend
or lean. SCA also produces this naturally — its random attractor placement means
the skeleton doesn't grow perfectly vertical.

The trunk's `lean_max_mm` hard-clamp is an extra safety for printability that
isn't needed if the FDM Z-tropism in SCA is tuned correctly. They solve the same
problem with different tools.

**Fix:** tune SCA tropism and per-step perturbation to produce the same lean
without a separate spine module.

---

## 4  Unified Architecture

### 4.1  Single skeleton

```
root (cx, cy, terrain_z)
    │ SCA grows upward — no attractors below crown_base_z
    │ → this vertical path IS the trunk
    ├─── branch A (primary)
    │        ├── branch A1 (secondary)
    │        └── branch A2 (secondary)
    └─── branch B (primary)
             └── ...
```

One run of SCA from one root produces the complete skeleton. No separate trunk
spine. No hand-off at `sca_trunk_root_frac`.

#### Multi-root option

Some trees have multiple base stems (coppiced growth, split trunk). Support by
seeding 2–3 root nodes within a small radius of `(cx, cy, terrain_z)`, each
slightly offset and angled. This is the same `root_positions` array SCA already
accepts — it already supports multi-root; we just move the roots from upper spine
points to the actual ground.

### 4.2  Universal surface model: swept rings

Every skeleton edge is rendered as a **swept cross-section** rather than a plain
frustum. The cross-section is the same for trunk and branches — elliptical, with
bark ridges and wrinkles — but the detail level is gated on the local radius.

The implemented surface model is specified in [`design-bark.md`](design-bark.md),
especially [`§2`](design-bark.md#2--scale-adaptive-detail) for radius thresholds.
This unified tree document is responsible only for the skeleton that feeds that
surface builder.

### 4.3  Frenet frames along full paths, not per-edge

The current trunk uses parallel-transport frames along the entire spine, giving
continuous ridge phase. The unified skeleton requires the same continuity along
each root-to-tip path, not just per-edge.

Frame propagation and arc-distance ridge phase are specified in
[`design-bark.md` §5](design-bark.md#5--frame-propagation-parallel-transport-bisector)
and [`§3.2`](design-bark.md#32--axial-bark-ridges).

### 4.4  Root flare

Root flare is a surface property, not an SCA-growth property. It is applied by
the shared bark builder to low rings near `terrain_z`; see
[`design-bark.md` §4](design-bark.md#4--root-flare).

### 4.5  Junction geometry

At a bifurcation, two children branch from one parent. The overlapping frustum
approach (current branches.py) is FDM-safe because slicers union closed shells.
For the unified model, the implemented strategy is the simple swept-ring junction
described in [`design-bark.md` §6.5](design-bark.md#65--junction-geometry).

---

## 5  Apical Dominance: How to Get a Clear Trunk

Pure SCA without modification produces bushy structures — all tips compete
equally, so the skeleton fans out uniformly into the attractor cloud. Real trees
have **apical dominance**: the central upward-growing leader suppresses lateral
branching until it reaches a certain height.

Several techniques to simulate this in SCA:

### 5.1  Attractor exclusion zone (preferred)

Seed no attractors below `crown_base_z = terrain_z + trunk_min_height_mm`. The
root grows straight upward (only tropism operates) until it enters the attractor
field. Below the crown, there is no lateral competition, so the path stays nearly
straight. This is already partially in the current implementation but controlled
by the pre-built trunk spine rather than by attractor placement.

This is the cleanest approach: **the trunk forms automatically as the
attractor-free path below the crown.** No special-case code.

### 5.2  Variable perception radius with height

Give lower skeleton nodes a smaller perception radius, so they "see" fewer
attractors and branch less eagerly. Upper nodes have larger perception radius and
branch more. This models the real gradual increase in light-seeking as the tree
reaches the canopy.

Adds one extra parameter (`perception_r_base`, `perception_r_crown`) but gives
more control over where branching begins.

### 5.3  Directional growth bias early-on

Apply a stronger vertical tropism for the first `N_trunk_seg` steps, then reduce
it. This gives a straight early path before the crown is reached. Simpler than
variable perception radius but less physically motivated.

---

## 6  Bark Surface Continuity Across Bifurcations

The unified skeleton must provide `arc_dists`, the cumulative path distance from
root to each node. The bark builder uses that value for ridge phase and twist so
surface detail continues from trunk into branches. The detailed formula lives in
[`design-bark.md` §3.2](design-bark.md#32--axial-bark-ridges).

---

## 7  FDM Constraints in the Unified Model

### 7.1  Overhang

The FDM overhang constraint (≤ 45° from vertical) is enforced the same way as in
the current `branches.py`: clamp SCA growth directions to non-negative Z, and
apply positive vertical tropism. In the unified model this applies to every
segment, including the trunk equivalent — but the trunk path naturally stays
vertical anyway because of the attractor exclusion zone.

### 7.2  Minimum printable radius

The `branch_min_r_mm` threshold in `BarkConfig` prevents generating swept-ring
geometry below the printable branch radius. The exact scale-adaptive behavior is
specified in [`design-bark.md` §2](design-bark.md#2--scale-adaptive-detail).

### 7.3  Root seal

The root node receives a closed base cap sunk to `terrain_z - sink_mm`; effective
leaf nodes receive top caps. See [`design-bark.md` §6.3](design-bark.md#63--base-cap)
and [`§6.4`](design-bark.md#64--top-fan).

---

## 8  Configuration Changes

### 8.1  Parameters that go away

| Current parameter | Why removed |
|---|---|
| `TreeConfig.height_mm` | Height is now determined by the attractor cloud placement and SCA growth |
| `TreeConfig.n_seg` | No separate trunk spine |
| `TreeConfig.lean_mm`, `lean_max_mm` | SCA tropism + attractor placement handles lean |
| `BranchConfig.sca_trunk_root_frac` | No trunk spine to fraction |
| `build_trunk()` / `build_branches()` separation | Replaced by `build_tree()` |

### 8.2  New parameters

The unified tree config should hold SCA skeleton parameters plus a `BarkConfig` for
all surface parameters. The bark fields themselves are specified in
[`design-bark.md` §7](design-bark.md#7--barkconfig-reference).

```python
@dataclass
class TreeConfig:
    bark: BarkConfig

    # ── Crown placement (controls trunk height) ───────────────────────────
    crown_base_z_mm:    Sample[float] = D[15.0:30.0]  # height of crown bottom above terrain
    crown_rx:           Sample[float] = D[8.0:14.0]   # crown ellipsoid half-widths
    crown_ry:           Sample[float] = D[8.0:14.0]
    crown_rz:           Sample[float] = D[6.0:12.0]
    crown_offset_z:     Sample[float] = D[-2.0:4.0]   # vertical shift of crown centre

    # ── SCA ───────────────────────────────────────────────────────────────
    n_attractors:       int           = 180
    sca_segment_mm:     float         = 2.0
    sca_perception_r:   float         = 9.0
    sca_kill_r:         float         = 4.0
    sca_max_steps:      int           = 80
    sca_tropism:        float         = 0.25            # vertical bias (same as before)
    sca_branch_xy_std:  float         = 0.30
    sca_min_branch_att: int           = 3
```

The trunk-specific parameters (`height_mm`, `n_seg`, `lean_mm`, `lean_max_mm`,
`n_stubs`, `stub_*`) are dropped because their roles are taken over by the SCA
configuration and the scale-adaptive surface model.

---

## 9  Proposed Module Structure

```
src/dharmatiles/
  trees/
    __init__.py
    skeleton.py         SCA growth → node graph  (replaces branches.py _sca_grow)
    surface.py          swept-ring surface builder  (replaces trunk.py rings+stitch)
    radii.py            da Vinci pipe-model radius assignment  (extracted)
    tree.py             build_tree() orchestrator  (replaces trunk.py + branches.py top level)
  scatter/
    trees.py            Trees scatter thing  (unchanged interface)
```

The current `trunk.py` and `branches.py` are replaced by a cleaner three-way
split: **skeleton** (where do nodes go?), **surface** (what does each edge look
like?), **tree** (orchestrate skeleton → radii → surface → mesh).

---

## 10  Migration Path

Because the public API of `Trees` (the scatter thing) is unchanged, the migration
can happen entirely inside `trees/`:

1. Implement `skeleton.py` with `grow_skeleton(root_pos, cfg, rng)` returning
   `(nodes_xyz, parents, arc_dists)`.  Test: does it produce a visible trunk + crown?

2. Implement `radii.py` with `assign_radii(parents, r_tip, r_root_override)`.
   The root node's radius is overridden to `r_base_mm` (sampled) rather than
   computed from children — this lets the spec author control the trunk girth
   independently of the branching density.

3. Implement `surface.py` with `swept_mesh(p0, p1, r0, r1, frame0, frame1,
   arc_dist, cfg)` returning a trimesh for one edge, plus `build_tree_mesh(nodes,
   parents, radii, frames, arc_dists, cfg)` combining all edges.

4. Implement `tree.py` `build_tree()` that wires 1–3 together and adds the root
   flare and base cap.

5. Delete `trunk.py` and `branches.py`. Update imports. Regenerate all STLs.

---

## 11  Open Questions and Trade-offs

| Question | Options | Recommendation |
|---|---|---|
| **Trunk height control** | `crown_base_z_mm` (explicit) vs. emergent from SCA + tropism | Explicit: gives spec authors direct control |
| **Root radius** | Pipe model (computed) vs. explicit `r_base_mm` override | Override: pipe model radius at root is sensitive to branching density; explicit is more predictable |
| **Bifurcation surface** | Overlapping meshes (Option A) vs. junction caps (Option B) vs. parent widening (Option C) | Option A first, Option C if seams are visible |
| **Ridge continuity at bifurcations** | Arc-distance (§6) vs. Z-based (current) vs. none | Arc-distance is more correct; Z-based is a reasonable fallback and simpler |
| **Multi-trunk** | Multiple root nodes close together (already supported by SCA) | Expose as `n_roots: int = 1` with `root_spread_mm: float = 2.0` |
| **Stubs** | Drop entirely (SCA produces them naturally) | Drop: the thin outer SCA twigs serve the same "reads as tree" purpose as manual stubs |
| **Leaf geometry** | Out of scope for this document | Flat disk at each twig tip? Deferred to a separate design. |

---

## 12  What We Gain

- **One algorithm** governs the complete tree. Trunk character is a consequence
  of SCA geometry, not a separate model bolted on.
- **Bark surface everywhere.** Primary branches look like bark, not smooth cones.
  The tree reads as organic at every scale.
- **Da Vinci radii are authoritative.** The current implementation uses a power-law
  taper for the trunk and pipe-model for branches, which produces a discontinuity
  at the junction. The unified model uses pipe-model radii everywhere, with only
  the root node's radius overridden by the spec for legibility.
- **Fewer special cases.** No `sca_trunk_root_frac` hack. No "is this node a
  trunk node or a branch node?" conditional. No hand-off between two different
  mesh builders.
- **Composable surface.** The swept-ring surface builder becomes a general-purpose
  tool. It could in principle be reused for roots, vines, tentacles, or other
  organic tubular geometry.

## 13  What We Lose

- **Bark ring count independence.** The current trunk has `n_seg = 18` rings
  regardless of height; SCA segment count depends on how many steps the skeleton
  takes to fill the attractor cloud. For short trees with a small crown the step
  count may be low, producing a coarser trunk than desired. Mitigated by
  subdividing skeleton edges in `surface.py` (adaptive ring insertion between
  skeleton nodes).
- **Deterministic trunk height.** `height_mm` was a direct, readable parameter.
  `crown_base_z_mm` is close but the actual trunk height (root to first branch)
  also depends on SCA convergence. A spec author who wants exactly 28 mm of bare
  trunk has to tune `crown_base_z_mm` empirically.
- **Existing STL fidelity.** Trees generated with the old system will look
  different. This is acceptable; the STLs are generative outputs, not authored
  assets.

---

## 14  Key References (additions to design-trees.md §10)

- Palubicki et al., [*Self-organizing Tree Models for Image Synthesis*](https://algorithmicbotany.org/papers/selforg.sig2009.pdf), SIGGRAPH 2009 — extended SCA with bud fate model and apical dominance
- Runions et al., original SCA paper, §4 "Extension to 3D" — parallel-transport frame derivation for 3D skeleton meshes
- Prusinkiewicz & Lindenmayer, *The Algorithmic Beauty of Plants* (1990), Chapter 3 — general reference for self-similar branching structures
