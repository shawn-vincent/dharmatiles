# Experimental Leaf Placement — Greedy Lowest-First Accretion (2026-07-01)

**Status: experimental, side-by-side.** Do NOT replace the current meridian-arc
placer. This is a parallel track to prototype and compare on renders. All
constants/names below are proposals.

---

## Scope (what this touches and what it does NOT)

This experiment is **only a new placement + solidification strategy**. Held fixed,
reused verbatim:

- **Leaf-root ovals** — the embedded root ring, its `embed_mm`, protrusion
  tolerance, and containment gate.
- **The leaf surface itself** — `build_leaf_surface`, its control points, taper,
  curl, cross-section, and all `leaf_kw` configurability.
- **Solidification** — `solidify_leaf` (skin blade-rim → oval-rim, 1:1 index
  correspondence, watertight neck).
- **Foliage-cluster and branch construction** — `_build_foliage_cluster_mesh`,
  `build_branch_mesh`, skeleton, all upstream tree geometry.

What changes is **where** each leaf's base lands and **in what order**, i.e. the
logic that today lives in `_build_meridians` / `_compute_row_z_positions` /
`_collect_row_slots` / the slot loop in `_place_leaf_slot`. The output of the new
path is the same thing the old one produces: per leaf, a `(base_pos, tangent,
up_hint, contact_angle)` pose fed into the **unchanged** surface builder +
solidifier. Contact-angle solve and normal pull-away are reused as-is.

## The Idea (as stated)

Instead of pre-computing rows and columns and placing leaves into them, grow the
canopy by greedy accretion:

> Loop until no mesh has a leaf-safe position left:
>   1. Pick a random parent mesh from the input set.
>   2. Pick a random leaf angle (the existing base-rotation jitter).
>   3. Find the **lowest** leaf-safe position on that mesh.
>   4. Fully construct the leaf solid there and commit it.

All meshes **and all previously placed leaves** are live inputs to the safety
test at every step. "We can build whatever data structure we want to track leaf
placements and remaining mesh surface — just make it fast."

### "Leaf-safe" (the placement predicate)

A candidate position is leaf-safe iff **all** hold:

1. **Root oval fully embeds** into the parent mesh (existing oval-contains gate,
   `placement.py:1105–1120`, with the `_OVAL_PROTRUSION_TOL_MM = 0.75` tolerance).
2. **Leaf surface fits** other leaves horizontally *and* vertically. Overlap is
   judged **only over the widest-point→tip span** of the blade; the base→widest
   span may overlap freely (bases nestle/tuck; tips must clear).
3. **Base point is on bare mesh**, not on top of another leaf.
4. **Leaf surface is not embedded** in *any* parent mesh in the scene (not just
   its own).

### Placement mechanics (unchanged primitives)

- **Contact angle**: the existing analytic belly-dip solve
  (`_contact_angle_analytic`, `_leaf_belly_dip`) — lean so the lowest belly point
  and the base match the parent-surface distance. One `acos`, no iteration.
- **Pull-away**: translate the built blade outward along `up_hint` until the leaf
  **surface** no longer intersects any leaf or any mesh (existing normal
  pull-away, `_PULL_CLEARANCE_MM`, `_PULL_MAX_MM`). Root oval stays plugged.

So per-leaf *construction* reuses everything the current path already solved fast.
The genuinely new machinery is only: (a) the candidate search ("lowest safe"),
and (b) the leaf-vs-leaf fit test (constraints 2 and 3).

---

## How It Differs From the Current Algorithm

The current placer (`place_leaves_on_multiple_meshes`) is a **deterministic
parametric grid**:

1. `_build_meridians` — slice each cluster at ~64 z-levels (`mesh.section`),
   building an axisymmetric meridian model. **Assumes star-convex cross-sections**
   (samples via a ray from the per-z centroid, `_polygon_point_at_phi`).
2. `_compute_row_z_positions` — rows at even surface-arc spacing (arc↔z inversion).
3. `_collect_row_slots` — columns per row at even `col_step = W·(1−h_overlap)`.
4. Per slot: analytic contact → build → curl pull-away → shingle layer (world
   voxel `occ`, proactive) → tip-z fix.
5. **After** building, a reactive cross-cluster cull removes any leaf with
   `≥ 30%` of blade verts inside a neighbour (`_CROSS_CLUSTER_BLADE_INSIDE_FRAC`).

| Axis | Current (meridian grid) | Proposed (greedy accretion) |
|---|---|---|
| Structure | Fixed rows × columns per cluster | No grid; positions emerge |
| Order | Arc/z-driven per cluster, batched | Global, lowest-z first, cross-cluster |
| Overlap model | Shingle world-voxel layer offset | "base overlaps free, tip must clear" |
| Cross-cluster | **Reactive** post-build fraction cull | **Native** — all meshes/leaves in the predicate |
| Surface assumption | **Star-convex** (ray from centroid) | **None** — samples the actual mesh |
| Determinism | Full, given seed | Full, given seed + tie-break rule |
| Wasted builds | ~66% (2156 built → 721 kept) | ~0 if cheap-reject precedes build |
| Coverage guarantee | Uniform arc coverage by construction | Greedy heuristic; can strand gaps |
| Look | Faint rings/rows visible | Non-gridded, more organic |

Two structural advantages fall out for free:

- **No star-convexity assumption.** Because candidate bases come from the mesh
  itself (face centres / a surface sample), the greedy sampler works on *any*
  surface — including the CSG **union** of overlapping clusters that Option C
  wanted but couldn't handle (the meridian ray-from-one-centroid breaks on a
  concave "peanut" section). This side-steps the entire cross-cluster meridian
  problem the current code fights with a post-hoc cull.
- **Cross-cluster is not a special case.** "Not embedded in *any* mesh" and "fits
  *all* placed leaves" make the seam just another region. No separate cull, no
  shared-vs-per-mesh occupancy split.

---

## Edge Cases You Should Design For

1. **"Lowest" means lowest *printable* — and printability ≜ the root-embed test.**
   (User decision.) We do NOT place at the lowest point regardless; we place at
   the lowest position where **the leaf root oval can fully embed into the foliage
   cluster** (constraint 1). On a strong downward overhang the surface curves away
   faster than the flat oval, so the oval protrudes and the candidate is rejected
   for free — the embed feasibility *is* the printability gate, no separate
   FDM-normal test needed. **Open question:** on a *gentle* underside the flat
   oval may still embed, so embed-feasibility might not fully exclude down-facing
   placements — keep a mild normal-elevation floor as a cheap backstop in reserve
   if renders show leaves on undersides.

2. **"Lowest leaf-safe position" is a search over a continuous surface.** You must
   discretise candidate bases (face centres, or a Poisson-disk surface sample, or
   a `(phi, z)` lattice). "Lowest" = min-z among *currently-safe* candidates. This
   is the whole performance question (see below) — naively it is
   O(placed × candidates × neighbours).

3. **Saturation detection — *dissolved* by the single-sweep model.** In the naive
   "find lowest safe each iteration" framing this was a heavy tail (re-scanning
   candidates that all fail). But with the pre-generated candidate set swept **once**
   in z-order (see Candidate Generation), "until no safe position" is simply "sweep
   reached the end." No re-scan, no tail. Each candidate is visited exactly once and
   either accepted (build) or dropped. This is the strongest reason to prefer the
   pre-generate-then-sweep formulation over per-leaf search.

4. **Angle-then-position coupling / wasted commitment.** If you fix the random
   roll angle *before* searching, a scan may find no safe base for that angle and
   waste the pass. Since the contact solve already fixes most of the pose (growth
   up the surface), keep the free "angle" tiny — just the existing azimuthal
   jitter — and, ideally, evaluate safety at a base *independent* of that jitter
   so the jitter never invalidates a found spot.

5. **Concave seams get no normal placement.** (User decision.) Between two touching
   clusters, pulling away from A can push *into* B. Rule: **no valid standoff ==
   not leaf-safe** — pack up to the seam from *both* directions and leave the
   crease. Cap the pull (`_PULL_MAX_MM`); if no standoff clears every mesh, the
   candidate is dead. The crease reads as a shadowed valley (accepted Option-A
   look). *Possible later enhancement (not now):* seed special candidates **in**
   the corner oriented along the seam bisector (combined outward normal) so leaves
   fan *out of* the valley — a valid pose the normal-placement rule excludes. Deferred.

6. **"Base on bare mesh, not another leaf" needs a bare-surface map.** This is a
   second dynamic occupancy query (base point vs placed-leaf footprints), not the
   tip-fit test. It shrinks as leaves land, which is why the "remaining surface"
   data structure matters.

7. **Greedy strands gaps.** Lowest-first is a heuristic, not a global optimiser.
   Early leaves can block a diagonal band that then never fills, where the grid
   would have covered it. The apex (highest → filled last) can get starved or pile
   up. Only a render tells you if this reads worse than the current faint rings.

8. **Determinism / tie-breaking.** Equal-z candidates need a stable order
   (z, then phi, then candidate index). Otherwise runs aren't reproducible.

9. **Base overlap is a *placement* allowance, not coplanarity.** (User correction.)
   Allowing base→widest footprints to overlap only means two leaves may be seeded
   close together; we **still nudge each outward along its normal** exactly as the
   current shingle offset does — no coplanar leaves. So the outward-standoff
   actuator (`layer × _SHINGLE_DELTA_MM · up_hint`, root oval stays plugged) is
   **retained wholesale**; only the *decision of which candidates coexist* changes.

10. **Mesh-selection weighting.** Uniform-random mesh choice under-fills large
    clusters relative to small ones within any fixed budget; since the loop runs
    to saturation this only affects *who wins contested seam space*, not final
    counts — but weight by remaining-safe-area if seam ownership looks wrong.

11. **Pull-away vs the fit test are coupled.** A leaf declared safe at build time
    can be pushed by pull-away into a neighbour it *was* clear of. Re-test the fit
    after the final pull, or fold the pull target into the fit test.

---

## Candidate Generation — "don't search the mesh, pre-generate" (user direction)

The key reframe (user): **we are not searching an unknown surface — we HAVE the
mesh.** So generate the entire candidate set up front, then the placement loop
never touches the mesh again. Two simplifications fall out:

### Use the pre-noise (smooth) cluster surface

The foliage cluster is surface-noise applied to a smooth swept D-section
cone/dome along the branch spine. That **pre-noise** surface is analytic: given
`(phi, s)` we get base point, outward normal, and up-slope tangent in closed form
— no `mesh.section`, no meridian slicing, no star-convex ray hack. Generate all
candidate poses on the smooth surface.

**Settled: smooth surface throughout** (generate, embed, *and* clear against
smooth), because **cluster noise only displaces INWARD.** Confirmed at source:
`mesh.py:1477-1482` subtracts `noise_peak` so max displacement is exactly 0 (the
smooth envelope) and "the full 2A trough range erodes inward — no outward
expansion." The smooth surface is therefore the strict outer envelope; the real
noisy surface is always at-or-below it. Two consequences make this provably correct, not just an approximation:

- **Clearance is free.** A blade held at/above the smooth surface (which the
  outward shingle nudge already guarantees) can *never* be reached by the real
  surface, since the real surface only recedes inward. So constraint 4 ("surface
  not embedded in any mesh") reduces to "clear of the smooth envelopes" — already
  guaranteed by building on smooth + nudging out. No real-mesh test needed.
- **Root embed must out-reach the noise.** The one thing that deliberately dips
  *inward* is the root oval (by `embed_mm`). To guarantee the root always reaches
  real material even in the deepest noise pit, require
  **`embed_mm > max inward noise amplitude`** (add an assertion / derive `embed_mm`
  from the cluster's noise config). Otherwise a root embedded from the smooth
  envelope can float over a noise valley. This is the *only* new numeric
  constraint the smooth-throughout choice imposes. Concretely: the deepest inward
  displacement is the noise's `2A` trough (`mesh.py` comment), so require
  `embed_mm > 2A_max`. Current `embed_mm = 0.75` (`leaf.py:804`) — **verify** it
  exceeds the cluster's actual max trough depth (`_noise_scale·(gaussian+coarse)`,
  smoothstep-scaled, peaks at the canopy rim); bump `embed_mm` or clamp noise if not.

**Global sweep, not per-cluster.** One z-ordered sweep across *all* clusters'
candidates at once. A seam is then resolved by whichever leaf's base is lower in
world-z — which is what makes cross-cluster placement native (no separate cull,
no per-mesh vs shared occupancy split).

### Over-pack, then resolve by conflict — on proxies, not solids

The winning structure unifies "greedy lowest-first" and "over-pack then cull":

> Pre-generate a candidate set **denser than can possibly fit**, each with its
> analytic pose. **Sort by printable-z** (lowest embeddable first). **Sweep
> upward once**, accepting a candidate iff it is compatible with the
> already-accepted set; assign its shingle layer as you go. Solidify **only**
> the accepted candidates.

Greedy-lowest-first *is* exactly this sweep over a precomputed set — the two ideas
were the same algorithm. Crucially, "compatible?" is tested on a **proxy** (the
leaf's tip-region sample points / its bounding oval), never a built solid, so the
sweep costs a spatial-hash lookup per candidate and geometry is built once per
*kept* leaf. That is the whole "over-pack then use intersects to check" idea,
made cheap by (a) proxy intersection and (b) build-after-accept.

### Candidate distribution: phyllotaxis > Voronoi > random > face-centres

The candidate seeds want even spacing at ~½ leaf pitch with no grid artifacts:

- **Golden-angle phyllotaxis** mapped onto the `(phi, s)` surface — leaves in
  nature *are* phyllotactic; gives organic, low-conflict density essentially for
  free, and is deterministic. Recommended first try.
- **Blue-noise / Voronoi-relaxed** samples (your "densely packed Voronoi grid") —
  even density, no lattice look; a couple of Lloyd relaxations. Good fallback.
- **Random / face-centres** — random clumps; face-centres inherit tessellation
  regularity. Use only as a baseline.

Over-generate whichever you pick (e.g. 2–3× the feasible count); the sweep's
conflict test does the thinning, so density just needs to exceed what fits.

## Making It Fast (the data-structure design)

The blunt truth: per **placed** leaf, this does the *same* geometry work as the
current path (analytic contact + build + pull-away) **plus** a search **plus** a
leaf-fit test. So it is only a win if it (a) avoids the current path's **wasted
builds** and (b) does the search/fit with cheap self-managed structures — never
`trimesh.proximity.closest_point`, R-tree intersection, or per-leaf `Trimesh`
scans (those *were* the 2026-07-01 performance crisis: 610 s / 54% of runtime;
they are a standing hard constraint and must not return).

### Where the win actually is

The current path builds **2156** leaves and culls **1435** (66%) after paying
full contact+build+curl cost on every one. If the greedy predicate rejects a bad
candidate **before** building geometry, those 1435 full builds evaporate. That,
not the search, is the headroom. **Design principle: cheap-reject → then build.**
Order the leaf-safe checks cheapest-first; only survivors reach `build_leaf_surface`.

Cheap-reject ladder (all O(1)–O(k), no geometry build):
1. Candidate already dead-marked → skip.
2. Base normal below FDM elevation → skip (array lookup).
3. Base voxel already "covered by a leaf base" in the **bare-surface grid** → skip.
4. Coarse footprint hits an occupied tip-region in the **leaf occupancy hash** → skip.
5. Base inside another mesh (embree `mesh.contains`, single point, ~0.002 ms) → skip.

Only now build the surface, run the fine tip-fit and the pull-away.

### The proxy and the per-candidate sweep (concrete sketch)

Reconciling constraint 3 ("base on bare mesh, not another leaf") with the
base-overlap allowance ("base→widest may overlap"): they act on different things,
so there are **three** occupancy concerns, not one —

| Concern | Governs | Structure | Test |
|---|---|---|---|
| **Root-point** (C3) | the single root *location* | coarse root-spot grid | base within `min_root_gap` of a claimed root → drop |
| **Tip-half** (C2) | the widest→tip *blade area* | shingle bitmask (world voxels) | pick lowest free layer over tip cells; none free → drop |
| **Clearance** (C1/C4) | root reaches material; blade clears mesh | smooth envelope + `embed_mm>noise` | free (see Candidate Generation) |

The base→widest blade area is deliberately **unconstrained** — that region tucks
under a neighbour's blade, which is exactly imbrication.

**The proxy = tip-half sample points only.** In the leaf's placement frame
(`base=pt3d`, `t=tangent` up-slope, `lat=normalize(cross(up_hint,t))`, widest at
length-fraction `f_w`, tip at `L`), sample a tiny grid over `[f_w·L, L]` at
**delta = 0** (on the smooth surface, layer-independent, so overlapping tips hash
to shared voxels):

```python
def tip_proxy_cells(base, t, lat, L, f_w, width_at, cell,
                    n_along=4, n_across=3):
    cells = set()
    for i in range(n_along):
        f = f_w + (1.0 - f_w) * i / (n_along - 1)     # widest → tip
        c = base + f * L * t
        hw = 0.5 * width_at(f)                          # blade tapers to tip
        for j in range(n_across):
            a = (-1.0 + 2.0 * j / (n_across - 1)) * hw  # −hw … +hw across
            p = c + a * lat
            cells.add((int(p[0] // cell), int(p[1] // cell), int(p[2] // cell)))
    return cells
```

**Per-candidate sweep** (single global pass, candidates pre-sorted by base
world-z ascending):

```python
for cand in candidates_sorted_by_z:            # global, all clusters
    base, n, t = cand.base, cand.normal, cand.tangent   # analytic, from smooth
    if root_grid.occupied_near(base, min_root_gap):     # C3
        continue
    if seam(cand):                              # no valid outward standoff
        continue
    # C1/C4 are guaranteed by smooth-envelope + embed_mm>noise; no test here.
    lat   = normalize(cross(n, t))
    cells = tip_proxy_cells(base, t, lat, L, f_w, width_at, CELL)
    layer = lowest_free_layer(occ, cells)       # C2: shingle bitmask
    if layer is None:                           # tip region saturated to cap
        continue
    # ---- ACCEPT: build once, on the smooth surface ----
    base_blade = base + layer * DELTA_MM * n    # retained outward nudge
    surf = build_leaf_surface(base_pos=base_blade, tangent=t, up_hint=n, ...)
    solid = solidify_leaf(surf, oval_at(base))  # root oval plugged at base
    occ.write(cells, layer)
    root_grid.mark(base)
    emit(solid)
```

No mesh touched in the loop; accepted candidates build exactly once; rejected
candidates cost a hash lookup. `lowest_free_layer` / `occ.write` are the existing
`_shingle_pick_layer` / `_shingle_write` (key-agnostic) — only the *cells* change
(tip-half proxy instead of full footprint). This is the whole runtime.

### Structure 1 — candidate surface samples, z-sorted, with a live cursor

Precompute candidate bases once per placement run: sample every mesh surface
(face centres, or blue-noise for evenness), store `(z, phi, mesh_id, point,
normal)` and **sort ascending by z**. Keep a per-mesh cursor into this list.
"Find lowest safe" = advance from the cursor, return the first candidate passing
the cheap-reject ladder. Dead/committed candidates below the cursor are never
revisited → the scan is amortised near-O(1) per placement, not O(N). This is the
whole answer to "lowest is a search": pre-sort once, sweep upward, never look back.

### Structure 2 — leaf occupancy hash (the tip-fit test)

A **uniform spatial hash** (`dict[(ix,iy,iz)] → list[leaf_id]`, cell ≈ ½ leaf
width) storing **sample points of each placed leaf's widest→tip span only**
(constraint 2 explicitly excludes base→widest). To test a candidate: hash its own
widest→tip samples, gather leaf_ids from those + neighbour cells, and do a cheap
point-cloud proximity (a handful of squared-distance comparisons, or a small
`cKDTree` over just the local sample set). This is the history's own blessed
"Next Step B" (point-cloud + KD-tree instead of per-mesh BVH) — cheap because leaf
sample clouds are tiny and the hash bounds neighbours to O(k). Insert a leaf's
tip samples on commit; O(samples) amortised.

### Structure 3 — bare-surface grid (base-on-bare-mesh + saturation)

A coarse grid keyed the same way as Structure 1's candidates. When a leaf commits,
mark the surface cells its **base/oval footprint** covers as "leaf-occupied".
Constraint 3 (base on bare mesh) is then an O(1) cell lookup. **Saturation** is
"cursor reached the top on every mesh with no placement" — and because covered
cells dead-mark their candidates, the heavy failing tail shrinks: exhausted
regions stop generating live candidates.

### Structure 4 — embree for the two containment gates only

Keep embree `mesh.contains` (already a dep, ~0.002 ms/point) for "root oval
embeds" and "surface not inside any mesh". Prune the mesh set per candidate with
an AABB test on the base point (only clusters within one leaf-length can be hit) —
the same bounding-sphere prune already added for the cross-cluster cull.

### Expected cost

Per **committed** leaf: cheap ladder (µs) + one build (~few ms, same as today) +
one pull-away `on_surface` (~2.8 ms, same as today). Per **rejected** candidate:
just the ladder (µs) — no build. If cheap-reject kills most bad candidates before
geometry, total geometry builds drop from ~2156 toward ~721, so the greedy tile
could land **at or below** the current ~28 s *while* producing organic coverage
and native cross-cluster handling. The risk is the saturation tail and fit-test
constant; both are bounded by the hash + dead-marking above. Net: **plausibly
faster, not guaranteed** — must be measured on the two-cluster test first.

---

## Recommendation

Prototype **alongside** the current placer, gated behind a flag, validated on
`src/scripts/test-multi-parent-mesh-leaves.py` (the A+B two-cluster case, ~5 s)
before any full tile. Build the four data structures from day one — retrofitting
speed onto a `closest_point` prototype is exactly the trap that cost a full
session in the deconfliction crisis. First render should answer:

- Does organic (non-ringed) coverage read better than the current faint rows?
- Does "lowest-first + FDM gate" avoid wasting placements on hidden undersides?
- Does the seam fill naturally (native cross-cluster) vs the current bald cull?
- Is the saturation tail bounded (dead-marking working) — total time vs 28 s?

Keep the hard constraints in force: no `closest_point`/R-tree/`placed_meshes`
deconfliction, no `fix_normals`; proactive cheap-reject before every build.

---

# IMPLEMENTATION PLAN (execute in a fresh context — do NOT start yet)

Goal: a **second** leaf placer (greedy lowest-first), selectable via config,
running **in parallel** with the existing meridian-arc placer. Default stays
`meridian` so all existing tiles are byte-identical. Two callers switch to the
new path: the multi-parent test and the 1×1 grass-tree+water tile.

## Grounding — the exact wiring today

Config threads through **three** layers as flat kwargs (no dataclass):

```
Tree.__init__            src/dharmatiles/trees/layer.py:36    (leaf_* attrs, lines 71–135)
  └─ build_branch_mesh   src/dharmatiles/trees/mesh.py:55     (leaf_* params, lines 73–92)
       └─ place_leaves_on_multiple_meshes  src/dharmatiles/trees/placement.py:1248
```

- Foliage clumps are built in the BFS loop (`mesh.py:260–289`) via
  `_build_foliage_cluster_mesh(... leaves=False)` (`mesh.py:1147`), collected into
  `foliage_clumps` (noised meshes), then placed in **one** batched call
  (`mesh.py:303–325`).
- **Noise is applied inside** `_build_foliage_cluster_mesh` at `mesh.py:1473–1482`
  (inward-only; smooth envelope = pre-noise verts).
- Leaf geometry primitives to REUSE unchanged: `build_leaf_surface`
  (`leaf.py:611`), `build_leaf_oval_offsets` (`leaf.py:798`), `solidify_leaf`
  (`leaf.py:852`); width profile peaks at **s ≈ 1/3** (`leaf.py:198`) → `f_w = 1/3`.
- Analytic contact + belly dip: `_contact_angle_analytic`, `_leaf_belly_dip`
  (`placement.py:421`, `:382`). Pull-away + tip-z: `_PULL_*`, `_TIP_Z_*`
  (`placement.py:125`, `:63`). Shingle helpers `_shingle_pick_layer` /
  `_shingle_write` (`placement.py:171`, `:188`) are key-agnostic — reuse verbatim.
- `LeafPlacementStats` (`placement.py:204`) is the return contract the test's
  `_check_artifacts` (`test-multi-parent-mesh-leaves.py:109`) consumes — the greedy
  path must return the same dataclass (fields may be zero/empty where N/A).

## Phase 0 — config selector plumbing (no algorithm yet)

Thread one string, default preserves current behaviour.

1. `layer.py` `Tree.__init__`: add `leaf_placement: str = "meridian"`, store
   `self.leaf_placement = str(leaf_placement)`. Pass `leaf_placement=self.leaf_placement`
   into the `build_branch_mesh(...)` call (near `layer.py:208–234`).
2. `mesh.py` `build_branch_mesh`: add `leaf_placement: str = "meridian"` param;
   thread it to where the batched placement happens.
3. `mesh.py` batched block (`:303–325`): branch on `leaf_placement`. `"meridian"`
   → today's `place_leaves_on_multiple_meshes(...)` call unchanged. `"greedy"`
   → new `place_leaves_greedy(...)` (Phase 2). Keep the `_leaf_parts` /
   `leaf_solids.extend` plumbing identical for both.
4. Validate Phase 0 alone: with default `"meridian"`, regenerate nothing yet —
   just `python -m compileall` the three files and confirm an unknown string
   raises a clear `ValueError`.

## Phase 1 — smooth-surface acquisition

The greedy placer needs the **pre-noise** clump. Noise is inward-only, so the
smooth envelope is `verts` *before* `mesh.py:1482`.

1. `_build_foliage_cluster_mesh`: add `apply_noise: bool = True`. When `False`,
   skip the `disp` displacement (`:1473–1482`) and return the smooth envelope
   mesh (still `fix_normals()`'d). The visible foliage clump keeps `apply_noise=True`.
2. `build_branch_mesh` BFS loop: when `leaf_placement=="greedy"`, ALSO build a
   smooth clump per tip (`apply_noise=False`) and collect `foliage_clumps_smooth`
   in parallel with the noised `foliage_solids`. The noised clump still feeds
   `foliage_solids` (visible); the **smooth** clumps feed `place_leaves_greedy`.
   (Cheapest impl: build the clump once with `apply_noise=False` for the smooth
   copy and once `True` for the visible copy; or refactor to return both from one
   call. Prefer one call returning `(noised, smooth)` to avoid double icosphere cost.)
3. Assert `embed_mm > 2·A_max` where `A_max` is the clump's max inward noise
   amplitude (derive from the noise config feeding `mesh.py:1473–1476`,
   `_noise_scale·(gaussian+coarse)`). If `embed_mm` (0.75) is too small, bump it
   or clamp noise. Log once per run.

## Phase 2 — the greedy placer (`placement_greedy.py`, new module)

New file `src/dharmatiles/trees/placement_greedy.py`. One public entry mirroring
the meridian signature so the dispatch in `mesh.py` is a drop-in:

```python
def place_leaves_greedy(
    meshes: list[trimesh.Trimesh],          # SMOOTH envelopes
    *,
    length_mm, width_mm, thickness_mm, fold_angle_deg,
    inner_curve, outer_curve, curl_deg, lift_mm,
    seeds: int | list[int] = 0,
    labels: str | list[str] | None = None,
    angle_jitter_deg: float = 0.0,
    pos_jitter: float = 0.0,
    # greedy-specific (module-const defaults, promote to config later):
    candidate_density: float = 2.5,         # over-generation factor
    min_root_gap_mm: float | None = None,   # default ← width_mm × packing factor
    row_color_fn=None,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
```

Internals, in order:

1. **Candidate generation (per mesh, once).** Golden-angle phyllotaxis over the
   smooth surface parametrised by `(t ∈ [0,1] along spine/height, φ)`. For each
   sample compute `base`, outward `normal`, up-slope `tangent` from the smooth
   surface (reuse the clump's own cross-section radius function; normals via the
   smooth mesh). Over-generate `candidate_density × expected_count`. Tag each
   candidate with `mesh_id`. Fallback generator: blue-noise/Voronoi if phyllotaxis
   reads too regular (leave a `candidate_mode` switch).
2. **Global sort** all candidates by `base.z` ascending; deterministic tiebreak
   `(z, φ, mesh_id, idx)`.
3. **Occupancy structures** (all module-local, no trimesh proximity):
   - `root_grid`: `dict[(ix,iy,iz)→count]` at cell = `min_root_gap_mm`; C3 lookup.
   - `occ`: shared world-voxel shingle bitmask (reuse `_shingle_*`), cell =
     `_SHINGLE_WORLD_CELL_MM`. **Shared across all meshes** (global sweep).
   - per-mesh embree `mesh.contains` handle for the seam/standoff test only.
4. **Sweep** (the pseudocode already in this doc, "per-candidate sweep"):
   cheap-reject ladder → `tip_proxy_cells` (`f_w=1/3`, width from
   `_leaf_width_profile`) → `_shingle_pick_layer` → on accept:
   `base_blade = base + layer·_SHINGLE_DELTA_MM·normal`; analytic contact angle;
   `build_leaf_surface(base_pos=base_blade, tangent, up_hint=normal, **leaf_kw)`;
   `inner_v = build_leaf_oval_offsets(...) + base`; `solidify_leaf`; pull-away
   (`_PULL_*`) measured against the **smooth** mesh; tip-z fix (`_TIP_Z_*`);
   `_shingle_write`; `root_grid` mark; append part + stats.
5. **Seam / standoff test**: a candidate is dead if pulling out along `normal`
   immediately re-enters another mesh (embree `contains(base + ε·normal)` on
   neighbours, AABB-pruned). No valid standoff ⇒ skip (pack up to seam).
6. **Stats**: fill `LeafPlacementStats` (per-mesh) with the fields the test reads
   (`leaf_float_dists`, `leaf_buried_depths`, placed/attempted, `shingle_layers`,
   `pull_aways`, `tip_z_*`). Zero/empty the meridian-only fields (`rows`,
   `row_perims`).

Reuse, do NOT re-implement: `_contact_angle_analytic`, `_leaf_belly_dip`,
`_shingle_pick_layer/_write`, `_inside_neighbour` (embree contains + AABB reject,
`placement.py:475`), `build_leaf_surface/oval_offsets/solidify_leaf`. Import them;
keep `placement.py` as the home of shared primitives.

## Phase 3 — wire the two callers to the new config

- **`src/scripts/test-multi-parent-mesh-leaves.py`**: it calls
  `place_leaves_on_multiple_meshes` directly (`:76–87`). Add a module switch
  `PLACEMENT = "greedy"` (or `argv` flag) and branch to `place_leaves_greedy(...)`
  with the same `_LEAF` kwargs. Keep the meridian path reachable for side-by-side
  renders. `_check_artifacts` stays as-is (same stats contract).
- **`src/tiles/water/1x1-grass-tree+water.tile.py`**: add
  `leaf_placement="greedy"` to the `Tree(...)` call (after `foliage_clusters=True`,
  near the existing `leaf_*` kwargs). No other change; the tile drives the whole
  Tree→mesh→greedy path.

## Phase 4 — validation (in order; do not skip to the tile)

1. `python -m compileall -q src/dharmatiles/trees/*.py src/tiles/water/1x1-grass-tree+water.tile.py`.
2. **Two-cluster test first** (~5 s):
   `python -u src/scripts/test-multi-parent-mesh-leaves.py /tmp/greedy-iter.stl`.
   Eyeball top/side/persp renders: organic (non-ringed) coverage, seam packed to
   the crease not skewered, apex rosette present, no undersides. Confirm timing
   ≤ meridian baseline and watertight per-leaf solids.
3. Compare against `PLACEMENT="meridian"` render side-by-side (silhouette parity).
4. Only then the tile:
   `dharmatiles-gen --tile "src/tiles/water/1x1-grass-tree+water.tile.py" --formats db`
   (use `--no-png` if the pyrender teardown still exits non-zero — see the
   2026-07-01 perf notes). Report leaf/face count, runtime, watertight status vs
   the meridian tile.
5. Regenerate the **meridian** tiles too and diff counts to prove Phase 0 didn't
   perturb the default path.

## Sequencing / rollback

Phases 0→1→2→3→4 are independently landable. After Phase 0+1, `"greedy"` can be a
stub returning `([], [])` (empty canopy) that still generates a valid tree — a
safe checkpoint. The meridian path is never edited (only branched around), so
rollback is deleting `placement_greedy.py` and the selector.

## Open items to resolve during implementation

- `min_root_gap_mm` default (coverage-density knob now that layers handle overlap)
  — start at `width_mm × 0.5`, tune on render.
- `candidate_density` — 2–3× feasible; too low = bald, too high = wasted sweep.
- Whether phyllotaxis or Voronoi reads better (keep both behind `candidate_mode`).
- Confirm `embed_mm=0.75 > 2·A_max` for the real cluster noise; adjust if not.
- Decide if any `greedy_*` knobs get promoted from module constants to `Tree(...)`
  config in this pass or a follow-up (selector-only is the minimum ask).

---

# IMPLEMENTATION RESULTS (2026-07-01, all phases landed on `greedy-leaf-placement`)

All five phases implemented, tested, and pushed. Meridian remains the default;
`leaf_placement="greedy"` opts in. Summary of what was built and measured.

## What shipped

- **Phase 0** — `leaf_placement` string threaded `Tree.__init__` →
  `build_branch_mesh` → the batched dispatch; unknown value raises `ValueError`.
- **Phase 1** — `_build_foliage_cluster_mesh(apply_noise=…)`; greedy path builds
  a smooth (pre-noise) envelope per terminal tip (`foliage_clumps_smooth`),
  same topology as the noised clump, provably the strict outer surface.
- **Phase 2** — `placement_greedy.py`: candidate over-generation on the smooth
  surface → global z-sort → single sweep with a cheap-reject ladder
  (root-gap grid, seam/standoff embree point test, thin-region punch-through
  guard, analytic lean, tip-half shingle occupancy). Geometry built once per
  accepted leaf, reusing the meridian primitives verbatim.
- **Phase 3** — `test-multi-parent-mesh-leaves.py --placement {meridian,greedy}`;
  `1x1-grass-tree+water.tile.py` set to `leaf_placement="greedy"`.
- **Phase 4** — validation below.

## Correctness — the embed / connection finding (important)

The doc's premise that `embed_mm` must beat the noise was correct, but the
magnitude was under-stated. The noise is **peak-shifted** (`mesh.py`), so the
surface erodes inward by the full peak-to-trough *range*, not a single-sided
amplitude. **Measured** max inward erosion on the test clusters is **~2.3 mm**,
i.e. ≈2× `_FOLIAGE_MAX_NOISE_MM` (1.2). The greedy embed is therefore derived as
`_MAX_INWARD_EROSION_MM (≈2.6) + 0.5 = ~3.1 mm`, and — because a scalar-bound
proof is fragile across the curved oval span — each accepted leaf's root oval is
tested for connection **directly against the real noised clump** (`real_meshes`,
passed alongside the smooth envelopes). This is the "defensive: only correct
leaves are built" guarantee; the new test caught 2–3 would-be-detached leaves
before it was added.

`src/scripts/test-greedy-leaf-placement.py` asserts, and all pass: non-empty
placement, **every** leaf watertight, **every** root connected to the real
noised clump, tip-z ordering, and determinism.

## Performance (measured)

Two-cluster case (`test-multi-parent-mesh-leaves.py`):

| | Meridian | Greedy |
|---|---|---|
| leaves | 54 | 45 |
| placement time | 0.54 s | **0.27 s** |
| per-leaf watertight | yes | yes |

`1x1-grass-tree+water` tile, meadow `Tree` layer (db, `--no-png`):

| | Meridian | Greedy |
|---|---|---|
| leaves placed | (grid) | 342 |
| leaf placement | 20.6 s | **6.3 s** |
| Tree layer total | 26.0 s | **16.2 s** |
| export verts / faces | 355,110 / 698,746 | **261,876 / 513,794** |

Greedy is ~3.3× faster on leaf placement and ~1.6× faster on the whole Tree
layer, with a lighter mesh — the "avoid wasted builds" thesis held.

## Meridian-default unperturbed (proven)

`ground/1x1-grass-tree` and `ground/2x2-grass-tree` regenerated on the branch and
on `main` produce **byte-identical** vertex/face counts (259,855 / 512,498 and
419,313 / 829,322). Phases 0–3 do not touch the default path.

## Open items / next steps (not blocking)

- **Renders not yet eyeballed** for organic-vs-ringed look, seam packing, apex
  rosette (the doc's first-render questions). The numbers and correctness gates
  pass; the aesthetic judgement still wants a human look at the PNGs.
- `min_root_gap_mm` left at `width_mm × 0.5`; `candidate_density` at 2.5. Greedy
  currently packs denser than meridian near seams — tune on render if desired.
- Phyllotaxis candidate mode still a `candidate_mode` TODO (current: area-weighted
  random, which + root-gap rejection already yields blue-noise dart-throwing).
- Greedy knobs remain module constants; promote to `Tree(...)` config in a
  follow-up if the experiment graduates.
