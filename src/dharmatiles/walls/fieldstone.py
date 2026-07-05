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

from ..scatter.stones import _relief_field, _round_edges
from .masonry import CutStoneWall, _Cell, _Seg, _RELIEF_WAVES

# ── Iteration knobs (module constants while prototyping) ─────────────────────
# Crack network
_WOBBLE_AMP_MM    = (0.14, 0.32)  # per-sine bed wobble amplitude (2 sines).
                                  # Total ≤ 0.64: two INDEPENDENT beds can
                                  # close on each other by twice that, and
                                  # the thinnest stones (split pairs of a
                                  # 5.5 mm course at 2.8–7.5 mm courses)
                                  # are ~2.5 mm — the beds must never cross
_WOBBLE_WL_MM     = (14.0, 40.0)  # bed wobble wavelengths
_WOBBLE_TAPER_MM  = 12.0          # wobble fades to 0 within this of a
                                  # segment end: corners/butt ends pack
                                  # straight and tight
_DRIFT_MM         = 2.2           # head joints slant up to ±this across
                                  # their course (staggered, never aligned)
# Stone body (E15: no belly/pillow, no relief — "just the cracks and
# the roundovers".  Stones are straight extruded prisms; every corner
# is rounded: outline corners by the 2D buffer, face↔side edges by a
# circular-arc inset over the first/last roundover_mm of depth)
_PROUD_MM         = (0.10, 0.50)  # per-stone face recession (both faces)
_PROUD_DEEP_PROB  = 0.12          # a rare stone sits notably deeper —
_PROUD_DEEP_MM    = (0.70, 1.10)  # the odd deep stone the references show
_RING_STEP_MM     = 1.2           # outline densify spacing
_ARC_T            = (0.0, 0.29, 0.71, 1.0)   # roundover arc stations
                                             # (cosine-spaced fractions
                                             # of the roundover radius)
# Cell topology
_THROUGH_FRAC     = 0.20          # fraction of eligible cells merged with
                                  # the cell above into a throughstone
_SPLIT_H_PROB     = 0.28          # tall cell → two stacked thinner stones
_SPLIT_H_MIN_MM   = 5.5           # eligible cell height for an h-split
# Rubble hearting (E10 guarantee, unchanged)
_RUBBLE_SPACING_MM = 4.2
_RUBBLE_FOOT      = (8.5, 11.0)
_RUBBLE_H         = (5.5, 7.5)
_RUBBLE_SETBACK_MM = 1.6
# Hull-stone shape (rubble only)
_LUMP             = (0.90, 1.0)
_DIR_JITTER       = 0.12
# Cell-key tags (ints, not strings: _place_block hashes cell.key for the
# per-stone rng and str hashes vary per process).  Base keys are
# (course, seg, bay), so tagged keys are longer, never equal.
_K_THROUGH = 9
_K_SPLIT_B, _K_SPLIT_T = 1, 2


def _earcut(pts: np.ndarray) -> np.ndarray:
    """Ear-clipping triangulation of a simple CCW polygon (n, 2).

    Dependency-free stand-in for mapbox_earcut (not installed): the cap
    triangulation must follow the polygon interior — a centroid fan
    makes long radial triangles whose interpolated normals streak under
    smooth shading."""
    n = len(pts)
    idx = list(range(n))
    tris: list[tuple[int, int, int]] = []

    def area2(a, b, c):
        return ((b[0] - a[0]) * (c[1] - a[1])
                - (b[1] - a[1]) * (c[0] - a[0]))

    guard = 0
    while len(idx) > 3 and guard < 100 * n:
        guard += 1
        m = len(idx)
        clipped = False
        for k in range(m):
            i0, i1, i2 = idx[k - 1], idx[k], idx[(k + 1) % m]
            a, b, c = pts[i0], pts[i1], pts[i2]
            if area2(a, b, c) <= 1e-12:          # reflex / degenerate
                continue
            ok = True
            for j in idx:
                if j in (i0, i1, i2):
                    continue
                p = pts[j]
                if (area2(a, b, p) >= -1e-12
                        and area2(b, c, p) >= -1e-12
                        and area2(c, a, p) >= -1e-12):
                    ok = False
                    break
            if ok:
                tris.append((i0, i1, i2))
                idx.pop(k)
                clipped = True
                break
        if not clipped:
            break                                # numeric dead end: fan
    if len(idx) == 3:
        tris.append(tuple(idx))
    else:
        for k in range(1, len(idx) - 1):
            tris.append((idx[0], idx[k], idx[k + 1]))
    return np.asarray(tris, dtype=int)


def _rubble_mesh(lx: float, ly: float, lz: float,
                 rng: np.random.Generator) -> trimesh.Trimesh:
    """Cheap hull stone for the hearting: reads through cracks a
    millimetre behind the faces — no remesh/relief for ~100 background
    stones."""
    M = int(rng.integers(13, 18))
    i = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta), np.cos(phi)], axis=1)
    d += rng.normal(0.0, _DIR_JITTER, d.shape)
    d[:, 1] *= rng.uniform(0.5, 0.8)
    d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-12
    half = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
    blockiness = rng.uniform(0.25, 0.55)
    r_ell = 1.0 / np.sqrt(((d / half) ** 2).sum(axis=1))
    r_box = 1.0 / (np.abs(d) / half).max(axis=1)
    r = ((1.0 - blockiness) * r_ell + blockiness * r_box)
    r *= rng.uniform(*_LUMP, M)
    p = d * r[:, None] + half
    hull = trimesh.convex.convex_hull(p)
    v, f = _round_edges(np.asarray(hull.vertices), np.asarray(hull.faces),
                        0.35, rng)
    return trimesh.Trimesh(vertices=v, faces=f, process=False)


class FieldstoneWall(CutStoneWall):
    """Direct TileLayer: a drystone fieldstone wall on a plan spine.

    Same spine convention and contracts as :class:`CutStoneWall`; the
    stones are crack-network tessellated (module docstring).
    """

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (2.8, 7.5),
                 bay_mm:    tuple[float, float] = (5.0, 16.0),
                 joint_mm:  float = 0.1,   # physical crack gap (Shawn E14:
                                           # nonzero but hairline).  The
                                           # VISIBLE crack stays wider: the
                                           # edge roundover recedes each
                                           # stone by roundover_mm at the
                                           # face plane, so the face shows
                                           # a dark V-groove that closes to
                                           # this gap at roundover depth
                 reveal_mm: float = 2.8,
                 roundover_mm: float | None = 0.42,
                 relief_mm:    float | None = 0.0,
                 relief_wl:    tuple[float, float] | None = (3.0, 9.0),
                 min_bond_mm:  float = 1.8,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         roundover_mm=roundover_mm, relief_mm=relief_mm,
                         relief_wl=relief_wl, min_bond_mm=min_bond_mm,
                         **kwargs)
        self._beds:   dict[tuple, tuple] = {}   # (seg, zkey) → sine params
        self._drifts: dict[tuple, float] = {}   # (seg, course, tkey) → drift

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
            amp = rng.uniform(*_WOBBLE_AMP_MM, 2)
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

    def _drift(self, seg_i: int, course: int, t: float) -> float:
        """Slant of the head joint at cut position t in this course."""
        key = (seg_i, course, int(round(t * 100.0)))
        if key not in self._drifts:
            rng = self._feature_rng(11, *key)
            self._drifts[key] = float(rng.uniform(-_DRIFT_MM, _DRIFT_MM))
        return self._drifts[key]

    # ── cell topology ────────────────────────────────────────────────────────
    def _cells(self, segs: list[_Seg], T: float, H: float,
               rng: np.random.Generator) -> list[_Cell]:
        self._beds.clear()
        self._drifts.clear()
        cells = self._merge_throughstones(super()._cells(segs, T, H, rng),
                                          rng)
        return self._split_cells(cells, rng)

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
                        or min(c.t1, a.t1) - max(c.t0, a.t0) < 3.0):
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
    @staticmethod
    def _cap_triangulation(ring: np.ndarray, poly) -> tuple:
        """Well-shaped triangulation of the face polygon: Delaunay over
        the ring plus a ~1.4 mm interior grid (clear of the boundary so
        ring edges stay locally Delaunay and the cap seams onto the
        loft), triangles filtered to the polygon interior."""
        import scipy.spatial as _ss
        import shapely as _sh
        inner = poly.buffer(-1.0)
        pts = np.empty((0, 2))
        if not inner.is_empty:
            x0, z0, x1, z1 = inner.bounds
            xs = np.arange(x0, x1 + 1e-6, 1.4)
            zs = np.arange(z0, z1 + 1e-6, 1.4)
            if len(xs) and len(zs):
                gx, gz = np.meshgrid(xs, zs)
                cand = np.column_stack([gx.ravel(), gz.ravel()])
                keep = _sh.contains_xy(inner, cand[:, 0], cand[:, 1])
                pts = cand[keep]
        allp = np.vstack([ring, pts]) if len(pts) else ring
        tri = _ss.Delaunay(allp)
        cent = allp[tri.simplices].mean(axis=1)
        keep = _sh.contains_xy(poly, cent[:, 0], cent[:, 1])
        return pts, tri.simplices[keep]

    def _outline(self, cell: _Cell, seg: _Seg, seg_i: int,
                 ) -> np.ndarray:
        """Face outline polygon (t, z) bounded by the cell's cracks.
        Every edge — including every side ENDPOINT — evaluates curves
        shared with the neighbour across it, so the tessellation has no
        overlaps and no wedge voids anywhere, by construction."""
        bands = getattr(cell, 'side_bands',
                        [(cell.key[0], cell.z0, cell.z1)])

        def zeff(z: float, t: float) -> float:
            """Actual (wobbled) height of the bed line at t."""
            if cell.is_bottom and z <= cell.z0 + 1e-9:
                return -self.embed_mm          # flat buried seat
            if cell.is_top and z >= cell.z1 - 1e-9:
                return z                       # flat cap plane (R6)
            return z + float(self._bed(seg_i, z, seg.L)(t))

        def side(t_cut: float, end: str):
            """[(t, z), …] bottom→top along this side crack.  Interior
            band boundaries contribute two points (the drift changes
            where the neighbour changes) — a small shared jog."""
            if end == 'face':                  # wall end / corner arris
                return [(t_cut, zeff(cell.z0, t_cut)),
                        (t_cut, zeff(cell.z1, t_cut))]
            pts = []
            for crs, zb0, zb1 in bands:
                d = self._drift(seg_i, crs, t_cut)
                zm = (zb0 + zb1) / 2.0
                for zb in (zb0, zb1):
                    t = t_cut + d * (zb - zm) / (zb1 - zb0)
                    pts.append((t, zeff(zb, t)))
            return pts

        left  = side(cell.t0, cell.end0)
        right = side(cell.t1, cell.end1)

        def bed(z: float, ta: float, tb: float):
            n = max(2, int(abs(tb - ta) / 4.0) + 2)
            ts = np.linspace(ta, tb, n)
            return [(t, zeff(z, t)) for t in ts]

        bot = bed(cell.z0, left[0][0], right[0][0])
        top = bed(cell.z1, right[-1][0], left[-1][0])

        # CCW: bottom left→right, right side up, top right→left, left
        # down.  Slices drop the duplicated corner points (bed edges
        # include both endpoints, which coincide with the side ends).
        pts = (bot + right[1:] + top[1:] + left[::-1][1:-1])
        return np.asarray(pts, dtype=float)

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh | None:
        import shapely
        import shapely.geometry as sgeom

        seg = segs[cell.seg]
        brng = np.random.default_rng(
            (self.seed * 1_000_003 + hash(cell.key)) & 0x7FFFFFFF)
        outline = self._outline(cell, seg, cell.seg)

        # Inset by half the crack, round the corners (negative-positive
        # buffer): the crack is a real thin gap by construction.
        r = self.roundover_mm
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

        # Straight prism through the thickness (E15: pillowing dropped);
        # face↔side edges get a circular-arc roundover: within the
        # first/last r of depth the outline insets along its 2D outward
        # vertex normals by d(e) = r − sqrt(2re − e²).  No whole-stone
        # rotation — it would open wedges; per-stone tone comes from
        # proudness alone.
        def recess():
            if brng.random() < _PROUD_DEEP_PROB:
                return brng.uniform(*_PROUD_DEEP_MM)
            return brng.uniform(*_PROUD_MM)
        y0, y1 = recess(), self.thickness_mm - recess()

        tang = np.roll(ring, -1, axis=0) - np.roll(ring, 1, axis=0)
        nrm = np.column_stack([tang[:, 1], -tang[:, 0]])       # CCW → out
        nrm /= np.linalg.norm(nrm, axis=1, keepdims=True) + 1e-12

        r3 = self.roundover_mm
        e_front = [t * r3 for t in _ARC_T]
        stations = ([y0 + e for e in e_front]
                    + [(y0 + y1) / 2.0]
                    + [y1 - e for e in reversed(e_front)])
        K = len(stations)
        verts = []
        for y in stations:
            e = max(min(y - y0, y1 - y), 0.0)
            d = r3 - np.sqrt(max(2.0 * r3 * e - e * e, 0.0)) if e < r3 \
                else 0.0
            rk = ring - nrm * d
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
        # End caps.  Well-shaped interior triangles matter: the face is
        # the most visible surface, and skinny cap triangles streak
        # under smooth shading once relief displaces their vertices
        # (fan caps and my sequential earcut both showed this; the
        # blur-remesh alternative terraces — near-planar faces lie
        # almost parallel to a voxel plane family, marching cubes'
        # worst case).  Delaunay over ring + interior grid, triangles
        # filtered to the polygon; earcut fallback if the seam fails.
        # Front and back stations carry the SAME scale (belly = 0 at
        # both ends), so one triangulation serves both caps.
        extras, capf = self._cap_triangulation(ring, poly)
        m = len(extras)
        if m:
            verts.append(np.column_stack(
                [extras[:, 0], np.full(m, y0), extras[:, 1]]))
            verts.append(np.column_stack(
                [extras[:, 0], np.full(m, y1), extras[:, 1]]))
        v = np.vstack(verts)
        mapF = np.concatenate([np.arange(n), K * n + np.arange(m)])
        mapB = np.concatenate([(K - 1) * n + np.arange(n),
                               K * n + m + np.arange(m)])
        faces.append(mapF[capf][:, ::-1])                 # front (y0)
        faces.append(mapB[capf])                          # back  (y1)
        f = np.vstack(faces)
        body = trimesh.Trimesh(vertices=v, faces=f, process=False)
        if not body.is_watertight:
            capf = _earcut(ring)
            faces = faces[:-2]
            faces.append(capf[:, ::-1])
            faces.append(capf + (K - 1) * n)
            body = trimesh.Trimesh(vertices=v[:K * n],
                                   faces=np.vstack(faces), process=False)
        trimesh.repair.fix_normals(body)

        # Optional relief (off by default since E15 — crisp prisms with
        # roundovers only): subdivide for sampling density, displace,
        # then light Taubin to soften the plane-wave crests, whose
        # organised interference pattern reads clearly on flat faces.
        # No voxel remesh: the fitted silhouette must not shrink or
        # soften (the E7 shave lesson).
        if self.relief_mm > 0.0:
            body = body.subdivide()
            disp = self.relief_mm * _relief_field(
                body.vertices, brng, _RELIEF_WAVES, *self.relief_wl)
            body = trimesh.Trimesh(
                vertices=(body.vertices
                          + np.asarray(body.vertex_normals)
                          * disp[:, None]),
                faces=body.faces, process=False)
            trimesh.smoothing.filter_taubin(body, iterations=4)

        m = np.eye(4)
        m[:2, 0] = seg.d
        m[:2, 1] = seg.n
        m[:2, 3] = seg.a
        m[2, 3] = seat_z
        body.apply_transform(m)
        return body

    # ── rubble hearting ──────────────────────────────────────────────────────
    def _extra_parts(self, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> list:
        """A sealed sheet of small rubble stones through the wall body
        (E10): every crack shows deeper stones, never the core plane.
        Two y-layers, half-pitch staggered in t and z; the rubble honours
        the face setback in t too — segment END planes are visible faces
        (free ends and the corner arris)."""
        T, H = self.thickness_mm, self.height_mm
        sb = _RUBBLE_SETBACK_MM
        y_bands = [(sb, 0.55 * T), (0.45 * T, T - sb)]
        parts = []
        for seg in segs:
            nt = max(2, int(round(seg.L / _RUBBLE_SPACING_MM)) + 1)
            nz = max(2, int(round(H / _RUBBLE_SPACING_MM)) + 1)
            for layer, (yb0, yb1) in enumerate(y_bands):
                off = 0.5 * layer
                for i in range(nt):
                    for j in range(nz):
                        tc = ((i + 0.5 * (j % 2) + off
                               + rng.uniform(-0.25, 0.25))
                              * seg.L / (nt - 1))
                        zc = ((j + off + rng.uniform(-0.25, 0.25))
                              * H / (nz - 1))
                        w = rng.uniform(*_RUBBLE_FOOT)
                        h = rng.uniform(*_RUBBLE_H)
                        t0 = np.clip(tc - w / 2.0, sb, seg.L - sb - w)
                        z0 = np.clip(zc - h / 2.0, 0.2, H - 0.6 - h)
                        body = _rubble_mesh(w, yb1 - yb0, h, rng)
                        b0, b1 = body.bounds
                        tgt0 = np.array([t0, yb0, z0])
                        tgt1 = np.array([t0 + w, yb1, z0 + h])
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
