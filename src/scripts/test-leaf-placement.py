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
# Set to ~1× leaf thickness (0.24 mm) so shallow tip burial (e.g. apex leaf
# arching back into the sphere at ~0.35 mm) is caught.
_BURIED_LEAF_CURL_DEPTH_MM = 0.25

# Top-row spread: maximum acceptable minimum-width (mm) of the XY convex hull
# of the topmost row's base positions.  A large spread means the top row is
# far from the apex rather than converging near the tip.
_TOP_ROW_SPREAD_MAX_MM = 1.0


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
    leaf_angle_jitter_deg = 0.0,
    leaf_pos_jitter       = 0.0,
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
    n_meridians    = int(_LEAF["leaf_arc_meridians"]),
    z_samples      = int(_LEAF["leaf_arc_z_samples"]),
)


# ── Artifact analysis and reporting ──────────────────────────────────────────

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
            f"skip(down={stats.skipped_downward} r={stats.skipped_small_r})  "
            f"ca_clamped={stats.ca_clamped}  errors={stats.build_errors}"
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
    all_stats: list[LeafPlacementStats] = []

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
