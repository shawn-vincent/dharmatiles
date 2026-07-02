"""Greedy lowest-first leaf placement (experimental, side-by-side).

An alternative to the meridian-arc placer in :mod:`placement`.  Instead of a
deterministic rows×columns grid per cluster, this grows the canopy by greedy
accretion over a **pre-generated** candidate set, working directly on the real
(noised) foliage clumps:

    Sample candidate bases on each real clump surface, each with an analytic
    pose (interpolated normal; horizontal-outward growth).  Sort by world-z
    ascending.  Sweep upward once; accept a candidate iff it clears a cheap
    reject ladder on self-managed hash grids.  Build geometry ONLY for accepted
    candidates.

Design points
-------------
* **Roots seat just below the real surface.**  The root oval embeds a fixed
  shallow depth (:data:`_GREEDY_EMBED_MM`) at the candidate point — like the
  meridian placer — so it is trivially in real material (no smooth-envelope
  bookkeeping, no separate connection gate, no over-long neck).
* **The oval is the thing being placed; the blade comes along rigidly.**  The
  candidate point is the oval CENTER.  The oval keeps its absolute dimensions
  and is pitched about its own center until both ends sit equally deep in the
  real mesh (:func:`_seat_oval_tilt` — an iterated split-the-difference Newton
  step on two embree depth rays).  The blade is derived from the seated frame
  (:func:`_leaf_frame_and_oval`), so the solidify stitch is aligned by
  construction.  The mesh is only ever asked "how deep is this point?", never
  "where should this point be" — no closest-point footprints, no sphere fit.
* **Clearance is a containment cull, not a search.**  A leaf's tip and belly
  must clear its own clump and neighbour clumps (``mesh.contains`` probes); a
  leaf that doesn't clear is culled, never lifted.
* **Printability skew.**  On down-tilted frames the blade is slid in its own
  plane toward the base (along −tangent) until the blade tip clears the root
  oval's tip in world z — otherwise the tip-end walls overhang downward and
  print unsupported.  Frames needing more than L/2 of slide are culled.

Hard constraints carried over from the 2026-07-01 perf crisis (do NOT violate):
no ``trimesh.proximity.closest_point`` / R-tree / per-leaf ``Trimesh`` scans in
the per-candidate sweep; no ``fix_normals`` on placed leaves; cheap-reject before
every build.  Per-candidate rejection costs only spatial-hash lookups; geometry
is built once per *accepted* leaf.

Public entry point mirrors :func:`placement.place_leaves_on_multiple_meshes` so
the dispatch in :func:`mesh.build_branch_mesh` is a drop-in.
"""
from __future__ import annotations

import math
import time
from collections.abc import Callable

import numpy as np
import trimesh

from ._utils import _safe_norm
from .leaf import (
    build_leaf_oval_offsets,
    build_leaf_surface,
    solidify_leaf,
)
from .mesh import _LEAF_PLACEABLE_NORMAL_Z, _hash01_int
from .placement import LeafPlacementStats

# ── Greedy-specific constants ─────────────────────────────────────────────────
# Root oval embed depth.  The placer works directly on the real (noised) clump,
# so the root seats a fixed shallow amount just below the actual foliage surface
# at the candidate point — the same value the meridian placer uses.  (No deep
# smooth-envelope embed: that produced over-long necks and forced large tip-z
# lifts.)  It is trivially in real material, so no separate connection gate.
_GREEDY_EMBED_MM: float = 0.75

# Blade standoff above the real foliage surface: the blade's CLOSEST vertex is
# placed exactly this far off the noised surface (enforced per leaf by a graze
# translation along the normal — see _min_blade_standoff in the sweep).
_PROTRUSION_MM: float = 0.3

# Printability skew: the blade tip must clear the root oval's tip in world z by
# at least this margin, else the tip-end walls overhang downward (FDM-unprintable).
# The blade is slid in-plane toward the base until the margin holds.
_SKEW_TIP_MARGIN_MM: float = 0.05


# ── Occupancy structures (self-managed, no trimesh proximity) ─────────────────

def _root_cell(pt: np.ndarray, gap: float) -> tuple[int, int, int]:
    return (
        int(math.floor(float(pt[0]) / gap)),
        int(math.floor(float(pt[1]) / gap)),
        int(math.floor(float(pt[2]) / gap)),
    )


def _root_occupied_near(root_grid: set, pt: np.ndarray, gap: float) -> bool:
    """True if any claimed root sits in the 3×3×3 neighbourhood of ``pt``'s cell.

    A coarse gap-sized grid: two roots landing in the same or adjacent cells are
    within ~``gap`` of each other.  This is the constraint-3 "base on bare mesh,
    not another leaf" gate plus the min-root-spacing that turns random
    over-generation into blue-noise placement (dart-throwing).
    """
    ix, iy, iz = _root_cell(pt, gap)
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                if (ix + dx, iy + dy, iz + dz) in root_grid:
                    return True
    return False


def _points_inside_any(meshes: list, pts: np.ndarray, near_pt: np.ndarray, reach: float) -> bool:
    """True if ANY of ``pts`` is inside any neighbour solid (AABB-pruned by ``near_pt``).

    Uses embree ``mesh.contains`` (fast; a hard dep).  Neighbours the leaf cannot
    reach (bbox expanded by ``reach``) are skipped, so the test stays O(k) on a
    dense canopy.  ``pts`` is a single point (shape (1,3)) for the seam pre-test
    or the blade's widest→tip vertices for the burial test.
    """
    if len(pts) == 0:
        return False
    for _m in meshes:
        _b = _m.bounds
        if (near_pt < _b[0] - reach).any() or (near_pt > _b[1] + reach).any():
            continue
        if bool(_m.contains(pts).any()):
            return True
    return False


# ── Oval seating: equal-depth pitch solve against the real mesh ───────────────

def _seat_oval_tilt(
    mesh: trimesh.Trimesh,
    P0: np.ndarray, n0: np.ndarray, T0: np.ndarray,
    L: float, embed_mm: float,
    *,
    max_iter: int = 3,
    tol_mm: float = 0.05,
    max_tilt_rad: float = math.radians(60.0),
) -> float | None:
    """Pitch the rigid root oval about its own center until both ends sit
    equally deep below the REAL (noised) surface.

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
        inside = mesh.contains(ends)
        # Depth along the base normal: embedded ends cast outward (+n0) to the
        # exit; poking ends cast inward (−n0) to the surface below (negative).
        dirs = np.where(inside[:, np.newaxis], n0[np.newaxis], -n0[np.newaxis])
        loc, ray_idx, _ = mesh.ray.intersects_location(ends, dirs, multiple_hits=False)
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


def _min_blade_standoff(
    mesh: trimesh.Trimesh, verts: np.ndarray, n0: np.ndarray,
) -> float | None:
    """Minimum signed height of ``verts`` above the surface, along ``n0``.

    Outside vertices cast a ray along ``−n0`` to the surface below (positive
    standoff); inside vertices cast along ``+n0`` to the exit above (negative).
    Vertices whose ray misses the mesh (past the silhouette) contribute no
    measurement.  Returns ``None`` when nothing hits.
    """
    inside = mesh.contains(verts)
    dirs = np.where(inside[:, np.newaxis], n0[np.newaxis], -n0[np.newaxis])
    loc, ray_idx, _ = mesh.ray.intersects_location(verts, dirs, multiple_hits=False)
    if len(ray_idx) == 0:
        return None
    ray_idx = ray_idx.astype(np.int64)
    d = np.linalg.norm(loc - verts[ray_idx], axis=1)
    signed = np.where(inside[ray_idx], -d, d)
    return float(signed.min())


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


# ── Candidate generation ──────────────────────────────────────────────────────

class _Candidate:
    __slots__ = ("z", "phi", "mesh_id", "idx", "base", "normal", "tangent")

    def __init__(self, z, phi, mesh_id, idx, base, normal, tangent):
        self.z = z
        self.phi = phi
        self.mesh_id = mesh_id
        self.idx = idx
        self.base = base
        self.normal = normal
        self.tangent = tangent


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


def _generate_candidates(
    mesh: trimesh.Trimesh,
    mesh_id: int,
    seed: int,
    *,
    candidate_density: float,
    min_root_gap_mm: float,
) -> list[_Candidate]:
    """Over-generate blue-noise-thinnable candidate poses on the smooth surface."""
    rng = np.random.default_rng(int(seed) & 0xFFFFFFFF)
    area = float(mesh.area)
    if area <= 0.0:
        return []
    # Over-generate relative to the feasible root count (~area / gap²).
    n_cand = int(max(8, candidate_density * area / (min_root_gap_mm ** 2)))
    pts, fi = _sample_surface(mesh, n_cand, rng)
    if len(pts) == 0:
        return []
    # Smooth (barycentric-interpolated vertex) normals — NOT face normals — so
    # leaf base orientation varies smoothly across the coarse icosphere instead
    # of jumping per triangle.  Matches the meridian placer's up_hint.
    tris = mesh.triangles[fi]                                      # (n, 3, 3)
    bary = trimesh.triangles.points_to_barycentric(tris, pts)     # (n, 3)
    vn = mesh.vertex_normals[mesh.faces[fi]]                       # (n, 3, 3)
    normals = np.einsum("nk,nkj->nj", bary, vn)                    # (n, 3)
    _nl = np.linalg.norm(normals, axis=1, keepdims=True)
    normals = np.divide(normals, _nl, out=np.zeros_like(normals), where=_nl > 1e-9)
    cx = float(mesh.vertices[:, 0].mean())
    cy = float(mesh.vertices[:, 1].mean())
    centroid = np.array([cx, cy, float(mesh.vertices[:, 2].mean())])

    cands: list[_Candidate] = []
    for k in range(len(pts)):
        n = normals[k]
        # Cheap printability backstop (edge case #1): reject down-facing bases so
        # leaves never land on hidden undersides.  Root-embed feasibility is the
        # primary gate; this is the reserve normal-elevation floor.
        if float(n[2]) < _LEAF_PLACEABLE_NORMAL_Z:
            continue
        base = pts[k]
        t = _growth_tangent(n, base, centroid)
        phi = math.atan2(float(base[1] - cy), float(base[0] - cx))
        cands.append(_Candidate(float(base[2]), phi, mesh_id, k, base, _safe_norm(n), t))
    return cands


# ── Public entry point ────────────────────────────────────────────────────────

def place_leaves_greedy(
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
    # greedy-specific (module-const defaults; promote to config later):
    candidate_density: float = 2.5,
    min_root_gap_mm: float | None = None,
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
    verbose: bool = True,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
    """Greedy lowest-first leaf placement across the real (noised) foliage clumps.

    Candidates are sampled on the real clump surface; each candidate point is
    the CENTER of a leaf's root oval, seated a fixed shallow depth below that
    surface and pitched so both oval ends sit equally deep (_seat_oval_tilt).
    Burial clearance is judged against the same real clumps.

    Returns ``(parts_per_mesh, stats_per_mesh)`` — the same contract as
    :func:`placement.place_leaves_on_multiple_meshes`.  Meridian-only stats
    fields (``rows``, ``row_perims``, ``n_rows``) are left empty/zero.
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
    gap = float(min_root_gap_mm) if min_root_gap_mm is not None else max(W * 0.5, 1e-3)
    col_step = max(W, 1e-3)
    expected_row_step = max(L * 0.5, 1e-3)

    # The greedy placer seats leaves flat against the surface via the
    # per-candidate equal-depth oval seat (_seat_oval_tilt).  The leaf
    # `lift_mm` rigid tip-rotation is an extra up-angle on top of that, so it is
    # disabled here: greedy leaves have no default angling.
    del lift_mm
    leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
        lift_mm        = 0.0,
    )
    t_total = time.perf_counter()

    # ── Per-mesh setup: stats, neighbour prune ────────────────────────────────
    stats_list: list[LeafPlacementStats] = []
    parts_list: list[list[trimesh.Trimesh]] = []
    z_mins: list[float] = []
    all_cands: list[_Candidate] = []

    bounds_centers = []
    bounds_radii = []
    for mi, (mesh, seed, label) in enumerate(zip(meshes, seeds_list, labels_list)):
        z_mins.append(float(mesh.vertices[:, 2].min()))
        cx = float(mesh.vertices[:, 0].mean())
        cy = float(mesh.vertices[:, 1].mean())
        z_top = float(mesh.vertices[:, 2].max())
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
            lift_mm           = 0.0,   # greedy disables the leaf lift rotation
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

    # Neighbour lists (AABB/bounding-sphere pruned; same rule as the meridian
    # cross-cluster prune) for the seam pre-test and the blade-burial cull.
    neighbours: list[list[trimesh.Trimesh]] = []       # real neighbour clumps (cheap culls)
    for mi in range(n):
        nb = []
        for oi in range(n):
            if oi == mi:
                continue
            gap_c = float(np.linalg.norm(bounds_centers[mi] - bounds_centers[oi]))
            if gap_c <= bounds_radii[mi] + bounds_radii[oi] + L:
                nb.append(meshes[oi])
        neighbours.append(nb)

    # ── Global z-ordered sweep ────────────────────────────────────────────────
    all_cands.sort(key=lambda c: (c.z, c.phi, c.mesh_id, c.idx))

    root_grid: set[tuple[int, int, int]] = set()   # claimed root cells (fill spacing)
    n_rejected_root = 0
    n_rejected_buried = 0

    for cand in all_cands:
        mi = cand.mesh_id
        base = cand.base
        normal = cand.normal
        T0 = cand.tangent          # steepest-descent (down-slope) growth; no jitter
        stats = stats_list[mi]

        # Cull for space-filling: keep roots ~min_root_gap apart (dense packing).
        if _root_occupied_near(root_grid, base, gap):
            n_rejected_root += 1
            continue

        lseed = int(_hash01_int(int(seeds_list[mi]), "greedy-leaf", cand.idx))

        # Seat the oval: candidate point = oval center; pitch the rigid oval
        # about that center until both ends sit equally deep in the REAL mesh.
        tilt = _seat_oval_tilt(
            meshes[mi], base, normal, T0, L, _GREEDY_EMBED_MM,
        )
        if tilt is None:
            n_rejected_buried += 1
            stats.skipped_cross_buried += 1
            continue

        # Frame + root oval, both in the leaf's own frame: the blade is
        # rigidly attached to the seated oval (see _leaf_frame_and_oval).
        frame = _leaf_frame_and_oval(
            base, normal, T0, L, W, _GREEDY_EMBED_MM, _PROTRUSION_MM, tilt,
        )
        if frame is None:
            stats.build_errors += 1
            continue
        surf_base, tangent_leaf, up_leaf, inner_v = frame

        # Guard: both oval ends must be inside the clump (a thin spot where the
        # embed punches through the far side would print detached).
        if not bool(meshes[mi].contains(inner_v[[-2, -1]]).all()):
            n_rejected_buried += 1
            stats.skipped_cross_buried += 1
            continue

        stats.n_attempted += 1

        try:
            surf, _geom = build_leaf_surface(
                base_pos=surf_base, tangent=tangent_leaf, up_hint=up_leaf,
                seed=lseed, **leaf_kw,
            )
        except (RuntimeError, ValueError):
            stats.build_errors += 1
            continue
        tip_idx = len(surf.vertices) - 1
        base_idx = len(surf.vertices) - 2

        # ── Graze translation: enforce the protrusion constant on the mesh ────
        # The flat frame leaves the blade floating above a convex surface (the
        # tangent plane falls away in every direction), so the fixed anchor
        # offset alone does NOT put the blade _PROTRUSION_MM off the real
        # surface.  Translate the whole blade along ±n0 so its closest vertex
        # sits exactly _PROTRUSION_MM above the real (noised) surface.  The
        # oval keeps its seated depth; the stitch walls just shorten.
        #
        # ── Printability SKEW ─────────────────────────────────────────────────
        # On down-tilted frames the blade tip can land BELOW the root oval's tip
        # in world z — the tip-end wall would then overhang downward and print
        # unsupported.  Slide the whole blade surface in its own plane
        # (perpendicular to the leaf normal, toward the base: along
        # −tangent_leaf) until the blade tip sits above the oval tip.  This
        # shears the blade↔oval stitch slightly, but keeps the tip-end walls
        # climbing upward.
        #
        # The two corrections perturb each other (the up-slope slide can push
        # vertices into rising terrain; the graze drop can lower the tip), so
        # they alternate for two passes, ENDING on the skew — the printability
        # constraint is exact, the standoff is within the second skew's small
        # residual.  A blade needing more than L/2 of either correction
        # (cumulative) is culled.
        pull_mm = 0.0        # cumulative graze translation (+out / −drop)
        skew_mm = 0.0        # cumulative in-plane printability slide
        seat_failed = None   # which stats counter to bump on cull
        for _seat_pass in range(2):
            standoff = _min_blade_standoff(
                meshes[mi], np.asarray(surf.vertices), normal,
            )
            if standoff is not None:
                dp = _PROTRUSION_MM - standoff
                pull_mm += dp
                if abs(pull_mm) > 0.5 * L:
                    seat_failed = "preburied"
                    break
                surf.vertices = surf.vertices + dp * normal

            z_need = (float(inner_v[-1][2]) + _SKEW_TIP_MARGIN_MM
                      - float(surf.vertices[tip_idx][2]))
            if z_need > 0.0:
                t_z = float(tangent_leaf[2])
                ds = z_need / -t_z if t_z < -1e-6 else float("inf")
                skew_mm += ds
                if skew_mm > 0.5 * L:
                    seat_failed = "floor"
                    break
                surf.vertices = surf.vertices - ds * tangent_leaf
        if seat_failed is not None:
            if seat_failed == "preburied":
                stats.skipped_preburied += 1
            else:
                stats.skipped_below_floor += 1
            continue

        # Cull if the blade grows INTO a clump (its own or a neighbour): keep the
        # tip and belly (lowest widest→tip vertex) out of every clump.
        curl_mask = np.linalg.norm(surf.vertices - surf.vertices[base_idx], axis=1) > (L / 2.0)
        curl_idx = np.nonzero(curl_mask)[0]
        if len(curl_idx) == 0:
            curl_idx = np.arange(len(surf.vertices))
        belly_idx = int(curl_idx[int(np.argmin(surf.vertices[curl_idx, 2]))])
        probe = surf.vertices[np.array([tip_idx, belly_idx])]
        if _points_inside_any([meshes[mi], *neighbours[mi]], probe, base, L):
            n_rejected_buried += 1
            stats.skipped_cross_buried += 1
            continue

        try:
            solid, _ = solidify_leaf(surf, inner_v)
        except (RuntimeError, ValueError):
            stats.build_errors += 1
            continue

        # ── COMMIT ────────────────────────────────────────────────────────────
        root_grid.add(_root_cell(base, gap))
        row_idx = int((float(base[2]) - z_mins[mi]) / expected_row_step)
        stats.base_positions.append(base.copy())
        stats.base_tangents.append(tangent_leaf.copy())
        stats.base_row_idx.append(row_idx)
        stats.root_depths.append(_GREEDY_EMBED_MM)
        stats.leaf_float_dists.append(0.0)
        stats.leaf_buried_depths.append(0.0)
        stats.shingle_layers.append(0)
        stats.tip_z_clearances.append(
            float(surf.vertices[tip_idx][2]) - float(inner_v[-1][2]),
        )
        stats.tip_z_lifts.append(skew_mm)   # in-plane printability slide (mm)
        stats.pull_aways.append(pull_mm)   # graze translation along n0 (+out/−drop)
        stats.n_placed += 1

        if len(solid.vertices) > 0:
            if row_color_fn is not None:
                color = np.asarray(row_color_fn(row_idx), dtype=np.uint8)
                solid.visual = trimesh.visual.ColorVisuals(
                    mesh=solid,
                    face_colors=np.tile(color, (len(solid.faces), 1)),
                )
            parts_list[mi].append(solid)

    elapsed = time.perf_counter() - t_total
    if verbose:
        placed = sum(s.n_placed for s in stats_list)
        print(
            f"\n── greedy leaf placement ──  {placed} placed  "
            f"({len(all_cands)} candidates: root-rej={n_rejected_root} "
            f"buried-rej={n_rejected_buried})  {elapsed:.3f}s\n"
        )

    return parts_list, stats_list
