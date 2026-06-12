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

from ..core.color import Material, tag as _tag, export_color_stl, build_scene
from ..core.config import BaseConfig, SurfaceConfig


SYSTEM_SUFFIX = "openlock"
OPENLOCK_SQUARE_MM = 25.4   # canonical 1-inch imperial standard

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

# OpenLOCK clip-retention side-cut dimensions (spec §3c).
RETENTION_OFFSET_MM = 8.35  # mm — slot centre distance from socket centre
RETENTION_WIDTH_MM  = 4.7   # mm — slot width along tile-edge axis
RETENTION_DEPTH_MM  = 9.0   # mm — slot depth into tile

# T-shaped clip-socket polygon in local (du, dv) coordinates.
# du = offset from socket centre along the tile edge.
# dv = depth measured inward from the tile face (0 = face, positive = inside tile).
#
#   face  ←——— 14 mm (WIDE_1) ———→          dv = 0  (mouth)
#         ←—— 12 mm (WIDE_2) ——→           dv = 2  (DEEP_1 step)
#           ←— 10 mm (WIDE_3) —→           dv = 5  (DEEP_3 step)
#           ←— 10 mm —————————→           dv = 7  (back wall)
_T_POLY: list[tuple[float, float]] = [
    (-CUTOUT_WIDE_1 / 2,  0.0),
    (-CUTOUT_WIDE_1 / 2,  CUTOUT_DEEP_1),
    (-CUTOUT_WIDE_2 / 2,  CUTOUT_DEEP_2),
    (-CUTOUT_WIDE_3 / 2,  CUTOUT_DEEP_3),
    (-CUTOUT_WIDE_3 / 2,  CUTOUT_DEEP_3 + 2.0),
    ( CUTOUT_WIDE_3 / 2,  CUTOUT_DEEP_3 + 2.0),
    ( CUTOUT_WIDE_3 / 2,  CUTOUT_DEEP_3),
    ( CUTOUT_WIDE_2 / 2,  CUTOUT_DEEP_2),
    ( CUTOUT_WIDE_1 / 2,  CUTOUT_DEEP_1),
    ( CUTOUT_WIDE_1 / 2,  0.0),
]


def _explicit_base(surface: SurfaceConfig) -> trimesh.Trimesh:
    """Build the OpenLOCK base entirely via manifold3d Boolean CSG.

    Starts from a solid box and subtracts T-slot sockets, retention side-cuts,
    and the logo inset — all as clean closed-manifold primitives.  The result
    is a watertight trimesh with correct topology by construction.
    """
    import manifold3d as m3d
    from ..core.logo import _logo_contours_mm

    width  = surface.tile_w    # = cols × square_mm
    height = surface.tile_h    # = rows × square_mm
    sq     = surface.square_mm

    def _cs(poly: list) -> m3d.CrossSection:
        return m3d.CrossSection([poly], fillrule=m3d.FillRule.EvenOdd)

    def _extrude(cs: m3d.CrossSection,
                 z_bot: float, z_top: float) -> m3d.Manifold:
        return m3d.Manifold.extrude(cs, height=z_top - z_bot).translate(
            (0.0, 0.0, z_bot))

    # ── Outer box ─────────────────────────────────────────────────────────────
    base = _extrude(
        _cs([(0, 0), (width, 0), (width, height), (0, height)]),
        -WALL_HEIGHT, 0.0,
    )

    z0 = -WALL_HEIGHT + CUTOUT_START_Z   # bottom of clip slot (−6.6 mm)
    z1 = z0 + CUTOUT_HEIGHT              # top    of clip slot (−2.4 mm)

    # T-slot template in (du, dv) space — one entry per contour vertex
    t_poly = list(_T_POLY)   # 10 vertices, CCW or CW; EvenOdd handles either

    # ── South / North sockets ─────────────────────────────────────────────────
    for ci in range(surface.cols):
        cx = (ci + 0.5) * sq

        # South: (du, dv) → (cx+du, dv)
        base -= _extrude(_cs([(cx + du, dv)        for du, dv in t_poly]), z0, z1)
        # North: mirror in Y
        base -= _extrude(_cs([(cx + du, height - dv) for du, dv in t_poly]), z0, z1)

        for sign in (-1.0, +1.0):
            uc  = cx + sign * RETENTION_OFFSET_MM
            u0  = uc - RETENTION_WIDTH_MM / 2;  u1 = uc + RETENTION_WIDTH_MM / 2
            vd  = RETENTION_DEPTH_MM
            # South retention slot
            base -= _extrude(_cs([(u0, 0), (u1, 0), (u1, vd), (u0, vd)]),         z0, z1)
            # North retention slot
            base -= _extrude(_cs([(u0, height - vd), (u1, height - vd),
                                   (u1, height),      (u0, height)]),               z0, z1)

    # ── West / East sockets ───────────────────────────────────────────────────
    for ri in range(surface.rows):
        cy = (ri + 0.5) * sq

        # West: (du, dv) → (dv, cy+du)
        base -= _extrude(_cs([(dv,         cy + du) for du, dv in t_poly]), z0, z1)
        # East: mirror in X
        base -= _extrude(_cs([(width - dv, cy + du) for du, dv in t_poly]), z0, z1)

        for sign in (-1.0, +1.0):
            uc  = cy + sign * RETENTION_OFFSET_MM
            u0  = uc - RETENTION_WIDTH_MM / 2;  u1 = uc + RETENTION_WIDTH_MM / 2
            vd  = RETENTION_DEPTH_MM
            # West retention slot
            base -= _extrude(_cs([(0,           u0), (vd,          u0),
                                   (vd,          u1), (0,           u1)]),           z0, z1)
            # East retention slot
            base -= _extrude(_cs([(width - vd,  u0), (width,       u0),
                                   (width,       u1), (width - vd,  u1)]),           z0, z1)

    # ── Logo inset on bottom cap ───────────────────────────────────────────────
    logo_side    = min(22.0, min(width, height) * 0.75)
    logo_contours = _logo_contours_mm(width / 2.0, height / 2.0, logo_side)
    logo_cs       = m3d.CrossSection(logo_contours, fillrule=m3d.FillRule.EvenOdd)
    base         -= _extrude(logo_cs, -WALL_HEIGHT, -WALL_HEIGHT + 0.4)

    # ── Convert Manifold → trimesh ─────────────────────────────────────────────
    msh  = base.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(msh.vert_properties, dtype=float)[:, :3],
        faces=np.array(msh.tri_verts, dtype=int),
        process=False,
    )
    mesh.fix_normals()
    return mesh


def make_base(surface: SurfaceConfig) -> trimesh.Trimesh:
    """Return an OpenLock base for the N×M DharmaTiles footprint.

    Clip sockets are subtracted from perimeter edges only. Interior square
    boundaries remain solid, which is the appropriate geometry for one merged
    N×M tile rather than separate 1×1 tiles.
    """
    return _explicit_base(surface)


def export(colored_meshes: list[trimesh.Trimesh],
           surface: SurfaceConfig,
           base_cfg: BaseConfig,
           terrain_z: np.ndarray,
           output_path: pathlib.Path) -> trimesh.Trimesh:
    """Attach an OpenLOCK base; write colour STL and 3MF side-by-side.

    Parameters
    ----------
    colored_meshes:
        Per-material mesh list produced by the tile pipeline.
    surface, base_cfg, terrain_z:
        As before.
    output_path:
        Target ``.stl`` path.  A sibling ``.3mf`` is written automatically.
    """
    del base_cfg, terrain_z
    base_mesh = make_base(surface)
    _tag(base_mesh, Material.BASE)

    all_meshes = [base_mesh] + list(colored_meshes)

    # ── Colour STL (Materialise RGB15 attribute bytes) ────────────────────────
    combined = trimesh.util.concatenate(all_meshes)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(combined, output_path)

    # ── 3MF (one object per material, native multi-colour) ────────────────────
    tmf_path = output_path.with_suffix('.3mf')
    build_scene(all_meshes).export(str(tmf_path))

    return combined
