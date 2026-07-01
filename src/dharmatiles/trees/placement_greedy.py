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
    _LEAF_N_LONG,
    _leaf_width_profile,
    build_leaf_oval_offsets,
    build_leaf_surface,
    solidify_leaf,
)
from .mesh import _LEAF_PLACEABLE_NORMAL_Z, _hash01_int
from .placement import (
    _FLOOR_TOL_MM,
    _OVAL_PROTRUSION_TOL_MM,
    _PULL_CLEARANCE_MM,
    _SHINGLE_DELTA_MM,
    _SHINGLE_MAX_LAYERS,
    _SHINGLE_WORLD_CELL_MM,
    LeafPlacementStats,
    _contact_angle_analytic,
    _leaf_belly_dip,
)

# ── Greedy-specific constants ─────────────────────────────────────────────────
# Width-profile peak fraction: leaf half-width peaks at s ≈ 1/3 (leaf.py:198).
# Overlap for constraints 2/3 is judged over the widest-point → tip span only.
_F_W: float = 1.0 / 3.0

# Root oval embed depth.  The placer works directly on the real (noised) clump,
# so the root seats a fixed shallow amount just below the actual foliage surface
# at the candidate point — the same value the meridian placer uses.  (No deep
# smooth-envelope embed: that produced over-long necks and forced large tip-z
# lifts.)  It is trivially in real material, so no separate connection gate.
_GREEDY_EMBED_MM: float = 0.75

# Small outward standoff used for the seam / no-valid-standoff test.
_SEAM_EPS_MM: float = _PULL_CLEARANCE_MM


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


def _tip_proxy_cells(
    base: np.ndarray,
    t: np.ndarray,
    lat: np.ndarray,
    L: float,
    width_mm: float,
    cell: float,
    n_along: int = 4,
    n_across: int = 3,
) -> list[tuple[int, int, int]]:
    """World-voxel cells covered by the leaf's widest-point → tip blade span.

    Sampled on the smooth surface (``delta=0``, layer-independent) so two
    overlapping tips hash to shared voxels regardless of their standoff layer.
    The base → widest span is deliberately excluded: it tucks under a
    neighbour's blade (imbrication), which is allowed.
    """
    cells: set[tuple[int, int, int]] = set()
    base = np.asarray(base, float)
    for i in range(n_along):
        f = _F_W + (1.0 - _F_W) * i / (n_along - 1)     # widest → tip
        c = base + (f * L) * t
        hw = 0.5 * width_mm * float(_leaf_width_profile(np.array(f)))
        for j in range(n_across):
            a = (-1.0 + 2.0 * j / (n_across - 1)) * hw   # −hw … +hw across
            p = c + a * lat
            cells.add((
                int(math.floor(float(p[0]) / cell)),
                int(math.floor(float(p[1]) / cell)),
                int(math.floor(float(p[2]) / cell)),
            ))
    return list(cells)


def _lowest_free_layer(occ: dict, cells: list[tuple[int, int, int]]) -> int | None:
    """Lowest shingle layer free across every covered cell, or None if saturated.

    ORs the cells' layer bitmasks and returns the lowest clear bit.  Returns
    None when every layer 0…_SHINGLE_MAX_LAYERS-1 is occupied somewhere in the
    footprint — the "tip region saturated to the cap, drop the candidate" case
    that thins the over-generated candidate set.
    """
    mask = 0
    for c in cells:
        mask |= occ.get(c, 0)
    layer = 0
    while layer < _SHINGLE_MAX_LAYERS and (mask >> layer) & 1:
        layer += 1
    if layer >= _SHINGLE_MAX_LAYERS:
        return None
    return layer


def _shingle_write(occ: dict, cells: list[tuple[int, int, int]], layer: int) -> None:
    bit = 1 << layer
    for c in cells:
        occ[c] = occ.get(c, 0) | bit


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


# ── Placed-leaf world-voxel occupancy (leaf-vs-leaf clearance) ─────────────────
# A shared set of occupied world cells recording where every placed leaf's blade
# surface sits.  A new leaf's widest→tip vertices are lifted along the surface
# normal until none of them fall in an occupied cell — so no tip ends up buried
# in a leaf below it.  Cell edge ≈ 2.5× the leaf thickness so a thin blade fully
# occupies its cells (no gaps a tip could slip through).  Self-managed hash: no
# trimesh.proximity, no per-leaf BVH, O(verts) per query.
_LEAF_OCC_CELL_MM: float = 0.6
# Max outward standoff (ray-cast clump exit + leaf-clearance lift) before a leaf
# is deemed unplaceable along its normal and culled.
_CLEAR_MAX_MM: float = 4.5


def _occ_mark(leaf_occ: set, pts: np.ndarray, cell: float) -> None:
    q = np.floor(np.asarray(pts, float) / cell).astype(np.int64)
    for c in q:
        leaf_occ.add((int(c[0]), int(c[1]), int(c[2])))


def _occ_hit(leaf_occ: set, pts: np.ndarray, cell: float) -> bool:
    if not leaf_occ or len(pts) == 0:
        return False
    q = np.floor(np.asarray(pts, float) / cell).astype(np.int64)
    for c in q:
        if (int(c[0]), int(c[1]), int(c[2])) in leaf_occ:
            return True
    return False


def _ray_exit_distance(mesh, p: np.ndarray, direction: np.ndarray) -> float | None:
    """Distance from ``p`` along ``+direction`` to the FARTHEST exit of ``mesh``.

    ``p`` is assumed inside ``mesh``.  A single ray cast gives the crossings
    directly — no iterative lift search.  The farthest crossing is the point at
    which ``p`` fully leaves the (possibly re-entrant) solid; lifting past it
    clears the point.  Returns None if the ray finds no exit at all (unclearable
    along this direction).  The caller culls when the distance exceeds its cap.
    """
    locs = mesh.ray.intersects_location(p[np.newaxis], direction[np.newaxis])[0]
    if len(locs) == 0:
        return None
    return float(np.linalg.norm(locs - p[np.newaxis], axis=1).max())


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

    # The greedy placer seats leaves flat against the surface (belly on the
    # surface) and lifts them only as much as clearing obstacles requires.  The
    # leaf `lift_mm` rigid tip-rotation is an unconditional up-angle on top of
    # that, so it is disabled here: greedy leaves have no default angling.
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
    belly_dip = _leaf_belly_dip(**leaf_kw)   # (dL, dN) — shape-only binding point

    t_total = time.perf_counter()

    # ── Per-mesh setup: proximity, stats, neighbour prune ─────────────────────
    proximities: list[trimesh.proximity.ProximityQuery] = []
    stats_list: list[LeafPlacementStats] = []
    parts_list: list[list[trimesh.Trimesh]] = []
    z_mins: list[float] = []
    all_cands: list[_Candidate] = []

    bounds_centers = []
    bounds_radii = []
    for mi, (mesh, seed, label) in enumerate(zip(meshes, seeds_list, labels_list)):
        proximities.append(trimesh.proximity.ProximityQuery(mesh))
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

    root_grid: set[tuple[int, int, int]] = set()   # claimed root cells (shared)
    occ: dict = {}                                  # shingle bitmask (shared)
    leaf_occ: set[tuple[int, int, int]] = set()     # placed-leaf blade cells (shared)
    n_rejected_root = 0
    n_rejected_seam = 0
    n_rejected_sat = 0
    n_rejected_buried = 0

    for cand in all_cands:
        mi = cand.mesh_id
        base = cand.base
        normal = cand.normal
        T0 = cand.tangent
        stats = stats_list[mi]

        # ── Cheap-reject ladder (all O(1)–O(k), no geometry) ─────────────────
        # C3: base on bare mesh, not another leaf's root.
        if _root_occupied_near(root_grid, base, gap):
            n_rejected_root += 1
            continue

        # Seam / no-valid-standoff (native cross-cluster): if pulling out along
        # the normal immediately re-enters a neighbour, there is no standoff that
        # clears every mesh — pack up to the crease and drop this candidate.
        if neighbours[mi] and _points_inside_any(
            neighbours[mi], (base + _SEAM_EPS_MM * normal)[np.newaxis], base, L,
        ):
            n_rejected_seam += 1
            continue

        # Angle jitter FIRST: pivot the growth direction T0 azimuthally about the
        # normal, then solve the lean for the jittered direction (matches the
        # meridian order — jitter T0, not the leaned tangent).
        lseed = int(_hash01_int(int(seeds_list[mi]), "greedy-leaf", cand.idx))
        if angle_jitter_deg != 0.0:
            theta = math.radians(angle_jitter_deg) * (
                _hash01(int(seeds_list[mi]), "greedy-ang", cand.idx) * 2.0 - 1.0
            )
            ct, st = math.cos(theta), math.sin(theta)
            T0 = _safe_norm(T0 * ct + np.cross(normal, T0) * st)

        # Analytic lean, pressing the belly-dip against the LOCAL surface.  The
        # plane normal m is measured at the belly-dip point (one on_surface
        # query), matching the meridian placer's _ANALYTIC_MEASURE_BELLY=True.
        # Using the base normal instead makes leaves stand off ("lift") on convex
        # clumps because it ignores the surface curving away under the belly.
        _bp = base + belly_dip[0] * T0 + belly_dip[1] * normal
        _mc, _, _mt = proximities[mi].on_surface(_bp[np.newaxis])
        _m = meshes[mi].face_normals[int(_mt[0])]
        ca = _contact_angle_analytic(belly_dip[0], belly_dip[1], T0, normal, m=_m)
        tangent = _safe_norm(T0 * math.cos(ca) - normal * math.sin(ca))
        up_placed = _safe_norm(normal * math.cos(ca) + T0 * math.sin(ca))

        # C2: tip-half footprint vs the shared shingle occupancy → lowest free
        # standoff layer, or drop if the tip region is saturated to the cap.
        lat = np.cross(normal, tangent)
        lat_len = float(np.linalg.norm(lat))
        if lat_len > 1e-6:
            lat = lat / lat_len
        cells = _tip_proxy_cells(base, tangent, lat, L, W, _SHINGLE_WORLD_CELL_MM)
        layer = _lowest_free_layer(occ, cells)
        if layer is None:
            n_rejected_sat += 1
            continue

        # ── ACCEPT: build once, on the smooth surface ────────────────────────
        # pos jitter: tiny in-tangent-plane nudge, re-snapped to the surface
        # (only paid per accepted candidate).
        pt3d = base
        if pos_jitter != 0.0:
            jmm = pos_jitter * L
            r_t = _hash01(int(seeds_list[mi]), "greedy-pjt", cand.idx) * 2.0 - 1.0
            r_l = _hash01(int(seeds_list[mi]), "greedy-pjl", cand.idx) * 2.0 - 1.0
            pt3d = base + tangent * (jmm * r_t) + lat * (jmm * r_l)
            _snp, _, _ = proximities[mi].on_surface(pt3d[np.newaxis])
            pt3d = _snp[0].copy()

        base_blade = pt3d + (float(layer) * _SHINGLE_DELTA_MM) * normal

        # Cheap-reject (still no geometry built): a leaned tip below the mesh
        # floor would poke out the bottom of the clump.  Not counted as a build
        # attempt — it is analogous to the meridian grid never emitting a slot
        # there, so it must not depress the placed/attempted coverage metric.
        tip_z = base_blade[2] + L * tangent[2]
        if tip_z < z_mins[mi] - _FLOOR_TOL_MM:
            stats.skipped_below_floor += 1
            continue

        stats.n_attempted += 1

        # ── Oval gates FIRST (cheap: build_leaf_oval_offsets is pure NumPy) ────
        # Reject bad poses before paying for build_leaf_surface / solidify_leaf,
        # so the expensive surface build happens only for leaves that will ship.
        lat_ov = np.cross(normal, tangent)
        lat_ov_len = float(np.linalg.norm(lat_ov))
        if lat_ov_len > 1e-6:
            lat_ov = lat_ov / lat_ov_len
        oval_off = build_leaf_oval_offsets(
            n_hat=normal, T_along=tangent, across=lat_ov,
            L=L, W=W, embed_mm=_GREEDY_EMBED_MM,
        )
        inner_v = oval_off + pt3d[np.newaxis]
        # C1: the root oval embeds a fixed shallow depth just below the real
        # foliage surface at the candidate point.  Small protrusions where the
        # flat oval overshoots a convex face are tolerated up to the embed depth;
        # a deeper protrusion means the root would poke out of the foliage —
        # reject.  (The root is in real material by construction, so there is no
        # separate connection gate.)
        outside = ~meshes[mi].contains(inner_v)
        if outside.any():
            _, ov_d, _ = trimesh.proximity.closest_point(meshes[mi], inner_v[outside])
            if float(ov_d.max()) > _OVAL_PROTRUSION_TOL_MM:
                stats.build_errors += 1
                continue

        # ── ACCEPTED: build the surface + solidify (once, for shipped leaves) ──
        try:
            surf, _geom = build_leaf_surface(
                base_pos = base_blade,
                tangent  = tangent,
                up_hint  = up_placed,
                seed     = lseed,
                **leaf_kw,
            )
            tip_idx = len(surf.vertices) - 1
            solid, _ = solidify_leaf(surf, inner_v)
        except (RuntimeError, ValueError):
            stats.build_errors += 1
            continue

        # ── Clear obstacles by lifting the blade along its normal ──────────────
        # We only require two probe points — the blade TIP and BELLY (the lowest
        # widest→tip vertex) — to clear every obstruction: the leaf's own clump,
        # neighbour clumps, and previously-placed leaves.  The lift height is
        # computed directly, not searched: for a probe inside a clump, a single
        # ray cast along +normal gives the exit distance; for placed leaves, a
        # short march counts cells along +normal.  The whole leaf is then lifted
        # once by the max required distance + a tolerance.
        base_idx = len(surf.vertices) - 2       # base_pt vertex (surface layout)
        curl_mask = np.linalg.norm(surf.vertices - surf.vertices[base_idx], axis=1) > (L / 2.0)
        curl_idx = np.nonzero(curl_mask)[0]
        if len(curl_idx) == 0:
            curl_idx = np.arange(len(surf.vertices))
        belly_idx = int(curl_idx[int(np.argmin(surf.vertices[curl_idx, 2]))])
        probe_pts = surf.vertices[np.array([tip_idx, belly_idx])]   # (2, 3)
        nbrs = neighbours[mi]
        own_mesh = meshes[mi]

        lift_needed = 0.0
        cull = False
        for cm in (own_mesh, *nbrs):
            cb = cm.bounds
            for p in probe_pts:
                if (p < cb[0]).any() or (p > cb[1]).any():
                    continue
                if not bool(cm.contains(p[np.newaxis])[0]):
                    continue
                d = _ray_exit_distance(cm, p, normal)
                if d is None:               # can't exit along the normal → not leaf-safe
                    cull = True
                    break
                lift_needed = max(lift_needed, d)
            if cull:
                break
        if not cull:
            march = _LEAF_OCC_CELL_MM * 0.5
            for p in probe_pts:
                d = 0.0
                while d <= _CLEAR_MAX_MM and _occ_hit(
                    leaf_occ, (p + d * normal)[np.newaxis], _LEAF_OCC_CELL_MM
                ):
                    d += march
                # Overshoot one cell so the lifted vertex lands clearly inside the
                # free cell rather than on the occupied cell's boundary.
                if d > 0.0:
                    d += _LEAF_OCC_CELL_MM
                lift_needed = max(lift_needed, d)
        if cull or lift_needed + _PULL_CLEARANCE_MM > _CLEAR_MAX_MM:
            n_rejected_buried += 1
            stats.skipped_cross_buried += 1
            continue

        pull_away = (lift_needed + _PULL_CLEARANCE_MM) if lift_needed > 1e-9 else 0.0
        if pull_away > 0.0:
            lift_vec = pull_away * normal
            surf.vertices += lift_vec
            solid.vertices[:len(surf.vertices)] += lift_vec

        # Tip-z ordering (visible tip above the root tip so the leaf reads as
        # growing outward, not drooping) is obtained for free from the horizontal
        # growth direction — no post-hoc +Z lift, which used to shove a cleared
        # blade back into the clump.
        burial_d = 0.0
        tip_z_lift = 0.0
        oval_tip_i = len(surf.vertices) + tip_idx
        tip_z_clearance = float(solid.vertices[tip_idx, 2] - solid.vertices[oval_tip_i, 2])
        float_d = pull_away           # standoff from the seated pose (stat only)

        # ── COMMIT ────────────────────────────────────────────────────────────
        _shingle_write(occ, cells, layer)
        root_grid.add(_root_cell(base, gap))
        # Record this leaf's blade footprint so later (higher) leaves lift clear
        # of it instead of burying a tip in it.
        _occ_mark(leaf_occ, surf.vertices, _LEAF_OCC_CELL_MM)

        row_idx = int((float(pt3d[2]) - z_mins[mi]) / expected_row_step)
        stats.base_positions.append(pt3d.copy())
        stats.base_tangents.append(tangent.copy())
        stats.base_row_idx.append(row_idx)
        stats.root_depths.append(_GREEDY_EMBED_MM)
        stats.leaf_float_dists.append(float_d)
        stats.leaf_buried_depths.append(burial_d)
        stats.shingle_layers.append(layer)
        stats.tip_z_clearances.append(tip_z_clearance)
        stats.tip_z_lifts.append(tip_z_lift)
        stats.pull_aways.append(pull_away)
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
            f"seam-rej={n_rejected_seam} "
            f"sat-rej={n_rejected_sat} buried-rej={n_rejected_buried})  {elapsed:.3f}s\n"
        )

    return parts_list, stats_list
