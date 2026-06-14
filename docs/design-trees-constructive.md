# Design Document: Constructive Tree

**Date:** 2026-06-14  
**Status:** Draft — pre-implementation design  
**Scope:** Constructive (parametric + spatial) tree growth as a new tree variant alongside `ScaTree`  
**Predecessor docs:** `docs/design-trees-sca-phase1.md`, `docs/design-trees-sca-unified.md`

---

## 1  Motivation and Positioning

> **`ConstTree` in one sentence:** a deterministic, space-aware, constructive tree
> generator that sits halfway between Weber & Penn and Space Colonization — sacrificing
> biological realism in favour of direct artistic control, predictable silhouettes, and
> excellent FDM printability.

`ScaTree` (Space Colonization Algorithm) grows trees by emergent competition for attractor
points. This produces organic crown shapes but has two weaknesses that matter for FDM
terrain tiles:

| Problem | SCA behaviour | Constructive goal |
|---------|--------------|-------------------|
| Height control | Indirect: tune `crown_base_z_mm` and hope SCA converges at the right level | `height_max_mm` is specified directly; `seg_len_mm` is derived automatically |
| Predictable silhouette | Shape depends on attractor density and SCA stochasticity | Level count, segment counts, and spread angle determine the silhouette class up-front |

**Five parameters largely determine species silhouette.** Everything else is secondary
tuning:

| Parameter | Controls |
|-----------|---------|
| `height_max_mm` | Overall size |
| `n_levels` | Branching complexity |
| `n_segs_per_level` | How long major limbs are |
| `spread_angle_deg` | Columnar vs vase vs spreading crown |
| `pipe_model_exp` | Delicate (birch) vs chunky (fantasy oak) branch structure |

Both `ConstTree` and `ScaTree` feed into the same `BarkConfig` + `trees/surface.py`
bark pipeline; the only difference is how the skeleton (nodes + edges) is produced.

---

## 2  Core Concepts

### 2.1  Layered growth

The tree is grown in alternating **straight runs** and **splits**:

```
Root ─── n_trunk_segs segments ──► Trunk tip
                                        │
                               split into 2–3 children
                                        │
                    each child: n_segs_per_level[0] segments
                                        │
                               split into 2–3 children
                                        │
                    each child: n_segs_per_level[1] segments
                                        │
                               ...  (n_levels total split events)
```

All tips at the same depth grow **simultaneously**, one segment at a time. At each step
every active tip can see the positions of all other active tips and uses that information
to steer into unoccupied space.

### 2.2  No tapering within segment runs

Radius is **constant** along any straight run between two splits. Radius only changes at
split points, governed by da Vinci's pipe model. This simplifies the surface builder call
(all rings in a run are the same size) while still producing the visually important taper
effect across branching levels.

The config carries `taper_within_run: bool = False` as a forward-compatibility stub.
When `True` in a future phase, the node radii within each run would be interpolated by
a power law between split radius and next-split radius. No implementation is required
now — the surface builder already accepts per-node radii.

### 2.3  Direct height control

```
seg_len_mm = height_max_mm / (n_trunk_segs + sum(n_segs_per_level))
```

This is computed automatically. The trunk and all branches use the same `seg_len_mm`, so
the tree reaches `≈ height_max_mm` if growing straight up. The spread angle means actual
tips are slightly below `height_max_mm` (a branch tilted 30° from vertical covers only
`cos 30° ≈ 0.87` of the vertical height per segment), which is correct — the crown is
shorter than the full-extension trunk height.

### 2.4  Da Vinci pipe model with configurable exponent

At every split, child radii satisfy:

```
r_parent ^ n = Σᵢ r_child_i ^ n       (n = pipe_model_exp)
```

`pipe_model_exp` defaults to `2.0` (Leonardo's rule, area preserved) but is the
**single most visually impactful parameter after spread angle**. Its effect on
two equal daughter branches:

| `n` | Daughter / parent ratio | Character |
|-----|------------------------|-----------|
| 1.8 – 2.0 | 0.67 – 0.71 | Delicate, airy, birch-like |
| 2.0 – 2.3 | 0.71 – 0.74 | Classic deciduous; most real hardwoods |
| 2.3 – 2.7 | 0.74 – 0.77 | Heavy-limbed oak, old-growth |
| 2.7 – 3.0 | 0.77 – 0.79 | Chunky, fantasy-tree, baobab-like |

By generation 4 the compound effect is large: `n=2` produces twigs at 25 % of trunk
diameter; `n=3` leaves them at 40 % — visually very different trees from a single
parameter change.

Root node radius is overridden to `BarkConfig.r_base_mm` (sampled) exactly as in
`ScaTree`, decoupling trunk girth from branching density.

`radii.py:assign_radii()` is updated to accept an `exp: float = 2.0` parameter (see §4);
`ScaTree` calls it with `exp=2.0` and is unaffected.

---

## 3  Growth Algorithm

### 3.1  Initialization

```
root_dir  = (lean direction at initial_lean_deg from vertical, random azimuth)
root_node = Node(pos=(cx, cy, tz), dir=root_dir, radius=r_base, depth=0)
active_tips = [root_node]
```

### 3.2  Trunk growth loop

```
for seg in range(n_trunk_segs):
    next_tips = []
    for tip in active_tips:          # currently one tip during trunk phase
        new_dir = _step_direction(tip, active_tips, level='trunk', cfg, rng)
        new_pos = tip.pos + new_dir * seg_len_mm
        new_node = Node(pos=new_pos, dir=new_dir, radius=tip.radius, parent=tip)
        next_tips.append(new_node)
    active_tips = next_tips
```

### 3.3  Per-level split + grow loop

```
for level in range(n_levels):
    # ── Split all current tips ────────────────────────────────────────────
    post_split = []
    for tip in active_tips:
        n_children = rng.integers(split_count_min, split_count_max + 1)
        children   = _split_tip(tip, n_children, all_tips=active_tips, cfg, rng)
        post_split.extend(children)
    active_tips = post_split

    # ── Grow all children simultaneously ──────────────────────────────────
    n_segs = cfg.n_segs_per_level[level]
    for seg in range(n_segs):
        next_tips = []
        for tip in active_tips:
            new_dir = _step_direction(tip, active_tips, level=level, cfg, rng)
            new_pos = tip.pos + new_dir * seg_len_mm
            if new_pos[2] >= tz + cfg.height_max_mm:
                break               # this tip has reached max height; stop growing it
            new_node = Node(pos=new_pos, dir=new_dir, radius=tip.radius, parent=tip)
            next_tips.append(new_node)
        active_tips = next_tips
```

### 3.4  `_split_tip`: assigning child directions at a split

Children are placed around the parent direction using a **phyllotaxis spiral** — the
same golden-angle arrangement found in leaf bud spacing that guarantees maximum
angular separation regardless of `n_children`:

```
golden_angle  = 137.5°
spread        = cfg.spread_angle_deg[level]        # per-level lookup (see §6)
azimuth_i     = initial_phase + i * golden_angle   (i = 0..n_children-1)
elevation_i   = 90° - spread                       (tilt from vertical)
child_dir_i   = rotate(parent_dir, azimuth_i, elevation_i)
```

`initial_phase` is randomly sampled per split event so the spiral is not always aligned
the same way. This gives deterministically-even coverage while still looking organic.

**Why per-level spread angle matters:** real trees typically explode outward at the first
split (wide primary branches) then become more vertical again in the crown (narrower
secondary angles). A configuration like `spread_angle_deg=[35, 22, 14]` produces this
naturally; a single global value cannot.

**Elevation floor**: if `elevation_i < min_elevation_deg` the child direction is clamped
up to the floor before the first segment is grown — not deferred to post-pruning as in SCA.

**Asymmetric split** (when `dominant_branch = True`):

One child (index 0) is the *dominant*; the rest are *lateral*.

```
# Dominant: inherits parent direction, small tilt
dominant_dir   = rotate(parent_dir, azimuth_0, spread_angle_deg * dominant_angle_factor)

# Laterals: normal spread, remaining angular space partitioned by phyllotaxis
for i in 1..n_children:
    lateral_dir_i = rotate(parent_dir, azimuth_i, spread_angle_deg)

# Radius assignment using da Vinci:
r_dominant = (dominant_r_frac ** (1 / pipe_model_exp)) * r_parent
r_each_lateral = ((1 - dominant_r_frac) / (n_children - 1)) ** (1 / pipe_model_exp) * r_parent
```

This models the *monopodial* branching strategy (main leader continues, laterals depart)
as opposed to the default *sympodial* mode (all children are equal).

### 3.5  `_step_direction`: per-segment direction computation

For each tip's next growth step:

```
# 1. Momentum: continue in current direction
base_dir = tip.dir

# 2. Sibling repulsion: steer away from other tips at similar Z
repulsion = _compute_repulsion(tip, active_tips, cfg)
guided    = base_dir + repulsion_strength * repulsion

# 3. Random wander: Gaussian perturbation in the plane ⊥ to guided
wander  = rng.normal(0, sin(radians(wander_deg)), 2)   # 2D in XY plane
wandered = guided + [wander[0], wander[1], 0]

# 4. Elevation clamp: never below min_elevation_deg from horizontal
result  = _clamp_elevation(wandered, min_elevation_deg)
return result / np.linalg.norm(result)
```

### 3.6  Sibling repulsion: angular sector approach

Rather than a naive inverse-distance repulsion field (which can produce numerical
instability when tips are very close), we use an **angular sector** approach:

```
def _compute_repulsion(tip, all_tips, cfg):
    others = [t for t in all_tips if t is not tip and abs(t.pos[2] - tip.pos[2]) < 2 * seg_len_mm]
    if not others:
        return np.zeros(3)

    # For each sibling, compute the XY bearing angle from this tip
    bearings = [atan2(o.pos[1] - tip.pos[1], o.pos[0] - tip.pos[0]) for o in others]

    # Find the largest angular gap
    bearings_sorted = sorted(bearings)
    gaps = [(bearings_sorted[(i+1) % len(bearings_sorted)] - bearings_sorted[i]) % (2π)
            for i in range(len(bearings_sorted))]
    largest_gap_start = bearings_sorted[np.argmax(gaps)]
    target_bearing    = largest_gap_start + max(gaps) / 2

    # Convert target bearing to XY repulsion vector
    repulsion_xy = np.array([cos(target_bearing), sin(target_bearing)])
    return np.array([repulsion_xy[0], repulsion_xy[1], 0.0])
```

The angular-sector approach has two advantages over distance-based repulsion:
1. It produces a unit-magnitude repulsion regardless of inter-tip distance, so
   `repulsion_strength` is a stable, interpretable parameter.
2. Tips that are very close together steer away cleanly without division-by-zero.

---

## 4  Radius Assignment

`radii.py:assign_radii()` gains one new parameter:

```python
def assign_radii(
    parents:    np.ndarray,
    r_tip_mm:   float,
    r_root_mm:  float,
    exp:        float = 2.0,          # ← new; defaults preserve current ScaTree behaviour
) -> np.ndarray:
    ...
    for i in range(N - 1, 0, -1):
        p = int(parents[i])
        if p >= 0:
            radii[p] = (radii[p] ** exp + radii[i] ** exp) ** (1.0 / exp)
    radii[0] = r_root_mm
    return radii
```

`ScaTree` continues to call `assign_radii(..., exp=2.0)` — no change in behaviour.
`ConstTree` calls `assign_radii(..., exp=cfg.pipe_model_exp)`.

`branch_r_tip_mm` (from `BarkConfig`) is the starting radius at leaf nodes, unchanged.

---

## 5  Mesh Construction

The skeleton (nodes_xyz, parents) produced by `const_skeleton.py` is fed directly into
`trees/surface.py:build_tree_mesh()` and `trees/radii.py:assign_radii()`.

**No changes to the surface or radius modules.** The bark ring builder, frame propagation,
root flare, and base cap are all reused verbatim. The only per-skeleton difference is that
within each straight run, all rings have the same radius (no taper), so the swept tube is
a **cylinder** between splits and a **cone frustum** is only ever produced at the first
ring after a split (where the new, smaller radius has been assigned).

---

## 6  Configuration: `ConstTreeConfig`

Parameters are grouped by importance: **primary shape** (the five that define the
species silhouette), then **secondary tuning**, then **FDM safety**.

```python
@dataclass(init=False)
class ConstTreeConfig:
    """Full config for ConstTree: bark surface + constructive skeleton.

    Tile specs pass all parameters flat to ``ConstTree(...)``; the constructor
    splits them into ``bark`` and the constructive skeleton fields.
    """

    bark: BarkConfig   # reused from ScaTreeConfig / BarkConfig

    # ════════════════════════════════════════════════════════════════════════
    # PRIMARY SHAPE PARAMETERS — these five define the species silhouette
    # ════════════════════════════════════════════════════════════════════════

    height_max_mm:    Sample[float]                    = D[20.0:40.0]
    # seg_len_mm is NOT a config field — derived automatically:
    #   seg_len_mm = height_max_mm / (n_trunk_segs + sum(n_segs_per_level))

    n_levels:         int                              = 3
    # Number of recursive split events.

    n_segs_per_level: int | list[int]                 = 4
    # Segments grown between splits at each level.
    # Scalar: same count at every level.
    # List:   must have n_levels entries (level 0 = primary branches, etc.)
    # Example: [6, 3, 2]  →  long primary, medium secondary, short tertiary

    spread_angle_deg: Sample[float] | list[Sample[float]] = D[20.0:38.0]
    # Angle children tilt away from parent direction at each split.
    # Scalar: same angle at all levels.
    # List:   per-level spread (must have n_levels entries).
    # Example: [35, 22, 14]  →  wide first split, narrowing toward crown tips

    pipe_model_exp:   float                            = 2.0
    # Da Vinci area-preservation exponent.
    # 2.0 = Leonardo's rule (classic deciduous)
    # 2.3 = heavy-limbed oak / old-growth
    # 3.0 = chunky fantasy tree / baobab

    # ════════════════════════════════════════════════════════════════════════
    # SECONDARY TUNING
    # ════════════════════════════════════════════════════════════════════════

    n_trunk_segs:      int                             = 5
    # Segments on the bare trunk before the first split.

    split_count_min:   int                             = 2
    split_count_max:   int                             = 3   # inclusive; 2 = always binary

    initial_lean_deg:  Sample[float]                   = D[0.0:8.0]
    # Random tilt of the trunk from vertical (random azimuth).

    wander_deg:        Sample[float]                   = D[3.0:8.0]
    # Gaussian σ of per-segment angular noise (organic randomness).

    upward_bias:       float                           = 0.0
    # Legacy compatibility knob; constructive growth no longer pulls branches upward.

    repulsion_strength: float                          = 0.40
    # Blend weight toward the open angular sector (sibling repulsion).

    repulsion_z_window: float                          = 2.0
    # Tips within this many × seg_len_mm in Z are treated as "at the same level"
    # for the purposes of repulsion.

    # ── Asymmetric / monopodial branching ────────────────────────────────────
    dominant_branch:       bool                        = False
    # When True, one child at each split is the "leader" — less tilt, more radius.

    dominant_r_frac:       float                       = 0.65
    # Fraction of the parent's area budget allocated to the dominant child.
    # (Remainder is shared equally among laterals.)

    dominant_angle_factor: float                       = 0.30
    # Dominant child tilts spread_angle × this factor from parent direction.

    # ── Forward-compatibility stub ────────────────────────────────────────────
    taper_within_run:  bool                            = False
    # Phase 2: when True, apply a power-law radius taper within each segment run.
    # No implementation yet; the surface builder already accepts per-node radii.

    # ════════════════════════════════════════════════════════════════════════
    # FDM SAFETY
    # ════════════════════════════════════════════════════════════════════════

    min_elevation_deg: float                           = 45.0
    # No segment (trunk, branch, or post-split child) may point more than this
    # many degrees below vertical.  Enforced at split time and each growth step.
```

### `spread_angle_deg` distributions

The `Sample[float] | list[Sample[float]]` type covers the useful spread-angle cases:

| Config value | Behaviour |
|---|---|
| `spread_angle_deg=30` | Constant 30° at every split level |
| `spread_angle_deg=D[20:38]` | One random angle sampled per tree, used at all levels; each tree is consistently wider or narrower |
| `spread_angle_deg=[35, 22, 14]` | Explicit per-level sequence: wide primary split, narrowing toward tips |
| `spread_angle_deg=[D[30:45], D[18:26], D[10:18]]` | Per-level ranges, each sampled independently per tree |

There is no single-token shorthand for "resample the same distribution at every
level." If that is needed, write the per-level list explicitly so each level's range
is intentional and reviewable.

### Typical species recipes

```python
# Slender birch — many fine branches, delicate taper
ConstTree(n_levels=4, n_segs_per_level=3, spread_angle_deg=D[25:35], pipe_model_exp=1.9)

# Classic oak — wide first split, converging crown
ConstTree(n_levels=3, n_segs_per_level=[6, 3, 2],
          spread_angle_deg=[35, 22, 14], pipe_model_exp=2.3)

# Fantasy tree — chunky, thick-branched, dramatic
ConstTree(n_levels=2, n_segs_per_level=[4, 3], spread_angle_deg=D[30:45],
          pipe_model_exp=3.0, dominant_branch=True, dominant_r_frac=0.70)
```

---

## 7  Scatter Thing: `ConstTree`

`scatter/const_tree.py` follows the identical interface as `ScaTree` / `Flowers` / `Rocks`:

```python
class ConstTree:
    """Scatter constructive deciduous trees into a region.

    Usage in a tile spec::

        from dharmatiles.scatter import ConstTree
        from dharmatiles.scatter.config import Uniform

        ScatterLayer(
            ConstTree(height_max_mm=D[25:38], n_levels=3, placement=Uniform(count_per_square=1)),
            Grass(species=species),
        )
    """
    def __init__(self, *, placement=None, **tree_kwargs): ...
    def scatter(self, scene, *, placement_mask, layer_idx=0) -> list[trimesh.Trimesh]: ...
```

`stamp_tree()` from `trees/tree.py` is reused verbatim — the footprint-stamping
logic is independent of which skeleton algorithm was used.

---

## 8  Module Layout

```
src/dharmatiles/
  core/
    config.py                 + ConstTreeConfig dataclass
  trees/
    const_skeleton.py         constructive skeleton grower  (new)
    # surface.py, radii.py, tree.py unchanged
  scatter/
    const_tree.py             ConstTree scatter thing  (new)
    # sca_tree.py unchanged
src/tiles/ground/
  1x1-grass-const-trees.tile.py   example spec  (new)
```

---

## 9  Comparison to Existing Algorithms

| Feature | `ScaTree` (SCA) | Weber & Penn | L-system | `ConstTree` |
|---------|-----------------|-------------|----------|-------------|
| Height control | Indirect (`crown_base_z_mm`) | Indirect | Indirect | **Direct** (`height_max_mm`) |
| Branching origin | Emergent — attractors | ~25 params/level | Grammar rules | 5 primary params + spatial |
| Simultaneous tip growth | No | N/A | Yes | **Yes** |
| Space-filling without attractors | No | No | No | **Yes — angular-sector repulsion** |
| Silhouette predictability | Low | Medium | Low | **High** |
| FDM compliance | Post-prune | Per-species tuning | Per-species tuning | **Baked in at split + step** |

`ConstTree` is operationally equivalent to a simple bracketed L-system with two symbols
(`FORWARD`, `BRANCH`) applied at fixed depths — but adds **spatial sibling awareness**
that no standard L-system implementation has. It simplifies Weber & Penn to the
parameters that matter at 25–40 mm printed scale, dropping grandparent-dependency
formulas and per-level taper curves that are invisible at miniature resolution.

---

## 10  Open Questions

| Question | Recommendation |
|----------|----------------|
| Should `split_count` vary per level? | Defer — single `split_count_min/max` for now; promote to `list[int]` if needed. |
| What if a tip hits `height_max_mm` mid-segment-run? | Stop that tip; siblings continue. No prune pass needed. |
| Should phyllotaxis phase accumulate across branching levels? | Start with per-split random phase. Cross-level accumulation (advancing the phase by `n_children × 137.5°` at each split) improves coverage on short 2-level trees and is trivial to add. |
| Can `ConstTree` and `ScaTree` appear in the same `ScatterLayer`? | Yes — independent scatter things sharing the same bark surface pipeline. |
| When should you use `ScaTree` vs `ConstTree`? | `ScaTree` for wide naturalistic deciduous crowns (hero specimen trees). `ConstTree` for controlled silhouettes, filler trees that need a consistent look, or any time you want to dial in a specific species shape via the five primary parameters. |
