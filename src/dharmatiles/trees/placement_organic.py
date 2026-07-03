"""Organic union-surface leaf placement (greenfield, fourth placer).

Requirements source:
``docs/meta/history/2026-07-02-foliage-greenfield-requirements.md`` —
Animal Crossing-style chunky discrete leaves; TOTAL coverage with no
visible order; configurable overlap with per-leaf height layering; seams
handled by tiling the union canopy surface; down-slope direction with
coherent variation; supportless FDM.

Algorithm
---------
1. **Union placement surface.** The (noised) cluster solids are
   boolean-unioned into ONE mesh.  All placement queries — sampling,
   normals, seating, containment — run against it.  Inside corners are
   just creases of the union; "no leaf buried in a neighbour" is free
   because there are no neighbours.
2. **Coverage by maximal Poisson-disk.**  Over-generate area-weighted
   candidates on the union surface, dart-throw with an exact-distance
   grid to saturation.  Maximality ⇒ every placeable surface point lies
   within one spacing radius of an accepted root ⇒ no bare patch wider
   than 2·spacing, by construction.  A verification pass measures
   residual gaps (from per-leaf build failures) and reports them.
3. **Direction field.**  Down-slope per root, rotated by a smooth
   low-frequency positional angle field — coherent variation, not
   i.i.d. jitter, not combed.
4. **Overlap layering.**  Two prototype modes select each leaf's standoff
   layer (0..N): ``"systematic"`` — sweep roots bottom-up, each leaf
   rides one step above the tallest already-layered neighbour it
   overlaps (upper-over-lower shingling); ``"random"`` — hash-assigned.
   The layer lifts the blade along the seat normal AFTER the belly-dip
   seat (root oval stays plugged), so overlapping leaves sit at visibly
   distinct heights.
5. **Per-leaf build** uses the shared per-leaf pipeline
   (:func:`placement_leaf._attempt_leaf`): equal-depth oval seat,
   rigid blade↔oval frame, printability skew, belly-dip drop to
   ``_PROTRUSION_MM``, tip/belly cull, solidify.

Hard perf constraints (2026-07-01 crisis) remain: no per-leaf mesh scans;
embree only; cheap-reject before every build.
"""
from __future__ import annotations

import math
import time
from collections.abc import Callable

import numpy as np
import trimesh

from ._utils import _hash01, _safe_norm
from .placement_leaf import (
    _ROOT_EMBED_MM,
    LeafPlacementStats,
    _attempt_leaf_gen,
    _drive_batched,
    _growth_tangent,
    _project_to_surface_gen,
    _rotate_about,
    _sample_surface,
)

# ── Organic-specific constants (module constants while iterating) ─────────────
# Root spacing as a fraction of leaf width — THE overlap knob.
# ~0.45 = moderate AC-style shingle; ~0.8 = light touch.
_ORGANIC_SPACING_FRAC: float = 0.45

# Candidate over-generation factor: candidates ≈ this × (area / spacing²).
# Generous so the dart throw actually saturates (maximality needs excess).
_ORGANIC_CANDIDATE_FACTOR: float = 10.0

# Blade shape blends CONTINUOUSLY with the surface normal instead of a
# hard pitched/flush switch: a smoothstep zone factor st runs 1 on
# clearly-upward faces (nz ≥ _ORGANIC_ZONE_HI) to 0 on undersides
# (nz ≤ _ORGANIC_ZONE_LO).  curl, tip lift, shingle standoff and the tip
# clearance ceiling all scale with st; the end-to-end arch scales with
# (1−st).  So curl/tip-height fade gradually down the canopy, reaching
# the fully arch-embedded blade exactly where overhangs would begin.
# Below _ORGANIC_PLACEABLE_NORMAL_Z → bare (deep underside).
_ORGANIC_ZONE_HI: float = 0.30
_ORGANIC_ZONE_LO: float = -0.45
_ORGANIC_PLACEABLE_NORMAL_Z: float = -0.75

# Tip clearance ceiling blend: ceiling = touch + st × this.
_ORGANIC_TIP_CEIL_RANGE_MM: float = 2.4

# Direction field: down-slope rotated by a smooth positional angle field.
# Kept modest: divergent neighbours are the main source of blade sheets
# slicing through each other.
_ORGANIC_DIR_VAR_DEG: float = 15.0     # peak deviation from down-slope
_ORGANIC_DIR_WAVELEN_MM: float = 7.0   # spatial wavelength of the field

# Shingle pitch: every pitched blade's TIP floats this far above its base
# level (the leaf builder's rigid tip rotation).  0.6 mm (≈2× blade
# thickness) was the hug-the-mass baseline after the aesthetic-judge pass
# flagged flaring tips; now deliberately doubled to EXAGGERATE upturned
# tips at the crown.  Scales with st, so the underside is untouched and
# the smoothstep zone provides the transition down the canopy.  The
# monotone standoff below (not the lift) owns the upper-over-lower
# guarantee.
_ORGANIC_TIP_LIFT_MM: float = 1.2

# Per-leaf size jitter, downward only (max size = the configured leaf):
# scale ∈ [1−jitter, 1].  Uniform stamped-looking leaves were judge item 8.
_ORGANIC_SIZE_JITTER: float = 0.2

# Height-sorted shingle standoff: leaves are processed bottom-up; each
# leaf stands off by STEP more than the tallest already-processed leaf it
# overlaps (capped).  Guarantees a REAL separation between any two
# overlapping leaves at different heights — a global monotone ramp spread
# the budget so thin (~0.04 mm between neighbours) that blade-arch
# variation still crossed sheets.  At the cap, ties are resolved by the
# tip lift (tip always clears a base).
_ORGANIC_SHINGLE_STEP_MM: float = 0.3
_ORGANIC_SHINGLE_CAP_MM: float = 1.2
_ORGANIC_SHINGLE_NEIGHBOR_MM: float = 2.4
_ORGANIC_STANDOFF_JITTER_MM: float = 0.05

# Flush-blade shape overrides.  The flush blade is a pure end-to-end ARCH:
# no curl, no lift — a parabolic rise (arch_mm at mid-span) between ends
# that touch/tuck.  On a convex underside lobe a flat blade pinned at both
# ends chords THROUGH the bulge (~0.5 mm sag over a 4.5 mm blade at
# r≈5.5); the arch clears it, and unlike curl it can never bend back into
# the surface.
_ORGANIC_FLUSH_CURL_DEG: float = 0.0
_ORGANIC_FLUSH_LIFT_MM: float = 0.0
_ORGANIC_FLUSH_ARCH_MM: float = 0.8

# Pitched-blade curl cap: the configured 40° curl made every blade a
# curling tongue that presents its EDGE (shaggy canopy); 16° kept flat
# plates presenting their FACE.  Doubled to exaggerate upturned crown
# tips (scales with st, so only clearly-upward faces feel the full cap;
# a tile's leaf_curl_deg still caps below this).
_ORGANIC_PITCH_CURL_DEG: float = 32.0

# Neck gate: reject a blade whose accumulated in-plane slide + net normal
# standoff would stretch the blade→oval stitch walls beyond this — the
# "long-rooted leaves sticking out peculiarly" (wall chimneys/fans).
_ORGANIC_MAX_NECK_MM: float = 1.8

# Skew cap for pitched blades (fraction of leaf length).
_ORGANIC_MAX_SKEW_FRAC: float = 0.35

# Point-end conflict radius for the standoff escalation: a leaf's TIP
# within this of another leaf's base-end/centre/tip is a real overlap;
# base-to-base nestling is not.
_ORGANIC_TIP_CONFLICT_MM: float = 1.5

# ── Sheet solidification (EXPERIMENT PARKED — not wired) ──
# Each leaf is a thin SHEET: the blade surface, a bottom surface offset by
# _ORGANIC_SHEET_MM along the seat normal, and an edge band — plus a root
# TAB at the base (the bottom offset deepens toward the base and dives
# into the clump) that anchors the leaf.  This replaces the oval + wall
# stitch: the old solid dropped a wall skirt from the whole perimeter to
# a fat embedded oval, which made every leaf read as a PILLOW/pod and
# grew fan-shaped wall flares wherever the blade sat proud.  No oval ⇒
# no skew rule, no neck gate, no oval containment guards.
_ORGANIC_SHEET_MM: float = 0.65        # sheet thickness (≥3 FDM layers)
_ORGANIC_TAB_EXTRA_MM: float = 0.9     # tab penetration below the surface
_ORGANIC_TAB_SPAN: float = 0.35        # tab fades out by this s along blade

# Coverage verification sampling density (test points ≈ area / this²).
_ORGANIC_VERIFY_RES_MM: float = 0.9


# ── Normal-aware Poisson grid ─────────────────────────────────────────────────
# A plain Euclidean disk test fails at inside corners: the two walls of a
# union-seam V are only 1–2 mm apart THROUGH SPACE, so a root on one wall
# blocks candidates on the opposite wall and every crease grows a bald band
# beside it.  Storing the surface normal with each root and requiring
# normal agreement (dot > _ROOT_BLOCK_COS) makes the conflict test a cheap
# proxy for surface distance: opposite V-walls never block each other,
# while same-wall neighbours (whose normals differ by only a few degrees
# per mm of dome) always do.
_ROOT_BLOCK_COS: float = 0.26          # ≈ 75° — walls more oblique don't block


def _root_cell_of(pt: np.ndarray, gap: float) -> tuple[int, int, int]:
    return (
        int(math.floor(float(pt[0]) / gap)),
        int(math.floor(float(pt[1]) / gap)),
        int(math.floor(float(pt[2]) / gap)),
    )


def _root_blocked_n(
    cells: dict, pt: np.ndarray, nrm: np.ndarray, gap: float,
) -> bool:
    ix, iy, iz = _root_cell_of(pt, gap)
    g2 = gap * gap
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for q, qn in cells.get((ix + dx, iy + dy, iz + dz), ()):
                    if (
                        float(((pt - q) ** 2).sum()) < g2
                        and float(np.dot(nrm, qn)) > _ROOT_BLOCK_COS
                    ):
                        return True
    return False


def _root_mark_n(
    cells: dict, pt: np.ndarray, nrm: np.ndarray, gap: float,
) -> None:
    cells.setdefault(_root_cell_of(pt, gap), []).append(
        (np.asarray(pt, float), np.asarray(nrm, float)),
    )


# ── Union surface ─────────────────────────────────────────────────────────────

def _union_surface(meshes: list[trimesh.Trimesh]) -> trimesh.Trimesh:
    """Boolean union of the cluster solids (manifold engine)."""
    if len(meshes) == 1:
        return meshes[0]
    return trimesh.boolean.union(meshes, engine="manifold")


# ── Candidate generation on the union ─────────────────────────────────────────

def _surface_points_with_normals(
    mesh: trimesh.Trimesh, n: int, rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray]:
    """Area-weighted samples with smooth (barycentric vertex) normals."""
    pts, fi = _sample_surface(mesh, n, rng)
    if len(pts) == 0:
        return pts, pts
    tris = mesh.triangles[fi]
    bary = trimesh.triangles.points_to_barycentric(tris, pts)
    vn = mesh.vertex_normals[mesh.faces[fi]]
    normals = np.einsum("nk,nkj->nj", bary, vn)
    nl = np.linalg.norm(normals, axis=1, keepdims=True)
    normals = np.divide(normals, nl, out=np.zeros_like(normals), where=nl > 1e-9)
    return pts, normals


# ── Direction field ───────────────────────────────────────────────────────────

def _direction_field_angle(p: np.ndarray, seed: int) -> float:
    """Smooth low-frequency angle (radians) at a position.

    Sum of three positional sines with hash-derived phases: neighbouring
    leaves deviate together (coherent), far-apart leaves deviate
    independently — organic, not combed, not i.i.d. noise.
    """
    f = 2.0 * math.pi / _ORGANIC_DIR_WAVELEN_MM
    ph1 = _hash01(seed, "org-dir", 1) * 2.0 * math.pi
    ph2 = _hash01(seed, "org-dir", 2) * 2.0 * math.pi
    ph3 = _hash01(seed, "org-dir", 3) * 2.0 * math.pi
    s = (
        math.sin(f * float(p[0]) + ph1)
        + math.sin(f * 0.83 * float(p[1]) + ph2)
        + math.sin(f * 1.19 * float(p[2]) + ph3)
    ) / 3.0
    return math.radians(_ORGANIC_DIR_VAR_DEG) * s


# ── Shingle standoff ──────────────────────────────────────────────────────────

def _shingle_standoffs(
    bases: np.ndarray, dirs: np.ndarray, L: float, seed: int,
) -> np.ndarray:
    """Per-root standoff (mm): height-sorted accumulation over TIP conflicts.

    Bottom-up over all leaves: each stands off ``STEP`` more than the
    tallest already-processed leaf it has a POINT-END conflict with,
    clamped to ``CAP``.  A conflict exists only when one leaf's TIP comes
    near the other leaf's body (base-end, centre, or tip) — base-end to
    base-end adjacency is harmless nestling and must NOT propagate height,
    or the escalation runs away and every stitch neck stretches.
    """
    n = len(bases)
    out = np.zeros(n)
    tips = bases + 0.45 * L * dirs
    tails = bases - 0.45 * L * dirs
    d = _ORGANIC_SHINGLE_NEIGHBOR_MM
    r2 = _ORGANIC_TIP_CONFLICT_MM ** 2

    def _tip_conflict(i: int, j: int) -> bool:
        for a, b in ((i, j), (j, i)):
            ta = tips[a]
            for pt in (tails[b], bases[b], tips[b]):
                if float(((ta - pt) ** 2).sum()) < r2:
                    return True
        return False

    cells: dict[tuple[int, int, int], list[int]] = {}
    for i in np.argsort(bases[:, 2], kind="stable"):
        p = bases[i]
        c = (int(p[0] // d), int(p[1] // d), int(p[2] // d))
        tallest = -1.0
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    for j in cells.get((c[0] + dx, c[1] + dy, c[2] + dz), ()):
                        if _tip_conflict(int(i), j):
                            tallest = max(tallest, float(out[j]))
        base_so = 0.0 if tallest < 0.0 else tallest + _ORGANIC_SHINGLE_STEP_MM
        out[i] = (
            min(_ORGANIC_SHINGLE_CAP_MM, base_so)
            + _ORGANIC_STANDOFF_JITTER_MM * _hash01(seed, "org-so", int(i))
        )
        cells.setdefault(c, []).append(int(i))
    return out


# ── Public entry point ────────────────────────────────────────────────────────

def place_leaves_organic(
    meshes: list[trimesh.Trimesh],          # real (noised) foliage clumps
    *,
    length_mm: float,
    width_mm: float,
    thickness_mm: float,
    fold_angle_deg: float,
    inner_curve: float,
    outer_curve: float,
    curl_deg: float,
    seeds: int | list[int] = 0,
    labels: str | list[str] | None = None,
    # organic-specific (module-const defaults; promote to config later):
    spacing_frac: float | None = None,
    avoid_meshes: list[trimesh.Trimesh] | None = None,
    layering: str = "systematic",
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
    verbose: bool = True,
    debug_outcomes: list | None = None,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
    """Organic union-surface leaf placement (see module docstring).

    ``avoid_meshes`` (e.g. the branch/wood tubes): any leaf whose blade
    SURFACE intersects one is culled — leaves must never skewer branches.

    ``row_color_fn``, when given, is called with each leaf's LAYER index
    so the overlap layering is legible in debug renders.

    Returns ``(parts_per_mesh, stats_per_mesh)``; each leaf is attributed
    to the source cluster whose solid its root sits on.
    """
    if layering not in ("systematic", "random"):
        raise ValueError(f"layering must be 'systematic' or 'random', got {layering!r}")
    n_mesh = len(meshes)
    seeds_list = [seeds] * n_mesh if isinstance(seeds, int) else list(seeds)
    if labels is None:
        labels_list = [f"mesh-{i}" for i in range(n_mesh)]
    elif isinstance(labels, str):
        labels_list = [labels] * n_mesh
    else:
        labels_list = list(labels)
    seed0 = int(seeds_list[0]) if seeds_list else 0

    L = float(length_mm)
    W = float(width_mm)
    frac = _ORGANIC_SPACING_FRAC if spacing_frac is None else float(spacing_frac)
    spacing = max(frac * W, 0.3)
    expected_row_step = max(L * 0.5, 1e-3)

    leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = min(float(curl_deg), _ORGANIC_PITCH_CURL_DEG),
        lift_mm        = _ORGANIC_TIP_LIFT_MM,
    )
    t_total = time.perf_counter()

    # ── Stats shells (per source mesh) ────────────────────────────────────────
    stats_list: list[LeafPlacementStats] = []
    parts_list: list[list[trimesh.Trimesh]] = []
    z_mins: list[float] = []
    for mesh, label in zip(meshes, labels_list):
        z_mins.append(float(mesh.vertices[:, 2].min()))
        stats_list.append(LeafPlacementStats(
            label             = label,
            leaf_length_mm    = L,
            leaf_width_mm     = W,
            col_step          = max(W, 1e-3),
            expected_row_step = expected_row_step,
            z_top             = float(mesh.vertices[:, 2].max()),
            z_top_anchor      = float(mesh.vertices[:, 2].max()),
            cx                = float(mesh.vertices[:, 0].mean()),
            cy                = float(mesh.vertices[:, 1].mean()),
            lift_mm           = 0.0,
        ))
        parts_list.append([])

    # ── Phase 1: union + maximal Poisson-disk roots ───────────────────────────
    union = _union_surface(meshes)
    area = float(union.area)
    centroid = union.vertices.mean(axis=0)
    rng = np.random.default_rng(seed0 & 0xFFFFFFFF)

    n_cand = int(max(64, _ORGANIC_CANDIDATE_FACTOR * area / (spacing ** 2)))
    pts, normals = _surface_points_with_normals(union, n_cand, rng)
    placeable = normals[:, 2] >= _ORGANIC_PLACEABLE_NORMAL_Z

    grid: dict[tuple[int, int, int], list] = {}
    root_pts: list[np.ndarray] = []
    root_nrm: list[np.ndarray] = []
    for k in np.nonzero(placeable)[0]:
        p = pts[k]
        nrm_k = _safe_norm(normals[k])
        if _root_blocked_n(grid, p, nrm_k, spacing):
            continue
        _root_mark_n(grid, p, nrm_k, spacing)
        root_pts.append(p)
        root_nrm.append(nrm_k)
    bases = np.array(root_pts) if root_pts else np.zeros((0, 3))
    n_roots = len(bases)

    # ── Coverage verification: fresh samples, all must be within spacing of a
    # root on a SIMILAR-FACING surface (same normal-aware rule as placement,
    # else a root across a seam V "covers" the opposite wall it can't reach).
    n_test = int(max(64, area / (_ORGANIC_VERIFY_RES_MM ** 2)))
    tpts, tnrm = _surface_points_with_normals(
        union, n_test, np.random.default_rng((seed0 ^ 0x5EED) & 0xFFFFFFFF),
    )
    tmask = tnrm[:, 2] >= _ORGANIC_PLACEABLE_NORMAL_Z
    n_uncovered = sum(
        1 for k in np.nonzero(tmask)[0]
        if not _root_blocked_n(grid, tpts[k], _safe_norm(tnrm[k]), spacing)
    )

    # ── Phase 2: standoffs + direction + build ────────────────────────────────
    # Source-cluster attribution, batched per mesh (probe just inside the
    # surface).  Per-leaf contains() against every cluster was the full-tree
    # hot spot (~30 embree calls × 2000 leaves).
    src_idx = np.zeros(n_roots, dtype=int)
    if n_roots:
        probes = bases - 0.2 * np.array(root_nrm)
        assigned = np.zeros(n_roots, dtype=bool)
        for mi in range(n_mesh):
            ins = np.asarray(meshes[mi].contains(probes), dtype=bool)
            take = ins & ~assigned
            src_idx[take] = mi
            assigned |= ins

    # Per-root growth directions (needed by the standoff pass: escalation
    # only propagates through point-end conflicts, which depend on where
    # each leaf's TIP lands).
    dirs_leaf = np.zeros((n_roots, 3))
    for i in range(n_roots):
        T0 = _growth_tangent(root_nrm[i], bases[i], centroid)
        dirs_leaf[i] = _safe_norm(_rotate_about(
            T0, root_nrm[i], _direction_field_angle(bases[i], seed0),
        ))

    standoffs = (
        _shingle_standoffs(bases, dirs_leaf, L, seed0)
        if n_roots else np.zeros(0)
    )

    avoid = list(avoid_meshes) if avoid_meshes else []

    # ── Per-leaf jobs, driven BATCHED ─────────────────────────────────────
    # Each root's whole pipeline (anchor re-projection → seat/build/tuck →
    # branch cull) is one request generator; _drive_batched advances all of
    # them in rounds, grouping every same-kind embree query of a round into
    # ONE call.  The per-leaf math is the shared scalar pipeline, unchanged
    # — embree contains/first-hit results are per-point independent, so the
    # outcomes are identical to the old leaf-at-a-time loop; only the ~85 µs
    # fixed per-call overhead (~26 tiny calls per leaf) is amortised.
    def _leaf_job(i: int, Ls: float, Ws: float, kw: dict, st: float, flush: bool,
                  lseed: int):
        base = bases[i]
        nrm = root_nrm[i]
        T_leaf = dirs_leaf[i]

        # Centre the blade on the root.  The build machinery anchors the
        # oval's tip-half at its anchor point, so the blade spans 0.75·L
        # up-slope but only 0.25·L down-slope of it — a 75/25 bias that
        # leaves bare bands wherever "up-slope" exits a face (e.g. over a
        # crest).  Re-projecting the anchor 0.25·L down-slope makes the
        # blade cover ±L/2 around the ACTUAL root, so Poisson maximality
        # over roots translates into visual coverage.
        # Tight drop budget: a projection that falls further than ~1 mm has
        # crossed a crease onto a different wall — anchoring there put the
        # blade millimetres from its root oval (long-neck extrusions).
        proj = yield from _project_to_surface_gen(
            union, base + 0.25 * Ls * T_leaf, nrm,
            1.0 + 0.25 * Ls + 1.0,
        )
        anchor, anchor_n = proj if proj is not None else (base, nrm)

        result, reason = yield from _attempt_leaf_gen(
            union, [], anchor, anchor_n, T_leaf, Ls, Ws, kw, lseed,
            standoff_mm=st * float(standoffs[i]),
            bury_lift=True,
            seat_fallback_flat=True,
            skip_skew=flush,
            max_skew_frac=_ORGANIC_MAX_SKEW_FRAC,
            max_neck_mm=_ORGANIC_MAX_NECK_MM,
            tuck_base=True,
            tuck_tip=True,
            tuck_tip_max_mm=0.02 + st * _ORGANIC_TIP_CEIL_RANGE_MM,
            arch_mm=(1.0 - st) * _ORGANIC_FLUSH_ARCH_MM,
        )
        if result is None:
            return None, reason

        # Branch-collision cull: blade surface (first half of the solid's
        # vertex block) must not intersect any EXPOSED branch.  A blade
        # vertex inside a wood tube that is itself inside the canopy union
        # is invisible and harmless — culling those holes the skin
        # wherever a branch runs under it (~20% of all leaves).  Only a
        # vertex that is inside a branch AND outside the canopy skewers
        # visibly.
        if avoid:
            solid = result[0]
            blade_v = solid.vertices[: len(solid.vertices) // 2]
            in_branch = np.zeros(len(blade_v), dtype=bool)
            for _bm in avoid:
                _bb = _bm.bounds
                if (base < _bb[0] - Ls).any() or (base > _bb[1] + Ls).any():
                    continue
                in_branch |= np.asarray(
                    (yield ("contains", _bm, blade_v)), dtype=bool,
                )
            if in_branch.any() and (~np.asarray(
                (yield ("contains", union, blade_v[in_branch])), dtype=bool,
            )).any():
                return result, "branch"
        return result, None

    jobs = []
    flush_flags = np.zeros(n_roots, dtype=bool)
    for i in range(n_roots):
        nrm = root_nrm[i]
        # Zone factor: 1 = upward face (pitched blade), 0 = underside
        # (arch-embedded blade), smooth in between.
        tz = min(max((float(nrm[2]) - _ORGANIC_ZONE_LO)
                     / (_ORGANIC_ZONE_HI - _ORGANIC_ZONE_LO), 0.0), 1.0)
        st = tz * tz * (3.0 - 2.0 * tz)
        flush = st < 0.5
        flush_flags[i] = flush
        lseed = int(_hash01(seed0, "org-leaf", i) * 2 ** 31)
        # Per-leaf size jitter, downward only: max stays at the configured
        # (printable) leaf size.
        scale = 1.0 - _ORGANIC_SIZE_JITTER * _hash01(seed0, "org-size", i)
        Ls = L * scale
        Ws = W * scale
        kw = dict(
            leaf_kw,
            length_mm=Ls, width_mm=Ws,
            curl_deg=st * min(float(curl_deg), _ORGANIC_PITCH_CURL_DEG),
            lift_mm=st * _ORGANIC_TIP_LIFT_MM,
        )
        jobs.append(_leaf_job(i, Ls, Ws, kw, st, flush, lseed))

    outcomes = _drive_batched(jobs)

    # ── Bookkeeping in root order (identical to the old sequential loop) ──
    n_branch_cull = 0
    n_build_fail = 0
    for i in range(n_roots):
        base = bases[i]
        flush = bool(flush_flags[i])
        src = int(src_idx[i])
        stats = stats_list[src]
        stats.n_attempted += 1

        result, reason = outcomes[i]
        if result is None:
            n_build_fail += 1
            if reason.startswith("buried"):
                stats.skipped_cross_buried += 1
            elif reason in ("floor", "neck"):
                stats.skipped_below_floor += 1
            else:
                stats.build_errors += 1
            if debug_outcomes is not None:
                debug_outcomes.append((base.copy(), f"fail-{reason}"))
            continue
        if debug_outcomes is not None:
            debug_outcomes.append((base.copy(), "placed"))
        solid, tangent_leaf, skew_mm, tip_clr, drop_mm = result
        if reason == "branch":
            n_branch_cull += 1
            if debug_outcomes is not None:
                debug_outcomes.append((base.copy(), "fail-branch"))
            continue

        row_idx = int((float(base[2]) - z_mins[src]) / expected_row_step)
        stats.base_positions.append(base.copy())
        stats.base_tangents.append(tangent_leaf.copy())
        stats.base_row_idx.append(row_idx)
        stats.root_depths.append(_ROOT_EMBED_MM)
        stats.leaf_float_dists.append(0.0)
        stats.leaf_buried_depths.append(0.0)
        stats.shingle_layers.append(1 if flush else 0)
        stats.tip_z_clearances.append(tip_clr)
        stats.tip_z_lifts.append(skew_mm)
        stats.pull_aways.append(-drop_mm)
        stats.n_placed += 1

        if len(solid.vertices) > 0:
            if row_color_fn is not None:
                # Debug colour by zone: 0 = pitched blade, 1 = flush blade.
                color = np.asarray(row_color_fn(1 if flush else 0), dtype=np.uint8)
                solid.visual = trimesh.visual.ColorVisuals(
                    mesh=solid,
                    face_colors=np.tile(color, (len(solid.faces), 1)),
                )
            parts_list[src].append(solid)

    elapsed = time.perf_counter() - t_total
    if verbose:
        placed = sum(s.n_placed for s in stats_list)
        print(
            f"\n── organic leaf placement ──  {placed} placed / {n_roots} roots "
            f"(spacing={spacing:.2f}mm, layering={layering}, "
            f"build-fail={n_build_fail}, branch-cull={n_branch_cull}, "
            f"uncovered-test-pts={n_uncovered})  "
            f"{elapsed:.3f}s\n"
        )

    return parts_list, stats_list
