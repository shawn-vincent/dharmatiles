"""
Fieldstone / drystone wall — walls campaign family 3.

Same chassis as :class:`CutStoneWall` (spine → segments, courses/bays
with bond offset, quoin alternation, recessed core, seat/stamp/clip);
the UNIT and the cell field change.  Reference read
(`docs/reference/walls/README.md` drystone photo, Hirst m70): **flat
slabby stones laid on their beds** — width ≫ height on the face — in
rough courses, packed to nearly touching with thin dark shadow joints
(no mortar), big size variety with occasional throughstones spanning
two courses, and the corner built from the same stones, just squarer.

Shawn round-3 verdict on the first cut ("terrible"): stones were too
equant and egg-like, the corner kept cut-stone BRICKS, joints gaped,
sizes were uniform.  This rework:

- stones are generated as slabs (vertical support directions squashed)
  so every stone reads laid-flat, like the m70 strips;
- corners use the same fieldstone unit at high blockiness — no
  `_block_mesh` bricks anywhere in this wall;
- packing: stones overshoot their cells and are shaved back by the
  rounding (`_PACK_COMP_MM`), joints thin (`joint_mm` 0.5);
- size irregularity: bay range widened AND a `_cells` post-pass merges
  ~1/7 of cells with the cell above into a double-height throughstone,
  plus per-stone height jitter down to ~55 % of the course (small
  pinning stones between big ones).
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..scatter.stones import _blur_remesh, _relief_field, _round_edges
from .masonry import CutStoneWall, _Cell, _Seg, _RELIEF_WAVES

# ── Iteration knobs (module constants while prototyping) ─────────────────────
_BLOCKINESS       = (0.55, 0.85)  # ellipsoid↔box blend for body stones —
                                  # below ~0.5 the silhouette recedes
                                  # between hull corners and the shared
                                  # edges open into mortar channels
                                  # (red diagnostic, round 2)
_QUOIN_BLOCKINESS = (0.70, 0.90)  # corner stones: same unit, squarer
_CAP_BLOCKINESS   = (0.60, 0.85)  # coping stones flat-topped and squarer
_LUMP             = (0.90, 1.0)   # per-point radius jitter
_DIR_JITTER       = 0.12          # support-direction jitter
_FLATTEN          = (0.50, 0.80)  # vertical squash of support directions:
                                  # stones are SLABS laid on their beds —
                                  # the round-3 "flatter stones" call.
                                  # Applied in direction space, so the hull
                                  # fills the (short) cell height while the
                                  # silhouette stays wide
_PACK_COMP_MM     = 2.1           # build oversize so rounding keeps real
                                  # curvature; exact-fit rescales to target
_FLATTEN_Y        = (0.35, 0.60)  # face squash — broad flattish face patch
                                  # (mortar diagnostic: tangent bellies covered
                                  # ~half the face; red sea between stones)
_FLATTEN_X_END    = (0.55, 0.80)  # end squash for wall-end stones only
_OVERLAP_MM       = 0.45          # stones overlap past joint midlines by
                                  # this — stacked stones TOUCH; the union
                                  # merges contacts into bedding creases
_BULGE_MM         = 0.35          # residual vertical swell into joints
_THROUGH_FRAC     = 0.20          # fraction of eligible cells merged with
                                  # the cell above into a throughstone
_PIN_PROB         = 1.0           # chance of a pinning stone at each
                                  # joint/course-line crossing — the star-
                                  # shaped voids four rounded shoulders
                                  # open up ("every space filled with a
                                  # rock", round-4)
_PIN_W            = (2.5, 5.0)    # pinning stone width
_PIN_H            = (2.2, 4.2)    # pinning stone height
_REMESH_SIGMA     = 0.9           # aged read (stones convention)
_RUBBLE_SPACING_MM = 4.8          # hearting grid pitch over each face
_RUBBLE_FOOT      = (6.0, 9.5)    # rubble footprint ≥ pitch → the sheet
                                  # seals by overlap: any void between big
                                  # stones is a window onto rubble, never
                                  # onto the flat core ("mortar plane")
_RUBBLE_H         = (3.8, 6.2)
_RUBBLE_SETBACK_MM = 0.15         # rubble faces sit this far behind the
                                  # wall face planes (reads as depth)


def _fieldstone_mesh(lx: float, ly: float, lz: float, blockiness: float,
                     flat_top: bool, roundover_mm: float, relief_mm: float,
                     relief_wl: tuple[float, float],
                     rng: np.random.Generator,
                     flatten_y: float = 1.0,
                     flatten_x: float = 1.0,
                     aged: bool = True) -> trimesh.Trimesh:
    """One flat-bedded stone filling the local box [0,lx]×[0,ly]×[0,lz]."""
    M = int(rng.integers(13, 18))
    i = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta), np.cos(phi)], axis=1)
    d += rng.normal(0.0, _DIR_JITTER, d.shape)
    # Squashes in direction space flatten the hull toward that axis's
    # planes.  z: flat-laid bed/top (the slab read).  y: broad flattish
    # FACE patches — the mortar-diagnostic fix: a round belly touches
    # the face plane at one point, so straight-on the wall read as
    # stones floating in mortar; a builder turns the flattest side out.
    # x: only for wall-end stones (their end is a visible face).
    d[:, 0] *= flatten_x
    d[:, 1] *= flatten_y
    d[:, 2] *= rng.uniform(*_FLATTEN)
    d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-12

    half = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
    r_ell = 1.0 / np.sqrt(((d / half) ** 2).sum(axis=1))
    r_box = 1.0 / (np.abs(d) / half).max(axis=1)
    r = ((1.0 - blockiness) * r_ell + blockiness * r_box)
    r *= rng.uniform(*_LUMP, M)
    p = d * r[:, None]
    if flat_top:
        p[:, 2] = np.minimum(p[:, 2], 0.90 * half[2])
    p += half

    hull = trimesh.convex.convex_hull(p)
    v, f = np.asarray(hull.vertices), np.asarray(hull.faces)
    if roundover_mm > 0.0:
        v, f = _round_edges(v, f, roundover_mm, rng)
    body = trimesh.Trimesh(vertices=v, faces=f, process=False)
    if not aged:
        # Cheap path for rubble hearting: hull + roundover only.  The
        # rubble reads through joints, a millimetre behind the faces —
        # remesh + relief cost would be paid ~100× for background stones.
        return body
    remeshed = _blur_remesh(body, max(lx, ly, lz), _REMESH_SIGMA)
    if remeshed is not None:
        body = remeshed
        if relief_mm > 0.0:
            disp = relief_mm * _relief_field(
                body.vertices, rng, _RELIEF_WAVES, *relief_wl)
            body = trimesh.Trimesh(
                vertices=(body.vertices
                          + np.asarray(body.vertex_normals) * disp[:, None]),
                faces=body.faces, process=False)
    return body


class FieldstoneWall(CutStoneWall):
    """Direct TileLayer: a drystone fieldstone wall on a plan spine.

    Same spine convention and contracts as :class:`CutStoneWall`.
    """

    yaw_max_deg:    float = 2.5   # stones sit less true than cut blocks
    tilt_max_deg:   float = 1.2
    face_recess_mm: float = 0.45

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (3.2, 8.8),
                 bay_mm:    tuple[float, float] = (4.5, 16.0),
                 joint_mm:  float = 0.4,
                 reveal_mm: float = 2.2,
                 roundover_mm: float | None = 0.45,
                 relief_mm:    float | None = 0.12,
                 relief_wl:    tuple[float, float] | None = (2.0, 7.0),
                 min_bond_mm:  float = 1.8,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         roundover_mm=roundover_mm, relief_mm=relief_mm,
                         relief_wl=relief_wl, min_bond_mm=min_bond_mm,
                         **kwargs)

    # ── size irregularity: throughstone merges ───────────────────────────────
    def _cells(self, segs: list[_Seg], T: float, H: float,
               rng: np.random.Generator) -> list[_Cell]:
        cells = super()._cells(segs, T, H, rng)
        # Merge some cells with a cell in the course directly above into
        # a double-height throughstone (drystone structure + the wide
        # size read).  Candidates must overlap in t and neither cell may
        # be already merged, a quoin, or the top course.
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
                        or min(c.t1, a.t1) - max(c.t0, a.t0) < 3.0):
                    continue
                lo = max(c.t0, a.t0)
                hi = min(c.t1, a.t1)
                # The throughstone takes the overlap column; donor cells
                # shrink away from it so neighbours stay in place.
                merged.append(_Cell(
                    seg=c.seg, t0=lo, t1=hi,
                    end0='joint', end1='joint',
                    z0=c.z0, z1=a.z1,
                    is_top=a.is_top, is_bottom=c.is_bottom,
                    key=(c.key[0], c.seg, c.key[2], 'through'),
                ))
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
        out.extend(self._pin_cells(out, segs, H, rng))
        return out

    def _pin_cells(self, cells: list[_Cell], segs: list[_Seg], H: float,
                   rng: np.random.Generator) -> list[_Cell]:
        """Small pinning stones wedged where a vertical joint crosses a
        course line — filling the voids that rounded shoulders open."""
        throughs = [c for c in cells if len(c.key) == 4]
        pins: list[_Cell] = []
        n = 0
        for c in cells:
            segL = segs[c.seg].L
            # Throughstone flanks: a double-height stone's waist narrows
            # at the course line it spans, and its neighbours' shoulders
            # open a void there that the crossing pins never cover (the
            # crossing itself was consumed by the merge).
            if len(c.key) == 4 and c.key[3] == 'through':
                z_mids = {t.z1 for t in cells
                          if len(t.key) == 3 and t.seg == c.seg
                          and c.z0 + 0.5 < t.z1 < c.z1 - 0.5}
                for z_mid in z_mids:
                    for t_edge in (c.t0, c.t1):
                        if rng.random() > 0.8:
                            continue
                        w = rng.uniform(*_PIN_W)
                        h = rng.uniform(*_PIN_H)
                        t0 = np.clip(t_edge - w / 2.0, 0.5, segL - 0.5 - w)
                        pins.append(_Cell(
                            seg=c.seg, t0=float(t0), t1=float(t0 + w),
                            end0='joint', end1='joint',
                            z0=float(z_mid - h / 2.0),
                            z1=float(z_mid + h / 2.0),
                            is_top=False, is_bottom=False,
                            key=(-1, c.seg, n, 'pin'),
                        ))
                        n += 1
                continue
            if c.end0 != 'joint':
                continue
            for z_line in (c.z0, c.z1):
                if not 1.0 < z_line < H - 1.0:
                    # Cap joints get their own flush pins below.
                    continue
                if rng.random() > _PIN_PROB:
                    continue
                if any(t.seg == c.seg and t.z0 < z_line < t.z1
                       and t.t0 - 1.0 < c.t0 < t.t1 + 1.0
                       for t in throughs):
                    continue
                w = rng.uniform(*_PIN_W)
                h = rng.uniform(*_PIN_H)
                t0 = np.clip(c.t0 - w / 2.0, 0.5, segL - 0.5 - w)
                pins.append(_Cell(
                    seg=c.seg, t0=float(t0), t1=float(t0 + w),
                    end0='joint', end1='joint',
                    z0=float(z_line - h / 2.0), z1=float(z_line + h / 2.0),
                    is_top=False, is_bottom=False,
                    key=(-1, c.seg, n, 'pin'),
                ))
                n += 1
            # Cap-joint pins: a small stone flush with the cap plane
            # filling the dip between two coping stones.
            if c.is_top and rng.random() < _PIN_PROB:
                w = rng.uniform(*_PIN_W)
                h = rng.uniform(*_PIN_H)
                t0 = np.clip(c.t0 - w / 2.0, 0.5, segL - 0.5 - w)
                pins.append(_Cell(
                    seg=c.seg, t0=float(t0), t1=float(t0 + w),
                    end0='joint', end1='joint',
                    z0=float(H - h), z1=float(H),
                    is_top=True, is_bottom=False,
                    key=(-1, c.seg, n, 'pin'),
                ))
                n += 1
        return pins

    # ── rubble hearting ──────────────────────────────────────────────────────
    def _extra_parts(self, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> list:
        """A sealed sheet of small rubble stones through the wall body.

        By-construction guarantee for the red-mortar diagnostic: pins
        chase voids case-by-case, but void shapes are unpredictable —
        the hearting covers every (t, z) with overlapping rubble, so
        any gap between face stones shows deeper stones, exactly like
        a real drystone wall's packed core.
        """
        import trimesh as _t
        T, H = self.thickness_mm, self.height_mm
        sb = _RUBBLE_SETBACK_MM
        parts = []
        for seg in segs:
            nt = max(2, int(round(seg.L / _RUBBLE_SPACING_MM)))
            nz = max(2, int(round((H - 3.0) / _RUBBLE_SPACING_MM)))
            for i in range(nt):
                for j in range(nz):
                    # Half-pitch stagger per row (brick bond): aligned
                    # grid crevices can line up with face voids and leak
                    # a red pinhole through both layers.
                    tc = ((i + 0.5 + 0.5 * (j % 2)
                           + rng.uniform(-0.3, 0.3)) * seg.L / nt)
                    zc = 1.2 + (j + 0.5 + rng.uniform(-0.3, 0.3)) \
                        * (H - 3.0) / nz
                    w = rng.uniform(*_RUBBLE_FOOT)
                    h = rng.uniform(*_RUBBLE_H)
                    t0 = np.clip(tc - w / 2.0, 0.3, seg.L - 0.3 - w)
                    body = _fieldstone_mesh(
                        w, T - 2 * sb, h,
                        rng.uniform(0.25, 0.55), False,
                        0.35, 0.0, self.relief_wl, rng,
                        flatten_y=rng.uniform(0.5, 0.8), aged=False)
                    b0, b1 = body.bounds
                    tgt0 = np.array([t0, sb, zc - h / 2.0])
                    tgt1 = np.array([t0 + w, T - sb, zc + h / 2.0])
                    body.apply_translation(-b0)
                    body.apply_scale((tgt1 - tgt0) / (b1 - b0))
                    body.apply_translation(tgt0)
                    m = np.eye(4)
                    m[:2, 0] = seg.d
                    m[:2, 1] = seg.n
                    m[:2, 3] = seg.a
                    m[2, 3] = seat_z
                    body.apply_transform(m)
                    parts.append(body)
        return parts

    # ── the unit ─────────────────────────────────────────────────────────────
    def _unit_mesh(self, lx: float, ly: float, lz: float, chamfer: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        # Same fieldstone unit EVERYWHERE — corners just get squarer
        # stones (round-3: "you've left bricks in the corner").
        if cell.is_quoin:
            blockiness = rng.uniform(*_QUOIN_BLOCKINESS)
        elif cell.is_top:
            blockiness = rng.uniform(*_CAP_BLOCKINESS)
        else:
            blockiness = rng.uniform(*_BLOCKINESS)

        up = 0.0 if cell.is_top else rng.uniform(0.0, _BULGE_MM)
        dn = 0.0 if cell.is_bottom else rng.uniform(0.0, _BULGE_MM)
        # Build OVERSIZE so rounding keeps real curvature, then EXACT-FIT
        # rescale to the target box.  This replaces shave compensation
        # (round-4 fix): guessing the roundover+blur shave either leaves
        # stones flush with the core (flat-slab read) or overshoots and
        # bulges them past the outer face plane, where the tile-boundary
        # clip PLANES them into flat-faced rocks (Shawn's outside find).
        # Exact fit makes every belly tangent to its face plane — round
        # faces by construction, nothing for the clip to shave.
        fy = rng.uniform(*_FLATTEN_Y)
        fx = (rng.uniform(*_FLATTEN_X_END)
              if 'face' in (cell.end0, cell.end1) else 1.0)
        c = _PACK_COMP_MM
        body = _fieldstone_mesh(lx + c, ly + c, lz + up + dn + c,
                                blockiness, cell.is_top or cell.is_quoin,
                                self.roundover_mm, self.relief_mm,
                                self.relief_wl, rng,
                                flatten_y=fy, flatten_x=fx)
        # Target box overlaps joint midlines (stacked stones touch);
        # face/end planes and the cap stay exact.
        ov = self.joint_mm / 2.0 + _OVERLAP_MM
        ox0 = ov if cell.end0 == 'joint' else 0.0
        ox1 = ov if cell.end1 == 'joint' else 0.0
        oz0 = 0.0 if cell.is_bottom else ov
        oz1 = 0.0 if cell.is_top else ov
        b0, b1 = body.bounds
        target0 = np.array([-ox0, 0.0, -dn - oz0])
        target1 = np.array([lx + ox1, ly, lz + up + oz1])
        body.apply_translation(-b0)
        body.apply_scale((target1 - target0) / (b1 - b0))
        body.apply_translation(target0)
        return body
