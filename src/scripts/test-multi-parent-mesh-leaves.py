#!/usr/bin/env python3
"""
Multi-parent-mesh leaf placement test — two intersecting irregular foliage clusters.

Two foliage clusters occupy overlapping regions of 3D space.  Leaves are placed
via place_leaves_on_multiple_meshes, which keeps per-mesh BVHs for surface
queries but shares one world-space shingle occupancy grid across the clusters.
Rows from both meshes are interleaved globally by ascending z, so bottom-most
leaves on both clusters are placed before upper rows on either.

No jitter, no overlap (hardcoded defaults for this test).

Cluster A: vertical, tip at (0, 0, 22).
Cluster B: curved spine (35°→55°), tip at (6, 2, 23) — 6 mm from A, clear intersection.

Output: stl/test/multi-parent-mesh-leaves.stl

Usage::

    python src/scripts/test-multi-parent-mesh-leaves.py
    python src/scripts/test-multi-parent-mesh-leaves.py /path/to/output.stl
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
    place_leaves_on_multiple_meshes,
)
from dharmatiles.trees._utils import _safe_norm
from dharmatiles.trees.mesh import _build_foliage_cluster_mesh
from dharmatiles.trees.placement_greedy import place_leaves_greedy

from dharmatiles.core.color import (
    GRAY_SHADES,
    Material,
    RGBA,
    export_color_stl,
    tag as _color_tag,
)

# ── Placement parameters — no jitter, no overlap ──────────────────────────────

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
    r_wood          = 1.0,
    r_foliage       = 5.5,
    clump_length_mm = 10.5,
    leaves          = False,
    **_LEAF,
)

_PLACE_KW = dict(
    length_mm        = float(_LEAF["leaf_length_mm"]),
    width_mm         = float(_LEAF["leaf_width_mm"]),
    thickness_mm     = float(_LEAF["leaf_thickness_mm"]),
    fold_angle_deg   = float(_LEAF["leaf_fold_angle_deg"]),
    inner_curve      = float(_LEAF["leaf_inner_curve"]),
    outer_curve      = float(_LEAF["leaf_outer_curve"]),
    curl_deg         = float(_LEAF["leaf_curl_deg"]),
    lift_mm          = float(_LEAF["leaf_lift_mm"]),
    h_overlap        = 0.0,
    v_overlap        = 0.0,
    n_meridians      = int(_LEAF["leaf_arc_meridians"]),
    z_samples        = int(_LEAF["leaf_arc_z_samples"]),
    angle_jitter_deg = 0.0,
    pos_jitter       = 0.0,
)

# ── Artifact detection thresholds (matches regular test) ──────────────────────

_BALD_ROW_THRESHOLD       = 0.25
_ROOT_DEPTH_MAX_MM        = 4.0
_ROW_GAP_FACTOR           = 2.0
_FLOATING_LEAF_EXTRA_MM   = 1.0
_BURIED_LEAF_CURL_DEPTH_MM = 0.25
_TOP_ROW_SPREAD_MAX_MM    = 1.0
_UPWARD_TANGENT_Z_THRESHOLD = 0.1


def _row_rgba(row_idx: int) -> tuple[int, int, int, int]:
    return RGBA[GRAY_SHADES[row_idx % len(GRAY_SHADES)]]


# ── Minimal artifact check ─────────────────────────────────────────────────────

def _check_artifacts(all_stats: list[LeafPlacementStats]) -> int:
    """Print a structured artifact report; return total issue count."""
    print("\n" + "=" * 64)
    print("ARTIFACT DETECTION REPORT")
    print("=" * 64)
    total_issues = 0

    for stats in all_stats:
        issues: list[str] = []
        has_leaves = len(stats.base_positions) > 0
        bases      = np.stack(stats.base_positions) if has_leaves else None
        row_ids    = np.array(stats.base_row_idx)   if has_leaves else None

        covered_rows = [(z, att, pl) for z, att, pl in stats.rows if pl > 0]
        covered_zs   = [z for z, att, pl in covered_rows]

        # 1. Bald rows
        bald = [(i, z, att, pl)
                for i, (z, att, pl) in enumerate(stats.rows)
                if att > 0 and pl / att < _BALD_ROW_THRESHOLD]
        if bald:
            issues.append(
                f"BALD ROWS [{len(bald)}/{stats.n_rows}]: "
                f"rows {[i for i, *_ in bald[:8]]}{'...' if len(bald) > 8 else ''}"
            )

        # 2. Low overall coverage
        if stats.n_attempted > 0:
            cov = stats.n_placed / stats.n_attempted
            if cov < 0.5:
                issues.append(f"LOW COVERAGE: {cov:.1%} of {stats.n_attempted} attempted")

        # 3. Bald top
        if covered_zs:
            top_gap = stats.z_top_anchor - covered_zs[-1]
            if top_gap > stats.expected_row_step:
                issues.append(
                    f"BALD TOP: last row z={covered_zs[-1]:.2f}, "
                    f"expected apex z≈{stats.z_top_anchor:.2f} — gap {top_gap:.2f} mm"
                )

        # 4. Interior row gaps
        if len(covered_zs) > 1:
            big = [(covered_zs[i+1] - covered_zs[i], i)
                   for i in range(len(covered_zs) - 1)
                   if covered_zs[i+1] - covered_zs[i] > stats.expected_row_step * _ROW_GAP_FACTOR]
            if big:
                worst_g, worst_i = max(big, key=lambda x: x[0])
                issues.append(
                    f"INTERIOR GAPS: {len(big)} gap(s) > {_ROW_GAP_FACTOR:.0f}× step "
                    f"(worst {worst_g:.2f} mm at position {worst_i})"
                )

        # 5. Long roots
        if stats.root_depths:
            long = [d for d in stats.root_depths if d > _ROOT_DEPTH_MAX_MM]
            if long:
                issues.append(
                    f"LONG ROOTS: {len(long)}/{len(stats.root_depths)} "
                    f"(max={max(long):.2f} mm)"
                )

        # 6. Floating curl region
        if stats.leaf_float_dists:
            _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
            floaters = [d for d in stats.leaf_float_dists if d > _float_thresh]
            if floaters:
                issues.append(
                    f"FLOATING LEAVES: {len(floaters)}/{len(stats.leaf_float_dists)} "
                    f"(max={max(floaters):.2f} mm, thresh={_float_thresh:.2f})"
                )

        # 7. Buried curl region
        if stats.leaf_buried_depths:
            buried = [d for d in stats.leaf_buried_depths if d > _BURIED_LEAF_CURL_DEPTH_MM]
            if buried:
                issues.append(
                    f"BURIED LEAVES: {len(buried)}/{len(stats.leaf_buried_depths)} "
                    f"(max={max(buried):.2f} mm)"
                )

        # 8. Top-row spread
        if has_leaves:
            top_ridx  = int(max(stats.base_row_idx))
            top_bases = np.stack([
                b for b, r in zip(stats.base_positions, stats.base_row_idx)
                if r == top_ridx
            ])
            spread = min_width_xy(top_bases)
            if spread > _TOP_ROW_SPREAD_MAX_MM:
                issues.append(
                    f"TOP ROW SPREAD: top-row min-width {spread:.1f} mm "
                    f"(thresh {_TOP_ROW_SPREAD_MAX_MM:.0f} mm)"
                )

        # 9. Upward tangents
        if has_leaves:
            tang_arr    = np.stack(stats.base_tangents)
            upward_mask = tang_arr[:, 2] > _UPWARD_TANGENT_Z_THRESHOLD
            if upward_mask.any():
                issues.append(
                    f"UPWARD TANGENTS: {int(upward_mask.sum())} leaves "
                    f"(worst z={float(tang_arr[upward_mask, 2].max()):.3f})"
                )

        status = "PASS" if not issues else "FAIL"
        total_issues += len(issues)

        print(f"\n[{status}] {stats.label}")
        if stats.rows:
            print(
                f"  z_range=[{stats.rows[0][0]:.2f}, {stats.rows[-1][0]:.2f}]  "
                f"z_top_anchor={stats.z_top_anchor:.2f}  "
                f"expected_step={stats.expected_row_step:.2f} mm"
            )
        print(
            f"  placed={stats.n_placed}  rows={stats.n_rows}  "
            f"attempted={stats.n_attempted}  "
            f"skip(down={stats.skipped_downward} r={stats.skipped_small_r} "
            f"preburied={stats.skipped_preburied} floor={stats.skipped_below_floor} "
            f"xbury={stats.skipped_cross_buried})  "
            f"errors={stats.build_errors}"
        )
        if stats.rows:
            print("  rows: " + "  ".join(
                f"z={z:.1f}:{pl}/{att}" for z, att, pl in stats.rows
            ))
        if stats.root_depths:
            print(
                f"  root depth mm: "
                f"min={min(stats.root_depths):.2f}  "
                f"median={float(np.median(stats.root_depths)):.2f}  "
                f"max={max(stats.root_depths):.2f}"
            )
        if stats.leaf_float_dists:
            _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
            n_float  = sum(1 for d in stats.leaf_float_dists  if d > _float_thresh)
            n_buried = sum(1 for d in stats.leaf_buried_depths if d > _BURIED_LEAF_CURL_DEPTH_MM)
            print(
                f"  curl outside mm: "
                f"median={float(np.median(stats.leaf_float_dists)):.2f}  "
                f"max={max(stats.leaf_float_dists):.2f}  floating={n_float}"
            )
            print(
                f"  curl buried mm: "
                f"median={float(np.median(stats.leaf_buried_depths)):.2f}  "
                f"max={max(stats.leaf_buried_depths):.2f}  buried={n_buried}"
            )
        if stats.tip_z_clearances:
            lifted = [d for d in stats.tip_z_lifts if d > 0.0]
            print(
                f"  tip z clearance mm: "
                f"min={min(stats.tip_z_clearances):.2f}  "
                f"median={float(np.median(stats.tip_z_clearances)):.2f}  "
                f"max={max(stats.tip_z_clearances):.2f}  "
                f"blade_lifted={len(lifted)}"
            )
            if lifted:
                print(
                    f"  tip z correction mm: "
                    f"median={float(np.median(lifted)):.2f}  max={max(lifted):.2f}"
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
    try:
        from dharmatiles.render import render as _render
    except ImportError:
        print("  (pyrender not available — skipping PNG render)")
        return
    views = [
        (35.0, -135.0, "perspective", "multi-parent-mesh-leaves (perspective)"),
        (85.0,    0.0, "top",         "multi-parent-mesh-leaves (top)"),
        ( 5.0,    0.0, "side",        "multi-parent-mesh-leaves (side)"),
    ]
    print("\nRendering PNG views …")
    for elev, azim, suffix, label in views:
        out_png = stl_path.parent / f"{stl_path.stem}-{suffix}.png"
        try:
            _render(
                all_parts, out_png,
                elev=elev, azim=azim, resolution=(1600, 900), quiet=False, label=label,
                smooth=False,
            )
        except Exception as exc:
            print(f"  (render failed for {suffix}: {exc})")


# ── Error leaf colouring ───────────────────────────────────────────────────────

def _mark_error_leaves(leaves: list[trimesh.Trimesh], stats: LeafPlacementStats) -> None:
    if len(leaves) != len(stats.root_depths):
        return
    error_color = np.asarray(RGBA_FLAG_FAIL, dtype=np.uint8)
    _float_thresh = stats.lift_mm + _FLOATING_LEAF_EXTRA_MM
    for i, leaf in enumerate(leaves):
        is_error = (
            stats.root_depths[i]       > _ROOT_DEPTH_MAX_MM
            or stats.leaf_float_dists[i]   > _float_thresh
            or stats.leaf_buried_depths[i] > _BURIED_LEAF_CURL_DEPTH_MM
            or float(stats.base_tangents[i][2]) > _UPWARD_TANGENT_Z_THRESHOLD
        )
        if is_error:
            leaf.visual = trimesh.visual.ColorVisuals(
                mesh=leaf,
                face_colors=np.tile(error_color, (len(leaf.faces), 1)),
            )


# ── Cluster factory ────────────────────────────────────────────────────────────

def _make_cluster(
    cx: float, cy: float, z_tip: float,
    tip_t: list[float], start_t: list[float],
    edge_id: int, bark_seed: int, label: str,
    *, apply_noise: bool = True,
) -> trimesh.Trimesh:
    tip_t_n   = _safe_norm(np.asarray(tip_t,   float))
    start_t_n = _safe_norm(np.asarray(start_t, float))
    tip_p     = np.array([cx, cy, z_tip], float)
    start_p   = tip_p - float(_CLUSTER["clump_length_mm"]) * tip_t_n
    tilt_deg  = math.degrees(math.acos(float(np.clip(tip_t_n[2], -1.0, 1.0))))
    if apply_noise:
        print(
            f"  [{label}] edge_id={edge_id}  tilt={tilt_deg:.1f}°  "
            f"tip={[f'{v:.1f}' for v in tip_p]}  start={[f'{v:.1f}' for v in start_p]}"
        )
    cluster, _ = _build_foliage_cluster_mesh(
        tip_pos       = tip_p,
        tip_tangent   = tip_t_n,
        start_pos     = start_p,
        start_tangent = start_t_n,
        edge_id       = edge_id,
        bark_seed     = bark_seed,
        apply_noise   = apply_noise,
        **_CLUSTER,
    )
    return cluster


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    import argparse
    parser = argparse.ArgumentParser(description="Multi-parent-mesh leaf placement test")
    parser.add_argument("out", nargs="?", help="Output STL path")
    parser.add_argument(
        "--placement", choices=["meridian", "greedy"], default="meridian",
        help="Leaf placer to exercise (default: meridian).",
    )
    args = parser.parse_args()
    placement = args.placement

    default_out = (
        Path(__file__).parents[2] / "stl" / "test" / "multi-parent-mesh-leaves.stl"
    )
    out = Path(args.out) if args.out else default_out
    out.parent.mkdir(parents=True, exist_ok=True)

    t0 = time.perf_counter()


    a35 = math.radians(35)
    a55 = math.radians(55)
    t_vert = [0.0, 0.0, 1.0]
    t_35   = [math.sin(a35), 0.0, math.cos(a35)]
    t_55   = [math.sin(a55), 0.0, math.cos(a55)]

    Z_TIP = 22.0

    # ── A + B intersecting — multi-parent-mesh ────────────────────────────────
    # A: vertical at origin; B: 55° tilt, tip 6 mm away → clear intersection
    # (r_foliage=5.5, so surfaces overlap by ~5 mm).
    print("\n=== A + B intersecting — multi-parent-mesh ===")
    print("  Cluster A: vertical at (0, 0)")
    ca = _make_cluster(
        cx=0.0, cy=0.0, z_tip=Z_TIP,
        tip_t=t_vert, start_t=t_vert,
        edge_id=0, bark_seed=33, label="A",
    )
    print("  Cluster B: 55° tilt, tip at (6, 2) — overlaps A")
    cb = _make_cluster(
        cx=6.0, cy=2.0, z_tip=Z_TIP + 1.0,
        tip_t=t_55, start_t=t_35,   # curved spine: start at 35°, tip at 55°
        edge_id=1, bark_seed=44, label="B",
    )

    _color_tag(ca, Material.FOLIAGE)
    _color_tag(cb, Material.WOOD)

    print(f"  Cluster A: {len(ca.vertices):,}v / {len(ca.faces):,}f")
    print(f"  Cluster B: {len(cb.vertices):,}v / {len(cb.faces):,}f")

    if placement == "greedy":
        print("  Placing leaves on A and B via GREEDY lowest-first accretion …")
        # Greedy runs on the SMOOTH envelopes; the noised ca/cb are the real
        # clumps used for the exact per-leaf root-connection gate.
        ca_s = _make_cluster(
            cx=0.0, cy=0.0, z_tip=Z_TIP, tip_t=t_vert, start_t=t_vert,
            edge_id=0, bark_seed=33, label="A", apply_noise=False,
        )
        cb_s = _make_cluster(
            cx=6.0, cy=2.0, z_tip=Z_TIP + 1.0, tip_t=t_55, start_t=t_35,
            edge_id=1, bark_seed=44, label="B", apply_noise=False,
        )
        parts_list, stats_list = place_leaves_greedy(
            [ca_s, cb_s],
            real_meshes      = [ca, cb],
            length_mm        = _PLACE_KW["length_mm"],
            width_mm         = _PLACE_KW["width_mm"],
            thickness_mm     = _PLACE_KW["thickness_mm"],
            fold_angle_deg   = _PLACE_KW["fold_angle_deg"],
            inner_curve      = _PLACE_KW["inner_curve"],
            outer_curve      = _PLACE_KW["outer_curve"],
            curl_deg         = _PLACE_KW["curl_deg"],
            lift_mm          = _PLACE_KW["lift_mm"],
            seeds            = [0, 1],
            labels           = ["A (vertical)", "B (55° tilt)"],
            angle_jitter_deg = _PLACE_KW["angle_jitter_deg"],
            pos_jitter       = _PLACE_KW["pos_jitter"],
            row_color_fn     = _row_rgba,
        )
    else:
        print("  Placing leaves on A and B with shared world-space shingling …")
        parts_list, stats_list = place_leaves_on_multiple_meshes(
            [ca, cb],
            **_PLACE_KW,
            seeds=[0, 1],
            labels=["A (vertical)", "B (55° tilt)"],
            row_color_fn=_row_rgba,
        )
    leaves_a, leaves_b = parts_list
    stats_a,  stats_b  = stats_list

    stats_a.label = "A — vertical at (0,0) (multi-parent-mesh)"
    stats_b.label = "B — 55° tilt, tip at (6,2) (multi-parent-mesh)"

    print(f"    → {len(leaves_a)} leaves on A,  {len(leaves_b)} leaves on B "
          f"({len(leaves_a) + len(leaves_b)} total)")

    all_parts = [ca, cb] + leaves_a + leaves_b
    all_stats  = [stats_a, stats_b]

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
    print("Colour key:")
    print("  Dark green (FOLIAGE) = cluster A body  (vertical, origin)")
    print("  Warm brown  (WOOD)   = cluster B body  (55° tilt, tip at (6,2))")
    print("  Black → white per global Z row = leaves (bottom dark, top light, cycles at 16)")


if __name__ == "__main__":
    main()
