#!/usr/bin/env python3
"""Simplified debug: sphere + trunk + four solid leaf solids.

Each leaf surface is closed into a solid by projecting its perimeter inward
along -normal by LEAF_ROOT_DEPTH_MM, forming a root ring that pierces the
parent mesh.  Walls connect the leaf perimeter to the root ring; a bottom cap
closes the root end.

Each leaf is placed with its base on the sphere surface and tilted as far down
as possible without any part of the leaf penetrating the sphere (plus a small
tolerance gap).

Leaf positions are always specified by their base point.  The base point is
chosen so that the flat leaf's midpoint lands at the canonical attachment
point on the sphere.  Tilt is derived entirely from that base point — the
base stays on the sphere and is always the rotation pivot.

Leaf normal definition
----------------------
The *leaf normal* is the normal to the plane in which the leaf lays flat:

    normal  =  cos(θ) · surface_normal_at_base  +  sin(θ) · L

where θ is the tilt angle and L is the longitudinal (world-up-projected) axis
in the tangent plane at the base.  Passed as ``up_hint`` to
``build_leaf_surface`` and returned for downstream use.

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-simplified-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import build_leaf_surface

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT   = pathlib.Path("stl/debug/leaf-placement-debug-simplified.stl")
SPHERE_RADIUS_MM = 12.0
TRUNK_HEIGHT_MM  = SPHERE_RADIUS_MM * 2.0 / 3.0
TRUNK_RADIUS_MM  = TRUNK_HEIGHT_MM / 2.0
LEAF_LENGTH_MM   = 6.0
LEAF_WIDTH_MM    = LEAF_LENGTH_MM * 2.0 / 3.0
LEAF_FOLD_DEG    = 6.0
LEAF_CURL_DEG    = 40.0

# Minimum clearance from the sphere surface for any non-base-zone leaf vertex.
TOLERANCE_GAP_MM = 0.0

# How far the leaf root extends inward along -normal to pierce the parent mesh.
LEAF_ROOT_DEPTH_MM = 1.0

# FDM overhang floor: faces whose normal's Z component is below -sin(angle)
# are unsupported overhangs (red); at or above are supported (green).
FLOOR_ANGLE_DEG = 45.0
# A root wall embedded in the parent is supported by the parent and does not
# need its angle corrected. Only the portion outside the parent counts toward
# this unsupported-span limit.
MAX_UNSUPPORTED_ROOT_WALL_MM = 0.0
# Contact tolerance for testing against the faceted support mesh. This is
# separate from TOLERANCE_GAP_MM: it compensates for an analytic sphere point
# lying slightly outside the inscribed triangles of a finite-resolution sphere.
SUPPORT_SURFACE_TOLERANCE_MM = 0.05

_COLOR_SUPPORTED = np.array([ 50, 200,  50, 255], dtype=np.uint8)   # green
_COLOR_OVERHANG  = np.array([220,  50,  50, 255], dtype=np.uint8)   # red

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Geometry helpers ───────────────────────────────────────────────────────────

def _leaf_frame(surface_normal: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf longitudinal and lateral axes in the tangent plane.

    L is world-up projected onto the tangent plane (falls back to +X at poles).
    T = cross(surface_normal, L).
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up   = np.array([0.0, 0.0, 1.0])
    proj = up - np.dot(up, n) * n
    if np.linalg.norm(proj) < 0.1:
        proj = np.array([1.0, 0.0, 0.0]) - np.dot([1.0, 0.0, 0.0], n) * n
    L = proj / (np.linalg.norm(proj) + 1e-12)
    T = np.cross(n, L);  T /= np.linalg.norm(T) + 1e-12
    return L, T


def _base_for_midpoint(
    midpoint:       np.ndarray,
    L:              np.ndarray,
    sphere_radius:  float,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Find the base point on the sphere whose flat-leaf midpoint ≈ ``midpoint``.

    At tilt=0 the leaf lays flat with tangent=-L, so:
        midpoint = base + (length/2) · (-L)  →  base = midpoint + (length/2) · L

    Project that candidate onto the sphere and recompute the surface frame
    at the true base position.

    Returns (base, surface_normal_at_base, L_at_base).
    """
    candidate    = midpoint + (LEAF_LENGTH_MM / 2.0) * L
    base_normal  = candidate / (np.linalg.norm(candidate) + 1e-12)
    base         = sphere_radius * base_normal
    L_base, _    = _leaf_frame(base_normal)
    return base, base_normal, L_base


def _boundary_loop(mesh: trimesh.Trimesh) -> list[int]:
    """Return perimeter vertex indices as an ordered loop for an open mesh."""
    # Boundary edges appear exactly once when all directed edges are made undirected.
    edges_sorted = np.sort(mesh.edges, axis=1)
    unique, counts = np.unique(edges_sorted, axis=0, return_counts=True)
    bnd_edges = unique[counts == 1]          # shape (NP, 2)

    # Build adjacency for the boundary graph and walk the loop.
    adj: dict[int, list[int]] = {}
    for a, b in bnd_edges.tolist():
        adj.setdefault(a, []).append(b)
        adj.setdefault(b, []).append(a)

    start = int(bnd_edges[0, 0])
    loop  = [start]
    prev, curr = -1, start
    while True:
        a, b  = adj[curr]
        nxt   = b if a == prev else a
        if nxt == start:
            break
        loop.append(nxt)
        prev, curr = curr, nxt
    return loop


def _segment_length_outside_sphere(
    start: np.ndarray,
    end: np.ndarray,
    radius: float,
) -> float:
    """Return the length of a line segment lying outside a centered sphere."""
    start = np.asarray(start, float)
    delta = np.asarray(end, float) - start
    length = float(np.linalg.norm(delta))
    if length < 1e-12:
        return 0.0

    # Split [0, 1] at every sphere intersection, then classify each interval
    # by its midpoint. This handles inside→outside, outside→inside, and chords.
    a = float(np.dot(delta, delta))
    b = 2.0 * float(np.dot(start, delta))
    c = float(np.dot(start, start) - radius * radius)
    discriminant = b * b - 4.0 * a * c
    cuts = [0.0, 1.0]
    if discriminant >= 0.0:
        sqrt_discriminant = float(np.sqrt(discriminant))
        for t in ((-b - sqrt_discriminant) / (2.0 * a),
                  (-b + sqrt_discriminant) / (2.0 * a)):
            if 0.0 < t < 1.0:
                cuts.append(float(t))
    cuts.sort()

    outside_fraction = 0.0
    for lo, hi in zip(cuts, cuts[1:]):
        midpoint = start + (0.5 * (lo + hi)) * delta
        if np.linalg.norm(midpoint) > radius + 1e-9:
            outside_fraction += hi - lo
    return outside_fraction * length


def _solidify_leaf(
    surface: trimesh.Trimesh,
    normal:  np.ndarray,
    depth:   float = LEAF_ROOT_DEPTH_MM,
    *,
    parent_sphere_radius: float | None = None,
    remove_tip_wall_faces: bool = False,
    debug_label: str | None = None,
) -> tuple[trimesh.Trimesh, range]:
    """Close an open leaf surface into a solid.

    For each boundary vertex of *surface*, create a *leaf_root* vertex
    offset by ``depth`` along ``-normal`` (into the parent mesh).  Then:

    * **Walls** — quads bridging each boundary edge to the corresponding
      root edge.
    * **Bottom cap** — a fan from the centroid of the root ring to close
      the buried end.

    The original surface faces are preserved as the top of the solid.
    ``fix_normals()`` is called on the result to ensure consistent winding.

    Returns ``(solid, wall_faces_range)`` where ``wall_faces_range`` is the
    slice of face indices that belong to the wall (edge) faces only.
    """
    n    = normal / (np.linalg.norm(normal) + 1e-12)
    loop = _boundary_loop(surface)
    NP   = len(loop)

    perim  = surface.vertices[loop]               # (NP, 3)  — leaf perimeter
    root   = perim - depth * n                    # (NP, 3)  — root ring

    # ── FDM tip fix: adjust only the tip root vertex ──────────────────────
    # The perimeter vertex with the lowest Z is the leaf tip — the first
    # thing printed.  If its root counterpart is above it (or not far enough
    # below), the edge root→perim overhangs more than FLOOR_ANGLE_DEG.
    # If the edge is also exposed outside the parent sphere beyond the allowed
    # unsupported span, push only that root vertex straight down until the edge
    # is exactly at the printability limit. Embedded wall is already supported.
    tip_i   = int(np.argmin(perim[:, 2]))
    root_before_tip_fix = root[tip_i].copy()
    horiz   = np.linalg.norm(root[tip_i, :2] - perim[tip_i, :2])
    max_z   = perim[tip_i, 2] - horiz / np.tan(np.radians(FLOOR_ANGLE_DEG))
    exposed_length = (
        _segment_length_outside_sphere(
            perim[tip_i],
            root_before_tip_fix,
            parent_sphere_radius,
        )
        if parent_sphere_radius is not None
        else np.linalg.norm(root_before_tip_fix - perim[tip_i])
    )
    angle_needs_fix = root[tip_i, 2] > max_z
    exposure_needs_fix = exposed_length > MAX_UNSUPPORTED_ROOT_WALL_MM + 1e-9
    if angle_needs_fix and exposure_needs_fix:
        root[tip_i, 2] = max_z

    if debug_label is not None:
        prev_i = (tip_i - 1) % NP
        next_i = (tip_i + 1) % NP
        print(f"    solidify[{debug_label}] perimeter vertices={NP}, lowest-z index={tip_i}")
        print(f"      perimeter tip       = {np.array2string(perim[tip_i], precision=4)}")
        print(f"      root before tip fix = {np.array2string(root_before_tip_fix, precision=4)}")
        print(f"      root after tip fix  = {np.array2string(root[tip_i], precision=4)}")
        print(f"      tip-root delta      = "
              f"{np.array2string(root[tip_i] - perim[tip_i], precision=4)}, "
              f"length={np.linalg.norm(root[tip_i] - perim[tip_i]):.4f} mm")
        print(f"      root displacement   = "
              f"{np.array2string(root[tip_i] - root_before_tip_fix, precision=4)}")
        print(f"      radii (tip/before/after) = "
              f"{np.linalg.norm(perim[tip_i]):.4f} / "
              f"{np.linalg.norm(root_before_tip_fix):.4f} / "
              f"{np.linalg.norm(root[tip_i]):.4f} mm")
        print(f"      FDM test            = root_before.z {root_before_tip_fix[2]:.4f} "
              f"> max_z {max_z:.4f} -> {angle_needs_fix}")
        print(f"      exposed root wall   = {exposed_length:.4f} mm "
              f"> allowed {MAX_UNSUPPORTED_ROOT_WALL_MM:.4f} mm -> "
              f"{exposure_needs_fix}")
        print(f"      adjacent roots      = prev "
              f"{np.array2string(root[prev_i], precision=4)}, next "
              f"{np.array2string(root[next_i], precision=4)}")
        print("      incident wall quads = "
              f"(perim[{prev_i}], perim[{tip_i}], root[{tip_i}], root[{prev_i}]) and "
              f"(perim[{tip_i}], perim[{next_i}], root[{next_i}], root[{tip_i}])")
    # ─────────────────────────────────────────────────────────────────────

    center = root.mean(axis=0)                    # (3,)     — bottom cap centre

    n_surf    = len(surface.vertices)
    root_base = n_surf                            # root[i] = root_base + i
    cap_ctr   = n_surf + NP                       # single centre vertex

    all_verts = np.vstack([surface.vertices, root, center[np.newaxis]])

    wall_faces: list[list[int]] = []
    for i in range(NP):
        j  = (i + 1) % NP
        if remove_tip_wall_faces and (i == tip_i or j == tip_i):
            continue
        a, b = loop[i], loop[j]                  # perimeter (top)
        d, c = root_base + i, root_base + j      # root (bottom), swapped for winding
        if i == tip_i:
            # Mirror the diagonal used by the quad on the other side of the
            # tip. A fixed a→c diagonal makes the two tip quads geometrically
            # asymmetric when the tip root differs from its neighbors.
            wall_faces += [[a, d, b], [b, d, c]]
        else:
            wall_faces += [[a, b, c], [a, c, d]]

    cap_faces: list[list[int]] = []
    for i in range(NP):
        j = (i + 1) % NP
        cap_faces.append([cap_ctr, root_base + j, root_base + i])

    wall_start = len(surface.faces)
    wall_end   = wall_start + len(wall_faces)

    all_faces = np.vstack([
        surface.faces,
        np.array(wall_faces, dtype=np.int32),
        np.array(cap_faces,  dtype=np.int32),
    ])

    solid = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    solid.fix_normals()
    if debug_label is not None and remove_tip_wall_faces:
        print("      removed tip walls   = 2 quads / 4 triangles")
    return solid, range(wall_start, wall_end)


def _color_wall_faces_by_fdm(
    mesh:         trimesh.Trimesh,
    wall_faces:   range,
    support_mesh: trimesh.Trimesh,
    *,
    debug_label:  str | None = None,
    debug_vertex: int | None = None,
) -> None:
    """Color only the wall (edge) faces green/red by FDM support in-place.

    Two failure modes are checked:

    1. **Angle overhang** — face normal's Z component is below -sin(FLOOR_ANGLE_DEG),
       meaning the face tilts more steeply than the printer can bridge unsupported.

    2. **Support** — direct support comes from contact with the parent mesh or
       a −Z ray hit. Printability then propagates upward across shared edges
       through faces that satisfy the overhang angle. Disconnected islands stay
       red even when their local face angles are otherwise printable.

    Surface and cap faces are left untouched.
    """
    threshold  = -np.sin(np.radians(FLOOR_ANGLE_DEG))
    wall_idx   = np.array(list(wall_faces), dtype=np.intp)

    # ── check 1: face-normal angle ────────────────────────────────────────
    wall_nz    = mesh.face_normals[wall_idx, 2]
    angle_ok   = wall_nz >= threshold

    # ── check 2: island detection (ray cast straight down) ────────────────
    face_verts  = mesh.vertices[mesh.faces[wall_idx]]              # (N, 3, 3)
    lowest_vi   = face_verts[:, :, 2].argmin(axis=1)               # (N,)
    lowest_pts  = face_verts[np.arange(len(wall_idx)), lowest_vi]  # (N, 3)
    # A point inside or on the support mesh is supported immediately: model
    # this as the downward ray hitting the enclosing material at distance zero.
    # Only points genuinely outside the support mesh need a ray cast.
    inside = support_mesh.contains(lowest_pts)
    _, surface_distance, _ = support_mesh.nearest.on_surface(lowest_pts)
    inside_or_on = inside | (surface_distance <= SUPPORT_SURFACE_TOLERANCE_MM)

    has_support = inside_or_on.copy()
    outside_idx = np.where(~inside_or_on)[0]
    if len(outside_idx):
        ray_origins = lowest_pts[outside_idx] - np.array([0.0, 0.0, 1e-3])
        ray_dirs = np.tile([0.0, 0.0, -1.0], (len(outside_idx), 1))
        has_support[outside_idx] = support_mesh.ray.intersects_any(
            ray_origins,
            ray_dirs,
        )

    # Propagate printability from directly supported faces through shared wall
    # edges, but only upward through faces that satisfy the overhang angle.
    # This prevents a locally acceptable face from being marked printable when
    # it is part of a disconnected floating island.
    face_min_z = face_verts[:, :, 2].min(axis=1)
    edge_to_faces: dict[tuple[int, int], list[int]] = {}
    for local_i, face in enumerate(mesh.faces[wall_idx]):
        for a, b in ((face[0], face[1]), (face[1], face[2]), (face[2], face[0])):
            edge = tuple(sorted((int(a), int(b))))
            edge_to_faces.setdefault(edge, []).append(local_i)

    neighbors: list[set[int]] = [set() for _ in wall_idx]
    for incident_faces in edge_to_faces.values():
        for face_i in incident_faces:
            neighbors[face_i].update(j for j in incident_faces if j != face_i)

    printable = has_support.copy()
    changed = True
    while changed:
        changed = False
        for face_i in np.argsort(face_min_z):
            if printable[face_i] or not angle_ok[face_i]:
                continue
            if any(
                printable[neighbor]
                and face_min_z[neighbor] <= face_min_z[face_i] + 1e-6
                for neighbor in neighbors[face_i]
            ):
                printable[face_i] = True
                changed = True

    overhang = ~printable

    mesh.visual.face_colors[wall_idx[~overhang]] = _COLOR_SUPPORTED
    mesh.visual.face_colors[wall_idx[ overhang]] = _COLOR_OVERHANG
    if debug_label is not None:
        print(f"    wall-fdm[{debug_label}] faces={len(wall_idx)}, "
              f"bad-angle={int(np.sum(~angle_ok))}, "
              f"inside-or-on={int(np.sum(inside_or_on))}, "
              f"no-vertical-support={int(np.sum(~has_support))}, "
              f"propagated-support={int(np.sum(printable & ~has_support))}, "
              f"red-union={int(np.sum(overhang))}")
        if debug_vertex is not None:
            incident = np.where(np.any(mesh.faces[wall_idx] == debug_vertex, axis=1))[0]
            for local_i in incident:
                face_i = int(wall_idx[local_i])
                vids = mesh.faces[face_i]
                print(f"      tip-face[{face_i}] vids={vids.tolist()}, "
                      f"z={np.array2string(mesh.vertices[vids, 2], precision=4)}, "
                      f"r={np.array2string(np.linalg.norm(mesh.vertices[vids], axis=1), precision=4)}, "
                      f"normal.z={mesh.face_normals[face_i, 2]:+.4f}, "
                      f"lowest-vid={int(vids[lowest_vi[local_i]])}, "
                      f"surface-distance={surface_distance[local_i]:.4f}, "
                      f"inside/on={bool(inside_or_on[local_i])}, "
                      f"ray-support={bool(has_support[local_i])}, "
                      f"red={bool(overhang[local_i])}")


def _find_tilt(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    sphere_radius:  float,
    curl_deg:       float = LEAF_CURL_DEG,
) -> float:
    """Binary-search for the maximum downward tilt angle θ (radians).

    The base is the rotation pivot and is on the sphere surface.  At θ=0 the
    leaf lays flat; at θ>0 the tip rotates toward the sphere.

    Vertices already within TOLERANCE_GAP_MM of the sphere at θ=0 (base-zone
    vertices near the attachment point) are excluded from the constraint.
    """
    flat = build_leaf_surface(
        base_pos=base,
        tangent=-L,
        length_mm=LEAF_LENGTH_MM,
        width_mm=LEAF_WIDTH_MM,
        fold_angle_deg=LEAF_FOLD_DEG,
        curl_deg=curl_deg,
        up_hint=surface_normal,
    )
    dists_flat = np.linalg.norm(flat.vertices, axis=1)
    far_mask   = dists_flat >= sphere_radius + TOLERANCE_GAP_MM
    far_idx    = np.where(far_mask)[0]

    if len(far_idx) == 0:
        return 0.0

    # Rotation axis: cross(L, surface_normal) → positive θ tilts tip toward sphere.
    # Pivot is the base.
    axis = np.cross(L, surface_normal)
    axis /= np.linalg.norm(axis) + 1e-12

    def _rotate(pts: np.ndarray, theta: float) -> np.ndarray:
        c, s = np.cos(theta), np.sin(theta)
        rel  = pts - base
        return base + rel * c + np.cross(axis, rel) * s \
               + axis * (rel @ axis)[:, np.newaxis] * (1.0 - c)

    far_verts_flat = flat.vertices[far_idx]

    def _ok(theta: float) -> bool:
        return bool(
            np.all(np.linalg.norm(_rotate(far_verts_flat, theta), axis=1)
                   >= sphere_radius + TOLERANCE_GAP_MM)
        )

    lo, hi = 0.0, np.pi
    while hi > 1e-4 and _ok(hi):
        hi /= 2.0
    if _ok(hi):
        return 0.0

    for _ in range(48):
        mid = 0.5 * (lo + hi)
        if _ok(mid):
            lo = mid
        else:
            hi = mid
    return lo


def _build_tilted_leaf(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    theta:          float,
    curl_deg:       float = LEAF_CURL_DEG,
) -> tuple[trimesh.Trimesh, np.ndarray]:
    """Build the leaf at tilt angle θ with its base on the sphere.

    The leaf normal — normal to the plane in which the leaf lays flat — is
    computed geometrically from the actual mesh after building:

        midrib  = normalize(tip_vertex − base)          (from real geometry)
        lateral = normalize(cross(surface_normal, L))   (T axis, unchanged by tilt)
        normal  = normalize(cross(midrib, lateral))     (normal to that plane)

    Using ``up_hint`` only as a build hint; the returned normal is derived from
    the real base-to-tip axis, not the rotation formula.
    """
    c, s = np.cos(theta), np.sin(theta)

    up_hint  = c * surface_normal + s * L
    up_hint /= np.linalg.norm(up_hint) + 1e-12

    tangent  = -c * L - s * surface_normal
    tangent /= np.linalg.norm(tangent) + 1e-12

    leaf = build_leaf_surface(
        base_pos=base,
        tangent=tangent,
        length_mm=LEAF_LENGTH_MM,
        width_mm=LEAF_WIDTH_MM,
        fold_angle_deg=LEAF_FOLD_DEG,
        curl_deg=curl_deg,
        up_hint=up_hint,
    )

    # Compute the midplane normal from the actual leaf geometry:
    # find the tip (vertex furthest from base along the tangent direction),
    # then take the normal to the plane spanned by (midrib, lateral).
    verts   = leaf.vertices
    tip_pt  = verts[np.argmax((verts - base) @ tangent)]
    midrib  = tip_pt - base
    midrib /= np.linalg.norm(midrib) + 1e-12

    lateral  = np.cross(surface_normal, L)   # T axis — perpendicular to tilt plane
    lateral /= np.linalg.norm(lateral) + 1e-12

    normal   = np.cross(midrib, lateral)
    normal  /= np.linalg.norm(normal) + 1e-12
    if np.dot(normal, surface_normal) < 0:   # ensure outward orientation
        normal = -normal

    return leaf, normal


def _fdm_tip_extension_outside_support(
    surface:      trimesh.Trimesh,
    normal:       np.ndarray,
    support_mesh: trimesh.Trimesh,
    depth:        float = LEAF_ROOT_DEPTH_MM,
) -> bool:
    """Whether the required FDM tip-root extension would leave the support mesh."""
    n = normal / (np.linalg.norm(normal) + 1e-12)
    loop = _boundary_loop(surface)
    perim = surface.vertices[loop]
    tip_i = int(np.argmin(perim[:, 2]))
    tip = perim[tip_i]
    root = tip - depth * n

    horiz = np.linalg.norm(root[:2] - tip[:2])
    max_z = tip[2] - horiz / np.tan(np.radians(FLOOR_ANGLE_DEG))
    if root[2] <= max_z:
        return False

    extended_root = root.copy()
    extended_root[2] = max_z
    point = extended_root[np.newaxis]
    inside = bool(support_mesh.contains(point)[0])
    _, distance, _ = support_mesh.nearest.on_surface(point)
    return not (inside or distance[0] <= SUPPORT_SURFACE_TOLERANCE_MM)


def _build_leaf_for_sphere(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    sphere_radius:  float,
    support_mesh:   trimesh.Trimesh,
) -> tuple[trimesh.Trimesh, np.ndarray, float, float]:
    """Find tilt and retain curl unless its FDM tip extension leaves support.

    Returns ``(leaf_surface, normal, theta, curl_deg_used)``.
    """
    for curl_deg in (LEAF_CURL_DEG, 0.0):
        theta = _find_tilt(
            base,
            L,
            surface_normal,
            sphere_radius,
            curl_deg=curl_deg,
        )
        leaf_surf, normal = _build_tilted_leaf(
            base,
            L,
            surface_normal,
            theta,
            curl_deg=curl_deg,
        )
        if curl_deg == 0.0 or not _fdm_tip_extension_outside_support(
            leaf_surf,
            normal,
            support_mesh,
        ):
            return leaf_surf, normal, theta, curl_deg

    raise AssertionError("zero-curl leaf construction did not return")


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(*, debug_solidify: bool = False) -> trimesh.Trimesh:
    """Sphere + trunk + four tilted bare leaf surfaces."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))

    _trunk_embed = 5.0
    trunk = trimesh.creation.cylinder(
        radius=TRUNK_RADIUS_MM,
        height=TRUNK_HEIGHT_MM + _trunk_embed,
        sections=32,
    )
    trunk.apply_translation(
        [0.0, 0.0, -SPHERE_RADIUS_MM - TRUNK_HEIGHT_MM / 2.0 + _trunk_embed / 2.0]
    )
    trunk.fix_normals()
    tag(trunk, debug_material(1))

    parts: list[trimesh.Trimesh] = [sphere, trunk]

    # Support mesh used for island detection: anything the leaf can rest on.
    support_mesh = trimesh.util.concatenate([sphere, trunk])

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        polar          = np.pi * fraction_down
        surface_normal = np.array([np.sin(polar), 0.0, np.cos(polar)])

        # Canonical attachment point — where we want the leaf midpoint to land.
        midpoint  = SPHERE_RADIUS_MM * surface_normal
        L_mid, _  = _leaf_frame(surface_normal)

        if fraction_down == 0.0:
            # Pole: can't straddle the apex (base on one side, tip on the other).
            # Place the base at the pole itself.
            base, n_base, L_base = midpoint, surface_normal, L_mid
        else:
            # Base point: on the sphere, half a leaf-length "above" midpoint along L,
            # so that the flat leaf's midpoint lands at `midpoint`.
            base, n_base, L_base = _base_for_midpoint(midpoint, L_mid, SPHERE_RADIUS_MM)

        # Tilt + curl selection: tries LEAF_CURL_DEG, falls back to 0 if the
        # tip would be outside the sphere.  Detection happens during construction.
        leaf_surf, normal, theta, curl_used = _build_leaf_for_sphere(
            base, L_base, n_base, SPHERE_RADIUS_MM, support_mesh,
        )

        leaf, wall_faces = _solidify_leaf(
            leaf_surf,
            normal,
            parent_sphere_radius=SPHERE_RADIUS_MM,
            debug_label=name if debug_solidify else None,
        )
        tip_vertex = int(np.argmin(leaf_surf.vertices[:, 2]))

        tag(leaf, debug_material(2 + leaf_index))                   # surface colour
        _color_wall_faces_by_fdm(
            leaf,
            wall_faces,
            support_mesh,
            debug_label=name if debug_solidify else None,
            debug_vertex=tip_vertex if debug_solidify else None,
        )

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok   = int(np.sum(wall_colors[:, 0] < 128))   # green has low red channel
        n_fail = len(wall_faces) - n_ok
        curl_note = f"  curl={curl_used:.0f}°" if curl_used != LEAF_CURL_DEG else ""
        print(f"  {name:13s}  tilt={np.degrees(theta):5.1f}°{curl_note}  "
              f"normal=({normal[0]:+.3f}, {normal[1]:+.3f}, {normal[2]:+.3f})  "
              f"wall green={n_ok} red={n_fail}  watertight={leaf.is_watertight}")
        parts.append(leaf)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument(
        "--debug-solidify",
        action="store_true",
        help="Print perimeter/root coordinates around each solidified leaf tip",
    )
    args = parser.parse_args()

    mesh = build_debug_mesh(debug_solidify=args.debug_solidify)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
