"""Mesh construction and support rasterisation for completed grass paths."""

from __future__ import annotations

import numpy as np
import trimesh

from .config import GrassConfig
from .growers import GROWERS
from .seed import GrassPath


def build_meshes(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> list[trimesh.Trimesh]:
    species_map = {species.name: species for species in cfg.species}
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        species = species_map[path.seed.species_id]
        grower = GROWERS[species.grower]
        mesh = grower.build_mesh(path, species, scene, surface)
        if mesh is not None:
            meshes.append(mesh)
    return meshes


def rasterise_paths_into_support(paths: list[GrassPath], cfg: GrassConfig, scene, surface) -> None:
    species_map = {species.name: species for species in cfg.species}
    for path in paths:
        species = species_map[path.seed.species_id]
        _rasterise_flat_path(scene.support_z, surface, np.asarray(path.points, dtype=float), path.seed.width, species.thickness)


def _rasterise_flat_path(
    support_z: np.ndarray,
    surface,
    path: np.ndarray,
    width: float,
    thickness: float,
) -> None:
    if len(path) == 0:
        return
    hw = width / 2.0
    for idx, point in enumerate(path):
        if idx == 0:
            _stamp_disk(support_z, surface, point[0], point[1], point[2] + thickness, hw)
            continue
        prev = path[idx - 1]
        _stamp_segment(support_z, surface, prev, point, width, point[2] + thickness)


def _stamp_disk(support_z: np.ndarray, surface, x: float, y: float, z: float, radius: float) -> None:
    ix0 = max(0, int((x - radius) / surface.cell_w) - 1)
    ix1 = min(surface.grid_w - 1, int((x + radius) / surface.cell_w) + 1)
    iy0 = max(0, int((y - radius) / surface.cell_w) - 1)
    iy1 = min(surface.grid_h - 1, int((y + radius) / surface.cell_w) + 1)
    np.maximum(support_z[iy0:iy1 + 1, ix0:ix1 + 1], z, out=support_z[iy0:iy1 + 1, ix0:ix1 + 1])


def _stamp_segment(support_z: np.ndarray, surface, p0: np.ndarray, p1: np.ndarray, width: float, z: float) -> None:
    x0, y0 = float(p0[0]), float(p0[1])
    x1, y1 = float(p1[0]), float(p1[1])
    hw = width / 2.0
    dx = x1 - x0
    dy = y1 - y0
    seg_len = float(np.hypot(dx, dy))
    if seg_len < 1e-9:
        _stamp_disk(support_z, surface, x1, y1, z, hw)
        return
    ux, uy = dx / seg_len, dy / seg_len
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
    mask = (along >= -surface.cell_w * 0.5) & (along <= seg_len + surface.cell_w * 0.5) & (np.abs(lateral) <= hw)
    block = support_z[iy0:iy1 + 1, ix0:ix1 + 1]
    np.maximum(block, np.where(mask, z, block), out=block)
