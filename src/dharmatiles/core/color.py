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
    Material.SOIL:  (105,  38,  12, 255),   # deep dark red-brown
    Material.ROCK:  ( 72,  92, 128, 255),   # dark slate blue-grey
    Material.GRASS: ( 42, 148,  28, 255),   # deep vivid green
    Material.WATER: ( 20, 200, 195, 255),   # vivid turquoise
    Material.BASE:  ( 45,  45,  45, 255),   # dark gray
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


def export_3mf_colored(meshes: list[trimesh.Trimesh],
                        path: str | pathlib.Path) -> None:
    """Export coloured mesh parts as a 3MF with Material Extension face colours.

    Each mesh's ``face_colors`` are encoded as per-face colour indices using
    the 3MF Material Extension ``<m:colorgroup>`` mechanism.  Supports both
    uniform-colour meshes (one colour for all faces) and per-face-coloured
    meshes (e.g. the terrain solid where regions carry distinct colours).

    Compatible with PrusaSlicer, Bambu Studio, and Windows 3D Builder.
    """
    import io
    import zipfile

    # ── Collect all unique RGBA colours across every mesh ────────────────────
    all_rgba: list[tuple[int, int, int, int]] = []
    color_index: dict[tuple[int, int, int, int], int] = {}

    def _register(rgba: tuple[int, int, int, int]) -> int:
        if rgba not in color_index:
            color_index[rgba] = len(all_rgba)
            all_rgba.append(rgba)
        return color_index[rgba]

    # Pre-scan to build the palette
    mesh_face_indices: list[np.ndarray] = []
    for mesh in meshes:
        n = len(mesh.faces)
        if n == 0:
            mesh_face_indices.append(np.empty(0, dtype=np.int32))
            continue
        try:
            fc = mesh.visual.face_colors  # (N, 4) uint8
            if fc is None or len(fc) != n:
                raise AttributeError
            indices = np.array(
                [_register(tuple(int(c) for c in row)) for row in fc],  # type: ignore[arg-type]
                dtype=np.int32,
            )
        except (AttributeError, Exception):
            fallback = RGBA.get(mesh.metadata.get('material', Material.SOIL),
                                RGBA[Material.SOIL])
            idx = _register(fallback)
            indices = np.full(n, idx, dtype=np.int32)
        mesh_face_indices.append(indices)

    # ── Build the 3MF XML ─────────────────────────────────────────────────────
    COLOR_GID = 1        # colorgroup resource id
    OBJ_ID_START = 10   # object ids start here

    def _hex(rgba: tuple[int, int, int, int]) -> str:
        return '#{:02X}{:02X}{:02X}{:02X}'.format(*rgba)

    # colorgroup XML
    cg_entries = '\n      '.join(
        f'<m:color color="{_hex(c)}"/>' for c in all_rgba
    )
    colorgroup_xml = (
        f'    <m:colorgroup id="{COLOR_GID}">\n'
        f'      {cg_entries}\n'
        f'    </m:colorgroup>'
    )

    # object XMLs
    obj_xmls: list[str] = []
    build_items: list[str] = []

    for obj_i, (mesh, fi) in enumerate(zip(meshes, mesh_face_indices)):
        oid = OBJ_ID_START + obj_i
        mat = mesh.metadata.get('material', Material.SOIL)
        name = mat.name.lower() if isinstance(mat, Material) else f'mesh_{obj_i}'

        if len(mesh.faces) == 0:
            continue

        verts = mesh.vertices
        faces = mesh.faces

        # Vertex lines
        vert_lines = '\n          '.join(
            f'<vertex x="{v[0]:.5f}" y="{v[1]:.5f}" z="{v[2]:.5f}"/>'
            for v in verts
        )

        # Triangle lines — carry per-face colour via p1=p2=p3=index
        tri_lines = '\n          '.join(
            f'<triangle v1="{f[0]}" v2="{f[1]}" v3="{f[2]}"'
            f' pid="{COLOR_GID}" p1="{fi[i]}" p2="{fi[i]}" p3="{fi[i]}"/>'
            for i, f in enumerate(faces)
        )

        obj_xmls.append(
            f'    <object id="{oid}" name="{name}" type="model">\n'
            f'      <mesh>\n'
            f'        <vertices>\n'
            f'          {vert_lines}\n'
            f'        </vertices>\n'
            f'        <triangles>\n'
            f'          {tri_lines}\n'
            f'        </triangles>\n'
            f'      </mesh>\n'
            f'    </object>'
        )
        build_items.append(f'    <item objectid="{oid}"/>')

    model_xml = (
        "<?xml version='1.0' encoding='utf-8'?>\n"
        '<model unit="millimeter" xml:lang="en-US"\n'
        '       xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"\n'
        '       xmlns:m="http://schemas.microsoft.com/3dmanufacturing/material/2015/02">\n'
        '  <resources>\n'
        + colorgroup_xml + '\n'
        + '\n'.join(obj_xmls) + '\n'
        '  </resources>\n'
        '  <build>\n'
        + '\n'.join(build_items) + '\n'
        '  </build>\n'
        '</model>\n'
    )

    content_types = (
        "<?xml version='1.0' encoding='utf-8'?>\n"
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
        '  <Default Extension="rels"'
        ' ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\n'
        '  <Default Extension="model"'
        ' ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>\n'
        '</Types>\n'
    )

    rels = (
        "<?xml version='1.0' encoding='utf-8'?>\n"
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">\n'
        '  <Relationship Target="/3D/3dmodel.model" Id="rel0"'
        ' Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>\n'
        '</Relationships>\n'
    )

    out = pathlib.Path(path)
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr('[Content_Types].xml', content_types)
        zf.writestr('_rels/.rels', rels)
        zf.writestr('3D/3dmodel.model', model_xml)
