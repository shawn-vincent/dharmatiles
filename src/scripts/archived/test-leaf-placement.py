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

import math
import sys
import time
from pathlib import Path

import numpy as np
import trimesh

import dharmatiles.trees.leaf as _leaf_mod

from dharmatiles.trees import (
    LeafPlacementStats,
    effective_ring_perimeter,
    min_width_xy,
    place_leaves_on_mesh,
)
from dharmatiles.trees._utils import _safe_norm
from dharmatiles.trees.mesh import _build_foliage_cluster_mesh

# ── Colour system ─────────────────────────────────────────────────────────────
from dharmatiles.core.color import (
    DEBUG_COLORS,
    Material,
    RGBA,
    RGBA_FLAG_FAIL,
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

# Extra float margin above lift_mm: maximum additional distance (mm) of any
# outside curl-region vertex from the mesh surface beyond what lift_mm explains.
# The arch typically adds ~0.5 mm on top of lift; 1.0 mm gives headroom.
# Full threshold per leaf run = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM.
_FLOATING_LEAF_EXTRA_MM = 1.0

# Buried-leaf threshold: maximum depth (mm) any curl-region vertex may penetrate
# inside the parent mesh before the leaf is considered buried.
# The root embedding stays inside L/2 from the base, so curl-region vertices
# inside the mesh are always unintentional burial.
# Set to ~1× leaf thickness (0.24 mm) so shallow tip burial (e.g. apex leaf
# arching back into the sphere at ~0.35 mm) is caught.
_BURIED_LEAF_CURL_DEPTH_MM = 0.25

# Top-row spread: maximum acceptable minimum-width (mm) of the XY convex hull
# of the topmost row's base positions.  A large spread means the top row is
# far from the apex rather than converging near the tip.
_TOP_ROW_SPREAD_MAX_MM = 1.0

# Upward-pointing tangent: on any convex mesh where the upper surface is covered
# (up_hint.z > 0), the leaf tangent should have a non-positive z-component —
# leaves grow outward and downward, not toward the apex.  A tangent.z above this
# threshold indicates the leaf is pointing the wrong way (upward into the mesh).
_UPWARD_TANGENT_Z_THRESHOLD = 0.1


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
    leaf_lift_mm        = 1.5,
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
    **_LEAF,
)

# ── Shared placement kwargs built from _LEAF ──────────────────────────────────

_PLACE_KW = dict(
    length_mm      = float(_LEAF["leaf_length_mm"]),
    width_mm       = float(_LEAF["leaf_width_mm"]),
    thickness_mm   = float(_LEAF["leaf_thickness_mm"]),
    fold_angle_deg = float(_LEAF["leaf_fold_angle_deg"]),
    inner_curve    = float(_LEAF["leaf_inner_curve"]),
    outer_curve    = float(_LEAF["leaf_outer_curve"]),
    curl_deg       = float(_LEAF["leaf_curl_deg"]),
    lift_mm        = float(_LEAF["leaf_lift_mm"]),
    h_overlap      = float(_LEAF["leaf_h_overlap"]),
    v_overlap      = float(_LEAF["leaf_v_overlap"]),
    n_meridians      = int(_LEAF["leaf_arc_meridians"]),
    z_samples        = int(_LEAF["leaf_arc_z_samples"]),
    angle_jitter_deg = 24.0,
    pos_jitter       = 0.165,
)


# ── Artifact analysis and reporting ──────────────────────────────────────────

_CONE_LEAF_HALF_DEG = 60.0   # max angle from -local_n  (= min 30° from leaf plane)
_CONE_FDM_HALF_DEG  = 60.0   # FDM overhang limit
_CONE_MESH_HALF_DEG = 90.0   # parent mesh — wide because mesh subtends ~hemisphere from tip
_CONE_N_SAMPLES     = 1000   # uniform sphere samples for intersection check
_CONE_C2_AXIS       = np.array([0.0, 0.0, -1.0])


def _cone_analysis(cone_data: list[dict]) -> None:
    """Report cone intersection geometry and timing for tip-vertex rays."""
    rng = np.random.default_rng(42)
    raw = rng.standard_normal((_CONE_N_SAMPLES, 3))
    sph = raw / np.linalg.norm(raw, axis=1, keepdims=True)   # (N, 3) unit vectors

    cos1 = math.cos(math.radians(_CONE_LEAF_HALF_DEG))
    cos2 = math.cos(math.radians(_CONE_FDM_HALF_DEG))
    cos3 = math.cos(math.radians(_CONE_MESH_HALF_DEG))

    n_empty = 0
    n_p12_fail = n_p13_fail = n_p23_fail = 0
    frac_list: list[float] = []
    ang1_all: list[np.ndarray] = []
    ang2_all: list[np.ndarray] = []
    ang3_all: list[np.ndarray] = []
    total_raycast = 0.0

    # Geometry of empty cases
    empty_fdm_mesh_angles: list[float] = []   # angle between c2 and c3 for empties
    empty_c3_z: list[float] = []              # c3_axis z-component (> 0 = mesh faces up)
    empty_blocked_by_undercut: int = 0        # empties that clear when undercut removed

    for d in cone_data:
        c1  = d['c1_axis']    # -local_n[tip]
        c3  = d['c3_axis']    # parent mesh inward normal at nearest point
        inw = d['inward_tip'] # undercut half-plane normal

        # Pairwise non-emptiness
        a12 = math.degrees(math.acos(float(np.clip(c1 @ _CONE_C2_AXIS, -1, 1))))
        a13 = math.degrees(math.acos(float(np.clip(c1 @ c3,            -1, 1))))
        a23 = math.degrees(math.acos(float(np.clip(_CONE_C2_AXIS @ c3, -1, 1))))
        if a12 >= _CONE_LEAF_HALF_DEG + _CONE_FDM_HALF_DEG:  n_p12_fail += 1
        if a13 >= _CONE_LEAF_HALF_DEG + _CONE_MESH_HALF_DEG: n_p13_fail += 1
        if a23 >= _CONE_FDM_HALF_DEG  + _CONE_MESH_HALF_DEG: n_p23_fail += 1

        # Triple intersection via sampling
        in_c1    = (sph @ c1)            >= cos1
        undercut = (sph @ inw)           >= 0.0
        in_c2    = (sph @ _CONE_C2_AXIS) >= cos2
        in_c3    = (sph @ c3)            >= cos3
        feasible = in_c1 & undercut & in_c2 & in_c3

        n_feas = int(feasible.sum())
        if n_feas == 0:
            n_empty += 1
            empty_fdm_mesh_angles.append(a23)
            empty_c3_z.append(float(c3[2]))
            # Would it be feasible if we dropped the undercut constraint?
            if int((in_c1 & in_c2 & in_c3).sum()) > 0:
                empty_blocked_by_undercut += 1
        else:
            frac_list.append(n_feas / _CONE_N_SAMPLES)
            feas = sph[feasible]
            ang1_all.append(np.degrees(np.arccos(np.clip(feas @ c1,            -1, 1))))
            ang2_all.append(np.degrees(np.arccos(np.clip(feas @ _CONE_C2_AXIS, -1, 1))))
            ang3_all.append(np.degrees(np.arccos(np.clip(feas @ c3,            -1, 1))))

        total_raycast += d['raycast_time']

    n   = len(cone_data)
    a1  = np.concatenate(ang1_all) if ang1_all else np.array([])
    a2  = np.concatenate(ang2_all) if ang2_all else np.array([])
    a3  = np.concatenate(ang3_all) if ang3_all else np.array([])
    efm = np.array(empty_fdm_mesh_angles)
    ecz = np.array(empty_c3_z)

    def _stat(arr: np.ndarray, unit: str = "°") -> str:
        if len(arr) == 0:
            return "(no data)"
        return (f"min={arr.min():.1f}{unit}  p25={float(np.percentile(arr,25)):.1f}{unit}"
                f"  median={float(np.median(arr)):.1f}{unit}"
                f"  p75={float(np.percentile(arr,75)):.1f}{unit}  max={arr.max():.1f}{unit}")

    print("\n" + "=" * 64)
    print("CONE INTERSECTION ANALYSIS  (tip vertex, per placed leaf)")
    print(f"  n={n}  samples={_CONE_N_SAMPLES}")
    print(f"  cone half-angles: leaf={_CONE_LEAF_HALF_DEG}°  "
          f"FDM={_CONE_FDM_HALF_DEG}°  mesh={_CONE_MESH_HALF_DEG}°")
    print(f"\n  Pairwise empty intersections:")
    print(f"    leaf ∩ FDM  : {n_p12_fail}/{n}")
    print(f"    leaf ∩ mesh : {n_p13_fail}/{n}")
    print(f"    FDM  ∩ mesh : {n_p23_fail}/{n}")
    print(f"\n  Triple intersection (all 3 cones + undercut half-plane):")
    print(f"    empty                          : {n_empty}/{n}")
    if n_empty:
        n_fw = int(np.sum(efm >= _CONE_FDM_HALF_DEG + _CONE_MESH_HALF_DEG))
        print(f"    ↳ caused by FDM∩mesh pairwise  : {n_fw}/{n_empty}")
        print(f"    ↳ cleared by dropping undercut : {empty_blocked_by_undercut}/{n_empty}")
        print(f"    ↳ remaining (geometry only)    : "
              f"{n_empty - n_fw - empty_blocked_by_undercut}/{n_empty}")
        print(f"\n  Empty-case geometry:")
        print(f"    FDM∩mesh angle (c2·c3)   : {_stat(efm)}")
        print(f"    mesh inward-normal z-comp : {_stat(ecz, '')}")
        n_up   = int(np.sum(ecz > 0.0))
        n_horiz = int(np.sum(np.abs(ecz) < 0.3))
        print(f"    mesh facing upward (c3_z>0)  : {n_up}/{n_empty}")
        print(f"    mesh near-horizontal (|z|<0.3): {n_horiz}/{n_empty}")
    if frac_list:
        fracs = np.array(frac_list)
        print(f"\n  Feasible (non-empty) intersection coverage:")
        print(f"    fraction of sphere : {_stat(fracs, '')}")
    print(f"\n  Angle ranges within intersection (non-empty leaves):")
    print(f"    from leaf-surface axis (-local_n) : {_stat(a1)}")
    print(f"    from FDM axis ([0,0,-1])           : {_stat(a2)}")
    print(f"    from mesh-normal axis              : {_stat(a3)}")
    mr = total_raycast * 1000
    print(f"\n  Timing — NP=24 raycast across {n} leaves:")
    print(f"    {mr:.1f} ms total  {mr/n:.3f} ms/leaf")
    print(f"  (BVH nearest-point query is now in the main solidify path)")
    print("=" * 64)


def _check_artifacts(all_stats: list[LeafPlacementStats]) -> int:
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
            _, n_dupe_pairs = _same_row_duplicate_indices(stats)
            if n_dupe_pairs:
                issues.append(
                    f"SAME-ROW DUPLICATES: {n_dupe_pairs} same-row pairs "
                    f"midpoints within {stats.col_step * _DUPE_FACTOR:.2f} mm "
                    f"({_DUPE_FACTOR:.0%} of col_step) — "
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
                    f"max wall depth > {_ROOT_DEPTH_MAX_MM:.1f} mm "
                    f"(max={max(long):.2f} mm, "
                    f"median={float(np.median(stats.root_depths)):.2f} mm)"
                )

        # ── 13. Floating leaves ───────────────────────────────────────────────
        # Max distance of any OUTSIDE curl-region vertex from the mesh surface.
        # Arch + ca=0 at the apex can lift the leaf body well above the surface
        # even when lift_mm=0; unsigned-distance min() misses this because some
        # curl-region vertices are still near the sphere surface.
        if stats.leaf_float_dists:
            _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
            floaters = [d for d in stats.leaf_float_dists
                        if d > _float_thresh]
            if floaters:
                issues.append(
                    f"FLOATING LEAVES: {len(floaters)}/{len(stats.leaf_float_dists)} "
                    f"leaves have curl-region outside-dist > {_float_thresh:.2f} mm "
                    f"(lift={stats.lift_mm:.2f}+extra={_FLOATING_LEAF_EXTRA_MM:.2f}) "
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
            spread = min_width_xy(top_bases)
            if spread > _TOP_ROW_SPREAD_MAX_MM:
                issues.append(
                    f"TOP ROW SPREAD: top row (idx={top_ridx}, "
                    f"n={len(top_bases)} leaves) minimum width {spread:.1f} mm "
                    f"(threshold {_TOP_ROW_SPREAD_MAX_MM:.0f} mm) — "
                    f"top-row bases not converging near apex"
                )

        # ── 16. Per-row effective horizontal overlap ──────────────────────────
        # Compute the path length traced by actual leaf midpoints (base + L/2
        # along tangent) sorted by angle around the centroid.  No shape
        # assumption — works correctly for spheres, cylinders, and tilted
        # clusters.
        # true_hov = 1 - (eff_perim / n_placed) / W
        #   > 0  → leaf bodies overlap
        #   = 0  → leaf bodies exactly touch
        #   < 0  → gap between leaf bodies (negative = wasted space)
        L = stats.leaf_length_mm
        W = stats.leaf_width_mm
        if W > 0 and has_leaves:
            spare_rows = []
            row_ids_arr = np.array(stats.base_row_idx)
            for row_i, (z, att, pl) in enumerate(stats.rows):
                if pl < 2:
                    continue
                mask = np.where(row_ids_arr == row_i)[0]
                row_bases = [stats.base_positions[i] for i in mask]
                row_tangs = [stats.base_tangents[i]  for i in mask]
                rp = stats.row_perims[row_i] if row_i < len(stats.row_perims) else 0.0
                eff_perim = effective_ring_perimeter(
                    row_bases, row_tangs, L, stats.cx, stats.cy
                )
                if eff_perim <= 0:
                    continue
                eff_step  = eff_perim / pl
                true_hov  = 1.0 - eff_step / W
                total_gap = eff_perim - pl * W
                extra     = int(math.floor(eff_perim / W)) - pl
                if extra > 0:
                    spare_rows.append((z, pl, rp, eff_perim, true_hov, total_gap, extra))
            if spare_rows:
                issues.append(
                    f"UNDERFILLED RINGS: {len(spare_rows)} row(s) could fit "
                    f"additional leaves — "
                    + "; ".join(
                        f"z={z:.1f} has {pl} leaves "
                        f"(base_perim={rp:.1f} eff_perim={ep:.1f} mm, "
                        f"true_hov={true_hov:+.2f}, gap={total_gap:.1f} mm, room for +{extra})"
                        for z, pl, rp, ep, true_hov, total_gap, extra in spare_rows
                    )
                )

        # ── 17. Upward-pointing tangents ──────────────────────────────────────
        # On the upper hemisphere of a convex mesh (where leaves are placed),
        # the leaf tangent should have tangent.z ≤ 0 — leaves droop outward and
        # downward, never toward the apex.  A positive tangent.z means the leaf
        # is growing upward into the top of the mesh (wrong direction).
        if has_leaves:
            tang_arr    = np.stack(stats.base_tangents)       # (N, 3)
            upward_mask = tang_arr[:, 2] > _UPWARD_TANGENT_Z_THRESHOLD
            if upward_mask.any():
                up_row_ids = np.array(stats.base_row_idx)[upward_mask]
                affected   = sorted(set(int(r) for r in up_row_ids))
                worst_z    = float(tang_arr[upward_mask, 2].max())
                issues.append(
                    f"UPWARD TANGENTS: {int(upward_mask.sum())} leaves "
                    f"with tangent.z > {_UPWARD_TANGENT_Z_THRESHOLD:.2f} "
                    f"(growing toward apex instead of away) — "
                    f"rows {affected[:8]}"
                    + ("..." if len(affected) > 8 else "")
                    + f"  worst tangent.z={worst_z:.3f}"
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
            f"preburied={stats.skipped_preburied} floor={stats.skipped_below_floor})  "
            f"contact_angle_clamped={stats.contact_angle_clamped}  errors={stats.build_errors}"
        )

        # Row-by-row coverage bar: z | pl/att
        if stats.rows:
            row_bar = "  rows: " + "  ".join(
                f"z={z:.1f}:{pl}/{att}" for z, att, pl in stats.rows
            )
            print(row_bar)

        # Per-row effective overlap table (actual midpoint ring path length)
        L = stats.leaf_length_mm
        W = stats.leaf_width_mm
        if W > 0 and has_leaves:
            parts_line = []
            row_ids_arr2 = np.array(stats.base_row_idx)
            for row_i, (z, att, pl) in enumerate(stats.rows):
                if pl < 2:
                    continue
                mask = np.where(row_ids_arr2 == row_i)[0]
                row_bases = [stats.base_positions[i] for i in mask]
                row_tangs = [stats.base_tangents[i]  for i in mask]
                rp = stats.row_perims[row_i] if row_i < len(stats.row_perims) else 0.0
                eff_perim = effective_ring_perimeter(
                    row_bases, row_tangs, L, stats.cx, stats.cy
                )
                if eff_perim <= 0:
                    continue
                true_hov = 1.0 - (eff_perim / pl) / W
                parts_line.append(
                    f"z={z:.1f}:hov={true_hov:+.2f}"
                    f"(base={rp:.1f} eff={eff_perim:.1f} n={pl})"
                )
            if parts_line:
                print("  eff-hov: " + "  ".join(parts_line))

        if stats.root_depths:
            print(
                f"  max wall depth mm: "
                f"min={min(stats.root_depths):.2f}  "
                f"median={float(np.median(stats.root_depths)):.2f}  "
                f"max={max(stats.root_depths):.2f}"
            )
        if stats.leaf_float_dists:
            _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
            n_float  = sum(1 for d in stats.leaf_float_dists  if d > _float_thresh)
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
                f"min-width={min_width_xy(top_bases):.1f} mm  "
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


# ── Error leaf colouring ──────────────────────────────────────────────────────

def _same_row_duplicate_indices(
    stats: LeafPlacementStats,
) -> tuple[set[int], int]:
    """Return (leaf_indices_in_duplicate_pairs, n_pairs) for same-row duplicates.

    Overlap is measured at the leaf midpoint (base + L/2 * tangent) — the widest
    part of the leaf.  Leaves fanning from a shared base in different directions
    have well-separated midpoints and are not flagged.  True algorithm duplicates
    (polygon iterated twice → identical tangent) have coincident midpoints.
    """
    if not stats.base_positions:
        return set(), 0
    half_L = stats.leaf_length_mm / 2.0
    dupe_thresh = stats.col_step * _DUPE_FACTOR
    row_to_items: dict[int, list[tuple[int, np.ndarray]]] = {}
    for i, (ridx, b, t) in enumerate(
        zip(stats.base_row_idx, stats.base_positions, stats.base_tangents)
    ):
        mid = b + half_L * t
        row_to_items.setdefault(ridx, []).append((i, mid))
    dupe_set: set[int] = set()
    n_pairs = 0
    for items in row_to_items.values():
        if len(items) < 2:
            continue
        global_idxs = [x[0] for x in items]
        arr = np.stack([x[1] for x in items])
        d   = np.linalg.norm(arr[:, np.newaxis] - arr[np.newaxis, :], axis=2)
        np.fill_diagonal(d, np.inf)
        is_dupe = d < dupe_thresh
        n_pairs += int(np.sum(is_dupe)) // 2
        for li, lj in zip(*np.where(is_dupe)):
            if li < lj:
                dupe_set.add(global_idxs[int(li)])
                dupe_set.add(global_idxs[int(lj)])
    return dupe_set, n_pairs


def _mark_error_leaves(
    leaves: list[trimesh.Trimesh],
    stats: LeafPlacementStats,
) -> None:
    """Re-colour any leaf that exceeds an error threshold to red (RGBA_FLAG_FAIL).

    Covers: long root, floating curl-region, buried curl-region, upward tangent,
    same-row duplicate.
    Leaves whose index can't be matched to the stats lists are skipped silently.
    """
    if len(leaves) != len(stats.root_depths):
        return
    dupe_set, _ = _same_row_duplicate_indices(stats)
    error_color = np.asarray(RGBA_FLAG_FAIL, dtype=np.uint8)
    _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
    for i, leaf in enumerate(leaves):
        is_error = (
            stats.root_depths[i]       > _ROOT_DEPTH_MAX_MM
            or stats.leaf_float_dists[i]   > _float_thresh
            or stats.leaf_buried_depths[i] > _BURIED_LEAF_CURL_DEPTH_MM
            or float(stats.base_tangents[i][2]) > _UPWARD_TANGENT_Z_THRESHOLD
            or i in dupe_set
        )
        if is_error:
            leaf.visual = trimesh.visual.ColorVisuals(
                mesh=leaf,
                face_colors=np.tile(error_color, (len(leaf.faces), 1)),
            )


# ── Cluster builder ───────────────────────────────────────────────────────────

def _make_cluster_parts(
    cx: float,
    cy: float,
    z_tip: float,
    tip_t: np.ndarray,
    start_t: np.ndarray,
    edge_id: int,
    label: str,
) -> tuple[list[trimesh.Trimesh], LeafPlacementStats]:
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

    leaves, stats = place_leaves_on_mesh(
        cluster, **_PLACE_KW, seed=edge_id, label=label, row_color_fn=_row_rgba,
    )
    _mark_error_leaves(leaves, stats)
    print(f"  -> {len(leaves)} leaves  "
          f"(cluster: {len(cluster.vertices):,}v / {len(cluster.faces):,}f)")
    return [cluster] + leaves, stats


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    import argparse
    parser = argparse.ArgumentParser(description="Leaf placement test")
    parser.add_argument("out", nargs="?", help="Output STL path")
    parser.add_argument(
        "--no-jitter", action="store_true",
        help="Set angle_jitter_deg and pos_jitter to zero",
    )
    parser.add_argument(
        "--no-overlap", action="store_true",
        help="Set h_overlap and v_overlap to zero",
    )
    args = parser.parse_args()

    default_out = (
        Path(__file__).parents[2] / "stl" / "test" / "leaf-placement-test.stl"
    )
    out = Path(args.out) if args.out else default_out
    out.parent.mkdir(parents=True, exist_ok=True)

    if args.no_jitter:
        _PLACE_KW["angle_jitter_deg"] = 0.0
        _PLACE_KW["pos_jitter"]       = 0.0
        print("Jitter disabled (angle_jitter_deg=0, pos_jitter=0)")
    if args.no_overlap:
        _PLACE_KW["h_overlap"] = 0.0
        _PLACE_KW["v_overlap"] = 0.0
        print("Overlap disabled (h_overlap=0, v_overlap=0)")

    t0 = time.perf_counter()
    all_parts: list[trimesh.Trimesh] = []
    all_stats: list[LeafPlacementStats] = []

    _leaf_mod._DEBUG_RAY_DIRS_COLLECTOR = []
    _leaf_mod._DEBUG_TIP_RAY_COLLECTOR  = []
    _leaf_mod._DEBUG_CONE_COLLECTOR     = []

    # ── Object 1: sphere ─────────────────────────────────────────────────────
    print("\n=== Object 1: Sphere r=10 mm at XY=(0, 0) ===")
    sphere = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    sphere.vertices[:, 2] += 10.0   # raise centre from origin to z=10
    sphere.fix_normals()
    _color_tag(sphere, Material.FOLIAGE)
    all_parts.append(sphere)

    sphere_leaves, sphere_stats = place_leaves_on_mesh(
        sphere, **_PLACE_KW, seed=0, label="sphere", row_color_fn=_row_rgba,
    )
    _mark_error_leaves(sphere_leaves, sphere_stats)
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

    # ── Ray angle distribution (angle vs world Z-down = [0,0,-1]) ────────────
    _ray_dirs_all = _leaf_mod._DEBUG_RAY_DIRS_COLLECTOR
    _tip_ray_all  = _leaf_mod._DEBUG_TIP_RAY_COLLECTOR
    _cone_data    = _leaf_mod._DEBUG_CONE_COLLECTOR
    _leaf_mod._DEBUG_RAY_DIRS_COLLECTOR = None
    _leaf_mod._DEBUG_TIP_RAY_COLLECTOR  = None
    _leaf_mod._DEBUG_CONE_COLLECTOR     = None
    if _ray_dirs_all:
        all_rd = np.concatenate(_ray_dirs_all, axis=0)          # (N_total, 3)
        # angle between ray and [0,0,-1]: cos(θ) = -ray_z (since dot([0,0,-1], r) = -r_z)
        cos_theta = np.clip(-all_rd[:, 2], -1.0, 1.0)
        angles_deg = np.degrees(np.arccos(cos_theta))
        pcts = [0, 10, 25, 50, 75, 90, 100]
        percs = np.percentile(angles_deg, pcts)
        print("\n" + "=" * 64)
        print("RAY ANGLE vs WORLD Z-DOWN  (0° = pointing straight down)")
        print("=" * 64)
        print(f"  samples : {len(angles_deg):,}  ({len(_ray_dirs_all)} leaves × {len(_ray_dirs_all[0])} perimeter vertices)")
        print(f"  min     : {angles_deg.min():.1f}°")
        print(f"  max     : {angles_deg.max():.1f}°")
        print(f"  mean    : {angles_deg.mean():.1f}°")
        for p, v in zip(pcts, percs):
            print(f"  p{p:<3}    : {v:.1f}°")
        # Histogram in 15° buckets
        bins = np.arange(0, 181, 15)
        counts, _ = np.histogram(angles_deg, bins=bins)
        print(f"\n  Histogram (15° buckets):")
        for lo, hi, c in zip(bins, bins[1:], counts):
            bar = "█" * int(c / max(counts) * 30)
            print(f"    {lo:3}–{hi:3}°  {bar}  {c:,}")
        print("=" * 64)

    # ── Tip-ray angle split: hit vs miss/long-root ────────────────────────────
    if _tip_ray_all:
        def _tip_angle(rd: np.ndarray) -> float:
            return float(np.degrees(np.arccos(np.clip(-rd[2], -1.0, 1.0))))

        hit_entries   = [(rd, d, ins) for rd, d, ins in _tip_ray_all if not np.isnan(d)]
        miss_entries  = [(rd, d, ins) for rd, d, ins in _tip_ray_all if np.isnan(d)]
        hit_angles    = [_tip_angle(rd) for rd, d, ins in hit_entries]
        miss_angles   = [_tip_angle(rd) for rd, d, ins in miss_entries]
        hit_dists     = [d   for rd, d, ins in hit_entries]
        n_hit_inside  = sum(1 for rd, d, ins in hit_entries  if ins)
        n_miss_inside = sum(1 for rd, d, ins in miss_entries if ins)

        # Split hits by whether origin was inside the mesh
        hit_inside_dists  = [d for rd, d, ins in hit_entries if ins]
        hit_outside_dists = [d for rd, d, ins in hit_entries if not ins]

        def _dist_report(angles: list[float], label: str, extra: str = "") -> None:
            if not angles:
                print(f"\n  {label}: 0 leaves{extra}")
                return
            a = np.array(angles)
            pcts = [0, 10, 25, 50, 75, 90, 100]
            percs = np.percentile(a, pcts)
            print(f"\n  {label}: {len(a)} leaves{extra}")
            print(f"    min={a.min():.1f}°  mean={a.mean():.1f}°  max={a.max():.1f}°")
            print(f"    " + "  ".join(f"p{p}={v:.1f}°" for p, v in zip(pcts, percs)))
            bins = np.arange(0, 181, 15)
            counts, _ = np.histogram(a, bins=bins)
            peak = max(counts) if counts.max() > 0 else 1
            for lo, hi, c in zip(bins, bins[1:], counts):
                if c == 0:
                    continue
                bar = "█" * int(c / peak * 20)
                print(f"    {lo:3}–{hi:3}°  {bar}  {c}")

        print("\n" + "=" * 64)
        print("TIP RAY ANGLE vs WORLD Z-DOWN — split by hit/miss")
        print(f"  total leaves: {len(_tip_ray_all)}")
        _dist_report(
            hit_angles,
            f"HIT (ray reached mesh within 10 mm)  origin-inside={n_hit_inside}/{len(hit_entries)}",
            f"  hit-dist: min={min(hit_dists):.2f}  median={float(np.median(hit_dists)):.2f}  max={max(hit_dists):.2f} mm" if hit_dists else "",
        )
        if hit_inside_dists:
            print(f"    → origin INSIDE  mesh: n={len(hit_inside_dists)}"
                  f"  hit-dist min={min(hit_inside_dists):.2f}"
                  f"  median={float(np.median(hit_inside_dists)):.2f}"
                  f"  max={max(hit_inside_dists):.2f} mm")
        if hit_outside_dists:
            print(f"    → origin OUTSIDE mesh: n={len(hit_outside_dists)}"
                  f"  hit-dist min={min(hit_outside_dists):.2f}"
                  f"  median={float(np.median(hit_outside_dists)):.2f}"
                  f"  max={max(hit_outside_dists):.2f} mm")
        _dist_report(
            miss_angles,
            f"MISS (no hit or hit > 10 mm — fallback)  origin-inside={n_miss_inside}/{len(miss_entries)}",
        )
        print("=" * 64)

    # ── Cone intersection analysis ────────────────────────────────────────────
    if _cone_data:
        _cone_analysis(_cone_data)

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
