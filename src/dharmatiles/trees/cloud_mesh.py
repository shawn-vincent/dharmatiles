"""Bezier-tube mesh builder for CloudTree skeletons.

Each (parent → child) edge is a curved tube whose cross-section rings are
parallel-transported along the Bézier path (Bishop frame), giving smooth,
twist-free curves.  The base ring of every child tube *reuses* the vertex
ring already written for its parent, so branch-point vertices are shared and
the mesh is topologically connected.  At junctions with more than one child
the shared ring is referenced by two or more child quad-strips; face overlaps
at those joints are intentional and harmless for 3D printing.
"""
from __future__ import annotations

import numpy as np
import trimesh

# Fixed polygon count for every cross-section ring.
# A single value is mandatory: child base rings ARE parent tip rings, so all
# rings must have exactly the same vertex count.
_N_SIDES = 12


def build_cloud_tree_mesh(
    nodes:    np.ndarray,      # (N, 3) — root + branch pts + attractors
    parents:  np.ndarray,      # (N,) int; -1 for root
    radii:    np.ndarray,      # (N,) — bottom-up pipe-model radii
    in_dirs:  np.ndarray,      # (N, 3) — tangent *arriving* at each node
    out_dirs: np.ndarray,      # (N, 3) — tangent *leaving* parent toward node
    *,
    terrain_z: float,
    handle_scale: float = 0.45,
    debug_attractors: np.ndarray | None = None,
    attractor_radius_mm: float = 0.6,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh]]:
    """Build a connected, curved Bézier-tube mesh from a simplified skeleton.

    Algorithm
    ---------
    1. Seed the root node with a consistent frame ``(u, v) ⊥ in_dirs[0]``
       via :func:`_basis`.
    2. For each edge (parent → child) in topological order:

       a. Bézier start tangent = ``in_dirs[parent]`` (parent's arriving
          direction), end tangent = ``in_dirs[child]``.  This gives C1
          continuity at every junction: every child starts growing in exactly
          the same direction the parent was already travelling, then curves to
          its own target.  No pre-transport is needed because the parent's
          ``(u, v)`` frame is already ⊥ to ``in_dirs[parent]``.
       b. Sample the cubic Bézier at adaptive spacing (~2.5 mm / step, min 4)
          and parallel-transport the frame at each step.
       c. **Ring 0 is the parent's existing ring** (no new vertices added).
          Rings 1…n_steps are new.
       d. Connect consecutive ring pairs with a quad strip.
       e. Store the child's transported frame and ring offset for later edges.

    3. Close the root bottom and every leaf tip with fan-triangulated caps.
    """
    n = len(nodes)

    # ── flat vertex / face accumulators ────────────────────────────────────
    verts_acc: list[np.ndarray] = []  # (k, 3) arrays → stacked at the end
    faces_acc: list[list[int]]  = []
    n_verts    = 0

    def _add_verts(arr: np.ndarray) -> int:
        """Append *arr* and return its starting vertex index."""
        nonlocal n_verts
        off = n_verts
        verts_acc.append(np.asarray(arr, float))
        n_verts += len(arr)
        return off

    # ── per-node state ─────────────────────────────────────────────────────
    node_frame: list[tuple[np.ndarray, np.ndarray] | None] = [None] * n
    ring_off:   list[int]                                   = [-1]   * n
    is_leaf     = np.ones(n, dtype=bool)   # cleared when a child is processed

    # ── root ──────────────────────────────────────────────────────────────
    root_in       = _safe_norm(np.asarray(in_dirs[0], float))
    u0, v0        = _basis(root_in)
    node_frame[0] = (u0, v0)
    ring_off[0]   = _add_verts(_make_ring(nodes[0], float(radii[0]), u0, v0))

    # ── edges (topological order guarantees parent < child) ────────────────
    for i in range(1, n):
        p  = int(parents[i])
        is_leaf[p] = False

        p0  = np.asarray(nodes[p], float)
        p3  = np.asarray(nodes[i], float)
        length = float(np.linalg.norm(p3 - p0))
        pu, pv = node_frame[p]

        if length < 1e-8:
            # Degenerate edge: copy parent frame, register child ring.
            node_frame[i] = (pu, pv)
            ring_off[i]   = _add_verts(_make_ring(nodes[i], float(radii[i]), pu, pv))
            continue

        r0 = max(float(radii[p]), 0.42)
        r1 = max(float(radii[i]), 0.42)

        # Start tangent = parent's *arriving* direction so every child starts
        # tangent to the parent branch at the junction (C1 continuity).  The
        # Bézier then curves from that direction to in_dirs[child] at the tip.
        # No pre-transport is needed: the parent's (u, v) frame is already
        # perpendicular to in_dirs[parent], which is now also the start tangent.
        t0 = _safe_norm(np.asarray(in_dirs[p], float))   # parent's heading
        t1 = _safe_norm(np.asarray(in_dirs[i], float))   # child's arriving dir
        h  = handle_scale * length
        p1 = p0 + h * t0   # Bézier control points
        p2 = p3 - h * t1

        # Adaptive sampling: ~one ring per 2.5 mm, at least 4 steps.
        n_steps = max(4, int(np.ceil(length / 2.5)))
        ts      = np.linspace(0.0, 1.0, n_steps + 1)
        curve   = _bezier_eval(p0, p1, p2, p3, ts)
        radii_t = r0 + (r1 - r0) * ts

        u, v = pu, pv

        # step_off[0] = parent's ring (no new vertices); [1..] = new rings.
        step_off = [ring_off[p]]
        for j in range(1, n_steps + 1):
            tan  = _safe_norm(_bezier_tangent(p0, p1, p2, p3, ts[j]))
            u, v = _transport(u, v, tan)
            step_off.append(_add_verts(_make_ring(curve[j], radii_t[j], u, v)))

        node_frame[i] = (u, v)
        ring_off[i]   = step_off[-1]

        # Quad strip: two triangles per quad, outward CCW winding.
        for j in range(n_steps):
            oa, ob = step_off[j], step_off[j + 1]
            for k in range(_N_SIDES):
                k1 = (k + 1) % _N_SIDES
                faces_acc.append([oa + k, oa + k1, ob + k1])
                faces_acc.append([oa + k, ob + k1, ob + k])

    # ── end caps ──────────────────────────────────────────────────────────
    # Root bottom: outward normal points *down* (flip winding).
    c    = _add_verts(np.asarray(nodes[0], float)[np.newaxis])
    ro   = ring_off[0]
    for k in range(_N_SIDES):
        k1 = (k + 1) % _N_SIDES
        faces_acc.append([c, ro + k1, ro + k])

    # Leaf tips: outward normal points *away from trunk* (standard winding).
    is_leaf[0] = False  # root has its own cap above
    for i in range(n):
        if not is_leaf[i]:
            continue
        c  = _add_verts(np.asarray(nodes[i], float)[np.newaxis])
        ro = ring_off[i]
        for k in range(_N_SIDES):
            k1 = (k + 1) % _N_SIDES
            faces_acc.append([c, ro + k, ro + k1])

    # ── assemble ──────────────────────────────────────────────────────────
    if not faces_acc:
        return trimesh.Trimesh(process=False), []

    verts = np.vstack(verts_acc)
    faces = np.array(faces_acc, dtype=np.int32)
    mesh  = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    for method in ("remove_duplicate_faces", "remove_degenerate_faces",
                   "remove_unreferenced_vertices"):
        fn = getattr(mesh, method, None)
        if fn is not None:
            fn()

    # Debug attractor spheres — returned separately so caller can tag them
    # with Material.FLOWER independently of the WOOD trunk/branch mesh.
    attractor_meshes: list[trimesh.Trimesh] = []
    if debug_attractors is not None and len(debug_attractors) > 0:
        ico_base = trimesh.creation.icosphere(subdivisions=0, radius=attractor_radius_mm)
        for pt in debug_attractors:
            s = ico_base.copy()
            s.vertices = s.vertices + pt
            attractor_meshes.append(s)

    return mesh, attractor_meshes


# ─────────────────────────────────────────────────────────────────────────────
# Geometry helpers
# ─────────────────────────────────────────────────────────────────────────────

def _bezier_eval(
    p0: np.ndarray, p1: np.ndarray, p2: np.ndarray, p3: np.ndarray,
    ts: np.ndarray,
) -> np.ndarray:
    """Evaluate a cubic Bézier at each *t* in *ts*; returns (len(ts), 3)."""
    t = ts[:, None]
    return (
        (1 - t) ** 3 * p0
        + 3 * (1 - t) ** 2 * t * p1
        + 3 * (1 - t) * t ** 2 * p2
        + t ** 3 * p3
    )


def _bezier_tangent(
    p0: np.ndarray, p1: np.ndarray, p2: np.ndarray, p3: np.ndarray,
    t: float,
) -> np.ndarray:
    """First derivative of a cubic Bézier at scalar *t*."""
    return (
        3.0 * (1.0 - t) ** 2 * (p1 - p0)
        + 6.0 * (1.0 - t) * t * (p2 - p1)
        + 3.0 * t ** 2 * (p3 - p2)
    )


def _safe_norm(v: np.ndarray) -> np.ndarray:
    """Return *v* / |*v*|; return *v* unchanged if near-zero."""
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def _transport(
    u: np.ndarray, v: np.ndarray, new_t: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    """Parallel-transport frame *(u, v)* into the plane perpendicular to *new_t*.

    Projects *u* onto the plane perp to *new_t*, renormalises, then recomputes
    *v = cross(new_t, u)* so that ``(u, v, new_t)`` is a right-handed frame.
    Falls back to :func:`_basis` on near-180° tangent flips.
    """
    u_new = u - float(np.dot(u, new_t)) * new_t
    n     = float(np.linalg.norm(u_new))
    if n < 1e-10:
        return _basis(new_t)
    u_new /= n
    v_new  = np.cross(new_t, u_new)
    v_new /= float(np.linalg.norm(v_new)) + 1e-12
    return u_new, v_new


def _make_ring(
    center: np.ndarray, radius: float,
    u: np.ndarray, v: np.ndarray,
) -> np.ndarray:
    """Return a ``(_N_SIDES, 3)`` ring centred at *center* in the *(u, v)* plane."""
    theta  = np.linspace(0.0, 2.0 * np.pi, _N_SIDES, endpoint=False)
    circle = np.cos(theta)[:, None] * u + np.sin(theta)[:, None] * v
    return center + radius * circle


def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return two unit vectors *(u, v)* ⊥ to *w*, forming a right-handed frame."""
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= float(np.linalg.norm(u)) + 1e-12
    v = np.cross(w, u)
    v /= float(np.linalg.norm(v)) + 1e-12
    return u, v
