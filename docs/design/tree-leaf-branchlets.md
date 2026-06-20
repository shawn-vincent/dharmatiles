# Design: Tree Leaf Support Volumes

Status: design - 2026-06-20

## Goal

Generate realistic-looking leaves on arbitrary 3D-printable foliage meshes while
guaranteeing that the resulting geometry is FDM-printable without unsupported
undercuts.

The visible leaf should read as a thin natural leaf:

- teardrop outline with a rounded base and pointed tip,
- central crease down the base-tip axis,
- two convex lobes on either side of the crease,
- mostly upward-pointing placement with natural twist, jitter, curl, and
  variation,
- future support for alternate initial leaf surface shapes.

The structural attachment should not be the current fixed keel. Instead, each
leaf gets a generated hidden support volume under the visible surface. That
support volume must be self-printable, connected to the supplied mesh, thick
enough to print, and rejected when it would become visually absurd or
geometrically invalid.

One sentence version:

> A realistic leaf is the visible cap; the printable object is the cap plus a
> hidden self-printable support volume whose upward slices form a continuous
> ancestry chain from the supplied mesh to every unsupported part of the leaf.

## Current Context

The live implementation is `src/dharmatiles/trees/leaf.py` plus leaf placement
in `src/dharmatiles/trees/mesh.py`.

Today, `build_leaf_mesh` builds a self-contained blade and optional keel. The
keel projects below the leaf plane along `-N`, so it only embeds cleanly when
the leaf top normal also equals the supporting surface normal. That breaks on
lower foliage-clump regions:

- `_emit_leaf` passes `up_hint = surface_normal`.
- `surface_normal` is the outward radial direction from the foliage clump.
- Upper hemisphere leaves have `surface_normal.z > 0`, so their top face points
  up and outward.
- Lower hemisphere leaves have `surface_normal.z < 0`, so their top face points
  downward or inward and can become an unsupported overhang.
- The keel is built in `-N`; when `N` points down, the keel can protrude
  outward/upward into open air.

Small local fixes were considered:

- clamp `N` toward world-up,
- taper lower-hemisphere leaf density,
- drop the keel,
- replace the keel with a raised midrib boss,
- add a petiole or grown neck.

Those are useful stepping stones but not sufficient. A neck-only model does not
support the full realistic leaf surface. A drooping petiole creates an
unsupported hook. A raised midrib boss conflicts with the desired creased leaf
anatomy. The robust model is a support volume under the leaf.

## Terms

**Base mesh (`M`)**: the supplied tree, branch, foliage clump, or arbitrary mesh
the leaf attaches to. It is assumed to be already printable or treated as fixed
support.

**Visible leaf (`L`)**: the realistic thin leaf surface. This is what should
read visually as the leaf.

**Support volume (`S`)**: generated hidden geometry under the visible leaf. It
connects `L` to `M` and makes the combined object printable.

**Output object (`O`)**: the combined geometry `M + S + L`, usually represented
as overlapping closed shells rather than expensive CSG booleans.

**Leaf frame**:

- `B`: base point,
- `T`: tip point,
- `A`: unit base-tip axis,
- `W`: unit lateral width axis,
- `N`: average top normal of the visible leaf,
- `D`: world down `(0, 0, -1)`,
- `U`: world up `(0, 0, 1)`.

**Support footprint**: the subset of the leaf underside that the support volume
must attach to and support.

**Bottom contact patch**: the patch of `S` embedded into `M`.

**FDM floor (`beta`)**: minimum printable underside plane angle above
horizontal. Examples:

- `0 deg`: horizontal underside, unprintable,
- `45 deg`: common conservative FDM limit,
- `90 deg`: vertical wall.

Equivalently, if using slicer-style max overhang from vertical `alpha`,
`beta = 90 deg - alpha`.

## Visible Leaf Surface

The leaf surface generator should be independent from attachment. It produces a
placed, visible cap plus metadata for support generation:

- top vertices and faces,
- underside vertices or underside sample points,
- perimeter ring with stable winding,
- base and tip anchors,
- support-footprint coordinates,
- optional shape metadata such as centerline coordinate, side, curl amount, and
  local normal.

The default surface keeps the current visual design:

- teardrop shape,
- rounded base,
- pointed tip,
- maximum width roughly one third from base to tip,
- central crease valley,
- convex raised lobes on both sides of the crease,
- slight thickness for robustness and easy meshing.

Variation should be applied to the visible surface before support generation:

- yaw, pitch, and roll around the base,
- tip curl,
- longitudinal bend,
- side-to-side twist,
- size and pose jitter.

The support solver then decides whether the resulting posed leaf can be printed.
If the support would be invalid, the leaf pose is rejected or retried.

## Placement Model

A candidate leaf pose is defined by:

- a base or anchor sample on `M`,
- leaf length, width, and surface style,
- yaw around the local support normal or a caller-provided placement axis,
- pitch around the lateral axis,
- roll around the base-tip axis,
- optional curl and bend parameters.

The desired default placement is not "realistic drooping leaves everywhere".
The desired visual result is many leaves roughly pointing upward with small
twist and jitter. That keeps support volumes small and avoids huge wedges under
downward leaves.

The leaf base may touch the base mesh, but it does not have to. A lifted leaf is
valid if the support volume connects it to `M` and satisfies all printability
constraints. Slight lifting can look better than forcing every leaf butt to lie
flat on the surface.

## Support Volume Model

The support volume is not merely a curve, petiole, or keel. Those can be used as
control structures, but the validated object is the solid support hull.

The support volume has three jobs:

1. Attach to `M` with enough embedded overlap for slicer-visible fusion.
2. Attach to and support the leaf underside.
3. Be self-printable from `M` upward, with no floating islands, hooks, thin
   walls, or unsupported undercuts.

The support may look like one of these styles:

- **Full draped underside**: safest, but can look thick from the side.
- **Inset underbody**: preferred first target; preserves a thin visible leaf
  edge while supporting most of the underside.
- **Hybrid saddle**: central/inset body supporting the first 20-60% of the leaf,
  with less support near the tip when legal.
- **Ribbed underbody**: visually lighter but riskier because gaps can become
  bridges or unsupported islands.

The earlier "branchlet" idea is best understood as one construction method for
`S`: a sequence of rings or slices grown from the leaf underside down toward
the supplied mesh.

## Support Footprint

The support footprint decides which parts of the leaf underside receive direct
support. Options:

1. **Full underside footprint**: maximum print safety, least delicate.
2. **Inset underside footprint**: support starts inside the visible perimeter,
   leaving a thin leaf edge. This is the likely default.
3. **Base + midsection footprint**: supports the base and middle, allowing a
   short printable cantilever near the tip.
4. **Multiple ribs**: supports the centerline and side lobes separately. This
   needs stronger validation because gaps can create bridges.

For first implementation, use an inset teardrop footprint:

- offset inward from the visible perimeter by roughly `0.3-0.8 mm`,
- cover the base and most of the body,
- taper before the tip if the tip is already legally supported,
- keep every unsupported visible-leaf sample inside a valid FDM support cone
  from lower support geometry.

## FDM Constraints

Use both a constructive slice rule and a final mesh face-normal rule. The
normal rule alone is not enough because it can miss floating islands and
unsupported slice ancestry.

### Slice Ancestry Rule

For a printable support volume, every upper slice must be supported by lower
geometry. In 2-D slice terms:

```text
S(z + dz) must lie inside dilate(S(z), tan(beta) * dz)
```

For `beta = 45 deg`, this means each upper slice can expand sideways by at most
the vertical rise since the previous slice.

This catches:

- hooks,
- sudden ledges,
- inverted mushrooms,
- floating rings,
- unsupported islands,
- inverted obelisk supports that flare too aggressively.

An equivalent pointwise cone test:

```text
for upper point p:
    exists lower point q
    q.z < p.z
    xy_distance(p, q) <= tan(beta) * (p.z - q.z)
```

### Face-Normal Rule

Every exterior face must also obey the FDM underside limit.

Let `n` be the outward unit normal of a face. For downward-facing faces
(`n.z < 0`), the face's plane angle above horizontal is:

```text
plane_angle_deg = degrees(acos(abs(n.z)))
```

The face passes when:

```text
plane_angle_deg >= beta
```

Equivalent normal limit:

```text
n.z >= -cos(beta)
```

If using slicer-style max overhang from vertical `alpha = 90 deg - beta`, this
is the same as:

```text
n.z >= -sin(alpha)
```

Known checks:

- horizontal downward face: `n.z = -1`, fails,
- vertical wall: `n.z = 0`, passes,
- 45-degree underside: `n.z ~= -0.707`, passes at `beta <= 45 deg`.

### Minimum Thickness Rule

No printable feature may be thinner than `w_min`.

Suggested values:

- absolute lower bound: `0.8 mm` for a 0.4 mm nozzle,
- preferred lower bound: `1.0-1.2 mm`,
- root contact and neck-like regions should usually be larger.

This applies to:

- support ribs,
- thin side walls,
- final contact patch,
- branch points in the support volume,
- near-touching but non-fused surfaces,
- leaf tip support if the visible tip is thickened.

## Hard Validity Constraints

A generated leaf is valid only if all hard constraints pass.

1. **Connected solid**
   `S` overlaps or embeds into `M`, `S` overlaps or embeds into `L`, and
   `M + S + L` has one connected printed component.

2. **Embedded bottom contact**
   The bottom contact patch penetrates `M` by `embed_depth`. Tangent contact is
   not enough.

3. **Leaf underside support**
   Every visible-leaf underside sample either belongs to the support footprint
   or has lower support within the legal FDM cone.

4. **Self-printable support**
   Every upward slice of `S` has ancestry to lower geometry according to the
   slice rule.

5. **Face-normal printability**
   Every exterior face passes the normal rule.

6. **No unsupported islands**
   Each connected island in a slice must overlap or be legally supported by
   lower slices.

7. **Minimum printable thickness**
   Every local feature is at least `w_min`, or the candidate fails.

8. **No self-intersection**
   Support walls must not fold through themselves. Rings must not invert, pinch
   to zero area, or create accidental side contact.

9. **Intentional contact only**
   Allowed intersections are the embedded contact with `M` and the attachment
   to `L`. Forbidden cases include support passing through `M` midway, exiting
   and re-entering `M`, or the leaf piercing the base mesh accidentally.

10. **Aesthetic sanity**
    Technically printable but visually absurd supports should be rejected.

## Aesthetic Constraints

These are soft costs during generation and hard reject thresholds after a
candidate becomes too strange.

Penalize:

- support volume larger than the leaf,
- tall skinny spikes,
- inverted pyramids or obelisks,
- fat wedges under small leaves,
- support visible outside the leaf silhouette,
- sudden cross-section growth,
- abrupt ring twist,
- excessive curvature,
- support surfaces that visually dominate the leaf.

Useful metrics:

- `support_volume / leaf_area`,
- maximum support height,
- maximum support width,
- projected support area outside the leaf silhouette,
- cross-section area growth per step,
- centerline curvature,
- support footprint coverage ratio.

Plain rule: if the hidden support is visually more important than the leaf,
reject the pose.

## Ring-Based Construction

A first implementation can build `S` as a ringed loft. Each ring is a horizontal
or near-horizontal slice/control loop of the support volume.

Inputs:

- visible leaf underside samples,
- support footprint on the leaf underside,
- target base mesh `M`,
- `beta`,
- `w_min`,
- `embed_depth`,
- support visibility limits,
- maximum retries.

Suggested algorithm:

1. Generate the visible leaf candidate.
2. Choose an inset support footprint on its underside.
3. Resample the footprint into one or more closed loops.
4. Find candidate target contact patches on `M` reachable inside legal downward
   cones from the support footprint.
5. Choose target patches by distance, support volume, visibility, and smoothness
   cost.
6. Build ring paths from the leaf support footprint down to the embedded target
   patch.
7. Loft a watertight support hull from rings.
8. Add or preserve an embedded bottom cap.
9. Validate slice ancestry, face normals, thickness, self-intersection,
   collisions, and aesthetic thresholds.
10. Accept, retry with a safer pose/support, or reject.

Rings must obey:

- simple closed loops,
- consistent winding,
- gradual area changes,
- no inversion,
- no non-adjacent side contact unless intentionally fused,
- centroid shifts constrained by the overhang floor,
- local thickness above `w_min`.

Retry order:

1. reduce leaf pitch/droop,
2. reduce curl or twist,
3. increase support footprint coverage,
4. reduce undercut/shrink,
5. add intermediate rings,
6. use a closer/wider contact patch,
7. reject.

## Direction Preferences

Generation should prefer support that extends:

- away from the leaf average top normal (`-N`),
- toward world ground (`D`),
- toward the nearest reachable base mesh region.

Those are preferences only. They cannot override the hard FDM and connectivity
rules.

Useful target search rule:

```text
candidate target q is legal for point p when:
    q.z < p.z
    xy_distance(p, q) <= tan(beta) * (p.z - q.z)
```

Targets outside the downward print cone are not valid unless another existing
or generated lower support provides a legal ancestry path.

## Arbitrary Mesh Requirements

The base mesh should provide reliable local queries:

- closest point,
- ray or proximity query,
- outward normal,
- inside/outside or signed distance if available,
- collision/intersection checks,
- connected-component checks after assembly if possible.

Watertight manifold meshes are preferred. Non-watertight meshes may still work
if local contact and collision queries are reliable, but failure should be
conservative. If the solver cannot prove embedded contact and print ancestry,
the leaf candidate fails.

## Bonus: Air Joint / Leg Case

A support may connect to a point in the air only if that point already has a
valid printable ancestry path to `M`.

Model this as a support dependency graph:

- graph nodes are support rings, patches, or joints,
- directed edges point from lower support to higher supported geometry,
- every node must have a path to `M`,
- every edge must satisfy the FDM overhang rule,
- every generated feature must satisfy minimum thickness.

Valid:

- a printable leg rises from `M`,
- an air joint sits on top of that leg,
- a leaf support connects to the air joint without undercuts.

Invalid:

- floating air joint,
- leg that hooks downward,
- leaf connected to a joint with no ancestry path to `M`.

## Public Configuration

Initial configuration can be threaded through `Tree` and `build_branch_mesh`:

```python
leaf_support_volumes: bool = True
leaf_support_mode: str = "inset_underbody"
leaf_support_min_plane_angle_deg: float = 45.0
leaf_support_min_width_mm: float = 0.9
leaf_support_preferred_width_mm: float = 1.1
leaf_support_embed_mm: float = 0.35
leaf_support_inset_mm: float = 0.45
leaf_support_max_volume_ratio: float = 1.5
leaf_support_max_visible_outside_ratio: float = 0.2
leaf_support_ring_spacing_mm: float = 0.35
leaf_support_max_retries: int = 6
```

The existing keel settings should become legacy controls:

```python
leaf_keel_depth_mm
leaf_keel_tip_angle_deg
```

Once support volumes are proven, keel generation should be disabled by default.

## Error Semantics

Support generation has three outcomes:

1. **Success**
   Emit the visible leaf and its printable support volume.

2. **Rejected placement**
   This sampled pose cannot be made printable or visually acceptable. The caller
   may retry a different pose or skip the leaf.

3. **Configuration error**
   The settings are impossible in general, such as invalid FDM angle,
   non-positive `w_min`, impossible embed depth, or support footprint larger
   than the visible leaf.

The renderer should treat individual rejected placements as normal sampling
failures. It should raise or warn when rejection rates exceed a configured
threshold, because that means the requested leaf density or pose distribution is
not compatible with printability.

## Implementation Plan

1. Split the current leaf builder into a visible surface builder and an assembly
   layer.
2. Make the visible surface return underside samples, perimeter, base/tip, and
   support-footprint candidates.
3. Add support-volume generation behind a feature flag.
4. Implement and test the FDM face-normal helper.
5. Implement conservative slice ancestry validation for generated support
   volumes.
6. Add minimum-width checks for rings and contact patches.
7. Add collision classification against `M` and `L`.
8. Add aesthetic thresholds and debug rejection counters.
9. Replace keel generation with support volumes for new foliage leaves.
10. Deprecate keel parameters after visual and print validation.

## Tests

Focused tests:

- default visible leaf keeps base, tip, crease, lobes, and teardrop perimeter,
- support footprint is inset and consistently wound,
- horizontal downward faces fail the normal test,
- vertical faces pass the normal test,
- 45-degree undersides pass at a 45-degree floor,
- support slice expansion fails when it exceeds `tan(beta) * dz`,
- floating upper support islands fail,
- generated support embeds into a simple base mesh,
- too-thin support ribs fail,
- ring inversion or self-intersection fails,
- technically printable but over-large supports fail aesthetic thresholds,
- deterministic seeds produce identical accepted/rejected placements.

Integration tests:

- tree mesh with support-volume leaves remains exportable,
- rejected placements do not crash tile generation,
- high rejection rate reports useful debug reasons,
- legacy tree configs can run with support volumes disabled,
- arbitrary simple meshes can receive valid leaves or reject invalid poses.

## Open Questions

- What support footprint gives the best miniature-scale balance: full underside,
  inset underside, base-plus-midsection, or ribs?
- What visual thresholds should reject supports as too strange?
- How much side thickness is acceptable before leaves stop reading as thin?
- How exact do booleans need to be, versus overlapping closed shells for slicer
  fusion?
- What is the most robust local thickness check available with Trimesh for this
  scale?
- Should failed leaf poses be locally adjusted by the support solver, or should
  the caller own pose retries?
- How much lower-taper foliage should receive leaves versus being intentionally
  sparse?
