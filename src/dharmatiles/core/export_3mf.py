"""3MF export and Bambu X1C build-plate packing for DharmaTiles mesh output.

This module handles multi-tile 3MF project generation:

* :func:`export_3mf_colored` — write a Bambu Studio-compatible ``.3mf``
  containing one or more coloured tiles laid out on a 256 × 256 mm build plate.
* :func:`build_scene` — wrap coloured trimesh parts as a :class:`trimesh.Scene`.
* :func:`_pack_plates` — internal plate-packing algorithm (compact row packing).

Material tagging and the colour palette live in :mod:`core.color`.
"""
from __future__ import annotations

import pathlib

import numpy as np
import trimesh

from .color import Material


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


def _vert_lines_xml(verts: np.ndarray) -> str:
    """Vectorized: '<vertex x="…" y="…" z="…"/>' for every row of *verts* (N×3)."""
    xs = np.char.mod('%.5f', verts[:, 0])
    ys = np.char.mod('%.5f', verts[:, 1])
    zs = np.char.mod('%.5f', verts[:, 2])
    lines = np.char.add(
        '<vertex x="',
        np.char.add(xs, np.char.add(
            '" y="', np.char.add(ys, np.char.add(
                '" z="', np.char.add(zs, '"/>'),
            )),
        )),
    )
    return '\n     '.join(lines.tolist())


def _face_lines_xml(faces: np.ndarray) -> str:
    """Vectorized: '<triangle v1="…" v2="…" v3="…"/>' for every row of *faces* (F×3)."""
    v1s = np.char.mod('%d', faces[:, 0])
    v2s = np.char.mod('%d', faces[:, 1])
    v3s = np.char.mod('%d', faces[:, 2])
    lines = np.char.add(
        '<triangle v1="',
        np.char.add(v1s, np.char.add(
            '" v2="', np.char.add(v2s, np.char.add(
                '" v3="', np.char.add(v3s, '"/>'),
            )),
        )),
    )
    return '\n     '.join(lines.tolist())


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
            vert_lines = _vert_lines_xml(verts)
            tri_lines  = _face_lines_xml(faces)
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
