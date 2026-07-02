"""Shoot-based leaf placement (experimental, side-by-side).

A third placement strategy alongside the meridian-arc placer
(:mod:`placement`) and the greedy lowest-first placer
(:mod:`placement_greedy`).  Design source:
``docs/meta/history/2026-07-02-fable-leaf-placement-design-review.md`` —
"place shoots, not leaves".

The greedy placer treats every leaf as an independent, identically-sized
individual, which yields blue-noise uniformity — the fingerprint of
procedural scatter.  Real canopy leaves come in coherent groups along
shoots.  This placer keeps the greedy chassis unchanged (pre-generated
candidates, global z-ordered sweep, cheap-reject ladder, per-leaf oval
seating) but makes the placement unit a **shoot**:

    A candidate is a shoot START (its lowest station).  From there a spine
    is marched UP-slope along the real clump surface, one internode step at
    a time, re-projecting onto the surface by a short ray cast.  Each
    station carries one leaf pointing down-slope, splayed alternately
    left/right of the spine, with sizes diminishing toward the apical
    (up-slope) end.  Marching up-slope means each higher leaf's blade
    reaches down over the station below it — chain imbrication, shingled
    like roof tiles laid bottom row first.

Per-leaf mechanics — oval seat (:func:`placement_greedy._seat_oval_tilt`),
rigid blade↔oval frame (:func:`placement_greedy._leaf_frame_and_oval`),
containment guards, and the printability skew — are imported from the
greedy module verbatim; only the *grouping* above the leaf changes.

Conflict resolution is shoot-atomic: all of a prospective shoot's stations
are tested against the claimed-root grid (and neighbour containment) as a
unit before any of its leaves are built, so within-shoot stations never
collide with each other while shoot-vs-shoot spacing still emerges from the
shared grid.  A shoot that cannot seat at least
:data:`_SHOOT_MIN_PLACED_STATIONS` stations is discarded whole.

Hard constraints carried over from the 2026-07-01 perf crisis (do NOT
violate): no ``trimesh.proximity.closest_point`` / R-tree / per-leaf
``Trimesh`` scans in the sweep; no ``fix_normals`` on placed leaves;
cheap-reject before every build.

Public entry point mirrors :func:`placement_greedy.place_leaves_greedy` so
the dispatch in :func:`mesh.build_branch_mesh` is a drop-in.
"""
from __future__ import annotations

import math
import time
from collections.abc import Callable

import numpy as np
import trimesh

from ._utils import _hash01, _safe_norm
from .leaf import build_leaf_surface, solidify_leaf
from .mesh import _LEAF_PLACEABLE_NORMAL_Z, _hash01_int
from .placement import LeafPlacementStats
from .placement_greedy import (
    _GREEDY_EMBED_MM,
    _PROTRUSION_MM,
    _SKEW_TIP_MARGIN_MM,
    _generate_candidates,
    _growth_tangent,
    _leaf_frame_and_oval,
    _points_inside_any,
    _root_cell,
    _root_occupied_near,
    _seat_oval_tilt,
)

# ── Shoot-specific constants ──────────────────────────────────────────────────
# Leaves per shoot: hash-drawn uniformly from [MIN, MAX] per candidate, then
# truncated by march failures and root-grid conflicts.
_SHOOT_LEAVES_MIN: int = 3
_SHOOT_LEAVES_MAX: int = 7

# A shoot that cannot seat at least this many stations is discarded whole —
# a 1-leaf "shoot" is just greedy scatter again.
_SHOOT_MIN_PLACED_STATIONS: int = 2

# Station (internode) spacing along the spine, as a fraction of leaf length.
# 0.5 ⇒ each leaf's blade overlaps ~half of the next station down — the
# within-shoot imbrication that reads as a leafy sprig.
_SHOOT_INTERNODE_FRAC: float = 0.5

# Alternate splay: each leaf's growth direction is rotated about the surface
# normal by ±this angle from the local down-slope, alternating left/right
# along the shoot (botanical alternate/distichous arrangement, flattened
# onto the surface).  Combined with the rank offset below this exposes leaf
# TIPS out the sides of the shoot (pinnate-sprig look).  Kept moderate: past
# ~60° the tangent loses its downward component on gentle slopes and the
# printability skew culls whole regions (60° balded the apex dome).
_SHOOT_SPLAY_DEG: float = 35.0

# Two-rank herringbone: each leaf's BASE is displaced off the spine to its
# own splay side by this fraction of leaf width (then re-projected onto the
# surface).  Bases line up beside the flank of the leaf above them instead
# of on the centerline — so a parent's tip runs down its own rank, not into
# the side of the child below it.
_SHOOT_RANK_OFFSET_FRAC: float = 0.4

# Leaf size scale at the sprig TIP.  Leaves point down-slope, so the sprig's
# tip is its lowest (down-slope) station and its base is the highest: station
# 0 gets this scale and leaves grow linearly to 1.0 at the up-slope (basal)
# end — biggest leaf at the beginning of the shoot, youngest/smallest at the
# tip, like a real sprig.
_SHOOT_TIP_SCALE: float = 0.65

# Surface re-projection during the march: the predicted next station is
# lifted this far along the local normal, then dropped back by ray cast.
_SHOOT_MARCH_LIFT_MM: float = 1.0

# Extra ray-drop allowance beyond lift+step before a re-projection is deemed
# to have fallen past a rim onto the far side.  Must exceed the clump noise's
# max inward erosion (~2.3 mm measured, see the 2026-07-01 greedy writeup) or
# marches truncate in every noise pit.
_SHOOT_MARCH_DROP_SLACK_MM: float = 2.5


# ── Small vector helper ───────────────────────────────────────────────────────

def _rotate_about(v: np.ndarray, axis: np.ndarray, angle_rad: float) -> np.ndarray:
    """Rodrigues rotation of ``v`` about unit ``axis`` by ``angle_rad``."""
    c = math.cos(angle_rad)
    s = math.sin(angle_rad)
    return v * c + np.cross(axis, v) * s + axis * float(np.dot(axis, v)) * (1.0 - c)


# ── Surface re-projection ─────────────────────────────────────────────────────

def _project_to_surface(
    mesh: trimesh.Trimesh,
    Q: np.ndarray,
    n: np.ndarray,
    max_drop_mm: float,
) -> tuple[np.ndarray, np.ndarray] | None:
    """Drop ``Q`` onto the surface along ``−n`` from a short lift above it.

    Returns ``(point, smooth_normal)`` — the smooth normal is the barycentric
    blend of the hit triangle's vertex normals, matching the candidate
    generator — or ``None`` when the ray misses or lands implausibly far
    (past a rim / on the far side).
    """
    G = Q + _SHOOT_MARCH_LIFT_MM * n
    loc, _ray_idx, tri_idx = mesh.ray.intersects_location(
        G[np.newaxis], (-n)[np.newaxis], multiple_hits=False,
    )
    if len(loc) == 0:
        return None
    Pn = np.asarray(loc[0], float)
    if float(np.linalg.norm(Pn - G)) > max_drop_mm:
        return None
    fi = int(tri_idx[0])
    tri = mesh.triangles[fi]
    bary = trimesh.triangles.points_to_barycentric(
        tri[np.newaxis], Pn[np.newaxis],
    )[0]
    vn = mesh.vertex_normals[mesh.faces[fi]]
    nn = _safe_norm(bary @ vn)
    return Pn, nn


# ── Spine march ───────────────────────────────────────────────────────────────

def _march_stations(
    mesh: trimesh.Trimesh,
    base: np.ndarray,
    normal: np.ndarray,
    n_stations: int,
    step_mm: float,
    centroid: np.ndarray,
) -> list[tuple[np.ndarray, np.ndarray, np.ndarray]]:
    """March the shoot spine up-slope along the real surface from ``base``.

    Station 0 is the candidate itself (the shoot's lowest leaf).  Each
    subsequent station steps ``step_mm`` against the local down-slope
    tangent, is lifted ``_SHOOT_MARCH_LIFT_MM`` along the local normal, and
    re-projected onto the surface with a single embree ray cast along
    ``−normal``.  The smooth (barycentric vertex) normal at the hit point
    keeps leaf orientation continuous across the coarse icosphere, matching
    the candidate generator.

    The march truncates (returns what it has) when the ray misses, lands
    implausibly far (past a rim / on the far side), reaches a down-facing
    normal, or degenerates near the apex where the down-slope direction
    vanishes and steps stop making progress.

    Returns a list of ``(point, smooth_normal, downslope_tangent)``.
    """
    stations: list[tuple[np.ndarray, np.ndarray, np.ndarray]] = []
    P = np.asarray(base, float)
    n = np.asarray(normal, float)
    T = _growth_tangent(n, P, centroid)
    stations.append((P, n, T))
    max_drop = _SHOOT_MARCH_LIFT_MM + step_mm + _SHOOT_MARCH_DROP_SLACK_MM
    for _ in range(n_stations - 1):
        proj = _project_to_surface(mesh, P - step_mm * T, n, max_drop)
        if proj is None:
            break                       # ray miss, past a rim, or the far side
        Pn, nn = proj
        if float(np.linalg.norm(Pn - P)) < 0.25 * step_mm:
            break                       # no progress (apex degeneracy)
        if float(nn[2]) < _LEAF_PLACEABLE_NORMAL_Z:
            break                       # marched onto a hidden underside
        P, n = Pn, nn
        T = _growth_tangent(n, P, centroid)
        stations.append((P, n, T))
    return stations


# ── Per-station leaf attempt (the greedy per-leaf pipeline, parameterised) ────

def _attempt_leaf(
    mesh: trimesh.Trimesh,
    neighbour_meshes: list,
    base: np.ndarray,
    normal: np.ndarray,
    T0: np.ndarray,
    L: float,
    W: float,
    leaf_kw: dict,
    lseed: int,
):
    """Seat, build, and cull one leaf at a shoot station.

    Exactly the greedy sweep body (equal-depth oval seat → rigid frame →
    oval containment guard → build → printability skew → tip/belly burial
    cull → solidify), with ``L``/``W`` free so shoot stations can carry
    scaled leaves.

    Returns ``((solid, tangent_leaf, skew_mm, tip_z_clearance), None)`` on
    success or ``(None, reason)`` with reason in ``{"buried", "floor",
    "error"}``.
    """
    tilt = _seat_oval_tilt(mesh, base, normal, T0, L, _GREEDY_EMBED_MM)
    if tilt is None:
        return None, "buried"
    frame = _leaf_frame_and_oval(
        base, normal, T0, L, W, _GREEDY_EMBED_MM, _PROTRUSION_MM, tilt,
    )
    if frame is None:
        return None, "error"
    surf_base, tangent_leaf, up_leaf, inner_v = frame

    if not bool(mesh.contains(inner_v[[-2, -1]]).all()):
        return None, "buried"

    try:
        surf, _geom = build_leaf_surface(
            base_pos=surf_base, tangent=tangent_leaf, up_hint=up_leaf,
            seed=lseed, **leaf_kw,
        )
    except (RuntimeError, ValueError):
        return None, "error"
    tip_idx = len(surf.vertices) - 1
    base_idx = len(surf.vertices) - 2

    skew_mm = 0.0
    z_need = (float(inner_v[-1][2]) + _SKEW_TIP_MARGIN_MM
              - float(surf.vertices[tip_idx][2]))
    if z_need > 0.0:
        t_z = float(tangent_leaf[2])
        skew_mm = z_need / -t_z if t_z < -1e-6 else float("inf")
        if skew_mm > 0.5 * L:
            return None, "floor"
        surf.vertices = surf.vertices - skew_mm * tangent_leaf

    curl_mask = np.linalg.norm(
        surf.vertices - surf.vertices[base_idx], axis=1,
    ) > (L / 2.0)
    curl_idx = np.nonzero(curl_mask)[0]
    if len(curl_idx) == 0:
        curl_idx = np.arange(len(surf.vertices))
    belly_idx = int(curl_idx[int(np.argmin(surf.vertices[curl_idx, 2]))])
    probe = surf.vertices[np.array([tip_idx, belly_idx])]
    if _points_inside_any([mesh, *neighbour_meshes], probe, base, L):
        return None, "buried"

    try:
        solid, _ = solidify_leaf(surf, inner_v)
    except (RuntimeError, ValueError):
        return None, "error"

    tip_z_clearance = float(surf.vertices[tip_idx][2]) - float(inner_v[-1][2])
    return (solid, tangent_leaf, skew_mm, tip_z_clearance), None


# ── Public entry point ────────────────────────────────────────────────────────

def place_leaves_shoots(
    meshes: list[trimesh.Trimesh],          # real (noised) foliage clumps
    *,
    length_mm: float,
    width_mm: float,
    thickness_mm: float,
    fold_angle_deg: float,
    inner_curve: float,
    outer_curve: float,
    curl_deg: float,
    lift_mm: float,
    seeds: int | list[int] = 0,
    labels: str | list[str] | None = None,
    angle_jitter_deg: float = 0.0,
    pos_jitter: float = 0.0,
    # shoot-specific (module-const defaults; promote to config later):
    candidate_density: float = 2.5,
    min_root_gap_mm: float | None = None,
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
    verbose: bool = True,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
    """Shoot-based leaf placement across the real (noised) foliage clumps.

    Candidates are shoot starts sampled on the real clump surface, swept in
    global z order like the greedy placer.  Each accepted candidate marches
    a spine up-slope and emits an alternating, size-diminishing run of
    leaves along it (see module docstring).

    ``row_color_fn``, when given, is called with the SHOOT index (not the
    z-band row index) so each shoot renders as a distinct shade — the
    per-leaf ``stats.base_row_idx`` still records the z band for the
    artifact checks.

    ``lift_mm``, ``angle_jitter_deg`` and ``pos_jitter`` are accepted for
    signature parity with the other placers and ignored: the oval seat
    replaces the lift, and the shoot's splay/taper IS the (coherent)
    variation, replacing i.i.d. jitter.

    Returns ``(parts_per_mesh, stats_per_mesh)`` — the same contract as the
    meridian and greedy placers.  Meridian-only stats fields are left
    empty/zero.
    """
    n = len(meshes)
    seeds_list: list[int] = [seeds] * n if isinstance(seeds, int) else list(seeds)
    if labels is None:
        labels_list: list[str] = [f"mesh-{i}" for i in range(n)]
    elif isinstance(labels, str):
        labels_list = [labels] * n
    else:
        labels_list = list(labels)

    L = float(length_mm)
    W = float(width_mm)
    # Slightly tighter default than greedy's W×0.5: shoot leaves taper toward
    # the apical end (× _SHOOT_TIP_SCALE), so equal-gap packing reads sparser.
    gap = float(min_root_gap_mm) if min_root_gap_mm is not None else max(W * 0.4, 1e-3)
    col_step = max(W, 1e-3)
    expected_row_step = max(L * 0.5, 1e-3)
    step_mm = _SHOOT_INTERNODE_FRAC * L

    del lift_mm, angle_jitter_deg, pos_jitter   # parity-only (see docstring)
    leaf_kw_base = dict(
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
        lift_mm        = 0.0,
    )
    t_total = time.perf_counter()

    # ── Per-mesh setup: stats, centroids, neighbour prune ─────────────────────
    stats_list: list[LeafPlacementStats] = []
    parts_list: list[list[trimesh.Trimesh]] = []
    z_mins: list[float] = []
    centroids: list[np.ndarray] = []
    all_cands = []

    bounds_centers = []
    bounds_radii = []
    for mi, (mesh, seed, label) in enumerate(zip(meshes, seeds_list, labels_list)):
        z_mins.append(float(mesh.vertices[:, 2].min()))
        cx = float(mesh.vertices[:, 0].mean())
        cy = float(mesh.vertices[:, 1].mean())
        z_top = float(mesh.vertices[:, 2].max())
        centroids.append(np.array([cx, cy, float(mesh.vertices[:, 2].mean())]))
        stats = LeafPlacementStats(
            label             = label,
            leaf_length_mm    = L,
            leaf_width_mm     = W,
            col_step          = col_step,
            expected_row_step = expected_row_step,
            z_top             = z_top,
            z_top_anchor      = z_top,
            cx                = cx,
            cy                = cy,
            lift_mm           = 0.0,
        )
        stats_list.append(stats)
        parts_list.append([])
        all_cands.extend(_generate_candidates(
            mesh, mi, seed,
            candidate_density=candidate_density, min_root_gap_mm=gap,
        ))
        b = np.asarray(mesh.bounds, dtype=float)
        bounds_centers.append(0.5 * (b[0] + b[1]))
        bounds_radii.append(float(np.linalg.norm(b[1] - b[0]) * 0.5))

    neighbours: list[list[trimesh.Trimesh]] = []
    for mi in range(n):
        nb = []
        for oi in range(n):
            if oi == mi:
                continue
            gap_c = float(np.linalg.norm(bounds_centers[mi] - bounds_centers[oi]))
            if gap_c <= bounds_radii[mi] + bounds_radii[oi] + L:
                nb.append(meshes[oi])
        neighbours.append(nb)

    # ── Global z-ordered sweep over shoot starts ──────────────────────────────
    all_cands.sort(key=lambda c: (c.z, c.phi, c.mesh_id, c.idx))

    root_grid: set[tuple[int, int, int]] = set()
    n_rejected_root = 0
    n_rejected_short = 0
    n_shoots = 0

    for cand in all_cands:
        mi = cand.mesh_id
        stats = stats_list[mi]

        # Shoot start blocked by an already-claimed leaf root → dense enough here.
        if _root_occupied_near(root_grid, cand.base, gap):
            n_rejected_root += 1
            continue

        hseed = int(seeds_list[mi])
        n_target = _SHOOT_LEAVES_MIN + int(
            _hash01(hseed, "shoot-n", cand.idx)
            * (_SHOOT_LEAVES_MAX - _SHOOT_LEAVES_MIN + 1)
        )
        n_target = min(n_target, _SHOOT_LEAVES_MAX)
        phase = 1 if _hash01(hseed, "shoot-ph", cand.idx) < 0.5 else 0

        stations = _march_stations(
            meshes[mi], cand.base, cand.normal, n_target, step_mm, centroids[mi],
        )
        # Shoot-atomic conflict cull: every station tested against the grid
        # (and neighbour containment) as it stands BEFORE this shoot places
        # anything, so within-shoot stations never collide with each other.
        free = [
            st for st in stations
            if not _root_occupied_near(root_grid, st[0], gap)
            and not _points_inside_any(neighbours[mi], st[0][np.newaxis], st[0], 0.0)
        ]
        # A shoot whose MARCH truncated below the minimum (apex/rim geometry —
        # no spine can exist there) may fall back to a single leaf: rejecting
        # it leaves a genuinely bald spot.  A shoot shortened by GRID conflicts
        # is rejected as before — neighbouring leaves already cover that area.
        min_needed = (
            _SHOOT_MIN_PLACED_STATIONS
            if len(stations) >= _SHOOT_MIN_PLACED_STATIONS else 1
        )
        if len(free) < min_needed:
            n_rejected_short += 1
            continue

        n_free = len(free)
        placed_in_shoot = 0
        for si, (P, sn, sT) in enumerate(free):
            # si=0 is the lowest (down-slope) station = the sprig tip →
            # smallest leaf; the up-slope (basal) end carries the biggest.
            # Single-leaf fallbacks (apex/rim) stay full-size.
            if n_free == 1:
                scale = 1.0
            else:
                scale = _SHOOT_TIP_SCALE + (1.0 - _SHOOT_TIP_SCALE) * (
                    si / (n_free - 1)
                )
            Ls = L * scale
            Ws = W * scale
            if n_free == 1:
                # Apex/rim fallback single: pure down-slope, on the spine.
                # Splay costs it its downward component exactly where slopes
                # are gentlest.
                base, bn, T_leaf = P, sn, sT
            else:
                # Two-rank herringbone: displace the base off the spine to
                # its own splay side and re-project onto the surface, so it
                # lines up beside the flank of the leaf above (its parent)
                # rather than under that parent's tip.
                sign = 1.0 if (si + phase) % 2 == 0 else -1.0
                lat = _safe_norm(np.cross(sn, sT))
                offset = _SHOOT_RANK_OFFSET_FRAC * Ws
                proj = _project_to_surface(
                    meshes[mi], P + sign * offset * lat, sn,
                    _SHOOT_MARCH_LIFT_MM + offset + _SHOOT_MARCH_DROP_SLACK_MM,
                )
                if proj is None:
                    stats.skipped_cross_buried += 1
                    continue
                base, bn = proj
                if float(bn[2]) < _LEAF_PLACEABLE_NORMAL_Z:
                    stats.skipped_cross_buried += 1
                    continue
                if _points_inside_any(
                    neighbours[mi], base[np.newaxis], base, 0.0,
                ):
                    stats.skipped_cross_buried += 1
                    continue
                bT = _growth_tangent(bn, base, centroids[mi])
                T_leaf = _safe_norm(
                    _rotate_about(bT, bn, sign * math.radians(_SHOOT_SPLAY_DEG))
                )
            lseed = int(_hash01_int(hseed, "shoot-leaf", cand.idx, si))
            leaf_kw = dict(leaf_kw_base, length_mm=Ls, width_mm=Ws)

            stats.n_attempted += 1
            result, reason = _attempt_leaf(
                meshes[mi], neighbours[mi], base, bn, T_leaf, Ls, Ws, leaf_kw, lseed,
            )
            if result is None:
                if reason == "buried":
                    stats.skipped_cross_buried += 1
                elif reason == "floor":
                    stats.skipped_below_floor += 1
                else:
                    stats.build_errors += 1
                continue
            solid, tangent_leaf, skew_mm, tip_clr = result

            # ── COMMIT ────────────────────────────────────────────────────────
            root_grid.add(_root_cell(base, gap))
            row_idx = int((float(base[2]) - z_mins[mi]) / expected_row_step)
            stats.base_positions.append(base.copy())
            stats.base_tangents.append(tangent_leaf.copy())
            stats.base_row_idx.append(row_idx)
            stats.root_depths.append(_GREEDY_EMBED_MM)
            stats.leaf_float_dists.append(0.0)
            stats.leaf_buried_depths.append(0.0)
            stats.shingle_layers.append(0)
            stats.tip_z_clearances.append(tip_clr)
            stats.tip_z_lifts.append(skew_mm)
            stats.pull_aways.append(0.0)
            stats.n_placed += 1
            placed_in_shoot += 1

            if len(solid.vertices) > 0:
                if row_color_fn is not None:
                    color = np.asarray(row_color_fn(n_shoots), dtype=np.uint8)
                    solid.visual = trimesh.visual.ColorVisuals(
                        mesh=solid,
                        face_colors=np.tile(color, (len(solid.faces), 1)),
                    )
                parts_list[mi].append(solid)

        if placed_in_shoot > 0:
            n_shoots += 1

    elapsed = time.perf_counter() - t_total
    if verbose:
        placed = sum(s.n_placed for s in stats_list)
        print(
            f"\n── shoot leaf placement ──  {placed} leaves in {n_shoots} shoots  "
            f"({len(all_cands)} candidates: root-rej={n_rejected_root} "
            f"short-rej={n_rejected_short})  {elapsed:.3f}s\n"
        )

    return parts_list, stats_list
