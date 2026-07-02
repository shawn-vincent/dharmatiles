# Foliage Greenfield — Requirements Interview (2026-07-02)

**Status: requirements gathering — no design or code yet.**
Interviewer/scribe: Claude Fable 5. Answers recorded verbatim-ish as given.

## Why a greenfield

All three leaf placement strategies — meridian-arc rows, greedy lowest-first
accretion, and shoot-based sprigs — were implemented, rendered side-by-side on
the two-cluster test, and judged **ugly** by Shawn. Fixing the foliage cluster
geometry itself (structured sweep, rounded back end, short-edge spine fix,
commit e62475d) improved the substrate but the leafed result still reads
wrong. Verdict: iterating placement parameters is not converging; restart
from requirements.

What exists and is presumed reusable unless requirements say otherwise:
- Skeleton growth (space colonisation, FDM-aware) — solid, not in question.
- Foliage cluster substrate: swept rounded-cone solids at terminal branches.
- Leaf solid primitive: `build_leaf_surface` + `solidify_leaf` (watertight,
  oval-rooted).
- Three placers in `placement.py` / `placement_greedy.py` /
  `placement_shoots.py` — all disliked.

## Requirements Q&A

*(filled in as the interview proceeds)*

### Q1 — Visual style target
**Question:** At tabletop viewing distance (~50 cm, 35 mm tiles, ~40 mm
trees), what should the canopy read as?
**Answer:** Visible discrete leaves, big enough to FDM print. Slightly
cartoonish. Reference: **Animal Crossing (New Horizons) tree** — large
lobed leaves densely shingling blobby cluster masses.

### Q2 — What reads as ugly today
**Question:** Which offends most in the current results: the blobby cluster
silhouette, the leaf shapes themselves, the arrangement patterns, or the
seams/gaps between clusters?
**Answer:** Blobby silhouette is FINE (liked). Leaf shape is FINE. **The
arrangement is the failure**: bare spots over big chunks of the canopy,
heavy overlap where leaves do land. Complicated by multiple foliage
clusters (by design) creating inside corners. Overlap is acceptable
**as long as overlapping leaves sit at different heights from the parent
mesh** so they read as distinct leaves.

### Q3 — Print & finish constraints
**Question:** Hard constraints on printing and painting?
**Answer:** Supportless FDM, drybrush-friendly relief.

### Q4 — Greenfield scope
**Question:** How much gets rebuilt — just the leaf layer, the whole canopy
above the skeleton, or the whole tree?
**Answer:** Keep clusters and branch construction. **Rebuild ONLY leaf
placement & solidification.**

## Reference analysis — what the Animal Crossing tree actually does

Observations from the reference image, relevant to requirements:

- **Coverage is total.** No substrate visible anywhere; the canopy surface
  IS the leaf layer. Bare spots simply do not exist in the target look.
- **Leaves are large** relative to the cluster (each ~1/6–1/8 of a cluster's
  diameter) and simple-lobed; detail budget goes into few big leaves, not
  many small ones.
- **Arrangement is systematically shingled**: loose horizontal bands, upper
  leaves overlapping the ones below like roof tiles, tips pointing
  outward/down. It is *visibly regular* — and reads as charming, not
  mechanical, because the overlap is uniform and layered.
- **Every leaf reads distinct** via layering (each sits proud of its
  neighbours) and slight orientation variation — matching the Q2
  requirement that overlap is fine when heights differ.
- **Multi-cluster seams are unceremonious**: leafed ball meets leafed ball;
  the seam is just leaves meeting leaves. Full per-cluster coverage makes
  the inside-corner problem mostly disappear visually.

Notable implication: guaranteed-coverage row tiling (the meridian *family*)
is closer to the target than accretion scatter — the failures to fix are
coverage holes, undifferentiated overlap, and seam handling, not
regularity itself.

### Q5 — Leaf size & solid
**Question:** Bigger/chunkier leaves (AC proportions) or current?
**Answer:** **Current size and thickness** (4.5 × 3 × 0.24 mm). The
existing leaf solid is right; only placement changes.

### Q6 — Regularity of the arrangement
**Question:** Embrace AC-style visible shingle bands, or hide the order?
**Answer:** **Organic — no visible order.** Coverage must still be
complete. (Deliberate divergence from the AC reference's visible bands.)

### Q7 — Multi-cluster seams
**Question:** What happens at inside corners between clusters?
**Answer:** **Tile the union surface.** Treat the merged canopy as one
surface; leaves flow continuously across seams; no leaf buried inside a
neighbouring cluster.

### Q8 — Overlap layering mechanism
**Question:** Systematic shingle steps vs randomized per-leaf standoff?
**Answer:** **Prototype both, decide on render** (two-cluster test).

### Q9 — Undersides (supportless FDM can't hang leaves)
**Question:** Bare substrate, push to overhang limit, or relief texture?
**Answer:** **Explore relief stamps** — and/or rejigger the leaf geometry
for the underside zone so it still reads as a leaf but less
fancy/curved/raised (a flatter, print-safe leaf variant).

### Q10 — Direction field
**Question:** Which way do leaves point?
**Answer:** **Down-slope with variation** (±20–30° smooth variation; not
combed, not chaotic).

### Q11 — Overlap density
**Question:** Moderate shingle vs light touch?
**Answer:** **Configurable, moderate → light.** One knob sweeping ~50%
overlap down to ~15% (substrate glimpses acceptable at the light end
since the substrate is foliage-green).

### Q12 — Acceptance test
**Question:** What renders judge the result?
**Answer:** **Both** — the two-cluster A+B scene (seams, coverage) AND a
full skeleton-grown tree.

## Consolidated requirements

1. **Scope**: rebuild leaf *placement + solidification* only. Clusters,
   branches, skeleton, and the leaf solid primitive stay.
2. **Coverage**: complete — no bare patches on any surface above the FDM
   underside limit. This is the #1 failure of all prior attempts.
3. **Arrangement**: organic; no visible rows, rings, bands, or grids.
4. **Overlap**: configurable moderate (~50%) → light (~15%); overlapping
   leaves MUST sit at distinct heights from the substrate so each reads
   as its own leaf.
5. **Seams**: the canopy is tiled as ONE union surface across clusters;
   leaves flow through inside corners; no leaf buried in a neighbour.
6. **Direction**: down-slope with ±20–30° smooth (non-i.i.d.) variation.
7. **Undersides**: no discrete hanging leaves; explore (a) leaf-shaped
   relief stamped into the cluster surface and (b) a flat print-safe leaf
   variant, for the below-limit zone.
8. **Print/finish**: supportless FDM, drybrush-friendly relief.
9. **Leaf solid**: current geometry and size (4.5 × 3 × 0.24 mm) as-is
   for the main canopy.
10. **Acceptance**: eyeball pass on the two-cluster scene AND a full
    skeleton-grown tree, both prototyped with systematic vs random
    height layering for the overlap.
11. **Coexistence**: new placer lands as a fourth `leaf_placement`
    option; meridian/greedy/shoots stay side-by-side for now.
12. **Performance**: **< 5 s** leaf placement on a full tree (~30
    clusters). Hard constraints from the 2026-07-01 perf crisis remain:
    no per-leaf mesh scans; batch embree queries; cheap-reject before
    every build.
13. **Knobs**: module constants while iterating; promote to `Tree(...)`
    config after the look settles.

## Design sketch (proposed — not yet approved)

Working name: `placement_organic.py`, selector `leaf_placement="organic"`.

1. **Union placement surface.** Boolean-union the (noised) cluster solids
   via manifold3d (already a dependency). All placement queries — sampling,
   normals, seating, containment — run against this one mesh. Inside
   corners become creases of the union; "no leaf buried in a neighbour"
   is free because there are no neighbours.

2. **Coverage by maximal Poisson-disk.** Over-generate area-weighted
   candidates on the union surface; sweep with the exact-distance root
   grid (from the shoots work) accepting any candidate ≥ r from all
   accepted roots, to saturation.  Maximality ⇒ every surface point is
   within r of a root ⇒ **no bare patch wider than 2r, by construction**.
   The overlap knob IS r (≈ 0.45·W moderate → 0.8·W light).  Blue-noise
   = organic, no rows/rings.  A verification pass samples the surface
   (above the underside limit) and reports max-gap; a half-radius fill
   round closes any residual holes measurably.

3. **Direction field.** Down-slope per root, rotated by a smooth
   low-frequency angular field (sum of a few positional sines, ±25°) —
   coherent variation, not i.i.d. jitter, not combed.

4. **Overlap layering (the two prototypes).**
   (a) *Systematic:* sweep roots bottom-up; a leaf's standoff layer =
   count of already-accepted roots within overlap distance (capped ~3) ×
   step (~0.4–0.6 mm) — upper-over-lower shingling.
   (b) *Random:* hash-assigned layer ∈ {0,1,2} × step.
   Standoff translates the blade along the seat normal; the root oval
   stays plugged (existing rigid blade↔oval mechanics).  Seat each blade
   with the existing equal-depth oval tilt + belly-dip drop so the lowest
   layer grazes at `_PROTRUSION_MM`.

5. **Undersides** (normal below the printability floor): no discrete
   leaves.  Explore (a) leaf-silhouette relief stamped into the cluster
   surface (GrassCarpet-style displacement; needs finer tessellation of
   the underside band), and (b) a flat print-safe leaf variant hugging
   the surface.  (a) is print-safe by construction and the expected
   winner.

6. **Performance plan.** All embree work (candidate normals, seat rays,
   containment) batched across candidates in lockstep — nothing
   per-candidate inside the sweep except grid lookups.  Geometry built
   once per accepted leaf.  Target: the two-cluster scene well under 1 s,
   full tree < 5 s.

Phasing: A) union + placer + both layering modes → B) acceptance renders
(two-cluster + full tree, layering A/B side by side) → C) underside
exploration → D) knob promotion / cleanup decisions.
