"""FDM wall-printability colouring for debug leaf scripts.

Kept separate from ``dharmatiles.trees.leaf`` so production imports of
``leaf.py`` do not pull in trimesh ray-casting analysis paths.

Import in debug scripts as::

    from _leaf_debug import color_leaf_walls_by_fdm
"""
from __future__ import annotations

import numpy as np
import trimesh

from dharmatiles.trees.leaf import (
    _LEAF_FDM_FLOOR_DEG,
    _LEAF_FDM_SUPPORT_TOLERANCE_MM,
)


def color_leaf_walls_by_fdm(
    mesh:         trimesh.Trimesh,
    wall_faces:   range,
    support_mesh: trimesh.Trimesh,
    *,
    floor_angle_deg:      float = _LEAF_FDM_FLOOR_DEG,
    support_tolerance_mm: float = _LEAF_FDM_SUPPORT_TOLERANCE_MM,
    color_ok:    np.ndarray = np.array([ 50, 200,  50, 255], dtype=np.uint8),
    color_fail:  np.ndarray = np.array([220,  50,  50, 255], dtype=np.uint8),
) -> None:
    """Color wall faces green/red by FDM printability, in-place.

    Two failure conditions are checked for each wall face:

    1. **Angle** — face normal's Z component is below ``-sin(floor_angle_deg)``.
    2. **Support** — the face's lowest vertex is not inside ``support_mesh``,
       not within ``support_tolerance_mm`` of it, and has no geometry below it
       (downward ray miss).

    Printability propagates upward across shared wall edges from directly
    supported faces, so a face whose angle is fine but which floats in an
    unsupported island remains red.

    Surface and cap faces are not touched.

    Parameters
    ----------
    mesh                 : Solid returned by :func:`~dharmatiles.trees.leaf.solidify_leaf`.
    wall_faces           : Range of wall face indices from ``solidify_leaf``.
    support_mesh         : Mesh the leaf rests on (sphere + trunk, etc.).
    floor_angle_deg      : Printability floor angle (degrees from horizontal).
    support_tolerance_mm : Distance tolerance for on-surface detection.
    color_ok             : RGBA colour for printable faces (default green).
    color_fail           : RGBA colour for overhang faces (default red).
    """
    threshold = -np.sin(np.radians(floor_angle_deg))
    wall_idx  = np.array(list(wall_faces), dtype=np.intp)
    if len(wall_idx) == 0:
        return

    wall_nz  = mesh.face_normals[wall_idx, 2]
    angle_ok = wall_nz >= threshold

    face_verts  = mesh.vertices[mesh.faces[wall_idx]]              # (N, 3, 3)
    lowest_vi   = face_verts[:, :, 2].argmin(axis=1)
    lowest_pts  = face_verts[np.arange(len(wall_idx)), lowest_vi]  # (N, 3)

    inside = support_mesh.contains(lowest_pts)
    _, surf_dist, _ = support_mesh.nearest.on_surface(lowest_pts)
    inside_or_on = inside | (surf_dist <= support_tolerance_mm)

    has_support = inside_or_on.copy()
    outside_idx = np.where(~inside_or_on)[0]
    if len(outside_idx):
        ray_origins = lowest_pts[outside_idx] - np.array([0.0, 0.0, 1e-3])
        ray_dirs    = np.tile([0.0, 0.0, -1.0], (len(outside_idx), 1))
        has_support[outside_idx] = support_mesh.ray.intersects_any(
            ray_origins, ray_dirs,
        )

    # Propagate printability upward through angle-OK wall edges.
    face_min_z: np.ndarray = face_verts[:, :, 2].min(axis=1)
    edge_to_faces: dict[tuple[int, int], list[int]] = {}
    for local_i, face in enumerate(mesh.faces[wall_idx]):
        for a, b in ((face[0], face[1]), (face[1], face[2]), (face[2], face[0])):
            edge_to_faces.setdefault(tuple(sorted((int(a), int(b)))), []).append(local_i)

    neighbors: list[set[int]] = [set() for _ in wall_idx]
    for incident in edge_to_faces.values():
        for fi in incident:
            neighbors[fi].update(j for j in incident if j != fi)

    printable = has_support.copy()
    changed = True
    while changed:
        changed = False
        for fi in np.argsort(face_min_z):
            if printable[fi] or not angle_ok[fi]:
                continue
            if any(printable[nb] and face_min_z[nb] <= face_min_z[fi] + 1e-6
                   for nb in neighbors[fi]):
                printable[fi] = True
                changed = True

    mesh.visual.face_colors[wall_idx[ printable]] = color_ok
    mesh.visual.face_colors[wall_idx[~printable]] = color_fail
