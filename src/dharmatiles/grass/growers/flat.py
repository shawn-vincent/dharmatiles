"""Flat ribbon grass grower."""

from __future__ import annotations

import numpy as np
import trimesh

from ..config import SpeciesConfig
from ..seed import GrassPath, GrowingPath


class FlatGrassGrower:
    """Grow and mesh the first species: simple floppy flat grass."""

    @staticmethod
    def step(path: GrowingPath, occ_z: np.ndarray, scene, surface, cfg, species: SpeciesConfig) -> bool:
        if not path.alive or len(path.points) == 0:
            path.alive = False
            return False

        seed = path.seed
        cx, cy, cz = path.points[-1]
        direction = seed.direction + seed.curl * (len(path.points) - 1)
        tx = cx + seed.step_len * np.sin(direction)
        ty = cy + seed.step_len * np.cos(direction)
        hw = seed.width / 2.0

        if not (hw <= tx <= surface.tile_w - hw and hw <= ty <= surface.tile_h - hw):
            path.alive = False
            return False

        ix, iy = _cell_index(surface, tx, ty)
        if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
            path.alive = False
            return False

        floor_z = _sample_footprint_max(occ_z, surface, tx, ty, seed.width, direction)
        terrain_z = _sample_grid(scene.terrain_z, surface, tx, ty)
        nz = max(terrain_z, floor_z) + cfg.clearance

        if floor_z - terrain_z > cfg.max_stack_height:
            path.alive = False
            return False
        if nz > cz + seed.rise_cap:
            path.alive = False
            return False

        path.points.append((float(tx), float(ty), float(nz)))
        _stamp_swept_footprint(
            occ_z,
            surface,
            (cx, cy),
            (tx, ty),
            nz + species.thickness,
            seed.width,
        )
        return True

    @staticmethod
    def build_mesh(path: GrassPath, species: SpeciesConfig, scene, surface) -> trimesh.Trimesh | None:
        if len(path.points) < 2:
            return None

        seed = path.seed
        spine = np.asarray(path.points, dtype=float)
        root_z = _sample_grid(scene.terrain_z, surface, spine[0, 0], spine[0, 1]) - species.root_depth
        spine = np.vstack([np.array([[spine[0, 0], spine[0, 1], root_z]]), spine])

        n = len(spine)
        taper_start = max(1, int(np.floor((n - 1) * 0.8125)))
        widths = np.full(n, seed.width, dtype=float)
        if taper_start < n:
            t = np.linspace(0.0, 1.0, n - taper_start)
            widths[taper_start:] = seed.width * (0.25 + 0.75 * np.cos(t * np.pi / 2.0))

        return _build_flat_ribbon_mesh(spine, widths, species.thickness, surface)


def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy


def _sample_grid(grid: np.ndarray, surface, x: float, y: float) -> float:
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


def _sample_footprint_max(
    occ_z: np.ndarray,
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
        return float(occ_z[iy, ix])
    return float(occ_z[iy0:iy1 + 1, ix0:ix1 + 1][mask].max())


def _stamp_swept_footprint(
    occ_z: np.ndarray,
    surface,
    p0: tuple[float, float],
    p1: tuple[float, float],
    z: float,
    width: float,
) -> None:
    x0, y0 = p0
    x1, y1 = p1
    hw = width / 2.0
    dx = x1 - x0
    dy = y1 - y0
    seg_len = float(np.hypot(dx, dy))
    if seg_len < 1e-9:
        ux, uy = 0.0, 1.0
    else:
        ux, uy = dx / seg_len, dy / seg_len
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
    along = rel_x * ux + rel_y * uy
    lateral = rel_x * px + rel_y * py
    mask = (along >= -surface.cell_w * 0.5) & (along <= seg_len + surface.cell_w * 0.5) & (np.abs(lateral) <= hw)
    block = occ_z[iy0:iy1 + 1, ix0:ix1 + 1]
    np.maximum(block, np.where(mask, z, block), out=block)


def _build_flat_ribbon_mesh(path: np.ndarray, widths: np.ndarray, thickness: float, surface) -> trimesh.Trimesh:
    tangents = np.empty_like(path)
    tangents[:-1] = path[1:] - path[:-1]
    tangents[-1] = path[-1] - path[-2]
    xy = tangents[:, :2]
    norms = np.linalg.norm(xy, axis=1)
    fallback = norms < 1e-9
    norms[fallback] = 1.0
    side = np.column_stack([-xy[:, 1] / norms, xy[:, 0] / norms])
    if fallback.any():
        side[fallback] = np.array([1.0, 0.0])

    half = widths / 2.0
    verts = np.empty((len(path) * 4, 3), dtype=float)
    verts[0::4] = np.column_stack([path[:, 0] + side[:, 0] * half, path[:, 1] + side[:, 1] * half, path[:, 2]])
    verts[1::4] = np.column_stack([path[:, 0] - side[:, 0] * half, path[:, 1] - side[:, 1] * half, path[:, 2]])
    verts[2::4] = verts[0::4]
    verts[2::4, 2] += thickness
    verts[3::4] = verts[1::4]
    verts[3::4, 2] += thickness

    np.clip(verts[:, 0], 0.0, surface.tile_w, out=verts[:, 0])
    np.clip(verts[:, 1], 0.0, surface.tile_h, out=verts[:, 1])

    faces: list[list[int]] = []
    for i in range(len(path) - 1):
        a = i * 4
        b = (i + 1) * 4
        faces.extend([
            [a + 2, b + 2, a + 3], [a + 3, b + 2, b + 3],
            [a, a + 1, b], [a + 1, b + 1, b],
            [a, b, a + 2], [a + 2, b, b + 2],
            [a + 1, a + 3, b + 1], [a + 3, b + 3, b + 1],
        ])
    faces.extend([[0, 2, 1], [1, 2, 3]])
    e = (len(path) - 1) * 4
    faces.extend([[e, e + 1, e + 2], [e + 1, e + 3, e + 2]])

    mesh = trimesh.Trimesh(vertices=verts, faces=np.asarray(faces), process=False)
    mesh.fix_normals()
    return mesh
