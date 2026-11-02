# CloudTree Mesh Builder — Design Specification

## Goal

Produce a single, topologically connected, watertight tube mesh for one tree.
No boolean union is needed or used — the mesh is built clean from the start.

---

## Conceptual model

The skeleton is a tree of directed edges (parent → child).  Each edge is
rendered as a tapered cubic Bézier tube: a sequence of cross-section rings
swept along the curve.

**Rings are shared at junctions.**  The ring that closes one edge's tube *is*
the same ring (same vertex indices) that opens each of its children's tubes.
There are no caps at junctions — the tube surface is continuous across forks.

Caps appear at exactly **two kinds of location**:

| Location | Cap direction |
|---|---|
| Root node (bottom of trunk) | Faces downward (away from tree) |
| Each leaf tip | Faces outward (away from tree) |

Everything between root and leaves is open tube — no interior faces anywhere.

---

## Foliage integration

A leaf branch is either:

**A. Pure foliage cone** (`leaf_clump_length_mm = None`, or branch length ≤ K)
- The cone starts from the **parent's ring** (shared vertex ring) and expands
  to `foliage_radius_mm` at the attractor tip.
- The cone is NOT a separate mesh — it is built as a continuation of the wood
  tube quad-strip, transitioning from a circular cross-section to a D-shaped
  (half-circle + flat chord) cross-section over the first ring or two.
- Cap: only at the leaf tip (rounded nose).
- No start cap — the parent ring is open.

**B. Wood stub + foliage cone** (branch length > K, `leaf_clump_length_mm = K`)
- A plain constant-radius tube is swept from the parent's ring to the split
  point `t_split` along the branch Bézier.
- At `t_split` the tube transitions to the D-shaped foliage cone, which
  continues to the attractor tip.
- The stub-to-cone transition is seamless: the ring at `t_split` is shared
  between the last stub ring and the first cone ring.  No cap or join face.
- Cap: only at the leaf tip.  No start cap, no cap at the split point.

In both cases **the foliage shape is part of the same vertex/face arrays** as
the wood skeleton — not a separate mesh.

---

## Algorithm

### Initialisation

```
For each node i, store:
  ring_off[i]   — vertex offset of this node's ring in the global vertex array
  ring_verts[i] — the (N_SIDES, 3) positions (for geometry queries / frame transport)
  node_frame[i] — Bishop (u, v) frame at this node
```

**Root node (i = 0):**

- Derive Bishop frame `(u0, v0)` from `in_dirs[0]`.
- Build ring at `nodes[0]` with radius `radii[0]`.
- Emit ring vertices → `ring_off[0]`.
- Emit **bottom cap** (fan from ring centre, faces downward).

### Edge loop (i = 1 … N-1, breadth-first or topological order)

For each edge `(parent p → child i)`:

1. **Retrieve the parent's ring.**
   `start_verts = ring_verts[p]`  (positions only — used for frame initialisation)
   `start_off   = ring_off[p]`    (vertex indices already in the global array)

   The child does **not** add the parent's ring to the vertex array again.
   It uses `start_off` directly as the base of its quad strip.

2. **Bezier curve.**
   Tangent at `p0`: `t0 = in_dirs[p]` (the tangent *arriving* at the parent node,
   i.e. the parent's outgoing direction).
   Tangent at `p3`: `t1 = in_dirs[i]`.
   Handles: `p1 = p0 + h·t0`, `p2 = p3 − h·t1`, where `h = handle_scale × length`.

3. **Sweep rings from step 1 … n_steps.**
   Ring 0 already exists at `start_off`.
   For each subsequent sample point along the curve, parallel-transport the
   Bishop frame and emit a new ring.

4. **Quad strip.**
   Connect ring j and ring j+1 with 2·N_SIDES triangles for j = 0 … n_steps−1.
   Ring 0's vertices come from `start_off` (no new vertices emitted for it).

5. **Store the child's ring.**
   `ring_off[i]   = last ring's offset`
   `ring_verts[i] = last ring's positions`
   `node_frame[i] = final Bishop frame`

6. **Leaf tip cap.**
   If node i is a leaf (in wood-only mode, or the last ring before foliage starts):
   Emit a fan cap facing outward from the final ring.

7. **NO start cap, NO end cap at internal nodes.**

### Foliage cone (leaf edges when `foliage_radius_mm > 0`)

Instead of step 6 above, the leaf edge is extended with a foliage shape:

**Case A — pure cone (no stub):**

- Ring 0 of the cone = `ring_off[p]` / `ring_verts[p]` (the parent's ring, shared).
- Tangent at cone start = `in_dirs[p]` (matches parent's outgoing direction).
- Sweep D-rings from step 1 … n_steps, expanding from `r_wood` to `r_cone_end`.
  Ring 0 is a full circle (matching the parent's circular ring); rings 1 … n are
  D-shaped (half-circle arc + flat chord).
- Emit a rounded nose cap at the attractor tip.
- No start cap.

**Case B — stub + cone:**

- Stub: sweep from `ring_off[p]` (shared, no new ring-0 vertices) along the
  branch Bézier to `t_split`.  Use `in_dirs[p]` as the tangent at `p0`.
  Store the stub's final ring as `stub_off` / `stub_ring`.
- Cone: ring 0 = `stub_off` / `stub_ring` (shared with stub's last ring, no
  new vertices).  Tangent at cone start = tangent of the Bézier at `t_split`.
  Sweep D-rings to the attractor tip, emit nose cap.
- No start cap, no cap at the split point.

---

## Invariants (must always hold)

1. **No ring is emitted twice.** The parent's ring is reused by all children;
   it never appears in the vertex array more than once.

2. **No cap at any junction.** Caps only at root bottom and leaf tips (including
   the nose of every foliage cone).

3. **Tangent continuity at junctions.** The Bézier handle at the start of every
   child edge uses `in_dirs[p]` — the same direction the parent was travelling
   when it arrived at node `p` — so C1 continuity is preserved across forks.

4. **Foliage ring-0 = parent ring.** A foliage shape always starts from the
   parent's ring (or stub's final ring), with no cap between them.

5. **Single mesh, single vertex array.** Wood skeleton + all foliage shapes are
   accumulated into one flat vertex/face array.  No boolean union is needed or
   applied.

---

## What this fixes

| Old behaviour | New behaviour |
|---|---|
| Each branch emits its own start cap (interior face at every junction) | No start caps at junctions |
| Each branch emits its own end cap (interior face at every junction) | End cap only at leaf tips |
| Stub has `cap_start=True, cap_end=True` (interior faces at both ends) | Stub shares rings with parent and cone — no extra caps |
| Foliage cone built as a separate mesh with its own start cap | Cone is a continuation of the wood quad strip — no separate mesh, no start cap |
| Ring vertices copied at every junction (duplicate positions) | Ring vertex indices reused — one ring per node in the global array |
| Boolean union required to clean up overlapping caps | Mesh is clean without any boolean union |
