# Design Review: Constructive Tree Algorithm

**Date:** 2026-06-14  
**Reviewer:** Claude Sonnet 4.6  
**Subject:** `docs/design-trees-constructive.md` — pre-implementation review  
**Status:** Completed; proposed improvements documented below

---

## Review Scope

This review covers the proposed `ConstTree` / `ConstTreeConfig` design in
`docs/design-trees-constructive.md`. It benchmarks the design against five
reference algorithms, identifies structural gaps, and proposes concrete
improvements with their expected benefits.

Research sources consulted:

- Runions, Lane & Prusinkiewicz, [*Modeling Trees with a Space Colonization Algorithm*](https://algorithmicbotany.org/papers/colonization.egwnp2007.large.pdf) (2007)
- Weber & Penn, [*Creation and Rendering of Realistic Trees*](https://courses.cs.duke.edu/cps124/fall01/resources/p119-weber.pdf), SIGGRAPH 1995
- Prusinkiewicz & Lindenmayer, *The Algorithmic Beauty of Plants* (1990) — L-systems
- Palubicki et al., [*Self-Organizing Tree Models for Image Synthesis*](https://algorithmicbotany.org/papers/selforg.sig2009.pdf) (2009) — bud fate / open voxels
- Stava et al., [*Inverse Procedural Modelling of Trees*](https://www.researchgate.net/publication/334427060_Algorithms_for_procedural_generation_and_display_of_trees) and survey
- Academic paper on [*Printable 3D Trees*](https://www.researchgate.net/publication/320376287_Printable_3D_Trees) — FDM branch thickening / tilting
- Eloy / PLOS ONE, [*Tree Branching: Leonardo da Vinci's Rule versus Biomechanical Models*](https://journals.plosone.org/plosone/article?id=10.1371/journal.pone.0093535) (2014)
- Phyllotaxis / golden angle: [*Biophysical optimality of the golden angle*](https://www.nature.com/articles/srep15358)
- Research on divergence angles: [*The unified rule of phyllotaxis*](https://royalsocietypublishing.org/doi/10.1098/rsif.2018.0850)
- ChatGPT research note: da Vinci pipe model exponent visual effects (2026-06-14)

---

## 1  Summary Assessment

The design is **sound and well-suited for the FDM miniature context**. It correctly
identifies the central weakness of `ScaTree` (indirect height control) and addresses
it with a constructive approach that borrows the best parts of Weber & Penn
(per-level structure) and L-systems (simultaneous tip growth) while adding a
genuinely novel element: **sibling-aware angular-sector repulsion** for space-filling.

The two areas needing the most attention before implementation:

1. **Radius assignment generalisation** — `radii.py` currently hardcodes `exp=2.0`;
   the design proposes a `pipe_model_exp` parameter but the integration into the
   existing module needs explicit API design.
2. **Skeleton → surface handoff** — because ConstTree produces constant-radius
   segments between splits, the existing `build_tree_mesh()` will technically work,
   but the surface builder doesn't know which segments are "run starts" vs "mid-run."
   The visual difference is negligible for FDM output but worth confirming.

Five concrete improvements are proposed in §3 below.

---

## 2  Benchmarking Against Reference Algorithms

### 2.1  Weber & Penn (1995)

W&P uses a hierarchical parameter system with explicit controls per level for branch
length ratio, radius ratio, curve angle, and phyllotaxis angle. The proposed
`ConstTree` covers the same design space but with fewer parameters:

| W&P parameter | ConstTree equivalent |
|---------------|---------------------|
| `nLength[level]` (level length ratio) | `n_segs_per_level[level]` (integer count × seg_len_mm) |
| `nDownAngle[level]` | `spread_angle_deg` (single value, not per-level yet) |
| `nRotateAngle[level]` (phyllotaxis) | Golden-angle fixed at 137.5° (optimal, not configurable) |
| `nTaper[level]` | Not implemented yet (`taper_within_run` future flag) |
| `nBranches[level]` (branch count) | `split_count_min/max` (same range, not per-level yet) |
| `scale` | `height_max_mm` |

**Gap:** W&P allows per-level customisation of `spread_angle_deg` and `split_count`.
`ConstTree` currently uses single values for these. For Phase 1 this is acceptable
(simpler API); per-level variants can be added by promoting these fields to
`int | list[int]` / `Sample[float] | list[Sample[float]]` if needed.

**Advantage of ConstTree over W&P:** no grandparent-dependency formulas, no
`nCurveBack` / `nCurveV` complexity, and spatial sibling repulsion is impossible
in pure W&P (branches are placed purely procedurally with no awareness of other branches).

### 2.2  L-Systems (Prusinkiewicz & Lindenmayer 1990)

L-systems grow all branch tips simultaneously by rewriting a symbol string at each
generation — this is exactly the "simultaneous growth" the design specifies. The
proposed algorithm is **operationally equivalent to a context-free bracketed
L-system** but with:

- A fixed two-level grammar (`F` = grow segment; `[` = split) instead of an editable string
- Spatial tip context (sibling positions) baked into the interpretation step
- A built-in FDM elevation constraint

**L-system advantage not captured:** stochastic production rules can produce different
branch subtypes (short shoots vs. long shoots, as in some conifers). `ConstTree`
has only one branch type. This is the right trade-off for miniature-scale tabletop
terrain where subtlety is lost in a 3 cm printed tree.

**Unique ConstTree advantage:** L-systems have no concept of where sibling tips
currently are in 3D space — all rewriting is context-free at the branch string level.
The angular-sector repulsion is a genuine algorithmic contribution not found in
standard L-system implementations.

### 2.3  Space Colonization Algorithm (Runions 2007 — existing `ScaTree`)

SCA produces organic, irregular skeletons by emergent attractor competition.
`ConstTree` trades that irregularity for:

- Predictable silhouette class
- Efficient O(n_tips²) growth vs. SCA's O(n_tips × n_attractors) per step
- Direct height specification

**When to prefer SCA:** when the target look is a wide, naturalistic deciduous crown
with complex, light-seeking branching (forest backdrop trees, large feature specimens).

**When to prefer ConstTree:** when you want a predictable outline (forest-scene filler
trees all roughly the same shape, or tightly-specified "one specific tree species"
configs). Also: when you want to quickly sweep silhouette archetypes — compact vs.
spreading vs. vase-shaped — via a single `spread_angle_deg` value.

### 2.4  Palubicki et al. Self-Organizing Tree Models (2009)

This is the "open voxel" extension of SCA: each bud competes for light based on
how much voxel space is unoccupied above it. The result is apical dominance,
light competition, and realistic crown asymmetry.

The angular-sector repulsion in `ConstTree` is a simpler 2D version of this idea:
instead of voxelised 3D light, we use 2D angular availability in the XY plane.
For FDM miniatures at 3 cm scale the 2D approximation is entirely adequate.

**Improvement the Palubicki model suggests:** rather than repulsion from *all* sibling
tips at the same Z, weight repulsion by each sibling's *subtree size* (number of
downstream tips). Thick, large branches deserve more "territory" than thin ones.
This would naturally produce the asymmetric spreading seen in real trees without needing
the explicit `dominant_branch` flag.

### 2.5  Printable 3D Trees (Stava et al., ResearchGate 2017)

This paper modifies existing tree models for FDM by (a) detecting mechanically weak
branches and thickening them, and (b) tilting branches that violate overhang limits.

`ConstTree` avoids both post-processing steps by:

- Baking the elevation floor **at split time** (child directions are clamped before
  the first segment grows) rather than post-pruning
- Not generating branches below the minimum radius that would be mechanically weak
  (governed by `BarkConfig.branch_min_r_mm`)

**One gap:** the Stava paper's **thickening** pass increases the radius of branches
near the trunk to improve structural integrity in real-world printing. `ConstTree`
inherits `branch_min_r_mm` from `BarkConfig`, which *skips* thin segments rather
than thickening them. This means printed trees may have visible ring stubs where
thin tips were discarded. Mitigation: set `branch_r_tip_mm` high enough that no
tips fall below `branch_min_r_mm`.

---

## 3  Proposed Improvements

### 3.1  Configurable `pipe_model_exp` with backward-compatible `radii.py` API

**Current state:** `radii.py:assign_radii()` hardcodes `exp=2` in the accumulation
loop. `ConstTreeConfig` proposes `pipe_model_exp` but the existing function won't
use it.

**Proposed change:**

```python
def assign_radii(
    parents:    np.ndarray,
    r_tip_mm:   float,
    r_root_mm:  float,
    exp:        float = 2.0,          # ← new parameter, defaults to current behaviour
) -> np.ndarray:
    ...
    for i in range(N - 1, 0, -1):
        p = int(parents[i])
        if p >= 0:
            radii[p] = (radii[p] ** exp + radii[i] ** exp) ** (1.0 / exp)
    ...
```

`ScaTree` calls `assign_radii(..., exp=2.0)` (same result as before).
`ConstTree` calls `assign_radii(..., exp=cfg.pipe_model_exp)`.

**Benefit:** unlocks the full visual range from delicate birch (n=1.8) to fantasy oak
(n=3.0) via a single parameter. Per the ChatGPT research note, even a change from
2.0 to 2.3 produces a clearly visible difference in trunk-to-branch weight — this
parameter earns its place in the config.

---

### 3.2  Per-level `spread_angle_deg` (promote to `Sample[float] | list[Sample[float]]`)

**Current state:** single `spread_angle_deg` applies at every branching level.

**Problem:** real trees have different angles at different levels. Primary branches
depart the trunk at wide angles (30–45°); secondary branches within the crown split
at narrower angles (10–20°) to avoid exiting the crown volume.

**Proposed config addition:**

```python
spread_angle_deg: Sample[float] | list[Sample[float]] = D[20.0:38.0]
# Scalar: same angle at all levels.
# List: spread_angle_deg[level] used at that level; list must have n_levels entries.
```

**Benefit:** enables the characteristic "exploding then converging" crown shape of
oaks and maples — wide angle at the first split, narrowing at outer levels — with
a single list override, e.g. `spread_angle_deg=[35, 22, 14]`.

---

### 3.3  Subtree-weighted sibling repulsion (Palubicki-inspired)

**Current state:** all active tips contribute equally to the repulsion calculation
for any given tip.

**Problem:** after a few levels of splitting, a tip from level-2 branching should
not be equally repelled by a level-3 tip that has already committed to the same
quadrant. Subtrees that have grown many downstream branches have already "claimed"
their space.

**Proposed change:**

```python
# Weight each sibling's repulsion influence by its subtree size
def _compute_repulsion(tip, all_tips, subtree_sizes, cfg):
    for other in others:
        weight = subtree_sizes.get(other, 1.0)   # 1.0 for leaf tips
        bearing = atan2(...)
        ...
```

`subtree_sizes` is computed once per growth step as the number of downstream nodes
each tip has (leaves have size 1, their parents have size 2, etc.).

**Benefit:** large primary branches establish their territory firmly; thin secondary
branches negotiate around them rather than competing as equals. Produces the
"dominant main limbs" look characteristic of old deciduous trees. Low implementation
cost (one extra dict per step).

**Caution:** increases coupling between the skeleton loop and the node tracking.
The simpler equal-weight version is fine for Phase 1; this is a Phase 2 refinement.

---

### 3.4  Cross-level phyllotaxis phase accumulation

**Current state:** each split event samples a fresh random initial phase for the
golden-angle spiral. This gives good within-split coverage but doesn't coordinate
across levels.

**Problem:** if the level-0 split places two branches at 0° and 180°, and the
level-1 split on each child starts at a random phase, it's possible (with unlucky
seeds) to have all level-1 branches clustered near 0° and 180°.

**Proposed change:** propagate a `phyllotaxis_phase` accumulator through the node
tree, seeded at the root and advanced by `n_children × golden_angle` at each split.
Children inherit the next phase value from their parent's phase state.

```python
# In _split_tip:
phase = parent.phyllotaxis_phase    # inherited
for i in range(n_children):
    azimuth_i = phase + i * 137.5°
    ...
parent.phyllotaxis_phase += n_children * 137.5°   # advance for next split
```

**Benefit:** inter-level coverage guaranteed by the same mathematical property that
makes phyllotaxis optimal for leaves — no two consecutive branch generation sets
end up in the same angular zone. This matters most on short trees (2–3 levels) where
random phases have the highest chance of accidental clustering. Low cost, adds ~2
floats of state per node.

---

### 3.5  `taper_within_run: bool = False` stub in config

**Current state:** the design says "no tapering within segment runs (Phase 1)" but
the config has no hook for it.

**Proposed addition:**

```python
# In ConstTreeConfig:
taper_within_run: bool = False     # Phase 2: apply per-segment radius taper within a run
taper_run_exp:    float = 0.6      # power-law exponent if taper_within_run is True
```

The surface builder already supports variable per-segment radii (it receives a
`radii` array indexed by node, not by edge). When `taper_within_run = True`, the
node radii within a straight run would be set to a power-law interpolation between
split radius and next-split radius, producing gradually tapering runs.

**Benefit:** explicit forward-compatibility hook. Without it, adding taper later
requires modifying both `ConstTreeConfig` and `const_skeleton.py`; with it, only
`const_skeleton.py` needs a `if cfg.taper_within_run:` branch. Zero runtime cost
when `False`.

---

## 4  Improvements NOT Recommended

### 4.1  Weber & Penn grandparent-dependency

W&P makes branch length and radius depend on the parent's parent. This captures
real botanical data well (a branch coming off a thick trunk is longer than one
coming off a thin branch), but at 3 cm total tree height in FDM the perceptual
difference is zero. The added code complexity is not worth it.

### 4.2  Bud fate model (Palubicki et al.)

The Palubicki 2009 bud fate model governs whether a bud becomes a long shoot, short
shoot, or dies based on 3D light availability. Beautiful for rendering; overkill for
a 3 cm printed tree where the viewer is 60 cm away.

### 4.3  Full 3D voxel space-filling

True 3D voxel light maps (as in the Palubicki model) are O(n_voxels) to evaluate.
For 100+ trees on a 2×4 tile at 2 mm resolution that's a 100+ MB structure. The
2D angular-sector approach captures 90% of the benefit at negligible cost.

---

## 5  Implementation Priority Order

1. **`assign_radii` generalisation** (§3.1) — zero-risk, one-line change, unblocks
   the `pipe_model_exp` config parameter for both `ConstTree` and `ScaTree`.

2. **Core `const_skeleton.py`** — trunk growth loop, split, simultaneous grow loop,
   angular-sector repulsion, phyllotaxis child placement. This is the minimum viable
   `ConstTree`.

3. **`ConstTreeConfig` + `scatter/const_tree.py`** — wrap skeleton in scatter thing;
   add `ConstTree` to `scatter/__init__.py`.

4. **Per-level `spread_angle_deg`** (§3.2) — add immediately; cheap and high-value
   for realistic silhouettes.

5. **Phyllotaxis phase accumulation** (§3.4) — add to initial implementation; trivial
   to add at construction time, hard to retrofit.

6. **`taper_within_run` stub** (§3.5) — add to `ConstTreeConfig` as `False`; no
   implementation needed yet.

7. **Subtree-weighted repulsion** (§3.3) — Phase 2; add after visual validation of
   the basic tree silhouettes.

---

## 6  Files Renamed This Session

The following doc renames were made to clarify that those documents describe the SCA
tree specifically, not the tree system in general:

| Old name | New name |
|----------|----------|
| `docs/design-trees.md` | `docs/design-trees-sca-phase1.md` |
| `docs/design-trees-unified.md` | `docs/design-trees-sca-unified.md` |

The new constructive-tree design lives at `docs/design-trees-constructive.md`.
