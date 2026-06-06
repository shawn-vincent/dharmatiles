# Grass Requirements Review
*2026-06-06 — review of docs/grass/requirements.md v1*

Detailed pass over every requirement: edge cases, conflicts, ambiguities, open
questions.  Intended to drive revisions before design begins.

---

## Consistency check: grass-behavior.md vs requirements.md

*Added after grass-behavior.md was written.*

**Conflicts and gaps found:**

**1. Species not in requirements.**
grass-behavior.md has a dedicated "Seeds and species" section: seeds carry their
geometry, species are templates, multiple species can coexist.  requirements.md
has no mention of species.  REQ-CFG-1 ("single config dataclass") actively
conflicts with a multi-species design.  Requirements need a species model.

**2. "Flat" is the fundamental orientation, not just a terrain-following detail.**
grass-behavior.md opens with "blades lie almost flat — they are NOT upright" and
lists "blades should not stand upright" under what it should NOT do.  requirements.md
treats flatness only as a consequence of terrain following (REQ-TRN-2: flat on
flat terrain).  There is no top-level requirement stating that the grass type is
fundamentally horizontal.  This should be explicit — e.g. "blades grow in the
horizontal plane; vertical displacement is obstacle-driven only."

**3. Parallel-blade jitter gap.**
grass-behavior.md explicitly says: *"Adjacent blades running alongside each other
should lie flat — not jitter up and down."*  This is a specific negative requirement
with no counterpart in requirements.md.  It should be added (probably under
REQ-TRN or REQ-OBS).

**4. Flow field sampling timing.**
grass-behavior.md: "direction [is] baked in at creation time."  requirements.md
REQ-BLD-4: "blades lean/curve in the direction of the tile's flow field" — reads
as if the flow field is consulted during growth.  Reconcile: direction is sampled
from the flow field *at seed creation* and baked into the seed.  The growth
algorithm does not read the flow field.

**5. Missing negative requirements.**
grass-behavior.md "what it should NOT do" contains several things requirements.md
doesn't cover:
- No rows, grids, or radial patterns (aesthetic — hard to test, but worth stating)
- No tall stacks on flat terrain (covered partially by REQ-OBS-4, but not stated
  as a visual outcome)
- No staircasing on flat terrain (REQ-OBS-3 covers the mechanism, not the visible
  outcome)

**6. REQ-TRN-1 vs REQ-BLD-7 conflict (already noted above) confirmed here too.**
grass-behavior.md describes root embed as fundamental ("appear to grow from the
soil").  REQ-TRN-1 ("at or above terrain") contradicts it without the "above-ground
only" clarification.

**Overall verdict:** grass-behavior.md is more complete on intent.  requirements.md
needs: (a) species model, (b) flatness as a first-class requirement, (c) explicit
parallel-blade jitter requirement, (d) flow field timing clarification, (e)
REQ-TRN-1 scoped to above-ground portion.

---

## Cross-cutting note: seeds as species carriers

Before the per-requirement pass: a design point that changes how several
requirements should read.

Each seed is created with its geometry fully baked in — direction, length, curl,
width, cross-section, all sampled from a species template at planting time.  The
growth algorithm reads only from the seed; it never touches a global config again.
This means different groups (or even individual seeds) can represent different
plant species with completely different shapes, and the growth loop stays generic.

**Impact on requirements:** REQ-BLD-2 through REQ-BLD-5 describe properties as
coming from a "configurable range", which implies one global config.  They should
instead say the property is *set on the seed at creation time*, sampled from a
per-species template.  REQ-CFG-1 may need to be "one config dataclass per species,
plus a top-level config listing species and their relative densities."

---

## REQ-OUT-1 — watertight manifold meshes

**OK.** One concern: if blades stack (REQ-OBS-2) their meshes touch at the contact
surface.  Two meshes sharing a face are not independently manifold at that face.
For slicing purposes this is fine as long as each mesh is individually closed.
The requirement should clarify: *each blade mesh independently watertight*, not
the union.

---

## REQ-OUT-2 — no vertex outside XY footprint

**OK.** Achievable by clamping during mesh build.  Minor: the underground root
(REQ-BLD-7) inherits the seed's XY, which could be near the edge — the tube
cross-section extends ±hw from the spine in XY, so a spine right at the edge
could still overshoot.  Same clamp handles it but worth noting the root is a
special case (it's below terrain and invisible, so clamping is harmless).

---

## REQ-OUT-3 — no geometry outside tile bottom

**Revised by user from "below terrain_z" to "outside tile's bottom".**
This correctly accommodates the underground root (REQ-BLD-7), which must go
below terrain_z.  Confirm the intent: geometry may go below terrain_z, but not
below z = −base_h (the physical underside of the tile slab).

*Open question:* what is the tile bottom z-coordinate?  It is −base_h, which is
0 mm by default (flat terrain at z = 0, base below).  For non-flat terrain or
non-zero base_h the floor may not be z = 0.  The requirement should reference
the tile scene's actual bottom z, not a fixed value.

---

## REQ-OUT-4 — writes to support_z

**OK.** One subtlety: the layer must write the *top surface* of each blade, not
the spine z.  If the layer writes spine z, subsequent blades will be placed with
their bottom touching the spine rather than the top — they'll visually intersect.
Requirement should say "top surface of each placed blade" explicitly.

---

## REQ-BLD-1 — ribbon from seed, tapering tip

**OK.** "Growing outward from a seed point" is ambiguous: does the blade grow in
one direction only (current behaviour), or bidirectionally?  Real grass grows from
the base, so unidirectional makes sense, but the requirement doesn't say.
Recommend: *blades grow in a single direction from the seed.*

---

## REQ-BLD-2 — width range, sampled at seed time

**Conflict with REQ-BLD-6.**  width_min may be as small as 0.75 mm.  The taper
reduces blade width to 25% of full width at the tip → 0.19 mm.  FDM nozzle
diameter is 0.4 mm.  A 0.19 mm feature does not print.  Either:
- raise width_min so tip_width = 0.25 × width_min ≥ 0.4 mm → width_min ≥ 1.6 mm, or
- change the taper floor so the minimum printed width ≥ 0.4 mm regardless, or
- accept sub-nozzle tips (they'll merge with adjacent faces in the slicer and
  produce a rounded point — often acceptable).

*Open question:* is a sub-nozzle tip acceptable for visual quality at print scale?

---

## REQ-BLD-3 — length range, grows until target/obstacle/boundary

**Ambiguity:** "configurable (min/max range)" implies blades have a *minimum*
length.  What happens if a blade is blocked before reaching min length?
- Option A: discard it (don't add to the mesh).
- Option B: keep the stub (current behaviour).
- Option C: retry the seed at a different position.

Currently stubs are kept; their visual impact at < 0.5 mm is negligible, but they
add mesh complexity.  Recommend stating the policy explicitly.

Also: "target length" implies a per-blade target sampled from the range.  But the
current system uses max_segs (an integer step count) with a ±20% variation — not a
continuous length range.  For a rewrite with larger steps (see REQ-SCL concern
below), a continuous mm target is cleaner.

---

## REQ-BLD-4 — lean/curve from flow field

**Scope question:** does "lean" mean the blade lies in the direction of flow (i.e.,
the growth direction IS the flow direction)?  Or does it also mean the blade tilts
away from vertical like wind-blown grass?  On flat terrain these are the same.  On
sloped terrain they diverge — the current implementation doesn't handle slope, and
the requirements don't address it either.  Recommend scoping to flat terrain
explicitly, or calling out slope as a known out-of-scope item.

---

## REQ-BLD-5 — lateral curl

**OK with one clarification:** "driven by the flow field" — the flow field drives
the *direction* of curl (which way the blade curves), not the magnitude.  Per-blade
variation controls magnitude.  If this is the intent, say so to avoid confusing
"curl driven by flow field" with "curl magnitude = flow curvature".

---

## REQ-BLD-6 — FDM printability

**Conflict with REQ-BLD-2 tip width** (noted above).

Additional: "no overhangs steeper than ~45°" applies to the blade *cross-section*
shape, not the blade path.  Flat horizontal ribbon = no overhang concern.  Arched
leaf cross-section: the arch top is at most blade_width/2 horizontal overhang over
the keel, which at 2 mm width = 1 mm overhang at ~thickness height — fine for most
slicers.  This requirement is effectively already satisfied by the current
cross-section designs; worth noting rather than leaving it as an open constraint.

---

## REQ-BLD-7 — root embed below terrain

**Conflict with REQ-TRN-1.**  REQ-TRN-1 says "each point along a blade spine sits
at or above terrain_z".  REQ-BLD-7 says the blade embeds below terrain_z.  These
contradict.  Resolution: REQ-TRN-1 should apply only to the *above-ground* portion
of the blade (from the terrain surface up), and REQ-BLD-7 governs the underground
anchor separately.

---

## REQ-TRN-2 — flat on flat terrain

**Implementation concern:** "near-zero height" is vague.  Recommend specifying in
mm: the spine must be ≤ CLEARANCE above terrain_z, where CLEARANCE is a small
configurable value (currently 0.01 mm).  This is important because the clearance
determines whether coplanar-face z-fighting occurs with the terrain mesh.

---

## REQ-TRN-3 — rises smoothly over obstacle

**"Smoothly" is undefined.**  Two interpretations:
1. The path is smooth (no sharp Z kinks) — this is a mesh quality property.
2. The rise is gradual (limited slope) — this is the rise_cap property (REQ-TRN-5).

Both are probably intended.  Recommend splitting: REQ-TRN-3 covers gradual rise
(governed by rise_cap), REQ-TRN-5 covers path smoothness (governed by a
post-growth smoothing step or by the step size being large enough that kinks are
below print resolution).

---

## REQ-TRN-4 — drops back after obstacle

**Correctness concern:** "returns to terrain level" — but if the blade's own
previous stamps are in the cells beyond the stone, it might see its own elevated
trail and stay high.  This is exactly the self-staircase bug.  The requirement
implies a specific implementation constraint: the blade must not count its own
prior stamps when evaluating the floor on the far side of an obstacle.

This is the hardest single requirement to implement correctly (it drove the
own_stamps mechanism and the jitter issue in the old code).  Worth flagging
explicitly in the requirements as a known implementation hazard.

---

## REQ-TRN-5 — rise limit

**Units ambiguity:** "configurable rise limit per unit length" — is this mm/mm
(slope), or mm per growth step?  These are equivalent only if step size is fixed.
In the current implementation rise_cap is mm per step, and step = cell_w (variable
by resolution).  For a rewrite with a fixed mm step size, "per step" and
"per mm" are equivalent — but should be stated clearly.

---

## REQ-OBS-2 and REQ-OBS-3 — tension

**This is the core design challenge.**  REQ-OBS-2 says blades must rise over other
blades.  REQ-OBS-3 says a blade must not climb its own body.  In a grid-based
system where stamps are written by index, a blade's own trailing stamps occupy
the same cells as the path the blade is about to re-enter (if it doubles back) or
adjacent cells (if it runs parallel to itself).

The naive resolution (own_stamps dict) works but produces Z jitter on parallel
blades because the leading-edge transverse stamp alternately covers and misses the
adjacent blade's target cell as they advance diagonally together.

*Open question for design:* is there a representation of "what I already placed"
that lets the growth loop cleanly distinguish self-trail from external obstacle,
without grid aliasing artefacts?

---

## REQ-OBS-4 — stacking depth limit

**OK.** But note: the limit applies both at *seed time* (reject seeds on top of
a pile) and during *growth* (stop growing if the pile below would be too tall).
The requirement says both, but they need to use the same threshold value, which
should be stated.

---

## REQ-REG-3 — edge fill

**Direction ambiguity:** "blades growing inward from the edge" — should edge-fill
blades be constrained to grow away from the edge, or can they grow along it or
even outward?  Growing outward would immediately hit the boundary and produce a
zero-length blade.  Growing along the edge is fine visually.  Growing inward
fills the visual gap at the boundary.

Current implementation uses the flow direction for edge-fill blades, which may
send them along (or even toward) the edge.  Recommend: edge-fill blades should
use the inward normal of the boundary as their base direction, biased toward the
flow field, so they definitely grow into the tile.

---

## REQ-DEN-1 — groups/clumps

**Species question:** should all blades in a group be the same species?  Or can a
group mix species?  Biologically, a clump is typically one species.  If species is
determined at group creation, the requirement should say so.

---

## REQ-PRF-1 — < 5 s for 1×1 tile

**Tight but achievable.**  Key variables: blade count, step count, and cost per
step.  At default density (groups_per_square=50, group_min/max=20–30), up to ~1500
blades × ~18–21 rounds = ~27,000 steps.  If each step is a handful of array
lookups, 5 s is feasible.  If each step involves large footprint scans or Python
loops over all blades, it will be tight.

*Implication for step size:* using a larger step (0.4–0.8 mm vs the current
0.14 mm) means fewer rounds for the same blade length — directly reduces step
count.  At 0.5 mm step, a 2.5 mm blade needs only 5 steps, not 18.  This
deserves a performance-vs-resolution trade-off analysis in the design doc.

---

## REQ-MSH-2 — no coplanar overlapping faces

**Hardest mesh quality requirement.**  Two blades at the same Z with overlapping
XY produce z-fighting that is visible in mesh viewers (and potentially causes
slicer issues).  This requires either:
- guaranteed Z separation between any two blades that share an XY footprint, or
- a post-placement check and correction step.

Guaranteed separation requires that REQ-OBS-2 (rise over other blades) is always
triggered before coplanar overlap occurs.  This in turn requires that the stamp
height seen by blade B from blade A is always > clearance threshold.  If blade A's
stamp is exactly at clearance (e.g., because of the terrain-level threshold fix
we explored), blade B may land coplanar with A.  Worth flagging as a
correctness-critical interaction between REQ-OBS-2, REQ-OBS-3, and REQ-MSH-2.

---

## REQ-CFG-1 — single config dataclass

**Tension with species.**  If seeds carry species-specific parameters, the config
needs to describe multiple species (each a template for seed creation) plus a
top-level density/mix ratio.  A single flat dataclass won't cover this cleanly.
Suggest: `GrassConfig` holds global knobs (density, stack limit, clearance) plus a
`species: list[SpeciesConfig]` field where each `SpeciesConfig` holds width,
length, curl, cross-section, etc.  Seed creation samples from one species.

---

## Summary of open questions

| # | Question |
|---|---|
| Q1 | Is a sub-nozzle tip (< 0.4 mm) acceptable at print scale, or must we raise width_min? |
| Q2 | What is the tile bottom z? Is it always 0, or −base_h? |
| Q3 | What happens to blades blocked before reaching min length — discard or keep? |
| Q4 | Is slope handling in scope for the rewrite, or flat terrain only? |
| Q5 | What is the intended step size — cell_w (grid-tied) or a fixed mm value? |
| Q6 | For REQ-OBS-3, what representation avoids the self-trail / jitter problem? |
| Q7 | Should edge-fill blades be constrained to grow inward? |
| Q8 | Is species determined per-group, per-seed, or randomly? |
| Q9 | Is a single flat GrassConfig sufficient, or do we need a species list? |
