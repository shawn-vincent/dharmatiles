#!/usr/bin/env python3
"""Generate a craft-paint modular organizer STL.

The model is generated from signed-distance fields and meshed with a compact
marching-tetrahedra implementation, so it does not need external CAD or boolean
engines.  Dimensions are millimeters.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
from pathlib import Path

import numpy as np
from scipy.spatial import Delaunay
import trimesh

from dharmatiles.core.logo import _logo_contours_mm


DEFAULT_BALANCED_EDGE_WALL = 4.05
DEFAULT_BOX_ROUNDOVER = 1.9
DEFAULT_VERTICAL_CORNER_ROUNDOVER = 20.0
DEFAULT_HOLE_ROUNDOVER = 1.9
DEFAULT_MAGNET_DIAMETER = 10.0
DEFAULT_MAGNET_DEPTH = 3.0
DEFAULT_MAGNET_Z = 35.0
DEFAULT_MAGNET_SPACING = 80.9
DEFAULT_LOGO_SIZE = 36.0
DEFAULT_LOGO_DEPTH = 0.5
DEFAULT_LONG_SIDE_MAGNET_POSITIONS = (
    140.0 / 2.0 - DEFAULT_MAGNET_SPACING / 2.0,
    140.0 / 2.0 + DEFAULT_MAGNET_SPACING / 2.0,
)
DEFAULT_SHORT_SIDE_MAGNET_POSITIONS = (
    150.0 / 2.0 - DEFAULT_MAGNET_SPACING / 2.0,
    150.0 / 2.0 + DEFAULT_MAGNET_SPACING / 2.0,
)


@dataclass(frozen=True)
class OrganizerSpec:
    width: float = 140.0
    depth: float = 150.0
    height: float = 70.0
    columns: int = 3
    rows: int = 4
    hole_diameter: float = 35.0
    min_wall: float = 1.0
    edge_wall: float = DEFAULT_BALANCED_EDGE_WALL
    floor: float = 5.0
    box_roundover: float = DEFAULT_BOX_ROUNDOVER
    vertical_corner_roundover: float = DEFAULT_VERTICAL_CORNER_ROUNDOVER
    hole_roundover: float = DEFAULT_HOLE_ROUNDOVER
    magnet_diameter: float = DEFAULT_MAGNET_DIAMETER
    magnet_depth: float = DEFAULT_MAGNET_DEPTH
    magnet_z: float = DEFAULT_MAGNET_Z
    logo_size: float = DEFAULT_LOGO_SIZE
    logo_depth: float = DEFAULT_LOGO_DEPTH
    stagger: float = 8.0
    pitch: float = 1.0

    @property
    def roundover(self) -> float:
        return max(self.box_roundover, self.vertical_corner_roundover, self.hole_roundover)


_CUBE_CORNERS = np.array(
    [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ],
    dtype=np.float64,
)

_TETS = np.array(
    [
        [0, 5, 1, 6],
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
    ],
    dtype=np.int8,
)


def polygon_signed_area(points: np.ndarray) -> float:
    x = points[:, 0]
    y = points[:, 1]
    return float(0.5 * np.sum(x * np.roll(y, -1) - np.roll(x, -1) * y))


def circle_segments(diameter: float, segment_length: float = 1.5) -> int:
    return max(48, int(np.ceil(np.pi * diameter / segment_length)))


def point_in_polygon(point: np.ndarray, polygon: list[tuple[float, float]]) -> bool:
    x, y = point
    inside = False
    previous = len(polygon) - 1
    for current in range(len(polygon)):
        xi, yi = polygon[current]
        xj, yj = polygon[previous]
        if (yi > y) != (yj > y):
            x_intersect = (xj - xi) * (y - yi) / (yj - yi + 1e-18) + xi
            if x < x_intersect:
                inside = not inside
        previous = current
    return inside


def point_in_contours_evenodd(point: np.ndarray, contours: list[list[tuple[float, float]]]) -> bool:
    inside = False
    for contour in contours:
        if point_in_polygon(point, contour):
            inside = not inside
    return inside


def smooth_max(a: np.ndarray, b: np.ndarray, radius: float) -> np.ndarray:
    """Polynomial smooth maximum for rounded SDF subtraction."""
    if radius <= 0.0:
        return np.maximum(a, b)
    h = np.clip(0.5 + 0.5 * (a - b) / radius, 0.0, 1.0)
    return a * h + b * (1.0 - h) + radius * h * (1.0 - h)


def rounded_box_sdf(points: np.ndarray, spec: OrganizerSpec) -> np.ndarray:
    if spec.box_roundover <= 0.0:
        center = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
        half = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
        q = np.abs(points - center) - half
        outside = np.linalg.norm(np.maximum(q, 0.0), axis=1)
        inside = np.minimum(np.maximum.reduce(q, axis=1), 0.0)
        return outside + inside

    center = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
    half = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
    q = np.abs(points - center) - (half - spec.box_roundover)
    outside = np.linalg.norm(np.maximum(q, 0.0), axis=1)
    inside = np.minimum(np.maximum.reduce(q, axis=1), 0.0)
    return outside + inside - spec.box_roundover


def _hole_centers_for_edge_wall(spec: OrganizerSpec, edge_wall: float) -> list[tuple[float, float]]:
    radius = spec.hole_diameter / 2.0
    min_pitch = spec.hole_diameter + spec.min_wall
    stagger = max(0.0, spec.stagger)
    edge_margin = radius + edge_wall

    x_start = edge_margin + stagger
    x_stop = spec.width - edge_margin - stagger
    y_start = edge_margin
    y_stop = spec.depth - edge_margin

    if x_stop < x_start or y_stop < y_start:
        raise ValueError("Organizer is too small for the requested edge wall and stagger.")

    if spec.columns > 1 and (x_stop - x_start) / (spec.columns - 1) < min_pitch:
        raise ValueError("Organizer is too narrow for the requested holes and minimum wall.")

    xs = np.linspace(x_start, x_stop, spec.columns)
    ys = np.linspace(y_start, y_stop, spec.rows)

    centers: list[tuple[float, float]] = []
    for row, y in enumerate(ys):
        direction = -1.0 if row % 2 else 1.0
        for col, x in enumerate(xs):
            centers.append((float(x + direction * stagger), float(y)))

    if minimum_hole_wall(centers, spec.hole_diameter) < spec.min_wall:
        raise ValueError("Organizer is too small for the requested holes and minimum wall.")
    return centers


def minimum_hole_wall(centers: list[tuple[float, float]], hole_diameter: float) -> float:
    minimum = np.inf
    for index, a in enumerate(centers):
        for b in centers[index + 1:]:
            minimum = min(minimum, float(np.linalg.norm(np.subtract(a, b))))
    return minimum - hole_diameter


def balanced_edge_wall(spec: OrganizerSpec) -> float:
    radius = spec.hole_diameter / 2.0
    stagger = max(0.0, spec.stagger)
    max_x_edge = (spec.width - spec.hole_diameter - 2.0 * stagger) / 2.0
    max_y_edge = (spec.depth - spec.hole_diameter) / 2.0
    high = max(0.0, min(max_x_edge, max_y_edge))

    best_edge = spec.min_wall / 2.0
    best_error = np.inf
    for edge_wall in np.linspace(0.0, high, 1001):
        try:
            centers = _hole_centers_for_edge_wall(spec, float(edge_wall))
        except ValueError:
            continue
        hole_wall = minimum_hole_wall(centers, spec.hole_diameter)
        if hole_wall < spec.min_wall:
            continue
        error = abs(hole_wall - edge_wall)
        if error < best_error:
            best_edge = float(edge_wall)
            best_error = float(error)
    return best_edge


def hole_centers(spec: OrganizerSpec) -> list[tuple[float, float]]:
    return _hole_centers_for_edge_wall(spec, spec.edge_wall)


def rectangle_boundary(width: float, depth: float, segment_length: float) -> list[tuple[float, float]]:
    points: list[tuple[float, float]] = []
    corners = [(0.0, 0.0), (width, 0.0), (width, depth), (0.0, depth)]
    for start, end in zip(corners, corners[1:] + corners[:1]):
        x0, y0 = start
        x1, y1 = end
        length = float(np.linalg.norm(np.subtract(end, start)))
        count = max(1, int(np.ceil(length / segment_length)))
        for index in range(count):
            t = index / count
            points.append((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t))
    return points


def rounded_rectangle_ring(
    width: float,
    depth: float,
    inset: float,
    corner_radius: float,
    straight_segments: int,
    arc_segments: int,
) -> list[tuple[float, float]]:
    min_x = inset
    min_y = inset
    max_x = width - inset
    max_y = depth - inset
    radius = min(corner_radius, (max_x - min_x) / 2.0, (max_y - min_y) / 2.0)
    straight_segments = max(1, straight_segments)
    arc_segments = max(2, arc_segments)

    points: list[tuple[float, float]] = []

    def add_line(start: tuple[float, float], end: tuple[float, float]) -> None:
        for index in range(straight_segments):
            t = index / straight_segments
            points.append((start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t))

    def add_arc(center: tuple[float, float], start_angle: float, end_angle: float) -> None:
        for index in range(arc_segments):
            t = index / arc_segments
            angle = start_angle + (end_angle - start_angle) * t
            points.append((center[0] + radius * np.cos(angle), center[1] + radius * np.sin(angle)))

    add_line((min_x + radius, min_y), (max_x - radius, min_y))
    add_arc((max_x - radius, min_y + radius), -np.pi / 2.0, 0.0)
    add_line((max_x, min_y + radius), (max_x, max_y - radius))
    add_arc((max_x - radius, max_y - radius), 0.0, np.pi / 2.0)
    add_line((max_x - radius, max_y), (min_x + radius, max_y))
    add_arc((min_x + radius, max_y - radius), np.pi / 2.0, np.pi)
    add_line((min_x, max_y - radius), (min_x, min_y + radius))
    add_arc((min_x + radius, min_y + radius), np.pi, 3.0 * np.pi / 2.0)
    return points


def rounded_rectangle_sdf_2d(
    point: np.ndarray,
    width: float,
    depth: float,
    inset: float,
    corner_radius: float,
) -> float:
    center = np.array([width / 2.0, depth / 2.0])
    half = np.array([width / 2.0 - inset, depth / 2.0 - inset])
    radius = min(corner_radius, half[0], half[1])
    q = np.abs(point - center) - (half - radius)
    outside = np.linalg.norm(np.maximum(q, 0.0))
    inside = min(max(q[0], q[1]), 0.0)
    return float(outside + inside - radius)


def connect_rings(builder: MeshBuilder, lower: list[int], upper: list[int], reverse: bool = False) -> None:
    for lower_a, lower_b, upper_a, upper_b in zip(lower, lower[1:] + lower[:1], upper, upper[1:] + upper[:1]):
        if reverse:
            builder.face(lower_a, upper_b, lower_b)
            builder.face(lower_a, upper_a, upper_b)
        else:
            builder.face(lower_a, lower_b, upper_b)
            builder.face(lower_a, upper_b, upper_a)


def magnet_pockets(spec: OrganizerSpec) -> list[tuple[str, float, float]]:
    pockets: list[tuple[str, float, float]] = []
    for x in DEFAULT_LONG_SIDE_MAGNET_POSITIONS:
        pockets.append(("front", x, spec.magnet_z))
        pockets.append(("back", x, spec.magnet_z))
    for y in DEFAULT_SHORT_SIDE_MAGNET_POSITIONS:
        pockets.append(("left", y, spec.magnet_z))
        pockets.append(("right", y, spec.magnet_z))
    return pockets


def flat_side_segment(point_a: tuple[float, float], point_b: tuple[float, float], spec: OrganizerSpec) -> bool:
    eps = 1e-6
    corner = spec.vertical_corner_roundover
    ax, ay = point_a
    bx, by = point_b
    if abs(ay) < eps and abs(by) < eps and corner - eps <= ax <= spec.width - corner + eps:
        return True
    if abs(ay - spec.depth) < eps and abs(by - spec.depth) < eps and corner - eps <= ax <= spec.width - corner + eps:
        return True
    if abs(ax) < eps and abs(bx) < eps and corner - eps <= ay <= spec.depth - corner + eps:
        return True
    if abs(ax - spec.width) < eps and abs(bx - spec.width) < eps and corner - eps <= ay <= spec.depth - corner + eps:
        return True
    return False


def connect_outer_rings(
    builder: MeshBuilder,
    lower_ring: list[int],
    upper_ring: list[int],
    lower_points: list[tuple[float, float]],
    upper_points: list[tuple[float, float]],
    lower_z: float,
    upper_z: float,
    spec: OrganizerSpec,
) -> None:
    mid_span = lower_z >= spec.box_roundover - 1e-6 and upper_z <= spec.height - spec.box_roundover + 1e-6
    for index, (lower_a, lower_b, upper_a, upper_b) in enumerate(
        zip(lower_ring, lower_ring[1:] + lower_ring[:1], upper_ring, upper_ring[1:] + upper_ring[:1])
    ):
        point_a = lower_points[index]
        point_b = lower_points[(index + 1) % len(lower_points)]
        if mid_span and flat_side_segment(point_a, point_b, spec):
            continue
        builder.face(lower_a, lower_b, upper_b)
        builder.face(lower_a, upper_b, upper_a)


class MeshBuilder:
    def __init__(self) -> None:
        self.vertices: list[tuple[float, float, float]] = []
        self.faces: list[list[int]] = []
        self._vertex_index: dict[tuple[float, float, float], int] = {}

    def vertex(self, x: float, y: float, z: float) -> int:
        key = (round(float(x), 6), round(float(y), 6), round(float(z), 6))
        existing = self._vertex_index.get(key)
        if existing is not None:
            return existing
        index = len(self.vertices)
        self.vertices.append(key)
        self._vertex_index[key] = index
        return index

    def face(self, a: int, b: int, c: int) -> None:
        if a != b and b != c and c != a:
            self.faces.append([a, b, c])

    def mesh(self) -> trimesh.Trimesh:
        mesh = trimesh.Trimesh(
            vertices=np.asarray(self.vertices, dtype=np.float64),
            faces=np.asarray(self.faces, dtype=np.int64),
            process=False,
        )
        mesh.remove_unreferenced_vertices()
        trimesh.repair.fill_holes(mesh)
        mesh = fill_planar_boundary_loops(mesh, z_value=float(np.max(mesh.vertices[:, 2])))
        mesh.fix_normals()
        if mesh.volume < 0:
            mesh.invert()
        return mesh


def point_in_triangle(point: np.ndarray, triangle: np.ndarray, tolerance: float = 1e-9) -> bool:
    a, b, c = triangle
    v0 = c - a
    v1 = b - a
    v2 = point - a
    dot00 = float(np.dot(v0, v0))
    dot01 = float(np.dot(v0, v1))
    dot02 = float(np.dot(v0, v2))
    dot11 = float(np.dot(v1, v1))
    dot12 = float(np.dot(v1, v2))
    denom = dot00 * dot11 - dot01 * dot01
    if abs(denom) < tolerance:
        return False
    inv = 1.0 / denom
    u = (dot11 * dot02 - dot01 * dot12) * inv
    v = (dot00 * dot12 - dot01 * dot02) * inv
    return u >= -tolerance and v >= -tolerance and u + v <= 1.0 + tolerance


def ear_clip_loop(loop: list[int], vertices: np.ndarray) -> list[list[int]]:
    if len(loop) < 3:
        return []

    polygon = loop[:]
    xy = vertices[polygon, :2]
    if polygon_signed_area(xy) < 0.0:
        polygon.reverse()

    triangles: list[list[int]] = []
    guard = 0
    while len(polygon) > 3 and guard < len(loop) * len(loop):
        guard += 1
        clipped = False
        for index, current in enumerate(polygon):
            previous = polygon[index - 1]
            following = polygon[(index + 1) % len(polygon)]
            tri_xy = vertices[[previous, current, following], :2]
            if polygon_signed_area(tri_xy) <= 1e-9:
                continue
            if any(
                vertex not in {previous, current, following}
                and point_in_triangle(vertices[vertex, :2], tri_xy)
                for vertex in polygon
            ):
                continue
            triangles.append([previous, current, following])
            del polygon[index]
            clipped = True
            break
        if not clipped:
            break

    if len(polygon) == 3:
        triangles.append(polygon[:])
    return triangles


def fill_planar_boundary_loops(mesh: trimesh.Trimesh, z_value: float, tolerance: float = 1e-6) -> trimesh.Trimesh:
    edges, counts = np.unique(mesh.edges_sorted, axis=0, return_counts=True)
    planar_edges = [
        (int(a), int(b))
        for a, b in edges[counts == 1]
        if abs(mesh.vertices[a, 2] - z_value) <= tolerance and abs(mesh.vertices[b, 2] - z_value) <= tolerance
    ]
    if not planar_edges:
        return mesh

    adjacency: dict[int, list[int]] = {}
    for a, b in planar_edges:
        adjacency.setdefault(a, []).append(b)
        adjacency.setdefault(b, []).append(a)

    loops: list[list[int]] = []
    used_edges: set[tuple[int, int]] = set()
    for start, _ in planar_edges:
        if all(tuple(sorted((start, neighbor))) in used_edges for neighbor in adjacency[start]):
            continue

        loop = [start]
        previous: int | None = None
        current = start
        while True:
            candidates = [
                vertex
                for vertex in adjacency[current]
                if vertex != previous and tuple(sorted((current, vertex))) not in used_edges
            ]
            if not candidates:
                break
            following = candidates[0]
            used_edges.add(tuple(sorted((current, following))))
            previous, current = current, following
            if current == start:
                break
            loop.append(current)
        if len(loop) >= 3:
            loops.append(loop)

    faces = mesh.faces.tolist()
    for loop in loops:
        faces.extend(ear_clip_loop(loop, mesh.vertices))

    return trimesh.Trimesh(vertices=mesh.vertices.copy(), faces=np.asarray(faces), process=False)


def add_support_points(spec: OrganizerSpec, points: list[tuple[float, float]], spacing: float) -> None:
    radius = spec.hole_diameter / 2.0
    centers = hole_centers(spec)
    xs = np.arange(spacing, spec.width, spacing)
    ys = np.arange(spacing, spec.depth, spacing)
    for x in xs:
        for y in ys:
            if any(np.linalg.norm(np.subtract((x, y), center)) <= radius + 0.25 for center in centers):
                continue
            points.append((float(x), float(y)))


def add_rounded_top_support_points(spec: OrganizerSpec, points: list[tuple[float, float]], spacing: float) -> None:
    top_hole_radius = spec.hole_diameter / 2.0 + spec.hole_roundover
    centers = hole_centers(spec)
    xs = np.arange(spacing, spec.width, spacing)
    ys = np.arange(spacing, spec.depth, spacing)
    for x in xs:
        for y in ys:
            point = np.array([x, y])
            if rounded_rectangle_sdf_2d(point, spec.width, spec.depth, spec.box_roundover, spec.vertical_corner_roundover) > -0.25:
                continue
            if any(np.linalg.norm(point - np.asarray(center)) <= top_hole_radius + 0.25 for center in centers):
                continue
            points.append((float(x), float(y)))


def top_corner_sectors(spec: OrganizerSpec) -> list[tuple[tuple[float, float], float, float]]:
    inset = spec.box_roundover
    radius = spec.vertical_corner_roundover
    return [
        ((spec.width - inset - radius, inset + radius), -np.pi / 2.0, 0.0),
        ((spec.width - inset - radius, spec.depth - inset - radius), 0.0, np.pi / 2.0),
        ((inset + radius, spec.depth - inset - radius), np.pi / 2.0, np.pi),
        ((inset + radius, inset + radius), np.pi, 3.0 * np.pi / 2.0),
    ]


def point_in_top_corner_sector(point: np.ndarray, spec: OrganizerSpec, tolerance: float = 1e-6) -> bool:
    radius = spec.vertical_corner_roundover
    for center, angle_min, angle_max in top_corner_sectors(spec):
        vector = point - np.asarray(center)
        angle = np.arctan2(vector[1], vector[0])
        if angle < -np.pi / 2.0:
            angle += 2.0 * np.pi
        if angle_min < -np.pi / 2.0:
            angle_min += 2.0 * np.pi
        if angle_max < angle_min:
            angle_max += 2.0 * np.pi
        check_angle = angle if angle >= angle_min - tolerance else angle + 2.0 * np.pi
        if angle_min - tolerance <= check_angle <= angle_max + tolerance and np.linalg.norm(vector) <= radius + tolerance:
            return True
    return False


def add_top_corner_fans(
    builder: MeshBuilder,
    spec: OrganizerSpec,
    top_outer_points: list[tuple[float, float]],
    top_vertex,
) -> None:
    radius = spec.vertical_corner_roundover
    for center, angle_min, angle_max in top_corner_sectors(spec):
        center_index = builder.vertex(center[0], center[1], spec.height)
        sector_points: list[tuple[float, tuple[float, float]]] = []
        for point in top_outer_points:
            vector = np.asarray(point) - np.asarray(center)
            distance = np.linalg.norm(vector)
            if distance > radius + 1e-5:
                continue
            angle = np.arctan2(vector[1], vector[0])
            if angle < -np.pi / 2.0:
                angle += 2.0 * np.pi
            local_min = angle_min
            local_max = angle_max
            if local_min < -np.pi / 2.0:
                local_min += 2.0 * np.pi
            if local_max < local_min:
                local_max += 2.0 * np.pi
            check_angle = angle if angle >= local_min - 1e-6 else angle + 2.0 * np.pi
            if local_min - 1e-6 <= check_angle <= local_max + 1e-6:
                sector_points.append((check_angle, point))

        sector_points.sort(key=lambda item: item[0])
        ordered = [point for _, point in sector_points]
        for point_a, point_b in zip(ordered, ordered[1:]):
            builder.face(center_index, top_vertex(point_a), top_vertex(point_b))


def side_panel_pockets(spec: OrganizerSpec, side: str) -> list[tuple[float, float]]:
    if side in {"front", "back"}:
        return [(position, spec.magnet_z) for position in DEFAULT_LONG_SIDE_MAGNET_POSITIONS]
    return [(position, spec.magnet_z) for position in DEFAULT_SHORT_SIDE_MAGNET_POSITIONS]


def front_logo_contours(spec: OrganizerSpec) -> list[list[tuple[float, float]]]:
    contours = _logo_contours_mm(spec.width / 2.0, spec.height / 2.0, spec.logo_size)
    points = [point for contour in contours for point in contour]
    bbox_center = (
        (min(point[0] for point in points) + max(point[0] for point in points)) / 2.0,
        (min(point[1] for point in points) + max(point[1] for point in points)) / 2.0,
    )
    dx = spec.width / 2.0 - bbox_center[0]
    dz = spec.height / 2.0 - bbox_center[1]
    return [[(x + dx, z + dz) for x, z in contour] for contour in contours]


def logo_inset_cutter(spec: OrganizerSpec) -> trimesh.Trimesh:
    import manifold3d as m3d

    contours = front_logo_contours(spec)
    # Build in the XZ plane as manifold XY, then rotate/translate so extrusion
    # spans slightly outside the front face into the organizer body.
    cross_section = m3d.CrossSection(contours, fillrule=m3d.FillRule.EvenOdd)
    solid = m3d.Manifold.extrude(cross_section, height=spec.logo_depth + 0.05)
    solid = solid.rotate((90.0, 0.0, 0.0)).translate((0.0, spec.logo_depth + 0.025, 0.0))
    raw = solid.to_mesh()
    return trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )


def apply_logo_inset(mesh: trimesh.Trimesh, spec: OrganizerSpec) -> trimesh.Trimesh:
    if spec.logo_size <= 0.0 or spec.logo_depth <= 0.0:
        return mesh
    result = trimesh.boolean.difference([mesh, logo_inset_cutter(spec)], engine="manifold")
    result.fix_normals()
    return result


def add_side_panel_with_magnets(
    builder: MeshBuilder,
    spec: OrganizerSpec,
    side: str,
    circle_segments_count: int,
) -> None:
    z_min = spec.box_roundover
    z_max = spec.height - spec.box_roundover
    corner = spec.vertical_corner_roundover
    magnet_radius = spec.magnet_diameter / 2.0
    pocket_depth = spec.magnet_depth

    if side in {"front", "back"}:
        u_min, u_max = corner, spec.width - corner
        fixed = 0.0 if side == "front" else spec.depth
        inward_sign = 1.0 if side == "front" else -1.0
        reverse_panel = side == "front"
    else:
        u_min, u_max = corner, spec.depth - corner
        fixed = 0.0 if side == "left" else spec.width
        inward_sign = 1.0 if side == "left" else -1.0
        reverse_panel = side == "right"

    outer_points = rounded_rectangle_ring(
        spec.width,
        spec.depth,
        0.0,
        spec.vertical_corner_roundover,
        max(8, int(np.ceil(max(spec.width, spec.depth) / 1.5 / 4.0))),
        12,
    )
    if side == "front":
        boundary_us = sorted({round(x, 6) for x, y in outer_points if abs(y) < 1e-6 and u_min - 1e-6 <= x <= u_max + 1e-6})
    elif side == "back":
        boundary_us = sorted({round(x, 6) for x, y in outer_points if abs(y - spec.depth) < 1e-6 and u_min - 1e-6 <= x <= u_max + 1e-6})
    elif side == "left":
        boundary_us = sorted({round(y, 6) for x, y in outer_points if abs(x) < 1e-6 and u_min - 1e-6 <= y <= u_max + 1e-6})
    else:
        boundary_us = sorted({round(y, 6) for x, y in outer_points if abs(x - spec.width) < 1e-6 and u_min - 1e-6 <= y <= u_max + 1e-6})

    boundary: list[tuple[float, float]] = []
    boundary.extend((u, z_min) for u in boundary_us)
    boundary.append((u_max, z_max))
    boundary.extend((u, z_max) for u in reversed(boundary_us))
    boundary.append((u_min, z_min))

    theta = np.linspace(0.0, 2.0 * np.pi, circle_segments_count, endpoint=False)
    pocket_rings_2d: list[list[tuple[float, float]]] = []
    all_points = list(boundary)
    for u_center, z_center in side_panel_pockets(spec, side):
        ring = [
            (float(u_center + magnet_radius * np.cos(angle)), float(z_center + magnet_radius * np.sin(angle)))
            for angle in theta
        ]
        pocket_rings_2d.append(ring)
        all_points.extend(ring)

    logo_contours: list[list[tuple[float, float]]] = []

    seen: set[tuple[float, float]] = set()
    unique: list[tuple[float, float]] = []
    for point in all_points:
        key = (round(point[0], 6), round(point[1], 6))
        if key not in seen:
            seen.add(key)
            unique.append(key)

    points_2d = np.asarray(unique, dtype=np.float64)
    triangles = Delaunay(points_2d).simplices

    def logo_surface_depth(u: float, z: float) -> float:
        return 0.0

    def vertex_for(u: float, z: float, depth: float | None = None) -> int:
        if depth is None:
            depth = logo_surface_depth(u, z)
        offset = inward_sign * depth
        if side in {"front", "back"}:
            return builder.vertex(u, fixed + offset, z)
        return builder.vertex(fixed + offset, u, z)

    for tri in triangles:
        uv = points_2d[tri]
        centroid = uv.mean(axis=0)
        if not (u_min - 1e-6 <= centroid[0] <= u_max + 1e-6 and z_min - 1e-6 <= centroid[1] <= z_max + 1e-6):
            continue
        if any(np.linalg.norm(centroid - np.asarray(center)) < magnet_radius - 1e-6 for center in side_panel_pockets(spec, side)):
            continue
        face = [vertex_for(*points_2d[i]) for i in tri]
        if polygon_signed_area(uv) < 0.0:
            face = [face[0], face[2], face[1]]
        if reverse_panel:
            face = [face[0], face[2], face[1]]
        builder.face(*face)

    for ring_2d in pocket_rings_2d:
        outer_ring = [vertex_for(u, z, 0.0) for u, z in ring_2d]
        inner_ring = [vertex_for(u, z, pocket_depth) for u, z in ring_2d]
        connect_rings(builder, outer_ring, inner_ring, reverse=side in {"front", "right"})

        center_u = float(np.mean([p[0] for p in ring_2d]))
        center_z = float(np.mean([p[1] for p in ring_2d]))
        back_center = vertex_for(center_u, center_z, pocket_depth)
        for a, b in zip(inner_ring, inner_ring[1:] + inner_ring[:1]):
            if side in {"front", "left"}:
                builder.face(back_center, b, a)
            else:
                builder.face(back_center, a, b)



def build_analytic_mesh(spec: OrganizerSpec, circle_segment_length: float = 1.5) -> trimesh.Trimesh:
    builder = MeshBuilder()
    radius = spec.hole_diameter / 2.0
    outer = rectangle_boundary(spec.width, spec.depth, circle_segment_length)
    top_point_index: dict[tuple[float, float], int] = {}

    def top_vertex(point: tuple[float, float]) -> int:
        key = (round(point[0], 6), round(point[1], 6))
        existing = top_point_index.get(key)
        if existing is not None:
            return existing
        index = builder.vertex(key[0], key[1], spec.height)
        top_point_index[key] = index
        return index

    def bottom_vertex(point: tuple[float, float], z: float = 0.0) -> int:
        return builder.vertex(point[0], point[1], z)

    outer_top = [top_vertex(p) for p in outer]
    outer_bottom = [bottom_vertex(p) for p in outer]

    for top_a, top_b, bottom_a, bottom_b in zip(
        outer_top, outer_top[1:] + outer_top[:1], outer_bottom, outer_bottom[1:] + outer_bottom[:1]
    ):
        builder.face(top_a, bottom_a, bottom_b)
        builder.face(top_a, bottom_b, top_b)

    bottom_center = builder.vertex(spec.width / 2.0, spec.depth / 2.0, 0.0)
    for bottom_a, bottom_b in zip(outer_bottom, outer_bottom[1:] + outer_bottom[:1]):
        builder.face(bottom_center, bottom_b, bottom_a)

    all_top_points = list(outer)
    ring_count = circle_segments(spec.hole_diameter, circle_segment_length)
    theta = np.linspace(0.0, 2.0 * np.pi, ring_count, endpoint=False)

    for cx, cy in hole_centers(spec):
        ring = [(float(cx + radius * np.cos(t)), float(cy + radius * np.sin(t))) for t in theta]
        all_top_points.extend(ring)

        top_ring = [top_vertex(p) for p in ring]
        floor_ring = [builder.vertex(p[0], p[1], spec.floor) for p in ring]
        floor_center = builder.vertex(cx, cy, spec.floor)

        for top_a, top_b, floor_a, floor_b in zip(
            top_ring, top_ring[1:] + top_ring[:1], floor_ring, floor_ring[1:] + floor_ring[:1]
        ):
            builder.face(top_a, top_b, floor_b)
            builder.face(top_a, floor_b, floor_a)

        for floor_a, floor_b in zip(floor_ring, floor_ring[1:] + floor_ring[:1]):
            builder.face(floor_center, floor_a, floor_b)

    add_rounded_top_support_points(spec, all_top_points, spacing=8.0)

    unique_points: list[tuple[float, float]] = []
    point_seen: set[tuple[float, float]] = set()
    for point in all_top_points:
        key = (round(point[0], 6), round(point[1], 6))
        if key not in point_seen:
            point_seen.add(key)
            unique_points.append(key)

    points_2d = np.asarray(unique_points, dtype=np.float64)
    triangles = Delaunay(points_2d).simplices
    centers = hole_centers(spec)

    for tri in triangles:
        xy = points_2d[tri]
        centroid = xy.mean(axis=0)
        if not (0.0 <= centroid[0] <= spec.width and 0.0 <= centroid[1] <= spec.depth):
            continue
        if any(np.linalg.norm(centroid - np.asarray(center)) < radius - 1e-6 for center in centers):
            continue

        face = [top_vertex(tuple(points_2d[i])) for i in tri]
        if polygon_signed_area(xy) < 0.0:
            face = [face[0], face[2], face[1]]
        builder.face(*face)

    return builder.mesh()


def roundover_z_samples(spec: OrganizerSpec, sections: int) -> list[float]:
    r = spec.box_roundover
    if r <= 0.0:
        return [0.0, spec.height]

    bottom = [r - r * np.cos(angle) for angle in np.linspace(0.0, np.pi / 2.0, sections + 1)]
    top = [spec.height - z for z in reversed(bottom)]
    mid = [r, spec.height - r] if spec.height > 2.0 * r else []
    return sorted({round(float(z), 6) for z in [*bottom, *mid, *top]})


def outer_inset_at_z(spec: OrganizerSpec, z: float) -> float:
    r = spec.box_roundover
    if r <= 0.0:
        return 0.0
    if z < r:
        return r - np.sqrt(max(0.0, r * r - (z - r) * (z - r)))
    if z > spec.height - r:
        return r - np.sqrt(max(0.0, r * r - (z - (spec.height - r)) * (z - (spec.height - r))))
    return 0.0


def hole_profile(spec: OrganizerSpec, sections: int) -> list[tuple[float, float]]:
    r = spec.hole_roundover
    radius = spec.hole_diameter / 2.0
    if r <= 0.0:
        return [(spec.floor, radius), (spec.height, radius)]

    if r >= radius:
        raise ValueError("Roundover must be smaller than the hole radius.")
    if spec.floor + r >= spec.height - r:
        raise ValueError("Roundover is too large for the requested floor and height.")

    floor_arc = []
    for angle in np.linspace(0.0, np.pi / 2.0, sections + 1):
        # Internal fillet at the blind-hole floor:
        # tangent to the floor at radius - r and to the vertical bore at floor + r.
        z = spec.floor + r - r * np.cos(angle)
        hole_radius = radius - r + r * np.sin(angle)
        floor_arc.append((float(z), float(hole_radius)))

    top_arc = []
    for angle in np.linspace(0.0, np.pi / 2.0, sections + 1):
        # External roundover at the top rim:
        # tangent to the vertical bore at height - r and to the top face at radius + r.
        z = spec.height - r + r * np.sin(angle)
        hole_radius = radius + r - r * np.cos(angle)
        top_arc.append((float(z), float(hole_radius)))

    profile = [*floor_arc, (spec.height - r, radius), *top_arc]
    deduped: list[tuple[float, float]] = []
    for z, hole_radius in profile:
        if deduped and abs(deduped[-1][0] - z) < 1e-6 and abs(deduped[-1][1] - hole_radius) < 1e-6:
            continue
        deduped.append((z, hole_radius))
    return deduped


def build_rounded_analytic_mesh(
    spec: OrganizerSpec,
    circle_segment_length: float = 1.5,
    roundover_sections: int = 6,
) -> trimesh.Trimesh:
    builder = MeshBuilder()
    box_r = spec.box_roundover
    corner_r = spec.vertical_corner_roundover
    hole_r = spec.hole_roundover
    if box_r <= 0.0 and hole_r <= 0.0:
        return build_analytic_mesh(spec, circle_segment_length)

    outer_straight_segments = max(8, int(np.ceil(max(spec.width, spec.depth) / circle_segment_length / 4.0)))
    outer_arc_segments = max(6, roundover_sections * 2)
    ring_count = circle_segments(spec.hole_diameter + 2.0 * hole_r, circle_segment_length)
    theta = np.linspace(0.0, 2.0 * np.pi, ring_count, endpoint=False)

    outer_rings: list[list[int]] = []
    outer_points_by_z: list[tuple[float, list[tuple[float, float]]]] = []
    for z in roundover_z_samples(spec, roundover_sections):
        inset = outer_inset_at_z(spec, z)
        points = rounded_rectangle_ring(spec.width, spec.depth, inset, corner_r, outer_straight_segments, outer_arc_segments)
        outer_points_by_z.append((z, points))
        outer_rings.append([builder.vertex(x, y, z) for x, y in points])

    for index, (lower, upper) in enumerate(zip(outer_rings, outer_rings[1:])):
        lower_z, lower_points = outer_points_by_z[index]
        upper_z, upper_points = outer_points_by_z[index + 1]
        connect_outer_rings(builder, lower, upper, lower_points, upper_points, lower_z, upper_z, spec)

    bottom_center = builder.vertex(spec.width / 2.0, spec.depth / 2.0, 0.0)
    for a, b in zip(outer_rings[0], outer_rings[0][1:] + outer_rings[0][:1]):
        builder.face(bottom_center, b, a)

    hole_profiles = hole_profile(spec, roundover_sections)
    top_hole_radius = hole_profiles[-1][1]
    top_point_index: dict[tuple[float, float], int] = {}

    def top_vertex(point: tuple[float, float]) -> int:
        key = (round(point[0], 6), round(point[1], 6))
        existing = top_point_index.get(key)
        if existing is not None:
            return existing
        index = builder.vertex(key[0], key[1], spec.height)
        top_point_index[key] = index
        return index

    top_outer_points = outer_points_by_z[-1][1]
    for point, vertex_index in zip(top_outer_points, outer_rings[-1]):
        top_point_index[(round(point[0], 6), round(point[1], 6))] = vertex_index

    all_top_points = list(top_outer_points)
    centers = hole_centers(spec)

    for cx, cy in centers:
        rings: list[list[int]] = []
        for z, hole_radius in hole_profiles:
            ring_points = [(float(cx + hole_radius * np.cos(t)), float(cy + hole_radius * np.sin(t))) for t in theta]
            if abs(z - spec.height) < 1e-6:
                ring = [top_vertex(point) for point in ring_points]
                all_top_points.extend(ring_points)
            else:
                ring = [builder.vertex(x, y, z) for x, y in ring_points]
            rings.append(ring)

        for lower, upper in zip(rings, rings[1:]):
            connect_rings(builder, lower, upper, reverse=True)

        floor_center = builder.vertex(cx, cy, spec.floor)
        for a, b in zip(rings[0], rings[0][1:] + rings[0][:1]):
            builder.face(floor_center, a, b)

    magnet_segments = circle_segments(spec.magnet_diameter, circle_segment_length)
    for side in ("front", "back", "left", "right"):
        add_side_panel_with_magnets(builder, spec, side, magnet_segments)

    add_support_points(spec, all_top_points, spacing=8.0)

    unique_points: list[tuple[float, float]] = []
    point_seen: set[tuple[float, float]] = set()
    for point in all_top_points:
        key = (round(point[0], 6), round(point[1], 6))
        if key not in point_seen:
            point_seen.add(key)
            unique_points.append(key)

    points_2d = np.asarray(unique_points, dtype=np.float64)
    triangles = Delaunay(points_2d).simplices
    for tri in triangles:
        xy = points_2d[tri]
        centroid = xy.mean(axis=0)
        if rounded_rectangle_sdf_2d(centroid, spec.width, spec.depth, box_r, corner_r) > 1e-6:
            continue
        if any(np.linalg.norm(centroid - np.asarray(center)) < top_hole_radius - 1e-6 for center in centers):
            continue

        face = [top_vertex(tuple(points_2d[i])) for i in tri]
        if polygon_signed_area(xy) < 0.0:
            face = [face[0], face[2], face[1]]
        builder.face(*face)

    mesh = builder.mesh()
    return mesh


def open_blind_cylinder_sdf(points: np.ndarray, spec: OrganizerSpec) -> np.ndarray:
    radius = spec.hole_diameter / 2.0
    cavity = np.full(len(points), np.inf, dtype=np.float64)
    xy = points[:, :2]
    z = points[:, 2]

    for cx, cy in hole_centers(spec):
        radial = np.linalg.norm(xy - np.array([cx, cy]), axis=1) - radius
        bottom = spec.floor - z
        # The cylinder is open above the organizer.  Subtracting it from the
        # rounded box cuts the hole, while smooth_max rounds the rim and floor.
        cavity = np.minimum(cavity, np.maximum(radial, bottom))

    return cavity


def organizer_sdf(points: np.ndarray, spec: OrganizerSpec) -> np.ndarray:
    outer = rounded_box_sdf(points, spec)
    cavity = open_blind_cylinder_sdf(points, spec)
    return smooth_max(outer, -cavity, spec.roundover)


def interpolate(p0: np.ndarray, p1: np.ndarray, v0: float, v1: float) -> np.ndarray:
    if abs(v1 - v0) < 1e-12:
        return (p0 + p1) * 0.5
    t = np.clip(-v0 / (v1 - v0), 0.0, 1.0)
    return p0 + t * (p1 - p0)


def polygonise_tet(points: np.ndarray, values: np.ndarray) -> list[list[np.ndarray]]:
    inside = values <= 0.0
    count = int(inside.sum())
    if count == 0 or count == 4:
        return []

    if count == 1:
        i = int(np.nonzero(inside)[0][0])
        outs = np.nonzero(~inside)[0]
        return [[interpolate(points[i], points[j], values[i], values[j]) for j in outs]]

    if count == 3:
        o = int(np.nonzero(~inside)[0][0])
        ins = np.nonzero(inside)[0]
        tri = [interpolate(points[o], points[j], values[o], values[j]) for j in ins]
        return [tri[::-1]]

    ins = np.nonzero(inside)[0]
    outs = np.nonzero(~inside)[0]
    p00 = interpolate(points[ins[0]], points[outs[0]], values[ins[0]], values[outs[0]])
    p01 = interpolate(points[ins[0]], points[outs[1]], values[ins[0]], values[outs[1]])
    p10 = interpolate(points[ins[1]], points[outs[0]], values[ins[1]], values[outs[0]])
    p11 = interpolate(points[ins[1]], points[outs[1]], values[ins[1]], values[outs[1]])
    return [[p00, p01, p11], [p00, p11, p10]]


def build_sdf_mesh(spec: OrganizerSpec) -> trimesh.Trimesh:
    pitch = spec.pitch
    # Half-pitch offset avoids sampling exactly on sharp SDF boundaries, which
    # creates ambiguous tetrahedra and non-watertight meshes.
    xs = np.arange(-pitch * 0.5, spec.width + pitch, pitch)
    ys = np.arange(-pitch * 0.5, spec.depth + pitch, pitch)
    zs = np.arange(-pitch * 0.5, spec.height + pitch, pitch)

    grid = np.empty((len(xs), len(ys), len(zs)), dtype=np.float32)
    xx, yy = np.meshgrid(xs, ys, indexing="ij")
    xy = np.column_stack([xx.ravel(), yy.ravel()])
    for zi, z in enumerate(zs):
        pts = np.column_stack([xy, np.full(len(xy), z)])
        grid[:, :, zi] = organizer_sdf(pts, spec).reshape(len(xs), len(ys))

    vertices: list[np.ndarray] = []
    faces: list[list[int]] = []

    for ix in range(len(xs) - 1):
        for iy in range(len(ys) - 1):
            for iz in range(len(zs) - 1):
                values = np.array(
                    [grid[ix + int(c[0]), iy + int(c[1]), iz + int(c[2])] for c in _CUBE_CORNERS],
                    dtype=np.float64,
                )
                if np.all(values <= 0.0) or np.all(values > 0.0):
                    continue

                base = np.array([xs[ix], ys[iy], zs[iz]], dtype=np.float64)
                cube_points = base + _CUBE_CORNERS * pitch
                for tet in _TETS:
                    for tri in polygonise_tet(cube_points[tet], values[tet]):
                        start = len(vertices)
                        vertices.extend(tri)
                        faces.append([start, start + 1, start + 2])

    mesh = trimesh.Trimesh(vertices=np.asarray(vertices), faces=np.asarray(faces), process=False)
    mesh.merge_vertices(digits_vertex=4)
    mesh.update_faces(mesh.nondegenerate_faces())
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()

    if mesh.volume < 0:
        mesh.invert()
    return mesh


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("extras/craft-paint-modular-organizer.stl"))
    parser.add_argument("--pitch", type=float, default=OrganizerSpec.pitch, help="Meshing pitch in mm.")
    parser.add_argument("--stagger", type=float, default=OrganizerSpec.stagger, help="Alternate-row hole stagger in mm.")
    parser.add_argument(
        "--roundover",
        type=float,
        default=None,
        help="Convenience option to set both box and hole roundovers in mm.",
    )
    parser.add_argument(
        "--box-roundover",
        type=float,
        default=OrganizerSpec.box_roundover,
        help="Top and bottom outer box edge roundover radius in mm.",
    )
    parser.add_argument(
        "--vertical-corner-roundover",
        type=float,
        default=OrganizerSpec.vertical_corner_roundover,
        help="Outer box vertical-corner radius in mm.",
    )
    parser.add_argument(
        "--hole-roundover",
        type=float,
        default=OrganizerSpec.hole_roundover,
        help="Top and bottom hole roundover radius in mm.",
    )
    parser.add_argument("--magnet-diameter", type=float, default=OrganizerSpec.magnet_diameter, help="Magnet pocket diameter in mm.")
    parser.add_argument("--magnet-depth", type=float, default=OrganizerSpec.magnet_depth, help="Magnet pocket depth in mm.")
    parser.add_argument("--magnet-z", type=float, default=OrganizerSpec.magnet_z, help="Magnet pocket center height in mm.")
    parser.add_argument("--logo-size", type=float, default=OrganizerSpec.logo_size, help="Front logo stamp size in mm.")
    parser.add_argument("--logo-depth", type=float, default=OrganizerSpec.logo_depth, help="Front logo stamp recess depth in mm.")
    parser.add_argument("--min-wall", type=float, default=OrganizerSpec.min_wall, help="Minimum wall between holes in mm.")
    parser.add_argument(
        "--edge-wall",
        type=float,
        default=OrganizerSpec.edge_wall,
        help="Minimum wall between holes and outside edges in mm.",
    )
    parser.add_argument(
        "--solve-layout",
        action="store_true",
        help="Search for an edge wall that balances edge and hole-to-hole clearances.",
    )
    parser.add_argument(
        "--sdf",
        action="store_true",
        help="Use the slower signed-distance mesher instead of the analytic sharp-edge mesh.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    box_roundover = args.roundover if args.roundover is not None else args.box_roundover
    vertical_corner_roundover = (
        args.roundover if args.roundover is not None else args.vertical_corner_roundover
    )
    hole_roundover = args.roundover if args.roundover is not None else args.hole_roundover
    spec = OrganizerSpec(
        pitch=args.pitch,
        stagger=args.stagger,
        box_roundover=box_roundover,
        vertical_corner_roundover=vertical_corner_roundover,
        hole_roundover=hole_roundover,
        magnet_diameter=args.magnet_diameter,
        magnet_depth=args.magnet_depth,
        magnet_z=args.magnet_z,
        logo_size=args.logo_size,
        logo_depth=args.logo_depth,
        min_wall=args.min_wall,
        edge_wall=args.edge_wall,
    )
    if args.solve_layout:
        spec = replace(spec, edge_wall=balanced_edge_wall(spec))
        print(f"Solved edge wall: {spec.edge_wall:.3f} mm")

    if args.sdf:
        mesh = build_sdf_mesh(spec)
    elif spec.box_roundover > 0.0 or spec.hole_roundover > 0.0:
        mesh = build_rounded_analytic_mesh(spec)
    else:
        mesh = build_analytic_mesh(spec)
    mesh = apply_logo_inset(mesh, spec)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(args.output)
    print(f"Wrote {args.output}")
    print(f"Vertices: {len(mesh.vertices):,}")
    print(f"Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:,.1f} mm^3")


if __name__ == "__main__":
    main()
