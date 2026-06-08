#!/usr/bin/env python3
"""Generate a lightweight craft-paint modular organizer STL.

Dimensions are millimeters.
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
DEFAULT_LIGHTWEIGHT_CUP_HEIGHT = 50.0
DEFAULT_LIGHTWEIGHT_CUP_WALL = 1.5
DEFAULT_LIGHTWEIGHT_FLOOR = 1.0
DEFAULT_LIGHTWEIGHT_RETAINING_RING_HEIGHT = 5.0
DEFAULT_LIGHTWEIGHT_RETAINING_HOLE_DIAMETER = 29.0
DEFAULT_LIGHTWEIGHT_RETAINING_BEVEL_HEIGHT = 2.0
DEFAULT_LIGHTWEIGHT_RIB_WIDTH = 2.8
DEFAULT_LIGHTWEIGHT_RIB_HEIGHT = 3.0
DEFAULT_LIGHTWEIGHT_MAGNET_Z = 7.0
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_DEPTH = 4.5
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_WIDTH = 16.0
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_HEIGHT = 14.0
DEFAULT_LIGHTWEIGHT_PERIMETER_RIB_WIDTH = 4.0
DEFAULT_LIGHTWEIGHT_PERIMETER_RIB_DEPTH = 2.0
FASTENER_WALL_THICKNESS = 0.8
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


@dataclass(frozen=True)
class LightweightSpec:
    cup_height: float = DEFAULT_LIGHTWEIGHT_CUP_HEIGHT
    cup_wall: float = DEFAULT_LIGHTWEIGHT_CUP_WALL
    floor: float = DEFAULT_LIGHTWEIGHT_FLOOR
    retaining_ring_height: float = DEFAULT_LIGHTWEIGHT_RETAINING_RING_HEIGHT
    retaining_hole_diameter: float = DEFAULT_LIGHTWEIGHT_RETAINING_HOLE_DIAMETER
    retaining_bevel_height: float = DEFAULT_LIGHTWEIGHT_RETAINING_BEVEL_HEIGHT
    rib_width: float = DEFAULT_LIGHTWEIGHT_RIB_WIDTH
    rib_height: float = DEFAULT_LIGHTWEIGHT_RIB_HEIGHT
    magnet_z: float = DEFAULT_LIGHTWEIGHT_MAGNET_Z
    magnet_boss_depth: float = DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_DEPTH
    magnet_boss_width: float = DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_WIDTH
    magnet_boss_height: float = DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_HEIGHT
    perimeter_rib_width: float = DEFAULT_LIGHTWEIGHT_PERIMETER_RIB_WIDTH
    perimeter_rib_depth: float = DEFAULT_LIGHTWEIGHT_PERIMETER_RIB_DEPTH
    cup_wall_ribs: int = 3


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


def translated_box(extents: tuple[float, float, float], center: tuple[float, float, float]) -> trimesh.Trimesh:
    mesh = trimesh.creation.box(extents=extents)
    mesh.apply_translation(center)
    return mesh


def vertical_cylinder(radius: float, height: float, center: tuple[float, float, float], sections: int) -> trimesh.Trimesh:
    mesh = trimesh.creation.cylinder(radius=radius, height=height, sections=sections)
    mesh.apply_translation(center)
    return mesh


def vertical_frustum(
    lower_radius: float,
    upper_radius: float,
    height: float,
    center: tuple[float, float, float],
    sections: int,
) -> trimesh.Trimesh:
    angles = np.linspace(0.0, 2.0 * np.pi, sections, endpoint=False)
    lower_z = center[2] - height / 2.0
    upper_z = center[2] + height / 2.0
    lower = np.column_stack(
        (
            center[0] + lower_radius * np.cos(angles),
            center[1] + lower_radius * np.sin(angles),
            np.full(sections, lower_z),
        )
    )
    upper = np.column_stack(
        (
            center[0] + upper_radius * np.cos(angles),
            center[1] + upper_radius * np.sin(angles),
            np.full(sections, upper_z),
        )
    )
    vertices = np.vstack(
        (
            lower,
            upper,
            np.array(center) + (0.0, 0.0, -height / 2.0),
            np.array(center) + (0.0, 0.0, height / 2.0),
        )
    )
    lower_center = 2 * sections
    upper_center = lower_center + 1
    faces: list[list[int]] = []
    for index in range(sections):
        next_index = (index + 1) % sections
        faces.append([index, next_index, sections + next_index])
        faces.append([index, sections + next_index, sections + index])
        faces.append([lower_center, next_index, index])
        faces.append([upper_center, sections + index, sections + next_index])
    mesh = trimesh.Trimesh(vertices=vertices, faces=np.asarray(faces, dtype=np.int64), process=False)
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def axis_cylinder(
    radius: float,
    length: float,
    center: tuple[float, float, float],
    axis: tuple[float, float, float],
    sections: int,
) -> trimesh.Trimesh:
    mesh = trimesh.creation.cylinder(radius=radius, height=length, sections=sections)
    transform = trimesh.geometry.align_vectors(np.array([0.0, 0.0, 1.0]), np.asarray(axis, dtype=float))
    mesh.apply_transform(transform)
    mesh.apply_translation(center)
    return mesh


def strut_between(
    point_a: tuple[float, float],
    point_b: tuple[float, float],
    width: float,
    height: float,
    z_center: float,
) -> trimesh.Trimesh:
    a = np.asarray(point_a, dtype=float)
    b = np.asarray(point_b, dtype=float)
    vector = b - a
    length = float(np.linalg.norm(vector))
    angle = float(np.arctan2(vector[1], vector[0]))
    mesh = trimesh.creation.box(extents=(length, width, height))
    mesh.apply_transform(trimesh.transformations.rotation_matrix(angle, [0.0, 0.0, 1.0]))
    mesh.apply_translation((float((a[0] + b[0]) / 2.0), float((a[1] + b[1]) / 2.0), z_center))
    return mesh


def box_between_3d(
    point_a: tuple[float, float, float],
    point_b: tuple[float, float, float],
    normal: tuple[float, float, float],
    width: float,
    height: float,
) -> trimesh.Trimesh:
    a = np.asarray(point_a, dtype=float)
    b = np.asarray(point_b, dtype=float)
    axis = b - a
    length = float(np.linalg.norm(axis))
    if length <= 1e-6:
        return trimesh.Trimesh()

    x_axis = axis / length
    y_axis = np.asarray(normal, dtype=float)
    y_axis = y_axis - x_axis * np.dot(y_axis, x_axis)
    y_norm = float(np.linalg.norm(y_axis))
    if y_norm <= 1e-6:
        y_axis = np.array([0.0, 0.0, 1.0])
        y_axis = y_axis - x_axis * np.dot(y_axis, x_axis)
        y_norm = float(np.linalg.norm(y_axis))
    y_axis = y_axis / y_norm
    z_axis = np.cross(x_axis, y_axis)
    z_axis = z_axis / np.linalg.norm(z_axis)

    mesh = trimesh.creation.box(extents=(length, width, height))
    transform = np.eye(4)
    transform[:3, :3] = np.column_stack([x_axis, y_axis, z_axis])
    transform[:3, 3] = (a + b) / 2.0
    mesh.apply_transform(transform)
    return mesh


def perimeter_rail_band(
    spec: OrganizerSpec,
    rail_depth: float,
    rail_height: float,
    z_center: float,
) -> trimesh.Trimesh:
    import manifold3d as m3d

    straight_segments = max(8, int(np.ceil(max(spec.width, spec.depth) / 1.5 / 4.0)))
    arc_segments = 24
    outer = rounded_rectangle_ring(
        spec.width,
        spec.depth,
        0.0,
        spec.vertical_corner_roundover,
        straight_segments,
        arc_segments,
    )
    inner = rounded_rectangle_ring(
        spec.width,
        spec.depth,
        rail_depth,
        max(0.0, spec.vertical_corner_roundover - rail_depth),
        straight_segments,
        arc_segments,
    )
    cross_section = m3d.CrossSection([outer, inner], fillrule=m3d.FillRule.EvenOdd)
    solid = m3d.Manifold.extrude(cross_section, height=rail_height)
    solid = solid.translate((0.0, 0.0, z_center - rail_height / 2.0))
    raw = solid.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )
    mesh.merge_vertices(digits_vertex=2)
    mesh.update_faces(mesh.nondegenerate_faces())
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    solid_components = [component for component in mesh.split(only_watertight=False) if abs(component.volume) > 0.1]
    if len(solid_components) == 1:
        mesh = solid_components[0]
    elif solid_components:
        mesh = trimesh.util.concatenate(solid_components)
    mesh = cap_triangular_boundary_loops(mesh)
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def perimeter_triangular_top_rib(
    spec: OrganizerSpec,
    rib_depth: float,
    rib_height: float,
    z_top: float,
) -> trimesh.Trimesh:
    """Top outer rib with a triangular cross-section.

    The rib sits at the top of the perimeter wall. Its cross-section is a right
    triangle: the outer face is vertical (flush with the wall), the top face is
    horizontal, and the inner face is a bevel from the base of the outer wall
    (at z_top - rib_height) up to the top inner corner (at z_top, inset=rib_depth).
    """
    z_bottom = z_top - rib_height
    straight_segments = max(8, int(np.ceil(max(spec.width, spec.depth) / 1.5 / 4.0)))
    arc_segments = 24

    outer_points = rounded_rectangle_ring(
        spec.width, spec.depth, 0.0, spec.vertical_corner_roundover,
        straight_segments, arc_segments,
    )
    inner_points = rounded_rectangle_ring(
        spec.width, spec.depth, rib_depth, max(0.0, spec.vertical_corner_roundover - rib_depth),
        straight_segments, arc_segments,
    )

    builder = MeshBuilder()

    outer_bottom = [builder.vertex(x, y, z_bottom) for x, y in outer_points]
    outer_top = [builder.vertex(x, y, z_top) for x, y in outer_points]
    inner_top = [builder.vertex(x, y, z_top) for x, y in inner_points]

    connect_rings(builder, outer_bottom, outer_top, reverse=False)
    connect_rings(builder, outer_top, inner_top, reverse=False)
    connect_rings(builder, inner_top, outer_bottom, reverse=False)

    return builder.mesh()


def _side_rib_positions(
    side: str,
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    target_spacing: float = 25.0,
) -> list[float]:
    """U-axis positions for vertical perimeter ribs on one side.

    Places mandatory ribs flanking each magnet boss, then fills remaining
    gaps evenly at approximately target_spacing.
    """
    rib_width = lightweight.perimeter_rib_width
    boss_half_w = lightweight.magnet_boss_width / 2.0
    rib_half_w = rib_width / 2.0
    corner = spec.vertical_corner_roundover

    if side in {"front", "back"}:
        u_min = corner
        u_max = spec.width - corner
        magnet_us: list[float] = list(DEFAULT_LONG_SIDE_MAGNET_POSITIONS)
    else:
        u_min = corner
        u_max = spec.depth - corner
        magnet_us = list(DEFAULT_SHORT_SIDE_MAGNET_POSITIONS)

    anchors: list[float] = []
    for u_mag in magnet_us:
        for sign in (-1.0, 1.0):
            candidate = u_mag + sign * (boss_half_w + rib_half_w)
            if u_min + rib_half_w - 1e-6 <= candidate <= u_max - rib_half_w + 1e-6:
                anchors.append(round(candidate, 4))
    anchors = sorted(set(anchors))

    boundaries = [u_min] + anchors + [u_max]
    fill: list[float] = []
    for i in range(len(boundaries) - 1):
        gap_start, gap_end = boundaries[i], boundaries[i + 1]
        gap = gap_end - gap_start
        # n ribs divide the gap into n+1 sub-gaps; pick n so each is ~target_spacing
        n = max(0, round(gap / target_spacing) - 1)
        for j in range(1, n + 1):
            fill.append(round(gap_start + gap * j / (n + 1), 4))

    return sorted(set(anchors + fill))


def _vertical_rib_mesh(
    u: float,
    side: str,
    spec: OrganizerSpec,
    rib_width: float,
    rib_depth: float,
    z_bottom: float,
    z_top: float,
) -> trimesh.Trimesh:
    """Triangular-prism rib on the outer face of a flat side wall.

    The base (rib_width wide) lies flush with the wall's outer face;
    the apex projects rib_depth outward from the wall.
    """
    half_w = rib_width / 2.0
    # Points in CCW order when viewed from above, so bottom cap [0,2,1]
    # has downward normal and top cap [3,4,5] has upward normal.
    if side == "front":
        pts: list[tuple[float, float]] = [
            (u - half_w, 0.0), (u, rib_depth), (u + half_w, 0.0)
        ]
    elif side == "back":
        pts = [
            (u + half_w, spec.depth), (u, spec.depth - rib_depth), (u - half_w, spec.depth)
        ]
    elif side == "left":
        pts = [
            (rib_depth, u), (0.0, u - half_w), (0.0, u + half_w)
        ]
    elif side == "right":
        pts = [
            (spec.width - rib_depth, u), (spec.width, u + half_w), (spec.width, u - half_w)
        ]
    else:
        raise ValueError(f"Unknown side: {side}")

    verts = np.array(
        [(x, y, z_bottom) for x, y in pts] + [(x, y, z_top) for x, y in pts],
        dtype=np.float64,
    )
    faces = np.array([
        [0, 2, 1],        # bottom cap (CW from above → normal points -Z)
        [3, 4, 5],        # top cap   (CCW from above → normal points +Z)
        [0, 1, 4], [0, 4, 3],   # side 0→1
        [1, 2, 5], [1, 5, 4],   # side 1→2
        [2, 0, 3], [2, 3, 5],   # side 2→0
    ], dtype=np.int64)
    mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def perimeter_vertical_ribs(
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    z_bottom: float,
    z_top: float,
) -> list[trimesh.Trimesh]:
    """Triangular ribs on all four outer side walls."""
    ribs: list[trimesh.Trimesh] = []
    for side in ("front", "back", "left", "right"):
        for u in _side_rib_positions(side, spec, lightweight):
            ribs.append(
                _vertical_rib_mesh(
                    u, side, spec,
                    lightweight.perimeter_rib_width,
                    lightweight.perimeter_rib_depth,
                    z_bottom, z_top,
                )
            )
    return ribs


def lightweight_rib_edges(centers: list[tuple[float, float]], max_length: float = 56.0) -> list[tuple[int, int]]:
    points = np.asarray(centers, dtype=float)
    edges: set[tuple[int, int]] = set()
    for simplex in Delaunay(points).simplices:
        for local_a, local_b in ((0, 1), (1, 2), (2, 0)):
            a = int(simplex[local_a])
            b = int(simplex[local_b])
            if np.linalg.norm(points[a] - points[b]) <= max_length:
                edges.add(tuple(sorted((a, b))))
    return sorted(edges)


def perimeter_strut_endpoints(
    centers: list[tuple[float, float]],
    spec: OrganizerSpec,
    max_length: float = 56.0,
) -> list[tuple[tuple[float, float], tuple[float, float]]]:
    """Return (cup_center, wall_point) pairs for struts from outer cups to the perimeter."""
    pairs: list[tuple[tuple[float, float], tuple[float, float]]] = []
    for cx, cy in centers:
        walls = [
            (float(cx), (0.0, float(cy))),
            (float(spec.width - cx), (float(spec.width), float(cy))),
            (float(cy), (float(cx), 0.0)),
            (float(spec.depth - cy), (float(cx), float(spec.depth))),
        ]
        for dist, endpoint in walls:
            if dist <= max_length:
                pairs.append(((float(cx), float(cy)), endpoint))
    return pairs


def clipped_segment_runs(
    point_a: np.ndarray,
    point_b: np.ndarray,
    inside,
    sample_step: float = 1.0,
) -> list[tuple[tuple[float, float], tuple[float, float]]]:
    length = float(np.linalg.norm(point_b - point_a))
    if length <= 1e-6:
        return []
    steps = max(2, int(np.ceil(length / sample_step)) + 1)
    samples = [point_a + (point_b - point_a) * (index / (steps - 1)) for index in range(steps)]
    flags = [inside(sample) for sample in samples]

    runs: list[tuple[tuple[float, float], tuple[float, float]]] = []
    start: np.ndarray | None = None
    previous = samples[0]
    previous_inside = flags[0]
    if previous_inside:
        start = previous

    for current, current_inside in zip(samples[1:], flags[1:]):
        if current_inside and not previous_inside:
            start = current
        elif previous_inside and not current_inside and start is not None:
            if np.linalg.norm(previous - start) > 1.0:
                runs.append((tuple(start), tuple(previous)))
            start = None
        previous = current
        previous_inside = current_inside

    if previous_inside and start is not None and np.linalg.norm(previous - start) > 1.0:
        runs.append((tuple(start), tuple(previous)))
    return runs


def rounded_perimeter_mapper(spec: OrganizerSpec, inset: float):
    points = rounded_rectangle_ring(
        spec.width,
        spec.depth,
        inset,
        max(0.0, spec.vertical_corner_roundover - inset),
        max(8, int(np.ceil(max(spec.width, spec.depth) / 2.0 / 4.0))),
        20,
    )
    segments: list[tuple[np.ndarray, np.ndarray, float, float]] = []
    total = 0.0
    for start, end in zip(points, points[1:] + points[:1]):
        a = np.asarray(start, dtype=float)
        b = np.asarray(end, dtype=float)
        length = float(np.linalg.norm(b - a))
        if length <= 1e-9:
            continue
        segments.append((a, b, total, total + length))
        total += length

    def map_point(s: float, z: float) -> tuple[np.ndarray, np.ndarray]:
        wrapped = s % total
        for a, b, s0, s1 in segments:
            if wrapped <= s1 + 1e-9:
                t = (wrapped - s0) / (s1 - s0)
                xy = a + (b - a) * t
                tangent_xy = (b - a) / np.linalg.norm(b - a)
                normal_xy = np.array([tangent_xy[1], -tangent_xy[0]])
                point = np.array([xy[0], xy[1], z], dtype=float)
                normal = np.array([normal_xy[0], normal_xy[1], 0.0], dtype=float)
                return point, normal
        a, b, s0, s1 = segments[-1]
        tangent_xy = (b - a) / np.linalg.norm(b - a)
        normal_xy = np.array([tangent_xy[1], -tangent_xy[0]])
        return np.array([a[0], a[1], z], dtype=float), np.array([normal_xy[0], normal_xy[1], 0.0], dtype=float)

    return total, map_point

def cylindrical_thin_wall(
    center: tuple[float, float],
    inner_radius: float,
    z_bottom: float,
    height: float,
    segments: int,
) -> trimesh.Trimesh:
    import manifold3d as m3d

    outer_r = inner_radius + FASTENER_WALL_THICKNESS
    angles = np.linspace(0.0, 2.0 * np.pi, segments, endpoint=False)
    inner_pts = [(inner_radius * np.cos(a), inner_radius * np.sin(a)) for a in angles]
    outer_pts = [(outer_r * np.cos(a), outer_r * np.sin(a)) for a in angles]
    cross = m3d.CrossSection([outer_pts, list(reversed(inner_pts))], fillrule=m3d.FillRule.EvenOdd)
    solid = m3d.Manifold.extrude(cross, height=height)
    solid = solid.translate((center[0], center[1], z_bottom))
    raw = solid.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def _collapse_non_manifold_edges(mesh: trimesh.Trimesh) -> trimesh.Trimesh:
    """Repair non-manifold edges by vertex collapse.

    Each non-manifold edge (shared by > 2 faces) is an internal seam left by
    boolean operations.  Collapsing it — merging its two endpoints to their
    midpoint — makes the adjacent degenerate faces disappear naturally when
    nondegenerate_faces() runs.  No new geometry is added, so no new
    non-manifold edges can be introduced (unlike fill_holes on excised patches).
    Iterates up to 5 passes because merge_vertices after each collapse can
    expose further non-manifold edges.
    """
    for _pass in range(5):
        edges, counts = np.unique(mesh.edges_sorted, axis=0, return_counts=True)
        nm_edges = edges[counts > 2]
        if len(nm_edges) == 0:
            break

        vertices = mesh.vertices.copy()
        faces = mesh.faces.copy()

        canonical = np.arange(len(vertices))

        def find(x: int) -> int:
            while canonical[x] != x:
                canonical[x] = canonical[canonical[x]]
                x = int(canonical[x])
            return x

        for e in nm_edges:
            r0, r1 = find(int(e[0])), find(int(e[1]))
            if r0 == r1:
                continue
            vertices[r0] = (vertices[r0] + vertices[r1]) / 2.0
            canonical[r1] = r0

        new_faces = np.vectorize(find)(faces)
        mesh = trimesh.Trimesh(vertices=vertices, faces=new_faces, process=False)
        mesh.update_faces(mesh.nondegenerate_faces())
        mesh.remove_unreferenced_vertices()
        mesh.merge_vertices(digits_vertex=2)
        mesh.fix_normals()

    # Collapse any remaining near-degenerate boundary loops.  The vertex collapse
    # above can leave tiny open triangles whose vertices are < 0.1 mm apart;
    # cap_triangular_boundary_loops can't close them because the resulting face
    # has near-zero area and is removed by nondegenerate_faces.  For each such
    # loop we merge all its vertices to the centroid instead.
    bedges, bcounts = np.unique(mesh.edges_sorted, axis=0, return_counts=True)
    boundary_edges = bedges[bcounts == 1]
    if len(boundary_edges) > 0:
        bv: dict[int, list[int]] = {}
        for a, b in boundary_edges:
            bv.setdefault(int(a), []).append(int(b))
            bv.setdefault(int(b), []).append(int(a))
        visited2: set[int] = set()
        verts2 = mesh.vertices.copy()
        faces2 = mesh.faces.copy()
        canonical2 = np.arange(len(verts2))
        for start2 in bv:
            if start2 in visited2:
                continue
            stack2 = [start2]
            loop_verts: list[int] = []
            while stack2:
                cur2 = stack2.pop()
                if cur2 in visited2:
                    continue
                visited2.add(cur2)
                loop_verts.append(cur2)
                stack2.extend(n for n in bv[cur2] if n not in visited2)
            positions = verts2[loop_verts]
            span = float(np.linalg.norm(positions.max(axis=0) - positions.min(axis=0)))
            if span < 0.1:
                centroid = positions.mean(axis=0)
                verts2[loop_verts[0]] = centroid
                for lv in loop_verts[1:]:
                    canonical2[lv] = loop_verts[0]
        new_faces2 = np.vectorize(lambda x: int(canonical2[x]))(faces2)
        mesh = trimesh.Trimesh(vertices=verts2, faces=new_faces2, process=False)
        mesh.update_faces(mesh.nondegenerate_faces())
        mesh.remove_unreferenced_vertices()
        mesh.fix_normals()

    if mesh.volume < 0:
        mesh.invert()
    return mesh


def cap_triangular_boundary_loops(mesh: trimesh.Trimesh) -> trimesh.Trimesh:
    edges, counts = np.unique(mesh.edges_sorted, axis=0, return_counts=True)
    boundary_edges = [(int(a), int(b)) for a, b in edges[counts == 1]]
    if not boundary_edges:
        return mesh

    adjacency: dict[int, list[int]] = {}
    for a, b in boundary_edges:
        adjacency.setdefault(a, []).append(b)
        adjacency.setdefault(b, []).append(a)

    faces = mesh.faces.tolist()
    visited: set[int] = set()
    for start in adjacency:
        if start in visited:
            continue
        stack = [start]
        component: list[int] = []
        while stack:
            current = stack.pop()
            if current in visited:
                continue
            visited.add(current)
            component.append(current)
            stack.extend(vertex for vertex in adjacency.get(current, []) if vertex not in visited)

        if len(component) == 3 and all(len(adjacency[vertex]) == 2 for vertex in component):
            faces.append(component)

    result = trimesh.Trimesh(vertices=mesh.vertices.copy(), faces=np.asarray(faces, dtype=np.int64), process=False)
    result.merge_vertices(digits_vertex=2)
    result.update_faces(result.nondegenerate_faces())
    result.remove_unreferenced_vertices()
    result.fix_normals()
    return result


def magnet_boss_and_cutter(
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    side: str,
    position: float,
    z: float,
    circle_segments_count: int,
) -> tuple[trimesh.Trimesh, trimesh.Trimesh]:
    boss_depth = lightweight.magnet_boss_depth
    boss_width = lightweight.magnet_boss_width
    boss_top = z + lightweight.magnet_boss_height / 2.0
    boss_height = boss_top
    cutter_length = spec.magnet_depth + 0.25
    magnet_radius = spec.magnet_diameter / 2.0

    # Rectangular box in front-side frame: Y=0 is the wall face, Y=boss_depth
    # is the inner face, z runs from 0 to boss_height.
    boss = translated_box(
        (boss_width, boss_depth, boss_height),
        (0.0, boss_depth / 2.0, boss_height / 2.0),
    )

    if side == "front":
        boss.apply_translation([position, 0.0, 0.0])
        cutter = axis_cylinder(
            magnet_radius,
            cutter_length,
            (position, cutter_length / 2.0 - 0.05, z),
            (0.0, 1.0, 0.0),
            circle_segments_count,
        )
    elif side == "back":
        boss.apply_transform(trimesh.transformations.scale_matrix(-1.0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]))
        boss.apply_translation([position, spec.depth, 0.0])
        cutter = axis_cylinder(
            magnet_radius,
            cutter_length,
            (position, spec.depth - cutter_length / 2.0 + 0.05, z),
            (0.0, -1.0, 0.0),
            circle_segments_count,
        )
    elif side == "left":
        boss.apply_transform(trimesh.transformations.rotation_matrix(-np.pi / 2.0, [0.0, 0.0, 1.0]))
        boss.apply_translation([0.0, position, 0.0])
        cutter = axis_cylinder(
            magnet_radius,
            cutter_length,
            (cutter_length / 2.0 - 0.05, position, z),
            (1.0, 0.0, 0.0),
            circle_segments_count,
        )
    elif side == "right":
        boss.apply_transform(trimesh.transformations.rotation_matrix(np.pi / 2.0, [0.0, 0.0, 1.0]))
        boss.apply_translation([spec.width, position, 0.0])
        cutter = axis_cylinder(
            magnet_radius,
            cutter_length,
            (spec.width - cutter_length / 2.0 + 0.05, position, z),
            (-1.0, 0.0, 0.0),
            circle_segments_count,
        )
    else:
        raise ValueError(f"Unknown side: {side}")
    boss.fix_normals()
    if boss.volume < 0:
        boss.invert()
    return boss, cutter


def perimeter_corner_ribs(
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    z_bottom: float,
    z_top: float,
) -> list[trimesh.Trimesh]:
    """One inward-pointing triangular rib at the midpoint of each corner arc."""
    corner_r = spec.vertical_corner_roundover
    rib_width = lightweight.perimeter_rib_width
    rib_depth = lightweight.perimeter_rib_depth
    half_w = rib_width / 2.0

    # (arc_center_x, arc_center_y, mid_arc_angle)
    corners = [
        (corner_r,               corner_r,               5 * np.pi / 4),  # bottom-left
        (spec.width - corner_r,  corner_r,               7 * np.pi / 4),  # bottom-right
        (spec.width - corner_r,  spec.depth - corner_r,  1 * np.pi / 4),  # top-right
        (corner_r,               spec.depth - corner_r,  3 * np.pi / 4),  # top-left
    ]
    ribs: list[trimesh.Trimesh] = []
    for cx, cy, angle in corners:
        outward = np.array([np.cos(angle), np.sin(angle)])
        tangent = np.array([-np.sin(angle), np.cos(angle)])
        base_xy = np.array([cx, cy]) + corner_r * outward
        pts = [
            tuple(base_xy - half_w * tangent),
            tuple(base_xy - rib_depth * outward),   # inward
            tuple(base_xy + half_w * tangent),
        ]
        verts = np.array(
            [(x, y, z_bottom) for x, y in pts] + [(x, y, z_top) for x, y in pts],
            dtype=np.float64,
        )
        faces = np.array([
            [0, 2, 1], [3, 4, 5],
            [0, 1, 4], [0, 4, 3],
            [1, 2, 5], [1, 5, 4],
            [2, 0, 3], [2, 3, 5],
        ], dtype=np.int64)
        mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
        mesh.fix_normals()
        if mesh.volume < 0:
            mesh.invert()
        ribs.append(mesh)
    return ribs


def _cup_rib_mesh(
    cx: float,
    cy: float,
    outer_radius: float,
    angle: float,
    rib_width: float,
    rib_depth: float,
    z_bottom: float,
    z_top: float,
) -> trimesh.Trimesh:
    """Triangular-prism rib on the outer surface of a cylindrical cup."""
    outward = np.array([np.cos(angle), np.sin(angle)])
    tangent = np.array([-np.sin(angle), np.cos(angle)])
    base_xy = np.array([cx, cy]) + outer_radius * outward
    half_w = rib_width / 2.0

    # CCW from above: right_base → apex → left_base
    pts = [
        tuple(base_xy - half_w * tangent),
        tuple(base_xy + rib_depth * outward),
        tuple(base_xy + half_w * tangent),
    ]
    verts = np.array(
        [(x, y, z_bottom) for x, y in pts] + [(x, y, z_top) for x, y in pts],
        dtype=np.float64,
    )
    faces = np.array([
        [0, 2, 1],        # bottom cap
        [3, 4, 5],        # top cap
        [0, 1, 4], [0, 4, 3],
        [1, 2, 5], [1, 5, 4],
        [2, 0, 3], [2, 3, 5],
    ], dtype=np.int64)
    mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def cup_vertical_ribs(
    centers: list[tuple[float, float]],
    outer_radius: float,
    lightweight: LightweightSpec,
    z_bottom: float,
    z_top: float,
) -> list[trimesh.Trimesh]:
    """Evenly-spaced vertical ribs on the exterior of every cup."""
    n = lightweight.cup_wall_ribs
    if n <= 0:
        return []
    angles = np.linspace(0.0, 2.0 * np.pi, n, endpoint=False)
    ribs: list[trimesh.Trimesh] = []
    for cx, cy in centers:
        for angle in angles:
            ribs.append(
                _cup_rib_mesh(
                    cx, cy, outer_radius, float(angle),
                    lightweight.perimeter_rib_width,
                    lightweight.perimeter_rib_depth,
                    z_bottom, z_top,
                )
            )
    return ribs


def build_lightweight_mesh(
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    circle_segment_length: float = 1.5,
) -> trimesh.Trimesh:
    centers = hole_centers(spec)
    cup_height = min(lightweight.cup_height, spec.height)
    cup_floor = min(lightweight.floor, cup_height - 0.5)
    retaining_ring_height = max(
        0.0,
        min(lightweight.retaining_ring_height, cup_height - cup_floor - 0.5),
    )
    retaining_bevel_height = max(
        0.0,
        min(lightweight.retaining_bevel_height, cup_height - cup_floor - retaining_ring_height - 0.5),
    )
    retaining_ring_top_z = cup_floor + retaining_ring_height
    main_bore_z = retaining_ring_top_z + retaining_bevel_height
    inner_radius = spec.hole_diameter / 2.0
    retaining_hole_radius = min(lightweight.retaining_hole_diameter / 2.0, inner_radius - 0.25)
    outer_radius = inner_radius + FASTENER_WALL_THICKNESS
    top_ring_outer_radius = inner_radius + 2.0 * lightweight.cup_wall
    cup_segments = circle_segments(spec.hole_diameter + 2.0 * lightweight.cup_wall, circle_segment_length)
    retaining_hole_segments = circle_segments(lightweight.retaining_hole_diameter, circle_segment_length)
    magnet_segments = circle_segments(spec.magnet_diameter, circle_segment_length)

    solids: list[trimesh.Trimesh] = []
    fastener_walls: list[trimesh.Trimesh] = []
    cutters: list[trimesh.Trimesh] = []
    cup_cutters: list[trimesh.Trimesh] = []
    for cup_index, (cx, cy) in enumerate(centers):
        inner_height = cup_height - main_bore_z + 0.25
        inner = vertical_cylinder(
            inner_radius,
            inner_height,
            (cx, cy, main_bore_z + inner_height / 2.0 - 0.05),
            cup_segments,
        )
        retaining_hole = vertical_cylinder(
            retaining_hole_radius,
            retaining_ring_height + 0.1,
            (cx, cy, cup_floor + retaining_ring_height / 2.0 + 0.05),
            retaining_hole_segments,
        )
        retaining_bevel = vertical_frustum(
            retaining_hole_radius,
            inner_radius,
            retaining_bevel_height + 0.25,
            (cx, cy, retaining_ring_top_z + retaining_bevel_height / 2.0 + 0.075),
            cup_segments,
        )
        bottom_height = max(lightweight.rib_height, main_bore_z)
        bottom_floor = vertical_cylinder(
            outer_radius,
            bottom_height,
            (cx, cy, bottom_height / 2.0),
            cup_segments,
        )
        top_ring_bevel = vertical_frustum(
            inner_radius,
            top_ring_outer_radius,
            lightweight.rib_height,
            (cx, cy, cup_height - lightweight.rib_height / 2.0),
            cup_segments,
        )
        top_ring_drop = 0.5
        top_ring_bevel_h = top_ring_outer_radius - outer_radius  # 45°: Δz = Δr from horizontal
        z_top_bevel_start = cup_height - top_ring_drop
        z_top_bevel_bottom = z_top_bevel_start - top_ring_bevel_h
        solids.append(bottom_floor)
        solids.append(vertical_cylinder(
            top_ring_outer_radius,
            top_ring_drop,
            (cx, cy, cup_height - top_ring_drop / 2.0),
            cup_segments,
        ))
        solids.append(vertical_frustum(
            outer_radius,
            top_ring_outer_radius,
            top_ring_bevel_h,
            (cx, cy, z_top_bevel_bottom + top_ring_bevel_h / 2.0),
            cup_segments,
        ))
        # Start above the bottom floor to avoid a coincident outer face at r=outer_radius.
        wall_z_start = bottom_height + 0.1
        fastener_walls.append(cylindrical_thin_wall(
            (cx, cy), inner_radius, wall_z_start,
            cup_height - lightweight.rib_height + 0.2 - wall_z_start, cup_segments,
        ))
        cup_cutters.append(inner)
        cup_cutters.append(retaining_hole)
        if retaining_bevel_height > 0.0:
            cup_cutters.append(retaining_bevel)
        cup_cutters.append(top_ring_bevel)

    solids.extend(cup_vertical_ribs(centers, outer_radius, lightweight, 0.0, cup_height))

    internal_rib_zs = (
        lightweight.rib_height / 2.0,
        cup_height - lightweight.rib_height / 2.0,
    )
    for rib_z in internal_rib_zs:
        for a, b in lightweight_rib_edges(centers):
            solids.append(
                strut_between(
                    centers[a],
                    centers[b],
                    lightweight.rib_width,
                    lightweight.rib_height,
                    rib_z,
                )
            )

    for rib_z in internal_rib_zs:
        for cup_center, wall_point in perimeter_strut_endpoints(centers, spec, max_length=30.0):
            solids.append(
                strut_between(
                    cup_center,
                    wall_point,
                    lightweight.rib_width,
                    lightweight.rib_height,
                    rib_z,
                )
            )

    rail_depth = lightweight.magnet_boss_depth
    low_rim_height = lightweight.rib_height
    top_rail_z = cup_height - lightweight.rib_height / 2.0
    solids.append(perimeter_rail_band(spec, rail_depth / 2.0, low_rim_height, low_rim_height / 2.0))
    solids.append(perimeter_triangular_top_rib(spec, rail_depth, rail_depth, cup_height))

    side_z_max = cup_height - lightweight.rib_height + 0.2
    solids.append(perimeter_rail_band(spec, FASTENER_WALL_THICKNESS, side_z_max, side_z_max / 2.0))

    solids.extend(perimeter_vertical_ribs(spec, lightweight, 0.0, cup_height))
    solids.extend(perimeter_corner_ribs(spec, lightweight, 0.0, cup_height))

    magnet_spec = replace(spec, magnet_z=lightweight.magnet_z)
    for side in ("front", "back", "left", "right"):
        for position, z in side_panel_pockets(magnet_spec, side):
            boss, cutter = magnet_boss_and_cutter(magnet_spec, lightweight, side, position, z, magnet_segments)
            solids.append(boss)
            cutters.append(cutter)

    mesh = trimesh.boolean.union(solids, engine="manifold")
    if cup_cutters or cutters:
        mesh = trimesh.boolean.difference([mesh, *cup_cutters, *cutters], engine="manifold")
    mesh = trimesh.boolean.union([mesh, *fastener_walls], engine="manifold")
    mesh.merge_vertices(digits_vertex=2)
    mesh.update_faces(mesh.nondegenerate_faces())
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    solid_components = [component for component in mesh.split(only_watertight=False) if abs(component.volume) > 0.1]
    if len(solid_components) == 1:
        mesh = solid_components[0]
    elif solid_components:
        mesh = trimesh.util.concatenate(solid_components)
    mesh = cap_triangular_boundary_loops(mesh)
    mesh = _collapse_non_manifold_edges(mesh)
    mesh = fill_planar_boundary_loops(mesh, z_value=0.0)

    # Shear off any sub-zero protrusions (Bezier-arc bow or tilted trunk rings
    # can dip a fraction of a mm below z=0).  Done as a separate boolean so the
    # slab never shares a coincident face with the already-assembled mesh.
    # The slab top is at z=+0.05 (50 µm above the nominal floor) to avoid the
    # coplanar-face ambiguity; at print resolution this is invisible.
    slab_h = 2.0
    clip_slab = trimesh.creation.box(
        extents=(spec.width + slab_h * 2.0, spec.depth + slab_h * 2.0, slab_h + 0.05),
    )
    clip_slab.apply_translation((spec.width / 2.0, spec.depth / 2.0, -(slab_h - 0.05) / 2.0))
    if float(np.min(mesh.vertices[:, 2])) < -1e-4:
        mesh = trimesh.boolean.difference([mesh, clip_slab], engine="manifold")
        mesh.merge_vertices(digits_vertex=2)
        mesh.update_faces(mesh.nondegenerate_faces())
        mesh.remove_unreferenced_vertices()
        mesh.fix_normals()

    if mesh.volume < 0:
        mesh.invert()
    return mesh


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("stl/extras/craft-paint-modular-organizer.stl"))
    parser.add_argument("--stagger", type=float, default=OrganizerSpec.stagger, help="Alternate-row hole stagger in mm.")
    parser.add_argument(
        "--vertical-corner-roundover",
        type=float,
        default=OrganizerSpec.vertical_corner_roundover,
        help="Outer box vertical-corner radius in mm.",
    )
    parser.add_argument("--magnet-diameter", type=float, default=OrganizerSpec.magnet_diameter, help="Magnet pocket diameter in mm.")
    parser.add_argument("--magnet-depth", type=float, default=OrganizerSpec.magnet_depth, help="Magnet pocket depth in mm.")
    parser.add_argument("--cup-height", type=float, default=LightweightSpec.cup_height, help="Cup wall height in mm.")
    parser.add_argument("--cup-wall", type=float, default=LightweightSpec.cup_wall, help="Cup wall thickness in mm.")
    parser.add_argument("--floor", type=float, default=LightweightSpec.floor, help="Cup floor thickness in mm.")
    parser.add_argument("--retaining-ring-height", type=float, default=LightweightSpec.retaining_ring_height, help="Bottom retaining ring height in mm.")
    parser.add_argument("--retaining-hole-diameter", type=float, default=LightweightSpec.retaining_hole_diameter, help="Centered hole diameter through each bottom retaining ring in mm.")
    parser.add_argument("--retaining-bevel-height", type=float, default=LightweightSpec.retaining_bevel_height, help="Taper height above each bottom retaining ring in mm.")
    parser.add_argument("--rib-width", type=float, default=LightweightSpec.rib_width, help="Rib width in mm.")
    parser.add_argument("--rib-height", type=float, default=LightweightSpec.rib_height, help="Rib height in mm.")
    parser.add_argument("--magnet-z", type=float, default=LightweightSpec.magnet_z, help="Magnet pocket center height in mm.")
    parser.add_argument("--magnet-boss-depth", type=float, default=LightweightSpec.magnet_boss_depth, help="Side magnet boss depth in mm.")
    parser.add_argument("--magnet-boss-width", type=float, default=LightweightSpec.magnet_boss_width, help="Side magnet boss width in mm.")
    parser.add_argument("--magnet-boss-height", type=float, default=LightweightSpec.magnet_boss_height, help="Side magnet boss height in mm.")
    parser.add_argument("--perimeter-rib-width", type=float, default=LightweightSpec.perimeter_rib_width, help="Perimeter vertical rib base width in mm.")
    parser.add_argument("--perimeter-rib-depth", type=float, default=LightweightSpec.perimeter_rib_depth, help="Perimeter vertical rib outward depth in mm.")
    parser.add_argument("--cup-wall-ribs", type=int, default=LightweightSpec.cup_wall_ribs, help="Number of vertical ribs on the outside of each cup.")
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
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    spec = OrganizerSpec(
        stagger=args.stagger,
        vertical_corner_roundover=args.vertical_corner_roundover,
        magnet_diameter=args.magnet_diameter,
        magnet_depth=args.magnet_depth,
        min_wall=args.min_wall,
        edge_wall=args.edge_wall,
    )
    if args.solve_layout:
        spec = replace(spec, edge_wall=balanced_edge_wall(spec))
        print(f"Solved edge wall: {spec.edge_wall:.3f} mm")

    lightweight = LightweightSpec(
        cup_height=args.cup_height,
        cup_wall=args.cup_wall,
        floor=args.floor,
        retaining_ring_height=args.retaining_ring_height,
        retaining_hole_diameter=args.retaining_hole_diameter,
        retaining_bevel_height=args.retaining_bevel_height,
        rib_width=args.rib_width,
        rib_height=args.rib_height,
        magnet_z=args.magnet_z,
        magnet_boss_depth=args.magnet_boss_depth,
        magnet_boss_width=args.magnet_boss_width,
        magnet_boss_height=args.magnet_boss_height,
        perimeter_rib_width=args.perimeter_rib_width,
        perimeter_rib_depth=args.perimeter_rib_depth,
        cup_wall_ribs=args.cup_wall_ribs,
    )

    mesh = build_lightweight_mesh(spec, lightweight)
    output = args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(output)
    print(f"Wrote {output}")
    print(f"Vertices: {len(mesh.vertices):,}")
    print(f"Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:,.1f} mm^3")


if __name__ == "__main__":
    main()
