#!/usr/bin/env python3
"""Generate a debug sphere with branchlet+leaf assemblies at four latitudes.

Each branchlet grows straight out along the surface normal.  The loft morphs
from a circle at the root to the exact leaf perimeter at the tip, and the leaf
surface is stitched directly to that tip ring — loft walls, root cap, and leaf
surface form a single closed mesh with no overlapping parts.

Geometry that violates the FDM floor-angle rule is coloured debug-red
(DEBUG_COLOR_0, reserved for failures).  Passing parts use slots 1+.

Run from the repository root::

    python src/scripts/generate-leaf-placement-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    compute_leaf_geometry,
    _LEAF_N_LONG,
    _LEAF_N_LAT,
)

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT    = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM  = 12.0
LEAF_LENGTH_MM    = 6.0
LEAF_WIDTH_MM     = LEAF_LENGTH_MM * 2.0 / 3.0
LEAF_FOLD_DEG     = 6.0
LEAF_CURL_DEG     = 40.0   # concave turn over the second half of the leaf
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90   # root-ring diameter as fraction of max leaf dim

# Perimeter vertex count: rounded base + left edge (n_rings pts) +
#                         pointed tip + right edge (n_rings pts).
_N_RINGS  = _LEAF_N_LONG - 1          # interior longitudinal rings = 11
_N_LAT    = _LEAF_N_LAT               # lateral columns = 10
_NP       = 2 * _N_RINGS + 2          # perimeter vertex count = 24
_N_LOFT_RINGS = 2   # root circle + leaf perimeter; no intermediate rings needed

LEAF_THICKNESS_MM      = 0.16
UNDERCUT_MM            = 0.5   # undercut step: down -n and inward by this amount
UNDERCUT_MIN_ANGLE_DEG = 25.0  # min undercut slope angle above the leaf plane

FAIL_MATERIAL: Material = debug_material(0)   # red — reserved for FDM failures

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Geometry helpers ───────────────────────────────────────────────────────────

def _leaf_frame(surface_normal: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf long and lateral axes in the tangent plane.

    L is world-up projected onto the plane perpendicular to *surface_normal*
    (falls back to +X at the poles).  T = cross(surface_normal, L).
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up   = np.array([0.0, 0.0, 1.0])
    proj = up - np.dot(up, n) * n
    if np.linalg.norm(proj) < 0.1:
        proj = np.array([1.0, 0.0, 0.0]) - np.dot([1, 0, 0], n) * n
    L = proj / (np.linalg.norm(proj) + 1e-12)
    T = np.cross(n, L);  T /= np.linalg.norm(T) + 1e-12
    return L, T


def _circle_ring(
    center: np.ndarray,
    u: np.ndarray, v: np.ndarray,
    radius: float,
    n_pts: int,
) -> np.ndarray:
    """Circle of *n_pts* vertices in the plane spanned by *u* and *v*."""
    t = np.linspace(0, 2 * np.pi, n_pts, endpoint=False)
    return center + radius * (np.cos(t[:, None]) * u + np.sin(t[:, None]) * v)


def _is_printable(surface_normal: np.ndarray) -> bool:
    """True iff the branchlet axis clears the FDM floor angle."""
    n = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    return float(n[2]) >= np.sin(np.radians(FLOOR_ANGLE_DEG)) - 1e-6


# ── Branchlet + leaf (single closed mesh) ────────────────────────────────────

def build_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
    *,
    debug_tip: bool = False,
) -> tuple[trimesh.Trimesh, bool]:
    """Loft root circle → undercut ring → leaf perimeter, with leaf surface on top.

    Three-ring topology (root circle, undercut ring, boundary):
      - Loft walls:    root circle → undercut ring
      - Undercut wall: undercut ring → boundary  (the undercut itself)
      - Leaf surface:  stitched to boundary from above
      - Root cap:      closes the buried root end

    The undercut ring is derived from the boundary by stepping −n by
    LEAF_THICKNESS_MM and then inward toward the perimeter centroid by the
    same amount.  Because the loft terminates at the undercut ring and the
    leaf surface attaches to boundary, every boundary edge is shared by
    exactly two faces (undercut wall + leaf surface), so the mesh is manifold.
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n)

    root_radius = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, LEAF_WIDTH_MM)
    root_center = attachment_point - EMBED_DEPTH_MM * n
    tip_pos     = root_center + (EMBED_DEPTH_MM + LEAF_LENGTH_MM) * n

    # ── Leaf geometry ─────────────────────────────────────────────────────────
    g = compute_leaf_geometry(
        base_pos=tip_pos + (LEAF_LENGTH_MM / 2.0) * L,
        tangent=-L,
        length_mm=LEAF_LENGTH_MM,
        width_mm=LEAF_WIDTH_MM,
        fold_angle_deg=LEAF_FOLD_DEG,
        curl_deg=LEAF_CURL_DEG,
        up_hint=n,
    )

    NP = _NP       # 24  perimeter vertices
    nr = _N_RINGS  # 11  interior longitudinal rings
    NT = _N_LAT    # 10  lateral columns

    # ── Boundary (leaf perimeter, 24 vertices) ────────────────────────────────
    # Order: rounded_base | left_edge (s↑) | pointed_tip | right_edge (s↓)
    boundary = np.vstack([
        g.bp[np.newaxis],          # k=0:      rounded base
        g.top_pts[:, 0, :],        # k=1..11:  left edge, s increasing
        g.v_tip[np.newaxis],       # k=12:     pointed tip
        g.top_pts[::-1, NT, :],    # k=13..23: right edge, s decreasing
    ])  # (NP, 3)

    # ── Root ring ─────────────────────────────────────────────────────────────
    root_ring = _circle_ring(root_center, L, T, root_radius, NP)

    # ── Undercut ring ─────────────────────────────────────────────────────────
    # From each boundary vertex: move inward toward the perimeter centroid by
    # UNDERCUT_MM.  The -n drop is the amount needed to keep that undercut at
    # UNDERCUT_MIN_ANGLE_DEG or steeper above the leaf plane.
    t_mm         = UNDERCUT_MM
    perim_center = np.mean(boundary, axis=0)
    inward       = perim_center[None] - boundary
    inward_unit  = inward / np.maximum(np.linalg.norm(inward, axis=1, keepdims=True), 1e-10)

    down_mm = np.full(NP, t_mm)

    def _angle_above_leaf_plane(a: np.ndarray, b: np.ndarray) -> float:
        delta = b - a
        normal_delta = abs(float(np.dot(delta, n)))
        plane_delta = float(np.linalg.norm(delta - np.dot(delta, n) * n))
        return float(np.degrees(np.arctan2(normal_delta, max(plane_delta, 1e-10))))

    def _angle_at(a: np.ndarray, b: np.ndarray, c: np.ndarray) -> float:
        ba = b - a
        ca = c - a
        denom = max(float(np.linalg.norm(ba) * np.linalg.norm(ca)), 1e-10)
        return float(np.degrees(np.arccos(np.clip(float(np.dot(ba, ca)) / denom, -1.0, 1.0))))

    def _section_anchor(k: int) -> np.ndarray:
        if k == 0:
            return g.top_pts[0, NT // 2, :]
        if 1 <= k <= nr:
            return g.top_pts[k - 1, NT // 2, :]
        if k == nr + 1:
            return g.top_pts[-1, NT // 2, :]
        return g.top_pts[2 * nr + 1 - k, NT // 2, :]

    def _undercut_ring_for(depths: np.ndarray) -> np.ndarray:
        trial_down = np.asarray(depths, dtype=float)
        return boundary - trial_down[:, None] * n[None] + t_mm * inward_unit

    def _min_undercut_section_angle(depths: np.ndarray) -> float:
        trial_ring = _undercut_ring_for(depths)
        return min(
            _angle_at(boundary[k], _section_anchor(k), trial_ring[k])
            for k in range(NP)
        )

    for k in range(NP):
        def _local_min_angle(depth: float) -> float:
            trial_down = down_mm.copy()
            trial_down[k] = depth
            trial_ring = _undercut_ring_for(trial_down)
            return _angle_at(boundary[k], _section_anchor(k), trial_ring[k])

        if _local_min_angle(t_mm) >= UNDERCUT_MIN_ANGLE_DEG:
            continue
        lo = hi = t_mm
        while _local_min_angle(hi) < UNDERCUT_MIN_ANGLE_DEG and hi < 8.0 * t_mm:
            hi *= 1.25
        for _ in range(24):
            mid = 0.5 * (lo + hi)
            if _local_min_angle(mid) >= UNDERCUT_MIN_ANGLE_DEG:
                hi = mid
            else:
                lo = mid
        down_mm[k] = hi

    undercut_ring = boundary - down_mm[:, None] * n[None] + t_mm * inward_unit

    if debug_tip:
        tip_k = nr + 1
        print("Tip undercut debug:")
        print(f"  tip index: {tip_k}")
        print(f"  target angle: {UNDERCUT_MIN_ANGLE_DEG:.2f} deg")
        mid_k = nr // 2 + 1
        print(f"  normal drop: {down_mm[tip_k]:.4f} mm")
        print(f"  mid-edge index: {mid_k}")
        print(f"  mid-edge normal drop: {down_mm[mid_k]:.4f} mm")
        print(
            f"  tip section angle: "
            f"{_angle_at(boundary[tip_k], _section_anchor(tip_k), undercut_ring[tip_k]):.2f} deg"
        )
        print(
            f"  mid-edge section angle: "
            f"{_angle_at(boundary[mid_k], _section_anchor(mid_k), undercut_ring[mid_k]):.2f} deg"
        )
        for j in (tip_k - 1, tip_k + 1):
            print(
                f"  top tip -> top neighbor {j}: "
                f"{_angle_above_leaf_plane(boundary[tip_k], boundary[j]):.2f} deg"
            )
        for j in (tip_k - 1, tip_k, tip_k + 1):
            print(
                f"  top tip -> undercut {j}: "
                f"{_angle_above_leaf_plane(boundary[tip_k], undercut_ring[j]):.2f} deg"
            )
        print(f"  min undercut section angle: {_min_undercut_section_angle(down_mm):.2f} deg")

    # ── Vertex layout ─────────────────────────────────────────────────────────
    #   0 .. NP-1          root ring
    #   NP .. 2*NP-1       undercut ring
    #   2*NP               root cap centre
    #   2*NP+1 .. 3*NP     boundary (leaf perimeter)
    #   3*NP+1 ..          leaf interior grid  (nr*(NT-1) vertices)
    V_ROOT_CAP = 2 * NP
    V_BND_BASE = 2 * NP + 1
    V_INT_BASE = 3 * NP + 1

    verts = np.vstack([
        root_ring,
        undercut_ring,
        root_center[np.newaxis],
        boundary,
        g.top_pts[:, 1:NT, :].reshape(-1, 3),
    ])

    # ── Index helpers ─────────────────────────────────────────────────────────
    def vi_r0(k: int)  -> int: return k % NP
    def vi_uc(k: int)  -> int: return NP + k % NP
    def vi_bnd(k: int) -> int: return V_BND_BASE + k % NP

    def leaf_vi(ri: int, j: int) -> int:
        if j == 0:
            return vi_bnd(ri + 1)                       # left edge
        if j == NT:
            return vi_bnd(2 * nr + 1 - ri)              # right edge
        return V_INT_BASE + ri * (NT - 1) + (j - 1)    # interior

    def leaf_base_vi() -> int: return vi_bnd(0)
    def leaf_tip_vi()  -> int: return vi_bnd(nr + 1)

    # ── Face construction ─────────────────────────────────────────────────────
    faces: list[list[int]] = []

    # Loft walls: root ring → undercut ring
    for k in range(NP):
        k1 = (k + 1) % NP
        a, b = vi_r0(k),  vi_r0(k1)
        c, d = vi_uc(k1), vi_uc(k)
        faces += [[a, d, c], [a, c, b]]

    # Root cap — closes the embedded root end.
    for k in range(NP):
        k1 = (k + 1) % NP
        faces.append([V_ROOT_CAP, vi_r0(k1), vi_r0(k)])

    # Undercut wall: undercut ring → boundary (outward and upward step).
    for k in range(NP):
        k1 = (k + 1) % NP
        a, b = vi_uc(k),   vi_uc(k1)
        c, d = vi_bnd(k1), vi_bnd(k)
        faces += [[a, d, c], [a, c, b]]

    # Leaf surface — base fan (rounded base → ring 0 of top_pts).
    for j in range(NT):
        faces.append([leaf_base_vi(), leaf_vi(0, j), leaf_vi(0, j + 1)])

    # Leaf surface — body quads (interior rows 0..nr-2).
    for ri in range(nr - 1):
        for j in range(NT):
            a, b = leaf_vi(ri,   j),     leaf_vi(ri,   j + 1)
            c, d = leaf_vi(ri+1, j + 1), leaf_vi(ri+1, j)
            faces += [[a, d, c], [a, c, b]]

    # Leaf surface — tip fan (last row → pointed tip).
    for j in range(NT):
        faces.append([leaf_tip_vi(), leaf_vi(nr - 1, j + 1), leaf_vi(nr - 1, j)])

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh, not _is_printable(n)


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(*, debug_tip: bool = False) -> trimesh.Trimesh:
    """Build the coloured sphere and four branchlet+leaf assemblies."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))   # slot 0 reserved for failures

    parts: list[trimesh.Trimesh] = [sphere]

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        polar  = np.pi * fraction_down
        normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
        mesh, is_fail = build_branchlet_and_leaf(
            SPHERE_RADIUS_MM * normal,
            normal,
            debug_tip=debug_tip and name == "top",
        )
        slot   = 2 + leaf_index
        status = "FAIL (red)" if is_fail else f"ok  (slot {slot})  watertight={mesh.is_watertight}"
        print(f"  {name:13s}  {status}")
        tag(mesh, FAIL_MATERIAL if is_fail else debug_material(slot))
        parts.append(mesh)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument("--debug-tip", action="store_true",
                        help="Print diagnostic angles for the top leaf tip")
    args = parser.parse_args()

    mesh = build_debug_mesh(debug_tip=args.debug_tip)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
