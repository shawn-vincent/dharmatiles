"""Material tagging and face-colour helpers for DharmaTiles mesh output.

Each mesh part produced by the pipeline is tagged with a :class:`Material`
enum value stored in ``mesh.metadata['material']``.  After per-material
grouping, :func:`tag` stamps uniform RGBA face colours so downstream
exporters (3MF scenes, colour-STL) carry the intended palette.

Colour palette
--------------
SOIL     reddish-brown   #8B5A2B   dirt / bare-soil surface (SoilCarpet regions)
ROCK     blue-gray        #6A7F96   scattered half-ellipsoid rocks
GRASS    yellow-green     #9ACD32   grass carpet + 3-D blade geometry
WATER    turquoise        #40E0D0   water volume mesh
BASE     dark gray        #505050   socket-peg / T-slot underside base
"""
from __future__ import annotations

import pathlib
import struct
from enum import IntEnum

import numpy as np
import trimesh


class Material(IntEnum):
    SOIL  = 0
    ROCK  = 1
    GRASS = 2
    WATER = 3
    BASE  = 4


#: RGBA uint8 palette — one entry per :class:`Material`.
RGBA: dict[Material, tuple[int, int, int, int]] = {
    Material.SOIL:  (139,  90,  43, 255),   # reddish-brown
    Material.ROCK:  (106, 127, 150, 255),   # blue-gray
    Material.GRASS: (154, 205,  50, 255),   # yellow-green
    Material.WATER: ( 64, 224, 208, 255),   # turquoise
    Material.BASE:  ( 80,  80,  80, 255),   # dark gray
}


def tag(mesh: trimesh.Trimesh, mat: Material) -> None:
    """Stamp *mat* onto *mesh* in-place: metadata + uniform face colours.

    Safe to call after a boolean union — the union strips attributes but this
    re-establishes them.  Also safe on empty meshes (no-op when face count is 0).
    """
    mesh.metadata['material'] = mat
    n = len(mesh.faces)
    if n == 0:
        return
    rgba = RGBA[mat]
    mesh.visual = trimesh.visual.ColorVisuals(
        mesh=mesh,
        face_colors=np.tile(rgba, (n, 1)).astype(np.uint8),
    )


def export_color_stl(mesh: trimesh.Trimesh,
                     path: str | pathlib.Path) -> None:
    """Export *mesh* as a binary STL with RGB15 face-colour attribute bytes.

    The encoding follows the Materialise / Magics convention::

        attribute_byte = 0x8000 | (R5 << 10) | (G5 << 5) | B5

    Supported by: 3D Builder (Windows), MeshMixer, Materialise Magics.
    PrusaSlicer and Bambu Studio ignore it — use the 3MF output for those.
    """
    verts   = mesh.vertices.astype('<f4')       # (V, 3) little-endian float32
    faces   = mesh.faces                         # (F, 3) int
    normals = mesh.face_normals.astype('<f4')    # (F, 3) little-endian float32
    n       = len(faces)

    # Fetch face colours; fall back to SOIL brown if visual is absent.
    try:
        fc = mesh.visual.face_colors[:, :3].astype(np.uint16)
    except Exception:
        c  = RGBA[Material.SOIL][:3]
        fc = np.tile(c, (n, 1)).astype(np.uint16)

    r5   = fc[:, 0] >> 3
    g5   = fc[:, 1] >> 3
    b5   = fc[:, 2] >> 3
    attr = (np.uint16(0x8000) | (r5 << 10) | (g5 << 5) | b5).astype('<u2')

    # Build the binary body as a flat uint8 array.
    # Each triangle: 12 B normal + 12 B v0 + 12 B v1 + 12 B v2 + 2 B attr = 50 B
    body = np.empty(n * 50, dtype=np.uint8)
    bv   = body.reshape(n, 50)

    bv[:, 0:12]  = normals.view(np.uint8).reshape(n, 12)
    bv[:, 12:24] = verts[faces[:, 0]].view(np.uint8).reshape(n, 12)
    bv[:, 24:36] = verts[faces[:, 1]].view(np.uint8).reshape(n, 12)
    bv[:, 36:48] = verts[faces[:, 2]].view(np.uint8).reshape(n, 12)
    bv[:, 48:50] = attr.view(np.uint8).reshape(n, 2)

    with open(str(path), 'wb') as f:
        f.write(b'\x00' * 80)               # 80-byte header (blank)
        f.write(struct.pack('<I', n))        # face count
        f.write(body.tobytes())


def build_scene(meshes: list[trimesh.Trimesh]) -> trimesh.Scene:
    """Wrap coloured mesh parts as a :class:`trimesh.Scene` for 3MF export.

    Each part is added as a named geometry using its material label.
    Duplicate names are disambiguated with a numeric suffix.
    """
    scene: trimesh.Scene = trimesh.Scene()
    counts: dict[str, int] = {}
    for mesh in meshes:
        mat  = mesh.metadata.get('material', Material.SOIL)
        base = mat.name.lower() if isinstance(mat, Material) else str(mat).lower()
        idx  = counts.get(base, 0)
        name = base if idx == 0 else f"{base}_{idx}"
        counts[base] = idx + 1
        scene.add_geometry(mesh, geom_name=name)
    return scene
