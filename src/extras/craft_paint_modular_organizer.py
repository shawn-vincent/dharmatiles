#!/usr/bin/env python3
"""Generate a rounded craft-paint modular organizer STL.

The model is generated from signed-distance fields and meshed with a compact
marching-tetrahedra implementation, so it does not need external CAD or boolean
engines.  Dimensions are millimeters.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import trimesh


@dataclass(frozen=True)
class OrganizerSpec:
    width: float = 140.0
    depth: float = 150.0
    height: float = 70.0
    columns: int = 3
    rows: int = 4
    hole_diameter: float = 35.0
    floor: float = 5.0
    roundover: float = 5.0
    stagger: float = 8.0
    pitch: float = 1.0


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


def smooth_max(a: np.ndarray, b: np.ndarray, radius: float) -> np.ndarray:
    """Polynomial smooth maximum for rounded SDF subtraction."""
    if radius <= 0.0:
        return np.maximum(a, b)
    h = np.clip(0.5 + 0.5 * (a - b) / radius, 0.0, 1.0)
    return a * h + b * (1.0 - h) + radius * h * (1.0 - h)


def rounded_box_sdf(points: np.ndarray, spec: OrganizerSpec) -> np.ndarray:
    center = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
    half = np.array([spec.width / 2.0, spec.depth / 2.0, spec.height / 2.0])
    q = np.abs(points - center) - (half - spec.roundover)
    outside = np.linalg.norm(np.maximum(q, 0.0), axis=1)
    inside = np.minimum(np.maximum.reduce(q, axis=1), 0.0)
    return outside + inside - spec.roundover


def hole_centers(spec: OrganizerSpec) -> list[tuple[float, float]]:
    x_margin = spec.hole_diameter / 2.0 + spec.roundover + 3.0
    y_margin = spec.hole_diameter / 2.0 + spec.roundover + 3.0
    xs = np.linspace(x_margin, spec.width - x_margin, spec.columns)
    ys = np.linspace(y_margin, spec.depth - y_margin, spec.rows)

    centers: list[tuple[float, float]] = []
    for row, y in enumerate(ys):
        direction = -1.0 if row % 2 else 1.0
        for col, x in enumerate(xs):
            # Taper the stagger at the outside columns so every hole remains
            # comfortably inside the rounded body.
            col_bias = col - (spec.columns - 1) / 2.0
            centers.append((float(x + direction * spec.stagger * (1.0 - abs(col_bias) * 0.35)), float(y)))
    return centers


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


def build_mesh(spec: OrganizerSpec) -> trimesh.Trimesh:
    pitch = spec.pitch
    xs = np.arange(-pitch, spec.width + pitch * 1.5, pitch)
    ys = np.arange(-pitch, spec.depth + pitch * 1.5, pitch)
    zs = np.arange(-pitch, spec.height + pitch * 1.5, pitch)

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
    parser.add_argument("--output", type=Path, default=Path("@extras/craft-paint-modular-organizer.stl"))
    parser.add_argument("--pitch", type=float, default=OrganizerSpec.pitch, help="Meshing pitch in mm.")
    parser.add_argument("--stagger", type=float, default=OrganizerSpec.stagger, help="Alternate-row hole stagger in mm.")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    spec = OrganizerSpec(pitch=args.pitch, stagger=args.stagger)
    mesh = build_mesh(spec)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(args.output)
    print(f"Wrote {args.output}")
    print(f"Vertices: {len(mesh.vertices):,}")
    print(f"Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:,.1f} mm^3")


if __name__ == "__main__":
    main()
