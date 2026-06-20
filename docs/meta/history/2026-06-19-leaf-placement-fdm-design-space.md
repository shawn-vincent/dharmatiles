# Leaf Placement & Keel Design Space Exploration
*2026-06-19*

## The Problem

Debug images show red-highlighted leaves on foliage clusters where the leaf blade's top
face is pointing inward or downward into the cluster, and the keel protrudes outward
into open air.  These leaves are both visually broken (the "wrong" face is visible) and
unprintable (the blade top face is a downward-facing overhang, violating FDM constraints).

### Root Cause

`_emit_leaf` in `mesh.py` passes `up_hint = surface_normal` to `build_leaf_mesh`, where
`surface_normal` is the outward radial direction from the cluster center.  This means the
leaf's N-axis (its "top" / crease direction) always equals the outward radial.

On the **upper hemisphere** of the cluster, the outward radial points upward.  N.z > 0.
The leaf's top face faces upward or outward.  Printable. ✓

On the **lower hemisphere**, the outward radial points downward.  N.z < 0.  The leaf's
top face faces downward.  Unprintable overhang. ✗

The `keel_depth_mm = 1.0` keel is built in the `-N` direction (the leaf underside).  When
N points down, the keel protrudes upward — away from the cluster — floating in air.

The issue is visible for any leaf at a polar angle below the cluster equator.  With the
current default `leaf_tilt_deg = 45°` dip, the leaf tip descends further below the surface
plane, making the problem more pronounced.

---

## Design Space

### Axis 1 — Leaf Normal (N-axis) sourcing

The most fundamental choice: what direction does the leaf's "top" face point?

#### A. Surface-normal driven (current)
`N = surface_normal` — the leaf always faces away from the cluster center.

- Upper hemisphere: excellent. Face points up-and-out.
- Equatorial band: face points horizontally. Just barely printable.
- Lower hemisphere: face points down-and-in. Broken. ✗
- Keel always embeds cleanly into the cluster surface.

#### B. World-up clamped
`N = normalize(surface_normal + clamp_lift * world_up)` where `clamp_lift` is added only
when `surface_normal.z < min_nz_threshold`.  Equivalently: if `surface_normal.z < threshold`,
tilt N toward world-up until it meets the threshold.

Concrete form:
```python
min_nz = np.sin(np.radians(fdm_floor_deg))   # e.g. sin(26°) ≈ 0.44
if surface_normal[2] < min_nz:
    n_adj = surface_normal.copy()
    n_adj[2] = min_nz
    n_adj = _safe_norm(n_adj)
else:
    n_adj = surface_normal
```

- Upper hemisphere: unchanged, as good as A.
- Equatorial band: unchanged.
- Lower hemisphere: N is rotated toward horizontal/upward.  Top face is now printable.
- Keel: still points in -N direction.  For lower hemisphere leaves, -N now points
  at an angle rather than straight into the cluster.  The keel base ridge falls
  partially inside and partially outside the cluster body — the keel is not cleanly
  embedded, but there is still overlap (partial embedding) near the leaf base.
- Simplest implementation path.  Single 3-line guard in `_emit_leaf`.

#### C. World-up blend (smooth)
`N = normalize(alpha * world_up + (1-alpha) * surface_normal)` where alpha rises
smoothly from 0 on the upper pole to 1 at the equator and stays 1 on the lower hemisphere.

- Gives a continuous transition: leaves near the top are surface-normal driven; leaves
  near the equator blend toward world-up.
- Less abrupt than B; but the equatorial leaves are already fine, so the blend mostly
  changes leaves that weren't broken yet.
- Keel geometry degrades identically to B on the lower hemisphere.

#### D. Always world-up
`N = world_up` for every leaf, everywhere.

- Every leaf face points upward. All printable.
- Keel always points straight down (-world_up direction).
- On the upper hemisphere: keel digs DOWN into the cluster center from above.  Works.
- On the lower hemisphere: keel hangs downward below the cluster bottom.  Nothing to embed into.
- On the equatorial band: keel points straight down, neither into nor away from the cluster.
- Visually: all leaves lie flat (horizontal), regardless of their cluster position.
  Less naturalistic but very clean, scales-on-a-pinecone aesthetic.

---

### Axis 2 — Leaf Tangent (growth direction)

Independent of N, the tangent controls which direction the leaf "grows" from the base.

#### P. Gravity-biased (current)
`T0` = gravity projected onto the surface tangent plane, then jittered.
`ltan = T0 * cos(dip) - surface_normal * sin(dip)`

At `dip = 45°` the tangent has a 70% downward component from both T0 and the inward
surface_normal contribution.  On lower-hemisphere leaves this creates very steep downward
tips, well below the attachment point.

#### Q. Radial outward with gravity droop
`ltan = surface_normal * cos(dip) - world_down * sin(dip_gravity)`

This separates two ideas:
- Primary direction: outward from cluster (surface_normal), so the leaf extends away
  from the cluster body.
- Secondary direction: gravity-induced droop (small downward component).

For `dip_gravity = 15°` the tip is mostly outward, slightly below horizontal.  Very
printable (the leaf tangent is close to horizontal), but requires the blade to have
enough thickness that the face is recognisable as a leaf.

#### R. Outward-then-droop with FDM floor
Same as Q but the total tangent elevation angle (above horizontal) is clamped to
`>= fdm_target_deg` (e.g., 35°).  If the gravity droop would push the tangent below
the FDM floor, it is reduced until it meets the floor.  This guarantees every leaf
tip is at or above the printability threshold.

---

### Axis 3 — Keel design

The keel currently embeds via the -N embedding constraint.  When N no longer equals
surface_normal, the keel can't embed cleanly.  Four keel alternatives:

#### K1. Drop the keel for non-upper leaves
`keel_depth_mm = 0` when `surface_normal.z < threshold`.  The leaf becomes a flat
blade with no structural ridge.  At 35mm miniature scale, thin blades can still print
without a keel if they are stiff enough (thickness > 0.8mm) or nearly horizontal.
Clean, simple, but structurally weaker.

#### K2. Midrib boss (above-surface ridge)
Instead of a keel projecting in -N, build a raised ridge projecting in +N on the top
surface.  The structural function (stiffening the blade) is preserved.  The midrib
boss prints as a raised bump on the upper face, which is always supported from below.

This breaks the current `build_leaf_keel_prism` abstraction but is geometrically
equivalent — just invert the direction.

A midrib boss on a printable (upward-facing) leaf is always printable because the boss
adds upward geometry from an already-upward surface.

#### K3. Surface-tangent keel (flush embedding)
Instead of burying the keel inside the cluster, curve the keel base to lie flush with
the cluster surface at the attachment point.  The keel ridge runs along the midrib but
its outermost edge is tangent to the cluster skin.

This requires the keel to know the cluster surface geometry at the attachment point —
nontrivial without passing the cluster radius to `build_leaf_keel_prism`.  Possible but
adds coupling.

#### K4. Petiole stem
Replace the keel with a cylindrical petiole (stem) connecting the cluster surface to
the leaf base.  The petiole is a small (0.6–1.2mm radius) tube, angled along the leaf
tangent, with its root embedded 0.5–1mm into the cluster surface.

This is the biologically accurate model (leaves connect to branches via petioles) and
decouples the leaf blade orientation from the embedding constraint entirely.  The blade
can face any direction; the petiole connects it to the surface.

Downsides: more geometry per leaf; the petiole may be too small to print at 35mm scale
unless it is at least 0.8mm diameter (roughly 2 nozzle widths).  At miniature scale a
petiole ≥ 1.2mm diameter is safe; this reads as a somewhat thick-stemmed leaf.

---

### Axis 4 — Placement filtering vs. full coverage

Rather than fixing the geometry, filter which positions receive leaves.

#### F1. Upper-hemisphere only
Only emit leaves where `surface_normal.z > min_nz`.  The lower cluster surface is bare.

This looks fine for a dense canopy viewed from above but reveals a bare green dome from
any side view below the midline.  Works well if clusters are tightly packed (no visible
bare underside).

#### F2. Upper + equatorial band, skip lower
`surface_normal.z > -0.2` — includes the equator and most sides, skips the bottom 20%.

Much better coverage.  The visible result from normal viewing angles (slightly above
horizontal) is dense leaves on all visible surfaces.

#### F3. Density taper
Reduce leaf density by `max(0, surface_normal.z + 0.3)`.  Leaves taper off smoothly as
you go from top to bottom, reaching zero density at `surface_normal.z = -0.3`.

Naturalistic: real trees have fewer leaves on the shaded underside.  Dense at the top,
sparse around the equator, bare at the bottom.

---

## Evaluated Options (tradeoff matrix)

| Option | FDM-safe | Keel embeds | Realism | Complexity |
|---|---|---|---|---|
| A. Current (broken) | ✗ lower hem. | ✓ | good | 0 |
| B. World-up clamp on N | ✓ | partial | good | low |
| B + K2 (midrib boss) | ✓ | n/a (boss) | good | medium |
| D. Always world-up | ✓ | ✗ lower hem. | flat | low |
| Q + B (outward tangent + N clamp) | ✓ | partial | very good | low-medium |
| K4 petiole | ✓ | n/a | best | high |
| F2 equatorial filter | ✓ | ✓ | reduced | very low |
| F3 density taper | ✓ | ✓ | good | low |

---

## Recommended Path

### Immediate fix (low risk)

Combine **B (world-up clamp on N)** with **F3 (density taper)**.

1. In `_emit_leaf`: after computing `surface_normal`, apply the FDM floor clamp to get
   `surface_normal_adj`.  Use `surface_normal_adj` as `up_hint`.
2. In the leaf placement loop: scale the per-leaf emission probability by
   `np.clip(surface_normal.z + fdm_floor_blend, 0.0, 1.0)`.  With
   `fdm_floor_blend = 0.5` this fades leaves out from `surface_normal.z = -0.5` (no
   leaves) to `surface_normal.z = 0.5` (full density).

This is ~10 lines of code.  The keel is partially embedded in the cluster near the
equator; on the lower hemisphere the keel hangs slightly free but is visually hidden by
the cluster body above it and the leaf blade in front.

The density taper means there are fewer leaves near the bare zone, so the transition
is not jarring.

### Medium-term improvement

Replace the keel with **K2 (midrib boss)** on leaves where `surface_normal.z < 0.2`.
The boss is always printable (raised surface from a face that is already facing up).
The keel is retained for upper-hemisphere leaves where it embeds cleanly.

This requires a small extension to `build_leaf_mesh` to support both a keel and a boss
depending on a `keel_direction` parameter (`-N` for keel, `+N` for boss).

### Long-term option

**K4 petiole model** gives the most realistic result and fully decouples leaf blade
orientation from attachment.  Worth pursuing if the leaf visual quality is a priority,
but requires significant rework of the leaf mesh API and the cluster attachment logic.
Hold until the immediate fix is proven.

---

## Parameter Sensitivity Notes

Current defaults that interact with the problem:

| Parameter | Default | Problem relationship |
|---|---|---|
| `leaf_tilt_deg` | 45° | Larger values push the tip further below the surface plane, worsening lower-hemisphere leaves |
| `leaf_keel_depth_mm` | 1.0 | Deeper keels are more visible when they float; reducing to 0.5 makes the floating less obvious |
| `leaf_length_mm` | 8.0 mm | Longer leaves extend further below the cluster when tilted down |
| `_LEAF_UPPER_FLATTEN` | 0.55 | Reduces upward component on upper-facing surfaces; lower values make top-of-cluster leaves lie flatter (better for density, worse for thickness read) |
| `_LEAF_SURFACE_FLOOR_DEG` | 5° | Floor on leaf tangent elevation above the foliage surface plane — does NOT prevent the N-axis from pointing down |

The `_LEAF_SURFACE_FLOOR_DEG` constant (5°) sounds like it should prevent the problem
but it does not: it clamps the tangent elevation relative to the foliage *surface tangent
plane*, not relative to the world horizontal.  On a downward-facing foliage surface,
5° above the surface tangent plane is still a downward-pointing tangent in world space.

---

## Non-goals

- Do not attempt to fix FDM compliance by reducing `leaf_tilt_deg` alone.  At 0° the
  leaf has zero dip and lies flat in the surface tangent plane, but for lower-hemisphere
  leaves that tangent plane tilts toward vertical and the face still points inward.
- Do not attempt to reconstruct a full CSG boolean between leaves and the cluster
  (trimming leaves to the cluster surface).  The current concatenate-not-union approach
  is a deliberate performance choice; reversing it would 10× the leaf generation time.
- Do not add cluster-surface normals as a per-vertex attribute to the leaf placement
  loop.  The noise-displaced surface normal is unavailable without rebuilding the
  cluster; the pre-noise smooth surface normal is available and adequate.



=======

A conversation I had about this:  Ran out of tokens part way through but I want to explore this further:

Write(docs/meta/history/2026-06-19-leaf-placement-fdm-design-space.md)
Wrote 280 lines to docs/meta/history/2026-06-19-leaf-placement-fdm-design-space.md
 Leaf Placement & Keel Design Space Exploration
2026-06-19*
# The Problem

ebug images show red-highlighted leaves on foliage clusters whe
ace is pointing inward or downward into the cluster, and the ke
l protrudes outward
nto open air.  These leaves are both visually broken (the "wron
nprintable (the blade top face is a downward-facing overhang, v
olating FDM constraints).
… +377 lines

Document written. Here's the summary of what it covers:

---
Root cause

The leaf's N-axis (top-face direction) is set to surface_normal — the outward radial of the cluster. On the lower hemisphere that radial points downward, so the leaf's top face points down and the keel protrudes upward into open air. The red you're seeing in debug is the blade top face on leaves that are geometrically inverted relative to gravity.

The _LEAF_SURFACE_FLOOR_DEG = 5° constant looks related but isn't — it clamps tangent elevation relative to the local surface tangent plane, not world horizontal, so it doesn't help at all on the underside.

---
Key design axes explored

1. N-axis sourcing — surface-normal (current, broken), world-up clamp (simple fix), full world-up (all-flat result), smooth blend
2. Tangent direction — gravity-biased (current), outward-then-droop, outward with FDM floor clamp
3. Keel alternatives — keep (partial embed), drop on lower hemisphere, midrib boss (raised ridge on top face, always printable), petiole stem (decouples attachment entirely)
4. Placement filtering — upper-hemisphere only, equatorial cutoff, density taper (naturalistic fade)

---
Recommended path

Immediate (~10 lines): Clamp N toward world-up when surface_normal.z falls below sin(fdm_floor_deg), combined with a density taper so leaves fade out near the underside instead of appearing with bad geometry. The keel is partially embedded near the equator and barely visible on the lower zone.

Medium-term: Swap the keel for a midrib boss (raised ridge on the top face) for lower-hemisphere leaves — a boss on an upward-facing surface is always printable.

Long-term: A petiole stem model decouples leaf blade orientation from cluster attachment completely, is the most realistic, but requires reworking the leaf mesh API.

✻ Worked for 3m 24s

❯ Talk to me about petioole stem model.  What is it?

A petiole is the small stalk that connects a leaf blade to a branch in real botany. Think of a maple leaf — there's a thin stem between the branch and the flat part of the leaf. That stem is the petiole.

What the current model does

Right now, the leaf blade attaches directly to the cluster surface. The base vertex of the blade sits on the cluster skin, and the keel (the structural ridge on the underside) is expected to dig into the cluster to anchor it. This means the blade orientation and the attachment orientation are the same thing — you can't orient the blade upward without also pointing the keel upward (away from the surface).

That's the fundamental tension causing your red leaves.

What the petiole model would do

Separate the attachment point from the blade base into two distinct things:

cluster surface  →  [petiole stem]  →  leaf blade
     (anchor)          (tube)           (flat blade)

1. Anchor: a point on the cluster skin. The petiole root is embedded ~1mm into the cluster body here. The anchor has a direction (the outward radial of the cluster at that point).
2. Petiole: a short cylindrical tube (say 0.8–1.2mm radius, 2–4mm long) that runs from the anchor outward and then curves to point roughly downward or outward under gravity. Think of a small bent branch stub.
3. Leaf blade: attached at the far end of the petiole. Its N-axis is set to world-up (or gravity-clamped), independently of where the petiole points. The blade faces upward regardless of whether the anchor was on the top, side, or bottom of the cluster.

The keel is no longer needed at all — the petiole IS the structural connection. The blade can be a simple dome-shaped blade with no keel.

Why it fully solves the problem

The anchor on the cluster surface can be at any position, including the underside. The petiole bends away from the cluster and curves under gravity so its tip hangs somewhere nearby. The blade at the petiole tip can always face world-up, because the blade's orientation is set independently of the anchor direction.

A leaf on the bottom of the cluster would have: anchor pointing down, petiole curving down-and-out, blade hanging from the petiole tip with face up. Exactly like a real tree — leaves on the underside of a branch hang down but still face toward the sky.

What makes it complex

- The petiole is new geometry that doesn't exist in the codebase at all — a small swept tube, essentially a mini-branch.
- build_leaf_mesh currently assumes the blade starts at the base position. With a petiole, the blade starts at the petiole tip, which is offset from the cluster surface by 2–4mm. The API needs a new concept.
- The petiole needs to curve convincingly — straight petioles look like pins stuck in a ball. A slight Bezier bend is needed.
- At 35mm miniature scale, a 0.8mm petiole is exactly 2 nozzle widths. Printable, but right at the edge. 1.0mm is safer.
- The near-apex region of the cluster would need petioles pointing mostly upward (fine), but the density calculation changes because the petiole tip position differs from the anchor position — you need to ensure blades don't interpenetrate each other.

How it would look

More botanical and "leafy" than the current model. Real tree canopies have that characteristic looseness where leaves hang slightly off the cluster surface on their petioles rather than being plastered flat against it. At tabletop scale (~35mm tile) this reads as texture richness rather than individual anatomical detail.

The tradeoff is complexity vs. the simpler N-clamp fix. The N-clamp fix is 10 lines and gets you to "not broken." The petiole model is probably 150–200 lines of new geometry code and gets you to "botanically correct and beautiful."

✻ Brewed for 24s

❯ would petiole result in printable geometry with attachments on all possible foliage cluster surface points?  Note that these are all printable -- so we have no horizontal-bottom attachment points.

Good constraint — this changes the analysis significantly.

What "no horizontal-bottom attachment points" implies

If the cluster surface is fully FDM-compliant, then every point on its surface has a normal that points at or above the FDM printability floor. That means surface_normal.z >= sin(26°) ≈ 0.44 everywhere a leaf could attach. The surface normal never points straight down.

So yes — petioles would be printable everywhere

Here's why the constraint closes the loop:

The petiole itself: It starts at the attachment point, oriented along the surface normal. Since all surface normals are above the FDM floor, every petiole starts in a compliant direction. A 2–4mm petiole with gentle curvature can't curve enough in that distance to fall below the floor before it terminates. The petiole is always printable.

The leaf blade at the tip: With N = world_up, the blade's top face always points upward, independent of anything about the attachment point. Printable everywhere unconditionally.

What the petiole actually fixes

The FDM face-angle for the leaf blade isn't even the core issue, given the cluster constraint you just described — surface_normal.z >= 0.44 means the leaf face (with N = surface_normal) is already above the FDM floor at every attachment point.

The actual problem in the images is geometric penetration: the dip=45° tangent combined with the gravity-biased T0 sends the leaf tip back through the cluster surface. The tip ends up inside the sphere.

The petiole fixes this directly: by pushing the blade base 2–4mm away from the cluster surface before the blade starts, the blade can't intersect the cluster regardless of dip angle, because it's spatially separated from it.

What it looks like per-zone

- Upper attachment (surface_normal ≈ world_up): petiole points up, blade faces up, leaf sits flat on top of the cluster — natural.
- Side attachment (surface_normal ≈ horizontal): petiole extends horizontally, blade hangs 3mm off the side with face up — like a leaf on the side of a branch. Natural.
- Near-equatorial (surface_normal tilted): petiole angles outward-and-up, blade hangs from the tip facing up — drooping leaf. Natural.

No case breaks.

The one remaining question

At the equatorial band, the petiole is nearly horizontal and the blade is hanging from a horizontal arm. That's fine, but if you want the blade to actually droop (tip lower than base, like a real leaf under gravity), you need the tangent at the blade base to have a slight downward component. With a pure straight petiole, the blade base inherits the petiole's direction. A Bezier curve in the petiole — starting along surface_normal, curving toward a downward-tilted tip — would give you that droop naturally, and the curvature over 3mm is small enough to stay printable.

So: yes, fully printable everywhere, and it actually solves the penetration problem that the N-clamp alone doesn't fully address.

✻ Cooked for 1m 45s

❯ So you're seeing the petiole would come out and then head down and then attach to the leaf. If something comes out and heads down, then that will create an undercut. Right? So that won't be printable?

