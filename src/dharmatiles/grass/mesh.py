"""Mesh construction and support rasterisation for completed grass paths."""

from __future__ import annotations

import numpy as np
import trimesh

from .config import GrassConfig
from .growers import GROWERS
from .seed import GrassPath


def build_meshes(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> list[trimesh.Trimesh]:
    """Build meshes for all paths, interleaving point lifting and support updates.

    For each path (downstream-first order):
    1. Lift every interior point to max(planned_z, support_z).
       Snap the tip exactly to support_z (up or down).
    2. Build the mesh from the adjusted path.
    3. Update scene.support_z from the actual mesh top surface using
       profile-aware, slope-aware rasterisation.
    """
    species_map = {species.name: species for species in cfg.species}
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        species = species_map[path.seed.species_id]
        grower = GROWERS[species.grower]

        # Step 1: adjust path points against the current accumulated surface.
        lifted_points = _lift_path_points(path.points, scene.support_z, surface)
        lifted_path = GrassPath(seed=path.seed, points=lifted_points)

        # Step 2: build mesh.
        mesh = grower.build_mesh(lifted_path, species, scene, surface)
        if mesh is not None:
            meshes.append(mesh)

        # Step 3: update support_z from actual mesh top surface.
        n_pts = len(lifted_points)
        taper_start = max(1, int(np.floor((n_pts - 1) * 0.8125)))
        pt_thicknesses = np.full(n_pts, species.blade_thickness, dtype=float)
        if taper_start < n_pts:
            t = np.linspace(0.0, 1.0, n_pts - taper_start)
            pt_thicknesses[taper_start:] = species.blade_thickness * np.cos(t * np.pi / 2.0)

        _rasterise_sloped_path(
            scene.support_z,
            surface,
            np.asarray(lifted_points, dtype=float),
            path.seed.blade_width,
            pt_thicknesses,
            species.blade_top_facets,
        )

    return meshes


# ── Point lifting ─────────────────────────────────────────────────────────────

def _lift_path_points(
    points: list[tuple[float, float, float]],
    support_z: np.ndarray,
    surface,
) -> list[tuple[float, float, float]]:
    """Adjust path points against the current support surface.

    Interior points: raise only — max(planned_z, floor_z).
    Tip (last point): snap exactly to floor_z whether higher or lower.
    """
    lifted = []
    last_idx = len(points) - 1
    for i, (x, y, z) in enumerate(points):
        floor_z = _sample_grid(support_z, surface, x, y)
        if i == last_idx:
            new_z = floor_z          # snap tip to actual surface
        else:
            new_z = max(z, floor_z)  # interior: only lift
        lifted.append((float(x), float(y), float(new_z)))
    return lifted


def _sample_grid(grid: np.ndarray, surface, x: float, y: float) -> float:
    """Bilinear sample of *grid* at world coordinates (scalar)."""
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


# ── Profile-aware, slope-aware support rasterisation ─────────────────────────

def _rasterise_sloped_path(
    support_z: np.ndarray,
    surface,
    path: np.ndarray,           # (n, 3) spine positions
    width: float,
    thicknesses: np.ndarray,    # (n,) top-profile peak heights per point
    n_top_facets: int,
) -> None:
    """Stamp the blade top surface into support_z.

    Each segment is stamped with z values that:
    - interpolate linearly along the segment between the two endpoint spine z
      values (slope-aware), and
    - vary laterally across the blade width according to the top-profile shape
      (profile-aware): flat at equator for n=1, sine arc for n≥2.
    """
    if len(path) == 0:
        return
    hw = width / 2.0
    # Start-point disk: stamp at the profile peak height (equator for n=1)
    _stamp_disk(support_z, surface, path[0, 0], path[0, 1], path[0, 2] + thicknesses[0], hw)
    for idx in range(1, len(path)):
        prev = path[idx - 1]
        curr = path[idx]
        _stamp_segment_profile(
            support_z,
            surface,
            prev,
            curr,
            width,
            float(prev[2]),
            float(curr[2]),
            float(thicknesses[idx - 1]),
            float(thicknesses[idx]),
            n_top_facets,
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


def _stamp_segment_profile(
    support_z: np.ndarray,
    surface,
    p0: np.ndarray,
    p1: np.ndarray,
    width: float,
    z0: float,          # spine z at p0
    z1: float,          # spine z at p1
    t0: float,          # top-profile peak height at p0
    t1: float,          # top-profile peak height at p1
    n_top_facets: int,
) -> None:
    """Stamp the swept footprint of one segment with slope + profile awareness.

    For n=1 (flat): every cell is stamped at the interpolated spine z (top IS
    the equator; thickness contributes nothing).
    For n≥2: the stamp height also rises laterally toward the blade centre
    following thickness × sin(π × x_frac), where x_frac ∈ [0, 1] across width.
    """
    x0, y0 = float(p0[0]), float(p0[1])
    x1, y1 = float(p1[0]), float(p1[1])
    hw = width / 2.0
    dx = x1 - x0
    dy = y1 - y0
    segment_length = float(np.hypot(dx, dy))
    if segment_length < 1e-9:
        z_max = max(z0 + t0, z1 + t1)
        _stamp_disk(support_z, surface, x1, y1, z_max, hw)
        return
    ux, uy = dx / segment_length, dy / segment_length
    px, py = -uy, ux
    ix0g = max(0, int((min(x0, x1) - hw) / surface.cell_w) - 1)
    ix1g = min(surface.grid_w - 1, int((max(x0, x1) + hw) / surface.cell_w) + 1)
    iy0g = max(0, int((min(y0, y1) - hw) / surface.cell_w) - 1)
    iy1g = min(surface.grid_h - 1, int((max(y0, y1) + hw) / surface.cell_w) + 1)

    cols = np.arange(ix0g, ix1g + 1)
    rows = np.arange(iy0g, iy1g + 1)
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
    along_norm = np.clip(along / segment_length, 0.0, 1.0)
    z_spine  = z0 + (z1 - z0) * along_norm          # slope along segment
    t_interp = t0 + (t1 - t0) * along_norm          # profile height along segment

    if n_top_facets == 1:
        z_field = z_spine                            # flat: top IS equator
    else:
        x_frac = np.clip((lateral + hw) / max(width, 1e-9), 0.0, 1.0)
        z_field = z_spine + t_interp * np.sin(np.pi * x_frac)

    block = support_z[iy0g:iy1g + 1, ix0g:ix1g + 1]
    np.maximum(block, np.where(mask, z_field, block), out=block)
