"""
Region mask generation: boundary rasterisation + flood fill.

The region mask is a (grid_h, grid_w) int32 array:
  -2  = unassigned (no region flood-fill reached this cell)
  -1  = boundary line (impassable to flood fill)
   N  = region index N (0-based, matching tile.regions order)
"""
from __future__ import annotations

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


def _boundary_control_points_mm(spec: Boundary, surface: SurfaceConfig) -> np.ndarray:
    """Return boundary anchors and waypoints as an (N, 2) mm array."""
    points: list[tuple[float, float]] = [
        _anchor_to_mm(spec.from_anchor, surface.tile_w, surface.tile_h)
    ]
    points.extend(
        (float(x) * surface.tile_w, float(y) * surface.tile_h)
        for x, y in spec.waypoints
    )
    points.append(_anchor_to_mm(spec.to_anchor, surface.tile_w, surface.tile_h))
    return np.asarray(points, dtype=float)


# ── Boundary path generation ─────────────────────────────────────────────────

def boundary_path_mm(spec: Boundary, surface: SurfaceConfig,
                     n_samples: int = 4000) -> np.ndarray:
    """Return (n_samples, 2) float array of (x, y) path points in mm.

    The path starts at ``spec.from_anchor``, follows any normalised
    ``spec.waypoints``, and ends at ``spec.to_anchor``.  For 'organic' paths
    the noise tapers to zero at every control point, so the curve hits both
    anchors and all waypoints exactly.
    """
    controls = _boundary_control_points_mm(spec, surface)
    seg_vecs = controls[1:] - controls[:-1]
    seg_lens = np.hypot(seg_vecs[:, 0], seg_vecs[:, 1])
    keep = seg_lens > 1e-9
    if not np.any(keep):
        x, y = controls[0]
        return np.column_stack([np.full(n_samples, x), np.full(n_samples, y)])

    seg_vecs = seg_vecs[keep]
    seg_lens = seg_lens[keep]
    starts = controls[:-1][keep]

    total_len = float(np.sum(seg_lens))
    dist = np.linspace(0.0, total_len, n_samples)
    cum = np.concatenate([[0.0], np.cumsum(seg_lens)])
    seg_idx = np.searchsorted(cum[1:], dist, side='right')
    seg_idx = np.minimum(seg_idx, len(seg_lens) - 1)
    seg_start_dist = cum[seg_idx]
    local_t = (dist - seg_start_dist) / seg_lens[seg_idx]

    base = starts[seg_idx] + seg_vecs[seg_idx] * local_t[:, None]

    if spec.path == 'straight':
        return base
    if spec.path != 'organic':
        raise ValueError(
            f"Boundary '{spec.id}': unknown path {spec.path!r}; "
            "expected 'organic' or 'straight'."
        )

    # ── Organic: smooth stochastic perpendicular offsets ─────────────────────
    # Perpendicular unit vector (rotate each active segment 90° CCW)
    tangents = seg_vecs[seg_idx] / seg_lens[seg_idx, None]
    perp = np.column_stack([-tangents[:, 1], tangents[:, 0]])

    rng    = np.random.default_rng(surface.seed ^ spec.seed_offset ^ 0xB04DA7)
    corr_mm = max(spec.wavelength_mm, surface.cell_w)
    n_knots = max(5, int(np.ceil(total_len / corr_mm)) + 3)
    knot_t  = np.linspace(0.0, 1.0, n_knots)
    path_t  = dist / total_len

    # Random low-frequency control offsets, pinned by a per-segment taper at
    # every anchor/waypoint.  This avoids periodic shoreline bumps while keeping
    # the curve deterministic per seed.
    knot_offsets = rng.normal(0.0, 0.55 * spec.amplitude_mm, n_knots)
    knot_offsets[0] = 0.0
    knot_offsets[-1] = 0.0

    from scipy.interpolate import CubicSpline

    offset = CubicSpline(knot_t, knot_offsets, bc_type='natural')(path_t)
    taper  = np.sin(np.pi * local_t) ** 0.75  # pinned at every control point
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
        d_n_knots  = max(5, int(np.ceil(total_len / detail_corr_mm)) + 3)
        d_knot_t   = np.linspace(0.0, 1.0, d_n_knots)
        d_offsets  = rng.normal(0.0, 0.55 * detail_amp, d_n_knots)
        d_offsets[0]  = 0.0
        d_offsets[-1] = 0.0
        detail = CubicSpline(d_knot_t, d_offsets, bc_type='natural')(path_t)
        detail  = detail * taper
        d_max   = float(np.max(np.abs(detail)))
        if d_max > 1e-9:
            detail *= detail_amp / d_max
        offset = offset + detail

    return base + offset[:, None] * perp


# ── Rasterisation ─────────────────────────────────────────────────────────────

def _rasterise(path_mm: np.ndarray, surface: SurfaceConfig,
               mask: np.ndarray) -> None:
    """Mark cells along path_mm as _BOUNDARY (in-place).

    With the default 4 000 samples per boundary path, consecutive sample
    pairs are ≤ 0.1 cells apart on any realistic tile, so no gap-filling is
    needed.  A single vectorised index write replaces the old per-segment
    Bresenham loop.
    """
    cols = np.clip((path_mm[:, 0] / surface.cell_w).astype(int),
                   0, surface.grid_w - 1)
    rows = np.clip((path_mm[:, 1] / surface.cell_w).astype(int),
                   0, surface.grid_h - 1)
    mask[rows, cols] = _BOUNDARY


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


# ── Public API ────────────────────────────────────────────────────────────────

def build_region_mask(tile: Tile) -> np.ndarray:
    """Return (grid_h, grid_w) int32 array: region index per cell.

    Values:
      _UNASSIGNED (-2)  — no region claimed this cell
      _BOUNDARY   (-1)  — lies on a boundary curve
      N ≥ 0             — belongs to tile.regions[N]
    """
    import warnings
    from scipy.ndimage import label as _scipy_label

    surface = tile.surface
    mask = np.full((surface.grid_h, surface.grid_w), _UNASSIGNED, dtype=np.int32)

    for bnd in tile.boundaries:
        _rasterise_boundary(bnd, surface, mask)

    # Label all connected unassigned regions at once with scipy (C-speed).
    # Each seed selects the connected component it falls in; first seed wins
    # if two regions share a component.
    labeled_arr, _ = _scipy_label(mask == _UNASSIGNED)
    used_labels: set[int] = set()

    for idx, region in enumerate(tile.regions):
        for cx_n, cy_n in region.selector.seeds:
            col = int(np.clip(cx_n * surface.grid_w, 0, surface.grid_w - 1))
            row = int(np.clip(cy_n * surface.grid_h, 0, surface.grid_h - 1))
            seed_label = int(labeled_arr[row, col])
            if seed_label == 0 or seed_label in used_labels:
                warnings.warn(
                    f"Region '{region.id}': seed ({cx_n:.2f}, {cy_n:.2f}) "
                    f"landed on a boundary or another region — check your tile spec.",
                    stacklevel=2,
                )
            else:
                mask[labeled_arr == seed_label] = idx
                used_labels.add(seed_label)

    return mask
