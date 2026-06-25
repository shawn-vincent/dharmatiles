# Systematic Algorithm Development Protocol
*2026-06-24 — process note, not a bug report*

## Motivation

The foliage-cluster leaf-placement work (see `2026-06-24-foliage-cluster-baldness.md`)
demonstrates the canonical failure mode for geometry algorithm development:

- Six commits that each fix something and silently break or miss something else.
- Fixes that only get tested against one tile and one parameter set.
- Diagnostic code added for the immediate crisis, then removed (or not kept behind a flag).
- After every round, the devloper cannot be *certain* the fix is durable — only hopeful.

This document is a standing protocol to break that cycle.

---

## Core Principle

**Never fix an algorithm you haven't fully observed.**

The sequence is always:

1. **Define correctness** — write down what "correct" looks like, precisely, before touching code.
2. **Instrument** — add diagnostics that emit quantitative evidence of correctness or failure.
3. **Confirm you can reproduce failure** — run the diagnostics, see them fail.
4. **Fix** — change the minimum amount of algorithm logic.
5. **Confirm diagnostics pass** — across the full parameter matrix.
6. **Preserve diagnostics** — behind an env-var flag, never delete them.

---

## Step 0 — Define Correctness First

Before any code, write a bullet-point correctness specification:

```
SPEC: foliage cluster apex coverage
  - Every cluster must have at least one leaf whose center-point is
    within leaf_length_mm of the world-Z apex vertex.
  - No cluster may have a gap larger than 1.5 × row_step in Z
    between consecutive covered rows.
  - No leaf outward[2] < -0.15 (no downward-pointing leaves).
```

If you can't write this spec, you don't understand the algorithm yet.
Write the spec first — it forces clarity.

---

## Step 1 — Instrument Before Fixing

Add a `DHARMATILES_DEBUG_LEAVES=1` (or similar env-var) block that emits
structured per-cluster diagnostics:

```
[LEAF] cluster 17: z_range=[26.2, 40.0] wz_apex=40.0 branch_apex=35.4
[LEAF] cluster 17: rows placed=[0, 12, 13, 0, 0] (z=[39.67, 36.29, 32.92, 29.54, 26.17])
[LEAF] cluster 17: apex_cap_leaf_count=1 gap_above_last_row_mm=3.7
[LEAF] cluster 17: FAIL gap_above_last_row_mm=3.7 > 1.5*row_step=5.1  PASS
[LEAF] cluster 17: FAIL apex_cap_count=0  < 1  FAIL
```

Each diagnostic line must end with `PASS` or `FAIL` against the spec.

**Key rule:** commit the diagnostics in their own commit, *separate* from
the fix.  This lets you bisect to "instrumented but broken" vs. "fixed".

---

## Step 2 — Build a Parameter Matrix

The foliage problem only showed up clearly at certain branch angles.
One render is never enough.

Define a minimal matrix covering known edge cases:

| Parameter | Values to test |
|---|---|
| `canopy_radius_mm` | 8, 14, 20 |
| `foliage_cluster_length_mm` | 5, 10, None |
| `foliage_cluster_radius_mm` | 3.5, 5.5, 8.0 |
| `branch_target` | 0.2, 0.5, 0.8 |
| `n_attractors` | 50, 200 |

For a geometry bug, the matrix does not have to be exhaustive — cover
the extremes.  Aim for 8–16 combinations, not 3^5.

Store the matrix as a Python list of dicts in `src/scripts/leaf_matrix.py`
so it can be re-run at any time.

---

## Step 3 — Agent Loop for Iterative Development

Use Claude Code's `/loop` or a background agent with an explicit iteration
contract:

```
TASK: Fix foliage apex coverage.

ITERATION CONTRACT:
  SETUP: python src/scripts/leaf_matrix.py --diagnostics > /tmp/matrix.log
  CHECK: grep -c "FAIL" /tmp/matrix.log
  TARGET: 0 FAIL lines
  MAX_ITERS: 10

Each iteration:
  1. Read /tmp/matrix.log, identify failing clusters.
  2. Propose ONE code change to src/dharmatiles/trees/mesh.py.
  3. Apply the change.
  4. Re-run SETUP, re-check CHECK.
  5. If CHECK == 0, stop and summarize.
  6. If CHECK did not decrease from last iter, revert change and try a different approach.
```

The agent knows to stop when `FAIL` count reaches 0, and knows to backtrack
when a change makes things worse.  This is the fast loop.

**Important:** the iteration contract must be written down in the session
before the agent starts.  It is a contract, not a vibe.

---

## Step 4 — Preserve Diagnostics

After the fix passes the full matrix, the diagnostics do NOT get deleted.
They get gated:

```python
_DEBUG_LEAVES = os.getenv("DHARMATILES_DEBUG_LEAVES")

if _DEBUG_LEAVES:
    print(f"[LEAF] cluster {i}: z_range=[{z_lo:.1f}, {z_hi:.1f}] ...")
    if gap > 1.5 * row_step:
        print(f"[LEAF] cluster {i}: FAIL gap_above_last_row_mm={gap:.1f}")
    else:
        print(f"[LEAF] cluster {i}: PASS")
```

Add a note to CLAUDE.md (or a `docs/diagnostics.md` file) listing all
available env-var debug flags and what they emit.

---

## Step 5 — Regression Render Set

After each algorithm fix, render a small set of tiles and store the output:

```bash
mkdir -p docs/regression/YYYY-MM-DD
for spec in src/tiles/ground/2x2-grass-tree.tile.py src/tiles/water/1x1-grass-tree+water.tile.py; do
    dharmatiles-gen --tile "$spec"
done
# render thumbnails (pyrender, see docs/meta/history/2026-06-22-leaf-placement-debug-pipeline-review.md)
python src/scripts/render_thumbnails.py --outdir docs/regression/YYYY-MM-DD
```

The regression set becomes the visual ground truth.  In future sessions,
run it and diff the thumbnails before declaring a fix good.

---

## Checklist (copy-paste for each algorithm fix session)

```
[ ] Correctness spec written (what does PASS look like, precisely?)
[ ] Diagnostic instrumentation added (env-var gated, PASS/FAIL lines)
[ ] Can reproduce failure via: DHARMATILES_DEBUG_LEAVES=1 python ...
[ ] Parameter matrix defined (src/scripts/<algorithm>_matrix.py)
[ ] Agent loop iteration contract written (SETUP / CHECK / TARGET / MAX_ITERS)
[ ] Fix applied; all FAIL lines cleared across full matrix
[ ] Diagnostics committed (not deleted) behind env-var flag
[ ] Regression renders saved to docs/regression/YYYY-MM-DD/
[ ] History doc updated (docs/meta/history/YYYY-MM-DD-<name>.md)
```

---

## Case Study: What This Would Have Looked Like for Apex Coverage

Had this protocol been followed from commit `de6dca5`:

1. **Spec**: "every cluster has ≥1 leaf within `leaf_length_mm` of world-Z apex".
2. **Diagnostic**: per-cluster `gap_above_last_row_mm` and `apex_cap_count` printed.
3. **Matrix**: 16 combinations of radius, length, branch_target.
4. **Agent loop**: "reduce FAIL count to 0".  Agent would have discovered the
   `argmax(dot(tip_t))` vs. `argmax(z)` distinction in iteration 2 by seeing
   that tilted-branch clusters consistently failed while vertical ones passed.
5. **Result**: one clear commit that changes one line and passes the matrix.
   No six-commit archaeology needed.

Instead: six commits over multiple sessions, none fully verified.

---

## Anti-Patterns to Avoid

| Anti-pattern | Why it fails |
|---|---|
| "I'll verify it visually later" | Visual inspection from one angle misses gaps on other faces |
| "It looks good on this tile" | One tile with one parameter set is not a parameter matrix |
| "I deleted the debug print after fixing" | Next bug requires rediscovering everything |
| "I fixed the symptom I could see" | The check that passed may have fixed the wrong thing |
| "Fix, re-render, looks okay, ship" | The render was taken from a favorable angle |

---

## When to Apply This Protocol

- **Any** algorithm with a correctness criterion that isn't trivially visible
  in the output (e.g., leaf coverage, branch angles, watertight meshes).
- **Any** fix that has already failed once after an apparent success.
- **Any** algorithm that involves a threshold, a filter, or a geometric selection
  (these are the most parameter-sensitive).

Applies directly to: leaf placement, rock scattering, grass growth, skeleton
branching angle checks, water pool shaping, soil blob distribution.
