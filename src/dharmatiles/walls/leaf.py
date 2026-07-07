"""
Leaves — door leaf / window leaf (shutter) / hatch leaf (trapdoor).

Design: docs/design/walls-doors.md (rev 2), stage O5.  A leaf is a
separate solid fitted to the opening's clear rectangle (leaves stay
RECTANGULAR even under arches — the arch above a door leaf stays
open).  Built in the opening's local frame — x across the clear
width, y through the wall thickness (y=0 is the OUTER face side), z
up the clear height — and placed by the wall, so the same generators
serve standing doorways and ``laid_flat`` hatches (a trapdoor is a
door lying down, exactly as a floor is a wall lying down).

Every leaf overlaps the jamb reveals by ``_FUSE_MM`` per side and
roots ``_FOOT_MM`` below its sill line, so the tile union fuses it to
the masonry — an integrated leaf is part of the print, never a
floating shell.  Open leaves rotate about their hinge EDGE before
placement; the hinge line sits mid-fuse inside the jamb, so the
overlap band stays buried at any angle.
"""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import trimesh

from ..core.color import Material

_FUSE_MM     = 0.8    # embed into the jamb reveals (union fusion)
_FOOT_MM     = 1.0    # leaves also root below their sill line (an
                      # OPEN leaf's free end must reach the soil even
                      # where the surface dips below the seat plane)
_PLANK_W     = (2.8, 4.0)   # per-plank width range
_PLANK_GAP   = 0.30   # dark line between planks
_PLANK_JIT   = 0.25   # per-plank front-face recession (relief life)
_BACK_MM     = 1.0    # backing slab (keeps the leaf one solid)
_LEDGE_H     = 2.3    # horizontal batten height
_LEDGE_D     = 0.9    # batten proudness off the back face
_LEDGE_AT    = (0.20, 0.76)  # batten centres as height fractions
_STUD_R      = 0.42   # nail-stud dome radius
_RING_R      = 1.5    # handle ring radius
_RING_T      = 0.38   # handle ring tube radius
_BAR_R       = 0.65   # window/prison bar radius
_BAR_PITCH   = 3.1    # bar spacing
_BAR_FRAME   = 1.7    # bar frame member width


def _box(extents, at, n_sub: int = 3) -> trimesh.Trimesh:
    """An axis-aligned box, uniformly subdivided.  Smooth-shaded
    renders average vertex normals across box arrises; on a raw box
    the gradient spans the whole face and a planked leaf reads
    accordion-folded.  Subdivision pins the interior flat (interior
    vertices see only coplanar faces) and narrows the gradient to a
    bevel-like band at the arrises.  Uniform splits keep shared edges
    consistent, so watertightness survives (subdivide_to_size does
    NOT — it T-vertexes adjacent faces)."""
    b = trimesh.creation.box(extents=extents)
    for _ in range(n_sub):
        b = b.subdivide()
    b.apply_translation(at)
    return b


@dataclass
class Leaf:
    """A leaf for an :class:`~dharmatiles.walls.openings.Opening`.

    ``kind``: ``'planks'`` (the default door), ``'shutters'`` (a pair
    of half-width plank leaves hinged left AND right), ``'bars'``
    (vertical grille, stone/metal tone), ``'trapdoor'`` (flush
    planked hatch lid — the planks kernel with no proud battens).

    ``open_deg``/``hinge``: the leaf solid is rotated about its hinge
    edge and unioned standing open at any angle (ref-06's red door).
    ``hinge`` is ``'left'``/``'right'`` (vertical edge; the swing is
    INWARD, toward the wall's body side) or ``'foot'``/``'head'``
    (horizontal edge — a trapdoor lifts about its foot edge, a
    shutter awning tips out about its head).
    """
    kind:         str   = 'planks'
    open_deg:     float = 0.0
    hinge:        str   = 'left'
    thickness_mm: float = 2.6
    seed:         int   = 0

    @property
    def material(self) -> Material:
        return Material.ROCK if self.kind == 'bars' else Material.WOOD


def _planks_solid(w: float, h: float, th: float,
                  rng: np.random.Generator, *,
                  ledges: bool = True, ring_x: float | None = None,
                  ring_z: float | None = None) -> trimesh.Trimesh:
    """Planked leaf in [0,w]×[0,th]×[0,h]: backing slab + proud
    vertical planks with dark gaps between + battens proud of the
    back + stud domes along the batten lines on the front + a ring
    handle on the front (y=0) face."""
    parts = [_box([w, _BACK_MM, h],
                  [w / 2.0, th - _BACK_MM / 2.0, h / 2.0])]

    n = max(2, int(round(w / float(np.mean(_PLANK_W)))))
    pw = rng.uniform(*_PLANK_W, n)
    pw *= w / pw.sum()
    edges = np.concatenate([[0.0], np.cumsum(pw)])
    pd = th - _BACK_MM
    for x0, x1 in zip(edges[:-1], edges[1:]):
        g0 = _PLANK_GAP / 2.0 if x0 > 1e-6 else 0.0
        g1 = _PLANK_GAP / 2.0 if x1 < w - 1e-6 else 0.0
        px = (x1 - g1) - (x0 + g0)
        d = pd - rng.uniform(0.0, _PLANK_JIT)
        # back flush against the backing slab; the FRONT face varies
        parts.append(_box([px, d, h],
                          [(x0 + g0) + px / 2.0, pd - d / 2.0, h / 2.0]))

    zs = [f * h for f in _LEDGE_AT]
    if ledges:
        for zc in zs:
            # embedded 0.2 into the back face, proud _LEDGE_D beyond
            parts.append(_box([w, _LEDGE_D + 0.2, _LEDGE_H],
                              [w / 2.0, th + (_LEDGE_D - 0.2) / 2.0,
                               zc], n_sub=2))
    # stud domes on the FRONT along the batten lines (through-bolts)
    for zc in zs:
        for x0, x1 in zip(edges[:-1], edges[1:]):
            s = trimesh.creation.icosphere(subdivisions=1, radius=_STUD_R)
            s.apply_translation([(x0 + x1) / 2.0, 0.12, zc])
            parts.append(s)
    if ring_x is not None:
        ring = trimesh.creation.torus(major_radius=_RING_R,
                                      minor_radius=_RING_T,
                                      major_sections=24, minor_sections=8)
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
    return trimesh.boolean.union(parts, engine='manifold')


def _bars_solid(w: float, h: float, th: float) -> trimesh.Trimesh:
    """Vertical round bars + top/bottom rails (prison / grille)."""
    parts = []
    yc = th / 2.0
    for z0, z1 in ((0.0, _BAR_FRAME), (h - _BAR_FRAME, h)):
        parts.append(_box([w, th, z1 - z0],
                          [w / 2.0, yc, (z0 + z1) / 2.0], n_sub=2))
    n = max(2, int(round(w / _BAR_PITCH)))
    for i in range(n):
        x = (i + 0.5) * w / n
        b = trimesh.creation.cylinder(radius=_BAR_R, height=h,
                                      sections=12)
        b.apply_translation([x, yc, h / 2.0])
        parts.append(b)
    return trimesh.boolean.union(parts, engine='manifold')


def _hinge_open(solid: trimesh.Trimesh, open_deg: float, hinge: str,
                w: float, h: float, th: float) -> trimesh.Trimesh:
    """Rotate the leaf about its hinge edge (in the local build
    frame).  'left'/'right' swing the free edge toward +y (inward,
    through the wall); 'foot'/'head' tip it toward −y (a trapdoor's
    −y is up out of the floor; a shutter awning tips outward)."""
    a = np.radians(open_deg)
    if a <= 1e-6:
        return solid
    if hinge == 'left':
        M = trimesh.transformations.rotation_matrix(
            a, [0, 0, 1], [0.0, th / 2.0, 0.0])
    elif hinge == 'right':
        M = trimesh.transformations.rotation_matrix(
            -a, [0, 0, 1], [w, th / 2.0, 0.0])
    elif hinge == 'foot':
        M = trimesh.transformations.rotation_matrix(
            a, [1, 0, 0], [0.0, th / 2.0, 0.0])
    elif hinge == 'head':
        M = trimesh.transformations.rotation_matrix(
            -a, [1, 0, 0], [0.0, th / 2.0, h])
    else:
        raise ValueError(f'unknown hinge {hinge!r}')
    return solid.apply_transform(M)


def build_leaf(leaf: Leaf, w_clear: float, h_clear: float,
               rng: np.random.Generator) -> trimesh.Trimesh:
    """The leaf solid in the opening's local frame: x ∈ [−fuse,
    w_clear+fuse] (embedded into both jamb reveals), z ∈ [−foot,
    h_clear]; y ∈ [0, thickness] is positioned by the CALLER (the
    wall knows its own thickness and reveal planes)."""
    th = leaf.thickness_mm
    fuse, foot = _FUSE_MM, _FOOT_MM
    w = w_clear + 2.0 * fuse
    h = h_clear + foot
    if leaf.kind == 'planks':
        s = _planks_solid(w, h, th, rng,
                          ring_x=(w * (0.82 if leaf.hinge == 'left'
                                       else 0.18)),
                          ring_z=h * 0.52)
        s = _hinge_open(s, leaf.open_deg, leaf.hinge, w, h, th)
    elif leaf.kind == 'trapdoor':
        s = _planks_solid(w, h, th, rng, ledges=False,
                          ring_x=w / 2.0, ring_z=h * 0.55)
        s = _hinge_open(s, leaf.open_deg, leaf.hinge, w, h, th)
    elif leaf.kind == 'bars':
        s = _bars_solid(w, h, th)      # bars don't swing
    elif leaf.kind == 'shutters':
        halves = []
        hw = w / 2.0 - 0.15
        for side, x0 in (('left', 0.0), ('right', w / 2.0 + 0.15)):
            half = _planks_solid(hw, h, th, rng, ledges=True,
                                 ring_x=None)
            half = _hinge_open(half, leaf.open_deg, side, hw, h, th)
            half.apply_translation([x0, 0.0, 0.0])
            halves.append(half)
        s = trimesh.util.concatenate(halves)
    else:
        raise ValueError(f'unknown leaf kind {leaf.kind!r}')
    s.apply_translation([-fuse, 0.0, -foot])
    return s
