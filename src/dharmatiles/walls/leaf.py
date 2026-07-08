"""
Leaves — door leaf / window leaf (shutter) / hatch leaf (trapdoor).

Design: docs/design/walls-doors.md (rev 2), stage O5.  A leaf is a
separate solid fitted to the opening's PROFILE: the same closed
polygon that shapes the surround also shapes the leaf, so an arched
doorway gets an arch-top door, an oculus gets a round grille, a well
can take a round lid.  Built in the opening's local frame — x across
the clear width, y through the wall thickness (y=0 is the OUTER face
side), z up from the profile bottom — and placed by the wall, so the
same generators serve standing doorways and ``laid_flat`` hatches (a
trapdoor is a door lying down, exactly as a floor is a wall lying
down).

Construction is the WALL strategy at leaf scale (Shawn: "reuse the
brick-wall strategy"): ``union(core, planks)`` where the core is a
thin profile prism recessed from both faces and the planks are
full-thickness solids with reveal gaps between them — so the board
grooves read identically on BOTH faces, floored by core, never
see-through.  Plank faces carry a carved wood grain (ridged noise
along the plank axis).  The assembly is clipped to the profile
dilated by ``_FUSE_MM``: the leaf overlaps the jamb reveals / sill /
surround all round, so the tile union fuses it to the masonry — an
integrated leaf is part of the print, never a floating shell.

Open leaves rotate about their hinge EDGE before placement; the
hinge line sits at the dilated boundary inside the jamb, so the
overlap band stays buried at any angle.
"""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import shapely.geometry as sgeom
import trimesh

from ..core.color import Material

_FUSE_MM     = 0.8    # dilate the profile: embed into the jamb
                      # reveals / sill / surround (union fusion)
_PLANK_W     = (2.8, 4.0)   # per-plank width range
_PLANK_GAP   = 0.30   # reveal gap between planks (both faces)
_PLANK_JIT   = 0.22   # per-plank per-FACE recession (proudness life)
_CORE_REVEAL = 0.55   # core recessed this far behind each face
_LEDGE_H     = 2.3    # horizontal batten height
_LEDGE_D     = 0.9    # batten proudness off the back face
_LEDGE_AT    = (0.20, 0.76)  # batten centres as height fractions
_STUD_R      = 0.42   # nail-stud dome radius
_RING_R      = 1.5    # handle ring radius
_RING_T      = 0.38   # handle ring tube radius
_BAR_R       = 0.65   # window/prison bar radius
_BAR_PITCH   = 3.1    # bar spacing
_BAR_FRAME   = 1.7    # bar frame band width (follows the profile)
_PORT_RAIL_PITCH = 6.0  # portcullis horizontal-rail spacing
_PORT_FOOT_MM    = 2.4  # pointed feet projecting below the bottom rail
_GRAIN_AMP   = 0.13   # wood-grain carve depth
_GRAIN_WL    = (0.8, 1.4)   # ridge pitch across the plank


@dataclass
class Leaf:
    """A leaf for an :class:`~dharmatiles.walls.openings.Opening`.

    ``kind``: ``'planks'`` (the default door/gate/lid), ``'shutters'``
    (a pair of half-width plank leaves hinged left AND right),
    ``'bars'`` (grille: a frame band following the profile + vertical
    round bars, stone/metal tone), ``'trapdoor'`` (flush planked
    hatch lid — the planks kernel with no proud battens).

    ``open_deg``/``hinge``: the leaf solid is rotated about its hinge
    edge and unioned standing open at any angle (ref-06's red door).
    ``hinge`` is ``'left'``/``'right'`` (vertical edge; the swing is
    INWARD, toward the wall's body side) or ``'foot'``/``'head'``
    (horizontal edge — a trapdoor lifts about its foot edge, a
    shutter awning tips out about its head).
    """
    #: 'planks' | 'shutters' | 'bars' | 'trapdoor' | 'portcullis'.
    #: A slot leaf (Opening.slot) slides rather than swings, so
    #: ``open_deg`` is ignored there; 'portcullis' is a slot leaf.
    kind:         str   = 'planks'
    open_deg:     float = 0.0
    hinge:        str   = 'left'
    thickness_mm: float = 2.6
    seed:         int   = 0

    @property
    def material(self) -> Material:
        return (Material.ROCK if self.kind in ('bars', 'portcullis')
                else Material.WOOD)


def _box(extents, at, n_sub: int = 3) -> trimesh.Trimesh:
    """An axis-aligned box, uniformly subdivided.  Smooth-shaded
    renders average vertex normals across box arrises; on a raw box
    the gradient spans the whole face and a planked leaf reads
    accordion-folded.  Subdivision pins the interior flat and narrows
    the gradient to a bevel-like band at the arrises.  Uniform splits
    keep shared edges consistent, so watertightness survives
    (subdivide_to_size does NOT — it T-vertexes adjacent faces)."""
    b = trimesh.creation.box(extents=extents)
    for _ in range(n_sub):
        b = b.subdivide()
    b.apply_translation(at)
    return b


def _prism(poly, y0: float, y1: float) -> trimesh.Trimesh:
    """Extrude a shapely polygon drawn in the leaf's (x, z) plane
    through the thickness axis, spanning y ∈ [y0, y1]."""
    p = trimesh.creation.extrude_polygon(poly, height=y1 - y0)
    M = np.array([[1.0, 0.0, 0.0, 0.0],      # x stays x
                  [0.0, 0.0, 1.0, y0],       # extrusion axis → y
                  [0.0, 1.0, 0.0, 0.0],      # polygon y → z
                  [0.0, 0.0, 0.0, 1.0]])
    p.apply_transform(M)   # det −1: trimesh flips winding itself
    return p


def _grain(body: trimesh.Trimesh, rng: np.random.Generator,
           along: int = 2) -> None:
    """Carve wood grain into both thickness faces of a plank, in
    place: ridged stripes running ALONG the board axis (``along``:
    2 = vertical planks, 0 = horizontal battens), gently wavy,
    recess-only so the reveal gaps between planks are untouched."""
    v = body.vertices.copy()
    across = 0 if along == 2 else 2
    wl = rng.uniform(*_GRAIN_WL)
    ph, ph2 = rng.uniform(0.0, 2.0 * np.pi, 2)
    wave = 0.22 * np.sin(2.0 * np.pi * v[:, along] / 9.0 + ph2)
    g = np.sin(2.0 * np.pi * (v[:, across] / wl + wave) + ph)
    carve = _GRAIN_AMP * (0.5 + 0.5 * g) ** 1.6
    lo, hi = v[:, 1].min(), v[:, 1].max()
    on_front = np.abs(v[:, 1] - lo) < 1e-6
    on_back  = np.abs(v[:, 1] - hi) < 1e-6
    v[on_front, 1] += carve[on_front]
    v[on_back, 1]  -= carve[on_back]
    body.vertices = v


def _planks_parts(x0: float, x1: float, z0: float, z1: float,
                  th: float, rng: np.random.Generator,
                  *, ledges: bool, ring_x: float | None,
                  ring_z: float | None) -> list[trimesh.Trimesh]:
    """The planked kernel over a bounding box, the WALL strategy at
    leaf scale: a recessed core sheet + full-thickness planks with
    reveal gaps, per-face recession jitter, and carved grain —
    grooves read the same on both faces.  Battens (back), stud domes
    (front) and a ring handle (front) complete the door furniture."""
    w, h = x1 - x0, z1 - z0
    parts = []

    n = max(2, int(round(w / float(np.mean(_PLANK_W)))))
    pw = rng.uniform(*_PLANK_W, n)
    pw *= w / pw.sum()
    edges = x0 + np.concatenate([[0.0], np.cumsum(pw)])
    for pa, pb in zip(edges[:-1], edges[1:]):
        g0 = _PLANK_GAP / 2.0 if pa > x0 + 1e-6 else 0.0
        g1 = _PLANK_GAP / 2.0 if pb < x1 - 1e-6 else 0.0
        px = (pb - g1) - (pa + g0)
        r0 = rng.uniform(0.0, _PLANK_JIT)         # front recession
        r1 = rng.uniform(0.0, _PLANK_JIT)         # back recession
        p = _box([px, th - r0 - r1, h],
                 [(pa + g0) + px / 2.0, r0 + (th - r0 - r1) / 2.0,
                  z0 + h / 2.0], n_sub=4)
        _grain(p, rng)
        parts.append(p)

    zs = [z0 + f * h for f in _LEDGE_AT]
    if ledges:
        for zc in zs:
            # embedded 0.2 into the back face, proud _LEDGE_D beyond
            b = _box([w, _LEDGE_D + 0.2, _LEDGE_H],
                     [x0 + w / 2.0, th + (_LEDGE_D - 0.2) / 2.0, zc],
                     n_sub=2)
            _grain(b, rng, along=0)
            parts.append(b)
    # stud domes on the FRONT along the batten lines (through-bolts)
    for zc in zs:
        for pa, pb in zip(edges[:-1], edges[1:]):
            s = trimesh.creation.icosphere(subdivisions=1,
                                           radius=_STUD_R)
            s.apply_translation([(pa + pb) / 2.0, 0.12, zc])
            parts.append(s)
    if ring_x is not None:
        ring = trimesh.creation.torus(major_radius=_RING_R,
                                      minor_radius=_RING_T,
                                      major_sections=24,
                                      minor_sections=8)
        # flat against the front face, hanging from a small boss
        ring.apply_transform(trimesh.transformations.rotation_matrix(
            np.pi / 2.0, [1, 0, 0]))
        ring.apply_translation([ring_x, 0.0, ring_z - _RING_R * 0.8])
        boss = trimesh.creation.cylinder(radius=0.6, height=1.0,
                                         sections=12)
        boss.apply_transform(trimesh.transformations.rotation_matrix(
            np.pi / 2.0, [1, 0, 0]))
        boss.apply_translation([ring_x, 0.1, ring_z])
        parts += [ring, boss]
    return parts


def _planks_leaf(outline, th: float, rng: np.random.Generator, *,
                 ledges: bool, ring_frac: float | None
                 ) -> trimesh.Trimesh:
    """Planks + core clipped to the (already dilated) outline.
    ``ring_frac`` is the handle's x position as a width fraction
    (None = no ring)."""
    x0, z0, x1, z1 = outline.bounds
    ring_x = None if ring_frac is None else x0 + (x1 - x0) * ring_frac
    parts = _planks_parts(x0, x1, z0, z1, th, rng, ledges=ledges,
                          ring_x=ring_x,
                          ring_z=z0 + (z1 - z0) * 0.52)
    parts.append(_prism(outline, _CORE_REVEAL, th - _CORE_REVEAL))
    s = trimesh.boolean.union(parts, engine='manifold')
    # Clip to the profile in (x, z); the clip prism spans every proud
    # bit in y (battens, ring) — only the outline shape cuts.
    clip = _prism(outline, -4.0, th + 4.0)
    return trimesh.boolean.intersection([s, clip], engine='manifold')


def _bars_leaf(outline, th: float) -> trimesh.Trimesh:
    """Grille: a frame band following the profile boundary (an
    annulus on a round opening) + vertical round bars."""
    x0, z0, x1, z1 = outline.bounds
    inner = outline.buffer(-_BAR_FRAME)
    parts = []
    if not inner.is_empty:
        frame = outline.difference(inner)
        for g in getattr(frame, 'geoms', [frame]):
            parts.append(_prism(sgeom.polygon.orient(g, 1.0), 0.0, th))
    n = max(2, int(round((x1 - x0) / _BAR_PITCH)))
    for i in range(n):
        x = x0 + (i + 0.5) * (x1 - x0) / n
        b = trimesh.creation.cylinder(radius=_BAR_R, height=z1 - z0,
                                      sections=12)
        b.apply_translation([x, th / 2.0, (z0 + z1) / 2.0])
        parts.append(b)
    s = trimesh.boolean.union(parts, engine='manifold')
    clip = _prism(outline, -2.0, th + 2.0)
    return trimesh.boolean.intersection([s, clip], engine='manifold')


def _portcullis_leaf(outline, th: float) -> trimesh.Trimesh:
    """A dropped iron gate: a lattice of vertical round bars +
    horizontal rails, the verticals tapering to POINTED FEET below the
    bottom rail (the classic portcullis).  The grid is clipped to the
    profile; the feet project past it (they spike into the sill)."""
    x0, z0, x1, z1 = outline.bounds
    grid = []
    nv = max(2, int(round((x1 - x0) / _BAR_PITCH)))
    xs = [x0 + (i + 0.5) * (x1 - x0) / nv for i in range(nv)]
    for x in xs:
        b = trimesh.creation.cylinder(radius=_BAR_R, height=z1 - z0,
                                      sections=12)
        b.apply_translation([x, th / 2.0, (z0 + z1) / 2.0])
        grid.append(b)
    nh = max(2, int(round((z1 - z0) / _PORT_RAIL_PITCH)))
    for j in range(nh):
        z = z0 + (j + 0.5) * (z1 - z0) / nh
        b = trimesh.creation.cylinder(radius=_BAR_R, height=x1 - x0,
                                      sections=12)
        b.apply_transform(trimesh.transformations.rotation_matrix(
            np.pi / 2.0, [0, 1, 0]))
        b.apply_translation([(x0 + x1) / 2.0, th / 2.0, z])
        grid.append(b)
    s = trimesh.boolean.union(grid, engine='manifold')
    clip = _prism(outline, -2.0, th + 2.0)
    s = trimesh.boolean.intersection([s, clip], engine='manifold')
    # Pointed feet: a downward cone under each vertical bar, spiking
    # below the profile (not clipped) — they seat into the sill.
    feet = [s]
    for x in xs:
        cone = trimesh.creation.cone(radius=_BAR_R, height=_PORT_FOOT_MM,
                                     sections=12)
        cone.apply_transform(trimesh.transformations.rotation_matrix(
            np.pi, [1, 0, 0]))               # apex points down (−z)
        cone.apply_translation([x, th / 2.0, z0])
        feet.append(cone)
    return trimesh.boolean.union(feet, engine='manifold')


def _hinge_open(solid: trimesh.Trimesh, open_deg: float, hinge: str,
                bounds, th: float) -> trimesh.Trimesh:
    """Rotate the leaf about its hinge edge — the dilated outline's
    bbox edge, buried inside the jamb/sill.  'left'/'right' swing the
    free edge toward +y (inward, through the wall); 'foot'/'head' tip
    it toward −y (a trapdoor's −y is up out of the floor; a shutter
    awning tips outward)."""
    a = np.radians(open_deg)
    if a <= 1e-6:
        return solid
    x0, z0, x1, z1 = bounds
    if hinge == 'left':
        M = trimesh.transformations.rotation_matrix(
            a, [0, 0, 1], [x0, th / 2.0, 0.0])
    elif hinge == 'right':
        M = trimesh.transformations.rotation_matrix(
            -a, [0, 0, 1], [x1, th / 2.0, 0.0])
    elif hinge == 'foot':
        M = trimesh.transformations.rotation_matrix(
            a, [1, 0, 0], [0.0, th / 2.0, z0])
    elif hinge == 'head':
        M = trimesh.transformations.rotation_matrix(
            -a, [1, 0, 0], [0.0, th / 2.0, z1])
    else:
        raise ValueError(f'unknown hinge {hinge!r}')
    return solid.apply_transform(M)


def build_leaf(leaf: Leaf, profile: np.ndarray,
               rng: np.random.Generator, *,
               slot_clearance: float | None = None) -> trimesh.Trimesh:
    """The leaf solid in the opening's local frame.  ``profile`` is
    the opening's closed polygon translated so its bbox corner is at
    the origin.  y ∈ [0, thickness] is positioned by the CALLER (the
    wall knows its own thickness and reveal planes).

    Fit (O5 vs O6) is set by ``slot_clearance``:

    - ``None`` — an INTEGRATED leaf: the outline is dilated by
      ``_FUSE_MM`` so it embeds into the jamb reveals / sill /
      surround and the export union fuses it to the masonry.  It may
      swing open about its hinge.
    - a value — a SLOT leaf: the outline is ERODED by half the
      clearance so it fits the mid-thickness channel WITHOUT touching
      the surround, staying a separate removable object.  It slides
      rather than swings, so ``open_deg`` is ignored."""
    th = leaf.thickness_mm
    if slot_clearance is None:
        outline = sgeom.Polygon(profile).buffer(_FUSE_MM, quad_segs=8)
    else:
        outline = sgeom.Polygon(profile).buffer(-slot_clearance / 2.0,
                                                quad_segs=8)
    swing = slot_clearance is None
    bounds = outline.bounds
    if leaf.kind == 'planks':
        s = _planks_leaf(outline, th, rng, ledges=True,
                         ring_frac=0.82 if leaf.hinge == 'left'
                         else 0.18)
        if swing:
            s = _hinge_open(s, leaf.open_deg, leaf.hinge, bounds, th)
    elif leaf.kind == 'trapdoor':
        # lid ring sits centred, not at a closing edge
        s = _planks_leaf(outline, th, rng, ledges=False,
                         ring_frac=0.5)
        if swing:
            s = _hinge_open(s, leaf.open_deg, leaf.hinge, bounds, th)
    elif leaf.kind == 'bars':
        s = _bars_leaf(outline, th)     # bars don't swing
    elif leaf.kind == 'portcullis':
        s = _portcullis_leaf(outline, th)   # slides in the slot
    elif leaf.kind == 'shutters':
        x0, z0, x1, z1 = bounds
        xm = (x0 + x1) / 2.0
        halves = []
        for side, hx0, hx1 in (('left', x0, xm - 0.15),
                               ('right', xm + 0.15, x1)):
            half_out = outline.intersection(
                sgeom.box(hx0, z0 - 1.0, hx1, z1 + 1.0))
            if half_out.is_empty:
                continue
            if half_out.geom_type == 'MultiPolygon':
                half_out = max(half_out.geoms, key=lambda g: g.area)
            half = _planks_leaf(half_out, th, rng, ledges=True,
                                ring_frac=None)
            if swing:
                half = _hinge_open(half, leaf.open_deg, side,
                                   half_out.bounds, th)
            halves.append(half)
        s = trimesh.util.concatenate(halves)
    else:
        raise ValueError(f'unknown leaf kind {leaf.kind!r}')
    return s
