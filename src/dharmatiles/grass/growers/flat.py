"""Flat / peaked / round grass grower — pluggable cross-section."""

from __future__ import annotations

import numpy as np
import trimesh

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
        direction = seed.blade_direction + seed.blade_curl * (len(path.points) - 1)
        tx = cx + seed.blade_segment_length * np.sin(direction)
        ty = cy + seed.blade_segment_length * np.cos(direction)
        hw = seed.blade_width / 2.0

        if not (hw <= tx <= surface.tile_w - hw and hw <= ty <= surface.tile_h - hw):
            path.alive = False
            return False

        ix, iy = _cell_index(surface, tx, ty)
        if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
            path.alive = False
            return False

        floor_z = _sample_footprint_max(
            occ_z,
            scene.support_z,
            path.last_stamp,
            surface,
            tx,
            ty,
            seed.blade_width,
            direction,
            x0=cx,
            y0=cy,
        )
        terrain_z = _sample_grid(scene.terrain_z, surface, tx, ty)
        nz = max(terrain_z, floor_z) + seed.blade_clearance

        if floor_z - terrain_z > cfg.max_stack_height:
            path.alive = False
            return False
        if nz > cz + seed.blade_rise_cap:
            path.alive = False
            return False

        path.points.append((float(tx), float(ty), float(nz)))
        path.last_stamp = _stamp_swept_footprint(
            occ_z,
            surface,
            (cx, cy),
            (tx, ty),
            cz,                     # spine z at previous point
            nz,                     # spine z at new point
            seed.blade_width,
            species.blade_thickness,
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
        taper_start = max(1, int(np.floor((n - 1) * 0.8125)))

        # ── Width taper (full from seed, taper to near-zero at tip) ─────────
        widths = np.full(n, seed.blade_width)
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            widths[taper_start:] = seed.blade_width * np.cos(t * np.pi / 2.0)

        # ── Keel depth taper (full from seed, taper near tip) ─────────────────
        base_keel = species.keel_fraction * seed.blade_width
        keel_depths = np.full(n, base_keel)
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            keel_depths[taper_start:] = base_keel * np.cos(t * np.pi / 2.0)

        # ── Top-profile height taper (full from seed, taper near tip) ─────────
        thicknesses = np.full(n, species.blade_thickness)
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            thicknesses[taper_start:] = species.blade_thickness * np.cos(t * np.pi / 2.0)

        # Both ends are normal rings closed with end-caps.  Ring 0 (seed) is
        # anchored in the terrain via its keel.  The tip ring tapers to ~zero
        # width; _make_ring_verts collapses it to a point naturally.
        return _build_blade_mesh(spine, widths, thicknesses, keel_depths, species.blade_top_facets, surface, close_bottom=True, close_top=True)


# ── Cross-section ring construction ──────────────────────────────────────────


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
      vertex 0         — keel  (centre, z = spine_z - keel_depth)
      vertices 1..n+1  — top profile from left (−w/2) to right (+w/2),
                         heights = thickness × sin(π × i/n) above spine_z.

    For n=1: heights at i=0 and i=1 are both sin(0)=sin(π)=0, so the top
    surface is flat at the equator (spine_z) and thickness has no effect.
    For n=2: three vertices with the centre one at spine_z + thickness.
    For n≥3: sine arc approximating a round cross-section.

    When width < 1e-6 all vertices are placed at *center* (a collapsed ring).
    The tube faces into a collapsed ring are degenerate and get filtered out
    by the area threshold in _build_blade_mesh.

    The lateral direction is perpendicular to the spine tangent in XY.
    Vertical is always world +Z.
    """
    nvr = n_top_facets + 2

    # Collapsed ring — fully-tapered tip (or any near-zero width)
    if width < 1e-6:
        return np.tile(center, (nvr, 1))

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

    Every ring always contributes nvr vertices.  Rings whose width tapers to
    ~zero have all vertices collapsed to the spine point by _make_ring_verts;
    the resulting degenerate faces are filtered by the area threshold below.
    Both ends can be closed with flat end-cap faces.
    """
    n_rings = len(spine)
    nvr = n_top_facets + 2  # verts per ring: keel + (n_top_facets+1) top-profile verts

    # Normalised tangents
    tangents = np.empty_like(spine)
    tangents[:-1] = spine[1:] - spine[:-1]
    tangents[-1] = spine[-1] - spine[-2]
    norms = np.linalg.norm(tangents, axis=1, keepdims=True)
    norms = np.where(norms < 1e-9, 1.0, norms)
    tangents /= norms

    # Every ring contributes exactly nvr vertices.  Rings whose width tapers to
    # ~zero have all their vertices collapsed to the spine point by _make_ring_verts;
    # the resulting degenerate faces are filtered by area_faces > 1e-12 below.
    verts_list: list[np.ndarray] = []
    for i in range(n_rings):
        ring = _make_ring_verts(
            spine[i], tangents[i],
            widths[i], thicknesses[i], keel_depths[i],
            n_top_facets,
        )
        verts_list.append(ring)

    all_verts = np.vstack(verts_list)
    np.clip(all_verts[:, 0], 0.0, surface.tile_w, out=all_verts[:, 0])
    np.clip(all_verts[:, 1], 0.0, surface.tile_h, out=all_verts[:, 1])

    faces: list[list[int]] = []
    for i in range(n_rings - 1):
        a = i * nvr
        b = (i + 1) * nvr
        for j in range(nvr):
            j1 = (j + 1) % nvr
            faces.append([a + j,  b + j,  b + j1])
            faces.append([a + j,  b + j1, a + j1])

    # Bottom end-cap: fan from keel vertex (ring 0, vertex 0) across the base.
    if close_bottom:
        a = 0
        for j in range(1, nvr - 1):
            faces.append([a, a + j, a + j + 1])

    # Top end-cap: fan from keel vertex (last ring, vertex 0) across the tip.
    if close_top:
        a = (n_rings - 1) * nvr
        for j in range(1, nvr - 1):
            faces.append([a, a + j + 1, a + j])

    if not faces:
        return None

    mesh = trimesh.Trimesh(vertices=all_verts, faces=np.asarray(faces), process=False)
    # Merge coincident vertices before filtering so that any zero-width ring
    # (whose nvr vertex copies all sit at the same point) collapses to a single
    # shared vertex, making convergence faces properly manifold.
    mesh.merge_vertices()
    mesh.update_faces(mesh.area_faces > 1e-12)
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    return mesh


# ── Spine smoothing ───────────────────────────────────────────────────────────

def _smooth_blade_spine(spine: np.ndarray, blade_smooth: float) -> np.ndarray:
    amount = float(np.clip(blade_smooth, 0.0, 1.0))
    if amount <= 0.0 or len(spine) < 3:
        return spine
    arc = _fit_quadratic_arc(spine)
    smoothed = spine + amount * (arc - spine)
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

def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy


def _sample_grid(grid: np.ndarray, surface, x: float, y: float) -> float:
    i = np.clip(x / surface.cell_w, 0, surface.grid_w - 1)
    j = np.clip(y / surface.cell_w, 0, surface.grid_h - 1)
    i0 = int(np.floor(i)); i1 = min(i0 + 1, surface.grid_w - 1)
    j0 = int(np.floor(j)); j1 = min(j0 + 1, surface.grid_h - 1)
    fi = i - i0; fj = j - j0
    return float(
        grid[j0, i0] * (1 - fi) * (1 - fj)
        + grid[j0, i1] * fi * (1 - fj)
        + grid[j1, i0] * (1 - fi) * fj
        + grid[j1, i1] * fi * fj
    )


def _sample_footprint_max(
    occ_z: np.ndarray,
    base_z: np.ndarray,
    last_stamp: dict[tuple[int, int], float] | None,
    surface,
    x: float,
    y: float,
    width: float,
    direction: float,
    x0: float | None = None,
    y0: float | None = None,
) -> float:
    """Sample the maximum occ_z over the swept footprint of the next step.

    The footprint is the swept rectangle from (x0, y0) to (x, y) — identical
    in shape to the stamp written by ``_stamp_swept_footprint``.  Cells from
    any previous step that are recorded in ``last_stamp`` are replaced with
    ``base_z`` so the blade does not climb over its own immediately-previous
    segment.

    If ``x0``/``y0`` are omitted the old ±hw-square fallback is used (for
    call-sites that have not yet been updated).
    """
    hw = width / 2.0

    # ── Swept-rectangle footprint (same geometry as _stamp_swept_footprint) ──
    if x0 is not None and y0 is not None:
        dx = x - x0
        dy = y - y0
        segment_length = float(np.hypot(dx, dy))
        if segment_length < 1e-9:
            iy, ix = _cell_index(surface, x, y)
            return _own_blind_cell_z(occ_z, base_z, last_stamp, iy, ix)
        ux, uy = dx / segment_length, dy / segment_length   # along unit vector
        px, py = -uy, ux                                    # lateral unit vector

        min_x = min(x0, x) - hw;  max_x = max(x0, x) + hw
        min_y = min(y0, y) - hw;  max_y = max(y0, y) + hw
        ix0g = max(0, int(min_x / surface.cell_w) - 1)
        ix1g = min(surface.grid_w - 1, int(max_x / surface.cell_w) + 1)
        iy0g = max(0, int(min_y / surface.cell_w) - 1)
        iy1g = min(surface.grid_h - 1, int(max_y / surface.cell_w) + 1)

        cols = np.arange(ix0g, ix1g + 1)
        rows = np.arange(iy0g, iy1g + 1)
        xx = (cols + 0.5) * surface.cell_w
        yy = (rows + 0.5) * surface.cell_w
        X, Y = np.meshgrid(xx, yy)
        rel_x = X - x0;  rel_y = Y - y0
        along   = rel_x * ux + rel_y * uy
        lateral = rel_x * px + rel_y * py
        mask = (
            (along >= 0.0)
            & (along <= segment_length)
            & (np.abs(lateral) <= hw)
        )
        if not np.any(mask):
            iy, ix = _cell_index(surface, x, y)
            return _own_blind_cell_z(occ_z, base_z, last_stamp, iy, ix)

        block = occ_z[iy0g:iy1g + 1, ix0g:ix1g + 1].copy()
        if last_stamp:
            local_rows, local_cols = np.where(mask)
            for lr, lc in zip(local_rows, local_cols):
                iy = iy0g + int(lr)
                ix = ix0g + int(lc)
                own_z = last_stamp.get((iy, ix))
                if own_z is not None and block[lr, lc] <= own_z + 1e-9:
                    block[lr, lc] = base_z[iy, ix]
        return float(block[mask].max())

    # ── Legacy fallback: ±hw square around endpoint ───────────────────────────
    ix0 = max(0, int((x - hw) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((x + hw) / surface.cell_w) + 1)
    iy0 = max(0, int((y - hw) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((y + hw) / surface.cell_w) + 1)

    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    xx = (cols + 0.5) * surface.cell_w
    yy = (rows + 0.5) * surface.cell_w
    X, Y = np.meshgrid(xx, yy)
    perp_x = np.cos(direction)
    perp_y = -np.sin(direction)
    lateral = (X - x) * perp_x + (Y - y) * perp_y
    mask = np.abs(lateral) <= hw
    if not np.any(mask):
        iy, ix = _cell_index(surface, x, y)
        return _own_blind_cell_z(occ_z, base_z, last_stamp, iy, ix)

    block = occ_z[iy0:iy1 + 1, ix0:ix1 + 1].copy()
    if last_stamp:
        local_rows, local_cols = np.where(mask)
        for lr, lc in zip(local_rows, local_cols):
            iy = iy0 + int(lr)
            ix = ix0 + int(lc)
            own_z = last_stamp.get((iy, ix))
            if own_z is not None and block[lr, lc] <= own_z + 1e-9:
                block[lr, lc] = base_z[iy, ix]
    return float(block[mask].max())


def _stamp_swept_footprint(
    occ_z: np.ndarray,
    surface,
    p0: tuple[float, float],
    p1: tuple[float, float],
    z0: float,          # spine z at p0
    z1: float,          # spine z at p1
    width: float,
    thickness: float,   # top-profile peak height (ignored for n_top_facets=1)
    n_top_facets: int,
) -> dict[tuple[int, int], float]:
    """Stamp the swept segment footprint into occ_z.

    The stamp height is profile-aware:
      n=1  → flat at equator (z_spine); thickness is ignored.
      n≥2  → laterally varying: z_spine + thickness×sin(π×x_frac) where
              x_frac is the cell's position across the blade width (0=left, 1=right).
    Height also varies along the segment (linear interpolation of z0..z1).
    The return dict maps (iy, ix) → stamped z for own-trail detection.
    """
    x0, y0 = p0
    x1, y1 = p1
    hw = width / 2.0
    dx = x1 - x0
    dy = y1 - y0
    segment_length = float(np.hypot(dx, dy))
    if segment_length < 1e-9:
        ux, uy = 0.0, 1.0
    else:
        ux, uy = dx / segment_length, dy / segment_length
    px, py = -uy, ux

    min_x = max(0.0, min(x0, x1) - hw)
    max_x = min(surface.tile_w, max(x0, x1) + hw)
    min_y = max(0.0, min(y0, y1) - hw)
    max_y = min(surface.tile_h, max(y0, y1) + hw)
    ix0 = max(0, int(min_x / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int(max_x / surface.cell_w) + 1)
    iy0 = max(0, int(min_y / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int(max_y / surface.cell_w) + 1)

    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    xx = (cols + 0.5) * surface.cell_w
    yy = (rows + 0.5) * surface.cell_w
    X, Y = np.meshgrid(xx, yy)
    rel_x = X - x0
    rel_y = Y - y0
    along   = rel_x * ux + rel_y * uy
    lateral = rel_x * px + rel_y * py
    mask = (
        (along >= -surface.cell_w * 0.5)
        & (along <= segment_length + surface.cell_w * 0.5)
        & (np.abs(lateral) <= hw)
    )

    along_norm = np.clip(along / max(segment_length, 1e-9), 0.0, 1.0)
    z_spine = z0 + (z1 - z0) * along_norm          # slope along segment

    if n_top_facets == 1:
        z_field = z_spine                           # flat: top IS the equator
    else:
        x_frac = np.clip((lateral + hw) / max(width, 1e-9), 0.0, 1.0)
        z_field = z_spine + thickness * np.sin(np.pi * x_frac)

    block = occ_z[iy0:iy1 + 1, ix0:ix1 + 1]
    np.maximum(block, np.where(mask, z_field, block), out=block)
    local_rows, local_cols = np.where(mask)
    return {
        (iy0 + int(lr), ix0 + int(lc)): float(block[lr, lc])
        for lr, lc in zip(local_rows, local_cols)
    }


def _own_blind_cell_z(
    occ_z: np.ndarray,
    base_z: np.ndarray,
    last_stamp: dict[tuple[int, int], float] | None,
    iy: int,
    ix: int,
) -> float:
    own_z = last_stamp.get((iy, ix)) if last_stamp else None
    if own_z is not None and occ_z[iy, ix] <= own_z + 1e-9:
        return float(base_z[iy, ix])
    return float(occ_z[iy, ix])
