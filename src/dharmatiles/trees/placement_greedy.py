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
* **Leaves grow horizontally outward** (see :func:`_growth_tangent`) so the tip
  sits above the root without any post-hoc vertical lift.
* **Clearance is computed, not searched.**  A leaf's tip and belly must clear
  its own clump, neighbour clumps and previously-placed leaves; the required
  outward lift is a single ray cast per clump (exit distance along the normal)
  plus a short cell march for placed leaves — the whole leaf is lifted once by
  the max.  A leaf that can't clear within :data:`_CLEAR_MAX_MM` is culled.

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

from ._utils import _hash01, _safe_norm
from .leaf import (
    _LEAF_N_LAT,
    _LEAF_N_LONG,
    _leaf_width_profile,
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

# Fixed outward protrusion of the leaf blade base above the foliage surface.
_PROTRUSION_MM: float = 0.3


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


# ── Leaf frame + root oval from two embedded mesh points ──────────────────────

def _leaf_frame_and_oval(
    P0: np.ndarray, n0: np.ndarray, t_horiz: np.ndarray,
    L: float, W: float, embed_mm: float, protrusion_mm: float,
    mesh: trimesh.Trimesh,
):
    """Build the leaf frame and its flat root oval from two mesh footprint points.

    The whole leaf is defined by two surface points — the base-end ``P0`` (the
    candidate) and the tip-end (the surface point under ``P0 + L·t_horiz``).  Each
    is dropped ``embed_mm`` inside along its LOCAL surface normal to give the
    oval's two axial ends (``ob``, ``ot``).  The oval axis runs between them (so it
    follows the surface), and the oval/leaf normal ``N`` is that average surface
    normal made perpendicular to the axis.  Because both ends are embedded by the
    same amount, they are equidistant from the surface — and the leaf surface,
    built one ``embed+protrusion`` step out along the same ``N``, inherits that
    equidistance for free.

    Returns ``(surf_base, axis, N, lat, inner_v)`` where ``inner_v`` is the
    123-vertex oval (same layout as the leaf surface, for :func:`solidify_leaf`),
    or ``None`` if the frame is degenerate.
    """
    P1, _, tri = trimesh.proximity.closest_point(mesh, (P0 + L * t_horiz)[np.newaxis])
    P1 = P1[0]
    n1 = mesh.face_normals[int(tri[0])]

    ob = P0 - embed_mm * n0        # oval base end, embed_mm below the surface
    ot = P1 - embed_mm * n1        # oval tip  end, embed_mm below the surface
    av = ot - ob
    D = float(np.linalg.norm(av))
    if D < 1e-6:
        return None
    axis = av / D
    navg = _safe_norm(n0 + n1)
    N = _safe_norm(navg - float(np.dot(navg, axis)) * axis)   # oval normal ⟂ axis
    lat = np.cross(N, axis)
    ll = float(np.linalg.norm(lat))
    if ll < 1e-6:
        return None
    lat = lat / ll

    # Flat oval spanning ob→ot with the leaf's own width profile, so it underlies
    # the blade outline.  Same layout as the leaf surface for the 1:1 skin.
    s_int = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]     # (ring_count,)
    lat_pos = np.linspace(-1.0, 1.0, _LEAF_N_LAT + 1)          # (lat_count+1,)
    w_s = 0.5 * W * _leaf_width_profile(s_int)                 # (ring_count,)
    centers = ob[np.newaxis] + s_int[:, np.newaxis] * av[np.newaxis]
    grid = (
        centers[:, np.newaxis, :]
        + (lat_pos[np.newaxis, :, np.newaxis] * w_s[:, np.newaxis, np.newaxis])
        * lat[np.newaxis, np.newaxis, :]
    )
    inner_v = np.concatenate([grid.reshape(-1, 3), ob[np.newaxis], ot[np.newaxis]], axis=0)

    # Leaf surface base: one embed+protrusion step out along N (⇒ protrusion above
    # the surface), so the blade is built on the same normal as the oval.
    surf_base = ob + (embed_mm + protrusion_mm) * N
    return surf_base, axis, N, lat, inner_v


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
    """Horizontal-outward growth direction in the local tangent plane.

    The leaf grows horizontally away from the cluster axis (radial in XY),
    projected onto the surface tangent plane — so it comes out level "along its
    bottom" rather than drooping downward.  On the upper canopy this tips the
    blade slightly upward (tip above the root), giving the tip-z ordering for
    free with no post-hoc lift.  Falls back to steepest ascent near a vertical
    surface where the horizontal projection degenerates.
    """
    radial = np.array([base[0] - centroid[0], base[1] - centroid[1], 0.0])
    t = radial - float(np.dot(radial, normal)) * normal
    tl = float(np.linalg.norm(t))
    if tl > 1e-6:
        return t / tl
    # Degenerate (base directly above/below the axis): grow up-slope.
    up = np.array([0.0, 0.0, 1.0])
    u = up - float(np.dot(up, normal)) * normal
    ul = float(np.linalg.norm(u))
    if ul > 1e-6:
        return u / ul
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

    Candidates are sampled on the real clump surface; each leaf's root seats a
    fixed shallow depth just below that surface at the candidate point, and
    burial clearance is judged against the same real clumps.

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

    # The greedy placer seats leaves flat against the surface via the frame
    # construction (equidistant oval ends → equidistant leaf ends).  The leaf
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
    # The leaf's arch+curl raises its tip endpoint above the base plane by a fixed
    # angle (same for every leaf of these params).  Measure it once so the build
    # frame can be tilted down by it — then the built tip lands level with the
    # base, i.e. base and tip end up equidistant from the surface.
    _ref, _ = build_leaf_surface(
        base_pos=np.zeros(3), tangent=np.array([1.0, 0.0, 0.0]),
        up_hint=np.array([0.0, 0.0, 1.0]), seed=0, **leaf_kw,
    )
    _reftip = _ref.vertices[len(_ref.vertices) - 1]
    tip_rise_angle = math.atan2(float(_reftip[2]), float(_reftip[0]))

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
        T0 = cand.tangent
        stats = stats_list[mi]

        # Cull for space-filling: keep roots ~min_root_gap apart (dense packing).
        if _root_occupied_near(root_grid, base, gap):
            n_rejected_root += 1
            continue

        lseed = int(_hash01_int(int(seeds_list[mi]), "greedy-leaf", cand.idx))
        T0 = cand.tangent          # horizontal-outward growth (footprint direction)
        # Optional azimuthal jitter of the growth direction about the normal.
        if angle_jitter_deg != 0.0:
            theta = math.radians(angle_jitter_deg) * (
                _hash01(int(seeds_list[mi]), "greedy-ang", cand.idx) * 2.0 - 1.0
            )
            ct, st = math.cos(theta), math.sin(theta)
            T0 = _safe_norm(T0 * ct + np.cross(normal, T0) * st)

        # Frame + root oval from two embedded mesh points (base end + tip end).
        # Both oval ends sit embed below the surface, so they are equidistant from
        # it; the leaf normal N is perpendicular to the axis between them.
        frame = _leaf_frame_and_oval(
            base, normal, T0, L, W, _GREEDY_EMBED_MM, _PROTRUSION_MM, meshes[mi],
        )
        if frame is None:
            stats.build_errors += 1
            continue
        surf_base, axis, N_oval, lat, inner_v = frame

        # Guard: both oval ends must be inside the clump (a thin spot where the
        # embed punches through the far side would print detached).
        if not bool(meshes[mi].contains(inner_v[[-2, -1]]).all()):
            n_rejected_buried += 1
            stats.skipped_cross_buried += 1
            continue

        stats.n_attempted += 1

        # Leaf surface: built on the SAME normal N as the oval, one protrusion
        # above the surface.  The build frame is tilted DOWN by the leaf's
        # intrinsic tip-rise angle so the curled tip lands level with the base —
        # base and tip then sit equidistant above the surface (mirroring the
        # equidistant oval ends below it).
        _c, _s = math.cos(tip_rise_angle), math.sin(tip_rise_angle)
        tangent_leaf = _safe_norm(axis * _c - N_oval * _s)
        up_leaf = _safe_norm(N_oval * _c + axis * _s)
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
        stats.base_tangents.append(axis.copy())
        stats.base_row_idx.append(row_idx)
        stats.root_depths.append(_GREEDY_EMBED_MM)
        stats.leaf_float_dists.append(0.0)
        stats.leaf_buried_depths.append(0.0)
        stats.shingle_layers.append(0)
        stats.tip_z_clearances.append(0.0)
        stats.tip_z_lifts.append(0.0)
        stats.pull_aways.append(0.0)
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
