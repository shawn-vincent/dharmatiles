"""Bezier-tube mesh builder for CloudTree skeletons."""
from __future__ import annotations

import numpy as np
import trimesh


def build_cloud_tree_mesh(
    nodes:    np.ndarray,      # (N, 3) — simplified: root + branch pts + attractors
    parents:  np.ndarray,      # (N,) int, -1 for root
    radii:    np.ndarray,      # (N,) — computed bottom-up; radii[0] is root radius
    in_dirs:  np.ndarray,      # (N, 3) — tangent *arriving* at each node
    out_dirs: np.ndarray,      # (N, 3) — tangent *leaving* parent toward this node
    *,
    terrain_z: float,
    handle_scale: float = 0.45,
    debug_attractors: np.ndarray | None = None,
    attractor_radius_mm: float = 0.6,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh]]:
    """Build a tapered cubic-Bezier tube mesh from a simplified CloudTree skeleton.

    Each (parent → child) edge is rendered as one cubic Bézier whose:
    - start tangent = out_dirs[child]  (outgoing from parent toward this child)
    - end   tangent = in_dirs[child]   (arriving at the child node)

    The number of tube segments along each edge is adaptive (~1 per 2.5 mm) so
    long branches get smooth curves without over-sampling short twigs.
    """
    meshes: list[trimesh.Trimesh] = []

    for i, p in enumerate(parents):
        if p < 0:
            continue
        p0 = nodes[int(p)]
        p3 = nodes[i]
        length = float(np.linalg.norm(p3 - p0))
        if length < 1e-8:
            continue
        r0 = max(float(radii[int(p)]), 0.42)
        r1 = max(float(radii[i]), 0.42)

        # Outgoing tangent from parent, arriving tangent at child.
        t0 = out_dirs[i]
        t1 = in_dirs[i]
        h  = handle_scale * length
        p1 = p0 + h * t0
        p2 = p3 - h * t1

        # Adaptive samples: ~one tube section per 2.5 mm, minimum 4.
        n_samples = max(4, int(np.ceil(length / 2.5)))
        ts        = np.linspace(0.0, 1.0, n_samples + 1)
        curve     = _bezier_eval(p0, p1, p2, p3, ts)
        radii_t   = r0 + (r1 - r0) * ts

        for j in range(n_samples):
            m = _tapered_tube(curve[j], curve[j + 1], radii_t[j], radii_t[j + 1])
            if m is not None and len(m.vertices) > 0:
                meshes.append(m)

    if not meshes:
        return trimesh.Trimesh(process=False), []
    mesh = trimesh.util.concatenate(meshes)
    for method in ("remove_duplicate_faces", "remove_degenerate_faces",
                   "remove_unreferenced_vertices"):
        fn = getattr(mesh, method, None)
        if fn is not None:
            fn()

    # Debug attractor spheres returned separately so the caller can tag them
    # independently (FLOWER/yellow) without being overwritten by the WOOD tag
    # applied to the trunk/branch mesh.
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
    t = ts[:, None]
    return (
        (1 - t) ** 3 * p0
        + 3 * (1 - t) ** 2 * t * p1
        + 3 * (1 - t) * t ** 2 * p2
        + t ** 3 * p3
    )


def _tapered_tube(
    p0: np.ndarray, p1: np.ndarray, r0: float, r1: float,
) -> trimesh.Trimesh | None:
    axis = p1 - p0
    length = float(np.linalg.norm(axis))
    if length < 1e-8:
        return None
    sections = _sections(max(r0, r1))
    w = axis / length
    u, v = _basis(w)
    theta = np.linspace(0.0, 2.0 * np.pi, sections, endpoint=False)
    circle = np.cos(theta)[:, None] * u + np.sin(theta)[:, None] * v
    ring0 = p0 + r0 * circle
    ring1 = p1 + r1 * circle
    verts = np.vstack([ring0, ring1, p0[None, :], p1[None, :]])
    c0, c1 = 2 * sections, 2 * sections + 1
    faces: list[list[int]] = []
    for a in range(sections):
        b = (a + 1) % sections
        faces += [
            [a, b, sections + b], [a, sections + b, sections + a],
            [c0, b, a],
            [c1, sections + a, sections + b],
        ]
    return trimesh.Trimesh(vertices=verts, faces=np.array(faces, dtype=int), process=False)


def _sections(radius: float) -> int:
    if radius >= 1.2:
        return 14
    if radius >= 0.7:
        return 12
    return 10


def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= np.linalg.norm(u) + 1e-12
    v = np.cross(w, u)
    v /= np.linalg.norm(v) + 1e-12
    return u, v
