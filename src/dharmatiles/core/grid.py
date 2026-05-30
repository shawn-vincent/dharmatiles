"""
Grid helpers: bilinear sampling and rasterisation onto the support heightfield.
"""
from __future__ import annotations

import numpy as np

from .tile import TileConfig


def sample_grid(grid: np.ndarray, cfg: TileConfig, x_mm, y_mm):
    """Bilinear sample of *grid* at world coordinates — accepts scalars or arrays.

    Parameters
    ----------
    grid : (GRID_RES, GRID_RES) float array  — indexed [row=j, col=i]
    x_mm, y_mm : scalar or array — world X / Y positions in mm
    """
    scalar = np.ndim(x_mm) == 0
    i  = np.clip(np.asarray(x_mm, dtype=float) / cfg.gx, 0, cfg.grid_res - 1)
    j  = np.clip(np.asarray(y_mm, dtype=float) / cfg.gy, 0, cfg.grid_res - 1)
    i0 = np.floor(i).astype(int);  i1 = np.minimum(i0 + 1, cfg.grid_res - 1)
    j0 = np.floor(j).astype(int);  j1 = np.minimum(j0 + 1, cfg.grid_res - 1)
    fi = i - i0;  fj = j - j0
    result = (grid[j0, i0] * (1 - fi) * (1 - fj) +
              grid[j0, i1] *      fi  * (1 - fj) +
              grid[j1, i0] * (1 - fi) *      fj  +
              grid[j1, i1] *      fi  *      fj)
    return float(result) if scalar else result


def rasterise_into_support(support_z: np.ndarray, cfg: TileConfig,
                            path_xyz, half_widths) -> None:
    """Paint the blade's top surface into *support_z* (in-place max).

    Walks each spine segment at sub-cell resolution, stamps a disk of radius
    *half_width* at each sample, and raises ``support_z`` to the blade's z.

    Note
    ----
    The fancy-index read-modify-write pattern ``support_z[jj, ii] = np.maximum(...)``
    is intentional — fancy indexing produces a *copy*, so we must assign back
    explicitly.  Do NOT replace with ``np.maximum(..., out=support_z[jj, ii])``.
    """
    path = np.asarray(path_xyz)   # (n_pts, 3)
    hws  = np.asarray(half_widths)  # (n_pts,)

    samples: list = []
    half_cell = 0.5 * min(cfg.gx, cfg.gy)

    for idx in range(len(path) - 1):
        p0, p1   = path[idx], path[idx + 1]
        hw0, hw1 = float(hws[idx]), float(hws[idx + 1])
        seg_len  = float(np.linalg.norm(p1[:2] - p0[:2]))
        n_steps  = max(1, int(np.ceil(seg_len / half_cell)))
        for step in range(n_steps):
            a  = step / n_steps
            p  = (1.0 - a) * p0 + a * p1
            hw = (1.0 - a) * hw0 + a * hw1
            samples.append((float(p[0]), float(p[1]), float(p[2]), float(hw)))

    # Always include the final point
    samples.append((float(path[-1, 0]), float(path[-1, 1]),
                    float(path[-1, 2]), float(hws[-1])))

    for x, y, z, hw in samples:
        r_cells = max(1, int(hw / cfg.gx) + 2)
        ic = int(np.clip(x / cfg.gx, 0, cfg.grid_res - 1))
        jc = int(np.clip(y / cfg.gy, 0, cfg.grid_res - 1))

        lo_i = max(0, ic - r_cells);  hi_i = min(cfg.grid_res - 1, ic + r_cells)
        lo_j = max(0, jc - r_cells);  hi_j = min(cfg.grid_res - 1, jc + r_cells)

        di = np.arange(lo_i - ic, hi_i - ic + 1)
        dj = np.arange(lo_j - jc, hi_j - jc + 1)
        DI, DJ = np.meshgrid(di, dj, indexing='ij')
        mask = (DI * cfg.gx) ** 2 + (DJ * cfg.gy) ** 2 <= hw * hw

        ii = ic + DI[mask]
        jj = jc + DJ[mask]
        support_z[jj, ii] = np.maximum(support_z[jj, ii], z)
