# Leaf Placement Code Review — 2026-06-27

**Files reviewed:** `src/dharmatiles/trees/placement.py`, `src/scripts/test-leaf-placement.py`,
and the leaf-placement sections of `src/dharmatiles/trees/mesh.py`
(`_build_foliage_cluster_mesh`, `_build_meridians`, `_compute_row_z_positions`,
`_interpolate_meridian_normal`, `_contact_angle_for_sphere`).

**Design reference:** `docs/design/leaf-placement.md` (meridian-arc algorithm).

---

## What Works Well

- The meridian-arc algorithm is fully implemented: `_build_meridians`, `_compute_row_z_positions`,
  `_interpolate_meridian_normal`, and `_contact_angle_for_sphere` match the design specification
  closely, with one intentional divergence documented in finding 3 below.
- The contact-angle analytical formula (`arctan2(dN, dL) + arcsin(DLN / 2R)`) is correct and
  cached efficiently.
- The belly-dip argmin (scanning tip-half midrib + tip vertex) is sound and documented.
- The `_build_meridians` ray-intersection approach (not the old angular-sector vertex search) gives
  accurate perimeter points at each azimuth.
- The top-row pinning that bypasses the arc↔z roundtrip on asymmetric clusters is a good fix;
  it's explained in `_compute_row_z_positions`.
- `placement.py` / `test-leaf-placement.py` is a solid diagnostic harness: 16 artifact checks,
  effective-overlap calculation, per-row coverage bars.

---

## Top 3 Issues

### 1. Test script validates a different code path than production

**What it is:** The meridian-arc placement loop is implemented **twice**:

- `placement.py:246-391` — `place_leaves_on_mesh`: called **only** from `test-leaf-placement.py`.
- `mesh.py:1480-1543` — inline loop inside `_build_foliage_cluster_mesh`: called **in
  production** from `build_branch_mesh`.

Both use the same shared helpers (`_build_meridians`, `_compute_row_z_positions`,
`_interpolate_meridian_normal`, `_contact_angle_for_sphere`), but the outer loops and
leaf-building code are **separate and have diverged**:

| Feature | `placement.py` | `mesh.py` inline |
|---|---|---|
| `ca ≥ π/2` guard | sets `ca = 0.0` | sets `ca = 0.0` |
| `local_r < 1.0` guard | increments `skipped_small_r`, `continue` | returns early inside `_emit_leaf` |
| Stats collection | full `LeafPlacementStats` | none |
| Jitter | not implemented | `jit = 0.0`, `pj = 0.0` wired in but disabled |
| Base-pos noise sink | not present | `_emit_leaf` sinks base onto noised skin |
| ca cache key | `round(r * 1000)` (int) | 5-tuple with geometry fields |
| Emit geometry scaling | not present | `emit_length_scale`, `emit_width_scale`, etc. |

**Why it matters:** Bugs fixed in `placement.py` (what the test validates) will not automatically
fix the production code, and vice versa. A regression in `mesh.py`'s inline loop will not be
caught by the test harness.

**Recommendation:** Have `_build_foliage_cluster_mesh` call `place_leaves_on_mesh` directly,
or extract the shared row+column loop into a third function that both callers use. The per-leaf
stats collection in `placement.py` can be optional (pass `None` for stats in production).

---

### 2. Dead constants and deprecated no-op parameters litter `mesh.py`

**Dead module-level constants** (defined but never read anywhere in the codebase):

```python
_LEAF_BASE_EMBED_MM    = 0.0   # mesh.py:632  — leftover from old placement path
_LEAF_SURFACE_FLOOR_DEG = 5.0  # mesh.py:636  — remnant of old floor-angle clamping
_LEAF_UPPER_FLATTEN    = 0.55  # mesh.py:640  — remnant of old upper-flatten pass
```

**Deprecated parameters still piped through the full call chain** despite having zero effect:

- `leaf_cap_count` (default 12): accepted by `build_branch_mesh` → passed to
  `_build_foliage_cluster_mesh` → never used. The spec marks it deprecated since the
  meridian-arc algorithm eliminates the apex cap.
- `leaf_base_count` (default 5): same — accepted, passed, ignored.

The parameters are set to `0` in `_CLUSTER` dict in the test script, but the defaults in
`build_branch_mesh` are still `12` and `5`, and a caller relying on the default would expect
them to do something.

**Dead stat field:** `LeafPlacementStats.skipped_ca` is defined with the comment
"contact angle ≥ π/2 (geometry impossible)" but is never incremented by `place_leaves_on_mesh`.
Both implementations now set `ca = 0.0` in that case rather than skipping; the counter will
always read 0 and the artifact-detection report never surfaces it.

**Recommendation:** Delete the three constants, remove `leaf_cap_count` and `leaf_base_count`
from `build_branch_mesh` and `_build_foliage_cluster_mesh` (and from the config/layer
parameters if applicable). Either increment `skipped_ca` when ca is clamped to 0, or remove
the field and rename the ca=0 path in a comment.

---

### 3. Spec pseudocode has two drifted claims vs actual code

Both divergences are *correct engineering choices* but the spec doc is now wrong in two places,
which makes it an unreliable reference for future work.

**Divergence A — `local_r` computation (`leaf-placement.md` line 412):**

The spec pseudocode reads:
```python
local_r = dist(pt3d, centroid_3d[:3])   # same as current
```
Both implementations compute `local_r = dist(pt3d, mesh_centroid_3d)` — the **3D centroid of
the whole mesh**, not the cross-section centroid at the current z_row. `mesh.py:1444-1449` has
a comment explaining why (using the cross-section centroid drives `local_r → 0` at the apex,
pushing `ca → π/2` and making apex leaves stand vertically). The spec pseudocode comment
"same as current" is therefore wrong; it should say "3D mesh centroid" and explain the reason.

**Divergence B — `ca ≥ π/2` guard (`leaf-placement.md`, Contact Angle / Guard section):**

The spec says: *"If `contact_angle >= π/2`, the leaf tangent points into the cluster surface.
Such leaves are skipped."*

Both implementations use `ca = 0.0` instead of skipping, and `mesh.py:1379-1385` explains why:
`ca = 0` gives a flat leaf that lies naturally on the near-horizontal apex surface, which is
correct behaviour. The spec text says "skip" but the code (rightly) does not skip.

**Recommendation:** Update `docs/design/leaf-placement.md`:
- Change the pseudocode `local_r` line to use `mesh_centroid_3d`, add a parenthetical explaining
  why the cross-section centroid is wrong.
- Change the guard description from "Such leaves are skipped" to "Such leaves are placed flat
  (`ca = 0`); the near-horizontal apex surface accepts flat leaves naturally."

---

## Minor Observations

- **Double normalization in `_emit_leaf`:** `_interpolate_meridian_normal` already returns a
  unit vector; `_emit_leaf` then calls `_safe_norm(radial)` on it again. Harmless but redundant.

- **`n_col` inflation block is copy-pasted** (the `_r_ring`, `_r_mid`, `_eff_perim_est` block)
  between `mesh.py:1508-1517` and `placement.py:282-286`. If the formula changes, both need
  updating.

- **The ca proxy in `n_col` inflation uses a global estimate**: `placement.py` pre-computes
  `_ca_ncol` from the mesh bounding radius; `mesh.py` uses `r_tip`. Both are approximations for
  inflating the perimeter to the effective midpoint ring. A small inaccuracy here causes
  `n_col` to be slightly off at strongly tilted rows, but the effect is minor.

- **`ca = 0.0` at the apex silently suppresses the `upward-pointing leaf` RuntimeWarning**
  (`mesh.py:1397-1406`). Flat leaves (`ca = 0`) with an upward-pointing `up_hint` will have
  `tangent ≈ T0` and `tangent[2]` near 0, so the `tangent[2] > 0.707` check won't fire.
  This is correct — the check was designed for the non-zero-ca case — but worth noting.

- **Jitter is still disabled** (open item #1 in the spec). `jit = 0.0` / `pj = 0.0` are
  hard-coded in `mesh.py:1277-1278`. Not a bug, but it means all leaves in a row are laid
  out in perfectly uniform yaw, which looks mechanical on close inspection.
