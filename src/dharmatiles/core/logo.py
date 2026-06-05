"""Vector logo emboss: parse SVG path → extrude → Boolean inset.

The dharmatiles logo is stored as an SVG in the assets folder.  This module
parses the path geometry directly (no rasterisation), producing a clean
manifold solid that is subtracted from the base mesh to create a smooth,
vector-accurate inset emboss.

Public API
----------
make_logo_inset(cx, cy, size_mm, z_base, depth_mm) → trimesh.Trimesh
    A watertight solid (the cutter) for use with trimesh.boolean.difference.
"""
from __future__ import annotations

import pathlib
import re
import xml.etree.ElementTree as ET

import numpy as np
import trimesh

_SVG_PATH = pathlib.Path(__file__).parent.parent / 'assets' / 'dharmatiles-logo.svg'
_SVG_VIEWBOX = 1024.0   # logo is defined in a 1024 × 1024 px square viewBox


# ── SVG path parser ───────────────────────────────────────────────────────────

def _parse_svg_d(d: str, tol_px: float = 1.5) -> list[list[tuple[float, float]]]:
    """Parse an SVG path *d* attribute into closed polygon contours.

    Handles absolute M / C / L / Z only (sufficient for this logo).
    Cubic bezier curves are adaptively subdivided until the maximum deviation
    of the control-point hull from the chord is below *tol_px* SVG units.

    Returns a list of contours; each contour is a list of (x, y) tuples in
    SVG coordinate space (Y increases downward, origin at top-left).
    """
    toks = re.findall(
        r'[MCLZmclz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?', d
    )
    pos = 0

    def read() -> float:
        nonlocal pos
        v = float(toks[pos]); pos += 1; return v

    def _flat(p0, p1, p2, p3) -> bool:
        """Return True if the bezier hull deviation is within *tol_px*."""
        ux = 3*p1[0] - 2*p0[0] - p3[0];  uy = 3*p1[1] - 2*p0[1] - p3[1]
        vx = 3*p2[0] - 2*p3[0] - p0[0];  vy = 3*p2[1] - 2*p3[1] - p0[1]
        return max(ux*ux + uy*uy, vx*vx + vy*vy) <= 16.0 * tol_px * tol_px

    def _subdivide(p0, p1, p2, p3) -> list[tuple[float, float]]:
        """Recursively halve a cubic bezier until flat; return points after p0."""
        if _flat(p0, p1, p2, p3):
            return [p3]
        m01  = ((p0[0]+p1[0])/2, (p0[1]+p1[1])/2)
        m12  = ((p1[0]+p2[0])/2, (p1[1]+p2[1])/2)
        m23  = ((p2[0]+p3[0])/2, (p2[1]+p3[1])/2)
        m012 = ((m01[0]+m12[0])/2, (m01[1]+m12[1])/2)
        m123 = ((m12[0]+m23[0])/2, (m12[1]+m23[1])/2)
        mid  = ((m012[0]+m123[0])/2, (m012[1]+m123[1])/2)
        return _subdivide(p0, m01, m012, mid) + _subdivide(mid, m123, m23, p3)

    contours: list[list[tuple[float, float]]] = []
    pts:      list[tuple[float, float]]       = []
    cx = cy = 0.0

    while pos < len(toks):
        t = toks[pos]
        if not re.match(r'[MCLZmclz]', t):
            raise ValueError(f"Expected SVG path command, got {t!r} at token {pos}")
        pos += 1

        if t == 'M':
            if pts:
                contours.append(pts)
            cx, cy = read(), read()
            pts = [(cx, cy)]

        elif t == 'L':
            cx, cy = read(), read()
            pts.append((cx, cy))

        elif t == 'C':
            x1, y1 = read(), read()
            x2, y2 = read(), read()
            x,  y  = read(), read()
            pts.extend(_subdivide((cx, cy), (x1, y1), (x2, y2), (x, y)))
            cx, cy = x, y

        elif t == 'Z':
            if pts:
                contours.append(pts)
            pts = []

    if pts:
        contours.append(pts)

    return contours


# ── Logo cross-section and solid ──────────────────────────────────────────────

def _logo_contours_mm(cx: float, cy: float,
                      size_mm: float) -> list[list[tuple[float, float]]]:
    """Parse the logo SVG and return contours scaled to *size_mm* in tile space.

    The logo is centred at *(cx, cy)* in the tile XY plane.  SVG Y is flipped
    so that Y increases upward (tile convention).
    """
    tree = ET.parse(_SVG_PATH)
    ns   = 'http://www.w3.org/2000/svg'
    path_el = tree.getroot().find(f'.//{{{ns}}}path')
    raw = _parse_svg_d(path_el.attrib['d'])

    scale  = size_mm / _SVG_VIEWBOX
    x_off  = cx - size_mm / 2.0
    y_off  = cy + size_mm / 2.0   # top of logo in tile-Y

    return [
        [(x_off + p[0] * scale,
          y_off - p[1] * scale)   # flip SVG Y → tile Y
         for p in contour]
        for contour in raw
    ]


def make_logo_manifold(cx: float, cy: float,
                       size_mm: float,
                       z_base: float,
                       depth_mm: float = 0.4,
                       clearance_mm: float = 0.35):
    """Return the logo inset as a ``manifold3d.Manifold`` solid.

    Stays entirely in manifold space — no trimesh round-trip.  Use this when
    the calling code also operates in manifold (e.g. the DB base builder) so
    the subtraction happens without any floating-point drift from conversion.

    Parameters
    ----------
    cx, cy       : centre of the logo in tile XY (mm).
    size_mm      : logo bounding square side length (mm).
    z_base       : z of the outermost base face (negative for DB/OL bottoms).
    depth_mm     : inset depth in mm (logo floor is at z_base + depth_mm).
    clearance_mm : inward shrink applied to lotus contours before extrusion.
    """
    import manifold3d as m3d

    contours = _logo_contours_mm(cx, cy, size_mm)

    if clearance_mm > 0.0:
        # The square-outline groove is a thin ring: offsetting the entire
        # cross-section inward shrinks it from both sides and collapses it.
        # Identify the groove as the one contour whose bounding-box area is
        # far larger than any lotus contour (≥ 50 % of the maximum).  Keep it
        # unchanged and apply the offset only to the lotus contours.
        def _bbox_area(c: list[tuple[float, float]]) -> float:
            xs = [p[0] for p in c]; ys = [p[1] for p in c]
            return (max(xs) - min(xs)) * (max(ys) - min(ys))

        areas      = [_bbox_area(c) for c in contours]
        max_area   = max(areas)
        groove     = [c for c, a in zip(contours, areas) if a / max_area >= 0.5]
        lotus      = [c for c, a in zip(contours, areas) if a / max_area <  0.5]

        cs_groove  = m3d.CrossSection(groove, fillrule=m3d.FillRule.EvenOdd)
        cs_lotus   = m3d.CrossSection(lotus,  fillrule=m3d.FillRule.EvenOdd)
        cs_lotus   = cs_lotus.offset(-clearance_mm, m3d.JoinType.Miter)
        # Groove and lotus occupy non-overlapping regions, so union is correct.
        cs = m3d.CrossSection.compose([cs_groove, cs_lotus])
    else:
        cs = m3d.CrossSection(contours, fillrule=m3d.FillRule.EvenOdd)

    # Extrude depth_mm then translate so bottom face sits at z_base.
    # The solid spans [z_base .. z_base + depth_mm].
    solid    = m3d.Manifold.extrude(cs, height=depth_mm)
    solid    = solid.translate((0.0, 0.0, z_base))

    return solid


def make_logo_inset(cx: float, cy: float,
                    size_mm: float,
                    z_base: float,
                    depth_mm: float = 0.4,
                    clearance_mm: float = 0.35) -> trimesh.Trimesh:
    """Trimesh wrapper around make_logo_manifold — use for OL where trimesh input
    is needed.  For DB, call make_logo_manifold directly to stay in manifold space
    and avoid the round-trip conversion that can corrupt coplanar boolean cuts.
    """
    solid = make_logo_manifold(cx, cy, size_mm, z_base, depth_mm, clearance_mm)
    msh  = solid.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(msh.vert_properties, dtype=float)[:, :3],
        faces=np.array(msh.tri_verts, dtype=int),
        process=False,
    )
    return mesh
