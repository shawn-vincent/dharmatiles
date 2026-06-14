# Tree Mesh Watertightness — Design Document

**Date**: 2026-06-14  
**Status**: Design only — not yet implemented  
**Affects**: `src/dharmatiles/trees/surface.py`

---

## Problem

Constructive trees (`ConstTree`) produce non-watertight STL meshes.
Confirmed by `mesh.is_watertight == False` on a default `ConstTreeConfig` tree.

There are two stacked root causes:

### Root cause 1 — Vertices are never merged (3360 open boundary edges)

Every `_side_strip(rings[p], rings[i])` call in `build_tree_mesh` builds its
own vertex array via `np.vstack([ring0, ring1])`.  Adjacent strips share ring
*positions* in 3D space but not vertex *indices*.  The final assembly:

```python
result = trimesh.util.concatenate(parts)   # stacks arrays, no dedup
result.fix_normals()                        # fixes winding, does NOT merge
```

…never deduplicates.  Trimesh's watertight check works by vertex index, so
two boundary edges at the same 3D location are still two open boundaries.

**After `mesh.merge_vertices()`**: all 3360 open boundary edges vanish —
the strip-to-strip seams are geometrically fine.

### Root cause 2 — Bifurcation rings are over-shared (144 non-manifold edges after merge)

The skeleton has 12 junction nodes with 2–3 children.  The surface builder
emits one full `_side_strip` per skeleton edge:

```python
for i in range(1, N):
    p = int(parents[i])
    parts.append(_side_strip(rings[p], rings[i]))   # full parent ring every time
```

At a junction node `p` with N active-ring children, N independent strips all
start from the **same full parent ring**.  After vertex merging, each of the
`az_segs` edges around that ring is shared by 2N faces instead of 2 →
non-manifold edge.

With `az_segs=16` and 9 junctions that have 2+ active-ring children:
`9 × 16 = 144 non-manifold edges`.

---

## Proposed fix — Sector lofting

Replace the flat "one full strip per edge" approach with **sector-aware
junction geometry**.  The idea:

- At chain nodes (1 active-ring child): keep the existing `_side_strip` — no
  change.
- At junction nodes (2+ active-ring children): partition the parent ring into
  N contiguous arc sectors, assign one sector to each child, then:
  1. Connect each sector arc → full child ring with a **fan loft**
  2. Close the gap between adjacent sectors with a **crotch cap**

No vertex merging is needed because all shared boundaries are built from
literally the same array slices.

### New functions

#### `_assign_sectors(active_children, nodes_xyz, ring_p, normal, binormal)`

For each active child of junction node `p`:
- Compute the child's direction vector `nodes_xyz[child] - nodes_xyz[p]`
- Project onto the ring plane (normal, binormal) → angle θ in [0, 2π)
- Sort children by θ
- Assign contiguous index ranges of `rings[p]` proportionally:
  - `k = az // N` vertices per child; last child gets `az - (N-1)*k` (handles
    non-even splits, e.g. az=16, N=3 → sectors of 5, 5, 6)
- Returns `list[tuple[int, int]]` — `(start, end)` index into `rings[p]`

#### `_fan_loft(arc_verts, ring_verts)`

Connects a k-vertex arc (open, the sector slice) to an az-vertex full ring.

Algorithm:
- For each ring column j (0..az-1), map it to an arc vertex: `a = j * k // az`
- Build a face between ring column j, ring column j+1, and arc vertices `a`
  and `a_next = (j+1)*k // az`:
  - If `a == a_next`: single triangle (degenerate quad — zero-area triangles
    are valid and don't create holes)
  - Otherwise: two triangles (proper quad)
- Winding: outward normals for a tube travelling from arc → ring

This means the arc "fans out" to the full ring as the branch grows.  The
mapping is proportional so that the midpoint of the arc maps to the midpoint
of the ring.

#### `_crotch_cap(node_center, arc_a_end, arc_b_start)`

Closes the gap between two adjacent sector arcs at the junction node.

- `arc_a_end`: the last vertex of sector A's arc (1D boundary point)
- `arc_b_start`: the first vertex of sector B's arc (1D boundary point)
- Emits a single triangle: `(node_center, arc_a_end, arc_b_start)` with
  outward winding

For N children there are N such boundary pairs (cyclically: sector N-1's end
→ sector 0's start also needs a cap).

### Modified `build_tree_mesh` loop

```
Identify active-ring children for every node.
For each node p:
  active = [c for c in children[p] if rings[c] is not None]
  if len(active) == 0:
      # leaf — handled by top caps below
  elif len(active) == 1:
      # chain — existing _side_strip
      parts.append(_side_strip(rings[p], rings[active[0]]))
  else:
      # junction — sector loft
      sectors = _assign_sectors(active, nodes_xyz, rings[p], normals[p], binormals[p])
      for child, (start, end) in zip(active_sorted_by_angle, sectors):
          arc = rings[p][start:end]   # contiguous slice, shape (k, 3)
          parts.append(_fan_loft(arc, rings[child]))
      # crotch caps between adjacent sectors
      for (_, end_a), (start_b, _) in zip(sectors, sectors[1:] + [sectors[0]]):
          parts.append(_crotch_cap(
              nodes_xyz[p],
              rings[p][end_a - 1],    # last vertex of sector A
              rings[p][start_b],      # first vertex of sector B
          ))
```

Root bottom cap and leaf top caps are unchanged.

### Expected result

| Metric | Before | After |
|---|---|---|
| Boundary (open) edges | 3360 | 0 |
| Non-manifold edges | 144 (after merge) | 0 |
| `mesh.is_watertight` | False | True |
| New functions | — | ~75 lines |
| Modified lines in `build_tree_mesh` | — | ~30 lines |

---

## What this does NOT fix

- **Bark ridge/wrinkle continuity across junctions**: the current bisector-frame
  approach already handles this reasonably well; the sector loft doesn't change
  how rings are built, only how adjacent rings are stitched.
- **Visual seam at junction base**: the fan loft produces a slight "pinch" at the
  base of each branch where the sector arc is narrower than a full ring.  This
  is physically plausible (branches taper into the trunk) and should look fine.
- **SCA trees**: SCA skeletons also use `build_tree_mesh` and have the same two
  root causes.  This fix applies equally to both skeleton types.

---

## Files to change

| File | Change |
|---|---|
| `src/dharmatiles/trees/surface.py` | Add `_assign_sectors`, `_fan_loft`, `_crotch_cap`; modify junction handling in `build_tree_mesh` |

No other files need to change.  The public API of `build_tree_mesh` is
unchanged.
