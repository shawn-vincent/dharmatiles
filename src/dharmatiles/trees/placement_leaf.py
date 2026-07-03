"""Shared per-leaf machinery for leaf placement.

Everything the organic placer needs to seat, build, and cull ONE leaf
against a foliage clump surface, plus the stats container.  Distilled from
the retired meridian/greedy/shoots placers (2026-07-03): the equal-depth
oval seat and rigid blade↔oval frame were born in the greedy placer, the
parameterised :func:`_attempt_leaf` in the shoots placer; the organic
placer (:mod:`placement_organic`) is the sole surviving caller.
"""
from __future__ import annotations

import dataclasses
import math

import numpy as np
import trimesh

from ._utils import _safe_norm
from .leaf import (
    _LEAF_N_LAT,
    _LEAF_N_LONG,
    build_leaf_oval_offsets,
    build_leaf_surface,
    solidify_leaf,
)

# ── Seat constants ────────────────────────────────────────────────────────────
# Root oval embed depth.  The placer works directly on the real (noised) clump,
# so the root seats a fixed shallow amount just below the actual foliage surface
# at the candidate point.  (No deep smooth-envelope embed: that produced
# over-long necks and forced large tip-z lifts.)  It is trivially in real
# material, so no separate connection gate.
_ROOT_EMBED_MM: float = 0.75

# Blade standoff above the real foliage surface: the blade's CLOSEST vertex is
# placed exactly this far off the noised surface (enforced per leaf by the
# adaptive belly-dip seat in _attempt_leaf).
_PROTRUSION_MM: float = 0.3

# Printability skew: the blade tip must clear the root oval's tip in world z by
# at least this margin, else the tip-end walls overhang downward (FDM-unprintable).
# The blade is slid in-plane toward the base until the margin holds.
_SKEW_TIP_MARGIN_MM: float = 0.05

# Surface re-projection: the query point is lifted this far along the local
# normal, then dropped back onto the surface by ray cast.
_PROJECT_LIFT_MM: float = 1.0


# ── Stats container ───────────────────────────────────────────────────────────

@dataclasses.dataclass
class LeafPlacementStats:
    """Metrics collected during leaf placement on one mesh object."""
    label: str = ""
    # Totals
    n_rows: int = 0
    n_attempted: int = 0       # candidates that reached the leaf-build step
    n_placed: int = 0          # leaves successfully solidified
    # Skip / error breakdown
    skipped_downward: int = 0   # up_hint.z < -0.5 (downward-facing surface)
    skipped_small_r: int = 0   # local_r < 1.0 mm (too close to centroid)
    skipped_preburied: int = 0  # post-placement: curl region buried
    skipped_below_floor: int = 0  # midrib tip (base + L*tangent) below mesh z_min
    skipped_cross_buried: int = 0  # base inside another cluster's solid
    contact_angle_clamped: int = 0  # initial contact angle ≥ π/2, clamped to max
    build_errors: int = 0      # RuntimeError / ValueError from leaf builder
    # Per-row data: (z, attempted, placed) — one entry per generated row
    rows: list[tuple[float, int, int]] = dataclasses.field(default_factory=list)
    # Per-leaf data
    base_positions: list[np.ndarray] = dataclasses.field(default_factory=list)
    base_tangents: list[np.ndarray] = dataclasses.field(default_factory=list)
    base_row_idx: list[int] = dataclasses.field(default_factory=list)
    root_depths: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum distance of any curl-region vertex (> L/2 from base)
    # that lies OUTSIDE the mesh.  Near zero = curl pressed against mesh.
    leaf_float_dists: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum depth of any curl-region vertex that lies INSIDE the
    # mesh (unsigned distance to nearest surface point).  > 0 = buried.
    leaf_buried_depths: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf shingle layer (0 = flat on surface, n = stood off by n×delta).
    shingle_layers: list[int] = dataclasses.field(default_factory=list)
    # Per-leaf: visible blade-tip z minus embedded root-oval-tip z, after any
    # tiny blade-only correction.  Should never be negative for placed leaves.
    tip_z_clearances: list[float] = dataclasses.field(default_factory=list)
    tip_z_lifts: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: outward normal translation applied to slide the blade clear of
    # the parent mesh.  0 = blade already clear.
    pull_aways: list[float] = dataclasses.field(default_factory=list)
    # Per-row perimeter (sum of all polygon perimeters in that cross-section slice)
    row_perims: list[float] = dataclasses.field(default_factory=list)
    # Geometry parameters (filled in once after the row loop)
    leaf_length_mm: float = 0.0      # L
    leaf_width_mm: float = 0.0       # W — used for effective-overlap calculation
    col_step: float = 0.0            # min expected spacing between same-row leaves
    expected_row_step: float = 0.0   # L * (1 - v_overlap) — target z-gap between rows
    z_top: float = 0.0               # mesh apex Z
    z_top_anchor: float = 0.0        # expected topmost row Z ≈ z_top − 0.25·L
    cx: float = 0.0                  # mesh centroid X (for phi-sector analysis)
    cy: float = 0.0                  # mesh centroid Y
    lift_mm: float = 0.0             # leaf lift parameter used for this run


# ── Small vector helper ───────────────────────────────────────────────────────

def _rotate_about(v: np.ndarray, axis: np.ndarray, angle_rad: float) -> np.ndarray:
    """Rodrigues rotation of ``v`` about unit ``axis`` by ``angle_rad``."""
    c = math.cos(angle_rad)
    s = math.sin(angle_rad)
    return v * c + np.cross(axis, v) * s + axis * float(np.dot(axis, v)) * (1.0 - c)


# ── Batched embree dispatch ───────────────────────────────────────────────────
# The seat/build/cull pipeline below is written as GENERATORS that yield embree
# requests instead of calling embree inline:
#
#     inside          = yield ("contains", mesh, pts)           # → bool (n,)
#     loc, ridx, tri  = yield ("ray", mesh, origins, dirs)      # → first hits
#
# Two drivers execute the requests.  :func:`_drive_scalar` runs ONE generator
# with an immediate embree call per request — behaviourally identical to the
# old inline code.  :func:`_drive_batched` advances MANY generators in rounds,
# concatenating every same-kind/same-mesh request of a round into ONE embree
# call.  ``contains`` (ray-parity per point) and first-hit ray queries are
# per-point/per-ray independent, so batching returns identical results — it
# only amortises the ~85 µs fixed per-call overhead that dominated placement
# (~26 one-or-two-point calls per leaf; see docs/meta/history/
# 2026-07-03-organic-leaf-placer-performance-analysis.md).

def _exec_request(req):
    """Execute one embree request tuple immediately (scalar path)."""
    if req[0] == "contains":
        _kind, mesh, pts = req
        return np.asarray(mesh.contains(pts), dtype=bool)
    _kind, mesh, origins, dirs = req
    loc, ridx, tri = mesh.ray.intersects_location(origins, dirs, multiple_hits=False)
    return (
        np.asarray(loc, dtype=float).reshape(-1, 3),
        np.asarray(ridx, dtype=int),
        np.asarray(tri, dtype=int),
    )


def _drive_scalar(gen):
    """Run one request generator to completion; return its return value."""
    resp = None
    try:
        while True:
            resp = _exec_request(gen.send(resp))
    except StopIteration as stop:
        return stop.value


def _drive_batched(gens: list) -> list:
    """Advance many request generators in rounds with grouped embree calls.

    Each round gathers every pending generator's request, groups them by
    ``(kind, mesh)``, executes one embree call per group, and hands each
    generator back exactly the slice of results its own request produced
    (ray hits are remapped to generator-local ray indices).  Generators
    progress independently — one can be on its third seat iteration while
    another is already at the tuck stage; rounds simply mix stages.

    Returns the generators' return values, in input order.
    """
    results: list = [None] * len(gens)
    pending: dict[int, tuple] = {}
    for i, g in enumerate(gens):
        try:
            pending[i] = g.send(None)
        except StopIteration as stop:
            results[i] = stop.value
    while pending:
        groups: dict[tuple, list[int]] = {}
        for i, req in pending.items():
            groups.setdefault((req[0], id(req[1])), []).append(i)
        responses: dict[int, object] = {}
        for idxs in groups.values():
            reqs = [pending[i] for i in idxs]
            kind, mesh = reqs[0][0], reqs[0][1]
            counts = [len(r[2]) for r in reqs]
            if kind == "contains":
                ins = np.asarray(
                    mesh.contains(np.concatenate([r[2] for r in reqs], axis=0)),
                    dtype=bool,
                )
                o = 0
                for i, k in zip(idxs, counts):
                    responses[i] = ins[o:o + k]
                    o += k
            else:
                origins = np.concatenate([r[2] for r in reqs], axis=0)
                dirs = np.concatenate([r[3] for r in reqs], axis=0)
                loc, ridx, tri = mesh.ray.intersects_location(
                    origins, dirs, multiple_hits=False,
                )
                loc = np.asarray(loc, dtype=float).reshape(-1, 3)
                ridx = np.asarray(ridx, dtype=int)
                tri = np.asarray(tri, dtype=int)
                o = 0
                for i, k in zip(idxs, counts):
                    sel = (ridx >= o) & (ridx < o + k)
                    responses[i] = (loc[sel], ridx[sel] - o, tri[sel])
                    o += k
        nxt: dict[int, tuple] = {}
        for i, resp in responses.items():
            try:
                nxt[i] = gens[i].send(resp)
            except StopIteration as stop:
                results[i] = stop.value
        pending = nxt
    return results


# ── Surface sampling / projection ─────────────────────────────────────────────

def _sample_surface(mesh: trimesh.Trimesh, n: int, rng: np.random.Generator):
    """Deterministic area-weighted surface samples → (points, face_index)."""
    areas = mesh.area_faces
    total = float(areas.sum())
    if total <= 0.0 or len(areas) == 0:
        return np.zeros((0, 3)), np.zeros(0, dtype=np.int64)
    cum = np.cumsum(areas)
    cum /= cum[-1]
    fi = np.searchsorted(cum, rng.random(n)).astype(np.int64)
    fi = np.clip(fi, 0, len(areas) - 1)
    tris = mesh.triangles[fi]                     # (n, 3, 3)
    u = rng.random((n, 1))
    v = rng.random((n, 1))
    over = (u + v) > 1.0
    u[over] = 1.0 - u[over]
    v[over] = 1.0 - v[over]
    pts = tris[:, 0] + u * (tris[:, 1] - tris[:, 0]) + v * (tris[:, 2] - tris[:, 0])
    return pts, fi


def _growth_tangent(normal: np.ndarray, base: np.ndarray, centroid: np.ndarray) -> np.ndarray:
    """Steepest-DESCENT growth direction in the local tangent plane.

    Every leaf points straight down-slope — world-down projected onto the surface
    tangent plane — so leaves are always vertical (along the surface but pointing
    down), never sideways and never up.  Near the apex, where the down projection
    degenerates, fall back to radially-outward-in-XY (which still heads down over
    the crown).
    """
    down = np.array([0.0, 0.0, -1.0])
    d = down - float(np.dot(down, normal)) * normal
    dl = float(np.linalg.norm(d))
    if dl > 1e-6:
        return d / dl
    # Apex (normal ≈ world-up): no down-slope direction — grow radially outward,
    # which then heads down over the crown.
    radial = np.array([base[0] - centroid[0], base[1] - centroid[1], 0.0])
    r = radial - float(np.dot(radial, normal)) * normal
    rl = float(np.linalg.norm(r))
    if rl > 1e-6:
        return r / rl
    return _safe_norm(np.array([1.0, 0.0, 0.0]) - float(normal[0]) * normal)


def _project_to_surface_gen(
    mesh: trimesh.Trimesh,
    Q: np.ndarray,
    n: np.ndarray,
    max_drop_mm: float,
):
    """Drop ``Q`` onto the surface along ``−n`` from a short lift above it.

    Request generator (see "Batched embree dispatch" above).  Returns
    ``(point, smooth_normal)`` — the smooth normal is the barycentric
    blend of the hit triangle's vertex normals, matching the placer's
    candidate sampling — or ``None`` when the ray misses or lands implausibly
    far (past a rim / on the far side).
    """
    G = Q + _PROJECT_LIFT_MM * n
    loc, _ray_idx, tri_idx = yield ("ray", mesh, G[np.newaxis], (-n)[np.newaxis])
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


def _project_to_surface(
    mesh: trimesh.Trimesh,
    Q: np.ndarray,
    n: np.ndarray,
    max_drop_mm: float,
) -> tuple[np.ndarray, np.ndarray] | None:
    """Scalar wrapper around :func:`_project_to_surface_gen`."""
    return _drive_scalar(_project_to_surface_gen(mesh, Q, n, max_drop_mm))


# ── Containment probe ─────────────────────────────────────────────────────────

def _points_inside_any_gen(meshes: list, pts: np.ndarray, near_pt: np.ndarray, reach: float):
    """True if ANY of ``pts`` is inside any neighbour solid (AABB-pruned by ``near_pt``).

    Request generator (see "Batched embree dispatch" above).  Neighbours the
    leaf cannot reach (bbox expanded by ``reach``) are skipped, so the test
    stays O(k) on a dense canopy.  ``pts`` is a single point (shape (1,3)) for
    the seam pre-test or the blade's widest→tip vertices for the burial test.
    """
    if len(pts) == 0:
        return False
    for _m in meshes:
        _b = _m.bounds
        if (near_pt < _b[0] - reach).any() or (near_pt > _b[1] + reach).any():
            continue
        inside = yield ("contains", _m, pts)
        if bool(inside.any()):
            return True
    return False


def _points_inside_any(meshes: list, pts: np.ndarray, near_pt: np.ndarray, reach: float) -> bool:
    """Scalar wrapper around :func:`_points_inside_any_gen`."""
    return _drive_scalar(_points_inside_any_gen(meshes, pts, near_pt, reach))


# ── Oval seating: equal-depth pitch solve against the real mesh ───────────────

def _seat_oval_tilt_gen(
    mesh: trimesh.Trimesh,
    P0: np.ndarray, n0: np.ndarray, T0: np.ndarray,
    L: float, embed_mm: float,
    *,
    max_iter: int = 3,
    tol_mm: float = 0.05,
    max_tilt_rad: float = math.radians(60.0),
):
    """Pitch the rigid root oval about its own center until both ends sit
    equally deep below the REAL (noised) surface.

    Request generator (see "Batched embree dispatch" above).

    The oval is a rigid segment of half-span ``L/4`` centered at
    ``C = P0 − embed·n0`` with its axis initially down-slope (``T0``).  Each
    iteration measures both ends' depth along ``±n0`` (embree rays; negative
    when an end pokes outside the mesh) and rotates by
    ``asin(imbalance / (L/2))`` — the Newton step that splits the difference:
    the deep end comes up by half the imbalance, the shallow end goes down by
    half.  Curvature and surface noise make one step inexact, so it is
    iterated (converges geometrically; ``max_iter`` is small on purpose).

    Returns the pitch angle in radians (positive = down-slope end tilts
    deeper), or ``None`` when a depth ray misses the mesh or the tilt cap is
    exceeded — pathological spots (past a rim, thin shell) that must be
    culled, not forced.
    """
    C = P0 - embed_mm * n0
    h = 0.25 * L          # oval half-span: the oval spans [L/2, L] ⇒ length L/2
    span = 0.5 * L
    theta = 0.0
    for _ in range(max_iter):
        c, s = math.cos(theta), math.sin(theta)
        t = T0 * c - n0 * s
        ends = np.array([C - h * t, C + h * t])     # [near (up-slope), far]
        inside = yield ("contains", mesh, ends)
        # Depth along the base normal: embedded ends cast outward (+n0) to the
        # exit; poking ends cast inward (−n0) to the surface below (negative).
        dirs = np.where(inside[:, np.newaxis], n0[np.newaxis], -n0[np.newaxis])
        loc, ray_idx, _ = yield ("ray", mesh, ends, dirs)
        if {int(i) for i in ray_idx} != {0, 1}:
            return None
        d = np.zeros(2)
        for k, ri in enumerate(ray_idx):
            ri = int(ri)
            dist = float(np.linalg.norm(loc[k] - ends[ri]))
            d[ri] = dist if inside[ri] else -dist
        imbalance = d[0] - d[1]                     # d_near − d_far
        if abs(imbalance) <= tol_mm:
            break
        theta += math.asin(max(-1.0, min(1.0, imbalance / span)))
        if abs(theta) > max_tilt_rad:
            return None
    return theta


def _seat_oval_tilt(
    mesh: trimesh.Trimesh,
    P0: np.ndarray, n0: np.ndarray, T0: np.ndarray,
    L: float, embed_mm: float,
    **kw,
) -> float | None:
    """Scalar wrapper around :func:`_seat_oval_tilt_gen`."""
    return _drive_scalar(_seat_oval_tilt_gen(mesh, P0, n0, T0, L, embed_mm, **kw))


# ── Leaf frame + root oval, both in the leaf's own frame ──────────────────────

def _leaf_frame_and_oval(
    P0: np.ndarray, n0: np.ndarray, T0: np.ndarray,
    L: float, W: float, embed_mm: float, protrusion_mm: float,
    tilt_rad: float,
):
    """Build the blade frame and its root oval in the LEAF's own frame,
    centered on the candidate point.

    The candidate point ``P0`` is the surface point directly above the oval
    CENTER.  The oval — rigid, absolute dimensions, spanning ``[L/2, L]`` of
    the leaf's own frame (via :func:`build_leaf_oval_offsets`) — is centered
    at ``P0 − embed·n0`` and pitched about that center by ``tilt_rad``, the
    angle :func:`_seat_oval_tilt` solved against the real mesh so both oval
    ends sit equally deep.  The blade is rigidly attached in the same frame
    (its base anchor is the oval-frame origin, ``0.75·L`` up-slope of the
    center, protruding ``protrusion_mm`` along ``n0``), so blade and oval
    share origin, direction and length BY CONSTRUCTION and the 1:1 index
    stitch in :func:`solidify_leaf` produces a short tapered neck everywhere.
    Placing the oval is the primary act; the blade comes along for the ride.

    Whether the seated oval actually sits inside the clump is a separate
    question, answered by the caller's containment guard on the oval end
    vertices.

    Returns ``(surf_base, tangent_leaf, up_leaf, inner_v)`` — ``inner_v`` is the
    123-vertex oval (leaf-surface layout, for :func:`solidify_leaf`) — or
    ``None`` if degenerate.
    """
    ca_c = math.cos(tilt_rad)
    ca_s = math.sin(tilt_rad)
    tangent_leaf = _safe_norm(T0 * ca_c - n0 * ca_s)
    up_leaf      = _safe_norm(n0 * ca_c + T0 * ca_s)
    lat = np.cross(n0, tangent_leaf)
    ll = float(np.linalg.norm(lat))
    if ll < 1e-6:
        return None
    lat = lat / ll

    # Oval-frame origin: the offsets put the oval center at 0.75·L·T − embed·n̂
    # relative to it, so anchoring the center at P0 − embed·n0 means the origin
    # sits 0.75·L up-slope of the candidate point.
    origin = P0 - 0.75 * L * tangent_leaf
    inner_v = build_leaf_oval_offsets(
        n_hat=n0, T_along=tangent_leaf, across=lat,
        L=L, W=W, embed_mm=embed_mm,
    ) + origin[np.newaxis]

    # Leaf surface base: protrusion above the oval-frame origin along the
    # surface normal (same rigid blade↔oval relation as before; only the
    # anchor point moved from the base to the oval center).
    surf_base = origin + protrusion_mm * n0
    return surf_base, tangent_leaf, up_leaf, inner_v


# ── Per-leaf attempt: seat → build → skew → seat → cull → solidify ────────────

def _attempt_leaf_gen(
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
    tuck_base: bool = False,
    tuck_tip: bool = False,
    tuck_tip_max_mm: float = 0.02,
    arch_mm: float = 0.0,
):
    """Seat, build, and cull one leaf at a candidate point.

    Request generator (see "Batched embree dispatch" above): all embree
    queries are yielded, so a batch driver can run MANY leaf attempts with
    grouped calls; :func:`_attempt_leaf` is the immediate scalar form.

    The full per-leaf pipeline: equal-depth oval seat → rigid frame → oval
    containment guard → build → printability skew → adaptive belly-dip seat
    → tip/belly burial cull → solidify, with ``L``/``W`` free so callers can
    carry scaled leaves.

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
    tilt = yield from _seat_oval_tilt_gen(mesh, base, normal, T0, L, _ROOT_EMBED_MM)
    if tilt is None:
        # The equal-depth solve gives up on pathological spots (crease
        # pockets, rims).  For coverage-guaranteeing placers a FLAT seat
        # beats a hole; classic placers keep the cull.
        if seat_fallback_flat:
            tilt = 0.0
        else:
            return None, "buried-seat"
    frame = _leaf_frame_and_oval(
        base, normal, T0, L, W, _ROOT_EMBED_MM, _PROTRUSION_MM, tilt,
    )
    if frame is None:
        return None, "error"
    surf_base, tangent_leaf, up_leaf, inner_v = frame

    oval_inside = yield ("contains", mesh, inner_v[[-2, -1]])
    if not bool(oval_inside.all()):
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

    # End-to-end lengthwise ARCH (flush/underside blades): a parabolic
    # offset 4·s·(1−s)·arch_mm along the seat normal.  Unlike curl (which
    # bends the tip back down and can re-enter the surface) a pure arch
    # only ever rises between its endpoints, so a blade whose ends touch a
    # convex bulge clears it mid-span instead of chording through it.
    if arch_mm > 0.0:
        s_prof = np.zeros(len(surf.vertices))
        s_ring = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]
        stride = _LEAF_N_LAT + 1
        for ri, sv in enumerate(s_ring):
            s_prof[ri * stride:(ri + 1) * stride] = sv
        s_prof[base_idx] = 0.0
        s_prof[tip_idx] = 1.0
        bulge = 4.0 * s_prof * (1.0 - s_prof) * arch_mm
        surf.vertices = surf.vertices + bulge[:, np.newaxis] * normal

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
    # displacement along the leaf normal, located on the built surface via
    # the ring layout.  Translate the blade along ±normal so the belly
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
    dip_inside = bool((yield ("contains", mesh, dip[np.newaxis]))[0])
    ray_dir = normal if dip_inside else -normal
    loc, ridx, _ = yield ("ray", mesh, dip[np.newaxis], ray_dir[np.newaxis])
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

    if tuck_base:
        # Tuck the BASE end into the foliage: pitch the blade about its
        # belly dip until the base sits slightly embedded.  The belly-seat
        # pins the belly at protrusion+standoff but leaves the base end
        # free — a base that finishes proud of the surface is the "lower
        # leaf's base lying over a higher leaf's tip" artifact.  An
        # embedded base can never overlie anything (and anchors better).
        bv = surf.vertices[base_idx]
        b_in = bool((yield ("contains", mesh, bv[np.newaxis]))[0])
        rd = normal if b_in else -normal
        loc, ridx, _tt = yield ("ray", mesh, bv[np.newaxis], rd[np.newaxis])
        if len(ridx):
            bc = (-1.0 if b_in else 1.0) * float(np.linalg.norm(loc[0] - bv))
            target = -0.2
            pivot = surf.vertices[dip_idx].copy()
            r = float(np.linalg.norm(bv - pivot))
            if bc > target and r > 1e-6:
                theta = min(
                    math.asin(min((bc - target) / r, 1.0)),
                    math.radians(30.0),
                )
                lat = _safe_norm(np.cross(normal, tangent_leaf))
                # Pick the rotation sign that moves the base along −normal.
                v0 = bv - pivot
                for sgn in (1.0, -1.0):
                    if float(np.dot(
                        _rotate_about(v0, lat, sgn * theta) - v0, -normal,
                    )) > 0.0:
                        break
                c = math.cos(sgn * theta)
                sn = math.sin(sgn * theta)
                V = surf.vertices - pivot
                surf.vertices = pivot + (
                    V * c
                    + np.cross(np.broadcast_to(lat, V.shape), V) * sn
                    + np.outer(V @ lat, lat) * (1.0 - c)
                )

    if tuck_tip:
        # Bury the TIP: on underside-hugging (flush) blades even a gentle
        # curl leaves the tip hanging in air below the blade — a floating
        # island for FDM.  Pitch about the (embedded) base vertex until
        # the tip dives slightly into the clump: the leaf becomes a
        # relief-like arc with both ends rooted; nothing hangs.
        tv = surf.vertices[tip_idx]
        t_in = bool((yield ("contains", mesh, tv[np.newaxis]))[0])
        rd = normal if t_in else -normal
        loc, ridx, _tt = yield ("ray", mesh, tv[np.newaxis], rd[np.newaxis])
        if len(ridx):
            tc = (-1.0 if t_in else 1.0) * float(np.linalg.norm(loc[0] - tv))
            # Tip clearance CEILING (never a burial): pull the tip down
            # only when it floats above tuck_tip_max_mm.  Callers blend
            # the ceiling by surface zone — generous on upward faces,
            # touching (~0.02) on undersides — for a gradual tip-height
            # transition down the canopy.
            target = tuck_tip_max_mm
            pivot = surf.vertices[base_idx].copy()
            r = float(np.linalg.norm(tv - pivot))
            if tc > target and r > 1e-6:
                theta = min(
                    math.asin(min((tc - target) / r, 1.0)),
                    math.radians(35.0),
                )
                lat = _safe_norm(np.cross(normal, tangent_leaf))
                v0 = tv - pivot
                for sgn in (1.0, -1.0):
                    if float(np.dot(
                        _rotate_about(v0, lat, sgn * theta) - v0, -normal,
                    )) > 0.0:
                        break
                c = math.cos(sgn * theta)
                sn = math.sin(sgn * theta)
                V = surf.vertices - pivot
                surf.vertices = pivot + (
                    V * c
                    + np.cross(np.broadcast_to(lat, V.shape), V) * sn
                    + np.outer(V @ lat, lat) * (1.0 - c)
                )

    curl_mask = np.linalg.norm(
        surf.vertices - surf.vertices[base_idx], axis=1,
    ) > (L / 2.0)
    curl_idx = np.nonzero(curl_mask)[0]
    if len(curl_idx) == 0:
        curl_idx = np.arange(len(surf.vertices))
    belly_idx = int(curl_idx[int(np.argmin(surf.vertices[curl_idx, 2]))])
    probe = surf.vertices[np.array([tip_idx, belly_idx])]
    # tuck_tip blades are INTENTIONALLY buried at both ends; the burial
    # cull (and its bury-lift) would pop the tucked tip back out.
    if not tuck_tip and (
        yield from _points_inside_any_gen([mesh, *neighbour_meshes], probe, base, L)
    ):
        # bury_lift (organic placer): a probe dipping into the PARENT
        # surface — e.g. the tip curling under at a dome crown — is lifted
        # out along the seat normal instead of culled (a cull would leave a
        # permanent coverage hole).  Only the parent mesh is measured; if a
        # neighbour is the buried-in solid (or the lift doesn't clear), the
        # cull stands.
        lifted = False
        if bury_lift:
            inside = yield ("contains", mesh, probe)
            if inside.any():
                idx = np.nonzero(inside)[0]
                dirs = np.tile(normal[np.newaxis], (len(idx), 1))
                loc, ridx, tri = yield ("ray", mesh, probe[idx], dirs)
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
                    # CAP the lift: an unbounded lift stretched the stitch
                    # walls into the long extrusions (and ran after the neck
                    # gate, ungated).  Lift at most 0.8 mm; whatever remains
                    # buried stays TUCKED into the foliage — visually fine,
                    # and anchored better, not worse.
                    surf.vertices = (
                        surf.vertices
                        + min(same_wall_depth + 0.05, 0.8) * normal
                    )
                lifted = True
        if not lifted:
            return None, "buried-probe"

    try:
        solid, _ = solidify_leaf(surf, inner_v)
    except (RuntimeError, ValueError):
        return None, "error"

    tip_z_clearance = float(surf.vertices[tip_idx][2]) - float(inner_v[-1][2])
    return (solid, tangent_leaf, skew_mm, tip_z_clearance, drop_mm), None


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
    **kw,
):
    """Scalar wrapper around :func:`_attempt_leaf_gen` (one leaf, immediate embree)."""
    return _drive_scalar(_attempt_leaf_gen(
        mesh, neighbour_meshes, base, normal, T0, L, W, leaf_kw, lseed, **kw,
    ))
