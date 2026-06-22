# Leaf Placement Debug Pipeline — Code Review — 2026-06-22

**Scope:** End-to-end review of the new leaf placement and solidification pipeline,
introduced across three commits on 2026-06-22 (`ebba32f`, `1299b3e`, `97d0f2f`).
Covers all new code in `src/dharmatiles/trees/leaf.py` and both debug scripts.
The old keel-leaf path used by main dharmatiles production is explicitly out of scope;
this new implementation is being evaluated as a potential replacement.

**Files reviewed:**
- `src/dharmatiles/trees/leaf.py` — new sections: surface-query helpers, solidification
  and FDM analysis, `find_max_dip_for_sphere`
- `src/scripts/generate-debug-leaf-placement-simplified-stl.py` — refactored to use
  the new leaf.py helpers
- `src/scripts/generate-debug-leaf-placement-random-stl.py` — new script

**Method:** 7-angle multi-agent review (line-by-line, removed-behavior, cross-file
callers, reuse, simplification, efficiency, altitude) followed by single-verifier
confirmation. Four confirmed/plausible bugs were fixed before this entry was written.

---

## Bugs Fixed (during this session)

### 1. `find_max_dip_for_sphere` — bracket logic inverted, returned 0.0 for valid-at-all-angles case

**Severity: HIGH (correctness)**  
**Fixed in:** `63b825c`

The original bracket phase used a while-loop to halve `hi` from π while `_ok(hi)`
was True:

```python
lo, hi = 0.0, np.pi
while hi > 1e-4 and _ok(hi):
    hi /= 2.0
if _ok(hi):
    return 0.0
```

**Problem:** `_ok(theta)` returns True when no leaf vertex penetrates the sphere.
For a small leaf on a large sphere, `_ok(π)` may be True (the leaf never penetrates
even at 180° dip). The halving loop then runs until `hi ≤ 1e-4` with `_ok` still
True — a value that is, if anything, *more* collision-free than π. The guard
`if _ok(hi): return 0.0` then fires and returns flat (0° dip), discarding the correct
answer (any dip up to π is valid).

**Fix:**

```python
lo, hi = 0.0, np.pi
if _ok(hi):
    return hi   # never penetrates; return maximum valid dip (π)
for _ in range(48):
    mid = 0.5 * (lo + hi)
    if _ok(mid): lo = mid
    else:        hi = mid
return lo
```

Note: the same inverted bracket appeared in `_find_tilt` in the simplified script
(which is also a duplicate of this function — see cleanup items below).

---

### 2. `build_leaf_mesh` — `outer_curve` default 0.6 vs 0.72 in `build_leaf_surface`; stale docstring

**Severity: MEDIUM (API inconsistency)**  
**Fixed in:** `63b825c`

`build_leaf_mesh` had `outer_curve=0.6`; `build_leaf_surface` and
`compute_leaf_geometry` both had `outer_curve=0.72`. The `build_leaf_mesh` docstring
also claimed "Default 0.15" — a value from an earlier prototype.

A caller switching from `build_leaf_surface` to `build_leaf_mesh` without an explicit
`outer_curve` would get a visibly different cross-section shape.

**Fix:** Aligned `build_leaf_mesh` to `outer_curve=0.72` and corrected the docstring.

---

### 3. `solidify_leaf` — centroid cap vertex can fall outside root ring on non-planar rings

**Severity: MEDIUM (geometry, plausible at high dip)**  
**Fixed in:** `63b825c`

```python
center = root.mean(axis=0)
```

When the root ring is non-planar (a deeply-dipped leaf on a small sphere), the
arithmetic mean of the ring vertices can fall outside the ring's 3D convex hull.
A cap fan from an exterior centroid produces self-intersecting triangles.
`fix_normals()` corrects winding but cannot resolve self-intersections.

**Fix:** Project the raw mean onto the cap plane (normal `n`, through `root[0]`):

```python
raw_center = root.mean(axis=0)
center     = raw_center - float(np.dot(raw_center - root[0], n)) * n
```

---

### 4. `leaf_placement_from_surface` — negative barycentric weights extrapolate `up_hint`

**Severity: LOW (geometry, reachable via floating-point)**  
**Fixed in:** `63b825c`

`trimesh.triangles.points_to_barycentric` can return a small negative weight when
`closest_point` snaps `base_pos` to a shared triangle edge, placing it just outside
the triangle in the plane. The negative weight causes `bary @ v_normals` to
extrapolate rather than interpolate, producing a subtly wrong `up_hint` (outward
surface normal), which misaligns the arch and keel.

**Fix:** Clamp and renormalise before blending:

```python
bary     = np.clip(bary, 0.0, 1.0)
bary_sum = float(bary.sum())
if bary_sum > 1e-10:
    bary /= bary_sum
up_hint = _safe_norm(bary @ v_normals)
```

---

## Open Items (not fixed — future work)

### 5. `_find_tilt` (simplified script) is a structural duplicate of `find_max_dip_for_sphere`

**Severity: MEDIUM (duplication, latent divergence)**

`_find_tilt` in the simplified script (~lines 88–148) implements the same
flat-leaf build, far-vertex mask, Rodrigues rotation, and bisection loop as
`find_max_dip_for_sphere` in `leaf.py`. The two already diverge on the Rodrigues
axis: the simplified script uses `cross(L, surface_normal)` while `leaf.py` uses
`cross(-T0, up_hint)` — equivalent only when sign conventions align, which they do
for the equatorial placements in the simplified script but may not for all
orientations.

**Recommended fix:** Refactor the simplified script to derive its `L`/`surface_normal`
frame and call `find_max_dip_for_sphere` directly, deleting ~60 lines. The sign
convention difference (`L = -T0`) must be verified before removing `_find_tilt`.

---

### 6. `_fdm_tip_outside_support` (simplified script) imports dead private symbols

**Severity: LOW (maintenance)**

`_fdm_tip_outside_support` at line 193 of the simplified script contains:

```python
from dharmatiles.trees.leaf import (
    boundary_loop, _LEAF_ROOT_DEPTH_MM, _LEAF_FDM_FLOOR_DEG,
    _segment_length_outside_sphere,
)
```

`_LEAF_ROOT_DEPTH_MM` and `_segment_length_outside_sphere` are never used — the
function checks `support_mesh.contains()` / `.nearest.on_surface()` directly.
The deferred import (inside the function body) means the `ImportError` for a renamed
private symbol would surface only at call time, not at module load.

**Recommended fix:** Remove the two dead imports. Consider whether the function
should call `solidify_leaf`'s internal tip-check logic rather than re-implementing it.

---

### 7. `leaf_placement_from_surface` docstring references removed steps 4 and 5

**Severity: LOW (clarity)**

The inline comments inside `leaf_placement_from_surface` are numbered `# 1.`, `# 2.`,
`# 3.` — the docstring's `Notes` section mentions steps 4 and 5 (Apply twist, Dip)
that were removed when `twist_deg` and `dip_deg` were eliminated from the public API.
A developer extending the function would search for the non-existent steps.

**Recommended fix:** Renumber comments to `# 1.`, `# 2.`, `# 3.` only. Update the
`Notes` section to describe what the caller should do to apply dip.

---

### 8. `color_leaf_walls_by_fdm` is debug/visualisation code in the production geometry module

**Severity: LOW (altitude)**

`color_leaf_walls_by_fdm` (green/red face colouring for FDM overhang analysis)
lives in `leaf.py` alongside `build_leaf_mesh`, `build_leaf_surface`, and
`solidify_leaf`. Every production import of `leaf.py` (e.g. for tree generation or
tile builds) pulls in the ray-casting analysis code. If trimesh's BVH ray intersector
performance regresses, it shows up as a production cost even though colouring is
never called in production paths.

**Recommended fix:** Move `color_leaf_walls_by_fdm` to a debug/scripts layer
(e.g. `src/scripts/_leaf_debug.py`) and import it only from the two debug scripts.

---

### 9. `find_max_dip_for_sphere` is sphere-only; no reuse path for branch-tube attachment

**Severity: LOW (altitude, future-proofing)**

The function computes `np.linalg.norm(vertices, axis=1)` to measure distance from the
origin — correct only for a sphere centred at origin. When leaves are attached to
branch tube meshes, this function is wrong and a new `find_max_dip_for_tube` (or
similar) must be written with the bisection logic duplicated again.

**Recommended fix:** Generalise the collision check to a callable:
`is_clear: Callable[[np.ndarray], bool]`. The sphere case becomes:
`is_clear = lambda pts: np.all(np.linalg.norm(pts, axis=1) >= radius)`.
This makes the bisection loop reusable for any convex parent shape without adding
complexity to the calling code.

---

### 10. Random script imports private constants by underscore name

**Severity: LOW (maintenance)**

`generate-debug-leaf-placement-random-stl.py` imports
`_LEAF_LENGTH_MM_DEFAULT`, `_LEAF_WIDTH_MM_DEFAULT`, `_LEAF_ROOT_DEPTH_MM` from
`leaf.py` at module level. Leading-underscore names signal "do not import" across
Python tooling; IDEs suppress exported-symbol hints for them and renaming them in
`leaf.py` breaks the import with no warning.

**Recommended fix:** Promote these three constants to public names (no leading
underscore), or have the script use the public function signature defaults directly
and pass the root depth as a literal where needed.
