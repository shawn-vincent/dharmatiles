"""Bezier-tube mesh builder for CloudTree skeletons."""
from __future__ import annotations

import numpy as np
import trimesh


def build_cloud_tree_mesh(
    nodes: np.ndarray,       # (N, 3)
    parents: np.ndarray,     # (N,) int, -1 for root
    radii: np.ndarray,       # (N,)
    prior_dirs: np.ndarray,  # (N, 3) — Bezier tangent arriving at each node
    *,
    terrain_z: float,
    trunk_radius_mm: float,
    handle_scale: float = 0.45,
    bezier_samples: int = 8,
    debug_attractors: np.ndarray | None = None,
    attractor_radius_mm: float = 0.8,
) -> trimesh.Trimesh:
    """Build a tapered cubic-Bezier tube mesh from a CloudTree node graph.

    Each (parent, child) edge is rendered as a cubic Bezier whose start and
    end tangents are the nodes' prior_dirs, giving C1 continuity at branch
    joins (because child prior_dirs are initialised to the parent's heading).
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
        t0 = prior_dirs[int(p)]
        t1 = prior_dirs[i]
        h = handle_scale * length
        # Cubic Bezier control points.
        p1 = p0 + h * t0
        p2 = p3 - h * t1

        ts = np.linspace(0.0, 1.0, bezier_samples + 1)
        curve = _bezier_eval(p0, p1, p2, p3, ts)
        radii_t = r0 + (r1 - r0) * ts

        for j in range(bezier_samples):
            m = _tapered_tube(curve[j], curve[j + 1], radii_t[j], radii_t[j + 1])
            if m is not None and len(m.vertices) > 0:
                meshes.append(m)

    # Root flare anchors the trunk to the terrain surface.
    base_r = max(float(radii[0]) if len(radii) > 0 else trunk_radius_mm, 0.42)
    flare_h = min(0.30 * max(trunk_radius_mm, 0.0), 4.0)
    if flare_h > 0.2 and len(nodes) > 0:
        root = nodes[0].copy()
        top = nodes[0].copy()
        root[2] = terrain_z - 0.2
        top[2] = terrain_z + flare_h
        m = _tapered_tube(root, top, max(base_r * 1.35, 1.4), base_r)
        if m is not None:
            meshes.append(m)

    # Optional debug markers: one icosahedron per attractor point, golden yellow.
    if debug_attractors is not None and len(debug_attractors) > 0:
        ico_base = trimesh.creation.icosphere(subdivisions=0, radius=attractor_radius_mm)
        yellow = np.array([245, 195, 0, 255], dtype=np.uint8)
        for pt in debug_attractors:
            s = ico_base.copy()
            s.vertices = s.vertices + pt
            n_f = len(s.faces)
            s.visual = trimesh.visual.ColorVisuals(
                mesh=s,
                face_colors=np.tile(yellow, (n_f, 1)).astype(np.uint8),
            )
            s.metadata['material'] = 5  # FLOWER — golden yellow
            meshes.append(s)

    if not meshes:
        return trimesh.Trimesh(process=False)
    mesh = trimesh.util.concatenate(meshes)
    for method in ("remove_duplicate_faces", "remove_degenerate_faces",
                   "remove_unreferenced_vertices"):
        fn = getattr(mesh, method, None)
        if fn is not None:
            fn()
    return mesh


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
