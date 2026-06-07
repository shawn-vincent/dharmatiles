"""Mesh construction and support rasterisation for completed grass paths."""

from __future__ import annotations

import numpy as np
import trimesh

from .config import GrassConfig
from .growers import GROWERS
from .seed import GrassPath


def build_meshes(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> list[trimesh.Trimesh]:
    """Build meshes for all paths, interleaving point lifting and support updates.

    For each path (in generation order):

    1. Lift every path point to ``max(planned_z, current support_z at that XY)``.
    2. Build the mesh from the adjusted path.
    3. Update ``scene.support_z`` from the actual mesh contours using slope-aware
       rasterisation so future paths and meshes interact with the most accurate
       available surface.
    """
    species_map = {species.name: species for species in cfg.species}
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        species = species_map[path.seed.species_id]
        grower = GROWERS[species.grower]

        # Step 1: lift every point against the current accumulated surface.
        lifted_points = _lift_path_points(path.points, scene.support_z, surface)
        lifted_path = GrassPath(seed=path.seed, points=lifted_points)

        # Step 2: build mesh from the adjusted path.
        mesh = grower.build_mesh(lifted_path, species, scene, surface)
        if mesh is not None:
            meshes.append(mesh)

        # Step 3: update support_z from actual mesh contours (slope-aware).
        _rasterise_sloped_path(
            scene.support_z,
            surface,
            np.asarray(lifted_points, dtype=float),
            path.seed.blade_width,
            species.thickness,
        )

    return meshes


# ── Point lifting ─────────────────────────────────────────────────────────────

def _lift_path_points(
    points: list[tuple[float, float, float]],
    support_z: np.ndarray,
    surface,
) -> list[tuple[float, float, float]]:
    """Adjust path points against the current support surface.

    Interior points (root through penultimate): raise only — max(planned_z, floor_z).
    Tip (last point): pin exactly to the support surface height at its XY,
    whether that is higher or lower than the planned z.
    """
    lifted = []
    last_idx = len(points) - 1
    for i, (x, y, z) in enumerate(points):
        floor_z = _sample_grid(support_z, surface, x, y)
        if i == last_idx:
            new_z = floor_z          # snap tip to actual surface — up or down
        else:
            new_z = max(z, floor_z)  # non-tip: only lift, never pull down
        lifted.append((float(x), float(y), float(new_z)))
    return lifted


def _sample_grid(grid: np.ndarray, surface, x: float, y: float) -> float:
    """Bilinear sample of *grid* at world coordinates (scalar)."""
    i = np.clip(x / surface.cell_w, 0, surface.grid_w - 1)
    j = np.clip(y / surface.cell_w, 0, surface.grid_h - 1)
    i0 = int(np.floor(i))
    j0 = int(np.floor(j))
    i1 = min(i0 + 1, surface.grid_w - 1)
    j1 = min(j0 + 1, surface.grid_h - 1)
    fi = i - i0
    fj = j - j0
    return float(
        grid[j0, i0] * (1 - fi) * (1 - fj)
        + grid[j0, i1] * fi * (1 - fj)
        + grid[j1, i0] * (1 - fi) * fj
        + grid[j1, i1] * fi * fj
    )


# ── Slope-aware support rasterisation ────────────────────────────────────────

def _rasterise_sloped_path(
    support_z: np.ndarray,
    surface,
    path: np.ndarray,
    width: float,
    thickness: float,
) -> None:
    """Stamp the blade top surface into support_z with per-segment slope interpolation.

    Unlike a flat stamp, each segment's footprint is stamped with z values that
    interpolate linearly between the blade-top heights at its two endpoints.  This
    records the actual slope of the mesh surface so subsequent blades interact with
    a faithful representation of the geometry.
    """
    if len(path) == 0:
        return
    hw = width / 2.0
    _stamp_disk(support_z, surface, path[0, 0], path[0, 1], path[0, 2] + thickness, hw)
    for idx in range(1, len(path)):
        prev = path[idx - 1]
        curr = path[idx]
        _stamp_segment_sloped(
            support_z,
            surface,
            prev,
            curr,
            width,
            float(prev[2]) + thickness,
            float(curr[2]) + thickness,
        )


def _stamp_disk(
    support_z: np.ndarray,
    surface,
    x: float,
    y: float,
    z: float,
    radius: float,
) -> None:
    ix0 = max(0, int((x - radius) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((x + radius) / surface.cell_w) + 1)
    iy0 = max(0, int((y - radius) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((y + radius) / surface.cell_w) + 1)
    np.maximum(support_z[iy0:iy1 + 1, ix0:ix1 + 1], z, out=support_z[iy0:iy1 + 1, ix0:ix1 + 1])


def _stamp_segment_sloped(
    support_z: np.ndarray,
    surface,
    p0: np.ndarray,
    p1: np.ndarray,
    width: float,
    z0: float,
    z1: float,
) -> None:
    """Stamp the swept footprint of one segment with linearly interpolated z."""
    x0, y0 = float(p0[0]), float(p0[1])
    x1, y1 = float(p1[0]), float(p1[1])
    hw = width / 2.0
    dx = x1 - x0
    dy = y1 - y0
    segment_length = float(np.hypot(dx, dy))
    if segment_length < 1e-9:
        _stamp_disk(support_z, surface, x1, y1, max(z0, z1), hw)
        return
    ux, uy = dx / segment_length, dy / segment_length
    px, py = -uy, ux
    ix0 = max(0, int((min(x0, x1) - hw) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((max(x0, x1) + hw) / surface.cell_w) + 1)
    iy0 = max(0, int((min(y0, y1) - hw) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((max(y0, y1) + hw) / surface.cell_w) + 1)

    cols = np.arange(ix0, ix1 + 1)
    rows = np.arange(iy0, iy1 + 1)
    xx = (cols + 0.5) * surface.cell_w
    yy = (rows + 0.5) * surface.cell_w
    X, Y = np.meshgrid(xx, yy)
    rel_x = X - x0
    rel_y = Y - y0
    along = rel_x * ux + rel_y * uy
    lateral = rel_x * px + rel_y * py
    mask = (
        (along >= -surface.cell_w * 0.5)
        & (along <= segment_length + surface.cell_w * 0.5)
        & (np.abs(lateral) <= hw)
    )
    along_normalized = np.clip(along / segment_length, 0.0, 1.0)
    z_field = z0 + (z1 - z0) * along_normalized
    block = support_z[iy0:iy1 + 1, ix0:ix1 + 1]
    np.maximum(block, np.where(mask, z_field, block), out=block)
