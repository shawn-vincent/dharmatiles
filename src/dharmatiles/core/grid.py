"""
Grid helpers: bilinear sampling of the terrain heightfield.
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
