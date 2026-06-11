"""
Region mask generation: boundary rasterisation + flood fill.

The region mask is a (grid_h, grid_w) int32 array:
  -2  = unassigned (no region flood-fill reached this cell)
  -1  = boundary line (impassable to flood fill)
   N  = region index N (0-based, matching tile.regions order)
"""
from __future__ import annotations

from collections import deque

import numpy as np

from .config import SurfaceConfig
from ..spec import Tile, Boundary, Anchor


# Sentinel values in the region mask
_UNASSIGNED = -2
_BOUNDARY   = -1


# ── Anchor → mm ──────────────────────────────────────────────────────────────

def _anchor_to_mm(anchor: Anchor | tuple,
                  tile_w: float, tile_h: float) -> tuple[float, float]:
    """Convert a perimeter anchor to (x_mm, y_mm).

    Accepts an :class:`~dharmatiles.spec.Anchor` (from :class:`~dharmatiles.spec.Edge`
    factory methods) or a legacy ``('edge', t)`` tuple.
    """
    if isinstance(anchor, tuple):
        edge, t = anchor
    else:
        edge, t = anchor.edge, anchor.t
    if edge == 'bottom': return (t * tile_w, 0.0)
    if edge == 'top':    return (t * tile_w, tile_h)
    if edge == 'left':   return (0.0, t * tile_h)
    if edge == 'right':  return (tile_w, t * tile_h)
    raise ValueError(f"Unknown edge {edge!r}; must be top/bottom/left/right")


# ── Boundary path generation ─────────────────────────────────────────────────

def boundary_path_mm(spec: Boundary, surface: SurfaceConfig,
                     n_samples: int = 4000) -> np.ndarray:
    """Return (n_samples, 2) float array of (x, y) path points in mm.

    The path starts at ``spec.from_anchor`` and ends at ``spec.to_anchor``.
    For 'organic' paths the noise tapers to zero at both ends so the curve
    hits the anchor points exactly.
    """
    xa, ya = _anchor_to_mm(spec.from_anchor, surface.tile_w, surface.tile_h)
    xb, yb = _anchor_to_mm(spec.to_anchor,   surface.tile_w, surface.tile_h)

    t  = np.linspace(0.0, 1.0, n_samples)
    dx = xb - xa
    dy = yb - ya

    if spec.path == 'straight':
        return np.column_stack([xa + t * dx, ya + t * dy])

    # ── Organic: smooth stochastic perpendicular offsets ─────────────────────
    length = float(np.hypot(dx, dy))
    if length < 1e-6:
        return np.column_stack([np.full(n_samples, xa),
                                np.full(n_samples, ya)])

    # Perpendicular unit vector (rotate 90° CCW)
    px, py = -dy / length, dx / length

    rng    = np.random.default_rng(surface.seed ^ spec.seed_offset ^ 0xB04DA7)
    corr_mm = max(spec.wavelength_mm, surface.cell_w)
    n_knots = max(5, int(np.ceil(length / corr_mm)) + 3)
    knot_t  = np.linspace(0.0, 1.0, n_knots)

    # Random low-frequency control offsets, pinned at both ends.  This avoids
    # periodic shoreline bumps while keeping the curve deterministic per seed.
    knot_offsets = rng.normal(0.0, 0.55 * spec.amplitude_mm, n_knots)
    knot_offsets[0] = 0.0
    knot_offsets[-1] = 0.0

    from scipy.interpolate import CubicSpline

    offset = CubicSpline(knot_t, knot_offsets, bc_type='natural')(t)
    taper  = np.sin(np.pi * t) ** 0.75   # pinned at anchors, relaxed in middle
    offset = offset * taper

    max_abs = float(np.max(np.abs(offset)))
    if max_abs > 1e-9 and spec.amplitude_mm > 0.0:
        offset *= spec.amplitude_mm / max_abs

    # ── Detail layer: ~4× frequency, detail_fraction × amplitude ─────────────
    # Adds fine-grained noise on top of the base low-frequency curve so the
    # boundary reads as organic rather than a single smooth wave.  The detail
    # knots are independently seeded (same rng stream, next draw) so they are
    # deterministic but uncorrelated with the base layer.
    if spec.detail_fraction > 0.0 and spec.amplitude_mm > 0.0:
        detail_amp = spec.amplitude_mm * spec.detail_fraction
        detail_corr_mm = max(spec.wavelength_mm / 4.0, surface.cell_w)
        d_n_knots  = max(5, int(np.ceil(length / detail_corr_mm)) + 3)
        d_knot_t   = np.linspace(0.0, 1.0, d_n_knots)
        d_offsets  = rng.normal(0.0, 0.55 * detail_amp, d_n_knots)
        d_offsets[0]  = 0.0
        d_offsets[-1] = 0.0
        detail = CubicSpline(d_knot_t, d_offsets, bc_type='natural')(t)
        detail  = detail * taper
        d_max   = float(np.max(np.abs(detail)))
        if d_max > 1e-9:
            detail *= detail_amp / d_max
        offset = offset + detail

    return np.column_stack([xa + t * dx + offset * px,
                            ya + t * dy + offset * py])


# ── Rasterisation ─────────────────────────────────────────────────────────────

def _rasterise(path_mm: np.ndarray, surface: SurfaceConfig,
               mask: np.ndarray) -> None:
    """Mark cells along path_mm as _BOUNDARY (in-place).

    Uses Bresenham-style segment stepping between consecutive sample pairs
    so there are no gaps even for coarser grids.
    """
    cols = np.clip((path_mm[:, 0] / surface.cell_w).astype(int),
                   0, surface.grid_w - 1)
    rows = np.clip((path_mm[:, 1] / surface.cell_w).astype(int),
                   0, surface.grid_h - 1)

    # Walk consecutive segments and fill any skipped cells
    for i in range(len(cols) - 1):
        c0, r0 = int(cols[i]),     int(rows[i])
        c1, r1 = int(cols[i + 1]), int(rows[i + 1])
        _bresenham(mask, r0, c0, r1, c1)


def _rasterise_boundary(bnd: Boundary, surface: SurfaceConfig,
                        mask: np.ndarray) -> None:
    """Mark a boundary centreline or finite-width strip as _BOUNDARY."""
    path = boundary_path_mm(bnd, surface)
    if bnd.width_mm <= 0.0:
        _rasterise(path, surface, mask)
        return

    line_mask = np.zeros_like(mask, dtype=bool)
    _rasterise(path, surface, line_mask)

    from scipy.ndimage import distance_transform_edt

    dist_cells = distance_transform_edt(~line_mask)
    # Boundary width is physical strip width, centred on the path.
    half_width_cells = max(0.5, bnd.width_mm / (2.0 * surface.cell_w))
    mask[dist_cells <= half_width_cells] = _BOUNDARY


def _bresenham(mask: np.ndarray, r0: int, c0: int, r1: int, c1: int) -> None:
    """Mark all cells on the Bresenham line from (r0,c0) to (r1,c1)."""
    gh, gw = mask.shape
    dr = abs(r1 - r0);  sr = 1 if r1 > r0 else -1
    dc = abs(c1 - c0);  sc = 1 if c1 > c0 else -1
    err = dr - dc
    r, c = r0, c0
    while True:
        if 0 <= r < gh and 0 <= c < gw:
            mask[r, c] = _BOUNDARY
        if r == r1 and c == c1:
            break
        e2 = 2 * err
        if e2 > -dc:  err -= dc;  r += sr
        if e2 <  dr:  err += dr;  c += sc


# ── Flood fill ────────────────────────────────────────────────────────────────

def _flood_fill(mask: np.ndarray, row: int, col: int, value: int) -> bool:
    """BFS flood fill: paint connected _UNASSIGNED cells with *value*.

    Returns True if at least one cell was filled, False if the seed
    cell was already occupied (boundary or another region).
    """
    gh, gw = mask.shape
    if mask[row, col] != _UNASSIGNED:
        return False
    q = deque([(row, col)])
    mask[row, col] = value
    while q:
        r, c = q.popleft()
        for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            nr, nc = r + dr, c + dc
            if 0 <= nr < gh and 0 <= nc < gw and mask[nr, nc] == _UNASSIGNED:
                mask[nr, nc] = value
                q.append((nr, nc))
    return True


# ── Public API ────────────────────────────────────────────────────────────────

def build_region_mask(tile: Tile) -> np.ndarray:
    """Return (grid_h, grid_w) int32 array: region index per cell.

    Values:
      _UNASSIGNED (-2)  — no region claimed this cell
      _BOUNDARY   (-1)  — lies on a boundary curve
      N ≥ 0             — belongs to tile.regions[N]
    """
    surface = tile.surface
    mask = np.full((surface.grid_h, surface.grid_w), _UNASSIGNED, dtype=np.int32)

    for bnd in tile.boundaries:
        _rasterise_boundary(bnd, surface, mask)

    for idx, region in enumerate(tile.regions):
        import warnings
        for cx_n, cy_n in region.selector.seeds:
            col = int(np.clip(cx_n * surface.grid_w, 0, surface.grid_w - 1))
            row = int(np.clip(cy_n * surface.grid_h, 0, surface.grid_h - 1))
            ok = _flood_fill(mask, row, col, idx)
            if not ok:
                warnings.warn(
                    f"Region '{region.id}': seed ({cx_n:.2f}, {cy_n:.2f}) "
                    f"landed on a boundary or another region — check your tile spec.",
                    stacklevel=2,
                )

    return mask
