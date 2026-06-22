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
LEAF_CURL_DEG    = 20.0

# Minimum clearance from the sphere surface for any non-base-zone leaf vertex.
TOLERANCE_GAP_MM = 0.10

# How far the leaf root extends inward along -normal to pierce the parent mesh.
LEAF_ROOT_DEPTH_MM = 1.0

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


def _solidify_leaf(
    surface: trimesh.Trimesh,
    normal:  np.ndarray,
    depth:   float = LEAF_ROOT_DEPTH_MM,
) -> trimesh.Trimesh:
    """Close an open leaf surface into a solid.

    For each boundary vertex of *surface*, create a *leaf_root* vertex
    offset by ``depth`` along ``-normal`` (into the parent mesh).  Then:

    * **Walls** — quads bridging each boundary edge to the corresponding
      root edge.
    * **Bottom cap** — a fan from the centroid of the root ring to close
      the buried end.

    The original surface faces are preserved as the top of the solid.
    ``fix_normals()`` is called on the result to ensure consistent winding.
    """
    n   = normal / (np.linalg.norm(normal) + 1e-12)
    loop = _boundary_loop(surface)
    NP   = len(loop)

    perim  = surface.vertices[loop]               # (NP, 3)  — leaf perimeter
    root   = perim - depth * n                    # (NP, 3)  — root ring
    center = root.mean(axis=0)                    # (3,)     — bottom cap centre

    # New vertex block appended after the surface's own vertices.
    n_surf    = len(surface.vertices)
    root_base = n_surf                            # root[i] = root_base + i
    cap_ctr   = n_surf + NP                       # single centre vertex

    all_verts = np.vstack([surface.vertices, root, center[np.newaxis]])

    wall_faces: list[list[int]] = []
    for i in range(NP):
        j  = (i + 1) % NP
        a, b = loop[i], loop[j]                  # perimeter (top)
        d, c = root_base + i, root_base + j      # root (bottom), swapped for winding
        wall_faces += [[a, b, c], [a, c, d]]

    cap_faces: list[list[int]] = []
    for i in range(NP):
        j = (i + 1) % NP
        cap_faces.append([cap_ctr, root_base + j, root_base + i])

    all_faces = np.vstack([
        surface.faces,
        np.array(wall_faces, dtype=np.int32),
        np.array(cap_faces,  dtype=np.int32),
    ])

    solid = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    solid.fix_normals()
    return solid


def _find_tilt(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    sphere_radius:  float,
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
        curl_deg=LEAF_CURL_DEG,
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
        curl_deg=LEAF_CURL_DEG,
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


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh() -> trimesh.Trimesh:
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

        # Tilt calculation and leaf build both use the base as pivot / origin.
        theta          = _find_tilt(base, L_base, n_base, SPHERE_RADIUS_MM)
        leaf_surf, normal = _build_tilted_leaf(base, L_base, n_base, theta)
        leaf           = _solidify_leaf(leaf_surf, normal)

        tag(leaf, debug_material(2 + leaf_index))
        print(f"  {name:13s}  slot {2 + leaf_index}  "
              f"tilt={np.degrees(theta):5.1f}°  "
              f"normal=({normal[0]:+.3f}, {normal[1]:+.3f}, {normal[2]:+.3f})  "
              f"watertight={leaf.is_watertight}")
        parts.append(leaf)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    args = parser.parse_args()

    mesh = build_debug_mesh()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
