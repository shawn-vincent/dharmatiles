"""Material tagging and face-colour helpers for DharmaTiles mesh output.

Each mesh part produced by the pipeline is tagged with a :class:`Material`
enum value stored in ``mesh.metadata['material']``.  After per-material
grouping, :func:`tag` stamps uniform RGBA face colours so downstream
exporters (3MF scenes, colour-STL) carry the intended palette.

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
DEBUG_COLOR_* vivid palette      —         12-slot cycling palette for per-group debug
              (red, blue, green, orange, purple, cyan, yellow, pink, lime,
               amber, indigo, brown-orange)
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

#: All DEBUG_COLOR_* materials in label order (index = label mod 12).
DEBUG_COLORS: list[Material] = [
    Material.DEBUG_COLOR_0,  Material.DEBUG_COLOR_1,  Material.DEBUG_COLOR_2,
    Material.DEBUG_COLOR_3,  Material.DEBUG_COLOR_4,  Material.DEBUG_COLOR_5,
    Material.DEBUG_COLOR_6,  Material.DEBUG_COLOR_7,  Material.DEBUG_COLOR_8,
    Material.DEBUG_COLOR_9,  Material.DEBUG_COLOR_10, Material.DEBUG_COLOR_11,
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


def _terrain_group(name: str) -> int:
    """Sort key for terrain type: water=0, soil=1, grass/other=2."""
    n = name.lower()
    if 'water' in n:
        return 0
    if 'soil' in n:
        return 1
    return 2


def _size_dims(name: str) -> frozenset[int]:
    """Return the grid-square dimensions from a tile name like '1x1-grass'.

    '1x1' → frozenset({1}), '1x2' → frozenset({1, 2}), '3x3' → frozenset({3}).
    Falls back to frozenset({1}) for names without the NxM prefix.
    """
    import re as _re
    m = _re.match(r'^(\d+)x(\d+)', name.lower())
    if m:
        return frozenset({int(m.group(1)), int(m.group(2))})
    return frozenset({1})


def _pack_plates(
    tile_infos: list[tuple[str, float, float]],  # (name, w, h)
    usable_w: float,
    usable_h: float,
    gap_mm: float,
) -> list[list[tuple[int, float, float]]]:
    """Pack tiles onto virtual plates, choosing a row width that produces the
    most compact (square-ish, minimum-perimeter) cluster on each plate.

    Tiles are sorted by tile size (min dim, max dim), then terrain group, then
    footprint area descending.  A new plate is started when:

    * the incoming tile's size dimensions are disjoint from all dimensions
      seen so far on the current plate; or
    * the tiles overflow the plate height at every candidate row width.

    For each plate's batch of tiles the function tries all candidate target
    widths (one per possible "first-row tile count") and picks the width that
    minimises perimeter (total_w + total_h), using aspect ratio as a
    tiebreaker so truly square clusters win over elongated ones with equal
    perimeter.

    Returns a list of plates; each plate is a list of
    ``(orig_tile_idx, layout_x, layout_y)`` tuples where *layout_x/y* is the
    top-left corner of the tile in layout space (origin at top-left, y down).
    """
    def _sort_key(i: int) -> tuple:
        name, w, h = tile_infos[i]
        dims = _size_dims(name)
        return (min(dims), max(dims), _terrain_group(name), -(w * h))

    order = sorted(range(len(tile_infos)), key=_sort_key)

    # ── inner helpers ─────────────────────────────────────────────────────────

    def _row_pack(indices: list[int], max_row_w: float,
                  ) -> list[tuple[int, float, float]] | None:
        """Row-pack *indices* into one plate using *max_row_w* as the row
        limit.  Returns None if the layout overflows *usable_h*."""
        result: list[tuple[int, float, float]] = []
        rx = ry = rh = 0.0
        for j in indices:
            _, w, h = tile_infos[j]
            if result and rx + w > max_row_w + 1e-6:
                ry += rh + gap_mm
                rx = rh = 0.0
            if ry + h > usable_h + 1e-6:
                return None
            result.append((j, rx, ry))
            rx += w + gap_mm
            rh = max(rh, h)
        return result

    def _bbox(layout: list[tuple[int, float, float]]) -> tuple[float, float]:
        tw = max(lx + tile_infos[j][1] for j, lx, _ in layout)
        th = max(ly + tile_infos[j][2] for j, _, ly in layout)
        return tw, th

    def _best_for_batch(indices: list[int]) -> list[list[tuple[int, float, float]]]:
        """Find compact packing for a size-compatible batch; may return
        multiple plates if the batch overflows *usable_h*."""
        if not indices:
            return []

        # Candidate row widths: use exactly k tiles in the first row,
        # taking the k widest tiles (worst-case first-row width).
        sorted_w = sorted((tile_infos[j][1] for j in indices), reverse=True)
        candidates: list[float] = []
        acc = 0.0
        for k, w in enumerate(sorted_w, 1):
            acc += w + (gap_mm if k > 1 else 0.0)
            if acc <= usable_w + 1e-6:
                candidates.append(acc)

        best_layout: list[tuple[int, float, float]] | None = None
        best_score = (float('inf'), float('inf'))

        for max_w in candidates:
            layout = _row_pack(indices, max_w)
            if layout is None:
                continue
            tw, th = _bbox(layout)
            perim  = tw + th
            aspect = max(tw, th) / max(min(tw, th), 1e-6)
            if (perim, aspect) < best_score:
                best_score  = (perim, aspect)
                best_layout = layout

        if best_layout is not None:
            return [best_layout]

        # Every width overflows usable_h → split batch roughly in half and
        # recurse so each half gets its own plate.
        mid = max(1, len(indices) // 2)
        return _best_for_batch(indices[:mid]) + _best_for_batch(indices[mid:])

    # ── main loop: accumulate size-compatible batches then pack each ──────────
    plates: list[list[tuple[int, float, float]]] = []
    batch:      list[int] = []
    batch_dims: set[int]  = set()

    for j in order:
        name      = tile_infos[j][0]
        tile_dims = _size_dims(name)

        if batch and tile_dims.isdisjoint(batch_dims):
            plates.extend(_best_for_batch(batch))
            batch      = []
            batch_dims = set()

        batch.append(j)
        batch_dims |= tile_dims

    if batch:
        plates.extend(_best_for_batch(batch))

    return plates


def export_3mf_colored(
    tiles: list[list[trimesh.Trimesh]],
    path:  str | pathlib.Path,
    *,
    names:            list[str] | None = None,
    plate_mm:         float = 256.0,
    gap_mm:           float = 1.0,
    front_margin_mm:  float = 12.0,
    edge_margin_mm:   float = 5.0,
) -> None:
    """Export multiple coloured tiles as a Bambu Studio-compatible 3MF project.

    *tiles* is a list of tiles; each tile is a list of trimesh parts with
    ``mesh.metadata['material']`` set.  *names* (same length as *tiles*)
    provides tile names used for terrain-group ordering and layout.

    Tiles are laid out on a 256 × 256 mm build plate (Bambu X1C) with the
    requested *gap_mm* between them, centred on the plate.  A *front_margin_mm*
    keep-out zone is honoured at the front edge (Y = 0) to avoid the X1C purge
    and filament-cutter areas.  If tiles overflow one plate they are split by
    terrain type onto additional plates within the same 3MF file.

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
    from collections import defaultdict as _dd

    # ── Filament slots ────────────────────────────────────────────────────────
    _EXT: dict[Material, int] = {
        Material.BASE:    1,
        Material.SOIL:    2,
        Material.ROCK:    3,
        Material.GRASS:   4,
        Material.WATER:   5,
        Material.FLOWER:  6,
        Material.WOOD:    7,
        Material.FOLIAGE: 8,
    }
    _SLOT_HEX = [
        "#2D2D2D",  # slot 1 — BASE
        "#692C0C",  # slot 2 — SOIL
        "#485C80",  # slot 3 — ROCK
        "#2A941C",  # slot 4 — GRASS
        "#1485D5",  # slot 5 — WATER
        "#F5C300",  # slot 6 — FLOWER (golden yellow)
        "#8B633F",  # slot 7 — WOOD   (light warm brown)
        "#175C0D",  # slot 8 — FOLIAGE (dark forest green)
    ]

    # ── Filter empty tiles ────────────────────────────────────────────────────
    tile_parts: list[list[trimesh.Trimesh]] = [
        [m for m in ms if len(m.faces) > 0] for ms in tiles
    ]
    tile_parts = [ms for ms in tile_parts if ms]
    N = len(tile_parts)
    if N == 0:
        return

    _names = list(names) if names and len(names) == len(tiles) else [f"tile_{i}" for i in range(len(tiles))]
    # Re-align names to filtered tile_parts (same filter as above)
    _names = [_names[i] for i, ms in enumerate(tiles) if any(len(m.faces) > 0 for m in ms)]

    # ── Per-tile XY/Z bounds ──────────────────────────────────────────────────
    # tile_bounds[i] = (x_min, y_min, x_max, y_max, z_min)
    tile_bounds: list[tuple[float, float, float, float, float]] = []
    for ms in tile_parts:
        v = np.concatenate([m.vertices for m in ms])
        tile_bounds.append((
            float(v[:, 0].min()), float(v[:, 1].min()),
            float(v[:, 0].max()), float(v[:, 1].max()),
            float(v[:, 2].min()),
        ))

    tile_footprints = [
        (_names[i], b[2] - b[0], b[3] - b[1])
        for i, b in enumerate(tile_bounds)
    ]

    # ── Pack tiles onto plates ────────────────────────────────────────────────
    # Plate coordinate system (Bambu Studio / BBS 3MF):
    #   origin = front-left corner of build plate
    #   X increases right, Y increases toward back, Z increases up
    #   Front edge = Y=0, back edge = Y=plate_mm
    usable_x0 = edge_margin_mm
    usable_x1 = plate_mm - edge_margin_mm
    usable_y0 = front_margin_mm          # front keep-out
    usable_y1 = plate_mm - edge_margin_mm
    usable_w  = usable_x1 - usable_x0
    usable_h  = usable_y1 - usable_y0

    plate_groups = _pack_plates(tile_footprints, usable_w, usable_h, gap_mm)

    # ── Compute per-tile (tx, ty, tz) plate transforms ────────────────────────
    # Layout space: origin top-left of usable area, y increases downward (toward front).
    # Plate space:  origin front-left of plate, y increases toward back.
    # Mapping from layout (lx, ly) to plate (px, py):
    #   px = usable_x0 + x_center_offset + lx
    #   py = plate_y_back_of_grid - ly - row_h   (bottom-aligned rows)
    # where plate_y_back = usable_y0 + usable_h - (usable_h - total_h)/2 = usable_y1 - (usable_h - total_h)/2

    tile_transform: list[tuple[float, float, float]] = [(0.0, 0.0, 0.0)] * N
    tile_plate:     list[int]                         = [0] * N  # which plate (0-based) each tile is on

    for plate_idx, plate in enumerate(plate_groups):
        # Row heights (max tile height per layout-y level, for bottom alignment)
        row_heights: dict[float, float] = _dd(float)
        for orig_idx, _lx, ly in plate:
            _, _, h = tile_footprints[orig_idx]
            row_heights[ly] = max(row_heights[ly], h)

        # Total bounding box of this plate's layout
        total_w = max(_lx + tile_footprints[j][1] for j, _lx, _ly in plate)
        total_h = sum(row_heights.values()) + gap_mm * (len(row_heights) - 1)

        # Centre the grid horizontally; centre vertically within usable area
        x_offset = usable_x0 + (usable_w - total_w) / 2
        usable_cy = (usable_y0 + usable_y1) / 2
        # Back edge of the grid in plate Y coords
        plate_y_back = usable_cy + total_h / 2
        plate_y_back = min(plate_y_back, usable_y1)

        for orig_idx, lx, ly in plate:
            bx_min, by_min, _, _, bz_min = tile_bounds[orig_idx]
            _, _, h = tile_footprints[orig_idx]
            rh = row_heights[ly]

            # Bottom-align tiles within each row
            px = x_offset + lx
            py = plate_y_back - ly - rh        # front edge of row in plate Y
            py = max(py, usable_y0)            # clamp to front margin

            tx = px - bx_min
            ty = py - by_min
            tz = -bz_min if bz_min < 0.0 else 0.0
            tile_transform[orig_idx] = (tx, ty, tz)
            tile_plate[orig_idx] = plate_idx

    # ── ID assignment ─────────────────────────────────────────────────────────
    # Part IDs 1..P (cumulative across all tiles), assembly IDs P+1..P+N.
    tile_part_start: list[int] = []
    gpart = 1
    for ms in tile_parts:
        tile_part_start.append(gpart)
        gpart += len(ms)
    P = gpart - 1
    ASSEMBLY_IDS = [P + 1 + ti for ti in range(N)]

    def _uuid(n: int) -> str:
        return str(_uuid_mod.UUID(int=n % (2 ** 128)))

    BUILD_UUID = _uuid(0)
    ASSM_UUID  = [_uuid(0x1000_0000 + ti) for ti in range(N)]
    ITEM_UUID  = [_uuid(0x2000_0000 + ti) for ti in range(N)]

    def _part_uuid(global_part_id: int) -> str:
        return _uuid(0x0001_0000 + global_part_id)

    id_transform = "1 0 0 0 1 0 0 0 1 0 0 0"
    id_matrix_16 = "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1"

    # ── 3D/Objects/object_1.model — all geometry ─────────────────────────────
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

    obj_xmls: list[str] = []
    for ti, ms in enumerate(tile_parts):
        for pi, mesh in enumerate(ms):
            oid   = tile_part_start[ti] + pi
            verts = mesh.vertices
            faces = mesh.faces
            # Build vertex/triangle XML lines from a single .tolist() call each
            # (one C-level array→list conversion rather than three column slices).
            vert_rows = verts.tolist()
            vert_lines = '\n     '.join(
                f'<vertex x="{r[0]:.5f}" y="{r[1]:.5f}" z="{r[2]:.5f}"/>'
                for r in vert_rows
            )
            face_rows = faces.tolist()
            tri_lines = '\n     '.join(
                f'<triangle v1="{r[0]}" v2="{r[1]}" v3="{r[2]}"/>'
                for r in face_rows
            )
            obj_xmls.append(
                f'  <object id="{oid}" p:UUID="{_part_uuid(oid)}" type="model">\n'
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

    object_model_xml = (
        _model_hdr
        + ' <resources>\n'
        + '\n'.join(obj_xmls) + '\n'
        + ' </resources>\n'
        + ' <build/>\n'
        + '</model>\n'
    )

    # ── 3D/3dmodel.model — assembly wrapper ───────────────────────────────────
    assm_objs: list[str] = []
    for ti in range(N):
        comp_lines = '\n    '.join(
            f'<component p:path="/3D/Objects/object_1.model"'
            f' objectid="{tile_part_start[ti] + pi}"'
            f' p:UUID="{_part_uuid(tile_part_start[ti] + pi)}"'
            f' transform="{id_transform}"/>'
            for pi in range(len(tile_parts[ti]))
        )
        assm_objs.append(
            f'  <object id="{ASSEMBLY_IDS[ti]}" p:UUID="{ASSM_UUID[ti]}" type="model">\n'
            f'   <components>\n'
            f'    {comp_lines}\n'
            f'   </components>\n'
            f'  </object>'
        )

    def _item_transform(ti: int) -> str:
        tx, ty, tz = tile_transform[ti]
        # Plate N tiles are offset by N * plate_mm in X so each plate occupies a
        # distinct slice of global 3D space.  Bambu Studio uses the plate sections
        # in model_settings.config to map each item to its virtual plate.
        tx += tile_plate[ti] * plate_mm
        return f"1 0 0 0 1 0 0 0 1 {tx:.6f} {ty:.6f} {tz:.6f}"

    item_lines = '\n  '.join(
        f'<item objectid="{ASSEMBLY_IDS[ti]}" p:UUID="{ITEM_UUID[ti]}"'
        f' transform="{_item_transform(ti)}" printable="1"/>'
        for ti in range(N)
    )

    assembly_xml = (
        _model_hdr
        + ' <metadata name="Application">BambuStudio-02.04.00.70</metadata>\n'
        + ' <resources>\n'
        + '\n'.join(assm_objs) + '\n'
        + ' </resources>\n'
        + f' <build p:UUID="{BUILD_UUID}">\n'
        + f'  {item_lines}\n'
        + ' </build>\n'
        + '</model>\n'
    )

    # ── Metadata/model_settings.config ───────────────────────────────────────
    obj_sections:   list[str] = []
    assemble_items: list[str] = []
    for ti, ms in enumerate(tile_parts):
        aid    = ASSEMBLY_IDS[ti]
        ti_fc  = sum(len(m.faces) for m in ms)
        tx, ty, tz = tile_transform[ti]
        assm_t = f"1 0 0 0 1 0 0 0 1 {tx:.6f} {ty:.6f} {tz:.6f}"
        part_entries = []
        for pi, mesh in enumerate(ms):
            oid      = tile_part_start[ti] + pi
            mat      = mesh.metadata.get('material', Material.SOIL)
            extruder = _EXT.get(mat, 2)
            pname    = mat.name.lower() if isinstance(mat, Material) else f'part_{oid}'
            fc       = len(mesh.faces)
            part_entries.append(
                f'    <part id="{oid}" subtype="normal_part">\n'
                f'      <metadata key="name" value="{pname}"/>\n'
                f'      <metadata key="matrix" value="{id_matrix_16}"/>\n'
                f'      <metadata key="extruder" value="{extruder}"/>\n'
                f'      <mesh_stat face_count="{fc}" edges_fixed="0"'
                f' degenerate_facets="0" facets_removed="0"'
                f' facets_reversed="0" backwards_edges="0"/>\n'
                f'    </part>'
            )
        obj_sections.append(
            f'  <object id="{aid}">\n'
            f'    <metadata key="name" value="{_names[ti]}"/>\n'
            f'    <metadata key="extruder" value="1"/>\n'
            f'    <metadata face_count="{ti_fc}"/>\n'
            + '\n'.join(part_entries) + '\n'
            + f'  </object>'
        )
        assemble_items.append(
            f'   <assemble_item object_id="{aid}" instance_id="0"'
            f' transform="{assm_t}" offset="0 0 0" />'
        )

    plate_sections: list[str] = []
    identify_id = 1
    for plate_idx, plate in enumerate(plate_groups):
        instances = []
        for orig_idx, _, _ in plate:
            aid = ASSEMBLY_IDS[orig_idx]
            instances.append(
                f'    <model_instance>\n'
                f'      <metadata key="object_id" value="{aid}"/>\n'
                f'      <metadata key="instance_id" value="0"/>\n'
                f'      <metadata key="identify_id" value="{identify_id}"/>\n'
                f'    </model_instance>'
            )
            identify_id += 1
        plate_sections.append(
            f'  <plate>\n'
            f'    <metadata key="plater_id" value="{plate_idx + 1}"/>\n'
            f'    <metadata key="plater_name" value=""/>\n'
            f'    <metadata key="locked" value="false"/>\n'
            f'    <metadata key="filament_map_mode" value="Auto For Flush"/>\n'
            + '\n'.join(instances) + '\n'
            + '  </plate>'
        )

    model_settings_xml = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<config>\n'
        + '\n'.join(obj_sections) + '\n'
        + '\n'.join(plate_sections) + '\n'
        + '  <assemble>\n'
        + '\n'.join(assemble_items) + '\n'
        + '  </assemble>\n'
        + '</config>\n'
    )

    # ── Metadata/project_settings.config ─────────────────────────────────────
    _tmpl_path = pathlib.Path(__file__).parent.parent / "assets" / "bambu_project_template.json"
    project_settings = _json.loads(_tmpl_path.read_text(encoding="utf-8"))
    project_settings["filament_colour"]       = _SLOT_HEX
    project_settings["filament_multi_colour"] = _SLOT_HEX

    # ── Package ───────────────────────────────────────────────────────────────
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
