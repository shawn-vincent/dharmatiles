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
DEFAULT_LIGHTWEIGHT_FLOOR = 0.5
DEFAULT_LIGHTWEIGHT_RETAINING_RING_HEIGHT = 5.0
DEFAULT_LIGHTWEIGHT_RETAINING_HOLE_DIAMETER = 29.0
DEFAULT_LIGHTWEIGHT_RETAINING_BEVEL_HEIGHT = 2.0
DEFAULT_LIGHTWEIGHT_RIB_WIDTH = 2.8
DEFAULT_LIGHTWEIGHT_RIB_HEIGHT = 3.0
DEFAULT_LIGHTWEIGHT_MAGNET_Z = 7.0
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_DEPTH = 4.5
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_WIDTH = 16.0
DEFAULT_LIGHTWEIGHT_MAGNET_BOSS_HEIGHT = 14.0
DEFAULT_LIGHTWEIGHT_TREE_STRUT_WIDTH = 2.4
DEFAULT_LIGHTWEIGHT_TREE_TOP_SPACING = 10.0
DEFAULT_LIGHTWEIGHT_TREE_ROOTS = 3
DEFAULT_LIGHTWEIGHT_TREE_BARK = False
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
    tree_strut_width: float = DEFAULT_LIGHTWEIGHT_TREE_STRUT_WIDTH
    tree_top_spacing: float = DEFAULT_LIGHTWEIGHT_TREE_TOP_SPACING
    tree_roots: int = DEFAULT_LIGHTWEIGHT_TREE_ROOTS
    tree_bark: bool = DEFAULT_LIGHTWEIGHT_TREE_BARK


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


# ---------------------------------------------------------------------------
# Swept-tube tree system
# ---------------------------------------------------------------------------

@dataclass
class _TreeNode:
    """One node in a tree skeleton."""
    pos: np.ndarray          # 3-D world position
    radius: float            # tube radius at this point
    tangent: np.ndarray      # unit direction of growth arriving at this node
    children: "list[_TreeNode]"


_TREE_RING_VERTS = 18  # vertices per cross-section ring
_TREE_MAX_ATTRACTORS = 16  # cap per tree; controls recursive branch density
_TREE_OVAL_RATIO = 1.45  # width multiplier along the surface-tangent axis
_TREE_BARK_MAX_DEPTH = 1.15  # mm of inward carve at the base of a mature trunk
_TREE_BARK_TRENCH_WIDTH = 0.55  # mm full-width of each V-shaped trench
_TREE_BARK_DEPTH_EXPONENT = 0.70  # <1 keeps bark visible higher up the tree
_TREE_BARK_GROOVE_COUNT = 7.0  # dominant V-shaped trenches around each tree


def _transport_frame(
    tangent: np.ndarray,
    side: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    """Parallel-transport a side axis to be perpendicular to a new tangent."""
    tangent = tangent / max(float(np.linalg.norm(tangent)), 1e-8)
    side = side - tangent * float(np.dot(side, tangent))
    side_norm = float(np.linalg.norm(side))
    if side_norm < 1e-8:
        # Tangent is nearly parallel to side — pick an arbitrary perp.
        perp = np.array([1.0, 0.0, 0.0])
        if abs(float(np.dot(perp, tangent))) > 0.9:
            perp = np.array([0.0, 1.0, 0.0])
        side = perp - tangent * float(np.dot(perp, tangent))
        side = side / max(float(np.linalg.norm(side)), 1e-8)
    else:
        side = side / side_norm
    up = np.cross(tangent, side)
    up = up / max(float(np.linalg.norm(up)), 1e-8)
    return side, up


def _ring_vertices(
    center: np.ndarray,
    radius: float,
    side: np.ndarray,
    up: np.ndarray,
    side_scale: float = 1.0,
) -> np.ndarray:
    """Return (RING_VERTS, 3) array of ring vertex positions.

    side_scale > 1 produces an oval cross-section widened along the *side* axis
    (aligned with the surface tangent at the tree base via parallel transport).
    """
    n = _TREE_RING_VERTS
    angles = np.linspace(0.0, 2.0 * np.pi, n, endpoint=False)
    return (
        center
        + np.outer(np.cos(angles), side) * (radius * side_scale)
        + np.outer(np.sin(angles), up) * radius
    )


def _connect_rings(
    verts: list[np.ndarray],
    faces: list[list[int]],
    lower_start: int,
    upper_start: int,
    n: int,
) -> None:
    """Stitch two rings (each n verts) with quads (2 tris each)."""
    for i in range(n):
        j = (i + 1) % n
        faces.append([lower_start + i, lower_start + j, upper_start + j])
        faces.append([lower_start + i, upper_start + j, upper_start + i])


def _build_tree_skeleton(
    base_pos: np.ndarray,
    attractors: np.ndarray,        # (M, 3) world-space attractor positions
    trunk_radius: float,
    tip_radius: float,
    z_max: float,
    seed: float = 0.0,
    _level: int = 0,
    _parent_tangent: np.ndarray | None = None,
) -> _TreeNode:
    """Recursively build a minimal tree skeleton (branch junctions + leaf tips only).

    No intermediate nodes are added along edges — that keeps the piece count
    low so the per-tree manifold union stays fast.
    """
    rng = np.random.default_rng(int(abs(seed * 1e6)) & 0xFFFFFFFF)

    if _parent_tangent is None:
        _parent_tangent = np.array([0.0, 0.0, 1.0])

    # No attractors or at the ceiling → single straight tip.
    if len(attractors) == 0 or base_pos[2] >= z_max - 1.0:
        tip_pos = base_pos.copy()
        tip_pos[2] = z_max
        tip = _TreeNode(pos=tip_pos, radius=tip_radius, tangent=_parent_tangent, children=[])
        return _TreeNode(pos=base_pos, radius=trunk_radius, tangent=_parent_tangent, children=[tip])

    crown = attractors.mean(axis=0)
    raw_dir = crown - base_pos
    raw_norm = float(np.linalg.norm(raw_dir))
    raw_dir = raw_dir / raw_norm if raw_norm > 1e-6 else _parent_tangent.copy()

    # Blend: trunk leans vertical; higher-level branches follow crown more.
    # Keeping more vertical bias at deeper levels avoids near-horizontal top branches.
    vertical_blend = max(0.1, 0.7 - _level * 0.15)
    direction = _parent_tangent * vertical_blend + raw_dir * (1.0 - vertical_blend)
    direction = direction / max(float(np.linalg.norm(direction)), 1e-8)

    sway = rng.uniform(-0.10, 0.10, size=3)
    sway[2] = 0.0
    direction = (direction + sway)
    direction = direction / max(float(np.linalg.norm(direction)), 1e-8)

    # Terminal: ≤2 attractors or max depth → grow directly to each.
    if len(attractors) <= 2 or _level >= 4:
        node = _TreeNode(pos=base_pos, radius=trunk_radius, tangent=direction, children=[])
        for attr in attractors:
            tip_dir = attr - base_pos
            tip_dir = tip_dir / max(float(np.linalg.norm(tip_dir)), 1e-8)
            node.children.append(
                _TreeNode(pos=attr.copy(), radius=tip_radius, tangent=tip_dir, children=[])
            )
        return node

    # Binary split at a single junction node — no intermediate nodes.
    # Lower branch_frac keeps splits in the lower portion of each span so top
    # branches have more vertical height and gentler angles for FDM printability.
    span = z_max - base_pos[2]
    branch_frac = 0.20 + _level * 0.08 + float(rng.uniform(-0.04, 0.04))
    branch_z = min(z_max - 2.0, base_pos[2] + span * branch_frac)
    dz = branch_z - base_pos[2]
    branch_pos = base_pos + direction * (dz / max(direction[2], 0.05))
    branch_pos[2] = branch_z

    child_r = max(tip_radius * 1.5, trunk_radius * (0.5 ** (1.0 / 2.5)))

    # Split attractors on widest XY axis.
    spread = attractors.max(axis=0) - attractors.min(axis=0)
    split_axis = int(np.argmax(spread[:2]))
    median = float(np.median(attractors[:, split_axis]))
    left_mask = attractors[:, split_axis] <= median
    right_mask = ~left_mask
    if not left_mask.any():
        left_mask[0] = True; right_mask[0] = False
    if not right_mask.any():
        right_mask[-1] = True; left_mask[-1] = False

    branch_node = _TreeNode(pos=branch_pos, radius=trunk_radius * 0.78, tangent=direction, children=[])
    for mask, child_seed in [(left_mask, seed + 1.3), (right_mask, seed + 2.7)]:
        branch_node.children.append(
            _build_tree_skeleton(
                branch_pos.copy(), attractors[mask], child_r, tip_radius,
                z_max, seed=child_seed, _level=_level + 1, _parent_tangent=direction,
            )
        )

    return _TreeNode(pos=base_pos, radius=trunk_radius, tangent=direction, children=[branch_node])


def _swept_tube_along_path(
    path_points: list[np.ndarray],
    radii: list[float],
    sections: int = _TREE_RING_VERTS,
    initial_side: np.ndarray | None = None,
    side_scale: float = 1.0,
) -> trimesh.Trimesh:
    """Single watertight mesh swept along a polyline, capped at both ends.

    Uses parallel-transport frames so the cross-section never twists.
    No boolean operations — one mesh built directly.

    initial_side: seed direction for the parallel-transport frame's side axis.
        Pass the surface tangent at the tree base so the oval wide axis stays
        aligned with that surface throughout the branch.  Defaults to (1,0,0).
    side_scale: oval ratio — the side axis is widened by this factor.
    """
    n = sections
    n_pts = len(path_points)
    if n_pts < 2:
        return trimesh.Trimesh()

    # Tangents at each sample point.
    tangents: list[np.ndarray] = []
    for i in range(n_pts):
        if i == 0:
            t = path_points[1] - path_points[0]
        elif i == n_pts - 1:
            t = path_points[-1] - path_points[-2]
        else:
            t = path_points[i + 1] - path_points[i - 1]
        tn = float(np.linalg.norm(t))
        tangents.append(t / tn if tn > 1e-8 else np.array([0.0, 0.0, 1.0]))

    # Parallel-transport frame seeded with the requested side axis so the oval
    # stays aligned with the surface tangent throughout the branch.
    side = initial_side.copy() if initial_side is not None else np.array([1.0, 0.0, 0.0])
    side, up = _transport_frame(tangents[0], side)

    all_verts: list[list[float]] = []
    all_faces: list[list[int]] = []
    ring_starts: list[int] = []

    for pos, r, tang in zip(path_points, radii, tangents):
        side, up = _transport_frame(tang, side)
        ring = _ring_vertices(pos, r, side, up, side_scale=side_scale)
        ring_starts.append(len(all_verts))
        all_verts.extend(ring.tolist())

    # Lateral quads connecting adjacent rings.
    for i in range(n_pts - 1):
        _connect_rings([], all_faces, ring_starts[i], ring_starts[i + 1], n)

    # Bottom cap (inward-facing fan).
    bc = len(all_verts)
    all_verts.append(path_points[0].tolist())
    for j in range(n):
        all_faces.append([bc, ring_starts[0] + (j + 1) % n, ring_starts[0] + j])

    # Top cap (outward-facing fan).
    tc = len(all_verts)
    all_verts.append(path_points[-1].tolist())
    last = ring_starts[-1]
    for j in range(n):
        all_faces.append([tc, last + j, last + (j + 1) % n])

    mesh = trimesh.Trimesh(
        vertices=np.array(all_verts, dtype=np.float64),
        faces=np.array(all_faces, dtype=np.int64),
        process=False,
    )
    mesh.merge_vertices(digits_vertex=3)
    mesh.update_faces(mesh.nondegenerate_faces())
    mesh.remove_unreferenced_vertices()
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()
    return mesh


_BEZIER_CURVE_SEGMENTS = 8   # samples along each branch Bezier arc


def _bezier_branch_tube(
    p0: np.ndarray,
    p1: np.ndarray,
    r0: float,
    r1: float,
    bow: float = 0.14,
    curve_segments: int = _BEZIER_CURVE_SEGMENTS,
    end_tangent: np.ndarray | None = None,
    initial_side: np.ndarray | None = None,
    side_scale: float = 1.0,
) -> trimesh.Trimesh:
    """Curved swept tube along a Bezier arc from p0 to p1.

    bow: fraction of branch length used as curvature offset (upward + lateral).

    end_tangent: when provided, a cubic Bezier is used so the tube arrives at
    p1 with exactly that tangent direction.  Pass (0,0,1) to arrive
    perpendicular to a horizontal underside surface, eliminating angular
    clip-through artefacts.  Without end_tangent a simpler quadratic arc is used.
    """
    direction = p1 - p0
    length = float(np.linalg.norm(direction))
    if length < 1e-6:
        return trimesh.Trimesh()

    horiz = direction.copy(); horiz[2] = 0.0
    horiz_len = float(np.linalg.norm(horiz))

    if end_tangent is not None:
        # Cubic Bezier with constrained end tangent.
        # c1: follows the natural branch direction with lateral bow.
        # c2: approaches p1 from below along end_tangent.
        unit_dir = direction / length
        if horiz_len > 1e-6:
            perp = np.array([-horiz[1], horiz[0], 0.0]) / horiz_len
            unit_dir = unit_dir + perp * (bow * 0.45)
            unit_dir = unit_dir / max(float(np.linalg.norm(unit_dir)), 1e-8)
        scale = length / 3.0
        c1 = p0 + unit_dir * scale
        end_t = np.asarray(end_tangent, dtype=float)
        end_t = end_t / max(float(np.linalg.norm(end_t)), 1e-8)
        c2 = p1 - end_t * scale

        path: list[np.ndarray] = []
        radii: list[float] = []
        for i in range(curve_segments):
            t = i / (curve_segments - 1)
            tm = 1.0 - t
            pos = tm**3 * p0 + 3.0*tm**2*t * c1 + 3.0*tm*t**2 * c2 + t**3 * p1
            path.append(pos)
            radii.append(r0 + (r1 - r0) * t)
        return _swept_tube_along_path(path, radii, initial_side=initial_side, side_scale=side_scale)

    # Quadratic Bezier (no end-tangent constraint).
    offset = np.array([0.0, 0.0, bow * length])
    if horiz_len > 1e-6:
        perp = np.array([-horiz[1], horiz[0], 0.0]) / horiz_len
        offset += perp * (bow * 0.45 * length)
    control = (p0 + p1) / 2.0 + offset

    path = []
    radii = []
    for i in range(curve_segments):
        t = i / (curve_segments - 1)
        tm = 1.0 - t
        pos = tm * tm * p0 + 2.0 * tm * t * control + t * t * p1
        path.append(pos)
        radii.append(r0 + (r1 - r0) * t)

    return _swept_tube_along_path(path, radii, initial_side=initial_side, side_scale=side_scale)


def _skeleton_to_mesh(
    root: _TreeNode,
    flat_base: bool = False,
    initial_side: np.ndarray | None = None,
    side_scale: float = 1.0,
) -> trimesh.Trimesh:
    """Convert a tree skeleton to a watertight mesh.

    Each edge becomes a curved swept tube (Bezier arc, no per-branch booleans).
    Junction nodes get a small oval for a natural knuckle.
    All pieces are merged in one manifold union call per tree.

    flat_base: skip the sphere at the root node so the swept tube's own flat
    end-cap becomes the base face (no sphere bump on the attachment surface).
    initial_side / side_scale: the oval wide axis is aligned with initial_side
    (surface tangent) via parallel transport.  Junction ovoids are stretched
    the same way so they fill the crotch of each branch fork.
    """
    pieces: list[trimesh.Trimesh] = []
    _is_root = [True]
    _side_seed = initial_side.copy() if initial_side is not None else np.array([1.0, 0.0, 0.0])

    def walk(node: _TreeNode) -> None:
        is_root = _is_root[0]
        _is_root[0] = False
        is_leaf = len(node.children) == 0
        # Ovoids at branch junctions blend overlapping tube ends at crotches.
        # Skip at the root (flat_base=True → flat floor cap suffices) and at
        # leaf tips (flat tube end-cap is the tip; sphere would add unwanted bulk
        # and pull the effective tip away from the target surface).
        if not (flat_base and is_root) and not is_leaf:
            sphere = trimesh.creation.icosphere(subdivisions=2, radius=node.radius * 1.08)
            if side_scale != 1.0:
                # Stretch the sphere along the locally-transported side axis so the
                # ovoid fills the oval cross-sections of both incoming branches.
                local_side, _ = _transport_frame(node.tangent, _side_seed)
                verts = sphere.vertices
                proj = (verts @ local_side)[:, np.newaxis] * local_side
                sphere.vertices = verts + (side_scale - 1.0) * proj
            sphere.apply_translation(node.pos)
            pieces.append(sphere)

        for child in node.children:
            is_tip = not child.children
            # Terminal branches get extra samples for smoother taper and a
            # forced vertical end-tangent so the flat cap lands flush against
            # the horizontal underside surface without angled clip-through.
            tube = _bezier_branch_tube(
                node.pos, child.pos, node.radius, child.radius,
                curve_segments=_BEZIER_CURVE_SEGMENTS + (1 if is_tip else 0),
                end_tangent=np.array([0.0, 0.0, 1.0]) if is_tip else None,
                initial_side=initial_side,
                side_scale=side_scale,
            )
            if len(tube.vertices) > 0:
                pieces.append(tube)
            walk(child)

    walk(root)

    if not pieces:
        return trimesh.Trimesh()

    valid = []
    for p in pieces:
        p.merge_vertices(digits_vertex=3)
        p.update_faces(p.nondegenerate_faces())
        p.remove_unreferenced_vertices()
        p.fix_normals()
        if p.is_volume and p.volume > 0:
            valid.append(p)
    if not valid:
        return trimesh.Trimesh()

    result = trimesh.boolean.union(valid, engine="manifold")
    result.fix_normals()
    if result.volume < 0:
        result.invert()
    return result


def _evenly_sample_attractors(attractors: np.ndarray, count: int) -> np.ndarray:
    """Select attractors with broad geometric coverage over the supported patch."""
    if len(attractors) <= count:
        return attractors

    points = attractors[:, :2]
    mins = points.min(axis=0)
    span = np.ptp(points, axis=0)
    span[span < 1e-6] = 1.0
    normalized = (points - mins) / span

    selected: list[int] = []
    center = normalized.mean(axis=0)
    first = int(np.argmin(np.sum((normalized - center) ** 2, axis=1)))
    selected.append(first)

    min_d2 = np.sum((normalized - normalized[first]) ** 2, axis=1)
    for _ in range(1, count):
        next_idx = int(np.argmax(min_d2))
        selected.append(next_idx)
        d2 = np.sum((normalized - normalized[next_idx]) ** 2, axis=1)
        min_d2 = np.minimum(min_d2, d2)

    return attractors[np.array(selected, dtype=int)]


def _smoothstep(edge0: float, edge1: float, x: np.ndarray) -> np.ndarray:
    t = np.clip((x - edge0) / max(edge1 - edge0, 1e-8), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def _value_noise_1d(x: np.ndarray, seed: float) -> np.ndarray:
    xi = np.floor(x).astype(np.int64)
    xf = x - xi
    fade = xf * xf * (3.0 - 2.0 * xf)
    phase = seed * 12.9898
    a = np.sin((xi * 127.1 + phase) * 43758.5453)
    b = np.sin(((xi + 1) * 127.1 + phase) * 43758.5453)
    a = a - np.floor(a)
    b = b - np.floor(b)
    return a * (1.0 - fade) + b * fade


def _periodic_v_trenches(
    phase: np.ndarray,
    groove_count: float,
    radius: np.ndarray,
    width_mm: float,
) -> np.ndarray:
    phase_dist = np.abs(np.arctan2(np.sin(phase), np.cos(phase)))
    angular_dist = phase_dist / max(groove_count, 1e-6)
    surface_dist = np.maximum(radius, 0.2) * angular_dist
    half_width = max(width_mm * 0.5, 1e-6)
    return np.clip(1.0 - surface_dist / half_width, 0.0, 1.0)


def _tree_bark_segments(
    root: _TreeNode,
    initial_side: np.ndarray | None,
) -> tuple[list[tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray, float, float]], float]:
    side_seed = initial_side.copy() if initial_side is not None else np.array([1.0, 0.0, 0.0])
    side_seed, _ = _transport_frame(root.tangent, side_seed)
    segments: list[tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray, float, float]] = []
    max_dist = 0.0

    def walk(node: _TreeNode, side: np.ndarray, dist0: float) -> None:
        nonlocal max_dist
        for child in node.children:
            axis = child.pos - node.pos
            length = float(np.linalg.norm(axis))
            if length < 1e-6:
                walk(child, side, dist0)
                continue
            tangent = axis / length
            local_side, local_up = _transport_frame(tangent, side)
            segments.append((node.pos.copy(), child.pos.copy(), tangent, local_side, local_up, dist0, length))
            dist1 = dist0 + length
            max_dist = max(max_dist, dist1)
            walk(child, local_side, dist1)

    walk(root, side_seed, 0.0)
    return segments, max(max_dist, 1e-6)


def _carve_bark_texture(
    mesh: trimesh.Trimesh,
    root: _TreeNode,
    base_pos: np.ndarray,
    z_max: float,
    seed: float,
    initial_side: np.ndarray | None = None,
    max_depth: float = _TREE_BARK_MAX_DEPTH,
) -> trimesh.Trimesh:
    """Displace side-surface vertices inward to create shallow bark trenches.

    The relief is deepest at the base and follows a curved falloff to zero at
    the branch tips.  The trench field is evaluated against the nearest tree
    skeleton segment, so it runs from trunk into both sides of branch splits.
    """
    if len(mesh.vertices) == 0 or max_depth <= 0.0:
        return mesh
    segments, max_tree_dist = _tree_bark_segments(root, initial_side)
    if not segments:
        return mesh

    vertices = mesh.vertices.copy()
    normals = mesh.vertex_normals

    best_d2 = np.full(len(vertices), np.inf, dtype=np.float64)
    best_theta = np.zeros(len(vertices), dtype=np.float64)
    best_t = np.zeros(len(vertices), dtype=np.float64)
    best_side_weight = np.zeros(len(vertices), dtype=np.float64)
    best_radius = np.zeros(len(vertices), dtype=np.float64)
    for p0, _p1, tangent, side, up, dist0, length in segments:
        rel = vertices - p0
        along = np.clip(rel @ tangent, 0.0, length)
        closest = p0 + along[:, np.newaxis] * tangent
        radial = vertices - closest
        d2 = np.einsum("ij,ij->i", radial, radial)
        update = d2 < best_d2
        if not np.any(update):
            continue
        best_d2[update] = d2[update]
        best_theta[update] = np.arctan2(radial[update] @ up, radial[update] @ side)
        best_t[update] = np.clip((dist0 + along[update]) / max_tree_dist, 0.0, 1.0)
        best_radius[update] = np.sqrt(np.maximum(d2[update], 1e-8))
        normal_axis = np.abs(normals[update] @ tangent)
        best_side_weight[update] = np.clip(1.0 - normal_axis, 0.0, 1.0) ** 2

    t = best_t
    theta = best_theta
    radius = best_radius
    side_weight = best_side_weight
    depth_falloff = np.power(np.clip(1.0 - t, 0.0, 1.0), _TREE_BARK_DEPTH_EXPONENT)
    if not np.any((depth_falloff > 1e-4) & (side_weight > 1e-4)):
        return mesh

    rng = np.random.default_rng(int(abs(seed * 1e6)) & 0xFFFFFFFF)
    swirl_turns = float(rng.uniform(0.08, 0.18))
    phase_seed = float(rng.uniform(0.0, 1000.0))
    phase_noise = (_value_noise_1d(t * 5.0 + phase_seed, seed + 3.17) - 0.5) * 0.28

    main_phase = (
        _TREE_BARK_GROOVE_COUNT
        * (theta + 2.0 * np.pi * swirl_turns * t + phase_noise)
        + float(rng.uniform(0.0, 2.0 * np.pi))
    )
    main_grooves = _periodic_v_trenches(
        main_phase,
        _TREE_BARK_GROOVE_COUNT,
        radius,
        _TREE_BARK_TRENCH_WIDTH,
    )
    organic_depth = 0.88 + 0.12 * _value_noise_1d(theta * 2.8 + t * 11.0, seed + 15.73)
    groove_field = np.clip(main_grooves * organic_depth, 0.0, 1.0)

    displacement = max_depth * depth_falloff * side_weight * groove_field
    vertices -= normals * displacement[:, np.newaxis]
    vertices[:, 2] = np.maximum(vertices[:, 2], float(base_pos[2]))

    carved = trimesh.Trimesh(vertices=vertices, faces=mesh.faces.copy(), process=False)
    carved.update_faces(carved.nondegenerate_faces())
    carved.remove_unreferenced_vertices()
    carved.fix_normals()
    if carved.volume < 0:
        carved.invert()
    if not carved.is_volume:
        carved = cap_triangular_boundary_loops(carved)
        carved = _collapse_non_manifold_edges(carved)
    return carved


def _make_tree(
    base_pos: np.ndarray,
    attractors: np.ndarray,
    trunk_radius: float,
    tip_radius: float,
    z_max: float,
    seed: float = 0.0,
    max_attractors: int = _TREE_MAX_ATTRACTORS,
    flat_base: bool = False,
    initial_side: np.ndarray | None = None,
    side_scale: float = 1.0,
    bark: bool = False,
) -> trimesh.Trimesh:
    """Build one swept-tube tree mesh."""
    # Cap attractor count — this directly limits terminal tips and recursive
    # splits, keeping the generated branch count predictable.
    if len(attractors) > max_attractors:
        attractors = _evenly_sample_attractors(attractors, max_attractors)
    root = _build_tree_skeleton(base_pos, attractors, trunk_radius, tip_radius, z_max, seed=seed)
    mesh = _skeleton_to_mesh(root, flat_base=flat_base, initial_side=initial_side, side_scale=side_scale)
    if not bark:
        return mesh
    bark_depth = min(_TREE_BARK_MAX_DEPTH, max(0.35, trunk_radius * 0.55))
    return _carve_bark_texture(
        mesh,
        root,
        base_pos,
        z_max,
        seed=seed,
        initial_side=initial_side,
        max_depth=bark_depth,
    )


def _side_tree_corner_s_positions(
    spec: OrganizerSpec,
    inset: float,
) -> list[float]:
    """Return arc-length positions at the mid-point of each of the 4 rounded corners."""
    r_eff = max(0.1, spec.vertical_corner_roundover - inset)
    arc_quarter = (np.pi / 2.0) * r_eff
    straight_x = max(0.0, spec.width - 2.0 * spec.vertical_corner_roundover)
    straight_y = max(0.0, spec.depth - 2.0 * spec.vertical_corner_roundover)

    # Perimeter starts at the bottom-left arc end, traces CCW:
    #   bottom straight → bottom-right arc → right straight → top-right arc
    #   → top straight → top-left arc → left straight → bottom-left arc
    s_br = straight_x + arc_quarter / 2.0
    s_tr = s_br + arc_quarter / 2.0 + straight_y + arc_quarter / 2.0
    s_tl = s_tr + arc_quarter / 2.0 + straight_x + arc_quarter / 2.0
    s_bl = s_tl + arc_quarter / 2.0 + straight_y + arc_quarter / 2.0
    return [s_br, s_tr, s_tl, s_bl]


def _side_tree_base_s_positions(spec: OrganizerSpec, inset: float) -> list[float]:
    """Arc-length positions for all 12 side-tree bases (4 corners + 2 per side)."""
    vcr = spec.vertical_corner_roundover
    r_eff = max(0.1, vcr - inset)
    arc_quarter = (np.pi / 2.0) * r_eff
    straight_x = max(0.0, spec.width - 2.0 * vcr)
    straight_y = max(0.0, spec.depth - 2.0 * vcr)
    corner_s = _side_tree_corner_s_positions(spec, inset)
    s_br, s_tr, s_tl, _ = corner_s

    section_data = [
        (0.0,                          straight_x),   # bottom
        (s_br + arc_quarter / 2.0,     straight_y),   # right
        (s_tr + arc_quarter / 2.0,     straight_x),   # top
        (s_tl + arc_quarter / 2.0,     straight_y),   # left
    ]
    extra_s: list[float] = []
    for sec_start, sec_len in section_data:
        if sec_len > 10.0:
            extra_s.append(float(sec_start + sec_len / 3.0))
            extra_s.append(float(sec_start + 2.0 * sec_len / 3.0))
    return corner_s + extra_s


def side_tree_struts(
    spec: OrganizerSpec,
    lightweight: LightweightSpec,
    z_min: float,
    z_max: float,
) -> list[trimesh.Trimesh]:
    rail_depth = lightweight.magnet_boss_depth
    inset = rail_depth / 2.0
    # Keep enough root diameter for stability without filling the whole rail depth.
    trunk_radius = max(1.15, lightweight.rib_width * 0.45, rail_depth * 0.34)
    tip_radius = max(0.4, trunk_radius * 0.18)

    perimeter, map_point = rounded_perimeter_mapper(spec, inset)

    base_s_positions = _side_tree_base_s_positions(spec, inset)
    base_pts = np.array([map_point(s, z_min)[0] for s in base_s_positions])

    # --- 2-D grid attractors: inset from every edge of the top rail by tip_radius ---
    # Z: tip sphere bottom sits at z_max → centre at z_max + tip_radius.
    # XY: usable half-depth = rail_depth/2 - tip_radius so every sphere edge
    #     lands ≥ tip_radius inside both the outer and inner rail faces.
    # Inset attractor centres by tip_radius so the tube EDGE lands at the rail
    # face, not the centre.  Z is exempt — flat cap top is already at attr_z.
    attr_z = z_max
    usable_half = rail_depth / 2.0 - tip_radius
    n_perim = max(60, int(np.ceil(perimeter / 2.5)))
    depth_steps = 6
    attractors_list: list[np.ndarray] = []
    attractor_s_vals: list[float] = []
    for i in range(n_perim):
        s = perimeter * i / n_perim
        pt, outward_normal = map_point(s, attr_z)
        for d in range(depth_steps):
            # d=0 → outer inset face; d=depth_steps-1 → inner inset face
            frac = d / (depth_steps - 1)
            offset = (1.0 - 2.0 * frac) * usable_half
            attractors_list.append(pt + outward_normal * offset)
            attractor_s_vals.append(s)
    attractors = np.array(attractors_list)
    attractor_s = np.array(attractor_s_vals)

    # Voronoi assignment by arc-length, not XY distance.
    # XY distance misassigns corner attractors to adjacent straight-section
    # trees (which are closer in XY despite being far along the perimeter).
    # Arc-length wraps correctly around the closed perimeter loop.
    base_s_arr = np.array(base_s_positions)
    if len(base_s_arr) > 1:
        raw_diff = np.abs(attractor_s[:, None] - base_s_arr[None, :])
        arc_dists = np.minimum(raw_diff, perimeter - raw_diff)
        assignments = np.argmin(arc_dists, axis=1)
    else:
        assignments = np.zeros(len(attractors), dtype=int)

    meshes: list[trimesh.Trimesh] = []
    for si in range(len(base_s_positions)):
        tree_attractors = attractors[assignments == si]
        if len(tree_attractors) == 0:
            tree_attractors = attractors[:max(4, len(attractors) // len(base_s_positions))]
        # Surface tangent at this base point: rotate the outward normal +90° in XY.
        _, outward_normal = map_point(base_s_positions[si], z_min)
        surface_tangent = np.array([-outward_normal[1], outward_normal[0], 0.0], dtype=float)
        mesh = _make_tree(
            base_pts[si].copy(), tree_attractors,
            trunk_radius, tip_radius, z_max,
            seed=float(si) * 1.618,
            flat_base=True,
            initial_side=surface_tangent,
            side_scale=_TREE_OVAL_RATIO,
            bark=lightweight.tree_bark,
        )
        if len(mesh.vertices) > 0:
            meshes.append(mesh)
    return meshes


def cylindrical_tree_struts(
    center: tuple[float, float],
    radius: float,
    wall_depth: float,
    z_min: float,
    z_max: float,
    lightweight: LightweightSpec,
    phase: float = 0.0,
) -> list[trimesh.Trimesh]:
    # Slimmer roots reduce volume while preserving a printable base and bore clearance.
    trunk_radius = max(1.10, lightweight.rib_width * 0.45, wall_depth * 0.40)
    tip_radius = max(0.4, trunk_radius * 0.18)
    center_xy = np.asarray(center, dtype=float)
    n_trees = 6   # evenly spaced at 60° intervals + phase

    # Trunk bases sit just inside the ring, biased toward the inner face but
    # offset outward by trunk_radius + 0.5 mm so the cup-bore cutter
    # (which reaches to cup_bore_r = radius - wall_depth/2) cannot intersect
    # the trunk body.
    # Top ring spans radius → radius + wall_depth/2 radially.
    # Attractors are inset from both radial edges and from the bottom z face
    # by tip_radius so every tip sphere lands fully inside the top ring volume.
    cup_bore_r = radius - wall_depth / 2.0     # inner edge of ring = bore cut radius
    base_r = cup_bore_r + trunk_radius + 0.5   # just clear of bore surface
    # Attractors must span the FULL ring underside: from the bore edge out to the
    # outer ring face.  raw_inner_r is the bore radius (not the midpoint).
    raw_inner_r = cup_bore_r                   # actual inner edge of top ring zone
    raw_outer_r = radius + wall_depth / 2.0   # physical outer edge of top ring zone
    # Inset attractor centres by tip_radius so the tube EDGE (not centre)
    # lands at each surface boundary.  Z is exempt — the flat cap top IS at
    # attr_z so no inset is needed in that direction.
    inner_r = raw_inner_r + tip_radius
    outer_r = raw_outer_r - tip_radius
    if inner_r > outer_r:
        inner_r = outer_r = (raw_inner_r + raw_outer_r) / 2.0

    # Build attractors with uniform areal density: scale angular sample count
    # proportionally to circumference at each radial step so inner and outer
    # rings get the same point spacing (~3 mm target).
    attr_z = z_max   # flat cap top lands exactly on the ring underside
    target_spacing = 3.0   # mm between attractor points
    depth_steps = 6
    attractors_list: list[np.ndarray] = []
    for d in range(depth_steps):
        frac = d / max(depth_steps - 1, 1)
        r_here = inner_r + frac * (outer_r - inner_r)
        n_at_r = max(4, int(np.ceil(2.0 * np.pi * r_here / target_spacing)))
        for i in range(n_at_r):
            angle = 2.0 * np.pi * i / n_at_r
            attractors_list.append(np.array([
                center_xy[0] + r_here * np.cos(angle),
                center_xy[1] + r_here * np.sin(angle),
                float(attr_z),
            ]))
    attractors = np.array(attractors_list)

    sector_half = np.pi / n_trees   # 45° half-angle for 4 trees
    meshes: list[trimesh.Trimesh] = []

    for ti in range(n_trees):
        tree_angle = 2.0 * np.pi * ti / n_trees + phase
        # Base on the outer cup surface (base_r), not the wider top-ring radius.
        base_pos = np.array([
            center_xy[0] + base_r * np.cos(tree_angle),
            center_xy[1] + base_r * np.sin(tree_angle),
            float(z_min),
        ])

        attr_angles = np.arctan2(
            attractors[:, 1] - center_xy[1],
            attractors[:, 0] - center_xy[0],
        )
        angular_diff = np.abs(((attr_angles - tree_angle + np.pi) % (2.0 * np.pi)) - np.pi)
        tree_attractors = attractors[angular_diff <= sector_half]
        if len(tree_attractors) == 0:
            tree_attractors = attractors

        # Circumferential tangent at this tree angle (tangent to the cup wall).
        surface_tangent = np.array([-np.sin(tree_angle), np.cos(tree_angle), 0.0], dtype=float)
        mesh = _make_tree(
            base_pos, tree_attractors,
            trunk_radius, tip_radius, z_max,
            seed=float(ti) * 2.718 + phase,
            flat_base=True,
            initial_side=surface_tangent,
            side_scale=_TREE_OVAL_RATIO,
            bark=lightweight.tree_bark,
        )
        if len(mesh.vertices) > 0:
            meshes.append(mesh)
    return meshes


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


def _rounded_top_boss(
    boss_width: float,
    boss_depth: float,
    boss_height: float,
    fillet: float,
    arc_segments: int = 12,
) -> trimesh.Trimesh:
    """Boss extruded from a rounded-top-rectangle profile (in the wall-facing view).

    Returned in 'front-side frame': X in [-boss_width/2, +boss_width/2],
    Y in [0, boss_depth], Z in [0, boss_height]. Fillet rounds the top X-axis
    corners (the silhouette seen looking at the wall from inside the box).
    """
    import manifold3d as m3d

    half_w = boss_width / 2.0
    r = max(0.0, min(fillet, half_w - 1e-3, boss_height - 1e-3))
    if r <= 1e-6:
        return translated_box(
            (boss_width, boss_depth, boss_height),
            (0.0, boss_depth / 2.0, boss_height / 2.0),
        )

    pts: list[tuple[float, float]] = [
        (-half_w, 0.0),
        (half_w, 0.0),
        (half_w, boss_height - r),
    ]
    for i in range(1, arc_segments + 1):
        theta = i / arc_segments * (np.pi / 2.0)
        pts.append((half_w - r + r * np.cos(theta), boss_height - r + r * np.sin(theta)))
    pts.append((-half_w + r, boss_height))
    for i in range(1, arc_segments + 1):
        theta = np.pi / 2.0 + i / arc_segments * (np.pi / 2.0)
        pts.append((-half_w + r + r * np.cos(theta), boss_height - r + r * np.sin(theta)))

    cross = m3d.CrossSection([pts], fillrule=m3d.FillRule.EvenOdd)
    solid = m3d.Manifold.extrude(cross, height=boss_depth)
    raw = solid.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )
    # extrude_polygon-equivalent leaves the mesh with X=width, Y=height, Z=depth.
    # Rotate +90° around X so depth maps to +Y and height stays as +Z.
    rotation = trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    mesh.apply_transform(rotation)
    mesh.apply_translation([0.0, boss_depth, 0.0])
    mesh.fix_normals()
    return mesh


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

    # Top-corner fillet softens the boss silhouette. Capped to keep at least
    # 2 mm of flat top and to leave ≥1 mm wall around the magnet hole at its
    # tightest cross-section (verified for the default 16×14 boss + 10 mm Ø
    # magnet at z=7).
    fillet = max(0.0, min(6.0, boss_width / 2.0 - 2.0))
    boss = _rounded_top_boss(boss_width, boss_depth, boss_height, fillet)

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
        boss.apply_translation([position, spec.depth - boss_depth, 0.0])
        cutter = axis_cylinder(
            magnet_radius,
            cutter_length,
            (position, spec.depth - cutter_length / 2.0 + 0.05, z),
            (0.0, -1.0, 0.0),
            circle_segments_count,
        )
    elif side == "left":
        boss.apply_transform(trimesh.transformations.rotation_matrix(np.pi / 2.0, [0.0, 0.0, 1.0]))
        boss.apply_translation([boss_depth, position, 0.0])
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
    return boss, cutter


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
    outer_radius = inner_radius + lightweight.cup_wall
    top_ring_outer_radius = inner_radius + 2.0 * lightweight.cup_wall
    cup_segments = circle_segments(spec.hole_diameter + 2.0 * lightweight.cup_wall, circle_segment_length)
    retaining_hole_segments = circle_segments(lightweight.retaining_hole_diameter, circle_segment_length)
    magnet_segments = circle_segments(spec.magnet_diameter, circle_segment_length)

    solids: list[trimesh.Trimesh] = []
    side_solids: list[trimesh.Trimesh] = []
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
        top_ring_outer = vertical_cylinder(
            top_ring_outer_radius,
            lightweight.rib_height,
            (cx, cy, cup_height - lightweight.rib_height / 2.0),
            cup_segments,
        )
        top_ring_bevel = vertical_frustum(
            inner_radius,
            top_ring_outer_radius,
            lightweight.rib_height,
            (cx, cy, cup_height - lightweight.rib_height / 2.0),
            cup_segments,
        )
        solids.append(bottom_floor)
        solids.append(top_ring_outer)
        tree_wall_depth = top_ring_outer_radius - inner_radius
        solids.extend(
            cylindrical_tree_struts(
                (cx, cy),
                inner_radius + tree_wall_depth / 2.0,
                tree_wall_depth,
                0.0,
                cup_height - lightweight.rib_height + 0.2,
                lightweight,
                phase=(cup_index * 0.173) % 1.0,
            )
        )
        cup_cutters.append(inner)
        cup_cutters.append(retaining_hole)
        if retaining_bevel_height > 0.0:
            cup_cutters.append(retaining_bevel)
        cup_cutters.append(top_ring_bevel)

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

    rail_depth = lightweight.magnet_boss_depth
    low_rim_height = lightweight.rib_height
    top_rail_z = cup_height - lightweight.rib_height / 2.0
    solids.append(perimeter_rail_band(spec, rail_depth, low_rim_height, low_rim_height / 2.0))
    solids.append(perimeter_rail_band(spec, rail_depth, lightweight.rib_height, top_rail_z))

    # Slim pillars carry side-tree bases (z ≈ magnet_boss_height) where the
    # bottom rail no longer reaches; magnet bosses provide the same anchor at
    # the 8 magnet positions.
    anchor_inset = rail_depth / 2.0
    anchor_radius = max(1.5, lightweight.tree_strut_width * 0.65)
    anchor_sections = circle_segments(2.0 * anchor_radius)
    _, perimeter_map = rounded_perimeter_mapper(spec, anchor_inset)
    for s in _side_tree_base_s_positions(spec, anchor_inset):
        pt, _ = perimeter_map(s, 0.0)
        solids.append(
            vertical_cylinder(
                anchor_radius,
                lightweight.magnet_boss_height,
                (float(pt[0]), float(pt[1]), lightweight.magnet_boss_height / 2.0),
                anchor_sections,
            )
        )

    side_solids.extend(
        side_tree_struts(
            spec,
            lightweight,
            lightweight.magnet_z + lightweight.magnet_boss_height / 2.0 - 0.2,
            cup_height - lightweight.rib_height + 0.2,
        )
    )

    magnet_spec = replace(spec, magnet_z=lightweight.magnet_z)
    for side in ("front", "back", "left", "right"):
        for position, z in side_panel_pockets(magnet_spec, side):
            boss, cutter = magnet_boss_and_cutter(magnet_spec, lightweight, side, position, z, magnet_segments)
            solids.append(boss)
            cutters.append(cutter)

    mesh = trimesh.boolean.union([*solids, *side_solids], engine="manifold")
    if cup_cutters or cutters:
        mesh = trimesh.boolean.difference([mesh, *cup_cutters, *cutters], engine="manifold")
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
    parser.add_argument("--tree-strut-width", type=float, default=LightweightSpec.tree_strut_width, help="Organic tree branch thickness in mm.")
    parser.add_argument("--tree-top-spacing", type=float, default=LightweightSpec.tree_top_spacing, help="Approximate spacing between tree supports at top ribs in mm.")
    parser.add_argument("--tree-roots", type=int, default=LightweightSpec.tree_roots, help="Root count for each cylindrical tree support.")
    parser.add_argument("--tree-bark", action="store_true", default=LightweightSpec.tree_bark, help="Carve V-shaped bark trenches into tree supports.")
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
        tree_strut_width=args.tree_strut_width,
        tree_top_spacing=args.tree_top_spacing,
        tree_roots=args.tree_roots,
        tree_bark=args.tree_bark,
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
