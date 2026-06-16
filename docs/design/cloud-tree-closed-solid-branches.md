# Design: CloudTree Closed Solid Branches

## Goal

Generate tree branch geometry that is valid for slicers by construction:

- each branch edge is a closed swept solid,
- branch solids overlap volumetrically at joints,
- a boolean union removes all internal caps and coincident surfaces,
- the final tree mesh is a single watertight wood volume.

This replaces the previous assumption that branch-joint repair can be left to
the slicer.

## Problem

The current CloudTree mesh path builds all branches into one flat triangle array.
At a fork, each child starts from a copied version of the parent's end ring.
Those rings occupy the same world coordinates, but they are separate vertices
and separate surface sheets.

That creates ambiguous branch joints:

- coincident faces or nearly coincident faces can appear around forks,
- internal open boundaries can remain at multi-child nodes,
- slicers may interpret the joint as non-manifold or produce unstable toolpaths.

The visual mesh may look acceptable, but it is not a robust solid model.

## Design Rule

Every skeleton edge `parent -> child` is meshed as its own closed volume.

The edge volume is allowed, and expected, to overlap the parent volume slightly.
The overlap must be volumetric. Merely touching at the same ring or plane is not
enough.

After all edge solids are generated, union them with `manifold3d`.

## Branch Overlap

For a non-root edge, move the child branch's geometric start point slightly
backward along the direction of the parent branch:

```text
overlap_mm = max(0.15, 0.25 * start_radius)
start      = fork_node - parent_tangent * overlap_mm
end        = child_node
```

`parent_tangent` is the tangent of the branch as it arrives at `fork_node`. In
the current skeleton data this is `in_dirs[parent]`, normalized.

The branch radius profile should not be shrunk at the joint. Use the same start
radius the child would have used at the fork. Shrinking the radius risks visible
necks and can turn the intended intersection into a tangent contact.

Root edges do not need backward overlap at the terrain root unless there is a
separate trunk/base solid to union with. They still need a closed start cap.

## Closed Edge Solid

For each edge:

1. Build the cubic Bezier path from the possibly-overlapped start point to the
   child node.
2. Sweep circular rings along the path using the existing Bishop frame transport.
3. Connect adjacent rings with quad strips.
4. Add a cap at the start ring.
5. Add a cap at the end ring, unless the edge uses a dome tip.
6. For leaf tips, use the existing rounded dome as the distal closure.

Internal caps are acceptable before the boolean union. They are deliberately
inside overlapping solids, so the union removes them from the final exterior.

## Continuing Branches

At a fork where the base branch continues and a side branch diverges, no special
join shape is required.

Each outgoing edge is still built as a closed solid. Each non-root outgoing edge
starts slightly behind the fork node, inside the incoming parent branch. The
continuing branch therefore overlaps the previous segment, and the diverging
branch also penetrates the previous segment before curving away.

Because both children have real volume inside the parent, the boolean kernel has
a proper intersection curve to resolve. There should be no gap or sliver as long
as the overlap is larger than numerical tolerance and smaller than the local
branch length.

## Boolean Union

Collect all closed branch solids for one tree and union them:

```text
tree_mesh = boolean_union(edge_solids, engine="manifold")
```

The project already depends on `manifold3d`, and other mesh paths already use
the `trimesh` manifold engine. The tree builder should perform this union
internally so the material grouping phase can still treat each tree as one wood
mesh.

The later terrain material grouping may continue to skip union for `WOOD` if
each individual tree has already been unioned internally.

## Optional Junction Stabilizer

A separate sphere or ellipsoid at each branch node is not part of the primary
design. It changes the branch silhouette and should not be necessary when edge
solids overlap correctly.

However, it is a reasonable fallback if boolean reliability is poor at dense
forks:

```text
node_radius = 1.02 * radii[node]
```

Use node stabilizers only when needed by test results, not as the default join
model.

## Edge Cases

Clamp overlap for short edges:

```text
overlap_mm <= 0.35 * edge_length
```

If the edge is too short to support a useful overlap, either skip the synthetic
edge during skeleton simplification or use a smaller overlap. Avoid creating
zero-length or reversed Bezier handles.

For very small terminal branches, the minimum printable radius still applies.
The boolean-union design fixes topology; it does not make sub-nozzle features
printable.

## Validation

A generated tree mesh should satisfy:

- `mesh.is_watertight` is true,
- `mesh.is_volume` is true,
- no duplicate coincident faces remain at branch joints,
- slicer preview shows one continuous wood body at every fork,
- final face count remains acceptable for tile generation.

Tests should include:

- a straight trunk with one continuing branch and one diverging branch,
- a three-child fork,
- a dense generated CloudTree using the normal tile parameters,
- a leaf-clump tree where distal dome caps replace flat end caps.

## Implementation Notes

The current `build_cloud_tree_mesh` can be refactored by extracting an
edge-solid builder:

```text
build_closed_edge_solid(parent, child, start_point, end_point, radius_profile)
```

That helper should return a `trimesh.Trimesh` with caps and consistent normals.
The existing Bezier evaluation, tangent evaluation, ring generation, and frame
transport helpers can be reused.

Once edge solids are closed, the final flat vertex accumulator in the tree
builder becomes an intermediate detail of each edge, not the final tree assembly
strategy.
