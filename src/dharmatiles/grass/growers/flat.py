"""Flat / peaked / round grass grower — pluggable cross-section."""

from __future__ import annotations

import numpy as np
import trimesh

from .._geometry import (
    _blade_step_geometry, _cell_index,
    _contained_segment_cells, _sample_grid, _spine_distances, _stamp_segment,
)
from ..config import SpeciesConfig
from ..seed import GrassPath, GrowingPath


class FlatGrassGrower:
    """Grow and mesh floppy grass blades with a configurable cross-section.

    Cross-section is controlled by ``species.blade_top_facets``:
      1  → flat (top surface IS the equator; thickness ignored)
      2  → peaked / leaf (two faces, centre ridge at +thickness)
      N  → round  (N faces, sine arc to +thickness at centre)

    A V-keel hangs below each blade edge at depth
    ``species.keel_fraction × blade_width``, tapering to zero at the tip.
    """

    @staticmethod
    def step(
        path: GrowingPath,
        occ_z: np.ndarray,
        scene,
        surface,
        cfg,
        species: SpeciesConfig,
    ) -> bool:
        if not path.alive or len(path.points) == 0:
            path.alive = False
            return False

        seed = path.seed
        cx, cy, cz = path.points[-1]
        step_idx = len(path.points) - 1
        tx, ty, _, prev_taper, next_taper = _blade_step_geometry(seed, step_idx, cx, cy)
        prev_width = seed.blade_width * prev_taper
        next_width = seed.blade_width * next_taper
        prev_thickness = species.blade_thickness * prev_taper
        next_thickness = species.blade_thickness * next_taper
        hw = next_width / 2.0

        if not (hw <= tx <= surface.tile_w - hw and hw <= ty <= surface.tile_h - hw):
            path.alive = False
            return False

        # Kill blade if the next step's spine OR leading-edge width enters any
        # obstacle cell.  The leading-edge check catches tangential approach:
        # a blade growing parallel to a flower boundary can have its physical
        # mesh (half-width) inside the obstacle even though the spine is just
        # outside, and the height-based floor_z check misses it because the
        # leading-edge strip points away from the obstacle interior.
        if scene.obstacle_mask is not None:
            tix, tiy = _cell_index(surface, tx, ty)
            if scene.obstacle_mask[tiy, tix]:
                path.alive = False
                return False
            _fp = _leading_edge_cells(surface, cx, cy, tx, ty, hw)
            if _fp is not None:
                _ix0, _ix1, _iy0, _iy1, _le, _, _ = _fp
                if np.any(_le) and np.any(
                    scene.obstacle_mask[_iy0:_iy1 + 1, _ix0:_ix1 + 1][_le]
                ):
                    path.alive = False
                    return False

        floor_z = _sample_footprint_max(
            occ_z,
            scene.terrain_support_z,
            surface,
            tx,
            ty,
            next_width,
            x0=cx,
            y0=cy,
        )
        terrain_z = _sample_grid(scene.terrain_z, surface, tx, ty)
        nz = max(terrain_z, floor_z) + seed.blade_clearance

        if floor_z - terrain_z > cfg.max_stack_height:
            path.alive = False
            return False
        if len(path.points) > 1 and nz > cz + seed.blade_rise_cap:
            path.alive = False
            return False

        path.points.append((float(tx), float(ty), float(nz)))
        _stamp_segment(
            occ_z, surface,
            cx, cy, tx, ty,
            prev_width, next_width,
            cz, nz,
            prev_thickness, next_thickness,
            species.blade_top_facets,
        )
        return True

    @staticmethod
    def build_mesh(
        path: GrassPath,
        species: SpeciesConfig,
        scene,
        surface,
    ) -> trimesh.Trimesh | None:
        if len(path.points) < 2:
            return None

        seed = path.seed
        spine = np.asarray(path.points, dtype=float)
        spine = _smooth_blade_spine(spine, species.blade_smooth)

        n = len(spine)
        path_dists = _spine_distances(spine)
        total_len = float(path_dists[-1])
        point_tapers = seed.distance_taper_vec(path_dists, total_len)

        # ── Width taper (physical distance from the actual final tip) ─────────
        widths = seed.blade_width * point_tapers

        # ── Body-width floor (FDM printability) ───────────────────────────────
        # Clamp widths to min_printable_width everywhere except the tip-taper
        # zone (last blade_taper mm), which is left free so the tip still tapers
        # to a point.  For a 0.4 mm nozzle, 1.2 mm = 3× nozzle is the default.
        min_w = float(species.min_printable_width)
        if min_w > 0.0:
            taper_start = total_len - float(seed.blade_taper)
            body_mask = path_dists <= taper_start
            # max(width, min_w) in body; max(width, 0) = width in tip zone.
            widths = np.maximum(widths, min_w * body_mask)

        # ── Keel depth — fixed throughout; does not taper with width ──────────
        base_keel = species.keel_fraction * seed.blade_width
        keel_depths = np.full(n, base_keel)

        # ── Top-profile height taper (full from seed, taper near tip) ─────────
        thicknesses = species.blade_thickness * point_tapers

        # Both ends are normal rings closed with end-caps.  Ring 0 (seed) is
        # anchored in the terrain via its keel.  The tip ring tapers to ~zero
        # width; _make_ring_verts collapses it to a point naturally.
        return _build_blade_mesh(spine, widths, thicknesses, keel_depths, species.blade_top_facets, surface, close_bottom=True, close_top=True)


# ── Cross-section ring construction ──────────────────────────────────────────

def _compute_keel_direction(tangent: np.ndarray) -> np.ndarray:
    """Return the unit keel direction for a blade tip with the given tangent.

    Rule 1 — shape: rotate the tangent 45° toward −Z in the vertical plane
    containing the tangent.  This keeps the keel end-cap at a consistent 45°
    mitre relative to the blade's own axis — giving upward blades bulk and
    giving horizontal blades the standard pointy-grass look.

    Rule 2 — printability floor: clamp so the result is never more than 45°
    below horizontal (elevation ≥ −45°, i.e. keel.z ≥ −1/√2 ≈ −0.707).
    This prevents the underside of a steeply-downward blade tip from becoming
    an unsupported FDM cantilever.
    """
    COS45 = np.sqrt(0.5)  # 1/√2

    # Singular case: blade is nearly vertical (no preferred horizontal direction).
    t_horiz_sq = tangent[0] ** 2 + tangent[1] ** 2
    if t_horiz_sq < 1e-18:
        # Use +X as the arbitrary horizontal component; keel at 45° below horizontal.
        return np.array([COS45, 0.0, -COS45])

    # N_down: perpendicular to T in the vertical plane containing T and Z,
    # pointing "downward" (toward −Z relative to T).
    z_dot_t = float(tangent[2])
    perp = np.array([0.0, 0.0, 1.0]) - z_dot_t * tangent  # toward +Z relative to T
    N_down = -(perp / np.linalg.norm(perp))               # flip → toward −Z relative to T

    # Rule 1: rotate T by 45° toward N_down.
    # Both T and N_down are orthogonal unit vectors, so the result is a unit vector.
    K = tangent * COS45 + N_down * COS45

    # Rule 2: clamp elevation to ≥ −45° (K[2] ≥ −COS45).
    if K[2] < -COS45:
        K_horiz = np.array([K[0], K[1], 0.0])
        K_horiz_norm = np.linalg.norm(K_horiz)
        K_horiz_dir = K_horiz / K_horiz_norm if K_horiz_norm > 1e-9 else np.array([1.0, 0.0, 0.0])
        K = K_horiz_dir * COS45 + np.array([0.0, 0.0, -COS45])

    return K


def _make_ring_verts(
    center: np.ndarray,
    tangent: np.ndarray,
    width: float,
    thickness: float,
    keel_depth: float,
    n_top_facets: int,
) -> np.ndarray:
    """Cross-section ring vertices: (n_top_facets + 2, 3).

    Layout:
      vertex 0         — keel  (centre + keel_dir * keel_depth)
      vertices 1..n+1  — top profile from left (−w/2) to right (+w/2),
                         heights = thickness × sin(π × i/n) above spine_z.

    For n=1: heights at i=0 and i=1 are both sin(0)=sin(π)=0, so the top
    surface is flat at the equator (spine_z) and thickness has no effect.
    For n=2: three vertices with the centre one at spine_z + thickness.
    For n≥3: sine arc approximating a round cross-section.

    For non-collapsed rings (width ≥ 1e-6), the keel hangs straight down
    (world −Z).  For collapsed tip rings (width < 1e-6), the top-profile
    vertices all collapse to *center* but the keel vertex is placed using
    _compute_keel_direction so the tip end-cap has the correct FDM angle:
    45° from the blade tangent, clamped to never point more than 45° below
    horizontal.

    The lateral direction is perpendicular to the spine tangent in XY.
    Vertical is always world +Z.
    """
    nvr = n_top_facets + 2

    # Collapsed tip ring — all top-profile vertices converge to *center*, but
    # the keel vertex is placed at the tangent-derived position so the tube
    # faces from the previous real ring form the correct tip-end angle.
    if width < 1e-6:
        verts = np.tile(center, (nvr, 1)).copy()
        if keel_depth > 1e-9:
            keel_dir = _compute_keel_direction(tangent)
            verts[0] = center + keel_dir * keel_depth
        return verts

    half_w = width / 2.0

    # Perpendicular to tangent in XY
    perp = np.array([-tangent[1], tangent[0], 0.0])
    pn = np.linalg.norm(perp)
    perp = perp / pn if pn > 1e-9 else np.array([1.0, 0.0, 0.0])
    up = np.array([0.0, 0.0, 1.0])

    keel_pt = center - up * keel_depth

    top_verts = []
    for i in range(n_top_facets + 1):
        x_frac = i / n_top_facets
        lat = -half_w + width * x_frac
        h = thickness * np.sin(np.pi * x_frac)
        top_verts.append(center + perp * lat + up * h)

    return np.array([keel_pt] + top_verts)


def _build_blade_mesh(
    spine: np.ndarray,           # (n_rings, 3)
    widths: np.ndarray,          # (n_rings,) — tapers toward zero at tip
    thicknesses: np.ndarray,     # (n_rings,) — profile heights
    keel_depths: np.ndarray,     # (n_rings,)
    n_top_facets: int,
    surface,
    close_bottom: bool = False,  # end-cap across ring 0
    close_top: bool = False,     # end-cap across last ring
) -> trimesh.Trimesh | None:
    """Build a tube mesh along the spine with the given cross-section.

    Ring construction and face connectivity are fully vectorised.  The only
    remaining Python loop handles the (typically one) collapsed tip ring
    (width < 1e-6) whose keel direction is tangent-dependent and can't be
    batched without a separate kernel.

    Both ends can be closed with flat end-cap fans.
    """
    n_rings = len(spine)
    nvr = n_top_facets + 2  # verts per ring: keel + (n_top_facets+1) top-profile verts

    # ── Normalised tangents ─────────────────────────────────────────────────
    tangents = np.empty_like(spine)
    tangents[:-1] = spine[1:] - spine[:-1]
    tangents[-1]  = spine[-1] - spine[-2]
    t_norms = np.linalg.norm(tangents, axis=1, keepdims=True)
    tangents /= np.where(t_norms < 1e-9, 1.0, t_norms)

    # ── Batch ring construction ─────────────────────────────────────────────
    # Perpendicular to tangent in XY: (-ty, tx, 0), normalised.
    perp = np.stack([-tangents[:, 1], tangents[:, 0],
                     np.zeros(n_rings, dtype=float)], axis=1)   # (n_rings, 3)
    pn = np.linalg.norm(perp, axis=1, keepdims=True)
    perp = np.where(pn > 1e-9, perp / np.maximum(pn, 1e-9),
                    np.array([[1.0, 0.0, 0.0]]))

    # Top-profile x-fractions (constant across rings)
    x_fracs  = np.arange(n_top_facets + 1) / max(n_top_facets, 1)  # (n_top_facets+1,)
    sin_vals = np.sin(np.pi * x_fracs)                              # (n_top_facets+1,)

    # Keel vertices: spine_z − keel_depth
    keel_verts = spine.copy()
    keel_verts[:, 2] -= keel_depths                                 # (n_rings, 3)

    # Top profile: (n_rings, n_top_facets+1, 3)
    lats     = (-widths[:, None] / 2.0 +
                 widths[:, None] * x_fracs[None, :])                # (n_rings, n_top_facets+1)
    heights  = thicknesses[:, None] * sin_vals[None, :]             # (n_rings, n_top_facets+1)
    up       = np.array([0.0, 0.0, 1.0])
    top_verts = (spine[:, None, :] +
                 perp[:, None, :] * lats[:, :, None] +
                 up * heights[:, :, None])                          # (n_rings, n_top_facets+1, 3)

    # Combined ring buffer: (n_rings, nvr, 3)
    all_rings = np.concatenate([keel_verts[:, None, :], top_verts], axis=1)

    # Overwrite collapsed rings (width < 1e-6): all verts → spine; keel → tangent-derived
    for ci in np.where(widths < 1e-6)[0]:
        all_rings[ci] = spine[ci]
        if keel_depths[ci] > 1e-9:
            kd = _compute_keel_direction(tangents[ci])
            all_rings[ci, 0] = spine[ci] + kd * keel_depths[ci]

    all_verts = all_rings.reshape(-1, 3)                            # (n_rings * nvr, 3)
    np.clip(all_verts[:, 0], 0.0, surface.tile_w, out=all_verts[:, 0])
    np.clip(all_verts[:, 1], 0.0, surface.tile_h, out=all_verts[:, 1])

    # ── Vectorised face construction ────────────────────────────────────────
    face_parts: list[np.ndarray] = []

    if n_rings > 1:
        ri  = np.arange(n_rings - 1, dtype=np.int32)   # (n_rings-1,)
        vi  = np.arange(nvr, dtype=np.int32)            # (nvr,)
        vi1 = (vi + 1) % nvr

        a  = ri[:, None] * nvr + vi[None, :]            # (n_rings-1, nvr)
        a1 = ri[:, None] * nvr + vi1[None, :]
        b  = (ri[:, None] + 1) * nvr + vi[None, :]
        b1 = (ri[:, None] + 1) * nvr + vi1[None, :]

        face_parts.append(np.stack([a.ravel(),  b.ravel(),  b1.ravel()], axis=1))
        face_parts.append(np.stack([a.ravel(),  b1.ravel(), a1.ravel()], axis=1))

    # Bottom end-cap: fan from keel vertex (ring 0, vert 0) across the base.
    if close_bottom:
        jb = np.arange(1, nvr - 1, dtype=np.int32)
        face_parts.append(np.column_stack(
            [np.zeros(nvr - 2, dtype=np.int32), jb, jb + 1]))

    # Top end-cap: fan from keel vertex (last ring, vert 0) across the tip.
    if close_top:
        a_t = np.int32((n_rings - 1) * nvr)
        jt  = np.arange(1, nvr - 1, dtype=np.int32)
        face_parts.append(np.column_stack(
            [np.full(nvr - 2, a_t, dtype=np.int32), a_t + jt + 1, a_t + jt]))

    if not face_parts:
        return None

    faces_arr = np.vstack(face_parts).astype(np.int32)
    mesh = trimesh.Trimesh(vertices=all_verts, faces=faces_arr, process=False)
    # Merge coincident vertices (collapsed tip rings produce duplicate verts).
    mesh.merge_vertices()
    mesh.update_faces(mesh.area_faces > 1e-12)
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    return mesh


# ── Spine smoothing ───────────────────────────────────────────────────────────

def _smooth_blade_spine(spine: np.ndarray, blade_smooth: float) -> np.ndarray:
    """Smooth only the Z component of the spine toward a quadratic arc.

    XY is left untouched so blade curl is preserved.  Z smoothing removes the
    stairstepping that accumulates when terrain-following increments are
    discretised to grid-cell heights.
    """
    amount = float(np.clip(blade_smooth, 0.0, 1.0))
    if amount <= 0.0 or len(spine) < 3:
        return spine
    arc = _fit_quadratic_arc(spine)
    smoothed = spine.copy()
    smoothed[:, 2] = spine[:, 2] + amount * (arc[:, 2] - spine[:, 2])
    smoothed[0] = spine[0]
    smoothed[-1] = spine[-1]
    return smoothed


def _fit_quadratic_arc(points: np.ndarray) -> np.ndarray:
    p0 = points[0]
    p1 = points[-1]
    segment_lengths = np.linalg.norm(np.diff(points, axis=0), axis=1)
    total_length = float(segment_lengths.sum())
    if total_length <= 1e-9:
        return points.copy()
    t = np.concatenate([[0.0], np.cumsum(segment_lengths) / total_length])
    basis = 2.0 * (1.0 - t) * t
    arc = ((1.0 - t) ** 2)[:, None] * p0 + (t ** 2)[:, None] * p1
    fit = points - arc
    denom = float(np.dot(basis, basis))
    if denom <= 1e-12:
        return points.copy()
    control = (basis[:, None] * fit).sum(axis=0) / denom
    arc += basis[:, None] * control
    arc[0] = p0
    arc[-1] = p1
    return arc


# ── Grid / footprint helpers ─────────────────────────────────────────────────

def _sample_footprint_max(
    occ_z: np.ndarray,
    base_z: np.ndarray,
    surface,
    x: float,
    y: float,
    width: float,
    x0: float | None = None,
    y0: float | None = None,
) -> float:
    """Sample max occ_z along the tapered leading edge of the next footprint.

    Stamping stays conservative and only writes fully-contained cells.  Sampling
    only probes the new segment's tapered front cap so the blade reacts to
    support ahead of it without detecting its own already-stamped swept trail.
    """
    hw = width / 2.0

    if x0 is not None and y0 is not None:
        footprint = _leading_edge_cells(surface, x0, y0, x, y, hw)
        if footprint is None:
            return _sample_grid(base_z, surface, x, y)
        ix0g, ix1g, iy0g, iy1g, mask, _, _ = footprint
        if not np.any(mask):
            return _sample_grid(base_z, surface, x, y)
        block = occ_z[iy0g:iy1g + 1, ix0g:ix1g + 1]
        return float(block[mask].max())

    return _sample_grid(base_z, surface, x, y)


def _leading_edge_cells(
    surface,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    hw: float,
) -> tuple[int, int, int, int, np.ndarray, np.ndarray, np.ndarray] | None:
    """Return cells intersected by the proposed segment's leading edge."""
    dx = x1 - x0
    dy = y1 - y0
    segment_length = float(np.hypot(dx, dy))
    if segment_length < 1e-9:
        return None

    ux, uy = dx / segment_length, dy / segment_length
    px, py = -uy, ux
    ax = x1 + px * hw
    ay = y1 + py * hw
    bx = x1 - px * hw
    by = y1 - py * hw

    min_x = max(0.0, min(ax, bx))
    max_x = min(surface.tile_w, max(ax, bx))
    min_y = max(0.0, min(ay, by))
    max_y = min(surface.tile_h, max(ay, by))
    if min_x > max_x or min_y > max_y:
        return None

    ix0 = max(0, int(min_x / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int(max_x / surface.cell_w) + 1)
    iy0 = max(0, int(min_y / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int(max_y / surface.cell_w) + 1)

    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    left = cols * surface.cell_w
    right = (cols + 1) * surface.cell_w
    bottom = rows * surface.cell_w
    top = (rows + 1) * surface.cell_w

    eps = 1e-9
    mask = _segment_intersects_cells(ax, ay, bx, by, left, right, bottom, top, eps)
    center_x = ((cols + 0.5) * surface.cell_w)[None, :]
    center_y = ((rows + 0.5) * surface.cell_w)[:, None]
    edge_len = max(2.0 * hw, eps)
    lateral_frac = np.clip(((center_x - ax) * (-px) + (center_y - ay) * (-py)) / edge_len, 0.0, 1.0)
    along_norm = np.ones_like(lateral_frac)

    return ix0, ix1, iy0, iy1, mask, along_norm, lateral_frac


def _segment_intersects_cells(
    ax: float,
    ay: float,
    bx: float,
    by: float,
    left: np.ndarray,
    right: np.ndarray,
    bottom: np.ndarray,
    top: np.ndarray,
    eps: float,
) -> np.ndarray:
    """Vectorized line-segment/AABB intersection for candidate grid cells."""
    rows = len(bottom)
    cols = len(left)
    t0 = np.zeros((rows, cols), dtype=float)
    t1 = np.ones((rows, cols), dtype=float)
    mask = np.ones((rows, cols), dtype=bool)

    dx = bx - ax
    if abs(dx) <= eps:
        mask &= (ax >= left[None, :] - eps) & (ax <= right[None, :] + eps)
    else:
        tx_a = (left - ax) / dx
        tx_b = (right - ax) / dx
        tx_min = np.minimum(tx_a, tx_b)[None, :]
        tx_max = np.maximum(tx_a, tx_b)[None, :]
        t0 = np.maximum(t0, tx_min)
        t1 = np.minimum(t1, tx_max)

    dy = by - ay
    if abs(dy) <= eps:
        mask &= (ay >= bottom[:, None] - eps) & (ay <= top[:, None] + eps)
    else:
        ty_a = (bottom - ay) / dy
        ty_b = (top - ay) / dy
        ty_min = np.minimum(ty_a, ty_b)[:, None]
        ty_max = np.maximum(ty_a, ty_b)[:, None]
        t0 = np.maximum(t0, ty_min)
        t1 = np.minimum(t1, ty_max)

    return mask & (t0 <= t1 + eps) & (t1 >= -eps) & (t0 <= 1.0 + eps)
