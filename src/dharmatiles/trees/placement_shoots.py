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
from .leaf import _LEAF_N_LAT, _LEAF_N_LONG, build_leaf_surface, solidify_leaf
from .mesh import _hash01_int
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

# Underside floor for shoots, looser than the shared
# _LEAF_PLACEABLE_NORMAL_Z (−0.5): permits bases on surfaces facing up to
# ~40° below horizontal, pushing the leaf fringe further under the clump so
# the bare underside zone shrinks.  Print risk rises on that last ring
# (steeper down-slope blades); tighten back toward −0.5 if prints droop.
_SHOOT_PLACEABLE_NORMAL_Z: float = -0.65


# ── Exact-distance root grid ──────────────────────────────────────────────────
# The greedy placer's cell-set grid (_root_occupied_near) blocks any point
# whose cell is within the 3×3×3 neighbourhood of a claimed cell — an
# effective exclusion radius of 1×–2× the gap.  The z-band outcome trace
# showed that over-blocking starving the clump tops (each root shadowed up
# to ~4× the area its blade covers), so the shoots placer stores the actual
# root points per cell and tests true distance: exclusion radius = gap,
# exactly.

def _root_blocked(root_cells: dict, pt: np.ndarray, gap: float) -> bool:
    ix, iy, iz = _root_cell(pt, gap)
    g2 = gap * gap
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                pts = root_cells.get((ix + dx, iy + dy, iz + dz))
                if pts is not None and any(
                    float(((pt - q) ** 2).sum()) < g2 for q in pts
                ):
                    return True
    return False


def _root_mark(root_cells: dict, pt: np.ndarray, gap: float) -> None:
    root_cells.setdefault(_root_cell(pt, gap), []).append(np.asarray(pt, float))


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
        if float(nn[2]) < _SHOOT_PLACEABLE_NORMAL_Z:
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
    standoff_mm: float = 0.0,
    bury_lift: bool = False,
    seat_fallback_flat: bool = False,
    skip_skew: bool = False,
    max_skew_frac: float = 0.5,
    max_neck_mm: float | None = None,
):
    """Seat, build, and cull one leaf at a shoot station.

    Exactly the greedy sweep body (equal-depth oval seat → rigid frame →
    oval containment guard → build → printability skew → tip/belly burial
    cull → solidify), with ``L``/``W`` free so shoot stations can carry
    scaled leaves.

    ``standoff_mm`` lifts the finished blade along the seat normal AFTER
    the belly-dip drop (root oval stays plugged; the stitch walls stretch)
    — the overlap-layering hook used by the organic placer.  Raising along
    +normal can only increase the tip's z-clearance, so it never undoes
    the printability skew.

    Returns ``((solid, tangent_leaf, skew_mm, tip_z_clearance, drop_mm),
    None)`` on success — ``drop_mm`` is the adaptive seat translation along
    ``−normal`` (positive = moved toward the surface) — or ``(None, reason)``
    with reason in ``{"buried", "floor", "error"}``.
    """
    tilt = _seat_oval_tilt(mesh, base, normal, T0, L, _GREEDY_EMBED_MM)
    if tilt is None:
        # The equal-depth solve gives up on pathological spots (crease
        # pockets, rims).  For coverage-guaranteeing placers a FLAT seat
        # beats a hole; classic placers keep the cull.
        if seat_fallback_flat:
            tilt = 0.0
        else:
            return None, "buried-seat"
    frame = _leaf_frame_and_oval(
        base, normal, T0, L, W, _GREEDY_EMBED_MM, _PROTRUSION_MM, tilt,
    )
    if frame is None:
        return None, "error"
    surf_base, tangent_leaf, up_leaf, inner_v = frame

    if not bool(mesh.contains(inner_v[[-2, -1]]).all()):
        return None, "buried-oval"

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
    # skip_skew: flush blade variants lie ON the substrate (walls under a
    # millimetre, supported by the surface below) — the tip-z overhang rule
    # doesn't apply and the slide would only stretch the stitch walls into
    # long prisms.
    z_need = 0.0 if skip_skew else (
        float(inner_v[-1][2]) + _SKEW_TIP_MARGIN_MM
        - float(surf.vertices[tip_idx][2])
    )
    if z_need > 0.0:
        t_z = float(tangent_leaf[2])
        skew_mm = z_need / -t_z if t_z < -1e-6 else float("inf")
        if skew_mm > max_skew_frac * L:
            return None, "floor"
        surf.vertices = surf.vertices - skew_mm * tangent_leaf

    # ── Adaptive seat on the BELLY DIP (after the skew) ───────────────────
    # The rigid frame anchors the blade BASE _PROTRUSION_MM above the oval
    # origin, but the arch + seat tilt leave the blade's low point hovering
    # up to ~1.4 mm off the clump.  The blade's closest-approach point is a
    # single canonical vertex — the belly dip: the tip-half midrib vertex
    # (or the tip itself, when there is no curl) with the smallest
    # displacement along the leaf normal.  Same definition as
    # :func:`placement._leaf_belly_dip`, located here on the built surface
    # via the ring layout.  Translate the blade along ±normal so the belly
    # dip sits exactly _PROTRUSION_MM above the parent surface: one
    # contains probe + one ray (outside → clearance along −normal; inside
    # → exit depth along +normal, counted negative to force a raise).  A
    # positive drop is capped by the tip's remaining z-clearance over the
    # oval so it never undoes the printability skew above.
    ring_stride = _LEAF_N_LAT + 1
    mid_col = _LEAF_N_LAT // 2
    s_int = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]
    mid_idx = np.nonzero(s_int > 0.5)[0] * ring_stride + mid_col
    cand_idx = np.append(mid_idx, tip_idx)
    d_normal = (surf.vertices[cand_idx] - surf.vertices[base_idx]) @ up_leaf
    dip_idx = int(cand_idx[int(np.argmin(d_normal))])

    drop_mm = 0.0
    dip = surf.vertices[dip_idx]
    dip_inside = bool(mesh.contains(dip[np.newaxis])[0])
    ray_dir = normal if dip_inside else -normal
    loc, ridx, _ = mesh.ray.intersects_location(
        dip[np.newaxis], ray_dir[np.newaxis], multiple_hits=False,
    )
    if len(ridx):
        c = float(np.linalg.norm(loc[0] - dip))
        drop_mm = (-c if dip_inside else c) - _PROTRUSION_MM
        nz = float(normal[2])
        if drop_mm > 0.0 and nz > 1e-6:
            tip_slack = (float(surf.vertices[tip_idx][2])
                         - float(inner_v[-1][2]) - _SKEW_TIP_MARGIN_MM)
            drop_mm = min(drop_mm, max(tip_slack, 0.0) / nz)
        surf.vertices = surf.vertices - drop_mm * normal

    # Overlap-layering standoff (organic placer): lift the blade after the
    # seat so layered leaves ride visibly proud of the layer below.
    if standoff_mm > 0.0:
        surf.vertices = surf.vertices + standoff_mm * normal

    # Neck gate: the blade→oval stitch walls stretch by the accumulated
    # in-plane slide and net normal offset.  Past max_neck_mm they read as
    # wall chimneys/fans ("long-rooted leaves"); reject instead.
    if max_neck_mm is not None:
        net_normal = max(standoff_mm - drop_mm, 0.0)
        if math.hypot(skew_mm, net_normal) > max_neck_mm:
            return None, "neck"

    curl_mask = np.linalg.norm(
        surf.vertices - surf.vertices[base_idx], axis=1,
    ) > (L / 2.0)
    curl_idx = np.nonzero(curl_mask)[0]
    if len(curl_idx) == 0:
        curl_idx = np.arange(len(surf.vertices))
    belly_idx = int(curl_idx[int(np.argmin(surf.vertices[curl_idx, 2]))])
    probe = surf.vertices[np.array([tip_idx, belly_idx])]
    if _points_inside_any([mesh, *neighbour_meshes], probe, base, L):
        # bury_lift (organic placer): a probe dipping into the PARENT
        # surface — e.g. the tip curling under at a dome crown — is lifted
        # out along the seat normal instead of culled (a cull would leave a
        # permanent coverage hole).  Only the parent mesh is measured; if a
        # neighbour is the buried-in solid (or the lift doesn't clear), the
        # cull stands.
        lifted = False
        if bury_lift:
            inside = mesh.contains(probe)
            if inside.any():
                idx = np.nonzero(inside)[0]
                dirs = np.tile(normal[np.newaxis], (len(idx), 1))
                loc, ridx, tri = mesh.ray.intersects_location(
                    probe[idx], dirs, multiple_hits=False,
                )
                ridx = np.asarray(ridx, dtype=int)
                tri = np.asarray(tri, dtype=int)
                # Per-probe classification.  A probe buried under a surface
                # whose normal diverges strongly from the seat normal (>50°)
                # has entered the OPPOSITE wall of an inside corner (union
                # seam) — tucking in is the desired look, keep it.  A probe
                # buried under its OWN wall (exit normal ≈ seat normal, e.g.
                # a tip curling under a dome crown) is lifted out by its
                # measured depth.  A probe whose exit ray misses is a
                # contains() parity graze right at the surface — not a real
                # burial; keep it.
                same_wall_depth = 0.0
                for j in range(len(ridx)):
                    d = float(np.linalg.norm(loc[j] - probe[idx[ridx[j]]]))
                    dot = float(np.dot(mesh.face_normals[tri[j]], normal))
                    if dot >= 0.64:
                        same_wall_depth = max(same_wall_depth, d)
                if same_wall_depth > 0.0:
                    surf.vertices = (
                        surf.vertices + (same_wall_depth + 0.05) * normal
                    )
                    probe = surf.vertices[np.array([tip_idx, belly_idx])]
                    lifted = not _points_inside_any(
                        [mesh, *neighbour_meshes], probe, base, L,
                    )
                else:
                    lifted = True
        if not lifted:
            return None, "buried-probe"

    try:
        solid, _ = solidify_leaf(surf, inner_v)
    except (RuntimeError, ValueError):
        return None, "error"

    tip_z_clearance = float(surf.vertices[tip_idx][2]) - float(inner_v[-1][2])
    return (solid, tangent_leaf, skew_mm, tip_z_clearance, drop_mm), None


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
    debug_outcomes: list | None = None,
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
    # With the exact-distance root grid this IS the true min spacing (the
    # greedy cell-set grid effectively doubles its gap).  Tuned on the
    # multi-parent test: 0.4 over-packs (~2× overlap everywhere), 0.65 goes
    # sparse with bare lanes; 0.5 gives full coverage with a healthy
    # shingle overlap.
    gap = float(min_root_gap_mm) if min_root_gap_mm is not None else max(W * 0.5, 1e-3)
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
            normal_z_floor=_SHOOT_PLACEABLE_NORMAL_Z,
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

    root_grid: dict[tuple[int, int, int], list[np.ndarray]] = {}
    n_rejected_root = 0
    n_rejected_short = 0
    n_shoots = 0

    def _note(cand, outcome: str) -> None:
        # Dev hook: per-candidate outcome trace for coverage diagnosis.
        if debug_outcomes is not None:
            debug_outcomes.append((cand.mesh_id, float(cand.z), outcome))

    for cand in all_cands:
        mi = cand.mesh_id
        stats = stats_list[mi]

        # Shoot start blocked by an already-claimed leaf root → dense enough here.
        if _root_blocked(root_grid, cand.base, gap):
            n_rejected_root += 1
            _note(cand, "root-blocked")
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
            if not _root_blocked(root_grid, st[0], gap)
            and not _points_inside_any(neighbours[mi], st[0][np.newaxis], st[0], 0.0)
        ]
        # Place whatever free stations exist — even a single one.  An earlier
        # rule rejected shoots shortened below 2 stations by grid conflicts,
        # reasoning that neighbouring leaves already covered the area; the
        # z-band outcome trace disproved that: a claimed root blocks a
        # 1.2–2.4 mm disc in EVERY direction while its blade covers only an
        # up-slope-pointing patch, so grid-shortened shoots concentrated at
        # the clump tops and their rejection left the domes bare.
        if len(free) == 0:
            n_rejected_short += 1
            _note(cand, f"short(march={len(stations)},free=0)")
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
                if float(bn[2]) < _SHOOT_PLACEABLE_NORMAL_Z:
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
                if reason.startswith("buried"):
                    stats.skipped_cross_buried += 1
                elif reason == "floor":
                    stats.skipped_below_floor += 1
                else:
                    stats.build_errors += 1
                if debug_outcomes is not None:
                    debug_outcomes.append((mi, float(base[2]), f"leaf-{reason}"))
                continue
            if debug_outcomes is not None:
                debug_outcomes.append((mi, float(base[2]), "placed"))
            solid, tangent_leaf, skew_mm, tip_clr, drop_mm = result

            # ── COMMIT ────────────────────────────────────────────────────────
            _root_mark(root_grid, base, gap)
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
            # Outward-positive convention: the adaptive seat drop moves the
            # blade TOWARD the surface, so it lands here negated.
            stats.pull_aways.append(-drop_mm)
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
