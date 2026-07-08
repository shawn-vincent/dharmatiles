"""
Bond-to-surround fit — the mason's cut pieces around an opening.

Design: docs/design/walls-doors.md ("Bond-to-surround fit").  A pure,
deterministic solver (no randomness anywhere): given the wall's block
cells and one opening's forbidden REGION — the surround units dilated
by the joint/press rule plus the passage dilated by the reveal — decide
per course band how each overlapping cell is re-shaped so the bond
tooths into the quoin-alternating jambs and follows the arch extrados,
with the gap to the surround an ordinary joint, never an exposed
mortar wedge.

Every cut is a SINGLE STRAIGHT LINE per side (Shawn: not a literal
curve trace): sample the region's support, least-squares a line, shift
it until every sample clears.  The cell carries the line as an
endpoint pair (``cut0``/``cut1`` for side cuts, ``cut_z0``/``cut_z1``
for horizontal ones); at place time it becomes ONE extra plane in the
block kernel's smooth-max, so the cut arris gets the same roundover as
every other edge (fieldstone applies the same line to its crack
outline and the sphere-morph rounds it natively).

The decision policy, per cell in the opening's segment:

1. If the surround only dips into the band from one horizontal side
   (all columns 'top' or all 'bottom'), a single horizontal cut that
   keeps the FULL block width is a candidate — taken only when it
   keeps more stone than the two side cuts would (a jamb grazing one
   end also classifies 'bottom', but there the side cut saves the
   whole brick).
2. Side remnants beside the surround get one angled side cut each;
   remnants narrower than ``_MIN_KEEP_MM`` are never dropped — they
   go through the fallback chain: ABSORB into the course neighbour
   (which extends across the vanished head joint), else a THIN END
   UNIT (a printable wedge at a wall end), else a SHORT BRICK fitted
   to the sub-band where a bimodal blocker (a sill nose over a jamb)
   leaves room.
3. A middle strip under a top-dip / over a bottom-rise between the
   flanks (or around an 'island' blocker — a hatch frame mid-slab)
   gets the mason's third piece, z-clipped by its horizontal support
   line.
"""
from __future__ import annotations

from dataclasses import replace

import numpy as np
import shapely.geometry as sgeom
from shapely.ops import unary_union

from .openings import band_extent

# The block box extends this margin past its cut line, so the kernel's
# own end plane never competes with the cut plane.
_CUT_MARGIN_MM = 1.2
# Trimmed remnants narrower than this go through the fallback chain
# (absorb / thin end unit / short brick) instead of standing alone.
_MIN_KEEP_MM   = 2.6
# A horizontal blocker penetrating less than this fuses invisibly —
# no cut needed (graze).
_GRAZE_MM      = 0.35
# A horizontal cut must keep at least a printable course height
# (a width rule like _MIN_KEEP_MM would wrongly reject a wide
# 1 mm-tall cut course under a sill — that is ordinary masonry).
_MIN_COURSE_MM = 1.0


def forbidden_region(quads, profile, dilate_mm: float, reveal_mm: float):
    """The region a fitted block must clear: the surround unit
    rectangles dilated by ``dilate_mm`` (= joint − press: a thin
    mortar line on mortared families, pressed interpenetration on
    drystone) + the passage profile dilated by the reveal."""
    return unary_union(
        [sgeom.Polygon(q).buffer(dilate_mm, join_style=2) for q in quads]
        + [sgeom.Polygon(profile).buffer(reveal_mm, quad_segs=6)])


def _fit_line(sa: np.ndarray, va: np.ndarray, keep_low: bool,
              e0: float, e1: float) -> tuple[float, float]:
    """Least-squares line v(s) through the support samples, shifted by
    the worst residual so EVERY sample clears (toward low v when
    ``keep_low``), evaluated at the span ends."""
    if len(sa) == 1:
        m, k = 0.0, float(va[0])
    else:
        m, k = np.polyfit(sa, va, 1)
    res = va - (m * sa + k)
    k += float(res.min() if keep_low else res.max())
    return (float(m * e0 + k), float(m * e1 + k))


def side_cut_line(region, z0: float, z1: float, side: str):
    """The single angled cut clearing the surround within the band on
    the given side ('L' = block to the left of the region, cut at its
    left extent; 'R' mirrored).  Returns (t@z0, t@z1) or None."""
    zs = np.linspace(z0 + 0.02, z1 - 0.02, 9)
    b0, _zl, b1, _zh = region.bounds
    pts = []
    for z in zs:
        row = region.intersection(
            sgeom.LineString([(b0 - 1.0, z), (b1 + 1.0, z)]))
        if row.is_empty:
            continue
        rb = row.bounds
        pts.append((z, rb[0] if side == 'L' else rb[2]))
    if not pts:
        return None
    za = np.array([p[0] for p in pts])
    ta = np.array([p[1] for p in pts])
    return _fit_line(za, ta, side == 'L', z0, z1)


def level_cut_line(region, t0: float, t1: float, z0: float, z1: float,
                   side: str):
    """Horizontal counterpart: the support line UNDER a surround
    hanging into the band from above (side 'T' → the block keeps its
    full width, cut from the top — the stones beneath a ring/arch
    bottom) or OVER one rising from below ('B' — the bond flowing
    over a keystone).  Returns (z@t0, z@t1) or None."""
    ts = np.linspace(t0 + 0.05, t1 - 0.05, 9)
    pts = []
    for t in ts:
        col = region.intersection(sgeom.LineString(
            [(t, z0 - 0.5), (t, z1 + 0.5)]))
        if col.is_empty:
            continue
        cb = col.bounds
        pts.append((t, cb[1] if side == 'T' else cb[3]))
    if not pts:
        return None
    ta = np.array([p[0] for p in pts])
    za = np.array([p[1] for p in pts])
    return _fit_line(ta, za, side == 'T', t0, t1)


def column_kinds(region, t_lo: float, t_hi: float,
                 z0: float, z1: float) -> set[str]:
    """Classify how the surround region blocks the band interval: per
    t-column, does it span the full band height ('full'), hang from
    the top ('top'), rise from the bottom ('bottom'), or float as an
    'island' (a hatch frame mid-slab: strips survive on BOTH sides —
    below via cut_z1, above via cut_z0)?"""
    ts = np.linspace(t_lo + 0.05, t_hi - 0.05, 7)
    kinds: set[str] = set()
    for t in ts:
        col = region.intersection(sgeom.LineString(
            [(t, z0 - 0.5), (t, z1 + 0.5)]))
        if col.is_empty:
            continue
        cb = col.bounds
        at_top = cb[3] >= z1 - 0.05
        at_bot = cb[1] <= z0 + 0.05
        if at_top and at_bot:
            kinds.add('full')
        elif at_top:
            kinds.add('top')
        elif at_bot:
            kinds.add('bottom')
        else:
            kinds.add('island')
    return kinds


def _horizontal_cut(c, oi: int, region, kinds, lrem, rrem):
    """Policy step 1: the full-width horizontal cut, if it keeps more
    stone than the side cuts would.  Returns the re-shaped cell, None
    to fall through to the side cuts, or 'graze'/'gone' sentinels."""
    side = 'T' if kinds == {'top'} else 'B'
    line = level_cut_line(region, c.t0, c.t1, c.z0, c.z1, side)
    if line is None:
        return None
    if side == 'T':
        pen = c.z1 - min(line)
        keep = max(line) - c.z0
    else:
        pen = max(line) - c.z0
        keep = c.z1 - min(line)
    if pen < _GRAZE_MM:
        return 'graze'
    bh = c.z1 - c.z0
    area_h = (c.t1 - c.t0) * keep if keep >= _MIN_COURSE_MM else 0.0
    area_s = (lrem if lrem >= _MIN_KEEP_MM else 0.0) * bh \
        + (rrem if rrem >= _MIN_KEEP_MM else 0.0) * bh
    if area_h < area_s:
        return None
    if area_h <= 0.0:
        return 'gone'           # nothing substantial left either way
    nc = replace(c, key=c.key + (705, oi))
    if side == 'T':
        nc.cut_z1 = line
    else:
        nc.cut_z0 = line
    return nc


def _middle_pieces(c, oi: int, region, kinds, lo, hi):
    """Policy step 3: the mason's third piece — the strip under a
    top-dip / over a bottom-rise between the flanks belonged to no
    one (bare core south of a hatch frame).  Full dip width,
    z-clipped by the horizontal support line (skipped automatically
    when full-height blockers leave no clearance: the line lands on
    the band edge)."""
    mid0, mid1 = max(c.t0, lo), min(c.t1, hi)
    if mid1 - mid0 < 1.6:
        return []
    out = []
    for dip, attr in (('top', 'cut_z1'), ('bottom', 'cut_z0')):
        if dip not in kinds and 'island' not in kinds:
            continue
        hl = level_cut_line(region, mid0, mid1, c.z0, c.z1,
                            'T' if dip == 'top' else 'B')
        if hl is None:
            continue
        if dip == 'top':
            keep = (sum(hl) / 2.0) - c.z0
            kw = dict(z1=min(max(hl) + _CUT_MARGIN_MM, c.z1),
                      is_top=False)
        else:
            keep = c.z1 - (sum(hl) / 2.0)
            kw = dict(z0=max(min(hl) - _CUT_MARGIN_MM, c.z0),
                      is_bottom=False)
        if keep < _MIN_COURSE_MM:
            continue
        nc = replace(c, t0=mid0, t1=mid1, end0='press', end1='press',
                     key=c.key + (709, oi), **kw)
        setattr(nc, attr, hl)
        out.append(nc)
    return out


def _absorb_into_neighbour(cells, seg_i: int, oi: int,
                           src, side, target, line) -> bool:
    """Fallback 1: the course neighbour extends across the vanished
    head joint to the surround — a mason's cut unit, never a column
    of exposed core (the E15 flat mortar band beside the brick
    door).  Mutates ``cells`` in place; False when there is no
    neighbour on that side."""
    edge = src.t0 if side == 'end1' else src.t1
    for i, c in enumerate(cells):
        if c.seg != seg_i or abs(c.z0 - src.z0) > 1e-6:
            continue
        if side == 'end1' and abs(c.t1 - edge) < 1e-6:
            nc = replace(c, t1=target, end1='press',
                         key=c.key + (703, oi))
            nc.cut1 = line
            cells[i] = nc
            return True
        if side == 'end0' and abs(c.t0 - edge) < 1e-6:
            nc = replace(c, t0=target, end0='press',
                         key=c.key + (704, oi))
            nc.cut0 = line
            cells[i] = nc
            return True
    return False


def _thin_end_unit(oi: int, src, side, target, line):
    """Fallback 2: no course neighbour (the remnant sits at a wall
    end / corner — e.g. the strip between a window's projecting sill
    and the free end).  A THIN printable wedge with one angled cut,
    or None when even that is too narrow."""
    if side == 'end1':
        mean_w = (sum(line) / 2.0 if line else target) - src.t0
    else:
        mean_w = src.t1 - (sum(line) / 2.0 if line else target)
    if mean_w < 1.1:
        return None
    if side == 'end1':
        nc = replace(src, t1=target, end1='press',
                     key=src.key + (706, oi))
        nc.cut1 = line
    else:
        nc = replace(src, t0=target, end0='press',
                     key=src.key + (707, oi))
        nc.cut0 = line
    return nc


def _short_brick(region, oi: int, src, side, target):
    """Fallback 3: a bimodal blocker (a projecting sill nose over a
    jamb) — one straight cut can't clear both, but a SHORT brick fits
    the sub-band where the space is wide (the mason's cut brick under
    the sill).  None when no printable sub-band exists."""
    zs = np.linspace(src.z0 + 0.02, src.z1 - 0.02, 15)
    bb0, _zl, bb1, _zh = region.bounds
    avail = []
    for z in zs:
        row = region.intersection(sgeom.LineString(
            [(bb0 - 1.0, z), (bb1 + 1.0, z)]))
        if row.is_empty:
            avail.append(src.t1 - src.t0)
            continue
        rb = row.bounds
        avail.append(rb[0] - src.t0 if side == 'end1'
                     else src.t1 - rb[2])
    ok = np.asarray(avail) >= 1.8
    best = (0, 0)               # widest True run as (start, stop)
    i = 0
    while i < len(ok):
        if ok[i]:
            j = i
            while j < len(ok) and ok[j]:
                j += 1
            if j - i > best[1] - best[0]:
                best = (i, j)
            i = j
        else:
            i += 1
    if best[1] == best[0]:
        return None
    z_lo = src.z0 if best[0] == 0 else float(zs[best[0]])
    z_hi = src.z1 if best[1] == len(ok) else float(zs[best[1] - 1])
    if z_hi - z_lo < _MIN_COURSE_MM:
        return None
    sline = side_cut_line(region, z_lo, z_hi,
                          'L' if side == 'end1' else 'R')
    kw = dict(z0=z_lo, z1=z_hi, is_top=False)
    if side == 'end1':
        kw.update(t1=(max(sline) + _CUT_MARGIN_MM) if sline else target,
                  end1='press')
    else:
        kw.update(t0=(min(sline) - _CUT_MARGIN_MM) if sline else target,
                  end0='press')
    nc = replace(src, key=src.key + (708, oi), **kw)
    if side == 'end1':
        nc.cut1 = sline
    else:
        nc.cut0 = sline
    return nc


def fit_cells(cells: list, seg_i: int, oi: int, quads, region) -> list:
    """Re-shape the wall cells around one opening (the policy at the
    top of this module).  ``cells`` covers the whole wall; only cells
    of segment ``seg_i`` whose course band meets a surround unit are
    touched.  Returns the new cell list."""
    out = []
    absorb = []     # (cell, side, target, line): remnants too narrow
    #               to keep, routed through the fallback chain below
    for c in cells:
        if c.seg != seg_i:
            out.append(c)
            continue
        exts = [e for q in quads
                if (e := band_extent(q, c.z0, c.z1)) is not None]
        if not exts:
            out.append(c)
            continue
        lo = min(e[0] for e in exts)
        hi = max(e[1] for e in exts)
        if c.t1 <= lo + 1e-6 or c.t0 >= hi - 1e-6:
            out.append(c)
            continue
        kinds = column_kinds(region, max(c.t0, lo), min(c.t1, hi),
                             c.z0, c.z1)
        if not kinds:
            out.append(c)
            continue
        lrem = lo - c.t0
        rrem = c.t1 - hi
        if kinds == {'top'} or kinds == {'bottom'}:
            got = _horizontal_cut(c, oi, region, kinds, lrem, rrem)
            if got == 'graze':
                out.append(c)
                continue
            if got == 'gone':
                continue
            if got is not None:
                out.append(got)
                continue
            # else: fall through to the side cuts
        if lrem > 1e-6:
            line = side_cut_line(region, c.z0, c.z1, 'L')
            t1 = (max(line) + _CUT_MARGIN_MM) if line else lo
            if lrem >= _MIN_KEEP_MM:
                nc = replace(c, t1=t1, end1='press',
                             key=c.key + (701, oi))
                nc.cut1 = line
                out.append(nc)
            else:
                absorb.append((c, 'end1', t1, line))
        if rrem > 1e-6:
            line = side_cut_line(region, c.z0, c.z1, 'R')
            t0 = (min(line) - _CUT_MARGIN_MM) if line else hi
            if rrem >= _MIN_KEEP_MM:
                nc = replace(c, t0=t0, end0='press',
                             key=c.key + (702, oi))
                nc.cut0 = line
                out.append(nc)
            else:
                absorb.append((c, 'end0', t0, line))
        out.extend(_middle_pieces(c, oi, region, kinds, lo, hi))
    for src, side, target, line in absorb:
        if _absorb_into_neighbour(out, seg_i, oi, src, side, target,
                                  line):
            continue
        nc = _thin_end_unit(oi, src, side, target, line)
        if nc is None:
            nc = _short_brick(region, oi, src, side, target)
        if nc is not None:
            out.append(nc)
    return out
