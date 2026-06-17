# Design: CloudTree Foliage Leaf Embossing

Status: design - 2026-06-16

## Goal

Add pointed oval leaf shapes as surface relief on every CloudTree foliage clump.
The texture should make each foliage cluster read as a mass of overlapping
leaves while preserving the current printability guarantees of the tree mesh.

Required behavior:

- every foliage clump receives leaf-shaped relief,
- each leaf is oval with a pointed tip,
- leaf shapes overlap in a deterministic, visually plausible order,
- unembossed foliage surface receives subtle random texture below the leaf
  relief layer,
- leaf tips point in one shared world-space direction by default: downward
  toward the ground,
- each leaf has small deterministic orientation jitter,
- relief must not create FDM-unprintable floating cantilevers, especially on
  downward-facing foliage cone surfaces.

## Current Context

The live implementation target is `dharmatiles.trees.cloud_mesh`.

Current foliage is not a separate leaf mesh. A terminal branch edge expands its
radius profile from wood radius to `leaf_clump_radius_mm`, optionally over the
last `leaf_clump_length_mm`, and receives a rounded dome tip. Each edge is built
as a closed swept solid and the tree is assembled by `trimesh.boolean.union`
with the `manifold` engine.

That means leaf embossing should be generated as part of the foliage edge solid
itself, by displacing foliage ring vertices before the per-edge solid is
returned. It should not add separate leaf plaques, decals, or post-union meshes.

The closest existing pattern is bark: bark modifies ring vertex radii during
mesh construction. Leaf embossing should follow that style, but only on foliage
regions and with explicit support-aware relief rules.

## Public Configuration

Add a foliage embossing config and thread it from `CloudTree` to
`build_cloud_tree_mesh`.

```python
@dataclass(frozen=True)
class LeafEmbossConfig:
    enabled: bool = True
    density_per_sq_mm: float = 0.035
    length_mm: float = 4.2
    width_mm: float = 2.1
    relief_height_mm: float = 0.18
    outline_depth_mm: float = 0.08
    midrib_height_mm: float = 0.07
    direction: tuple[float, float, float] = (0.0, 0.0, -1.0)
    direction_jitter_deg: float = 18.0
    size_jitter: float = 0.25
    overlap_shadow_depth_mm: float = 0.05
    background_noise_amplitude_mm: float = 0.045
    background_noise_cell_mm: float = 0.55
    min_printable_overhang_deg: float = 35.0
    underside_raise_limit_mm: float = 0.0
    seed: int = 0
```

Field meanings:

| Field | Meaning |
|---|---|
| `enabled` | Enables leaf relief on foliage clumps. |
| `density_per_sq_mm` | Approximate leaf stamp count per foliage surface area. |
| `length_mm`, `width_mm` | Mean leaf dimensions. |
| `relief_height_mm` | Maximum raised height on support-safe surfaces. |
| `outline_depth_mm` | Recessed outline/valley depth. |
| `midrib_height_mm` | Additional raised center vein on support-safe surfaces. |
| `direction` | World-space direction the leaf tip points toward. Default is groundward. |
| `direction_jitter_deg` | Per-leaf tangent-plane angular jitter. |
| `size_jitter` | Fractional leaf size variation. |
| `overlap_shadow_depth_mm` | Narrow groove where a top leaf crosses a lower one. |
| `background_noise_amplitude_mm` | Subtle all-over foliage texture below leaf relief. |
| `background_noise_cell_mm` | Approximate surface-space cell size for deterministic random noise. |
| `min_printable_overhang_deg` | Minimum printable surface elevation above horizontal. |
| `underside_raise_limit_mm` | Maximum outward raised relief allowed on downward-facing surfaces. Default zero. |
| `seed` | Extra deterministic seed mixed with the tree seed. |

Defaults should be conservative. The relief should be visible after slicing, but
small compared with the foliage radius so it cannot dominate branch thickness.

## Leaf Coordinate Model

Each foliage edge already has a sampled Bezier curve, a transported ring frame,
and a radius profile. During foliage ring construction, derive a local surface
parameter for every ring vertex:

```text
s     = arc length along the foliage edge
theta = angle around the local ring
p     = surface position
n     = outward surface normal approximation
```

Leaf stamps live in this `(s, theta)` surface domain. Distances in the `theta`
direction are measured as arc length:

```text
u_mm = radius(s) * wrapped_angle_delta(theta, leaf_theta)
v_mm = s - leaf_s
```

The implementation may treat each foliage edge independently for the first
pass. Cross-edge leaf continuity is not required because each terminal foliage
clump is already its own visual unit before boolean union.

## World-Space Direction

For each candidate leaf center, compute the tangent-space direction that best
matches the configured world direction.

```text
g = normalize(config.direction)
n = outward surface normal at the leaf center
d = g - dot(g, n) * n
```

If `|d|` is large enough, `normalize(d)` is the leaf's centerline direction on
the surface. This makes all leaves point in the same world-relative direction:
with the default `(0, 0, -1)`, their tips run down the clump toward the ground.

If the projection is nearly zero, the world direction is normal to the local
surface and there is no meaningful tangent direction. In that case:

1. prefer the local direction of decreasing world `z` if neighboring surface
   samples provide one,
2. otherwise skip the stamp on that patch rather than inventing an arbitrary
   orientation.

Skipping near-degenerate top or bottom patches is acceptable. It avoids random
leaf directions on nearly horizontal caps and removes the worst underside
cantilever cases.

After selecting the tangent direction, apply a deterministic jitter rotation in
the tangent plane:

```text
jitter = uniform(-direction_jitter_deg, +direction_jitter_deg)
```

The jitter changes local variation without breaking the global trend.

## Leaf Shape

Use a compact analytic stamp. In leaf-local coordinates, `x` is across the leaf
and `y` runs from base to tip:

```text
y_norm = clamp(y / length, -0.5, 0.5)
tip_t  = y_norm + 0.5        # 0 at base, 1 at tip
half_width(y) = width * 0.5 * sin(pi * tip_t) ** 0.55 * (1.0 - 0.35 * tip_t)
```

The point is enforced by `half_width(1) = 0`. The base is narrower and rounded,
while the body stays oval.

A point belongs to the leaf if:

```text
abs(x) <= half_width(y)
```

The relief profile has three components:

- leaf body: a smooth raised dome on support-safe surfaces,
- outline: a shallow recessed groove around the silhouette,
- midrib: a narrow raised vein along the centerline on support-safe surfaces.

The body profile should be smoothstep based, not a vertical wall:

```text
edge = abs(x) / max(half_width(y), eps)
body = (1 - edge**2) * sin(pi * tip_t)
body = smoothstep(0, 1, body)
```

The outline groove is a narrow band near `edge = 1`. It is always inward or
neutral; it never protrudes outward from the surface.

## Background Foliage Noise

Leaf stamps will not cover every vertex. Add a low-amplitude random texture
under the leaf layer so exposed foliage still reads as organic surface rather
than smooth cone.

The background texture is evaluated for every foliage vertex before leaf
composition:

```text
noise = value_noise(surface_x / background_noise_cell_mm,
                    surface_y / background_noise_cell_mm,
                    tree_seed,
                    edge_id)
noise_delta = background_noise_amplitude_mm * remap(noise, -1, 1)
```

The noise coordinate system should be stable in foliage surface space, not world
XYZ, so it follows the clump without creating vertical banding. A first
implementation can use quantized `(s, radius * theta)` cells with bilinear
interpolation and deterministic hashes. Smooth interpolation matters: single
vertex white noise will produce faceted print artifacts instead of natural
texture.

This noise is visually below the leaf embossing:

- where no leaf covers a vertex, the noise is visible at full strength,
- inside a raised leaf body, fade noise down so the leaf shape stays readable,
- inside outline and overlap grooves, let the groove displacement dominate,
- near the wood-to-foliage transition and dome apex, fade noise with the same
  relief fade used for leaf stamps.

Noise follows the same FDM support gate as leaf relief. On support-safe surfaces
it may be positive or negative. On downward-facing surfaces it must be clamped
to inward-only displacement unless `underside_raise_limit_mm` explicitly allows
otherwise.

## Leaf Placement

For each foliage region on a leaf edge:

1. Estimate available surface area from ring-to-ring quads.
2. Compute stamp count:

   ```text
   n_leaves = poisson(area * density_per_sq_mm)
   ```

3. Sample candidate centers on the foliage part of the edge, excluding:
   - the first transition band after `t_split`,
   - the final dome apex where tangent direction is unstable,
   - patches where the projected world direction is degenerate,
   - patches where the local foliage radius is too small for `width_mm`.
4. Assign deterministic size, rotation jitter, and layer priority from
   `hash(tree_seed, edge_id, leaf_id, config.seed)`.

Placement does not need hard collision rejection. Overlap is intentional.
However, avoid centers closer than `0.25 * width_mm` in surface distance so that
the texture does not collapse into noise.

## Overlap Composition

Leaves should overlap as if stamped in layers, not simply summed.

For every ring vertex, evaluate all stamps whose bounding boxes contain the
vertex. Each stamp produces:

```text
height_delta
outline_delta
layer_priority
inside_mask
edge_mask
```

Composition rule:

1. The visible raised body is the stamp with the highest effective layer among
   stamps that cover the point.
2. Recessed outline grooves are combined by taking the most inward displacement.
3. Where a higher-priority leaf overlaps a lower-priority leaf near its
   silhouette, apply `overlap_shadow_depth_mm` as a narrow inward crease on the
   lower leaf side.

This gives visible leaf-over-leaf crossings without creating multiple detached
surfaces. The output remains one displaced foliage surface.

Layer priority should be mostly deterministic random with a slight world-down
ordering bias:

```text
priority = random_priority + 0.15 * dot(center_position, -normalize(direction))
```

With the default groundward direction, lower leaves tend to sit visually above
leaves uphill of them, which reads naturally on drooping foliage.

## FDM Printability Rules

The critical rule is that embossing must never add unsupported outward material
to already risky downward-facing foliage.

Classify each vertex by outward normal `n` and print direction `up = (0,0,1)`.

```text
normal_up = dot(n, up)
```

Use the existing FDM convention: a surface is printable when its local tangent
does not create an overhang below the configured elevation. For displacement
relief, use a conservative normal-based gate:

```text
raise_allowed =
    normal_up >= -sin(90deg - min_printable_overhang_deg)
```

Practical first-pass simplification:

- upward and side-facing surfaces may receive raised body and raised midrib,
- downward-facing surfaces receive only inward outline/engraving,
- surfaces near the threshold fade raised relief to zero over a small band.

The final displacement is:

```text
background    = support_clamped_background_noise
outward_raise = support_weight * raised_profile + max(background, 0)
outward_raise = min(outward_raise, underside_raise_limit_mm) when normal_up < 0
inward_cut    = max(outline_cut, overlap_shadow_cut) + max(-background, 0)
delta_normal  = outward_raise - inward_cut
```

With the default `underside_raise_limit_mm = 0.0`, the underside never gains
new outward/downward material. It only receives shallow incised leaf marks.
That preserves visual leaf texture while avoiding floating cantilevers under
foliage cones.

Also enforce local thickness constraints:

- cap total inward displacement to at most `0.25 * local_radius`,
- never reduce effective radius below the same minimum safety radius used by
  bark (`0.42 mm` today),
- fade relief to zero within one ring of the wood-to-foliage transition and
  within the terminal dome apex.

## Mesh Integration

Implementation should be contained in `cloud_mesh.py` at first.

Suggested structure:

```text
LeafEmbossConfig
_LeafStamp
_make_leaf_stamps(...)
_foliage_background_noise(...)
_leaf_direction_at(...)
_leaf_relief_at_vertex(...)
_compose_leaf_relief(...)
```

`_build_closed_edge_solid` already computes:

- Bezier samples,
- arc lengths,
- radii per sample,
- transported frames,
- ring vertices.

Extend that loop so foliage rings call a foliage-aware ring builder:

```text
if is_foliage_leaf and t >= t_split:
    ring = _make_foliage_ring_with_leaf_relief(...)
else:
    ring = _make_ring(...)
```

The relief builder should:

1. construct the base circular or bark-modified ring,
2. estimate normal at each ring vertex,
3. evaluate support-clamped background noise in surface coordinates,
4. evaluate leaf relief in surface coordinates,
5. compose noise, leaf body, outlines, and overlap grooves,
6. displace the vertex along the local outward direction,
7. clamp displacement using the printability and thickness rules.

Bark and leaf embossing should not both modify the same foliage surface. Bark
already tapers off at foliage. Leaf embossing starts after that taper.

## Material Handling

The current tree mesh is tagged as `Material.WOOD` after union, even though a
`Material.FOLIAGE` enum exists. Leaf embossing does not require material splitting
to work. It is geometric relief.

A later improvement may split wood and foliage into separate material parts, but
that is outside this design. Avoid coupling embossing to material export.

## Tests

Add unit tests around the geometry helpers before relying on rendered snapshots.

Required tests:

- leaf analytic shape has zero width at the tip and a wider oval body,
- projected default direction points toward decreasing world `z` on side-facing
  surfaces,
- degenerate projection returns "skip" rather than arbitrary orientation,
- overlap composition is winner-takes-top for raised body and max-inward for
  grooves,
- support gate returns zero outward raise for downward-facing normals by
  default,
- inward relief never reduces effective radius below the minimum safe radius,
- background noise is smooth across adjacent surface cells and is suppressed
  under raised leaf bodies.

Add an integration test that builds a small deterministic CloudTree with
`leaf_clumps=True` and leaf embossing enabled, then asserts:

- mesh is non-empty,
- mesh is watertight after union,
- no vertex on downward-facing foliage moved outward compared with the same tree
  built with embossing disabled beyond a small numerical tolerance,
- triangle count increase stays within an expected bound.

## Visual Verification

Generate the existing `2x2-grass-cloud-tree` tile with leaf embossing enabled
and inspect:

- side view: leaves point groundward with jitter,
- top view: no obvious radial or per-branch random direction pattern,
- underside view: no raised leaf lips under cones,
- slicer preview: no isolated islands or unsupported cantilever shelves under
  foliage.

At least one debug render should color downward-facing faces or risky overhang
faces so printability regressions are easy to see.

## Open Questions

- Should underside engraving be enabled by default, or should downward-facing
  patches be skipped entirely for a cleaner silhouette?
- Should leaf density scale with clump radius so tiny clumps get fewer larger
  stamps instead of many clipped stamps?
- Should foliage eventually become a separate `Material.FOLIAGE` mesh part?
- Should leaf stamps wrap continuously across the ring seam, or is seam-local
  clipping acceptable for the first implementation?
