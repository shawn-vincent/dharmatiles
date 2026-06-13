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
WATER    bluish-turquoise #149BD2   water volume mesh
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
    Material.WATER: ( 20, 133, 213, 255),   # blue-turquoise
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
    """Export coloured mesh parts as a Bambu Studio-compatible 3MF project.

    Generates the Bambu Studio / PrusaSlicer project format:

    * ``3D/Objects/object_1.model`` — mesh geometry (one object per material part)
    * ``3D/3dmodel.model`` — assembly wrapper using the 3MF Production Extension
    * ``3D/_rels/3dmodel.model.rels`` — cross-part reference
    * ``Metadata/model_settings.config`` — per-part extruder (filament slot) assignments
    * ``Metadata/project_settings.config`` — filament colour definitions

    Filament slot assignments (1-indexed AMS slots):

    ======= ===== =================
    Slot    Mat   Colour
    ======= ===== =================
    1       BASE  #2D2D2D dark gray
    2       SOIL  #692C0C brown
    3       ROCK  #485C80 slate blue
    4       GRASS #2A941C green
    5       WATER #1485D5 turquoise
    ======= ===== =================
    """
    import json as _json
    import uuid as _uuid_mod
    import zipfile

    # ── Filament slot configuration ───────────────────────────────────────────
    # Material → AMS filament slot (1-indexed).
    _EXT: dict[Material, int] = {
        Material.BASE:  1,
        Material.SOIL:  2,
        Material.ROCK:  3,
        Material.GRASS: 4,
        Material.WATER: 5,
    }
    # Filament colours for project_settings, indexed by (slot - 1).
    # Match RGBA palette; use 6-digit #RRGGBB (Bambu ignores alpha).
    _SLOT_HEX = [
        "#2D2D2D",  # slot 1 — BASE  dark gray
        "#692C0C",  # slot 2 — SOIL  reddish-brown
        "#485C80",  # slot 3 — ROCK  slate blue-gray
        "#2A941C",  # slot 4 — GRASS deep green
        "#1485D5",  # slot 5 — WATER blue-turquoise
    ]
    N_SLOTS = len(_SLOT_HEX)

    # ── Filter empty meshes ───────────────────────────────────────────────────
    parts: list[trimesh.Trimesh] = [m for m in meshes if len(m.faces) > 0]
    if not parts:
        return
    N = len(parts)

    # ── ID and UUID helpers ───────────────────────────────────────────────────
    # Part object IDs in object_1.model: 1 .. N
    # Assembly wrapper ID in 3dmodel.model: N + 1
    ASSEMBLY_ID = N + 1

    def _uuid(n: int) -> str:
        """Deterministic UUID from an integer seed."""
        return str(_uuid_mod.UUID(int=n))

    ASSEMBLY_UUID = _uuid(0x0000_0001)
    BUILD_UUID    = _uuid(0x0000_0000)
    ITEM_UUID     = _uuid(0x0000_0100)

    def _part_uuid(idx: int) -> str:
        return _uuid(0x0001_0000 + idx)

    # ── Z-lift for assemble transform (put bottom of tile at build-plate z=0) ─
    min_z = min(float(m.vertices[:, 2].min()) for m in parts)
    lift_z = -min_z if min_z < 0.0 else 0.0
    id_transform   = "1 0 0 0 1 0 0 0 1 0 0 0"
    assm_transform = f"1 0 0 0 1 0 0 0 1 0 0 {lift_z:.6f}"
    id_matrix_16   = "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1"   # 4×4 identity, row-major

    # ── 3D/Objects/object_1.model — mesh geometry ─────────────────────────────
    # Triangles carry NO colour attributes; colour comes from model_settings.
    obj_xmls: list[str] = []
    for idx, mesh in enumerate(parts):
        oid   = idx + 1
        puuid = _part_uuid(idx)
        verts = mesh.vertices
        faces = mesh.faces
        vx, vy, vz = verts[:, 0].tolist(), verts[:, 1].tolist(), verts[:, 2].tolist()
        v1l,  v2l,  v3l  = faces[:, 0].tolist(), faces[:, 1].tolist(), faces[:, 2].tolist()

        vert_lines = '\n     '.join(
            f'<vertex x="{x:.5f}" y="{y:.5f}" z="{z:.5f}"/>'
            for x, y, z in zip(vx, vy, vz)
        )
        tri_lines = '\n     '.join(
            f'<triangle v1="{a}" v2="{b}" v3="{c}"/>'
            for a, b, c in zip(v1l, v2l, v3l)
        )
        obj_xmls.append(
            f'  <object id="{oid}" p:UUID="{puuid}" type="model">\n'
            f'   <mesh>\n'
            f'    <vertices>\n'
            f'     {vert_lines}\n'
            f'    </vertices>\n'
            f'    <triangles>\n'
            f'     {tri_lines}\n'
            f'    </triangles>\n'
            f'   </mesh>\n'
            f'  </object>'
        )

    NS_CORE  = 'http://schemas.microsoft.com/3dmanufacturing/core/2015/02'
    NS_BAMBU = 'http://schemas.bambulab.com/package/2021'
    NS_PROD  = 'http://schemas.microsoft.com/3dmanufacturing/production/2015/06'
    _model_hdr = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<model unit="millimeter" xml:lang="en-US"'
        f' xmlns="{NS_CORE}"'
        f' xmlns:BambuStudio="{NS_BAMBU}"'
        f' xmlns:p="{NS_PROD}"'
        f' requiredextensions="p">\n'
        f' <metadata name="BambuStudio:3mfVersion">1</metadata>\n'
    )

    object_model_xml = (
        _model_hdr
        + ' <resources>\n'
        + '\n'.join(obj_xmls) + '\n'
        + ' </resources>\n'
        + ' <build/>\n'
        + '</model>\n'
    )

    # ── 3D/3dmodel.model — assembly wrapper ───────────────────────────────────
    comp_lines = '\n    '.join(
        f'<component p:path="/3D/Objects/object_1.model"'
        f' objectid="{idx + 1}" p:UUID="{_part_uuid(idx)}"'
        f' transform="{id_transform}"/>'
        for idx in range(N)
    )

    assembly_xml = (
        _model_hdr
        # Bambu Studio checks this metadata to decide whether to load the full
        # project (model_settings + project_settings) or geometry only.
        # Without it, project_settings.config filament colours are ignored.
        + ' <metadata name="Application">BambuStudio-02.04.00.70</metadata>\n'
        + ' <resources>\n'
        + f'  <object id="{ASSEMBLY_ID}" p:UUID="{ASSEMBLY_UUID}" type="model">\n'
        + f'   <components>\n'
        + f'    {comp_lines}\n'
        + f'   </components>\n'
        + f'  </object>\n'
        + f' </resources>\n'
        + f' <build p:UUID="{BUILD_UUID}">\n'
        + f'  <item objectid="{ASSEMBLY_ID}" p:UUID="{ITEM_UUID}"'
        + f' transform="{assm_transform}" printable="1"/>\n'
        + f' </build>\n'
        + '</model>\n'
    )

    # ── Metadata/model_settings.config — extruder assignments ─────────────────
    total_faces = sum(len(m.faces) for m in parts)
    part_entries: list[str] = []
    for idx, mesh in enumerate(parts):
        mat      = mesh.metadata.get('material', Material.SOIL)
        extruder = _EXT.get(mat, 2)
        name     = mat.name.lower() if isinstance(mat, Material) else f'part_{idx + 1}'
        oid      = idx + 1
        fc       = len(mesh.faces)
        part_entries.append(
            f'    <part id="{oid}" subtype="normal_part">\n'
            f'      <metadata key="name" value="{name}"/>\n'
            f'      <metadata key="matrix" value="{id_matrix_16}"/>\n'
            f'      <metadata key="extruder" value="{extruder}"/>\n'
            f'      <mesh_stat face_count="{fc}"'
            f' edges_fixed="0" degenerate_facets="0"'
            f' facets_removed="0" facets_reversed="0" backwards_edges="0"/>\n'
            f'    </part>'
        )

    model_settings_xml = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<config>\n'
        f'  <object id="{ASSEMBLY_ID}">\n'
        f'    <metadata key="name" value="terrain"/>\n'
        f'    <metadata key="extruder" value="1"/>\n'
        f'    <metadata face_count="{total_faces}"/>\n'
        + '\n'.join(part_entries) + '\n'
        + '  </object>\n'
        + '  <plate>\n'
        + '    <metadata key="plater_id" value="1"/>\n'
        + '    <metadata key="plater_name" value=""/>\n'
        + '    <metadata key="locked" value="false"/>\n'
        + '    <metadata key="filament_map_mode" value="Auto For Flush"/>\n'
        + f'    <model_instance>\n'
        + f'      <metadata key="object_id" value="{ASSEMBLY_ID}"/>\n'
        + f'      <metadata key="instance_id" value="0"/>\n'
        + f'      <metadata key="identify_id" value="1"/>\n'
        + f'    </model_instance>\n'
        + '  </plate>\n'
        + '  <assemble>\n'
        + f'   <assemble_item object_id="{ASSEMBLY_ID}" instance_id="0"'
        + f' transform="{assm_transform}" offset="0 0 0" />\n'
        + '  </assemble>\n'
        + '</config>\n'
    )

    # ── Metadata/project_settings.config — full Bambu project settings ───────
    # Load the bundled template (503 keys matching what Bambu Studio expects),
    # then inject our palette colours.  Using the full template avoids the
    # "Customized Preset" dialog and "Default Filament" slot names that appear
    # when too many fields are missing.
    _tmpl_path = pathlib.Path(__file__).parent.parent / "assets" / "bambu_project_template.json"
    project_settings = _json.loads(_tmpl_path.read_text(encoding="utf-8"))
    project_settings["filament_colour"]       = _SLOT_HEX
    project_settings["filament_multi_colour"] = _SLOT_HEX

    # ── Package files ─────────────────────────────────────────────────────────
    content_types = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
        ' <Default Extension="rels"'
        ' ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\n'
        ' <Default Extension="model"'
        ' ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>\n'
        '</Types>\n'
    )

    root_rels = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">\n'
        ' <Relationship Target="/3D/3dmodel.model" Id="rel-1"'
        ' Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>\n'
        '</Relationships>\n'
    )

    model_rels = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">\n'
        ' <Relationship Target="/3D/Objects/object_1.model" Id="rel-1"'
        ' Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>\n'
        '</Relationships>\n'
    )

    out = pathlib.Path(path)
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as zf:
        zf.writestr('[Content_Types].xml',              content_types)
        zf.writestr('_rels/.rels',                      root_rels)
        zf.writestr('3D/3dmodel.model',                 assembly_xml)
        zf.writestr('3D/_rels/3dmodel.model.rels',      model_rels)
        zf.writestr('3D/Objects/object_1.model',        object_model_xml)
        zf.writestr('Metadata/model_settings.config',   model_settings_xml)
        zf.writestr('Metadata/project_settings.config',
                    _json.dumps(project_settings, indent=4))
