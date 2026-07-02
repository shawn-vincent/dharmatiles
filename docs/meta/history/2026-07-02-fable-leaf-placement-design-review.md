# Leaf Placement Design Review — Greedy vs. Meridian, and What's Next (2026-07-02)

**Source: Claude Fable 5** (Anthropic's Mythos-class model, first Claude 5 release).
This entry records Fable's design assessment of the leaf placement problem,
given the full history (meridian spec, greedy experiment writeup, FDM design
space, 2026-07-01 perf crisis) and the current `greedy-leaf-placement` branch
state (post-7f9e3ce: rigid leaf+oval in the leaf's own frame, equal-depth oval
seat, down-slope growth, printability skew).

The question posed: *for lots of leaves covering irregular foliage clusters,
positioned like real leaves (ignoring tilts/angles for now), is there a
better / faster / more realistic approach than what greedy or meridian are
trying to do?*

---

## Verdict

**The greedy z-sorted sweep is the right algorithm family — stop looking for a
third placer.** Greedy accretion over a pre-generated candidate set is
Poisson-disk sampling with a shingling order, which is essentially what
production foliage systems do at the leaf-card level. Meridian is a
parameterization-based tiler, and parameterization is the wrong tool for
irregular, unioned, noised blobs — every meridian pain point (star-convexity
assumption, ring artifacts, reactive cross-cluster culls, 66% wasted builds)
comes from forcing a (φ, z) chart onto a surface that doesn't want one.

The remaining wins are **above and below the placement layer**, not in it:

1. **Realism gap**: greedy places leaves as independent, identical,
   evenly-spaced individuals. Real canopies are none of those things. What
   makes a canopy read as real is *structure between the leaf and the blob*:
   coherent groups along shoots, density varying with light exposure, size
   variation, clumps and gaps. Blue-noise uniformity is the fingerprint of
   procedural scatter.
2. **Speed gap**: a full `build_leaf_surface` + `solidify_leaf` per accepted
   leaf, and thousands of tiny per-candidate embree calls. Both removable.

---

## Realism ideas, ranked by payoff

### 1. Place shoots, not leaves (biggest single win)

Make the placement unit a short *shoot*: a curve 1.5–3 leaf-lengths long
running down-slope on the surface, carrying 3–7 leaves in an alternate
left/right arrangement with slight splay and diminishing size toward the shoot
tip. The greedy sweep barely changes — a candidate is a shoot spine instead of
a point; the reject ladder tests its footprint; on accept the whole group is
emitted. Same total leaf count, near-identical cost, but local orientation
coherence and natural clumping fall out for free, and the gaps between shoots
read as intentional structure rather than sampling noise. This is how leaves
are actually positioned in nature, so it is the strongest possible
"positioned like real leaves" move.

### 2. Density taper + low-frequency density noise

Two cheap modulations of candidate acceptance: scale density by `normal.z`
(leaf-dense on the lit top, sparse below — the FDM doc's F3 option, never
implemented), and modulate `min_root_gap` by a smooth low-frequency noise over
the surface so the canopy has denser and thinner patches. Uniform density is
the thing that screams "procedural"; a factor-of-2 density swell across a
clump reads as biology.

### 3. Size variation

±20–25% length/width per leaf (hash-driven), biased smaller near the clump
apex. Trivial; disproportionate return — identical-size leaves are the
second-loudest procedural tell after uniform spacing.

### 4. The GrassCarpet trick, applied to foliage

The project already solved this exact aesthetic problem for grass: a dense
embossed 2D carpet *underneath* sparse true-3D blades. Do the same here —
emboss a dense leaf-stamp texture into the clump surface itself. Pure
embossing was abandoned ("looks like scales"), but as an *underlayer* it never
reads on its own; it kills the bald green gaps between 3D leaves. The 3D leaf
budget can then drop 30–50% while perceived coverage goes up. Simultaneously a
realism and a speed idea, consistent with the project's proven design
language (GrassCarpet + Grass).

### 5. Coherent (not i.i.d.) direction variation

Jitter is currently stripped to pure down-slope (777c8fa) — fine while
iterating, but pure down-slope everywhere will eventually read as "combed".
When variation returns, do NOT use per-leaf random jitter; use a smooth
low-frequency angular field over the surface (curl-noise-style, ±25° around
down-slope) so *neighboring leaves deviate together*. Neighbor coherence is
what distinguishes wind-and-growth from dice rolls. (Falls out automatically
from shoots — leaves on a shoot share its direction.)

### 6. Phyllotaxis candidate ordering (skip if shoots land)

Still an open TODO from the greedy experiment doc. With root-gap
dart-throwing already producing blue noise, a golden-angle spiral is only
worth it *instead of* shoots, not alongside them — shoots dominate it.

---

## Speed: two structural moves

### 1. Leaf instancing (the big one)

As of commit 7f9e3ce the root oval is built in the leaf's own frame — meaning
**the entire leaf solid is now rigid**. There is no per-placement conforming
geometry left. So stop building geometry per leaf: precompute a small library
of K leaf solids (8–16 seed variants × a few size scales) in canonical frame,
once, and place each accepted leaf with a single rigid transform of the
canonical vertices. The tip/belly containment probes become transforms of two
canonical points, so they run *before* any geometry exists. Per-leaf cost
collapses from Bezier surface evaluation + solidify (~ms) to a matrix multiply
(~µs). The 6.3 s tile placement likely drops under 1 s. Composes perfectly
with shoots (prefab a whole shoot assembly as one instance).

Related waste this removes: the skew-cull and containment-cull currently run
*after* `build_leaf_surface`, so a culled candidate has already paid for a
build.

### 2. Batch the embree work out of the sweep

Only the root-gap grid depends on previously accepted leaves.
`_seat_oval_tilt` (2 rays × 3 iterations per candidate) and both `contains`
guards depend only on the static meshes — so they need not run inside the
sequential sweep at all. Run the seat solve for **all** candidates in 3
batched ray casts (all candidates iterate in lockstep as N×2 ray arrays),
batch the containment probes the same way, annotate each candidate
viable/dead, *then* sweep with nothing but hash lookups. Thousands of tiny
embree calls become ~10 big vectorized ones. Same lesson as the 2026-07-01
perf crisis, one level up: it's not just *which* geometric query, it's
*per-candidate anything*.

---

## Explicitly rejected

- **Per-leaf physical simulation / space-colonization of leaves** — massive
  cost; at 4.5 mm leaves on a 35 mm tile only the statistical texture
  (grouping, density, size) reads, not structural realism.
- **Optimization-based packing** (annealing / global coverage optimization) —
  greedy's stranded-gap risk stops mattering once the carpet underlayer
  exists.
- **CSG union of leaves into the clump** — already ruled out for cost; still
  correct.
- **Reviving meridian for anything** — its one genuine advantage (guaranteed
  coverage) is better bought with the carpet underlayer.

---

## One-sentence summary

Keep the greedy sweep exactly as the chassis; make the placement unit a
**prefab shoot instance** (rigid, precomputed, 3–7 leaves) instead of a single
built-on-demand leaf, and add a density taper plus an embossed leaf carpet
under everything — faster than the current path *and* it attacks realism at
the level where human perception actually judges it (grouping, density
variation, gap texture) rather than at individual leaf placement, which is
already right.
