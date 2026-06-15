"""Mesh construction for printable envelope trees."""
from __future__ import annotations

import numpy as np
import trimesh


def build_tree_mesh(
    nodes: np.ndarray,
    parents: np.ndarray,
    radii: np.ndarray,
    *,
    terrain_z: float,
    trunk_height_mm: float,
) -> trimesh.Trimesh:
    """Build capped tapered tube segments plus a root flare."""
    meshes: list[trimesh.Trimesh] = []
    for i, p in enumerate(parents):
        if p < 0:
            continue
        p0 = nodes[int(p)]
        p1 = nodes[i]
        length = float(np.linalg.norm(p1 - p0))
        if length <= 1e-8:
            continue
        r0 = max(float(radii[int(p)]), 0.42)
        r1 = max(float(radii[i]), 0.42)
        # Slight overlap helps slicers treat adjacent capped segments as one mass.
        d = (p1 - p0) / length
        q0 = p0 - d * min(0.08, length * 0.1)
        q1 = p1 + d * min(0.08, length * 0.1)
        meshes.append(_tapered_tube(q0, q1, r0, r1, sections=_sections(max(r0, r1))))

    if len(nodes) > 1:
        flare_h = min(0.30 * max(trunk_height_mm, 0.0), 4.0)
        if flare_h > 0.2:
            root = nodes[0].copy()
            top = root.copy()
            root[2] = terrain_z - 0.2
            top[2] = terrain_z + flare_h
            meshes.append(_tapered_tube(root, top, max(radii[0] * 1.35, 1.4), radii[0], sections=16))

    if not meshes:
        return trimesh.Trimesh(process=False)
    mesh = trimesh.util.concatenate(meshes)
    for method in ("remove_duplicate_faces", "remove_degenerate_faces", "remove_unreferenced_vertices"):
        cleanup = getattr(mesh, method, None)
        if cleanup is not None:
            cleanup()
    return mesh


def _sections(radius: float) -> int:
    if radius >= 1.2:
        return 14
    if radius >= 0.7:
        return 12
    return 10


def _tapered_tube(p0: np.ndarray, p1: np.ndarray, r0: float, r1: float, *, sections: int) -> trimesh.Trimesh:
    axis = p1 - p0
    length = float(np.linalg.norm(axis))
    if length <= 1e-8:
        return trimesh.Trimesh(process=False)
    w = axis / length
    u, v = _basis(w)
    theta = np.linspace(0.0, 2.0 * np.pi, sections, endpoint=False)
    circle = np.cos(theta)[:, None] * u + np.sin(theta)[:, None] * v
    ring0 = p0 + r0 * circle
    ring1 = p1 + r1 * circle
    verts = np.vstack([ring0, ring1, p0[None, :], p1[None, :]])
    c0 = 2 * sections
    c1 = c0 + 1
    faces: list[list[int]] = []
    for a in range(sections):
        b = (a + 1) % sections
        faces.append([a, b, sections + b])
        faces.append([a, sections + b, sections + a])
        faces.append([c0, b, a])
        faces.append([c1, sections + a, sections + b])
    return trimesh.Trimesh(vertices=verts, faces=np.array(faces, dtype=int), process=False)


def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= np.linalg.norm(u) + 1e-12
    v = np.cross(w, u)
    v /= np.linalg.norm(v) + 1e-12
    return u, v
