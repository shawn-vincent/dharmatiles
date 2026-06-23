"""Material tagging and face-colour helpers for DharmaTiles mesh output.

Each mesh part produced by the pipeline is tagged with a :class:`Material`
enum value stored in ``mesh.metadata['material']``.  After per-material
grouping, :func:`tag` stamps uniform RGBA face colours so downstream
exporters (3MF scenes, colour-STL) carry the intended palette.

3MF export and Bambu X1C plate-packing live in :mod:`core.export_3mf`.

Colour palette
--------------
SOIL          reddish-brown     #8B5A2B   dirt / bare-soil surface (SoilCarpet regions)
ROCK          blue-gray          #6A7F96   scattered half-ellipsoid rocks
GRASS         yellow-green       #9ACD32   grass carpet + 3-D blade geometry
WATER         bluish-turquoise   #149BD2   water volume mesh
BASE          dark gray          #505050   socket-peg / T-slot underside base
FLOWER        golden yellow      #F5C300   flowers / attractor debug spheres (no groups)
WOOD          warm brown         #8B633F   tree trunks and branches
FOLIAGE       dark forest green  #175C0D   tree leaf clumps on terminal branches
DEBUG_COLOR_* vivid palette      —         10-slot cycling palette for per-group debug
              (blue, orange, purple, cyan, yellow, pink, lime, amber, indigo, brown-orange)
              Red and green are reserved for RGBA_FLAG_FAIL / RGBA_FLAG_PASS.

Status flag colours (not in the cycling palette — use these when signalling a
binary pass/fail condition anywhere in the codebase, not for group identity):
RGBA_FLAG_PASS  vivid green   pass / OK / printable
RGBA_FLAG_FAIL  vivid red     fail / error / overhang
"""
from __future__ import annotations

import pathlib
import struct
from enum import IntEnum

import numpy as np
import trimesh


class Material(IntEnum):
    SOIL          =  0
    ROCK          =  1
    GRASS         =  2
    WATER         =  3
    BASE          =  4
    FLOWER        =  5
    WOOD          =  6
    FOLIAGE       =  7
    # 12-slot vivid palette for per-group debug colouring (e.g. attractor spheres).
    # Use debug_material(group_label) to map an integer label → Material.
    DEBUG_COLOR_0  =  8
    DEBUG_COLOR_1  =  9
    DEBUG_COLOR_2  = 10
    DEBUG_COLOR_3  = 11
    DEBUG_COLOR_4  = 12
    DEBUG_COLOR_5  = 13
    DEBUG_COLOR_6  = 14
    DEBUG_COLOR_7  = 15
    DEBUG_COLOR_8  = 16
    DEBUG_COLOR_9  = 17
    DEBUG_COLOR_10 = 18
    DEBUG_COLOR_11 = 19


#: RGBA uint8 palette — one entry per :class:`Material`.
RGBA: dict[Material, tuple[int, int, int, int]] = {
    Material.SOIL:          (105,  38,  12, 255),   # deep dark red-brown
    Material.ROCK:          ( 72,  92, 128, 255),   # dark slate blue-grey
    Material.GRASS:         ( 42, 148,  28, 255),   # deep vivid green
    Material.WATER:         ( 20, 133, 213, 255),   # blue-turquoise
    Material.BASE:          ( 45,  45,  45, 255),   # dark gray
    Material.FLOWER:        (245, 195,   0, 255),   # golden yellow
    Material.WOOD:          (139,  99,  63, 255),   # light warm brown
    Material.FOLIAGE:       ( 23,  92,  13, 255),   # dark forest green
    # Debug palette — vivid, distinct, cycles via debug_material()
    Material.DEBUG_COLOR_0:  (255,  70,  70, 255),  # red
    Material.DEBUG_COLOR_1:  ( 70, 140, 255, 255),  # blue
    Material.DEBUG_COLOR_2:  ( 50, 210,  50, 255),  # green
    Material.DEBUG_COLOR_3:  (255, 170,  30, 255),  # orange
    Material.DEBUG_COLOR_4:  (200,  70, 230, 255),  # purple
    Material.DEBUG_COLOR_5:  ( 50, 215, 215, 255),  # cyan
    Material.DEBUG_COLOR_6:  (255, 255,  50, 255),  # yellow
    Material.DEBUG_COLOR_7:  (255, 110, 180, 255),  # pink
    Material.DEBUG_COLOR_8:  (130, 220,  60, 255),  # lime
    Material.DEBUG_COLOR_9:  (255, 155,  80, 255),  # amber
    Material.DEBUG_COLOR_10: (110, 110, 255, 255),  # indigo
    Material.DEBUG_COLOR_11: (210, 120,  40, 255),  # brown-orange
}

#: Vivid green — pass / OK status flag for any debug colouring.
#: Use this (not a DEBUG_COLOR_* slot) when signalling a pass/OK condition.
RGBA_FLAG_PASS: tuple[int, int, int, int] = ( 50, 210,  50, 255)

#: Vivid red — fail / error status flag for any debug colouring.
#: Use this (not a DEBUG_COLOR_* slot) when signalling a fail/error condition.
RGBA_FLAG_FAIL: tuple[int, int, int, int] = (220,  50,  50, 255)

#: Cycling palette for per-group debug colouring — 10 slots, red and green excluded.
#: Red and green are reserved for pass/fail status (RGBA_FLAG_PASS / RGBA_FLAG_FAIL)
#: and must not appear as group-identity colours anywhere in the codebase.
DEBUG_COLORS: list[Material] = [
    # slot 0 (red) and slot 2 (green) intentionally omitted — use RGBA_FLAG_FAIL / RGBA_FLAG_PASS
    Material.DEBUG_COLOR_1,   # blue
    Material.DEBUG_COLOR_3,   # orange
    Material.DEBUG_COLOR_4,   # purple
    Material.DEBUG_COLOR_5,   # cyan
    Material.DEBUG_COLOR_6,   # yellow
    Material.DEBUG_COLOR_7,   # pink
    Material.DEBUG_COLOR_8,   # lime
    Material.DEBUG_COLOR_9,   # amber
    Material.DEBUG_COLOR_10,  # indigo
    Material.DEBUG_COLOR_11,  # brown-orange
]


def debug_material(label: int) -> Material:
    """Map an integer group label to a :class:`Material` debug colour (cycles mod 12)."""
    return DEBUG_COLORS[int(label) % len(DEBUG_COLORS)]


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
