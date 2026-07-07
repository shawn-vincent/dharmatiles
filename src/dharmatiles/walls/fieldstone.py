"""
Fieldstone / drystone wall — walls campaign family 3.

E12 rework (Shawn round-8: "Distinct natural stones, fitting together
with tight cracks between … stones fitting into all the space").
Reference set: ``docs/reference/walls/fieldstone/`` — every image
agrees on the same geometric truths (see the README there):

- adjacent stones SHARE edge geometry (edges run parallel, a bulge in
  one stone matches a notch in its neighbour) — the builder chose
  stones to fit;
- stone covers >= 90 % of the face; cracks are thin (~0.3–0.7 mm at DB
  scale) and roughly uniform, never mortar seas or per-stone moats;
- beds are long, wavy, sub-horizontal shadow lines; head joints short,
  staggered, slightly slanted;
- faces are near-coplanar (small per-stone proudness); the drama is
  the dark crack network + per-stone light tone, not deep relief.

E6–E11 generated an independent hull per grid cell and could never
produce that complementarity — every round read as separate objects
arranged on a grid.  E12 inverts the construction, **crack network
first**:

1. The layout chassis (courses × bays × quoins, from CutStoneWall)
   provides the crack TOPOLOGY.
2. Each bed line gets a shared wobble function; each head joint a
   shared slant.  Both sides of any crack evaluate the SAME curve, so
   neighbouring stones complement each other exactly, by construction.
3. A stone's face outline IS the polygon its cracks bound, inset by
   half the crack width (shapely), corners rounded.
4. The solid is that outline extruded straight through the wall
   thickness with every edge rounded over (E15: the E12 belly/pillow
   and relief were dropped — "just the cracks and the roundovers").

Tessellation (fill-all-space), tight uniform cracks, and neighbour
complementarity therefore hold by construction; the rubble hearting
(E10) keeps every crack floored by deeper stone, never the core.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..stone import stone_relief
from .masonry import CutStoneWall, _Cell, _Seg, _frame

# ── Iteration knobs (module constants while prototyping) ─────────────────────
# Crack network
_WOBBLE_AMP_MM    = (0.16, 0.38)  # per-sine bed wobble amplitude (2 sines).
                                  # Total ≤ 0.76: two INDEPENDENT beds can
                                  # close on each other by twice that —
                                  # the course minimum (2.6) keeps the
                                  # thinnest waists ≥ ~1 mm (chink-class,
                                  # never crossing)
_WOBBLE_WL_MM     = (11.0, 32.0)  # bed wobble wavelengths (E21: shorter
                                  # = more wander per stone width)
_WOBBLE_TAPER_MM  = 12.0          # wobble fades to 0 within this of a
                                  # segment end: corners/butt ends pack
                                  # straight and tight
_DRIFT_MM         = 2.2           # head joints slant up to ±this across
                                  # their course (staggered, never aligned)
_BOW_MM           = 1.1           # head joints BOW up to ±this at their
                                  # midpoint (E21 wandery cracks): the
                                  # joint is a shared parabola, zero at
                                  # the bed lines, both neighbours
                                  # evaluating the same curve
_BED_OVERLAP_MM   = (0.35, 0.80)  # E25: stones extend past their BED
                                  # lines too (capped at 0.22× height) —
                                  # vertical pressed contacts, same
                                  # mechanism as the head joints.  This
                                  # is also what welds the perched top-
                                  # course rocks to the stones below
                                  # now that the core stops beneath
                                  # them.  is_top exempt; bottom course
                                  # keeps its flat buried seat
_END_MARGIN_MM    = 0.7           # 'face' ends (free ends, corner
                                  # arris) recede this far inside the
                                  # tile plane: the surface texture
                                  # displaces up to ~0.6 mm outward and
                                  # anything past the boundary is
                                  # plane-cut — the "sheared-off end
                                  # rocks" of E24
_HEAD_OVERLAP_MM  = (0.45, 0.95)  # E24: stones extend past their head-
                                  # joint lines on BOTH sides, drawn per
                                  # side per stone — neighbours
                                  # interpenetrate and the union fuses
                                  # the contact, so the visible mortar
                                  # at vertical joints shrinks to a
                                  # pressed crease.  Deliberately breaks
                                  # the no-overlap rule for head joints
                                  # only (beds stay shared curves); the
                                  # TOP COURSE is exempt (Shawn) and the
                                  # overlap is capped at 0.25× the
                                  # stone's width so thin stones aren't
                                  # swallowed
_RING_NOISE_MM    = (0.04, 0.16)  # per-stone INWARD-only outline noise
                                  # (floor, max; 2 sinusoids over the
                                  # perimeter): irregular stone shapes +
                                  # cracks that open and close along
                                  # their run.  Inward-only, so the
                                  # no-overlap guarantee is untouched.
                                  # The FLOOR keeps contacts hairline-
                                  # separated instead of exactly tangent
                                  # — an exact-tangency pinch survived
                                  # the union as one non-manifold edge
                                  # after float32 STL rounding (E21)
# Stone body (E17: full pebble morph — "an isosphere morphed to fit
# the outline/depth of the cracked region".  Sphere topology: longitude
# follows the crack-bounded outline (the EQUATOR ring is the outline at
# mid-depth, so silhouette, cracks, and dimensions stay put); latitude
# sweeps a smooth ellipse-like meridian from face pole to face pole.
# Every surface is curved; there are no flat faces and no arris lines.
# `roundover_mm` still rounds the 2D outline corners — the equator
# silhouette — per stone)
_ROUND_SIZE_CAP   = 0.26          # per-stone 2D roundover ≤ this × the
                                  # outline's smaller bbox dimension
                                  # (thin stones must keep their face;
                                  # 0.38 made dark slots of them, E16)
_MORPH_A          = (2.0, 3.2)    # superellipse meridian s=(1−|u|^a)^b,
_MORPH_B          = (0.33, 0.48)  # u=sin(lat): a=2,b=0.5 is a true
                                  # ellipse — which read as MELTED dough
                                  # (all curvature at the silhouette,
                                  # equator-line contact, cracks became
                                  # soft wide valleys).  Fuller profiles
                                  # (higher a, lower b) keep a gently
                                  # domed face and drop fast near the
                                  # silhouette, so cracks stay narrow
                                  # and dark.  Drawn per pole: the stone
                                  # bulges asymmetrically
_POLE_DRIFT_FRAC  = 0.12          # each pole wanders up to this × the
                                  # outline size from the centroid: the
                                  # summit of the dome is off-centre,
                                  # like a real cobble
_BED_FLAT_EXP     = (0.40, 0.65)  # z-scale = s^p (E19): the bed
                                  # surfaces stay near-FLAT over most of
                                  # the depth and only turn down close
                                  # to the face poles — we're piling
                                  # flat stones, not spheres.  Smaller
                                  # exponent = flatter beds (E23 0.30 →
                                  # 0.52, E25 0.40 → 0.65: successively
                                  # rounder tops/bottoms; the bed
                                  # overlap keeps the contacts pressed)
_PROUD_MM         = (0.10, 0.70)  # per-stone face recession (both faces;
                                  # E25: more protrusion variance)
_PROUD_DEEP_PROB  = 0.15          # a rare stone sits notably deeper —
_PROUD_DEEP_MM    = (0.70, 1.10)  # the odd deep stone the references show
_RING_STEP_MM     = 1.2           # outline densify spacing
_N_LAT            = 17            # latitude rings (poles excluded)
# Coping (E29): a course of thin stones ON EDGE capping the wall —
# the classic drystone finish (refs 02/05).  Same complementarity
# trick as the beds: each coping joint's LEAN is a shared line, so
# neighbouring coping stones lean together.
_COPING_H_MM       = 6.5          # coping band height (courses fill H − this)
_COPING_W_MM       = (2.2, 3.6)   # per-stone width (thin slabs on edge)
_COPING_LEAN       = 0.30         # max shared joint lean (dt per dz)
_COPING_TOP_DROP_MM = 1.1         # per-stone top drop below H (ragged line)
_COPING_OV_MM      = (0.15, 0.35) # sideways pressed contact
_COPING_SEAT_OV_MM = (0.30, 0.60) # downward press into the course below
_CORNER_SLAB_MM    = (0.5, 1.5)   # corner coping slab extends T + this
# Cell topology
_THROUGH_FRAC     = 0.20          # fraction of eligible cells merged with
                                  # the cell above into a throughstone
_THROUGH_MAX_MM   = 9.5           # merged throughstone height cap (E19:
                                  # cross-row stones must not get too
                                  # tall — two full courses could reach
                                  # ~12–15 mm)
_SPLIT_H_PROB     = 0.28          # tall cell → two stacked thinner stones
_SPLIT_H_MIN_MM   = 5.5           # eligible cell height for an h-split
# Cell-key tags (ints, not strings: _place_block hashes cell.key for the
# per-stone rng and str hashes vary per process).  Base keys are
# (course, seg, bay), so tagged keys are longer, never equal.
_K_THROUGH = 9
_K_SPLIT_B, _K_SPLIT_T = 1, 2
_K_COPING = 5


class FieldstoneWall(CutStoneWall):
    """Direct TileLayer: a drystone fieldstone wall on a plan spine.

    Same spine convention and contracts as :class:`CutStoneWall`; the
    stones are crack-network tessellated (module docstring).

    The look-defining knobs span the family space (all per-stone
    uniform ranges): ``roundover_mm`` + ``bed_flat_exp`` set the stone
    character from thin squared slab toward round cobble;
    ``head_overlap_mm`` + ``bed_overlap_mm`` set how pressed the
    contacts read (0 = every crack a shared curve, larger = fused
    joints, less visible "mortar"); ``proud_mm`` sets face-recession
    variance (near-coplanar faces vs rugged); ``wobble_amp_mm`` sets
    bed-crack wander.  Defaults are the approved E25 look.
    """

    #: drystone: the rubble hearting is structural, not a ruin state.
    hearting = True

    surround_vw:   float = 2.6     # thin slab voussoirs (refs 03/05)
    surround_ring: float = 4.6
    surround_jd:   float = 4.6     # larger squared jamb stones
    surround_jh:   tuple = (4.0, 6.5)
    surround_chip: float = 0.35    # roughly squared, not dressed
    surround_ro:   float = 0.55
    surround_frac: float = 1.10    # drystone: pressed contact, the
                                   # union fuses slab against slab
    surround_bond_press: float = 0.6   # wall stones press INTO the
                                   # jambs (covers the jamb chip pull
                                   # 0.35 + ring noise 0.16): stones
                                   # must TOUCH, never a gap

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (2.2, 5.2),
                 bay_mm:    tuple[float, float] = (5.0, 16.0),
                 joint_mm:  float = 0.0,   # physical crack gap (Shawn E18:
                                           # zero — stones TOUCH at their
                                           # equators; the crack is drawn
                                           # entirely by the pole-ward
                                           # curvature of the two stones
                                           # meeting there, a V-groove
                                           # with no gap at its root)
                 reveal_mm: float = 2.8,
                 roundover_mm: tuple[float, float] = (1.3, 2.6),
                 relief_mm:    float | None = None,
                 min_bond_mm:  float = 1.8,
                 wobble_amp_mm:   tuple[float, float] = _WOBBLE_AMP_MM,
                 head_overlap_mm: tuple[float, float] = _HEAD_OVERLAP_MM,
                 bed_overlap_mm:  tuple[float, float] = _BED_OVERLAP_MM,
                 proud_mm:        tuple[float, float] = _PROUD_MM,
                 bed_flat_exp:    tuple[float, float] = _BED_FLAT_EXP,
                 coping:          str | None = None,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         roundover_mm=roundover_mm, relief_mm=relief_mm,
                         min_bond_mm=min_bond_mm,
                         **kwargs)
        # The base ctor substitutes its texture preset when relief_mm
        # is None, but for fieldstone None means "auto amplitude from
        # the stone's footprint" (_stone_texture) — restore it.
        self.relief_mm = relief_mm
        self.wobble_amp_mm   = wobble_amp_mm
        self.head_overlap_mm = head_overlap_mm
        self.bed_overlap_mm  = bed_overlap_mm
        self.proud_mm        = proud_mm
        self.bed_flat_exp    = bed_flat_exp
        if coping not in (None, 'vertical'):
            raise ValueError(f"unknown coping style {coping!r}; "
                             f"options: None (perched rocks), 'vertical'")
        self.coping          = coping
        self._beds:   dict[tuple, tuple] = {}   # (seg, zkey) → sine params
        self._drifts: dict[tuple, tuple] = {}   # (seg,course,tkey) → (d,bow)
        self._leans:  dict[tuple, float] = {}   # (seg, tkey) → coping lean

    # ── shared crack curves ──────────────────────────────────────────────────
    # Both stones flanking a crack evaluate the SAME curve (cached by a
    # quantised key), so complementarity is exact by construction.

    def _feature_rng(self, *key: int) -> np.random.Generator:
        return np.random.default_rng(
            hash((self.seed,) + key) & 0x7FFFFFFF)

    def _bed(self, seg_i: int, z: float, L: float):
        """Wobble function f(t) for the bed line at wall-local z."""
        key = (seg_i, int(round(z * 100.0)))
        if key not in self._beds:
            rng = self._feature_rng(7, *key)
            amp = rng.uniform(*self.wobble_amp_mm, 2)
            wl  = rng.uniform(*_WOBBLE_WL_MM, 2)
            ph  = rng.uniform(0.0, 2.0 * np.pi, 2)
            self._beds[key] = (amp, wl, ph)
        amp, wl, ph = self._beds[key]

        def f(t):
            t = np.asarray(t, dtype=float)
            taper = np.clip(np.minimum(t, L - t) / _WOBBLE_TAPER_MM,
                            0.0, 1.0)
            w = sum(a * np.sin(2.0 * np.pi * t / l + p)
                    for a, l, p in zip(amp, wl, ph))
            return w * taper
        return f

    def _drift(self, seg_i: int, course: int, t: float) -> tuple:
        """(slant, bow) of the head joint at cut position t in this
        course — a shared parabola: zero at the bed lines, bowing at
        the midpoint, slanting across the course."""
        key = (seg_i, course, int(round(t * 100.0)))
        if key not in self._drifts:
            rng = self._feature_rng(11, *key)
            self._drifts[key] = (float(rng.uniform(-_DRIFT_MM, _DRIFT_MM)),
                                 float(rng.uniform(-_BOW_MM, _BOW_MM)))
        return self._drifts[key]

    def _lean(self, seg_i: int, t: float) -> float:
        """Shared lean (dt per dz) of the coping joint at cut position
        t — both flanking coping stones evaluate the same slanted line,
        so the whole course leans together."""
        key = (seg_i, int(round(t * 100.0)))
        if key not in self._leans:
            rng = self._feature_rng(13, *key)
            self._leans[key] = float(rng.uniform(-_COPING_LEAN,
                                                 _COPING_LEAN))
        return self._leans[key]

    # ── cell topology ────────────────────────────────────────────────────────
    def _cells(self, segs: list[_Seg], T: float, H: float,
               rng: np.random.Generator) -> list[_Cell]:
        self._beds.clear()
        self._drifts.clear()
        self._leans.clear()
        # With coping the regular courses stop a band short of H; the
        # coping course of on-edge stones fills the rest.
        Hc = H - _COPING_H_MM if self.coping else H
        cells = self._merge_throughstones(super()._cells(segs, T, Hc, rng),
                                          rng)
        cells = self._split_cells(cells, rng)
        if self.coping:
            # The ex-top course is interior now: it takes the normal
            # head/bed overlaps and its top evaluates the SHARED bed
            # wobble at Hc — the same curve the coping bottoms use.
            for c in cells:
                c.is_top = False
            cells += self._coping_cells(segs, T, Hc, H, rng)
        # Wall-local z where the top course begins: the core and the
        # rubble hearting stop beneath it (E25 — the top course reads
        # as separate rocks perched on the wall, no mortar between).
        self._cap_z0 = min((c.z0 for c in cells if c.is_top), default=H)
        return cells

    def _coping_cells(self, segs: list[_Seg], T: float, z0: float,
                      H: float, rng: np.random.Generator) -> list[_Cell]:
        """One course of thin stones ON EDGE from z0 to H.  Owning
        segments start/end with a wider corner slab spanning the
        corner cell (a sliver coping quoin reads broken, E11)."""
        n_joints = len(segs) - 1
        out: list[_Cell] = []
        for k, seg in enumerate(segs):
            owns_start = k > 0 and (k - 1) % 2 != 0
            owns_end   = k < n_joints and k % 2 == 0
            t0 = 0.0 if (k == 0 or owns_start) else T
            t1 = seg.L if (k == n_joints or owns_end) else seg.L - T
            # Corner slabs claim their span first.
            slab0 = slab1 = None
            if k > 0 and owns_start:
                slab0 = (0.0, T + float(rng.uniform(*_CORNER_SLAB_MM)))
                t0 = slab0[1]
            if k < n_joints and owns_end:
                slab1 = (seg.L - T - float(rng.uniform(*_CORNER_SLAB_MM)),
                         seg.L)
                t1 = slab1[0]
            span = t1 - t0
            n = max(1, int(round(span / float(np.mean(_COPING_W_MM)))))
            w = rng.uniform(*_COPING_W_MM, n)
            w *= span / w.sum()
            edges = t0 + np.concatenate([[0.0], np.cumsum(w)])
            pieces = ([slab0] if slab0 else []) \
                + list(zip(edges[:-1], edges[1:])) \
                + ([slab1] if slab1 else [])
            for b, (ta, tb) in enumerate(pieces):
                first = ta <= 1e-6 and (k == 0 or owns_start)
                last  = tb >= seg.L - 1e-6 and (k == n_joints or owns_end)
                cell = _Cell(
                    seg=k, t0=float(ta), t1=float(tb),
                    end0='face' if first else 'joint',
                    end1='face' if last else 'joint',
                    z0=z0, z1=H, is_top=True, is_bottom=False,
                    key=(9000 + k, k, b, _K_COPING))
                cell.coping = True
                out.append(cell)
        return out

    def _merge_throughstones(self, cells: list[_Cell],
                             rng: np.random.Generator) -> list[_Cell]:
        """Merge some vertically adjacent cell pairs into double-height
        throughstones (drystone structure + the big-stone size read)."""
        by_course: dict[tuple[int, int], list[_Cell]] = {}
        for c in cells:
            by_course.setdefault((c.key[0], c.seg), []).append(c)
        taken: set[tuple] = set()
        merged: list[_Cell] = []
        drop:   set[tuple] = set()
        for c in cells:
            if (c.is_top or c.is_quoin or c.key in taken
                    or rng.random() > _THROUGH_FRAC):
                continue
            above = by_course.get((c.key[0] + 1, c.seg), [])
            for a in above:
                if (a.is_quoin or a.key in taken
                        or min(c.t1, a.t1) - max(c.t0, a.t0) < 3.0
                        or a.z1 - c.z0 > _THROUGH_MAX_MM):
                    continue
                lo = max(c.t0, a.t0)
                hi = min(c.t1, a.t1)
                cell = _Cell(
                    seg=c.seg, t0=lo, t1=hi,
                    end0='joint', end1='joint',
                    z0=c.z0, z1=a.z1,
                    is_top=a.is_top, is_bottom=c.is_bottom,
                    key=(c.key[0], c.seg, c.key[2], _K_THROUGH),
                )
                # The side cracks cross two course bands, each shared
                # with that band's neighbour (drift keyed per course).
                cell.side_bands = [(c.key[0], c.z0, c.z1),
                                   (c.key[0] + 1, a.z0, a.z1)]
                merged.append(cell)
                taken.add(c.key)
                taken.add(a.key)
                for donor in (c, a):
                    if donor.t0 < lo - 1.0 or donor.t1 > hi + 1.0:
                        # keep the wider remainder as a shrunken cell
                        if donor.t0 < lo - 1.0:
                            donor.t1 = lo
                            donor.end1 = 'joint'
                        else:
                            donor.t0 = hi
                            donor.end0 = 'joint'
                    else:
                        drop.add(donor.key)
                break
        out = [c for c in cells if c.key not in drop]
        out.extend(merged)
        return out

    def _split_cells(self, cells: list[_Cell],
                     rng: np.random.Generator) -> list[_Cell]:
        """Some tall cells become two stacked thinner stones (full
        width — the pair still fills the cell; both share the new bed
        line, so the split is a crack like any other)."""
        out: list[_Cell] = []
        for c in cells:
            h = c.z1 - c.z0
            if (c.is_top or c.is_quoin or len(c.key) > 3
                    or h < _SPLIT_H_MIN_MM
                    or rng.random() > _SPLIT_H_PROB):
                out.append(c)
                continue
            zm = c.z0 + h * rng.uniform(0.45, 0.65)
            for z0, z1, tag, bot in ((c.z0, zm, _K_SPLIT_B, c.is_bottom),
                                     (zm, c.z1, _K_SPLIT_T, False)):
                out.append(_Cell(seg=c.seg, t0=c.t0, t1=c.t1,
                                 end0=c.end0, end1=c.end1, z0=z0, z1=z1,
                                 is_top=False, is_bottom=bot,
                                 key=c.key + (tag,)))
        return out

    # ── the stone ────────────────────────────────────────────────────────────
    def _outline(self, cell: _Cell, seg: _Seg, seg_i: int,
                 brng: np.random.Generator) -> np.ndarray:
        """Face outline polygon (t, z) bounded by the cell's cracks.
        Every edge evaluates curves shared with the neighbour across it;
        beds never overlap.  Head joints DO (E24): each stone extends
        past its side lines so the union fuses side-by-side neighbours
        into pressed contact — except in the top course."""
        bands = getattr(cell, 'side_bands',
                        [(cell.key[0], cell.z0, cell.z1)])
        coping = getattr(cell, 'coping', False)
        w = cell.t1 - cell.t0
        if coping:
            # On-edge coping stones press sideways into each other.
            ov0 = float(brng.uniform(*_COPING_OV_MM))
            ov1 = float(brng.uniform(*_COPING_OV_MM))
        elif cell.is_top:
            ov0 = ov1 = 0.0
        else:
            cap = 0.25 * w
            ov0 = min(float(brng.uniform(*self.head_overlap_mm)), cap)
            ov1 = min(float(brng.uniform(*self.head_overlap_mm)), cap)
        # Coping tops drop individually below H — the ragged coping line.
        top_drop = (float(brng.uniform(0.0, _COPING_TOP_DROP_MM))
                    if coping else 0.0)

        def zeff(z: float, t: float) -> float:
            """Actual (wobbled) height of the bed line at t."""
            if cell.is_bottom and z <= cell.z0 + 1e-9:
                # flat buried seat (token embed when seated on pavement)
                return -getattr(self, '_embed_eff', self.embed_mm)
            if cell.is_top and z >= cell.z1 - 1e-9:
                return z - top_drop            # flat cap plane (R6)
            return z + float(self._bed(seg_i, z, seg.L)(t))

        def side(t_cut: float, end: str, off: float, inward: float,
                 which: str = ''):
            """[(t, z), …] bottom→top along this side crack.  Interior
            band boundaries contribute two points (the drift changes
            where the neighbour changes) — a small shared jog.  ``off``
            pushes the whole line past the shared curve (head-joint
            overlap); the curve itself stays keyed at t_cut.
            ``inward`` (+1 left side, −1 right side) orients the
            'face'-end margin."""
            if end == 'press':
                # Butts an opening surround: the single angled cut
                # line computed at trim time (already pressed
                # surround_bond_press INTO the units) — drystone
                # stones TOUCH; the union fuses the contact (no end
                # margin, no drift wave that could open a gap).  The
                # sphere-morph rounds the cut arris like every edge.
                line = getattr(cell, which, None) if which else None
                if line is not None:
                    tA, tB = line
                    return [(tA, zeff(cell.z0, tA)),
                            (tB, zeff(cell.z1, tB))]
                return [(t_cut, zeff(cell.z0, t_cut)),
                        (t_cut, zeff(cell.z1, t_cut))]
            if end == 'face':                  # wall end / corner arris
                # Recede inside the tile plane: texture displacement
                # (~0.6 mm) past the boundary gets plane-cut — the
                # E24 "sheared-off end rocks".  Laid flat the ends ARE
                # tile edges and pavement must run flush (Shawn).
                em = 0.0 if self.laid_flat else _END_MARGIN_MM
                tq = t_cut + inward * em
                return [(tq, zeff(cell.z0, tq)),
                        (tq, zeff(cell.z1, tq))]
            if coping:
                # Shared lean: both flanking stones evaluate the same
                # slanted joint line (keyed at t_cut).
                lean = self._lean(seg_i, t_cut)
                return [(t_cut + off + lean * (z - cell.z0), zeff(z, t_cut))
                        for z in (cell.z0, cell.z1)]
            pts = []
            for crs, zb0, zb1 in bands:
                d, bow = self._drift(seg_i, crs, t_cut)
                zm = (zb0 + zb1) / 2.0
                hh = (zb1 - zb0) / 2.0
                for z in np.linspace(zb0, zb1, 5):
                    q = (z - zm) / hh
                    t = t_cut + off + d * q / 2.0 + bow * (1.0 - q * q)
                    zpt = zeff(z, t) if z in (zb0, zb1) else z
                    pts.append((t, zpt))
            return pts

        left  = side(cell.t0, cell.end0, -ov0, +1.0, 'cut0')
        right = side(cell.t1, cell.end1, +ov1, -1.0, 'cut1')

        def bed(z: float, ta: float, tb: float):
            n = max(2, int(abs(tb - ta) / 4.0) + 2)
            ts = np.linspace(ta, tb, n)
            return [(t, zeff(z, t)) for t in ts]

        bot = bed(cell.z0, left[0][0], right[0][0])
        top = bed(cell.z1, right[-1][0], left[-1][0])

        # CCW: bottom left→right, right side up, top right→left, left
        # down.  Slices drop the duplicated corner points (bed edges
        # include both endpoints, which coincide with the side ends).
        pts = np.asarray(bot + right[1:] + top[1:] + left[::-1][1:-1],
                         dtype=float)

        # Bed overlap (E25): stretch the outline past its bed lines —
        # bottom down by ovb, top up by ovt, interiors interpolated —
        # so vertically adjacent stones press together like the E24
        # head joints.  Top course exempt (perched rocks); the bottom
        # course keeps its flat buried seat.
        if coping:
            # Press down into the course below (which also dilates up).
            zlo, zhi = pts[:, 1].min(), pts[:, 1].max()
            u = (pts[:, 1] - zlo) / max(zhi - zlo, 1e-9)
            ovb = float(brng.uniform(*_COPING_SEAT_OV_MM))
            pts[:, 1] -= ovb * (1.0 - u)
        elif not cell.is_top:
            h = cell.z1 - cell.z0
            cap = 0.22 * h
            ovt = min(float(brng.uniform(*self.bed_overlap_mm)), cap)
            ovb = 0.0 if cell.is_bottom else min(
                float(brng.uniform(*self.bed_overlap_mm)), cap)
            zlo, zhi = pts[:, 1].min(), pts[:, 1].max()
            u = (pts[:, 1] - zlo) / max(zhi - zlo, 1e-9)
            pts[:, 1] += ovt * u - ovb * (1.0 - u)
        return pts

    def _core_boxes(self, segs: list[_Seg], seat_z: float) -> list:
        """Drystone: NO mortar core (E26, Shawn: "don't put mortar in
        them").  The wall is stones all the way through — face stones
        interpenetrate at their bed and head overlaps, and the rubble
        hearting fills the interior, so the union is one connected
        solid and every crack is floored by deeper stone.  (E25 had
        already lowered the core below the top course; this removes
        it everywhere.)"""
        return []

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh | None:
        import shapely
        import shapely.geometry as sgeom

        seg = segs[cell.seg]
        brng = np.random.default_rng(
            (self.seed * 1_000_003 + hash(cell.key)) & 0x7FFFFFFF)
        outline = self._outline(cell, seg, cell.seg, brng)

        # Inset by half the crack, round the corners (negative-positive
        # buffer): the crack is a real thin gap by construction.  The
        # roundover is drawn per stone (E16), capped by the stone's own
        # size so small stones don't vanish in the inset.
        span = outline.max(axis=0) - outline.min(axis=0)
        r = min(float(brng.uniform(*self.roundover_mm)),
                _ROUND_SIZE_CAP * float(min(span)))
        poly = sgeom.Polygon(outline)
        if not poly.is_valid:
            poly = poly.buffer(0.0)
        poly = poly.buffer(-(self.joint_mm / 2.0 + r), join_style=2,
                           mitre_limit=2.0)
        poly = poly.buffer(r, join_style=1, quad_segs=6)
        if poly.is_empty:
            return None
        if poly.geom_type == 'MultiPolygon':
            poly = max(poly.geoms, key=lambda g: g.area)
        poly = shapely.geometry.polygon.orient(poly, 1.0)

        # Ring = the polygon's own vertices, long edges densified.  A
        # uniform arc-length resample UNDER-samples the rounded corners
        # (2–3 points per 0.4 mm arc) — the sharp normal jumps rendered
        # as vertical banding fans on the stone flanks.  segmentize
        # keeps every corner vertex and only adds points on straights.
        dense = shapely.segmentize(poly, _RING_STEP_MM)
        ring = np.asarray(dense.exterior.coords)[:-1]
        n = len(ring)

        # Per-stone INWARD outline noise (E21): irregular silhouettes,
        # cracks that open and close along their run.  Inward-only, so
        # the no-overlap tessellation guarantee is untouched.
        tang = np.roll(ring, -1, axis=0) - np.roll(ring, 1, axis=0)
        nrm = np.column_stack([tang[:, 1], -tang[:, 0]])     # CCW → out
        nrm /= np.linalg.norm(nrm, axis=1, keepdims=True) + 1e-12
        for _ in range(2):
            nrm += 0.5 * (np.roll(nrm, 1, axis=0)
                          + np.roll(nrm, -1, axis=0))
            nrm /= np.linalg.norm(nrm, axis=1, keepdims=True) + 1e-12
        seglen = np.linalg.norm(
            np.diff(np.vstack([ring, ring[:1]]), axis=0), axis=1)
        s_arc = np.concatenate([[0.0], np.cumsum(seglen[:-1])])
        per = float(seglen.sum())
        wgt = np.ones(n)
        for lam, ph in zip(per / brng.uniform(1.5, 3.5, 2),
                           brng.uniform(0.0, 2.0 * np.pi, 2)):
            wgt *= 0.5 * (1.0 + np.sin(2.0 * np.pi * s_arc / lam + ph))
        lo, hi = _RING_NOISE_MM
        ring = ring - nrm * (lo + (hi - lo) * wgt)[:, None]

        # Sphere-morph (E17): latitude rings sweep from the front pole
        # to the back pole; each ring is the outline scaled about the
        # (drifting) centre by cos(lat)^k, at depth y = mid + h·sin(lat).
        # The equator IS the outline at mid-depth, so the silhouette,
        # the cracks, and the stone's dimensions are unchanged — but
        # every surface is smoothly curved.  No whole-stone rotation —
        # it would open wedges; per-stone character comes from the
        # outline, the pole drift, and the meridian exponents.
        def recess():
            if brng.random() < _PROUD_DEEP_PROB:
                return brng.uniform(*_PROUD_DEEP_MM)
            return brng.uniform(*self.proud_mm)
        y0, y1 = recess(), self.thickness_mm - recess()
        if self.laid_flat:
            # q = 0 is the pavement top: keep the proud jitter there
            # (per-flagstone height variation), overshoot the datum on
            # the underside for the bottom clip.
            y1 = self.thickness_mm + 1.0
        h = (y1 - y0) / 2.0
        ym = (y0 + y1) / 2.0

        ctr = np.array([poly.centroid.x, poly.centroid.y])
        a_f, a_b = brng.uniform(*_MORPH_A, 2)
        b_f, b_b = brng.uniform(*_MORPH_B, 2)
        pz = brng.uniform(*self.bed_flat_exp)
        pole_f = ctr + brng.uniform(-_POLE_DRIFT_FRAC,
                                    _POLE_DRIFT_FRAC, 2) * span
        pole_b = ctr + brng.uniform(-_POLE_DRIFT_FRAC,
                                    _POLE_DRIFT_FRAC, 2) * span

        coping = getattr(cell, 'coping', False)
        K = _N_LAT
        verts = []
        for jlat in range(K):
            lat = -np.pi / 2.0 + np.pi * (jlat + 1) / (K + 1)
            u = abs(np.sin(lat))
            ae, be, pole = ((a_f, b_f, pole_f) if lat < 0
                            else (a_b, b_b, pole_b))
            sk = (1.0 - u ** ae) ** be
            # Anisotropic (E19): t follows the morph, z flattens as
            # sk^pz — beds stay planes over most of the depth, closing
            # to the pole point only right at the faces.  Coping stones
            # lie ON EDGE (E29): their flat "beds" are the SIDES, so
            # the flattened axis swaps from z to t.
            szk = sk ** pz
            st_, sz_ = (szk, sk) if coping else (sk, szk)
            rk = np.column_stack([
                ctr[0] + (pole[0] - ctr[0]) * (1.0 - st_)
                + (ring[:, 0] - ctr[0]) * st_,
                ctr[1] + (pole[1] - ctr[1]) * (1.0 - sz_)
                + (ring[:, 1] - ctr[1]) * sz_,
            ])
            y = ym + h * np.sin(lat)
            verts.append(np.column_stack([rk[:, 0],
                                          np.full(n, y), rk[:, 1]]))
        v = np.vstack(verts)
        i = np.arange(n)
        j = (i + 1) % n
        faces = []
        for k in range(K - 1):
            a, b = k * n, (k + 1) * n
            faces.append(np.column_stack([a + i, a + j, b + j]))
            faces.append(np.column_stack([a + i, b + j, b + i]))
        # Poles close the sphere: small fans on a smoothly curved dome
        # render cleanly (UV-sphere poles), unlike fans across the old
        # FLAT caps — no Delaunay cap machinery needed any more.
        pf = np.array([pole_f[0], y0, pole_f[1]])
        pb = np.array([pole_b[0], y1, pole_b[1]])
        v = np.vstack([v, pf[None], pb[None]])
        faces.append(np.column_stack([np.full(n, K * n), j, i]))
        faces.append(np.column_stack([np.full(n, K * n + 1),
                                      (K - 1) * n + i, (K - 1) * n + j]))
        f = np.vstack(faces)
        body = trimesh.Trimesh(vertices=v, faces=f, process=False)
        trimesh.repair.fix_normals(body)

        # Surface texture (E20): the SHIPPED scatter-stones recipe
        # (stones.py aged pass), scaled to wall-stone footprints —
        # broad organic undulation + granular drybrush micro-grain,
        # modulated by a patchy envelope (calm fields vs active
        # shoulders), displaced along SMOOTHED normals with curvature
        # damping (relief fades in tight concave features — which also
        # protects the crack roots, exactly where stones touch).
        # relief_mm: None = auto amplitude from the stone's footprint;
        # 0 disables; a number overrides the undulation amplitude.
        if self.relief_mm is None or self.relief_mm > 0.0:
            body = self._stone_texture(body, brng)

        body.apply_transform(self._lay(seg) if self.laid_flat
                             else _frame(seg, z=seat_z))
        return body

    def _stone_texture(self, body: trimesh.Trimesh,
                       brng: np.random.Generator) -> trimesh.Trimesh:
        """The common stone relief (stone/finish.py) at wall-stone
        scale: plateau carved into worn recesses + gentle dish, patchy
        calm/incident envelope, curvature damping (which also protects
        the crack roots, exactly where the zero-gap stones touch).
        Carve depth auto-scales with the stone's footprint unless
        relief_mm overrides it."""
        p = np.asarray(body.vertices)
        foot = float(np.ptp(p[:, [0, 2]], axis=0).max())
        amp = (float(np.clip(0.05 * foot, 0.15, 0.50))
               if self.relief_mm is None else self.relief_mm)
        return stone_relief(body, brng,
                            scale_mm=max(foot / 2.4, 2.2),
                            carve_mm=amp, band=0.45,
                            dish_mm=0.5 * amp,
                            env=(0.35, foot), refine=1)

    # ── rubble hearting ──────────────────────────────────────────────────────
    # The hearting itself lives on the chassis (_hearting_parts); the
    # fieldstone-specific part is only the fill ceiling: rubble stops
    # beneath the top course / coping (E25 — gaps between the perched
    # top rocks or coping stones must show stone below, never chips
    # poking above them).
    def _heart_cap(self, seg_i: int, t: float) -> float:
        return min(super()._heart_cap(seg_i, t), self._cap_z0)
