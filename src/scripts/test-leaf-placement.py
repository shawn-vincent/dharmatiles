#!/usr/bin/env python3
"""
Leaf placement test — four mesh objects, one color-STL, artifact detection, PNG renders.

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

Three PNG renders are also written alongside the STL (requires pyrender):
  • leaf-placement-test-perspective.png — 35° elevation, isometric-ish
  • leaf-placement-test-top.png         — 85° elevation (near top-down)
  • leaf-placement-test-side.png        — 5° elevation (near horizontal)

Usage::

    cd /path/to/dharmatiles
    python src/scripts/test-leaf-placement.py
    python src/scripts/test-leaf-placement.py /tmp/leaves.stl
"""
from __future__ import annotations

import dataclasses
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
    _avg_z_for_arc,
)
from dharmatiles.trees.leaf import boundary_loop, build_leaf_surface, solidify_leaf
from dharmatiles.trees._utils import _safe_norm

# ── Colour system ─────────────────────────────────────────────────────────────
from dharmatiles.core.color import (
    DEBUG_COLORS,
    Material,
    RGBA,
    export_color_stl,
    tag as _color_tag,
)


# ── Artifact detection thresholds ─────────────────────────────────────────────

# A row whose (placed / attempted) ratio falls below this is reported as bald.
_BALD_ROW_THRESHOLD = 0.25

# A leaf whose mean wall depth exceeds this (in mm) is reported as having a
# long root.  The expected depth under normal conditions is LEAF_ROOT_EMBED_MM
# (≈ 0.75 mm); values >> 4× that indicate the leaf base is far from the surface.
_ROOT_DEPTH_MAX_MM = 4.0

# Row z-step is considered "too dense" when the median actual step is below this
# fraction of the expected step (L * (1 - v_overlap)).
_ROW_STEP_DENSE_FACTOR = 0.4

# Large gap between consecutive covered rows (as multiple of expected step).
_ROW_GAP_FACTOR = 2.0

# Phi-sector check: fraction of peak-count sector below which a sector is "sparse".
_PHI_SPARSE_FACTOR = 0.10

# Number of phi sectors for angular coverage analysis.
_N_PHI_SECTORS = 12

# Number of z-bands for vertical density analysis.
_N_Z_BANDS = 10

# Same-row pairs this fraction of col_step apart are likely algorithm duplicates.
_DUPE_FACTOR = 0.15

# Different-row pairs whose z-separation is below this fraction of expected_row_step
# are stacked (row spacing too tight).
_STACK_Z_FACTOR = 0.5

# Curl-region floating-leaf threshold: maximum distance (mm) of any outside
# curl-region vertex from the mesh surface above which the leaf is floating.
# Curl region = vertices > L/2 from the base attachment point.
_FLOATING_LEAF_CURL_DIST_MM = 1.5

# Buried-leaf threshold: maximum depth (mm) any curl-region vertex may penetrate
# inside the parent mesh before the leaf is considered buried.
# The root embedding stays inside L/2 from the base, so curl-region vertices
# inside the mesh are always unintentional burial.
_BURIED_LEAF_CURL_DEPTH_MM = 0.5

# Top-row spread: maximum acceptable minimum-width (mm) of the XY convex hull
# of the topmost row's base positions.  A large spread means the top row is
# far from the apex rather than converging near the tip.
_TOP_ROW_SPREAD_MAX_MM = 1.0


# ── Per-object placement statistics ──────────────────────────────────────────

@dataclasses.dataclass
class _PlacementStats:
    """Metrics collected during leaf placement on one mesh object."""
    label: str = ""
    # Totals
    n_rows: int = 0
    n_attempted: int = 0       # candidates that reached the leaf-build step
    n_placed: int = 0          # leaves successfully solidified
    # Skip / error breakdown
    skipped_downward: int = 0  # up_hint.z < -0.1 (downward-facing surface)
    skipped_small_r: int = 0   # local_r < 1.0 mm (too close to centroid)
    skipped_ca: int = 0        # contact angle ≥ π/2 (geometry impossible)
    build_errors: int = 0      # RuntimeError / ValueError from leaf builder
    # Per-row data: (z, attempted, placed) — one entry per generated row
    rows: list[tuple[float, int, int]] = dataclasses.field(default_factory=list)
    # Per-leaf data
    base_positions: list[np.ndarray] = dataclasses.field(default_factory=list)
    base_row_idx: list[int] = dataclasses.field(default_factory=list)
    root_depths: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum distance of any curl-region vertex (> L/2 from base)
    # that lies OUTSIDE the mesh.  Near zero = curl pressed against mesh;
    # > _FLOATING_LEAF_CURL_DIST_MM = leaf is floating above the surface.
    leaf_float_dists: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum depth of any curl-region vertex that lies INSIDE the
    # mesh (unsigned distance to nearest surface point).  > 0 = buried.
    leaf_buried_depths: list[float] = dataclasses.field(default_factory=list)
    # Geometry parameters (filled in once after the row loop)
    col_step: float = 0.0           # min expected spacing between same-row leaves
    expected_row_step: float = 0.0  # L * (1 - v_overlap) — target z-gap between rows
    z_top: float = 0.0              # mesh apex Z
    z_top_anchor: float = 0.0       # expected topmost row Z ≈ z_top − 0.25·L
    cx: float = 0.0                 # mesh centroid X (for phi-sector analysis)
    cy: float = 0.0                 # mesh centroid Y


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


def _min_width_xy(pts: np.ndarray) -> float:
    """Minimum width of the XY projection of *pts* (rotating-caliper approximation).

    Samples 90 evenly-spaced directions and returns the smallest
    (max_projection − min_projection), giving the minimum bounding-strip width.
    For a circular ring this equals the diameter; for a long skinny set it
    equals the narrow dimension.
    """
    if len(pts) < 2:
        return 0.0
    xy = pts[:, :2]
    angles = np.linspace(0.0, np.pi, 90, endpoint=False)
    dirs = np.stack([np.cos(angles), np.sin(angles)], axis=1)  # (90, 2)
    projs = xy @ dirs.T                                          # (N, 90)
    widths = projs.max(axis=0) - projs.min(axis=0)              # (90,)
    return float(widths.min())


# ── Shared leaf / cluster parameters ─────────────────────────────────────────

_LEAF = dict(
    leaf_length_mm      = 4.5,
    leaf_width_mm       = 3.0,
    leaf_thickness_mm   = 0.24,
    leaf_fold_angle_deg = 6.0,
    leaf_inner_curve    = 1.5,
    leaf_outer_curve    = 0.72,
    leaf_curl_deg       = 40.0,
    leaf_lift_mm        = 0.0,
    leaf_h_overlap      = 0.0,
    leaf_v_overlap      = 0.0,
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
) -> tuple[list[trimesh.Trimesh], _PlacementStats]:
    """Run meridian-arc leaf placement on any closed mesh.

    Each leaf is coloured by its row index (cycling DEBUG_COLORS) so the
    row structure is immediately visible in any coloured viewer.

    Returns
    -------
    (leaf_meshes, stats)
        List of solidified, coloured leaf Trimesh objects plus a
        _PlacementStats with coverage and artifact metrics.
    """
    L   = float(_LEAF["leaf_length_mm"])
    W   = float(_LEAF["leaf_width_mm"])
    hov = float(_LEAF["leaf_h_overlap"])
    vov = float(_LEAF["leaf_v_overlap"])

    col_step = max(W * (1.0 - hov), 1e-3)
    z_top    = float(mesh.vertices[:, 2].max())
    cx       = float(mesh.vertices[:, 0].mean())
    cy       = float(mesh.vertices[:, 1].mean())
    # 3-D mesh centroid for contact-angle radius (see comment in mesh.py).
    mesh_centroid_3d = np.array([cx, cy, float(mesh.vertices[:, 2].mean())])

    meridians = _build_meridians(
        mesh,
        n_meridians = int(_LEAF["leaf_arc_meridians"]),
        z_samples   = int(_LEAF["leaf_arc_z_samples"]),
    )
    row_zs = _compute_row_z_positions(meridians, L, vov, z_top)

    # Mirror the arc-based top-anchor from _compute_row_z_positions so the
    # BALD TOP check compares against the same target the algorithm uses.
    s_apex       = float(sum(m.arc_vals[-1] for m in meridians) / len(meridians))
    s_top_anchor = max(s_apex - 0.25 * L, 1e-6)
    z_top_anchor = _avg_z_for_arc(s_top_anchor, meridians)

    expected_row_step = L * max(1.0 - vov, 0.05)
    stats = _PlacementStats(
        label            = label,
        col_step         = col_step,
        expected_row_step = expected_row_step,
        z_top            = z_top,
        z_top_anchor     = z_top_anchor,
        cx               = cx,
        cy               = cy,
    )

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
        row_color   = _row_rgba(row_idx)
        row_attempt = 0
        row_placed  = 0

        sec = mesh.section(
            plane_origin = np.array([0.0, 0.0, z_row]),
            plane_normal = np.array([0.0, 0.0, 1.0]),
        )
        if sec is None:
            stats.rows.append((z_row, 0, 0))
            continue
        try:
            path2d, xform = sec.to_planar()
        except Exception:
            stats.rows.append((z_row, 0, 0))
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
                    stats.skipped_downward += 1
                    continue

                local_r = float(np.linalg.norm(pt3d - mesh_centroid_3d))
                if local_r < 1.0:
                    stats.skipped_small_r += 1
                    continue

                ca = _cached_ca(local_r)
                if ca >= math.pi / 2:
                    # Local radius too small for press-against-sphere geometry.
                    # Lay the leaf flat (ca=0) so it grows radially outward from
                    # the surface instead of skipping the position entirely.
                    ca = 0.0

                row_attempt += 1

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
                    loop         = boundary_loop(surf)
                    solid, _     = solidify_leaf(surf, up_placed, parent_mesh=mesh)
                except (RuntimeError, ValueError):
                    stats.build_errors += 1
                    continue

                # ── Root-depth measurement ───────────────────────────────────
                n_surf   = len(surf.vertices)
                NP       = len(loop)
                perim_v  = surf.vertices[np.array(loop)]              # (NP, 3)
                root_v   = solid.vertices[n_surf : n_surf + NP]       # (NP, 3)
                root_depth = float(np.mean(np.linalg.norm(root_v - perim_v, axis=1)))

                # ── Curl-region float / bury check ──────────────────────────
                # Curl region = vertices more than L/2 from the base attachment
                # point (excludes the embedded root zone).
                # Outside curl verts far from the mesh = floating.
                # Inside curl verts (body pierces the mesh) = buried.
                base_dists_v = np.linalg.norm(surf.vertices - pt3d, axis=1)
                curl_mask = base_dists_v > (L / 2.0)
                if curl_mask.any():
                    curl_verts = surf.vertices[curl_mask]
                    _, _curl_dists, _ = trimesh.proximity.closest_point(
                        mesh, curl_verts,
                    )
                    _inside = mesh.contains(curl_verts)
                    outside_d = _curl_dists[~_inside]
                    inside_d  = _curl_dists[_inside]
                    stats.leaf_float_dists.append(
                        float(outside_d.max()) if len(outside_d) else 0.0
                    )
                    stats.leaf_buried_depths.append(
                        float(inside_d.max()) if len(inside_d) else 0.0
                    )
                else:
                    stats.leaf_float_dists.append(0.0)
                    stats.leaf_buried_depths.append(0.0)

                stats.base_positions.append(pt3d.copy())
                stats.base_row_idx.append(row_idx)
                stats.root_depths.append(root_depth)
                row_placed        += 1
                stats.n_placed    += 1

                if len(solid.vertices) > 0:
                    _apply_face_color(solid, row_color)
                    parts.append(solid)

        stats.rows.append((z_row, row_attempt, row_placed))
        stats.n_attempted += row_attempt

    stats.n_rows = len(row_zs)
    return parts, stats


# ── Artifact analysis and reporting ──────────────────────────────────────────

def _check_artifacts(all_stats: list[_PlacementStats]) -> int:
    """Analyze placement stats; print a structured artifact report.

    Checks (in order):
      Coverage / bald spots
        1  BALD ROWS         — rows where attempts were made but few placed
        2  LOW COVERAGE      — overall placed/attempted below 50 %
        3  BALD TOP          — uncovered gap between last row and expected apex
        4  INTERIOR GAPS     — z-gap between covered rows > 2× expected step
        5  OVER-DENSE ROWS   — median actual z-step << expected (stacked rows)
        6  EMPTY Z-BANDS     — vertical z-bands with no leaves at all
        7  EMPTY PHI-SECTORS — angular sectors (30°) with no leaves
        8  SPARSE PHI-SECTOR — angular sector with < 10 % of peak density

      Leaf quality
        9  SAME-ROW DUPES    — two leaves in the same row < 15 % col_step apart
       10  CROSS-ROW STACK   — different-row pairs closer in z than 50 % of expected step
       11  LONG ROOTS        — mean wall depth > threshold

    Returns the total number of issues found (0 = clean).
    """
    print("\n" + "=" * 64)
    print("ARTIFACT DETECTION REPORT")
    print("=" * 64)

    total_issues = 0

    for stats in all_stats:
        issues: list[str] = []

        # ── Pre-compute arrays used by multiple checks ────────────────────────
        has_leaves = len(stats.base_positions) > 0
        bases      = np.stack(stats.base_positions) if has_leaves else None  # (N, 3)
        row_ids    = np.array(stats.base_row_idx)   if has_leaves else None  # (N,)

        covered_rows = [(z, att, pl) for z, att, pl in stats.rows if pl > 0]
        covered_zs   = [z for z, att, pl in covered_rows]

        # ── 1. Bald rows (attempts made but few placed) ───────────────────────
        bald = [(i, z, att, pl)
                for i, (z, att, pl) in enumerate(stats.rows)
                if att > 0 and pl / att < _BALD_ROW_THRESHOLD]
        if bald:
            covs = [f"{pl/max(1,att):.0%}" for _, _, att, pl in bald]
            issues.append(
                f"BALD ROWS [{len(bald)}/{stats.n_rows}]: "
                f"rows {[i for i,*_ in bald[:8]]}{'...' if len(bald)>8 else ''} "
                f"coverage {covs[:8]}"
            )

        # ── 2. Low overall coverage ───────────────────────────────────────────
        if stats.n_attempted > 0:
            cov = stats.n_placed / stats.n_attempted
            if cov < 0.5:
                issues.append(
                    f"LOW COVERAGE: {cov:.1%} of {stats.n_attempted} attempted "
                    f"positions placed (threshold 50%)"
                )

        # ── 3. Bald top — gap from last covered row to expected apex ──────────
        if covered_zs:
            top_gap = stats.z_top_anchor - covered_zs[-1]
            if top_gap > stats.expected_row_step:
                issues.append(
                    f"BALD TOP: last covered row z={covered_zs[-1]:.2f} mm, "
                    f"expected apex coverage to z≈{stats.z_top_anchor:.2f} mm — "
                    f"uncovered gap {top_gap:.2f} mm "
                    f"({top_gap/stats.expected_row_step:.1f}× row-step)"
                )

        # ── 4. Interior gaps between covered rows ─────────────────────────────
        if len(covered_zs) > 1:
            gaps = [(covered_zs[i+1] - covered_zs[i], i)
                    for i in range(len(covered_zs) - 1)]
            big  = [(g, i) for g, i in gaps
                    if g > stats.expected_row_step * _ROW_GAP_FACTOR]
            if big:
                worst_g, worst_i = max(big, key=lambda x: x[0])
                issues.append(
                    f"INTERIOR GAPS: {len(big)} gap(s) between covered rows "
                    f"> {_ROW_GAP_FACTOR:.0f}× expected step "
                    f"(worst {worst_g:.2f} mm at position {worst_i}, "
                    f"expected step {stats.expected_row_step:.2f} mm)"
                )

        # ── 5. Over-dense rows (row z-step far smaller than expected) ─────────
        if len(stats.rows) > 1:
            row_zs      = [z for z, _, _ in stats.rows]
            actual_steps = [row_zs[i+1] - row_zs[i]
                            for i in range(len(row_zs) - 1)
                            if row_zs[i+1] > row_zs[i]]
            if actual_steps:
                median_step = float(np.median(actual_steps))
                if median_step < stats.expected_row_step * _ROW_STEP_DENSE_FACTOR:
                    issues.append(
                        f"OVER-DENSE ROWS: median z-step {median_step:.3f} mm, "
                        f"expected {stats.expected_row_step:.2f} mm — "
                        f"{stats.expected_row_step/median_step:.1f}× too dense; "
                        f"rows will visually stack"
                    )
                elif median_step > stats.expected_row_step * _ROW_GAP_FACTOR:
                    issues.append(
                        f"UNDER-DENSE ROWS: median z-step {median_step:.3f} mm, "
                        f"expected {stats.expected_row_step:.2f} mm — "
                        f"{median_step/stats.expected_row_step:.1f}× too sparse"
                    )

        # ── 6. Bare zones — contiguous runs of 0/0 rows ──────────────────────
        # A "bare zone" is a contiguous run of rows that produced zero placed
        # leaves spanning more than one expected row step.  We distinguish the
        # cause: if the row had 0 attempts (sec returned None or all perimeter
        # positions were pre-filtered), it appears as (z, 0, 0); if attempts
        # were made but all were skipped (e.g. all downward), it still shows as
        # 0 placed.  Both count as bare; the downward-skip total is shown for
        # context so the caller can judge whether the bare zone is expected.
        # Skip when downward skips dominate — bare zones on the underside of
        # asymmetric objects (foliage clusters, tilted cones) are expected.
        bare_check_ok = stats.skipped_downward < max(1, stats.n_placed)
        if stats.rows and bare_check_ok:
            runs: list[tuple[float, float, int]] = []  # (z_start, z_end, n_rows)
            run_start = None
            run_count = 0
            for z, att, pl in stats.rows:
                if pl == 0:
                    if run_start is None:
                        run_start = z
                    run_z_end = z
                    run_count += 1
                else:
                    if run_start is not None:
                        span = run_z_end - run_start
                        if span >= stats.expected_row_step * 0.9:
                            runs.append((run_start, run_z_end, run_count))
                        run_start = None
                        run_count = 0
            if run_start is not None:          # run at end of list
                span = run_z_end - run_start
                if span >= stats.expected_row_step * 0.9:
                    runs.append((run_start, run_z_end, run_count))

            if runs:
                total_bare_mm = sum(e - s for s, e, _ in runs)
                full_range = stats.rows[-1][0] - stats.rows[0][0]
                pct = total_bare_mm / max(full_range, 1e-3) * 100
                details = "  ".join(
                    f"z={s:.1f}–{e:.1f} ({n} rows)"
                    for s, e, n in runs[:4]
                ) + ("  ..." if len(runs) > 4 else "")
                issues.append(
                    f"BARE ZONES: {len(runs)} run(s) of consecutive empty rows "
                    f"covering {total_bare_mm:.1f} mm ({pct:.0f}% of z-range) — "
                    f"{details}  "
                    f"[down-skips={stats.skipped_downward}]"
                )

        # ── 7. Empty z-bands across the full generated row range ──────────────
        # Divide the full z-range of generated rows (not just covered rows) into
        # bands and check density.  Uses the actual leaf positions, so empty bands
        # within the "should-have-leaves" zone stand out clearly.
        # Skip when n_rows ≤ _N_Z_BANDS: with fewer rows than bands, empty bands
        # are unavoidable by construction and not indicative of a problem.
        if has_leaves and stats.n_rows > _N_Z_BANDS and len(stats.rows) >= 2:
            leaf_zs = bases[:, 2]
            z_lo = stats.rows[0][0]
            z_hi = stats.rows[-1][0]
            if z_hi > z_lo:
                counts, edges = np.histogram(
                    leaf_zs, bins=_N_Z_BANDS, range=(z_lo, z_hi)
                )
                empty = int(np.sum(counts == 0))
                if empty:
                    band_w = (z_hi - z_lo) / _N_Z_BANDS
                    empty_zs = [
                        f"{edges[i]:.1f}–{edges[i+1]:.1f}"
                        for i in range(_N_Z_BANDS) if counts[i] == 0
                    ]
                    issues.append(
                        f"EMPTY Z-BANDS: {empty}/{_N_Z_BANDS} bands "
                        f"({band_w:.2f} mm wide, full range z={z_lo:.1f}–{z_hi:.1f}) "
                        f"have no leaves: {empty_zs[:4]}"
                        f"{'...' if len(empty_zs)>4 else ''}"
                    )

        # ── 8 & 9. Phi-sector coverage ─────────────────────────────────────────
        # Skip for heavily asymmetric objects (>50% downward skips) where large
        # empty sectors on the underside are expected by design.
        phi_check_ok = (stats.skipped_downward < max(1, stats.n_placed))
        if has_leaves and phi_check_ok and stats.n_placed >= _N_PHI_SECTORS:
            phis = np.arctan2(bases[:, 1] - stats.cy, bases[:, 0] - stats.cx)
            sector_counts, _ = np.histogram(
                phis, bins=_N_PHI_SECTORS, range=(-math.pi, math.pi)
            )
            max_s  = int(sector_counts.max())
            empty  = int(np.sum(sector_counts == 0))
            sparse = int(np.sum(
                (sector_counts > 0) & (sector_counts < max_s * _PHI_SPARSE_FACTOR)
            ))
            deg_per = 360 // _N_PHI_SECTORS
            if empty:
                issues.append(
                    f"EMPTY PHI-SECTORS: {empty}/{_N_PHI_SECTORS} "
                    f"{deg_per}° sectors have no leaves "
                    f"(sector counts: {sector_counts.tolist()})"
                )
            if sparse:
                issues.append(
                    f"SPARSE PHI-SECTORS: {sparse}/{_N_PHI_SECTORS} sectors "
                    f"< {_PHI_SPARSE_FACTOR:.0%} of peak ({max_s} leaves)"
                )

        # ── 10. Same-row near-duplicates ─────────────────────────────────────
        if has_leaves:
            dupe_thresh = stats.col_step * _DUPE_FACTOR
            row_to_bases: dict[int, list[np.ndarray]] = {}
            for ridx, b in zip(stats.base_row_idx, stats.base_positions):
                row_to_bases.setdefault(ridx, []).append(b)
            total_dupes = 0
            for ridx, bs in row_to_bases.items():
                if len(bs) < 2:
                    continue
                arr  = np.stack(bs)
                d    = np.linalg.norm(arr[:, np.newaxis] - arr[np.newaxis, :], axis=2)
                np.fill_diagonal(d, np.inf)
                total_dupes += int(np.sum(d < dupe_thresh)) // 2
            if total_dupes:
                issues.append(
                    f"SAME-ROW DUPLICATES: {total_dupes} same-row pairs "
                    f"within {dupe_thresh:.2f} mm ({_DUPE_FACTOR:.0%} of col_step) — "
                    f"likely polygon iterated twice"
                )

        # ── 11. Cross-row stacking (different rows, nearly same 3D position) ──
        # Use 3D distance, not just z-distance: rows near the apex are
        # concentric rings at different radii and are NOT stacked even when
        # their z-values are close.
        if has_leaves and stats.expected_row_step > 0 and len(stats.base_positions) > 1:
            stack_thresh = stats.expected_row_step * _STACK_Z_FACTOR
            d3   = np.linalg.norm(bases[:, np.newaxis] - bases[np.newaxis, :], axis=2)
            diff = row_ids[:, np.newaxis] != row_ids[np.newaxis, :]
            stacked = (d3 < stack_thresh) & diff
            np.fill_diagonal(stacked, False)
            n_stacked = int(np.sum(stacked)) // 2
            if n_stacked:
                min_d3 = float(d3[stacked].min()) if n_stacked else 0.0
                issues.append(
                    f"CROSS-ROW STACKING: {n_stacked} pairs from different rows "
                    f"within {stack_thresh:.2f} mm in 3D "
                    f"(min dist={min_d3:.3f} mm, expected row step "
                    f"{stats.expected_row_step:.2f} mm)"
                )

        # ── 12. Long roots ────────────────────────────────────────────────────
        if stats.root_depths:
            long = [d for d in stats.root_depths if d > _ROOT_DEPTH_MAX_MM]
            if long:
                issues.append(
                    f"LONG ROOTS: {len(long)}/{len(stats.root_depths)} leaves "
                    f"wall depth > {_ROOT_DEPTH_MAX_MM:.1f} mm "
                    f"(max={max(long):.2f} mm, "
                    f"median={float(np.median(stats.root_depths)):.2f} mm)"
                )

        # ── 13. Floating leaves ───────────────────────────────────────────────
        # Max distance of any OUTSIDE curl-region vertex from the mesh surface.
        # Arch + ca=0 at the apex can lift the leaf body well above the surface
        # even when lift_mm=0; unsigned-distance min() misses this because some
        # curl-region vertices are still near the sphere surface.
        if stats.leaf_float_dists:
            floaters = [d for d in stats.leaf_float_dists
                        if d > _FLOATING_LEAF_CURL_DIST_MM]
            if floaters:
                issues.append(
                    f"FLOATING LEAVES: {len(floaters)}/{len(stats.leaf_float_dists)} "
                    f"leaves have curl-region outside-dist > {_FLOATING_LEAF_CURL_DIST_MM:.2f} mm "
                    f"(max={max(floaters):.2f} mm, "
                    f"median={float(np.median(stats.leaf_float_dists)):.2f} mm)"
                )

        # ── 14. Buried leaves ─────────────────────────────────────────────────
        # Max depth of any curl-region vertex that lies INSIDE the mesh.
        # A steep contact angle can push the arched leaf body through the
        # parent sphere/cluster surface at intermediate stations.
        if stats.leaf_buried_depths:
            buried = [d for d in stats.leaf_buried_depths
                      if d > _BURIED_LEAF_CURL_DEPTH_MM]
            if buried:
                issues.append(
                    f"BURIED LEAVES: {len(buried)}/{len(stats.leaf_buried_depths)} "
                    f"leaves have curl-region inside-depth > {_BURIED_LEAF_CURL_DEPTH_MM:.2f} mm "
                    f"(max={max(buried):.2f} mm, "
                    f"median={float(np.median(stats.leaf_buried_depths)):.2f} mm)"
                )

        # ── 15. Top-row spread ────────────────────────────────────────────────
        # The topmost row of leaves should converge near the mesh apex — their
        # base positions should form a small ring.  A large minimum width
        # indicates the top row is too far down or the apex anchor is off.
        if has_leaves:
            top_ridx  = int(max(stats.base_row_idx))
            top_bases = np.stack([
                b for b, r in zip(stats.base_positions, stats.base_row_idx)
                if r == top_ridx
            ])
            spread = _min_width_xy(top_bases)
            if spread > _TOP_ROW_SPREAD_MAX_MM:
                issues.append(
                    f"TOP ROW SPREAD: top row (idx={top_ridx}, "
                    f"n={len(top_bases)} leaves) minimum width {spread:.1f} mm "
                    f"(threshold {_TOP_ROW_SPREAD_MAX_MM:.0f} mm) — "
                    f"top-row bases not converging near apex"
                )

        # ── Print per-object summary ──────────────────────────────────────────
        status = "PASS" if not issues else "FAIL"
        total_issues += len(issues)

        print(f"\n[{status}] {stats.label}")
        row_zs_str = (
            f"  z_range=[{stats.rows[0][0]:.2f}, {stats.rows[-1][0]:.2f}]  "
            f"z_top_anchor={stats.z_top_anchor:.2f}  "
            f"expected_step={stats.expected_row_step:.2f} mm"
            if stats.rows else ""
        )
        if row_zs_str:
            print(row_zs_str)
        print(
            f"  placed={stats.n_placed}  rows={stats.n_rows}  "
            f"attempted={stats.n_attempted}  "
            f"skip(down={stats.skipped_downward} r={stats.skipped_small_r} "
            f"ca={stats.skipped_ca})  errors={stats.build_errors}"
        )

        # Row-by-row coverage bar: z | pl/att
        if stats.rows:
            row_bar = "  rows: " + "  ".join(
                f"z={z:.1f}:{pl}/{att}" for z, att, pl in stats.rows
            )
            print(row_bar)

        if stats.root_depths:
            print(
                f"  root depth mm: "
                f"min={min(stats.root_depths):.2f}  "
                f"median={float(np.median(stats.root_depths)):.2f}  "
                f"max={max(stats.root_depths):.2f}"
            )
        if stats.leaf_float_dists:
            n_float  = sum(1 for d in stats.leaf_float_dists  if d > _FLOATING_LEAF_CURL_DIST_MM)
            n_buried = sum(1 for d in stats.leaf_buried_depths if d > _BURIED_LEAF_CURL_DEPTH_MM)
            print(
                f"  curl-region outside-dist mm: "
                f"median={float(np.median(stats.leaf_float_dists)):.2f}  "
                f"p90={float(np.percentile(stats.leaf_float_dists, 90)):.2f}  "
                f"max={max(stats.leaf_float_dists):.2f}  "
                f"floating={n_float}/{len(stats.leaf_float_dists)}"
            )
            print(
                f"  curl-region inside-depth mm: "
                f"median={float(np.median(stats.leaf_buried_depths)):.2f}  "
                f"p90={float(np.percentile(stats.leaf_buried_depths, 90)):.2f}  "
                f"max={max(stats.leaf_buried_depths):.2f}  "
                f"buried={n_buried}/{len(stats.leaf_buried_depths)}"
            )
        if has_leaves:
            top_ridx  = int(max(stats.base_row_idx))
            top_bases = np.stack([
                b for b, r in zip(stats.base_positions, stats.base_row_idx)
                if r == top_ridx
            ])
            print(
                f"  top-row (idx={top_ridx}, n={len(top_bases)}): "
                f"min-width={_min_width_xy(top_bases):.1f} mm  "
                f"(thresh {_TOP_ROW_SPREAD_MAX_MM:.0f} mm)"
            )

        for issue in issues:
            print(f"  ✗ {issue}")
        if not issues:
            print("  ✓ No artifacts detected")

    print(f"\n{'=' * 64}")
    print(f"Total issues: {total_issues}  ({'PASS' if total_issues == 0 else 'FAIL'})")
    print(f"{'=' * 64}\n")
    return total_issues


# ── PNG renders ───────────────────────────────────────────────────────────────

def _render_views(all_parts: list[trimesh.Trimesh], stl_path: Path) -> None:
    """Render three camera angles as PNGs alongside the STL."""
    try:
        from dharmatiles.render import render as _render
    except ImportError:
        print("  (pyrender not available — skipping PNG render)")
        return

    stem = stl_path.parent / stl_path.stem
    views = [
        (35.0, -135.0, "perspective", "leaf-placement (perspective)"),
        (85.0,    0.0, "top",         "leaf-placement (top view)"),
        ( 5.0,    0.0, "side",        "leaf-placement (side view)"),
    ]
    print("\nRendering PNG views …")
    for elev, azim, suffix, label in views:
        out_png = stl_path.parent / f"{stl_path.stem}-{suffix}.png"
        try:
            _render(
                all_parts,
                out_png,
                elev=elev,
                azim=azim,
                resolution=(1600, 900),
                quiet=False,
                label=label,
            )
        except Exception as exc:
            print(f"  (render failed for {suffix}: {exc})")


# ── Cluster builder ───────────────────────────────────────────────────────────

def _make_cluster_parts(
    cx: float,
    cy: float,
    z_tip: float,
    tip_t: np.ndarray,
    start_t: np.ndarray,
    edge_id: int,
    label: str,
) -> tuple[list[trimesh.Trimesh], _PlacementStats]:
    """Build a foliage cluster (shape only) then place row-coloured leaves.

    Returns (mesh_parts, stats).
    """
    tip_t_n   = _safe_norm(np.asarray(tip_t,   float))
    start_t_n = _safe_norm(np.asarray(start_t, float))
    tip_p     = np.array([cx, cy, z_tip], float)
    start_p   = tip_p - float(_CLUSTER["clump_length_mm"]) * tip_t_n

    print(f"\n[Cluster {edge_id}: {label}]")
    print(f"  tip_t  = [{tip_t_n[0]:.3f}, {tip_t_n[1]:.3f}, {tip_t_n[2]:.3f}]"
          f"  ({math.degrees(math.acos(float(tip_t_n[2]))):.1f}° from vertical)")
    print(f"  tip_pos={[f'{v:.1f}' for v in tip_p]}  "
          f"start_pos={[f'{v:.1f}' for v in start_p]}")

    cluster, _ = _build_foliage_cluster_mesh(
        tip_pos       = tip_p,
        tip_tangent   = tip_t_n,
        start_pos     = start_p,
        start_tangent = start_t_n,
        edge_id       = edge_id,
        bark_seed     = 42,
        **_CLUSTER,
    )
    _color_tag(cluster, Material.FOLIAGE)

    leaves, stats = _place_leaves_on_mesh(cluster, seed=edge_id, label=label)
    print(f"  -> {len(leaves)} leaves  "
          f"(cluster: {len(cluster.vertices):,}v / {len(cluster.faces):,}f)")
    return [cluster] + leaves, stats


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    default_out = (
        Path(__file__).parents[2] / "stl" / "test" / "leaf-placement-test.stl"
    )
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else default_out
    out.parent.mkdir(parents=True, exist_ok=True)

    t0 = time.perf_counter()
    all_parts: list[trimesh.Trimesh] = []
    all_stats: list[_PlacementStats] = []

    # ── Object 1: sphere ─────────────────────────────────────────────────────
    print("\n=== Object 1: Sphere r=10 mm at XY=(0, 0) ===")
    sphere = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    sphere.vertices[:, 2] += 10.0   # raise centre from origin to z=10
    sphere.fix_normals()
    _color_tag(sphere, Material.FOLIAGE)
    all_parts.append(sphere)

    sphere_leaves, sphere_stats = _place_leaves_on_mesh(sphere, seed=0, label="sphere")
    sphere_stats.label = "Object 1 — sphere r=10"
    print(f"  -> {len(sphere_leaves)} leaves")
    all_parts.extend(sphere_leaves)
    all_stats.append(sphere_stats)

    # ── Object 2: near-vertical cluster ──────────────────────────────────────
    print("\n=== Object 2: Cluster A — near-vertical ===")
    parts2, stats2 = _make_cluster_parts(
        cx=40.0, cy=0.0, z_tip=30.0,
        tip_t   = [0.0, 0.0, 1.0],
        start_t = [0.0, 0.0, 1.0],
        edge_id = 1,
        label   = "vertical (0°)",
    )
    stats2.label = "Object 2 — cluster A (0° tilt)"
    all_parts.extend(parts2)
    all_stats.append(stats2)

    # ── Object 3: 30° tilted cluster ─────────────────────────────────────────
    print("\n=== Object 3: Cluster B — 30° tilt ===")
    a30 = math.radians(30)
    parts3, stats3 = _make_cluster_parts(
        cx=80.0, cy=0.0, z_tip=30.0,
        tip_t   = [math.sin(a30), 0.0, math.cos(a30)],
        start_t = [math.sin(a30), 0.0, math.cos(a30)],
        edge_id = 2,
        label   = "30° tilt",
    )
    stats3.label = "Object 3 — cluster B (30° tilt)"
    all_parts.extend(parts3)
    all_stats.append(stats3)

    # ── Object 4: 58° tilt with curved spine ─────────────────────────────────
    print("\n=== Object 4: Cluster C — 58° tilt, curved spine ===")
    a58 = math.radians(58)
    parts4, stats4 = _make_cluster_parts(
        cx=120.0, cy=0.0, z_tip=30.0,
        tip_t   = [math.sin(a58), 0.0, math.cos(a58)],
        start_t = [math.sin(a30), 0.0, math.cos(a30)],  # different start → curve
        edge_id = 3,
        label   = "58° tilt, curved spine",
    )
    stats4.label = "Object 4 — cluster C (58° tilt, curved)"
    all_parts.extend(parts4)
    all_stats.append(stats4)

    # ── Export STL ────────────────────────────────────────────────────────────
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

    # ── Artifact detection ────────────────────────────────────────────────────
    _check_artifacts(all_stats)

    # ── PNG renders ───────────────────────────────────────────────────────────
    _render_views(all_parts, out)

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
