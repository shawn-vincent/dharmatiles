"""Flat / peaked / round grass grower — pluggable cross-section."""

from __future__ import annotations

import numpy as np
import trimesh

from ..config import SpeciesConfig
from ..seed import GrassPath, GrowingPath


class FlatGrassGrower:
    """Grow and mesh floppy grass blades with a configurable cross-section.

    Cross-section is controlled by ``species.n_top_facets``:
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
        direction = seed.direction + seed.curl * (len(path.points) - 1)
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
        )
        terrain_z = _sample_grid(scene.terrain_z, surface, tx, ty)
        nz = max(terrain_z, floor_z) + cfg.clearance

        if floor_z - terrain_z > cfg.max_stack_height:
            path.alive = False
            return False
        if nz > cz + seed.rise_cap:
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
            species.thickness,
            species.n_top_facets,
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

        terrain_root_z = _sample_grid(scene.terrain_z, surface, spine[0, 0], spine[0, 1])

        # For n=1 (flat) thickness is zero-effect above the equator; the root
        # anchor sits at terrain level.  For n≥2 the anchor sits below terrain
        # so the blade peak emerges exactly at the terrain surface.
        effective_top = 0.0 if species.n_top_facets == 1 else species.thickness
        root_z = terrain_root_z - effective_top

        # Prepend the collapsed root anchor (width=0 → single point below terrain)
        spine = np.vstack([[[spine[0, 0], spine[0, 1], root_z]], spine])

        n = len(spine)
        taper_start = max(1, int(np.floor((n - 1) * 0.8125)))

        # ── Width taper ────────────────────────────────────────────────────────
        widths = np.zeros(n)          # root (index 0) collapses to a point
        widths[1:] = seed.blade_width
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            widths[taper_start:] = seed.blade_width * np.cos(t * np.pi / 2.0)
        widths[-1] = 0.0  # collapse tip → convergence pyramid, no degenerate ring

        # ── Keel depth taper (0 at root, full through body, 0 at tip) ─────────
        base_keel = species.keel_fraction * seed.blade_width
        keel_depths = np.zeros(n)
        keel_depths[1:taper_start] = base_keel
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            keel_depths[taper_start:] = base_keel * np.cos(t * np.pi / 2.0)

        # ── Top-profile height taper (0 at root, full through body, 0 at tip) ─
        thicknesses = np.zeros(n)
        thicknesses[1:taper_start] = species.thickness
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            thicknesses[taper_start:] = species.thickness * np.cos(t * np.pi / 2.0)

        # Pin first grown ring so the blade emerges cleanly from the terrain
        spine[1, 2] = min(spine[1, 2], terrain_root_z - effective_top)

        return _build_blade_mesh(spine, widths, thicknesses, keel_depths, species.n_top_facets, surface)


# ── Cross-section ring construction ──────────────────────────────────────────

_RING_COLLAPSE_EPSILON = 1e-6   # mm — rings narrower than this collapse to a point


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

    When width < _RING_COLLAPSE_EPSILON all vertices are placed at *center*
    (a collapsed ring).  The tube between a collapsed ring and its neighbour
    forms a convergence pyramid, cleanly capping root and tip.

    The lateral direction is perpendicular to the spine tangent in XY.
    Vertical is always world +Z.
    """
    nvr = n_top_facets + 2

    # Collapsed ring — root anchor or fully-tapered tip
    if width < _RING_COLLAPSE_EPSILON:
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
    spine: np.ndarray,          # (n_rings, 3)
    widths: np.ndarray,         # (n_rings,) — 0 at root and tip (apex)
    thicknesses: np.ndarray,    # (n_rings,) — profile heights
    keel_depths: np.ndarray,    # (n_rings,)
    n_top_facets: int,
    surface,
) -> trimesh.Trimesh | None:
    """Build a closed tube mesh along the spine with the given cross-section.

    Rings with width < _RING_COLLAPSE_EPSILON are treated as apex rings and
    contribute a single vertex (not nvr).  Adjacent apex→normal or normal→apex
    transitions become convergence fans that close the mesh without requiring
    merge_vertices, keeping the mesh manifold by construction.
    """
    n_rings = len(spine)
    nvr = n_top_facets + 2  # verts per normal ring: keel + (n+1 top-profile verts)

    is_apex = widths < _RING_COLLAPSE_EPSILON

    # Normalised tangents
    tangents = np.empty_like(spine)
    tangents[:-1] = spine[1:] - spine[:-1]
    tangents[-1] = spine[-1] - spine[-2]
    norms = np.linalg.norm(tangents, axis=1, keepdims=True)
    norms = np.where(norms < 1e-9, 1.0, norms)
    tangents /= norms

    # Build vertices: apex rings → 1 vertex; normal rings → nvr vertices.
    ring_offsets = np.empty(n_rings, dtype=int)
    verts_list: list[np.ndarray] = []
    offset = 0
    for i in range(n_rings):
        ring_offsets[i] = offset
        if is_apex[i]:
            verts_list.append(spine[i : i + 1].copy())
            offset += 1
        else:
            ring = _make_ring_verts(
                spine[i], tangents[i],
                widths[i], thicknesses[i], keel_depths[i],
                n_top_facets,
            )
            verts_list.append(ring)
            offset += nvr

    all_verts = np.vstack(verts_list)
    np.clip(all_verts[:, 0], 0.0, surface.tile_w, out=all_verts[:, 0])
    np.clip(all_verts[:, 1], 0.0, surface.tile_h, out=all_verts[:, 1])

    faces: list[list[int]] = []
    for i in range(n_rings - 1):
        a = int(ring_offsets[i])
        b = int(ring_offsets[i + 1])
        a_apex = bool(is_apex[i])
        b_apex = bool(is_apex[i + 1])

        if a_apex and b_apex:
            pass  # two adjacent apices — no surface to build

        elif a_apex:
            # Root convergence: fan from single apex vertex to next ring
            for j in range(nvr):
                j1 = (j + 1) % nvr
                faces.append([a, b + j, b + j1])

        elif b_apex:
            # Tip convergence: fan from ring to single apex vertex
            for j in range(nvr):
                j1 = (j + 1) % nvr
                faces.append([a + j, b, a + j1])

        else:
            # Normal tube quad — two triangles per cross-section edge
            for j in range(nvr):
                j1 = (j + 1) % nvr
                faces.append([a + j,  b + j,  b + j1])
                faces.append([a + j,  b + j1, a + j1])

    if not faces:
        return None

    mesh = trimesh.Trimesh(vertices=all_verts, faces=np.asarray(faces), process=False)
    mesh.update_faces(mesh.area_faces > 1e-12)
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    return mesh


# ── Spine smoothing ───────────────────────────────────────────────────────────

def _smooth_blade_spine(spine: np.ndarray, blade_smooth: float) -> np.ndarray:
    """Blend the grown XY path toward a best-fit quadratic arc.

    Only the X and Y coordinates are smoothed.  Z is left exactly as
    computed by the growth + mesh-lift phases — smoothing Z would amplify
    brief obstacle crossings (e.g. brushing another blade's seed stamp)
    into a full bow-shaped arch visible in side view.
    """
    amount = float(np.clip(blade_smooth, 0.0, 1.0))
    if amount <= 0.0 or len(spine) < 3:
        return spine
    arc = _fit_quadratic_arc(spine)
    smoothed = spine.copy()
    smoothed[:, :2] += amount * (arc[:, :2] - spine[:, :2])   # XY only
    smoothed[0] = spine[0]   # pin base
    smoothed[-1] = spine[-1] # pin tip
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
) -> float:
    hw = width / 2.0
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
        (iy0 + int(lr), ix0 + int(lc)): float(z_field[lr, lc])
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
