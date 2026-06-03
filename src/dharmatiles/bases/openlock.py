"""OpenLock-compatible base generation and export.

OpenLock credit: clip socket dimensions and placement strategy are ported from
OpenSCAD-OpenLock by Caitlyn Byrne:
https://github.com/caitlynb/OpenSCAD-OpenLock

OpenLock project credit: this output targets the OpenLock tile system created
by Printable Scenery.
"""
from __future__ import annotations

import pathlib

import numpy as np
import trimesh

from ..core.config import BaseConfig, SurfaceConfig
from ..core.mesh import export_coloured_stl


SYSTEM_SUFFIX = "openlock"
OPENLOCK_SQUARE_MM = 25.0
DUNGEONBLOCKS_SQUARE_MM = 35.0
XY_SCALE = OPENLOCK_SQUARE_MM / DUNGEONBLOCKS_SQUARE_MM

# Ported from OpenSCAD-OpenLock/OpenLock.scad.
WALL_HEIGHT = 8.0
CUTOUT_HEIGHT = 4.2
CUTOUT_START_Z = 1.4
CUTOUT_WIDE_1 = 14.0
CUTOUT_DEEP_1 = 2.0
CUTOUT_DEEP_2 = 2.0
CUTOUT_WIDE_2 = 12.0
CUTOUT_WIDE_3 = 10.0
CUTOUT_DEEP_3 = 5.0


def _add_quad(verts: list[list[float]],
              faces: list[list[int]],
              a: list[float],
              b: list[float],
              c: list[float],
              d: list[float]) -> None:
    i = len(verts)
    verts.extend([a, b, c, d])
    faces.extend([[i, i + 1, i + 2], [i, i + 2, i + 3]])


def _side_with_rect_holes(verts: list[list[float]],
                          faces: list[list[int]],
                          edge: str,
                          width: float,
                          height: float,
                          holes: list[tuple[float, float, float, float]]) -> None:
    """Add one side wall, omitting rectangular clip-mouth holes."""
    side_len = width if edge in {"south", "north"} else height
    u_edges = [0.0, side_len]
    z_edges = [-WALL_HEIGHT, 0.0]
    for u0, u1, z0, z1 in holes:
        u_edges.extend([u0, u1])
        z_edges.extend([z0, z1])
    u_edges = sorted(set(round(v, 6) for v in u_edges))
    z_edges = sorted(set(round(v, 6) for v in z_edges))

    def in_hole(uc: float, zc: float) -> bool:
        return any(u0 <= uc <= u1 and z0 <= zc <= z1
                   for u0, u1, z0, z1 in holes)

    def point(u: float, z: float) -> list[float]:
        if edge == "south":
            return [u, 0.0, z]
        if edge == "north":
            return [side_len - u, height, z]
        if edge == "west":
            return [0.0, side_len - u, z]
        if edge == "east":
            return [width, u, z]
        raise ValueError(f"unknown edge {edge!r}")

    for ui in range(len(u_edges) - 1):
        for zi in range(len(z_edges) - 1):
            u0, u1 = u_edges[ui], u_edges[ui + 1]
            z0, z1 = z_edges[zi], z_edges[zi + 1]
            if in_hole((u0 + u1) / 2.0, (z0 + z1) / 2.0):
                continue
            _add_quad(verts, faces,
                      point(u0, z0), point(u1, z0),
                      point(u1, z1), point(u0, z1))


def _cap_grid(verts: list[list[float]],
              faces: list[list[int]],
              width: float,
              height: float,
              z: float,
              x_edges: list[float],
              y_edges: list[float]) -> None:
    """Add a top or bottom cap subdivided to match side-wall edge breaks."""
    xs = sorted(set(round(v, 6) for v in ([0.0, width] + x_edges)))
    ys = sorted(set(round(v, 6) for v in ([0.0, height] + y_edges)))
    for xi in range(len(xs) - 1):
        for yi in range(len(ys) - 1):
            x0, x1 = xs[xi], xs[xi + 1]
            y0, y1 = ys[yi], ys[yi + 1]
            if z >= 0.0:
                _add_quad(verts, faces, [x0, y0, z], [x1, y0, z],
                          [x1, y1, z], [x0, y1, z])
            else:
                _add_quad(verts, faces, [x0, y1, z], [x1, y1, z],
                          [x1, y0, z], [x0, y0, z])


def _add_socket_void(verts: list[list[float]],
                     faces: list[list[int]],
                     edge: str,
                     center: float,
                     width: float,
                     height: float) -> None:
    """Add inner walls for one side-opening OpenLock socket."""
    u0 = center - CUTOUT_WIDE_1 / 2.0
    u1 = center + CUTOUT_WIDE_1 / 2.0
    v0 = 0.0
    v1 = CUTOUT_DEEP_3 + 2.0
    z0 = -WALL_HEIGHT + CUTOUT_START_Z
    z1 = z0 + CUTOUT_HEIGHT

    def p(u: float, v: float, z: float) -> list[float]:
        if edge == "south":
            return [u, v, z]
        if edge == "north":
            return [u, height - v, z]
        if edge == "west":
            return [v, u, z]
        if edge == "east":
            return [width - v, u, z]
        raise ValueError(f"unknown edge {edge!r}")

    # Back wall and four interior side walls. The mouth at v0 is open.
    _add_quad(verts, faces, p(u1, v1, z0), p(u0, v1, z0),
              p(u0, v1, z1), p(u1, v1, z1))
    _add_quad(verts, faces, p(u0, v0, z0), p(u0, v1, z0),
              p(u0, v1, z1), p(u0, v0, z1))
    _add_quad(verts, faces, p(u1, v1, z0), p(u1, v0, z0),
              p(u1, v0, z1), p(u1, v1, z1))
    _add_quad(verts, faces, p(u0, v1, z1), p(u0, v0, z1),
              p(u1, v0, z1), p(u1, v1, z1))
    _add_quad(verts, faces, p(u0, v0, z0), p(u0, v1, z0),
              p(u1, v1, z0), p(u1, v0, z0))


def _explicit_base(surface: SurfaceConfig) -> trimesh.Trimesh:
    width = surface.cols * OPENLOCK_SQUARE_MM
    height = surface.rows * OPENLOCK_SQUARE_MM
    verts: list[list[float]] = []
    faces: list[list[int]] = []

    z0 = -WALL_HEIGHT + CUTOUT_START_Z
    z1 = z0 + CUTOUT_HEIGHT
    south_holes = []
    north_holes = []
    cap_x_edges: list[float] = []
    cap_y_edges: list[float] = []
    for ci in range(surface.cols):
        cx = (ci + 0.5) * OPENLOCK_SQUARE_MM
        hole = (cx - CUTOUT_WIDE_1 / 2.0, cx + CUTOUT_WIDE_1 / 2.0, z0, z1)
        south_holes.append(hole)
        north_holes.append(hole)
        cap_x_edges.extend([hole[0], hole[1]])
        _add_socket_void(verts, faces, "south", cx, width, height)
        _add_socket_void(verts, faces, "north", cx, width, height)

    west_holes = []
    east_holes = []
    for ri in range(surface.rows):
        cy = (ri + 0.5) * OPENLOCK_SQUARE_MM
        hole = (cy - CUTOUT_WIDE_1 / 2.0, cy + CUTOUT_WIDE_1 / 2.0, z0, z1)
        west_holes.append(hole)
        east_holes.append(hole)
        cap_y_edges.extend([hole[0], hole[1]])
        _add_socket_void(verts, faces, "west", cy, width, height)
        _add_socket_void(verts, faces, "east", cy, width, height)

    _cap_grid(verts, faces, width, height, 0.0, cap_x_edges, cap_y_edges)
    _cap_grid(verts, faces, width, height, -WALL_HEIGHT, cap_x_edges, cap_y_edges)

    _side_with_rect_holes(verts, faces, "south", width, height, south_holes)
    _side_with_rect_holes(verts, faces, "north", width, height, north_holes)
    _side_with_rect_holes(verts, faces, "west", width, height, west_holes)
    _side_with_rect_holes(verts, faces, "east", width, height, east_holes)

    mesh = trimesh.Trimesh(vertices=np.array(verts, dtype=float),
                           faces=np.array(faces, dtype=np.int32),
                           process=False)
    mesh.merge_vertices()
    mesh.fix_normals()
    return mesh


def make_base(surface: SurfaceConfig) -> trimesh.Trimesh:
    """Return an OpenLock base for the N×M DharmaTiles footprint.

    Clip sockets are subtracted from perimeter edges only. Interior square
    boundaries remain solid, which is the appropriate geometry for one merged
    N×M tile rather than separate 1×1 tiles.
    """
    return _explicit_base(surface)


def scale_tile_mesh(tile_mesh: trimesh.Trimesh) -> trimesh.Trimesh:
    """Scale a 35 mm DharmaTiles mesh down to the 25 mm OpenLock XY grid."""
    mesh = tile_mesh.copy()
    transform = np.eye(4)
    transform[0, 0] = XY_SCALE
    transform[1, 1] = XY_SCALE
    mesh.apply_transform(transform)
    return mesh


def add_base(tile_mesh: trimesh.Trimesh,
             surface: SurfaceConfig,
             base_cfg: BaseConfig,
             terrain_z: np.ndarray) -> trimesh.Trimesh:
    """Return a 25 mm-grid OpenLock tile with terrain and base attached."""
    del base_cfg, terrain_z
    base_mesh = make_base(surface)
    base_mesh.visual.face_colors = np.zeros((len(base_mesh.faces), 4), dtype=np.uint8)
    return trimesh.util.concatenate([base_mesh, scale_tile_mesh(tile_mesh)])


def export(tile_mesh: trimesh.Trimesh,
           surface: SurfaceConfig,
           base_cfg: BaseConfig,
           terrain_z: np.ndarray,
           output_path: pathlib.Path) -> trimesh.Trimesh:
    """Attach an OpenLock base and write the system-specific STL."""
    combined = add_base(tile_mesh, surface, base_cfg, terrain_z)
    export_coloured_stl(combined, output_path)
    return combined
