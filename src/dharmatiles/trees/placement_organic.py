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
5. **Per-leaf build** reuses the shoots pipeline verbatim
   (:func:`placement_shoots._attempt_leaf`): equal-depth oval seat,
   rigid blade↔oval frame, printability skew, belly-dip drop to
   ``_PROTRUSION_MM``, tip/belly cull, solidify.

Hard perf constraints (2026-07-01 crisis) remain: no per-leaf mesh scans;
embree only; cheap-reject before every build.

Public entry point mirrors the other placers so the dispatch in
:func:`mesh.build_branch_mesh` is a drop-in.
"""
from __future__ import annotations

import math
import time
from collections.abc import Callable

import numpy as np
import trimesh

from ._utils import _hash01, _safe_norm
from .placement import LeafPlacementStats
from .placement_greedy import _growth_tangent, _sample_surface
from .placement_shoots import (
    _GREEDY_EMBED_MM,
    _attempt_leaf,
    _project_to_surface,
    _rotate_about,
)

# ── Organic-specific constants (module constants while iterating) ─────────────
# Root spacing as a fraction of leaf width — THE overlap knob.
# ~0.45 = moderate AC-style shingle; ~0.8 = light touch.
_ORGANIC_SPACING_FRAC: float = 0.45

# Candidate over-generation factor: candidates ≈ this × (area / spacing²).
# Generous so the dart throw actually saturates (maximality needs excess).
_ORGANIC_CANDIDATE_FACTOR: float = 10.0

# Blade zones by surface normal:
#   nz ≥ _ORGANIC_PITCH_NORMAL_Z    → pitched blade (tip lifted, skewed)
#   nz ≥ _ORGANIC_PLACEABLE_NORMAL_Z → FLUSH blade: lift 0, gentle curl,
#       no printability skew.  A flush blade lies on the substrate (walls
#       well under a millimetre, supported by the surface below) so it
#       inherits the cluster's own printability — this is the Q9 "less
#       fancy leaf" for the downward zone, and it kills the long wall
#       prisms that skewed pitched blades grew on steep low surfaces.
#   below → bare (deep underside).
_ORGANIC_PITCH_NORMAL_Z: float = -0.05
_ORGANIC_PLACEABLE_NORMAL_Z: float = -0.75

# Direction field: down-slope rotated by a smooth positional angle field.
# Kept modest: divergent neighbours are the main source of blade sheets
# slicing through each other.
_ORGANIC_DIR_VAR_DEG: float = 15.0     # peak deviation from down-slope
_ORGANIC_DIR_WAVELEN_MM: float = 7.0   # spatial wavelength of the field

# Shingle pitch: every pitched blade's TIP floats this far above its base
# level (the leaf builder's rigid tip rotation) — just enough to clear the
# blade below (≈2× blade thickness), no more.  The aesthetic-judge pass
# flagged 1.2 mm as the "artichoke" tell: tips flaring off the mass.  The
# AC look wants blades HUGGING the mass, tips drooping down-slope; the
# monotone standoff below (not the lift) owns the upper-over-lower
# guarantee.
_ORGANIC_TIP_LIFT_MM: float = 0.6

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

# Neck gate: reject a blade whose accumulated in-plane slide + net normal
# standoff would stretch the blade→oval stitch walls beyond this — the
# "long-rooted leaves sticking out peculiarly" (wall chimneys/fans).
_ORGANIC_MAX_NECK_MM: float = 2.0

# Skew cap for pitched blades (fraction of leaf length): slides longer
# than this stretched the stitch walls into visible prisms; cull instead
# (rare, fringe-only, and the flush zone now owns the steep-down band).
_ORGANIC_MAX_SKEW_FRAC: float = 0.35

# Flush-blade shape overrides (the "less fancy" leaf).
_ORGANIC_FLUSH_CURL_DEG: float = 12.0

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
    bases: np.ndarray, src_idx: np.ndarray, meshes: list, seed: int,
) -> np.ndarray:
    """Per-root standoff (mm): height-sorted accumulation, capped.

    Bottom-up over ALL leaves: each stands off ``STEP`` more than the
    tallest already-processed leaf it overlaps (neighbourhood radius
    ``_ORGANIC_SHINGLE_NEIGHBOR_MM``), clamped to ``CAP``.  Overlapping
    leaves at different heights therefore differ by a full STEP until the
    cap; at the cap the tip lift resolves the remaining tip-over-base
    contacts.  Monotone in processing order ⇒ never inverts.
    """
    del src_idx, meshes
    n = len(bases)
    out = np.zeros(n)
    d = _ORGANIC_SHINGLE_NEIGHBOR_MM
    d2 = d * d
    cells: dict[tuple[int, int, int], list[int]] = {}
    for i in np.argsort(bases[:, 2], kind="stable"):
        p = bases[i]
        c = (int(p[0] // d), int(p[1] // d), int(p[2] // d))
        tallest = -1.0
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    for j in cells.get((c[0] + dx, c[1] + dy, c[2] + dz), ()):
                        if float(((p - bases[j]) ** 2).sum()) < d2:
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
    lift_mm: float,
    seeds: int | list[int] = 0,
    labels: str | list[str] | None = None,
    angle_jitter_deg: float = 0.0,
    pos_jitter: float = 0.0,
    # organic-specific (module-const defaults; promote to config later):
    spacing_frac: float | None = None,
    layering: str = "systematic",
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
    verbose: bool = True,
    debug_outcomes: list | None = None,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
    """Organic union-surface leaf placement (see module docstring).

    ``row_color_fn``, when given, is called with each leaf's LAYER index
    so the overlap layering is legible in debug renders.

    ``lift_mm``, ``angle_jitter_deg`` and ``pos_jitter`` are accepted for
    signature parity and ignored (the seat replaces the lift; the
    direction field replaces i.i.d. jitter).

    Returns ``(parts_per_mesh, stats_per_mesh)`` — the same contract as
    the other placers; each leaf is attributed to the source cluster
    whose solid its root sits on.
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

    del lift_mm, angle_jitter_deg, pos_jitter   # parity-only
    leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
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

    standoffs = (
        _shingle_standoffs(bases, src_idx, meshes, seed0)
        if n_roots else np.zeros(0)
    )

    # Flush-blade variant for the downward zone (see zone constants).
    leaf_kw_flush = dict(
        leaf_kw,
        lift_mm  = 0.0,
        curl_deg = _ORGANIC_FLUSH_CURL_DEG,
    )

    n_build_fail = 0
    for i in range(n_roots):
        base = bases[i]
        nrm = root_nrm[i]
        flush = float(nrm[2]) < _ORGANIC_PITCH_NORMAL_Z
        T0 = _growth_tangent(nrm, base, centroid)
        ang = _direction_field_angle(base, seed0)
        T_leaf = _safe_norm(_rotate_about(T0, nrm, ang))
        lseed = int(_hash01(seed0, "org-leaf", i) * 2 ** 31)

        src = int(src_idx[i])
        stats = stats_list[src]
        stats.n_attempted += 1

        # Per-leaf size jitter, downward only: max stays at the configured
        # (printable) leaf size.
        scale = 1.0 - _ORGANIC_SIZE_JITTER * _hash01(seed0, "org-size", i)
        Ls = L * scale
        Ws = W * scale
        kw = dict(
            leaf_kw_flush if flush else leaf_kw,
            length_mm=Ls, width_mm=Ws,
        )

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
        proj = _project_to_surface(
            union, base + 0.25 * Ls * T_leaf, nrm,
            1.0 + 0.25 * Ls + 1.0,
        )
        anchor, anchor_n = proj if proj is not None else (base, nrm)

        result, reason = _attempt_leaf(
            union, [], anchor, anchor_n, T_leaf, Ls, Ws, kw, lseed,
            standoff_mm=0.0 if flush else float(standoffs[i]),
            bury_lift=True,
            seat_fallback_flat=True,
            skip_skew=flush,
            max_skew_frac=_ORGANIC_MAX_SKEW_FRAC,
            max_neck_mm=_ORGANIC_MAX_NECK_MM,
        )
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

        row_idx = int((float(base[2]) - z_mins[src]) / expected_row_step)
        stats.base_positions.append(base.copy())
        stats.base_tangents.append(tangent_leaf.copy())
        stats.base_row_idx.append(row_idx)
        stats.root_depths.append(_GREEDY_EMBED_MM)
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
            f"build-fail={n_build_fail}, uncovered-test-pts={n_uncovered})  "
            f"{elapsed:.3f}s\n"
        )

    return parts_list, stats_list
