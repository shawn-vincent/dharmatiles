# Design: Branchlet Growth Algorithm

Status: design – 2026-06-20
Parent design: [Tree Leaf Support Volumes](tree-leaf-branchlets.md)

## What This Algorithm Does

A branchlet is a short tapered stem that connects a point on a branch surface to
a leaf. The leaf sits at the tip of the stem; the stem's job is to hold it there
in a way that can be FDM-printed without supports.

The algorithm takes an attachment point on a branch, grows a stem outward from
it for a fixed length, and derives where the leaf ends up. The leaf is an
unpositioned template — a shape at the origin. The algorithm places and orients
it in world space at the tip of the grown stem.

The central problem is direction: which way does the stem exit the branch? The
stem has to be printable all the way along, which means it can never point
downward past the FDM floor angle (typically 45°). Depending on which way the
branch surface faces at the attachment point, this is either trivial or requires
rotation.

## One Algorithm, Four Illustrated Behaviours

There is one algorithm. It contains one conditional — checking whether the
surface normal already points safely upward. The rest is geometry that follows
from whatever values you feed in. The four cases below are not code branches;
they describe what the same formula produces when the attachment surface faces in
four different directions.

## Key Distinction: Surface Face vs. Surface Normal

These are two independent properties and must not be confused.

**Whether a face is printable** depends on the face's own slope — specifically,
whether the face is steep enough that the plastic has something below it to land
on. Both the upper and the lower side of a 45° slope are equally printable as
surfaces.

**The surface normal** is the direction that points away from the solid interior
at the attachment point. The upper side of a 45° slope has its normal pointing
upward at 45°. The underside of that same slope has its normal pointing downward
at -45°. Same geometry; opposite normals.

What the algorithm cares about is the normal direction, not face printability.
Exiting the branch in the direction the normal points may or may not produce a
printable stem — it depends entirely on whether that normal points upward enough.

## The Algorithm

### Inputs

| Name | What it is |
|------|-----------|
| Branch mesh | The branch, trunk, or foliage clump the stem grows from |
| Leaf template | An unpositioned leaf shape, base at the world origin |
| Attachment point | A specific point on the branch surface |
| Surface normal | The outward normal of the branch mesh at the attachment point |
| Stem length | How long the branchlet grows (e.g. 8 mm) |
| Preferred leaf direction | Which way the leaf should face at its tip; default is straight up |
| FDM floor angle | Minimum printable elevation above horizontal; default 45° |
| Root diameter | Branchlet thickness at the branch end (e.g. 1.2 mm) |
| Tip diameter | Branchlet thickness at the leaf end (e.g. 0.6 mm) |
| Embed depth | How far the root peg penetrates the branch for slicer fusion (e.g. 0.4 mm) |
| Min feature width | Smallest printable feature (e.g. 0.8 mm) |
| Leaf yaw | Spin of the leaf around the stem tip axis (random variation) |

### Outputs

| Name | What it is |
|------|-----------|
| Stem mesh | Watertight tapered tube from attachment point to leaf anchor |
| Leaf mesh | Leaf template translated and rotated into world space |

The leaf anchor position and the leaf's world-space orientation are both derived
by the algorithm — they are not inputs.

---

### Step 1 — Find the exit direction

The exit direction is the direction the stem grows away from the branch surface.
It must point upward enough to be printable — elevation at or above the FDM
floor angle.

Check the surface normal's elevation angle above horizontal:

```
normal_elevation = asin(surface_normal.z)
```

**If the surface normal is already above the floor angle:**

Use the surface normal as the exit direction. The stem exits in the direction
the surface naturally faces outward. No adjustment needed.

**If the surface normal is below the floor angle** (horizontal or downward-facing
surface):

Take the horizontal component of the surface normal — the direction it points
when you ignore its vertical tilt. Then tilt that horizontal direction upward
until it just reaches the floor angle. That tilted direction becomes the exit
direction.

```
horizontal_part = normalize(surface_normal.xy)
    # if this is near-zero (nearly straight up or down),
    # fall back to the horizontal component of the preferred leaf direction

exit_direction = horizontal_part * cos(floor_angle)
              + straight_up      * sin(floor_angle)
```

This is the only conditional in the algorithm. After this step, the exit
direction is guaranteed to be printable regardless of what the surface normal was.

Note: when the surface normal points downward, the exit direction will point
away from the normal — into the space above the surface rather than below it.
The stem diverges from what "away from the branch" would mean in a naive sense.
This is intentional.

---

### Step 2 — Find where the stem tip ends up

The stem grows from the attachment point in a direction that blends between the
exit direction and the preferred leaf direction. This blend determines roughly
where the tip lands:

```
growth_direction = normalize(exit_direction + preferred_leaf_direction)
tip_point = attachment_point + stem_length * growth_direction
```

The tip point is the leaf anchor — where the base of the leaf will be placed.

---

### Step 3 — Build the curve

The stem follows a smooth cubic Bezier curve from the attachment point to the
tip. A cubic Bezier has four control points. Two of them are the endpoints;
the other two are handles that pull the curve in the exit and arrival directions:

```
exit_handle_length   = 0.4 * stem_length   (see per-case notes below)
arrival_handle_length = 0.4 * stem_length

control_point_A = attachment_point + exit_handle_length * exit_direction
control_point_B = tip_point - arrival_handle_length * preferred_leaf_direction
```

The curve goes: attachment point → pulled toward exit direction → pulled toward
preferred leaf direction → tip point.

The exit handle length is the primary tuning parameter. A longer exit handle
holds the exit direction for more of the stem's length before the curve begins
bending toward the leaf. When the exit direction diverges sharply from a straight
line to the leaf — as it does when the surface faces downward or sideways — a
longer exit handle prevents the curve from dipping back below the floor angle
in the middle.

---

### Step 4 — Validate the curve stays above the floor angle

Sample the curve at 24 evenly spaced points. At each point, compute the tangent
direction and check its elevation:

```
for each sample point:
    elevation = asin(tangent_at_point.z)
    if elevation < floor_angle:
        increase exit handle length and retry
```

If validation still fails after several retries, reject this attachment point.

---

### Step 5 — Place the leaf

The leaf template sits at the world origin with its base at the origin and its
axis along the local +Z. Transform it to world space:

```
translate the leaf base to the tip point
rotate so the leaf axis aligns with the preferred leaf direction
apply the yaw spin around that axis
```

The leaf is now positioned and oriented in world space at the end of the grown
stem.

---

### Step 6 — Build the tube

Extrude a tapered circular cross-section along the validated curve:

- At the attachment end: radius = root diameter / 2
- At the tip end: radius = tip diameter / 2
- Taper linearly by distance along the arc
- Add a short root peg pointing in the direction opposite to the exit direction,
  penetrating the branch mesh by the embed depth for slicer fusion

---

### Step 7 — Final checks

- Every exterior tube face points upward enough: face normal z ≥ −cos(floor_angle)
- Every cross-section is at least min feature width
- No tube self-intersection
- Root peg actually penetrates the branch mesh by at least the embed depth
- Leaf base overlaps the stem tip by at least the embed depth

---

## The Four Cases Illustrated

The following diagrams show a side view (looking along the Y axis) of what the
algorithm produces when the attachment surface faces in four directions. The
branch mesh is shown as a filled region. The surface normal arrow, exit
direction, and stem curve are derived by the same formula in all four cases.

---

### Case 1 — Surface faces straight up

The attachment point is on top of a branch. The surface normal points straight
up. The surface normal is already above the floor angle, so the exit direction
equals the surface normal: straight up.

The preferred leaf direction is also straight up, so the growth direction is
straight up, and the tip lands directly above the attachment point. The curve is
a straight vertical line (or a gentle arc if a non-vertical leaf direction is
requested).

The root peg points straight down into the branch.

```
         leaf
          |
          |     ← stem rises straight up
          |
    attachment
  ══════════════  (branch top surface, normal ↑)
  ▓▓▓▓▓▓▓▓▓▓▓▓
```

Exit handle length: 0.4 × stem length. This case never fails elevation
validation.

---

### Case 2 — Surface faces upward at an angle

The attachment point is on the upper face of a sloped branch — angled, but still
facing generally upward. The surface normal tilts upward and to one side. The
surface normal is above the floor angle, so the exit direction equals the surface
normal: up and outward along the slope.

The curve exits along the slope normal, then bends toward the preferred leaf
direction (straight up). The tip lands above and somewhat to the side.

The root peg points inward along the slope — downward and into the branch body.

```
              leaf
             /
            /
           /   ← stem curves from slope-normal toward vertical
          /
    attach
        ╱══════  (sloped surface, normal ↑ and to the right)
      ╱▓▓▓▓▓▓▓
    ╱▓▓▓▓▓▓▓▓▓
```

Exit handle length: 0.4–0.5 × stem length. Both the exit direction and the
arrival direction are above the floor angle, so the entire arc is guaranteed to
stay above it. This case very rarely fails.

---

### Case 3 — Surface faces sideways

The attachment point is on the side of a branch. The surface normal is
horizontal. Exiting horizontally would produce a tube whose bottom face is flat —
unprintable.

The algorithm takes the horizontal part of the surface normal (which is just
the outward horizontal direction from the wall) and tilts it upward to exactly
the floor angle. The stem exits diagonally: outward and upward at 45°.

The curve starts at this diagonal, then bends upward toward the preferred leaf
direction. The tip lands above and a little to the side.

The root peg points horizontally into the wall.

```
         leaf
         /
        /
       /      ← stem curves from diagonal exit toward vertical
      /
     / ← exit at 45° (floor angle), outward + up
  attach
  ║
  ║  (vertical branch wall, normal →)
  ║
```

Exit handle length: 0.6 × stem length. The stem starts at exactly the floor
angle limit; a longer exit handle is needed to hold that direction long enough
for the curve to gain elevation before bending.

---

### Case 4 — Surface faces downward

The attachment point is on the underside of a branch. The surface normal points
downward. Exiting downward would immediately produce an unsupported descending
tube with no printable ancestry from the bed.

The algorithm takes the horizontal component of the downward normal (or falls
back to the horizontal part of the preferred leaf direction if the normal is
straight down), and tilts it upward to the floor angle. The exit direction
points outward and upward at 45°, sweeping away from the underside in an arc.

The curve exits at the floor angle, then bends upward toward the preferred leaf
direction. The tip lands above and to the side.

The root peg points upward into the branch — because the solid is above the
attachment point, not below it.

```
  ▓▓▓▓▓▓▓▓▓▓▓▓
  ══════════════  (branch underside, normal ↓)
     attach
          \
           \    ← exit at 45° (floor angle), outward + up
            \
             \  ← stem curves from diagonal exit toward vertical
              \
              leaf
```

Exit handle length: 0.6–0.8 × stem length. The exit direction diverges most
sharply from a straight line to the leaf in this case, so the longest exit
handle is needed to prevent mid-curve dip.

**Note:** Cases 3 and 4 produce visually similar stems — both exit at the floor
angle and curve upward. The differences are where the attachment is on the
branch, and which direction the embed peg goes. In Case 3 the peg goes sideways
into the wall; in Case 4 the peg goes upward through the underside.

---

## Why the Handle Length Varies

The exit handle controls how long the curve "remembers" the exit direction before
bending toward the leaf. The further the exit direction is from a straight line
between the attachment point and the expected tip, the longer the handle needs to
be to prevent the Bezier from immediately cutting the corner and dipping below
the floor angle.

A formula that captures this:

```
deviation = max(0, floor_angle − normal_elevation)
exit_handle_length = stem_length * (0.4 + 0.4 * deviation / 90°)
```

This produces 0.4 × stem length when the surface normal is already above the
floor (Cases 1 and 2), and scales up toward 0.8 × stem length as the normal
approaches straight down (the extreme of Case 4). No case numbers needed — one
formula, continuous behaviour.

---

## Embed Peg Direction

The peg always points in the direction opposite to the exit direction: it embeds
into the branch along `−exit_direction`. This follows from a single rule regardless
of surface orientation:

- Case 1 (exit up): peg points down. The branch is below; the peg descends into it.
- Case 2 (exit along slope normal): peg points inward along the slope. Correct.
- Case 3 (exit outward+up): peg points inward+down into the wall. Correct.
- Case 4 (exit outward+up): peg points inward+down... which for an underside means
  pointing back upward through the surface into the branch body. Also correct.

No special case needed.

---

## Validity Checks

A completed branchlet passes when:

1. Exit direction elevation ≥ floor angle — starting tangent is printable
2. All 24 sampled tangent elevations ≥ floor angle — entire curve is printable
3. All exterior tube face normals satisfy `face_normal.z ≥ −cos(floor_angle)`
4. All cross-section widths ≥ minimum feature width
5. Root peg penetrates the branch mesh by at least the embed depth
6. Leaf base overlaps the stem tip by at least the embed depth
7. No tube self-intersection

---

## Open Questions

- **Handle length formula**: the linear formula above is a first approximation.
  The actual required handle may scale nonlinearly with deviation — worth
  measuring empirically against a set of test attachment normals.

- **Monotone elevation option**: should the algorithm enforce that the stem's
  elevation never decreases along the curve? This eliminates mid-arc dips
  entirely but may produce longer or more circuitous stems in Case 4.

- **Minimum viable stem length**: below some threshold, the arc radius required
  for Cases 3 and 4 drops below what can be printed at the root diameter.
  Approximately 3–4 mm for a 1.2 mm root. Should be checked before building
  the curve.

- **Case 4 fallback**: when an underside attachment fails after all retries,
  should the algorithm automatically search for a nearby attachment point that
  has a less extreme surface normal? This would reduce stem curvature at the
  cost of shifting the visual attachment location.
