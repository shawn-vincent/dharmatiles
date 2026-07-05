"""Watertightness and export guarantees shared by every stone family.

The recurring failure mode: a mesh that is manifold in float64 memory
stops being manifold after STL export, because STL stores float32 and
readers merge vertices by POSITION.  These helpers are the pipeline's
three answers — clip through a real boolean (never a plane cap),
verify a float32 round-trip, and separate coincident-but-unconnected
vertex pairs after a union.
"""
from __future__ import annotations

import warnings

import numpy as np
import trimesh


def clip_to_box(mesh: trimesh.Trimesh, box: trimesh.Trimesh,
                label: str) -> trimesh.Trimesh:
    """Manifold-intersect *mesh* with *box*; on failure warn and return
    the mesh unclipped (a proud mesh beats an open one)."""
    clipped = trimesh.boolean.intersection([mesh, box], engine='manifold')
    if len(clipped.faces) > 0 and clipped.is_watertight:
        return clipped
    warnings.warn(f'{label}: box clip failed; left unclipped',
                  RuntimeWarning)
    return mesh


def survives_stl32(mesh: trimesh.Trimesh) -> bool:
    """True if the mesh is still watertight after the float32 vertex
    quantization + position-merge round-trip that STL export/reload
    performs (sliver triangles can collapse and open the mesh even
    though the float64 result is watertight)."""
    v32 = np.asarray(mesh.vertices, dtype=np.float32).astype(np.float64)
    chk = trimesh.Trimesh(vertices=v32,
                          faces=np.asarray(mesh.faces).copy(),
                          process=True)
    return bool(chk.is_watertight)


def separate_pinches(mesh: trimesh.Trimesh) -> None:
    """Nudge apart vertex pairs that coincide WITHOUT being
    topologically connected.  Zero-gap stones + texture displacement
    leave near-zero clearances somewhere every build; the mesh stays
    index-manifold, but STL export merges vertices by POSITION on
    reload, collapsing such a pinch into a non-manifold edge
    (fieldstone E23).  The nudge is ASYMMETRIC (0.15 µm vs 0.30 µm
    inward along each vertex's own normal) so a pair separates by
    ≥0.15 µm even when the two normals are parallel — well past the
    float32 grid (~0.004 µm at these coordinates).  Iterates in case
    a nudge lands a vertex onto a third one (fieldstone E25)."""
    from scipy.spatial import cKDTree
    adjacent = {tuple(e) for e in np.sort(mesh.edges_unique, axis=1)}
    vn = np.asarray(mesh.vertex_normals)
    v = mesh.vertices.view(np.ndarray)
    for _ in range(4):
        pairs = cKDTree(v).query_pairs(2e-4, output_type='ndarray')
        moved = False
        for i, j in pairs:
            if (min(i, j), max(i, j)) in adjacent:
                continue
            v[i] -= vn[i] * 1.5e-4
            v[j] -= vn[j] * 3.0e-4
            moved = True
        if not moved:
            break
    mesh.vertices = v
