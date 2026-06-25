#!/usr/bin/env python3
"""
Leaf placement test — four mesh objects, one color-STL.

Generates a single binary STL (Materialise/Magics RGB15 face-colour encoding)
containing four objects floating in space:

  1. Sphere       r=10 mm,  centred at XY=(0, 0)
     Baseline reference — all six meridians are identical.

  2. Cluster A    near-vertical  tip_t=[0, 0, 1]       XY=(40, 0)
     Symmetric foliage cluster following a straight vertical branch.

  3. Cluster B    30° tilt       tip_t=[0.5, 0, 0.866] XY=(80, 0)
     Asymmetric cluster; front and back meridians differ noticeably.

  4. Cluster C    58° tilt, curved spine               XY=(120, 0)
     start_t=[0.5, 0, 0.866]  →  tip_t=[0.848, 0, 0.530]
     Tests steep-branch underside filtering and curved Bézier spine.

Colour scheme
-------------
Base meshes (sphere / foliage clusters) : Material.FOLIAGE  (dark forest green)
Leaves on every object                  : cycling DEBUG_COLORS by row index

The row colouring lets you directly see the arc-equidistant row structure
on each shape — rows near the apex are visually close together in Z but
evenly spaced in surface arc, which is the whole point of the meridian-arc
algorithm.

Output format
-------------
Binary STL with RGB15 face-colour attribute bytes (Materialise/Magics convention).
Open in:
  • Windows 3D Builder        — shows per-face colours
  • MeshMixer / Magics        — shows per-face colours
  • MeshLab                   — File → Import Mesh  (colours shown)
  • PrusaSlicer / Bambu Studio — geometry only (ignores colour bytes)

Usage::

    cd /path/to/dharmatiles
    python src/scripts/test-leaf-placement.py
    python src/scripts/test-leaf-placement.py /tmp/leaves.stl
"""
from __future__ import annotations

import math
import sys
import time
from pathlib import Path

import numpy as np
import trimesh

# ── Meridian-arc helpers from the trees module ────────────────────────────────
from dharmatiles.trees.mesh import (
    _build_foliage_cluster_mesh,
    _build_meridians,
    _compute_row_z_positions,
    _contact_angle_for_sphere,
    _hash01_int,
    _interpolate_meridian_normal,
)
from dharmatiles.trees.leaf import build_leaf_surface, solidify_leaf
from dharmatiles.trees._utils import _safe_norm

# ── Colour system ─────────────────────────────────────────────────────────────
from dharmatiles.core.color import (
    DEBUG_COLORS,
    Material,
    RGBA,
    export_color_stl,
    tag as _color_tag,
)


def _apply_face_color(
    mesh: trimesh.Trimesh,
    rgba: tuple[int, int, int, int],
) -> None:
    """Stamp a uniform RGBA face colour onto *mesh* in-place."""
    color = np.asarray(rgba, dtype=np.uint8)
    mesh.visual = trimesh.visual.ColorVisuals(
        mesh=mesh,
        face_colors=np.tile(color, (len(mesh.faces), 1)),
    )


def _row_rgba(row_idx: int) -> tuple[int, int, int, int]:
    """Map a row index to a cycling debug RGBA colour."""
    return RGBA[DEBUG_COLORS[row_idx % len(DEBUG_COLORS)]]


# ── Shared leaf / cluster parameters ─────────────────────────────────────────

_LEAF = dict(
    leaf_length_mm      = 4.5,
    leaf_width_mm       = 3.0,
    leaf_thickness_mm   = 0.24,
    leaf_fold_angle_deg = 6.0,
    leaf_inner_curve    = 1.5,
    leaf_outer_curve    = 0.72,
    leaf_curl_deg       = 40.0,
    leaf_lift_mm        = 3.0,
    leaf_h_overlap      = 0.2,
    leaf_v_overlap      = 0.5,
    leaf_arc_meridians  = 6,
    leaf_arc_z_samples  = 64,
)

_CLUSTER = dict(
    r_wood                = 1.0,
    r_foliage             = 5.5,
    clump_length_mm       = 10.5,
    leaves                = False,   # we run placement ourselves for row colouring
    leaf_base_count       = 0,
    leaf_cap_count        = 0,
    leaf_angle_jitter_deg = 0.0,
    leaf_pos_jitter       = 0.0,
    **_LEAF,
)

# Subset passed to _contact_angle_for_sphere and build_leaf_surface.
_LEAF_KW = dict(
    length_mm      = float(_LEAF["leaf_length_mm"]),
    width_mm       = float(_LEAF["leaf_width_mm"]),
    thickness_mm   = float(_LEAF["leaf_thickness_mm"]),
    fold_angle_deg = float(_LEAF["leaf_fold_angle_deg"]),
    inner_curve    = float(_LEAF["leaf_inner_curve"]),
    outer_curve    = float(_LEAF["leaf_outer_curve"]),
    curl_deg       = float(_LEAF["leaf_curl_deg"]),
    lift_mm        = float(_LEAF["leaf_lift_mm"]),
)


# ── Meridian-arc leaf placement (shared for sphere + clusters) ────────────────

def _place_leaves_on_mesh(
    mesh: trimesh.Trimesh,
    *,
    seed:  int = 0,
    label: str = "mesh",
) -> list[trimesh.Trimesh]:
    """Run meridian-arc leaf placement on any closed mesh.

    Each leaf is coloured by its row index (cycling DEBUG_COLORS) so the
    row structure is immediately visible in any coloured viewer.

    Parameters
    ----------
    mesh  : Closed watertight trimesh used as both the tiling surface and
            the parent for ``solidify_leaf`` raycast embedding.
    seed  : Integer seed for per-leaf deterministic randomisation.
    label : Label for progress output.

    Returns
    -------
    List of solidified, coloured leaf Trimesh objects.
    """
    L   = float(_LEAF["leaf_length_mm"])
    W   = float(_LEAF["leaf_width_mm"])
    hov = float(_LEAF["leaf_h_overlap"])
    vov = float(_LEAF["leaf_v_overlap"])

    col_step = max(W * (1.0 - hov), 1e-3)
    z_top    = float(mesh.vertices[:, 2].max())
    cx       = float(mesh.vertices[:, 0].mean())
    cy       = float(mesh.vertices[:, 1].mean())

    meridians = _build_meridians(
        mesh,
        n_meridians = int(_LEAF["leaf_arc_meridians"]),
        z_samples   = int(_LEAF["leaf_arc_z_samples"]),
    )
    row_zs = _compute_row_z_positions(meridians, L, vov, z_top)

    print(f"  [{label}] meridians={len(meridians)}  rows={len(row_zs)}")
    if row_zs:
        print(f"    z_range=[{row_zs[0]:.2f}, {row_zs[-1]:.2f}]  (z_top={z_top:.2f})")

    # Contact-angle cache: identical for all leaves sharing the same local radius.
    _ca_cache: dict[int, float] = {}

    def _cached_ca(r: float) -> float:
        key = round(r * 1000)
        if key not in _ca_cache:
            _ca_cache[key] = _contact_angle_for_sphere(r, **_LEAF_KW)
        return _ca_cache[key]

    parts: list[trimesh.Trimesh] = []

    for row_idx, z_row in enumerate(row_zs):
        row_color = _row_rgba(row_idx)

        sec = mesh.section(
            plane_origin = np.array([0.0, 0.0, z_row]),
            plane_normal = np.array([0.0, 0.0, 1.0]),
        )
        if sec is None:
            continue
        try:
            path2d, xform = sec.to_planar()
        except Exception:
            continue

        for poly in path2d.polygons_full:
            perim = float(poly.length)
            if perim < 1e-3:
                continue

            c2d = poly.centroid
            c4d = xform @ np.array([float(c2d.x), float(c2d.y), 0.0, 1.0])
            centroid_3d = c4d[:3].copy()

            n_col = max(1, int(math.ceil(perim / col_step)))
            for ci in range(n_col):
                t    = float(ci) / float(n_col)
                pt2  = poly.exterior.interpolate(t, normalized=True)
                p4d  = xform @ np.array([float(pt2.x), float(pt2.y), 0.0, 1.0])
                pt3d = p4d[:3].copy()

                phi     = float(np.arctan2(pt3d[1] - cy, pt3d[0] - cx))
                up_hint = _interpolate_meridian_normal(meridians, phi, z_row)

                if float(up_hint[2]) < -0.1:     # skip downward-facing
                    continue

                local_r = float(np.linalg.norm(pt3d - centroid_3d))
                if local_r < 1.0:
                    continue

                ca = _cached_ca(local_r)
                if ca >= math.pi / 2:
                    continue

                # T0 = gravity-down projected onto the surface tangent plane.
                grav  = np.array([0.0, 0.0, -1.0])
                proj  = grav - float(np.dot(grav, up_hint)) * up_hint
                plen  = float(np.linalg.norm(proj))
                if plen < 1e-6:
                    arb = (np.array([1.0, 0.0, 0.0])
                           if abs(float(up_hint[0])) < 0.9
                           else np.array([0.0, 1.0, 0.0]))
                    T0 = _safe_norm(np.cross(up_hint, arb))
                else:
                    T0 = proj / plen

                c_ca = math.cos(ca)
                s_ca = math.sin(ca)
                tangent   = _safe_norm(T0 * c_ca - up_hint * s_ca)
                up_placed = _safe_norm(up_hint * c_ca + T0 * s_ca)

                lseed = int(_hash01_int(seed, "leaf", row_idx, ci))
                try:
                    surf  = build_leaf_surface(
                        base_pos = pt3d,
                        tangent  = tangent,
                        up_hint  = up_placed,
                        seed     = lseed,
                        **_LEAF_KW,
                    )
                    solid, _ = solidify_leaf(surf, up_placed, parent_mesh=mesh)
                except (RuntimeError, ValueError):
                    continue

                if len(solid.vertices) > 0:
                    _apply_face_color(solid, row_color)
                    parts.append(solid)

    return parts


# ── Cluster builder ───────────────────────────────────────────────────────────

def _make_cluster_parts(
    cx: float,
    cy: float,
    z_tip: float,
    tip_t: np.ndarray,
    start_t: np.ndarray,
    edge_id: int,
    label: str,
) -> list[trimesh.Trimesh]:
    """Build a foliage cluster (shape only) then place row-coloured leaves."""
    tip_t_n   = _safe_norm(np.asarray(tip_t,   float))
    start_t_n = _safe_norm(np.asarray(start_t, float))
    tip_p     = np.array([cx, cy, z_tip], float)
    start_p   = tip_p - float(_CLUSTER["clump_length_mm"]) * tip_t_n

    print(f"\n[Cluster {edge_id}: {label}]")
    print(f"  tip_t  = [{tip_t_n[0]:.3f}, {tip_t_n[1]:.3f}, {tip_t_n[2]:.3f}]"
          f"  ({math.degrees(math.acos(float(tip_t_n[2]))):.1f}° from vertical)")
    print(f"  tip_pos={[f'{v:.1f}' for v in tip_p]}  "
          f"start_pos={[f'{v:.1f}' for v in start_p]}")

    # Get cluster shape only — leaves=False so we run our own placement loop
    # for per-row colour control.
    cluster, _ = _build_foliage_cluster_mesh(
        tip_pos       = tip_p,
        tip_tangent   = tip_t_n,
        start_pos     = start_p,
        start_tangent = start_t_n,
        edge_id       = edge_id,
        bark_seed     = 42,
        **_CLUSTER,   # already has leaves=False
    )
    _color_tag(cluster, Material.FOLIAGE)

    leaves = _place_leaves_on_mesh(cluster, seed=edge_id, label=label)
    print(f"  -> {len(leaves)} leaves  "
          f"(cluster: {len(cluster.vertices):,}v / {len(cluster.faces):,}f)")
    return [cluster] + leaves


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    default_out = (
        Path(__file__).parents[2] / "stl" / "test" / "leaf-placement-test.stl"
    )
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else default_out
    out.parent.mkdir(parents=True, exist_ok=True)

    t0 = time.perf_counter()
    all_parts: list[trimesh.Trimesh] = []

    # ── Object 1: sphere ─────────────────────────────────────────────────────
    print("\n=== Object 1: Sphere r=10 mm at XY=(0, 0) ===")
    sphere = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    sphere.vertices[:, 2] += 10.0   # raise centre from origin to z=10
    sphere.fix_normals()
    _color_tag(sphere, Material.FOLIAGE)
    all_parts.append(sphere)

    sphere_leaves = _place_leaves_on_mesh(sphere, seed=0, label="sphere")
    print(f"  -> {len(sphere_leaves)} leaves")
    all_parts.extend(sphere_leaves)

    # ── Object 2: near-vertical cluster ──────────────────────────────────────
    print("\n=== Object 2: Cluster A — near-vertical ===")
    all_parts.extend(_make_cluster_parts(
        cx=40.0, cy=0.0, z_tip=30.0,
        tip_t   = [0.0, 0.0, 1.0],
        start_t = [0.0, 0.0, 1.0],
        edge_id = 1,
        label   = "vertical (0°)",
    ))

    # ── Object 3: 30° tilted cluster ─────────────────────────────────────────
    print("\n=== Object 3: Cluster B — 30° tilt ===")
    a30 = math.radians(30)
    all_parts.extend(_make_cluster_parts(
        cx=80.0, cy=0.0, z_tip=30.0,
        tip_t   = [math.sin(a30), 0.0, math.cos(a30)],
        start_t = [math.sin(a30), 0.0, math.cos(a30)],
        edge_id = 2,
        label   = "30° tilt",
    ))

    # ── Object 4: 58° tilt with curved spine ─────────────────────────────────
    print("\n=== Object 4: Cluster C — 58° tilt, curved spine ===")
    a58 = math.radians(58)
    all_parts.extend(_make_cluster_parts(
        cx=120.0, cy=0.0, z_tip=30.0,
        tip_t   = [math.sin(a58), 0.0, math.cos(a58)],
        start_t = [math.sin(a30), 0.0, math.cos(a30)],  # different start → curve
        edge_id = 3,
        label   = "58° tilt, curved spine",
    ))

    # ── Export ────────────────────────────────────────────────────────────────
    elapsed = time.perf_counter() - t0
    print(f"\nAssembling {len(all_parts)} mesh parts … ({elapsed:.1f}s so far)")
    scene = trimesh.util.concatenate(all_parts)
    export_color_stl(scene, out)

    v  = len(scene.vertices)
    f  = len(scene.faces)
    bb = scene.bounding_box.extents
    print(f"\nExported → {out}")
    print(f"  vertices={v:,}  faces={f:,}")
    print(f"  bounding box: {bb[0]:.1f} × {bb[1]:.1f} × {bb[2]:.1f} mm")
    print(f"  total time: {time.perf_counter() - t0:.1f}s")
    print()
    print("View with colours (RGB15 face-colour STL):")
    print("  • Windows 3D Builder — drag and drop the .stl")
    print("  • MeshLab            — File → Import Mesh  (colours shown)")
    print("  • MeshMixer / Magics — native support")
    print("  (PrusaSlicer / Bambu Studio: geometry only — colour bytes ignored)")
    print()
    print("Colour key:")
    print("  Dark green = foliage cluster / sphere body  (Material.FOLIAGE)")
    print("  Cycling colours per row (blue, orange, purple, cyan, …)")
    print("  — each band of one colour = one arc-equidistant row of leaves")


if __name__ == "__main__":
    main()
