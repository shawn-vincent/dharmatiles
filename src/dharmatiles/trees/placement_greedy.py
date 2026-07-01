"""Greedy lowest-first leaf placement (experimental, side-by-side).

An alternative to the meridian-arc placer in :mod:`placement`.  Instead of a
deterministic rows×columns grid per cluster, this grows the canopy by greedy
accretion over a **pre-generated** candidate set:

    Pre-generate candidates (denser than can fit) on each cluster's SMOOTH
    (pre-noise) envelope, each with an analytic pose.  Sort by printable
    world-z ascending.  Sweep upward once; accept a candidate iff it is
    compatible with the already-accepted set (cheap-reject ladder on
    self-managed hash grids).  Build geometry ONLY for accepted candidates.

Why the smooth envelope (see the experiment doc, "Candidate Generation")
--------------------------------------------------------------------------
Cluster noise only displaces INWARD (``mesh.py`` peak-shifts the noise so max
displacement is exactly 0).  The smooth envelope is therefore the strict outer
surface; the real noisy surface is always at-or-below it.  Two consequences:

* **Clearance is free** — a blade held at/above the smooth surface can never be
  reached by the real (receded) surface, so "surface not embedded in any mesh"
  reduces to "clear of the smooth envelopes", already guaranteed by building on
  smooth + the outward shingle nudge.  No real-mesh intersection test needed.
* **Root embed must out-reach the noise** — the root oval is the one thing that
  deliberately dips inward; to guarantee it always reaches real material even in
  the deepest noise pit we embed by ``_GREEDY_EMBED_MM > _FOLIAGE_MAX_NOISE_MM``.

Hard constraints carried over from the 2026-07-01 perf crisis (do NOT violate):
no ``trimesh.proximity.closest_point`` / R-tree / per-leaf ``Trimesh`` scans in
the per-candidate sweep; no ``fix_normals`` on placed leaves; cheap-reject before
every build.  Per-candidate rejection costs only spatial-hash lookups; geometry
(build + one ``on_surface`` pull-away against the OWN smooth mesh, exactly as the
meridian path already pays) is built once per *accepted* leaf.

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
from .mesh import (
    _FOLIAGE_COARSE_NOISE_AMPLITUDE_MM,
    _FOLIAGE_NOISE_AMPLITUDE_MM,
    _LEAF_PLACEABLE_NORMAL_Z,
    _hash01_int,
)
from .placement import (
    _FLOOR_TOL_MM,
    _OVAL_PROTRUSION_TOL_MM,
    _PULL_CLEARANCE_MM,
    _PULL_MAX_MM,
    _SHINGLE_DELTA_MM,
    _SHINGLE_MAX_LAYERS,
    _SHINGLE_WORLD_CELL_MM,
    _TIP_Z_CLEARANCE_MM,
    LeafPlacementStats,
    _contact_angle_analytic,
    _leaf_belly_dip,
)

# ── Greedy-specific constants ─────────────────────────────────────────────────
# Width-profile peak fraction: leaf half-width peaks at s ≈ 1/3 (leaf.py:198).
# Overlap for constraints 2/3 is judged over the widest-point → tip span only.
_F_W: float = 1.0 / 3.0

# Maximum inward erosion of the noised foliage surface relative to the smooth
# envelope.  The noise is peak-shifted (mesh.py) so the surface erodes inward by
# the full trough range — the peak-to-trough span of (coarse + fine)·scale, NOT
# a single-sided amplitude.  A conservative upper bound: the full coarse range
# (2·amplitude) plus a 6σ fine-Gaussian span.  Measured max on the test clusters
# is ~2.3 mm; this bound (~2.6 mm) safely exceeds it.
_MAX_INWARD_EROSION_MM: float = (
    2.0 * _FOLIAGE_COARSE_NOISE_AMPLITUDE_MM + 6.0 * _FOLIAGE_NOISE_AMPLITUDE_MM
)

# Root oval embed depth for the greedy path.  MUST exceed the cluster's maximum
# inward noise erosion so a root embedded from the smooth envelope still reaches
# real foliage material in the deepest noise pit (otherwise the leaf floats over
# a valley and prints detached).  Derived from the noise config in mesh.py.
_GREEDY_EMBED_MM: float = _MAX_INWARD_EROSION_MM + 0.5
assert _GREEDY_EMBED_MM > _MAX_INWARD_EROSION_MM, (
    "greedy root embed must out-reach the cluster's max inward noise erosion"
)

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


def _point_inside_any(meshes: list, pt: np.ndarray, near_pt: np.ndarray, reach: float) -> bool:
    """True if ``pt`` is inside any neighbour solid (AABB-pruned by ``near_pt``).

    Uses embree ``mesh.contains`` on a single point (~0.002 ms).  Neighbours the
    candidate cannot reach (bbox expanded by ``reach``) are skipped.
    """
    p = pt[np.newaxis]
    for _m in meshes:
        _b = _m.bounds
        if (near_pt < _b[0] - reach).any() or (near_pt > _b[1] + reach).any():
            continue
        if bool(_m.contains(p)[0]):
            return True
    return False


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
    """Steepest-descent growth direction in the local tangent plane.

    Projects world-down onto the tangent plane (same rule the meridian placer
    uses).  Near-horizontal surfaces (apex) fall back to radially outward from
    the cluster centroid.
    """
    down = np.array([0.0, 0.0, -1.0])
    d = down - float(np.dot(down, normal)) * normal
    dl = float(np.linalg.norm(d))
    if dl > 1e-6:
        return d / dl
    radial = base - centroid
    radial[2] = 0.0
    radial = radial - float(np.dot(radial, normal)) * normal
    rl = float(np.linalg.norm(radial))
    if rl > 1e-6:
        return radial / rl
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
    normals = mesh.face_normals[fi]
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
    meshes: list[trimesh.Trimesh],          # SMOOTH (pre-noise) envelopes
    *,
    length_mm: float,
    width_mm: float,
    thickness_mm: float,
    fold_angle_deg: float,
    inner_curve: float,
    outer_curve: float,
    curl_deg: float,
    lift_mm: float,
    real_meshes: list[trimesh.Trimesh] | None = None,
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
    """Greedy lowest-first leaf placement across multiple SMOOTH cluster envelopes.

    ``meshes`` are the SMOOTH (pre-noise) envelopes: all generation, clearance,
    and pull-away run against them (the clearance-free guarantee).  ``real_meshes``
    (optional, index-aligned) are the corresponding NOISED clumps that will be
    printed; when supplied, each accepted leaf's embedded root oval is required to
    reach into its real clump (exact connection gate) so no leaf prints detached.
    When omitted, a conservative smooth-envelope + noise-bound heuristic is used
    instead.

    Returns ``(parts_per_mesh, stats_per_mesh)`` — the same contract as
    :func:`placement.place_leaves_on_multiple_meshes`.  Meridian-only stats
    fields (``rows``, ``row_perims``, ``n_rows``) are left empty/zero.
    """
    n = len(meshes)
    if real_meshes is not None and len(real_meshes) != n:
        raise ValueError("real_meshes must be index-aligned with meshes")
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

    leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
        lift_mm        = float(lift_mm),
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
            lift_mm           = float(lift_mm),
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
    # cross-cluster prune) for the seam / no-valid-standoff test.
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

    # ── Global z-ordered sweep ────────────────────────────────────────────────
    all_cands.sort(key=lambda c: (c.z, c.phi, c.mesh_id, c.idx))

    root_grid: set[tuple[int, int, int]] = set()   # claimed root cells (shared)
    occ: dict = {}                                  # shingle bitmask (shared)
    n_rejected_root = 0
    n_rejected_seam = 0
    n_rejected_sat = 0
    n_rejected_thin = 0

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
        if neighbours[mi] and _point_inside_any(
            neighbours[mi], base + _SEAM_EPS_MM * normal, base, L,
        ):
            n_rejected_seam += 1
            continue

        # Connection guarantee (constraint 1, made provable for the smooth
        # envelope): the root embeds _GREEDY_EMBED_MM inward along the normal.
        # Require that embedded point to lie inside the smooth envelope, which
        # (a) rejects thin regions where a deep embed would punch out the far
        # side, and (b) since the point is deeper than the max inward noise
        # amplitude, guarantees it is inside the REAL noised clump too — so the
        # root always plugs into real material and never prints detached.
        if not bool(meshes[mi].contains((base - _GREEDY_EMBED_MM * normal)[np.newaxis])[0]):
            n_rejected_thin += 1
            continue

        # Analytic lean (closed form, base-normal plane model; pull-away absorbs
        # residual curvature penetration — no proximity query here).
        ca = _contact_angle_analytic(belly_dip[0], belly_dip[1], T0, normal)
        tangent = _safe_norm(T0 * math.cos(ca) - normal * math.sin(ca))
        up_placed = _safe_norm(normal * math.cos(ca) + T0 * math.sin(ca))

        # Angle jitter: pivot the growth direction azimuthally about the normal.
        lseed = int(_hash01_int(int(seeds_list[mi]), "greedy-leaf", cand.idx))
        if angle_jitter_deg != 0.0:
            theta = math.radians(angle_jitter_deg) * (
                _hash01(int(seeds_list[mi]), "greedy-ang", cand.idx) * 2.0 - 1.0
            )
            ct, st = math.cos(theta), math.sin(theta)
            tangent = _safe_norm(tangent * ct + np.cross(normal, tangent) * st)

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
        # C1: root oval fully embeds into the smooth envelope (small protrusions
        # where the flat oval overshoots a convex face are tolerated up to the
        # embed depth; deeper protrusion == rejected).
        outside = ~meshes[mi].contains(inner_v)
        if outside.any():
            _, ov_d, _ = trimesh.proximity.closest_point(meshes[mi], inner_v[outside])
            if float(ov_d.max()) > _OVAL_PROTRUSION_TOL_MM:
                stats.build_errors += 1
                continue
        # Connection gate: the embedded root oval must reach real material, else
        # the leaf prints detached.  Exact when the noised clump is supplied
        # (test the oval directly against it); otherwise fall back to a
        # smooth-envelope + noise-bound proxy (an oval vertex pushed outward by
        # the max inward erosion that is still inside smooth is provably >= that
        # depth below the surface, hence inside the noised clump — sound only
        # where the local normal ≈ the base normal).
        if real_meshes is not None:
            _connected = bool(real_meshes[mi].contains(inner_v).any())
        else:
            _connected = bool(
                meshes[mi].contains(inner_v + _MAX_INWARD_EROSION_MM * normal).any()
            )
        if not _connected:
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

        # ── Pull-away against the OWN smooth mesh (one on_surface, per placed) ─
        cl, _, ti = proximities[mi].on_surface(surf.vertices)
        nrm = meshes[mi].face_normals[np.asarray(ti, dtype=np.int64)]
        signed = -np.einsum("ij,ij->i", surf.vertices - cl, nrm)   # inside positive
        curl_mask = np.linalg.norm(surf.vertices - pt3d, axis=1) > (L / 2.0)
        curl_signed = signed[curl_mask] if curl_mask.any() else np.empty(0)
        burial_d = float(curl_signed.max()) if len(curl_signed) else 0.0

        pull_away = 0.0
        if burial_d > _PULL_MAX_MM:
            stats.skipped_preburied += 1
            continue
        if burial_d > 0.0:
            pull_away = burial_d + _PULL_CLEARANCE_MM
            pull_vec = pull_away * normal
            surf.vertices += pull_vec
            solid.vertices[:len(surf.vertices)] += pull_vec
            signed -= pull_away
            burial_d = 0.0

        curl_signed = signed[curl_mask] if curl_mask.any() else np.empty(0)
        float_d = (
            float((-curl_signed[curl_signed < 0.0]).max())
            if np.any(curl_signed < 0.0) else 0.0
        )

        # ── Tip-z ordering: visible blade tip above the embedded root-oval tip ─
        tip_z_lift = 0.0
        tip_z_clearance = float(
            solid.vertices[tip_idx, 2] - solid.vertices[len(surf.vertices) + tip_idx, 2]
        )
        if tip_z_clearance < _TIP_Z_CLEARANCE_MM:
            tip_z_lift = _TIP_Z_CLEARANCE_MM - tip_z_clearance
            lift_vec = np.array([0.0, 0.0, tip_z_lift])
            surf.vertices += lift_vec
            solid.vertices[:len(surf.vertices)] += lift_vec
            tip_z_clearance = float(
                solid.vertices[tip_idx, 2] - solid.vertices[len(surf.vertices) + tip_idx, 2]
            )
        if tip_z_clearance <= 0.0:
            raise AssertionError(
                "greedy leaf surface tip z must be above root oval tip z "
                f"(clearance={tip_z_clearance:.6f}, mesh={mi}, cand={cand.idx})"
            )

        # ── COMMIT ────────────────────────────────────────────────────────────
        _shingle_write(occ, cells, layer)
        root_grid.add(_root_cell(base, gap))

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
            f"seam-rej={n_rejected_seam} thin-rej={n_rejected_thin} "
            f"sat-rej={n_rejected_sat})  {elapsed:.3f}s\n"
        )

    return parts_list, stats_list
