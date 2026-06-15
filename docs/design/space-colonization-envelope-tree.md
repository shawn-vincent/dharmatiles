# Design: Space Colonization Envelope Tree

This is a clean-room design for printable natural-looking trees generated from
one explicit crown envelope.  It intentionally does not depend on any previous
tree implementation.

## Public Parameters

The author-facing tree shape is controlled only by these parameters:

| Parameter | Meaning |
| --- | --- |
| `height_mm` | Total tree height above terrain. |
| `trunk_height_mm` | Bare trunk height before the crown begins. |
| `crown_radius_mm` | Maximum horizontal crown radius. |
| `crown_base_radius_mm` | Horizontal crown radius where the canopy meets the trunk. |
| `top_pointiness` | Top silhouette blend, `0 = round`, `1 = conic/pointed`. |
| `top_curve` | How quickly the top reaches full width. |
| `bottom_pointiness` | Bottom silhouette blend, `0 = round`, `1 = conic/pointed`. |
| `bottom_curve` | How quickly the bottom reaches full width. |

Everything else in this document is derived from those values and internal print
heuristics.  Implementation may expose a random seed, but not additional shape
controls.

## Envelope

The crown occupies the vertical interval:

```text
crown_base_z = terrain_z + trunk_height_mm
crown_top_z  = terrain_z + height_mm
crown_height = height_mm - trunk_height_mm
```

If `crown_height <= 0`, no crown is generated.  `trunk_height_mm` is clamped to
`[0, height_mm]`.

For normalized crown height `t in [0, 1]`, where `t=0` is the crown bottom and
`t=1` is the crown top:

```text
end_profile(u, pointiness, curve):
    u = clamp(u, 0, 1)
    curve = max(curve, 0.01)
    round = sin(pi/2 * u) ^ curve
    point = u ^ curve
    return lerp(round, point, clamp(pointiness, 0, 1))

raw_radius(t):
    base = crown_base_radius_mm / crown_radius_mm
    bottom = base + (1 - base) * end_profile(t, bottom_pointiness, bottom_curve)
    top = end_profile(1 - t, top_pointiness, top_curve)
    return smooth_min(
        bottom,
        top
    )

radius(t):
    return crown_radius_mm * raw_radius(t) / max(raw_radius(samples))
```

The radius is `crown_base_radius_mm` at the bottom endpoint and exactly zero at
the top endpoint, and it reaches `crown_radius_mm` somewhere between them.  The
smoothed join avoids a kink where the top and bottom profiles meet.  This gives
one continuous surface that can describe a round deciduous crown, a pear shape,
a high umbrella, or a pointed conifer-like silhouette without adding species
parameters.

## Attractor Cloud

Space colonization should fill the envelope by using a canopy-surface attractor
cloud, not by hardcoding branch levels.

1. Sample `N` attractors on the crown side surface.
2. Do not sample the bottom disk inside `crown_base_radius_mm`; the base radius
   shapes the canopy where it meets the trunk, but it is not a leaf target
   surface.
3. Do sample the side surface near the top taper, even where its horizontal
   radius is smaller than `crown_base_radius_mm`.
4. Weight vertical samples by surface area for a surface of revolution:

```text
z = crown_base_z + t * crown_height
r = radius(t)
theta = random(0, 2*pi)
point = (cx + r*cos(theta), cy + r*sin(theta), z)
```

Derived count:

```text
N = clamp(round(crown_volume / 55 mm^3), 90, 420)
```

where crown volume is numerically integrated from `pi * radius(t)^2`.

## Growth Model

The skeleton starts at `(cx, cy, terrain_z)` with one active tip.  The first
phase grows a trunk toward the crown before normal SCA branching begins.

### Trunk Phase

Grow from terrain to `crown_base_z` in short upward segments:

```text
segment_len = clamp(height_mm / 18, 1.2, 2.4)
```

Each trunk step points mostly upward with tiny coherent XY drift.  The drift is
derived from low-frequency noise seeded per tree and is limited so the trunk tip
remains within `0.12 * crown_radius_mm` of the center when it enters the crown.

No attractors are visible to the trunk until the tip reaches:

```text
crown_base_z - 0.15 * crown_height
```

This allows the upper trunk to begin leaning naturally toward the future crown
without forking below the desired bare trunk height.

### Colonization Phase

At each SCA iteration:

1. Remove attractors within `kill_radius` of any active tip.
2. For every remaining attractor, find active tips within `perception_radius`.
3. Assign the attractor to the closest visible tip.
4. For each influenced tip, compute the average direction to its attractors.
5. Add derived biases:
   - weak upward tropism,
   - weak outward bias when deep inside the crown,
   - parent-direction inertia for smooth limbs.
6. Create one or more child tips from the influenced tip.

Derived distances:

```text
step_len          = clamp(height_mm / 18, 1.0, 2.2)
kill_radius       = 1.25 * step_len
perception_radius = clamp(0.32 * crown_radius_mm, 4.0, 9.0)
```

The active tip position is clamped softly to the crown envelope.  If a proposed
point exits the envelope, slide it back to `0.98 * radius(t)` at the same height
instead of discarding it.  This keeps the final silhouette faithful without
creating flat cutoff artifacts.

## Branching

Natural branching comes from clustered attractors, not from fixed split counts.
For each active tip's assigned attractors:

1. Compute the covariance of attraction directions.
2. If the largest two angular clusters are separated by at least 25 degrees and
   both clusters contain enough pull, split into two children.
3. Otherwise grow one child.

Cluster thresholds are derived:

```text
min_cluster_attractors = max(3, round(N / 100))
max_children_per_tip   = 2
```

This binary limit is deliberate for FDM.  Three-way junctions are harder to mesh
cleanly, weaker when printed, and visually noisy at miniature scale.  Repeated
binary splits still create rich crowns.

Suppress new splits when any of these are true:

```text
z < crown_base_z
branch_radius < 0.65 mm
distance_to_nearest_existing_branch < 0.75 mm
```

The last rule prevents dense branch tangles that turn into fused blobs.

## Radius Model

Assign radii after the skeleton is complete.

Leaf tips get:

```text
tip_radius = 0.45 mm
```

Internal radii use a printable pipe model:

```text
parent_radius = max(
    structural_min_radius_at_height,
    (sum(child_radius ^ 2.25)) ^ (1 / 2.25)
)
```

The exponent above 2 makes parent limbs visibly stronger without producing
oversized trunks.  Clamp root radius from the generated structure:

```text
root_radius = clamp(computed_root_radius, 1.25 mm, 0.14 * height_mm)
```

Minimum printable radius varies by height:

```text
structural_min_radius_at_height =
    lerp(0.75 mm, 0.42 mm, smoothstep(0.35, 0.95, z / height_mm))
```

Branches that would fall below the minimum are not deleted abruptly.  They taper
into a rounded bud cap over the last 1 to 2 segments.  This avoids unsupported
needle tips and avoids open mesh ends.

## Mesh Construction

Build one watertight union-style skeleton mesh from shared rings, not separate
capped tubes.

1. Convert each skeleton node into an oriented elliptical ring.
2. Align each ring frame with the local branch tangent.
3. Use 10 to 14 radial segments depending on radius:
   - `radius >= 1.2 mm`: 14 segments
   - `0.7 mm <= radius < 1.2 mm`: 12 segments
   - `radius < 0.7 mm`: 10 segments
4. Loft parent-child rings with quads.
5. At binary branch junctions, build one blended crotch patch that connects the
   parent ring to both child rings without internal caps.
6. Cap only the root bottom and terminal bud tips.

The root penetrates terrain by `0.2 mm` for a reliable boolean/union contact.
Add a root flare as geometry derived from the first trunk segment:

```text
flare_height = min(0.30 * trunk_height_mm, 4.0 mm)
flare_radius = 1.35 * root_radius at terrain, tapering to root_radius
```

This flare is not an author parameter; it is a printability feature.

## FDM Print Rules

Generated trees should be printable as small terrain features without supports.

Hard constraints:

| Constraint | Value |
| --- | --- |
| Minimum branch radius | `0.42 mm` |
| Minimum trunk/root radius | `1.25 mm` |
| Minimum branch separation before meshing | `0.75 mm` |
| Minimum upward branch angle | 35 degrees above horizontal |
| Root terrain penetration | `0.2 mm` |
| Terminal shape | rounded bud cap, not a sharp point |

If growth proposes a branch below the angle limit, rotate it upward around its
parent tangent until it satisfies the limit.  If this pushes it outside the
envelope, shorten the step before clamping radius.  This preserves both
printability and silhouette.

## Naturalness Rules

The tree should avoid looking algorithmic.

Use deterministic seeded randomness in these derived places:

- trunk drift phase,
- attractor sample positions,
- per-tip inertia strength within a narrow range,
- slight elliptical crown distortion, limited to 8 percent,
- ring ellipticity and rotation for bark-like irregularity.

Do not expose those as public parameters.  The same envelope should generate
varied individuals while keeping the same authored silhouette.

Asymmetry should be introduced by removing 5 to 12 percent of attractors from
one random lower-side sector and adding a small number of replacement attractors
to the opposite upper-side sector.  This creates a dominant limb and a readable
natural lean without violating the envelope.

## Implementation Contract

The eventual implementation should be split into these clean modules:

```text
trees/envelope.py     radius(t), volume integration, point containment
trees/attractors.py   seeded attractor generation inside envelope
trees/skeleton.py     trunk phase + SCA growth
trees/radii.py        printable pipe-model radius assignment
trees/mesh.py         watertight ring/crotch/bud mesh generation
trees/layer.py        scatter-layer integration and terrain stamping
```

The only shape config passed into the public layer should be:

```python
Tree(
    height_mm=40.0,
    trunk_height_mm=5.0,
    crown_radius_mm=20.0,
    crown_base_radius_mm=5.0,
    top_pointiness=0.0,
    top_curve=1.4,
    bottom_pointiness=0.35,
    bottom_curve=0.8,
)
```

Tests should verify:

- `radius(0) == crown_base_radius_mm`,
- `radius(1) == 0`,
- `max(radius(t)) == crown_radius_mm`,
- all attractors are on the canopy side surface, with none on the bottom disk,
- all terminal branch tips are inside or on the envelope,
- generated mesh is watertight,
- no branch radius is below the printable minimum,
- no unsupported branch angle is below the minimum,
- the tree reaches at least 92 percent of `height_mm` unless the envelope is
  physically impossible under the print constraints.
