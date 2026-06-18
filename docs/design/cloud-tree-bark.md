# Design: CloudTree Bark

Status: design - 2026-06-16

## Goal

Add procedural bark to CloudTree trunks and branches as a separate, configurable
feature from tree shape, skeleton growth, and foliage.

The first bark style is a set of non-overlapping wavy vertical grooves:

- grooves start at the tree root,
- grooves climb the trunk and continue onto every branch,
- the same parent groove lineage continues onto both branches at a fork,
- grooves stop before foliage-only geometry,
- grooves continue through any wooden stub that precedes a foliage clump,
- each groove cuts a V-shaped trench into the wood surface,
- fewer grooves continue as branch diameter shrinks, keeping spacing visually
  even instead of crowding tiny branches.

## Scope

This document specifies bark placement and geometry. It does not change the
CloudTree skeleton algorithm, canopy envelope, foliage attractors, or terrain
placement.

The implementation target is `dharmatiles.trees.cloud_mesh`: bark should be
generated while building each closed branch edge solid, before the per-edge
solids are unioned.

## Public Specification

Add a bark configuration object, passed through `CloudTree` to
`build_cloud_tree_mesh`.

```python
@dataclass(frozen=True)
class BarkConfig:
    enabled: bool = True
    spacing_mm: float = 2.25
    depth_mm: float = 0.80
    width_mm: float = 0.72
    roughness_amplitude_mm: float = 0.05
    roughness_cell_mm: float = 0.90
    wave_amplitude_mm: float = 0.22
    wave_length_mm: float = 7.5
    phase_jitter: float = 1.0
    min_branch_radius_mm: float = 0.58
    foliage_clearance_mm: float = 0.6
```

Field meanings:

| Field | Meaning |
|---|---|
| `enabled` | Enables bark generation. |
| `spacing_mm` | Target arc distance between groove centers around the circumference. |
| `depth_mm` | Maximum inward cut depth at the groove centerline. |
| `width_mm` | Tangential width of the V-shaped trench at the original bark surface. |
| `roughness_amplitude_mm` | Maximum secondary surface roughness outside grooves. |
| `roughness_cell_mm` | Spatial scale of the secondary surface roughness. |
| `wave_amplitude_mm` | Tangential side-to-side drift of each groove. |
| `wave_length_mm` | Distance along the branch before the wave repeats. |
| `phase_jitter` | Randomizes initial phase per root groove while remaining deterministic. |
| `min_branch_radius_mm` | Branches below this radius carry no grooves. |
| `foliage_clearance_mm` | Bark stops this far before the foliage expansion begins. |

Defaults should be conservative for FDM printing: shallow enough to avoid
weakening small branches, wide enough to remain visible after slicing, and sparse
enough that grooves do not merge.

## Conceptual Model

Bark grooves are not independent decorations placed separately on every segment.
They are long-lived lineages that originate around the root circumference.

Each groove has:

```text
BarkLine
  id: stable integer
  phase: float
  root_theta: float
  current_theta_at_node[node_id]: float
```

At a branch fork, every continuing child receives the same candidate bark lines
from the parent. The child then keeps a radius-appropriate subset. This means a
visible groove that reaches a fork can continue up both outgoing branches, unless
one of those branches has become too narrow to carry that groove.

This intentionally differs from random per-branch bark. Per-branch bark would
create visual discontinuities at forks and would make the trunk read as a stack
of unrelated tubes.

## Coverage Rules

For each skeleton edge `parent -> child`, compute the interval of the edge that
is wood:

```text
wood_start_t = 0.0
wood_end_t   = 1.0
```

For non-leaf wood-only edges, bark may cover the whole edge.

For leaf edges with foliage disabled, bark may cover the whole edge.

For leaf edges with foliage enabled:

- if `leaf_clump_length_mm is None`, the entire leaf edge is foliage expansion;
  bark does not appear on that edge,
- if `leaf_clump_length_mm = K`, bark may cover only the wooden stub before
  `t_split`, stopping at `t_split` minus `foliage_clearance_mm` along arc length.

In other words, bark stops before foliage, but it still continues along any
branch segment that remains a wooden stub before the foliage segment.

## Groove Count And Spacing

At the root, choose the number of groove lineages from circumference:

```text
n_root = max(3, floor((2*pi*root_radius) / spacing_mm))
```

The root lineages are evenly spaced:

```text
root_theta_i = 2*pi*i / n_root
```

For an edge with local radius `r`, the desired number of active grooves is:

```text
n_desired = floor((2*pi*r) / spacing_mm)
```

Clamp:

```text
if r < min_branch_radius_mm:
    n_desired = 0
else:
    n_desired = max(1, n_desired)
```

When fewer grooves should continue, select a deterministic even subset from the
parent's active lineage IDs. The subset rule must preserve visual spacing and be
stable across runs. A simple first implementation:

```text
keep every round(n_parent / n_desired)-th groove by circular order
then fill/drop by largest angular gap until exactly n_desired remain
```

The selection is evaluated per child. Both children at a fork start from the
same parent lineages, so a groove can continue up both branches. If one child is
thicker than the other, the thicker child keeps more of those lineages.

## Wave Function

A groove's angular position on an edge varies with distance along that edge:

```text
theta(s) =
    theta_at_parent
    + (wave_amplitude_mm / max(radius(s), epsilon))
      * sin(2*pi*s / wave_length_mm + phase)
```

Where:

- `s` is arc length from the parent node along the Bezier edge,
- `radius(s)` is the branch radius profile at that sample,
- `phase` is deterministic per `BarkLine`,
- the amplitude is expressed in millimeters and converted to angular offset by
  dividing by radius.

Clamp tangential wave amplitude to keep neighboring grooves from crossing:

```text
max_wave_amplitude = 0.25 * spacing_mm
```

The implementation may also use a low-frequency noise function instead of a
single sine wave later, but the sine version is the canonical first pass because
it is deterministic, cheap, and easy to test.

## Non-Overlap Rule

Grooves must not overlap on a sampled ring.

For every ring sample:

1. Compute all candidate groove center angles.
2. Sort by angle around the ring.
3. Measure circular arc distance between neighbors.
4. If any distance is below `width_mm * 1.25`, drop the groove with the smaller
   local priority on that edge.

Priority is deterministic:

```text
priority = hash(tree_seed, bark_line_id, edge_id)
```

Dropping is local to the edge. It does not delete the lineage globally; a groove
may still continue on another child branch if it remains valid there.

## V-Shaped Trench Geometry

The groove is an inward displacement of branch ring vertices, not a separate
material mesh.

At each ring sample, each ring vertex has angular coordinate `theta_v`.
For every active groove center `theta_g`, compute shortest wrapped angular
distance:

```text
d_mm = radius * wrapped_abs(theta_v - theta_g)
```

The V profile is:

```text
if d_mm >= width_mm / 2:
    cut = 0
else:
    cut = depth_mm * (1 - d_mm / (width_mm / 2))
```

The displaced radius is:

```text
r_barked = max(r_original - cut, min_safe_radius)
```

`min_safe_radius` should protect tiny branches:

```text
min_safe_radius = max(0.42, r_original - 0.35 * r_original)
```

This creates a triangular cross-section groove: deepest at the center, tapering
linearly to the original bark surface at both sides.

### Ring Resolution Requirement

The current branch mesh uses a fixed 12-sided circular ring. That is too coarse
for narrow bark grooves. Bark needs either:

- a higher global ring side count when bark is enabled, or
- adaptive insertion of groove shoulder and center vertices into each ring.

The preferred implementation is adaptive ring angles:

```text
base ring angles
+ each groove center theta
+ each groove shoulder theta +/- width/(2*r)
```

Deduplicate angles closer than a small angular epsilon, sort them, and connect
adjacent rings by angular order. This keeps geometry concentrated where bark
exists instead of raising every branch to a high polygon count.

## Fork Continuity

The mesh builder already builds each edge as a separate closed solid with a
small volumetric overlap at non-root starts. Bark should follow the same edge
solid model.

Continuity requirement:

- a bark line active at a parent node has a defined `theta_at_parent`,
- each child maps that angle into the child's start frame,
- the groove begins inside the overlapped portion of the parent branch,
- after boolean union, the visible groove appears to enter the fork and continue
  onto both children.

Because child edges start slightly behind the fork, the first visible bark
sample on the child should begin at the overlapped start. This avoids a smooth
unbarked collar at branch joints.

If boolean union erases very shallow fork grooves, increase local groove depth
inside the overlap by up to 25 percent, but only within the hidden overlap
region. Do not add separate junction decals.

## Frame Mapping At Branches

Each edge has a Bishop frame `(u, v)` perpendicular to its tangent. A groove's
angle is interpreted in that local frame:

```text
surface_point = center + radius * (cos(theta) * u + sin(theta) * v)
```

At a child edge, `theta_at_parent` should be the angle whose surface direction
is closest to the parent groove's world-space outward normal at the fork.

```text
theta_child = atan2(dot(n_world, v_child), dot(n_world, u_child))
```

This preserves visual continuity even when the child branch rotates to a new
frame.

## Determinism

Bark must be deterministic for a given tile seed and tree seed.

Do not use global random state. Derive bark phases and priorities from stable
inputs:

```text
phase_i    = 2*pi*hash01(tree_seed, "bark-phase", bark_line_id)
priority_i = hash01(tree_seed, "bark-priority", edge_id, bark_line_id)
```

Changing unrelated branches should not reshuffle bark on existing branches
unless the skeleton topology itself changes.

## Implementation Plan

1. Add `BarkConfig` to `dharmatiles.trees.layer`.
2. Add `bark: BarkConfig | None` to `CloudTree.__init__`.
3. Pass the sampled/resolved bark config into `build_cloud_tree_mesh`.
4. In `build_cloud_tree_mesh`, build child lists as today, then propagate active
   `BarkLine` state breadth-first alongside `node_frame`.
5. Replace `_make_ring(center, radius, u, v)` with a bark-aware ring builder
   that accepts an optional list of active groove centers for that ring.
6. For bark-enabled edges, generate adaptive ring angles and apply the V profile
   before emitting vertices.
7. Keep caps bark-free except where the cap perimeter uses the already-barked
   ring. Do not carve grooves into root bottom caps or terminal dome caps.
8. Union edge solids exactly as today.

## Validation

Unit tests should cover:

- root groove count is derived from circumference and spacing,
- groove count decreases monotonically as radius decreases,
- selected groove IDs are an even deterministic subset,
- child branches receive candidate lineages from the parent,
- both children at a fork can retain the same parent groove lineage,
- leaf foliage edges receive no bark when the entire edge is foliage,
- wood stubs before foliage receive bark up to the foliage clearance,
- generated groove centers on a ring do not overlap.

Mesh tests should cover:

- a straight trunk with bark remains watertight and volumetric,
- a continuing-plus-diverging fork remains watertight after bark and union,
- a three-child fork remains watertight,
- foliage clump edges show bark only on the wooden stub,
- minimum-radius branches are left uncarved.

Visual checks should include:

- grooves read as long climbing lines from the root,
- lines visibly continue through forks,
- small branches have fewer, evenly spaced lines,
- no groove crosses or merges with a neighbor,
- no bark cuts appear on rounded foliage tips.

## Non-Goals

The first version does not attempt:

- raised bark ridges,
- bark color or material separation,
- cracks that split and merge independently of branch lineage,
- species-specific bark presets,
- high-frequency roughness/noise over the entire wood surface.

Those can be layered on later after the groove lineage model is working.
