"""DungeonBlocks-compatible base generation and export.

DungeonBlocks credit: the socket-peg base geometry is based on the
DungeonBlocks blank/floor tile standard published on MyMiniFactory:
https://www.myminifactory.com/object/3d-print-dungeonblocks-blank-floor-tile-177592
"""
from __future__ import annotations

import pathlib

import numpy as np
import trimesh

from ..core.color import Material, tag as _tag, export_color_stl, build_scene
from ..core.config import BaseConfig, SurfaceConfig
from ..core.logo import make_logo_manifold


SYSTEM_SUFFIX = "dungeonblocks"


def select_peg_height(terrain_z: np.ndarray,
                      base_cfg: BaseConfig) -> float:
    """Return peg column height (mm) for *terrain_z*.

    Uses ``base_cfg.peg_height`` when set; otherwise auto-selects
    ``tall_peg_height`` when the max terrain height exceeds
    ``auto_threshold_mm``, else ``short_peg_height``.
    """
    if base_cfg.peg_height is not None:
        return base_cfg.peg_height
    max_h = float(terrain_z.max())
    return (base_cfg.tall_peg_height
            if max_h > base_cfg.auto_threshold_mm
            else base_cfg.short_peg_height)


def _square_ring(tx: float, ty: float,
                 inset: float, tile_sz: float, z: float) -> np.ndarray:
    """Four CCW corner vertices of a square ring at the given z level."""
    i = inset
    s = tile_sz
    return np.array([
        [tx + i,     ty + i,     z],
        [tx + s - i, ty + i,     z],
        [tx + s - i, ty + s - i, z],
        [tx + i,     ty + s - i, z],
    ], dtype=float)


def _prismatoid_mesh(rings: list[np.ndarray]) -> trimesh.Trimesh:
    """Closed watertight mesh from top-to-bottom 4-vertex rectangular rings."""
    n = len(rings)
    verts = np.vstack(rings)
    faces: list[list[int]] = []

    faces += [[0, 1, 2], [0, 2, 3]]

    b = 4 * (n - 1)
    faces += [[b, b + 2, b + 1], [b, b + 3, b + 2]]

    for i in range(n - 1):
        a = 4 * i
        c = 4 * (i + 1)
        for j in range(4):
            j1 = (j + 1) % 4
            faces += [[a + j, c + j,  c + j1],
                      [a + j, c + j1, a + j1]]

    mesh = trimesh.Trimesh(vertices=verts,
                           faces=np.array(faces, dtype=np.int32),
                           process=False)
    mesh.fix_normals()
    return mesh


def make_base(surface: SurfaceConfig,
              peg_height: float,
              base_cfg: BaseConfig) -> trimesh.Trimesh:
    """DungeonBlocks socket-base mesh: one peg per 35 mm square.

    The mesh top sits at z = 0 (bottom of the terrain slab). The peg tip reaches
    z = -(peg_height + flare_height).
    """
    square_sz = surface.tile_w / surface.cols
    col       = base_cfg.col_size
    bevel     = base_cfg.col_bevel
    flare_h   = base_cfg.flare_height
    bevel_col = col - 2.0 * bevel

    col_inset   = (square_sz - col) / 2.0
    bevel_inset = (square_sz - bevel_col) / 2.0

    z0 = 0.0
    z1 = -flare_h
    z2 = -(peg_height - bevel + flare_h)
    z3 = -(peg_height + flare_h)

    # Logo inset: centred on each peg tip face at 80 % of bevel_col.
    logo_side = bevel_col * 0.80

    import manifold3d as m3d
    from manifold3d import Manifold, Mesh as _Mesh

    peg_manifolds: list = []
    for ci in range(surface.cols):
        for ri in range(surface.rows):
            tx = ci * square_sz
            ty = ri * square_sz
            rings = [
                _square_ring(tx, ty, 0.0,        square_sz, z0),
                _square_ring(tx, ty, col_inset,  square_sz, z1),
                _square_ring(tx, ty, col_inset,  square_sz, z2),
                _square_ring(tx, ty, bevel_inset,square_sz, z3),
            ]
            peg_tm = _prismatoid_mesh(rings)
            peg_m  = Manifold(mesh=_Mesh(
                vert_properties=peg_tm.vertices.astype('f4'),
                tri_verts=peg_tm.faces.astype('u4'),
            ))

            cx = tx + square_sz / 2.0
            cy = ty + square_sz / 2.0
            peg_m -= make_logo_manifold(cx, cy, logo_side, z_base=z3)
            peg_manifolds.append(peg_m)

    if not peg_manifolds:
        return trimesh.Trimesh()

    # Union all pegs in manifold space, convert once at the end.
    base_m = peg_manifolds[0]
    for pm in peg_manifolds[1:]:
        base_m += pm

    msh = base_m.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(msh.vert_properties, dtype=float)[:, :3],
        faces=np.array(msh.tri_verts, dtype=int),
        process=False,
    )
    mesh.fix_normals()
    return mesh


def export(colored_meshes: list[trimesh.Trimesh],
           surface: SurfaceConfig,
           base_cfg: BaseConfig,
           terrain_z: np.ndarray,
           output_path: pathlib.Path) -> trimesh.Trimesh:
    """Attach a DungeonBlocks base; write colour STL and 3MF side-by-side.

    Parameters
    ----------
    colored_meshes:
        Per-material mesh list produced by the tile pipeline.
    surface, base_cfg, terrain_z:
        As before.
    output_path:
        Target ``.stl`` path.  A sibling ``.3mf`` is written automatically.
    """
    peg_h     = select_peg_height(terrain_z, base_cfg)
    base_mesh = make_base(surface, peg_h, base_cfg)
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

