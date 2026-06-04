"""
Grid helpers: bilinear sampling and rasterisation onto the support heightfield.
"""
from __future__ import annotations

import numpy as np

from .config import SurfaceConfig


def sample_grid(grid: np.ndarray, surface: SurfaceConfig, x_mm, y_mm):
    """Bilinear sample of *grid* at world coordinates — accepts scalars or arrays.

    Parameters
    ----------
    grid    : (grid_h, grid_w) float array  — indexed [row=j, col=i]
    surface : SurfaceConfig — provides cell dimensions and grid shape.
    x_mm, y_mm : scalar or array — world X / Y positions in mm.
    """
    grid_w = surface.grid_w
    grid_h = surface.grid_h
    cw     = surface.cell_w

    scalar = np.ndim(x_mm) == 0
    i  = np.clip(np.asarray(x_mm, dtype=float) / cw, 0, grid_w - 1)
    j  = np.clip(np.asarray(y_mm, dtype=float) / cw, 0, grid_h - 1)
    i0 = np.floor(i).astype(int);  i1 = np.minimum(i0 + 1, grid_w - 1)
    j0 = np.floor(j).astype(int);  j1 = np.minimum(j0 + 1, grid_h - 1)
    fi = i - i0;  fj = j - j0
    result = (grid[j0, i0] * (1 - fi) * (1 - fj) +
              grid[j0, i1] *      fi  * (1 - fj) +
              grid[j1, i0] * (1 - fi) *      fj  +
              grid[j1, i1] *      fi  *      fj)
    return float(result) if scalar else result


def rasterise_into_support(support_z: np.ndarray, surface: SurfaceConfig,
                            path_xyz, half_widths) -> None:
    """Paint the blade's top surface into *support_z* (in-place max).

    Walks each spine segment at sub-cell resolution, stamps a disk of radius
    *half_width* at each sample, and raises ``support_z`` to the blade's z.

    Implementation: all samples along the full spine are computed as a single
    batch of numpy arrays; disk stamps are issued per unique integer radius
    using pre-built offset templates, avoiding the per-sample meshgrid overhead.

    Note
    ----
    The fancy-index read-modify-write pattern ``support_z[jj, ii] = np.maximum(...)``
    is intentional — fancy indexing produces a *copy*, so we must assign back
    explicitly.  Do NOT replace with ``np.maximum(..., out=support_z[jj, ii])``.
    """
    path = np.asarray(path_xyz, dtype=float)   # (n_pts, 3)
    hws  = np.asarray(half_widths, dtype=float) # (n_pts,)

    grid_w = surface.grid_w
    grid_h = surface.grid_h
    cw     = surface.cell_w

    # ── Generate all sub-cell sample points in one vectorised pass ────────────
    half_cell = 1.5 * cw
    n_pts     = len(path)

    # Segment lengths (XY only)
    seg_vecs   = path[1:, :2] - path[:-1, :2]          # (n_pts-1, 2)
    seg_lens   = np.linalg.norm(seg_vecs, axis=1)       # (n_pts-1,)
    n_steps_v  = np.maximum(1, np.ceil(seg_lens / half_cell).astype(int))  # (n_pts-1,)

    total_samples = int(n_steps_v.sum()) + 1            # +1 for final point

    xs  = np.empty(total_samples, dtype=float)
    ys  = np.empty(total_samples, dtype=float)
    zs  = np.empty(total_samples, dtype=float)
    hws_out = np.empty(total_samples, dtype=float)

    out_idx = 0
    for idx in range(n_pts - 1):
        n  = int(n_steps_v[idx])
        a  = np.arange(n, dtype=float) / n             # [0, 1)
        b  = 1.0 - a
        xs[out_idx:out_idx + n]  = b * path[idx, 0] + a * path[idx + 1, 0]
        ys[out_idx:out_idx + n]  = b * path[idx, 1] + a * path[idx + 1, 1]
        zs[out_idx:out_idx + n]  = b * path[idx, 2] + a * path[idx + 1, 2]
        hws_out[out_idx:out_idx + n] = b * hws[idx] + a * hws[idx + 1]
        out_idx += n

    xs[out_idx]      = path[-1, 0]
    ys[out_idx]      = path[-1, 1]
    zs[out_idx]      = path[-1, 2]
    hws_out[out_idx] = hws[-1]
    out_idx += 1
    n_samp = out_idx

    xs  = xs[:n_samp];  ys  = ys[:n_samp]
    zs  = zs[:n_samp];  hws_out = hws_out[:n_samp]

    # Integer grid centres and radii
    ic_arr     = np.clip((xs / cw).astype(int), 0, grid_w - 1)
    jc_arr     = np.clip((ys / cw).astype(int), 0, grid_h - 1)
    r_cells_arr = np.maximum(1, (hws_out / cw).astype(int) + 2)

    # ── Stamp per unique radius (small set: typically 1–3 distinct values) ────
    for r in np.unique(r_cells_arr):
        # Build disk template
        di_1d  = np.arange(-r, r + 1)
        DI, DJ = np.meshgrid(di_1d, di_1d, indexing='ij')
        in_disk = (DI * cw) ** 2 + (DJ * cw) ** 2 <= (r * cw) ** 2
        di_t   = DI[in_disk]                # (n_disk_cells,)
        dj_t   = DJ[in_disk]

        # Select samples with this radius
        sel    = (r_cells_arr == r)
        ic_sel = ic_arr[sel]
        jc_sel = jc_arr[sel]
        z_sel  = zs[sel]

        # For each sample, compute all affected (jj, ii) and the z to stamp.
        # Shape: (n_sel, n_disk) for indices; broadcast to flat list.
        ii_all = ic_sel[:, None] + di_t[None, :]   # (n_sel, n_disk)
        jj_all = jc_sel[:, None] + dj_t[None, :]
        z_all  = np.broadcast_to(z_sel[:, None], ii_all.shape)

        # Clip to grid bounds
        valid  = ((ii_all >= 0) & (ii_all < grid_w) &
                  (jj_all >= 0) & (jj_all < grid_h))

        ii_v   = ii_all[valid]
        jj_v   = jj_all[valid]
        z_v    = z_all[valid]

        # Apply max update: np.maximum.at handles repeated indices correctly
        np.maximum.at(support_z, (jj_v, ii_v), z_v)
