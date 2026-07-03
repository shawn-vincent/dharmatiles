# Leaf Placement — The Complete History (2026-07-03)

**Written the day the `greedy-leaf-placement` branch merged to main
(commit 951bf41) and the organic union-surface placer became the only leaf
generator.**  This entry records the whole arc: every approach tried, what
went wrong with each, the iteration that finally converged, why the winning
approach worked — and, most importantly, **what was done differently in the
final round that made it succeed where three previous placers failed**.
That last part is the reusable payload: the same development approach is the
plan for reworking grass.

Related entries: `2026-06-24-foliage-cluster-baldness`,
`2026-06-24-systematic-algorithm-development`,
`2026-06-25-floating-leaf-check-session`,
`2026-06-27-leaf-placement-code-review`,
`2026-06-28-leaf-placement-phi-distribution`,
`2026-06-29-leaf-jitter-and-placement-fixes`,
`2026-07-01-leaf-deconfliction-and-performance-crisis`,
`2026-07-02-fable-leaf-placement-design-review`,
`2026-07-02-foliage-greenfield-requirements`.
Current-state specs: `docs/design/leaf-placement.md`,
`docs/design/leaf-solidification.md`.

---

## The problem

Cover the foliage clusters of a procedurally grown tree with a few thousand
discrete, watertight, supportless-FDM-printable leaf solids, at 4.5 mm leaf
scale on a 35 mm tile, such that the canopy reads as charming and organic at
tabletop distance.  It took **five distinct approaches over roughly five
weeks** before one was accepted.

## Timeline of approaches

### Era 0 — pre-placement experiments (June)

- **Embossed surface relief**: leaf shapes as vertex displacement on the
  cluster itself.  Fast and print-safe, but "looks like scales".  (The idea
  survives as the candidate treatment for the bare underside zone.)
- **Branchlets / petioles**: tiny stems growing off the cluster with a leaf
  at the tip.  24+ commits; abandoned because any stem that bends downward
  is an FDM undercut.
- **First arc-parameterized placement**: rows/columns indexed by surface arc.
  Wrong outward direction at the dome top (blade-on-edge artifacts), brittle
  at the pole.

### Era 1 — Z-slice rows + apex cap

Rows at fixed dZ steps, leaves on each cross-section perimeter.  Correct on
cylinders, wrong on domes: fixed dZ ≠ fixed surface arc, so the near-apex
zone went bald.  An "apex cap" special case was **patched six times and
never fully solved it** — the canonical sign (recognised only later) that
the algorithm's shape was wrong and parameters couldn't save it.

### Era 2 — meridian-arc rows (placement.py, deleted 73e24f1)

A genuinely better tiler: rows spaced by *surface arc length* measured along
meridian curves, per-leaf normals interpolated from meridian tangents,
pinned top/bottom anchor rows.  Mathematically sound on any single
star-convex closed mesh, and it eliminated the apex cap.  With it came a
long tail of supporting engineering: analytic contact angle with belly-dip
seat, jitter wiring, solidification raycast fixes (tip-pole miss, cone
framework), multi-parent placement, imbrication via a (φ,s) layer map,
cross-cluster leaf culling, and finally the **2026-07-01 performance
crisis** (tree tile ~222 s → 28 s via embreex, removing per-leaf
deconfliction/fix_normals, batching).

What stayed wrong: the (φ,z) parameterization was forced onto irregular,
unioned, noised blobs.  Star-convexity assumptions, ring artifacts, reactive
cross-cluster culls, ~66 % wasted builds — every pain point traced back to
imposing a chart on a surface that didn't want one.  And on render it read
as rows.

### Era 3 — greedy lowest-first accretion (placement_greedy.py, deleted)

The branch begins here (phases 0–4, 2026-07-01): pre-generate candidates,
sort by world z, sweep upward once through a cheap-reject ladder, build
accepted leaves.  3× faster than meridian, and it discarded the
parameterization.  Then ~10 commits of hard iteration, with a notable
mid-course correction: the placer had re-accreted contact-angle solves,
shingle layers, clearance searches — and produced bad geometry — so Shawn
had it **stripped to basics** (81b6959) and rebuilt on direct construction:

- the root oval built **in the leaf's own frame**, rigid blade↔oval
  (7f9e3ce) — fixed the splayed stitch fans, and incidentally made the whole
  leaf solid rigid;
- the **equal-depth oval seat** (95d43d8): pitch the oval about its own
  center until both ends sit equally deep, asking the mesh only "how deep is
  this point?", never "where should this point be?" — placed count 29 → 42,
  coverage 17 % → 68 % on the test pair;
- the **printability skew** (13b9a39): slide the blade in-plane until its
  tip clears the oval tip in z, so tip walls never overhang;
- the **graze translation** (c097837): measure the real standoff of every
  blade vertex and translate so the closest vertex sits exactly at the
  standoff constant.

All of this machinery survives today in `placement_leaf.py`.  But the look
was still wrong: coverage holes, pod-like reads, blue-noise uniformity that
"screams procedural".

### Era 4 — the Fable design review (2ad81aa)

Asked "is there a better approach?", the review's verdict: greedy accretion
is the right *family*; the wins are above and below the placement layer
(shoots as the placement unit, density taper, size variation, an embossed
carpet underlayer, instancing, batching).  It explicitly warned against
per-leaf realism simulation and against reviving meridian.

### Era 5 — shoots (placement_shoots.py, deleted)

Sprigs, not leaves: each accepted candidate marches a spine up-slope
carrying 3–7 leaves in a herringbone.  Contributed two keepers — the
**adaptive belly-dip seat** (blade grazes `_PROTRUSION_MM` off the real
clump) and the **exact-distance root grid**, whose need was diagnosed from a
z-band outcome trace showing 90 % of top-band candidates dying root-blocked
against a cell-set grid.  Also fixed alongside: the cluster substrate itself
(e62475d — structured sweep replacing the bent icosphere).  Still judged
ugly on render.

### The pivot — requirements interview (7c5821b)

All three placers were rendered **side by side** on the two-cluster test and
judged unacceptable.  The recorded verdict: *"iterating placement parameters
is not converging; restart from requirements."*  Instead of a fourth
algorithm sketch, a structured interview
(`2026-07-02-foliage-greenfield-requirements.md`):

- **What's liked was separated from what fails.**  Cluster silhouette: fine.
  Leaf shape: fine.  **The arrangement is the failure** — bare spots,
  undifferentiated overlap, seam trouble.  This scoping decision alone
  prevented rebuilding things that already worked.
- **A concrete visual reference** (Animal Crossing: New Horizons tree) was
  chosen and *analyzed*, not just invoked: coverage is total; leaves large;
  overlap reads charming because every leaf sits proud of its neighbours;
  multi-cluster seams are unceremonious.
- **Requirements were written as testable guarantees** — total coverage
  above the FDM floor (#1), organic non-gridded arrangement, union-surface
  seams, distinct heights for overlapping leaves, down-slope coherent
  direction, supportless FDM, < 5 s per tree — plus named acceptance renders
  (two-cluster scene AND full tree) and the perf-crisis constraints carried
  in as requirements, not afterthoughts.

### Era 6 — the organic placer (66d85e3 → 43a2d51)

Phase A/B landed **the same day** as the interview, because the design was
now derivable from the requirements: boolean-union the clusters into one
placement surface; maximal Poisson-disk dart throw with a **normal-aware**
exact-distance grid; down-slope direction with a coherent positional angle
field; height layering via a standoff hook in the shared per-leaf pipeline.

Seven render-review iterations followed, each with the same rhythm — a
review names specific artifacts, instrumentation finds the root cause, the
fix is constructive (changes what *can* happen, not a parameter nudge):

| Iter | Named artifact (review) | Root cause | Constructive fix |
|---|---|---|---|
| bring-up | bald bands beside every seam | Euclidean disks block across seam-V walls | normal-aware conflict test (`_ROOT_BLOCK_COS`) |
| bring-up | bare bands over crests | blade spans 75/25 around its anchor | re-project anchor 0.25·L down-slope |
| 2 | bases over higher tips; sheet-through-sheet crossings; wall prisms; mid-canopy bare patches | slope-blind graph-colour layers; divergent directions; extreme skews; placement floor too high | monotone-in-height standoff; direction variation 25°→15°; skew cap; FLUSH blade zone |
| 3 | "artichoke, not Animal Crossing" (judge agent) | 1.2 mm tip lift flares tips off the mass | lift cut to clearance-only; ±20 % downward size jitter |
| 4 | neighbours still crossing; long-rooted pokers | global ramp spread ~0.04 mm between neighbours; ungated stitch stretch | height-sorted standoff *accumulation* (Shawn's suggestion, adopted verbatim); neck gate |
| 5 | proud bases; >2 mm extensions; shaggy edge-presenting blades | base end is the blade's high point; bury-lift unbounded and after the gate; 40° curl | base tuck (embedded base can't overlie anything); bury-lift capped, remainder tucked; curl capped 16° — **ACCEPTED** ("the placement is really nice... wow") |
| 6 | floating-island tips on undersides; long-neck chains | curl bends tips into air; base-to-base nestling propagated standoff | flush blades become a pure end-to-end ARCH (can never re-enter the surface); escalation through point-end conflicts only |
| 7 | hard zone switch visible | binary pitched/flush | smoothstep zone factor `st`; curl/lift/standoff/ceiling scale with st, arch with 1−st |

Then: cull leaves skewering *exposed* branches (3ff3d33 — culling ones
inside the canopy holed the skin, so the rule is exposed-only), double the
crown effect (43a2d51), and finally **delete the three losing placers**
(73e24f1), distilling their surviving machinery into `placement_leaf.py`.
Placement stats after the refactor were byte-identical to before it.

---

## Why the organic approach worked (technical)

1. **The union surface dissolved the seam problem instead of managing it.**
   Meridian and greedy fought cross-cluster burial with reactive culls;
   union makes "no leaf buried in a neighbour" vacuously true.
2. **Coverage became a mathematical property, not a tuning outcome.**
   Maximal Poisson-disk saturation guarantees no bare patch wider than
   2×spacing *by construction* — the #1 requirement can't regress under
   parameter changes, and a verification pass measures the residual every
   run.
3. **Ordering guarantees by construction.**  Height-sorted standoff
   accumulation makes "a higher-rooted leaf can never sit under a
   lower-rooted one" structurally true; the earlier global ramp tried to buy
   the same property with a budget spread too thin to survive geometry
   variation.
4. **Underside printability by shape, not by check.**  The arch blade cannot
   re-enter the surface between its endpoints; the tip tuck roots both ends.
   Where the old placers *tested* for FDM violations and culled (leaving
   holes), the organic underside blade *cannot produce* the violation.
5. **The losers' machinery was kept.**  Equal-depth seat, rigid frame,
   printability skew, belly-dip drop, exact-distance grid — all pre-debugged
   in eras 3–5.  Only the *strategy* was greenfielded; the per-leaf
   primitives were proven.
6. **Perf constraints were requirements, not cleanup.**  Everything batched,
   embree-only, cheap-reject before build — the placer landed at 5 s on a
   full tree and stayed there through seven iterations.

## Why it worked (process) — the part to reuse for grass

This is the answer to "what did we do differently, step by step, that made
it work when the others failed":

1. **We declared non-convergence honestly.**  The smell was visible long
   before it was named: the apex cap was patched six times; every greedy fix
   relocated an artifact rather than removing it (deep embed → long necks →
   tip-z lifts → re-burial).  When fixes stop removing problems and start
   moving them, the algorithm's *shape* is wrong, and parameter iteration
   will not converge.  The explicit stop — three placers side by side,
   judged, verdict recorded — is what unlocked everything after.
2. **Requirements interview before any design.**  Not a wishlist — a
   structured Q&A that (a) separated what's liked from what fails, so scope
   shrank to "placement + solidification only"; (b) forced a concrete
   reference image and *analyzed what it actually does*; (c) ended with
   numbered, testable requirements including a perf budget and named
   acceptance renders.  The design then wrote itself in a day.
3. **Requirements were restated as by-construction guarantees.**  For each
   "must", an algorithm element was chosen that makes violation impossible
   rather than unlikely: coverage → maximality; seams → union; height order →
   sorted accumulation; underside FDM → arch geometry.  This is the single
   biggest technical-process lesson.
4. **One render-judged iteration at a time, with artifacts named in plain
   language.**  "Artichoke", "wall chimneys", "floating-island tips",
   "long-neck chains", "bald bowl" — naming made each defect trackable to
   extinction, and every iteration commit records artifact → diagnosed root
   cause → constructive fix.  Fixes were judged on the same two acceptance
   scenes every time.
5. **Instrument before fixing.**  The z-band outcome trace found the root
   grid starvation; measured noise erosion (~2.3 mm) sized the root embed;
   the coverage verifier prints uncovered-test-points every run.  And
   instrumentation prevented fixing a *non*-problem: the stubborn "bald
   bowl" turned out to be the designed-bare underside of a tilted cluster,
   not a placement bug.
6. **A second aesthetic eye against the reference.**  Shawn's wireframe
   reviews drove iterations 2/4/5/6; an art-director judge agent compared
   renders to the AC reference side by side (iteration 3) and its punch list
   was triaged — adopted, deliberately rejected (visible rows violate Q6),
   or deferred to config.  Triage, not obedience.
7. **Knobs stayed module constants while iterating.**  No config plumbing
   until the look settles; promotion is a listed open item, not a blocker.
8. **The losers were deleted at the end, and their organs harvested.**  One
   generator, no dispatch, a distilled shared module
   (`placement_leaf.py`), regression-checked by identical placement stats.

## Applying this to grass

Grass is next: the current grass is not satisfying, and the plan is to reuse
the **development approach**, not any of the leaf algorithm.  The recipe,
concretely:

1. **Render the current grass and judge it side by side** (tile renders, a
   close-up scene equivalent to the two-cluster test).  Name what's liked
   and what offends in plain language, per artifact.  Check whether grass is
   in the "fixes relocate problems" regime.
2. **Run a requirements interview** before touching design.  Start by
   re-validating the existing docs (`docs/grass/requirements.md`,
   `docs/grass/design.md`, `docs/grass/grass-behavior.md`,
   `docs/floppy-grass-algorithm.md`) — they predate this method and likely
   mix requirements with implementation.  Find a concrete visual reference
   and analyze what it actually does.
3. **Scope by what's liked**: if blade geometry is fine and arrangement is
   the failure (or vice versa), rebuild only the failing layer and keep the
   proven machinery (growers, stamping, support rasterisation).
4. **Restate each requirement as a by-construction guarantee** and choose
   algorithm elements that make violations impossible — the leaf-era
   equivalents were maximality, union, sorted accumulation, arch geometry.
5. **Name acceptance scenes and a perf budget up front**; keep knobs as
   module constants while iterating.
6. **Iterate on renders with named artifacts**, instrumenting before fixing,
   with a human judge (and optionally a judge agent) against the reference.
7. **When accepted: delete the losers**, distill shared machinery, verify
   stats-identical refactors.

Known grass-specific open threads to feed the interview: blade directions
are purely i.i.d. random (`core/flow.py` was removed as dead code; the leaf
direction field — down-slope ⊕ coherent positional angle field — shows what
"coherent variation" buys), and the GrassCarpet/3D-grass split already
mirrors the carpet-underlayer idea from the Fable review.
