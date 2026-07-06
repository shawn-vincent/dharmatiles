"""
Faceted stone primitive — gen-1 prototype (E1/E2 of the rocks rework).

Design: docs/design/rocks-faceted-stones.md.  Each stone is the convex
hull of M support points sampled on an anisotropic lumpy ellipsoid, so
every face is planar with a distinct normal — faceted by construction
(R1).  Fibonacci-sphere direction sampling bounds facet size from below
(R3).  A corrective loop clamps lean (then deepens burial) until every
above-ground downward face passes the printable-overhang audit (R4).

This module currently supports explicitly authored stones (StoneSpec)
for the acceptance-scene prototypes; the sampled/clustered placement
machinery (R6) arrives with experiment E4.
"""
from __future__ import annotations

import warnings
from dataclasses import dataclass

import numpy as np
import trimesh

from ..core.grid import sample_grid
from ..core.color import Material, tag as _tag
from ..core.tile import derive_seed
from ..stone import (blur_remesh, clip_to_box, fibonacci_sphere,
                     round_edges, stone_relief, survives_stl32)
from ..stone.cracks import _CRACK_PROUD_MM, engrave_cracks
from .config import Uniform
from .distribute import scatter_positions

# ── Iteration knobs (module constants while prototyping) ─────────────────────
_LUMPINESS        = 0.12    # per-point radius jitter fraction
_DIR_JITTER       = 0.35    # tangent jitter, fraction of Fibonacci spacing
_EGG_WIDEST_T     = -0.25   # egg profile: widest slice at this local height
                            # (t ∈ [-1, 1]) instead of the ellipsoid equator —
                            # bed-to-widest then buries less stone and the
                            # base doesn't flare into tent-flaps
_EGG_TAPER        = 0.35    # horizontal shrink at the very top (t = 1)

# ── Concave weathering bites (spheroidal-weathering morphology) ──────────────
# A convex hull can never show a hollow, and real rocks are defined by their
# concavities: faces retreat non-planar, and exfoliation spalls leave curved
# concave scars with crisp rims, biased to edges/corners.
_DISH_MIN_FACE_MM2 = 5.0          # faces at least this big get dished
_DISH_SAG_FRAC     = (0.03, 0.08) # dish depth × sqrt(face area)
_SPALL_R_FRAC      = (0.22, 0.42) # scar radius × footprint
_SPALL_DEPTH_FRAC  = (0.25, 0.45) # scar depth × scar radius
_SPALL_MIN_FOOT_MM = 3.5          # stones smaller than this get no scars
_UNDULATE_FRAC     = 0.35         # bulge amplitude × roundover: the reference
                                  # boulders are pillowy — broad convex bulges
                                  # flowing into shallow saddles, never flat
                                  # planes (the residual CAD read)
_UNDULATE_FOOT     = 0.03         # extra amplitude per mm of footprint over
                                  # 8 mm — fixed-mm bulges vanish on hero
                                  # stones (E9: reference pillowing is ~5 % of
                                  # diameter, ours was 2 %)

# ── Micro-texture (E10): granular tooth for drybrushing ─────────────────────
# References show grain at ~1–2 % of size; post-Taubin stones were glass.
# The goal is FDM-miniature FEEL, not geology: bumps must survive a 0.4 mm
# nozzle, so grain feature size ~1.3 mm and amplitude 0.1–0.25 mm.
_MICRO_AMP_FRAC   = 0.012   # amplitude = frac × footprint, clamped below
_MICRO_AMP_MM     = (0.10, 0.25)

# ── E11 calm (essence = contrast between quiet fields and sparse incident) ──
_ENV_FLOOR  = 0.2   # patchy envelope: quiet zones keep this fraction of the
                    # undulation+grain amplitude, active zones get 1.0
_FACE_CALM  = 0.85  # displacement damping on the protected hero face —
                    # the one big facet that survives aging as a single tone

# ── E12 facet-edge warp ──────────────────────────────────────────────────────
# Hull arrises are straight lines by construction (planes intersecting), but
# real joint faces are gently CURVED fracture surfaces — reference facet
# boundaries wander.  A low-frequency bend of the subdivided body curves the
# faces and their edge lines together while keeping facet identity readable.
_WARP_FRAC     = 0.04   # bend amplitude × footprint
_WARP_MIN_FOOT = 4.0    # pebbles skip the warp (and the subdivision cost)
_OVERHANG_NZ      = -0.72   # fail when a face normal's z is below this (≈45°)
_OVERHANG_CHORD   = 1.6     # mm — under-tucks narrower than this are allowed
_GROUND_MARGIN    = 1.0     # mm — faces below terrain+margin are exempt from
                            # the overhang audit: the seal/skirt laps soil up
                            # to _MAX_SEAL_LIFT_MM against them, so they are
                            # supported in the print even though they face down
_LEAN_STEP_DEG    = 2.0     # corrective lean reduction per audit round
_BURIAL_STEP      = 0.05    # corrective burial increase per audit round
_BURIAL_MAX       = 1.40
_WIDEST_ZONE      = 0.65    # widest-slice search: bottom fraction of height
_FLOOR_MM         = 0.3     # buried stone bodies are sliced off below this z —
                            # deep bedding must never punch through the tile
_SEAL_LIP_MM      = 0.15    # terrain sealed slightly above the stone underside

# ── E4 cluster-sampling tables (module constants while prototyping) ──────────
_DOM_FOOT_RANGE   = (4.5, 8.5)   # dominant footprint (mm)
_DOM_FOOT_SKEW    = 1.4          # rng.random()**skew — skews toward small
_SEAT_MIN_SEP_MM  = 11.0         # min distance between cluster seats
_CLASS_NAMES      = ('lump', 'slab', 'shard')
_DOM_CLASS_W      = (0.50, 0.28, 0.22)   # dominant-stone class mix
_COMP_CLASS_W     = (0.60, 0.40, 0.0)    # companions are never shards
_COMP_COUNT_W     = (0.20, 0.35, 0.28, 0.17)  # P(0..3 companions)
_COMP_DECAY       = (0.45, 0.75)  # companion footprint / dominant footprint
_COMP_MIN_FOOT_MM = 1.6           # companions below this are dropped
_COMP_DIST        = (0.92, 1.12)  # × (r_dom + r_comp) — touching-ish
_YAW_JITTER_DEG   = 25.0          # companion yaw jitter around the group yaw
_CLASS_PARAMS = {
    #         height/foot    aspect        facets    lean °     burial       egg   roundover mm
    'lump':  ((0.90, 1.30), (0.75, 0.95), (11, 14), (0.0,  6.0), (0.90, 1.05), 0.35, (0.3, 0.9)),
    'slab':  ((0.55, 0.75), (0.80, 0.95), ( 8, 12), (0.0,  5.0), (0.95, 1.10), 0.35, (0.2, 0.7)),
    'shard': ((1.50, 2.10), (0.60, 0.80), (10, 13), (4.0, 14.0), (0.85, 1.00), 0.12, (0.0, 0.25)),
}
_MAX_SEAL_LIFT_MM = 1.3     # don't build soil walls under real overhangs
_MAX_SKIRT_LIFT_MM = 2.4    # the skirt may bank higher than the rim seal —
                            # soil piled against a stone flank reads natural,
                            # and a lower cap leaves arch-shaped voids where
                            # a stone rim overhangs a terrain pocket
_SKIRT_W_MM       = 3.0     # lapping-soil annulus width around the footprint

# Crack engraving lives in stone/cracks.py (shared with StoneFloor).


@dataclass
class StoneSpec:
    """One explicitly authored faceted stone (fully resolved geometry)."""
    x:            float
    y:            float
    footprint_mm: float          # nominal long horizontal diameter
    height_mm:    float          # vertical extent before lean
    aspect:       float = 0.8    # short horizontal diameter / long
    facets:       int   = 10     # M support points — the weathering knob
    yaw_deg:      float = 0.0
    lean_deg:     float = 0.0
    lean_dir_deg: float = 0.0    # world direction the top leans toward
    burial:       float = 0.9    # depth relative to the widest cross-section:
                                 # 1.0 = bedded exactly to the widest line, so
                                 # every visible flank slopes outward into the
                                 # soil (no perched read); <1 shallower, >1 deeper
    egg:          float | None = None  # per-stone egg taper; None → _EGG_TAPER.
                                       # Low for shards (mass stays high),
                                       # high for lumps (mass sits low)
    lumpiness:    float | None = None  # per-point radius jitter; None →
                                       # _LUMPINESS
    roundover_mm: float = 0.0    # THE weathering knob: edges/corners are
                                 # filleted by ~this radius (randomized per
                                 # corner) while the overall shape stays —
                                 # 0 = crisp shard-cut, ~1.5 = river cobble
    spall_scars:  int   = -1     # concave exfoliation scars: -1 = auto by
                                 # size, 0 = none, N = exactly N bites
    crown_flat:   float = 0.0    # 0-1: compress the top support points into
                                 # a plateau — the glacial-erratic loaf has a
                                 # broad flattish top, never an apex point
                                 # (low facet counts put a single Fibonacci
                                 # point at the pole = cone read)
    seam_z:       float | None = None  # 0-1 fraction of exposed height: carve
                                       # one long near-horizontal fracture seam
                                       # wrapping the stone (Letipea's defining
                                       # feature — bedding, not a scar)
    seed:         int   = 0


# ── Stone body ────────────────────────────────────────────────────────────────

def _support_points(spec: StoneSpec) -> np.ndarray:
    """M lumpy support points on the stone's local ellipsoid (z = up)."""
    M   = spec.facets
    rng = np.random.default_rng(spec.seed)
    d   = fibonacci_sphere(M)

    # Tangent jitter, bounded to a fraction of the Fibonacci spacing so the
    # minimum angular separation (facet-size floor, R3) survives.
    spacing = np.sqrt(4.0 / M)
    d += rng.normal(0.0, spacing * _DIR_JITTER / 2.0, d.shape)
    d /= np.linalg.norm(d, axis=1, keepdims=True)

    radii = np.array([spec.footprint_mm / 2.0,
                      spec.footprint_mm * spec.aspect / 2.0,
                      spec.height_mm / 2.0])
    lumpiness = _LUMPINESS if spec.lumpiness is None else spec.lumpiness
    lump = rng.uniform(1.0 - lumpiness, 1.0 + lumpiness, (M, 1))
    pts  = d * radii * lump

    # Egg profile: taper the horizontal radius above _EGG_WIDEST_T so the
    # widest slice sits low on the stone rather than at the equator.
    # Quadratic in the normalized height — girth holds near the widest
    # line and sheds near the top (linear taper reads as a pup-tent).
    egg = _EGG_TAPER if spec.egg is None else spec.egg
    t = d[:, 2]
    u = np.clip(t - _EGG_WIDEST_T, 0.0, None) / (1.0 - _EGG_WIDEST_T)
    s = 1.0 - egg * u * u
    pts[:, 0] *= s
    pts[:, 1] *= s

    if spec.crown_flat > 0.0:
        rz   = spec.height_mm / 2.0
        zcap = rz * (1.0 - 0.4 * spec.crown_flat)
        over = pts[:, 2] > zcap
        pts[over, 2] = zcap + (pts[over, 2] - zcap) * 0.25
    return pts


def _seat_rotation(verts: np.ndarray, faces: np.ndarray) -> np.ndarray:
    """Rotation that lays the stone's best sitting face flat down.

    Stones settle onto a stable face; without this a hull can meet the
    ground along a single edge and read as about to tip over.  The best
    face is the largest one already pointing downward (within ~60°);
    identity if none exists."""
    tris = verts[faces]
    n    = np.cross(tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0])
    area = 0.5 * np.linalg.norm(n, axis=1)
    n    = n / (2.0 * area[:, None] + 1e-12)
    score = area * np.clip(-n[:, 2], 0.0, None)
    score[n[:, 2] > -0.5] = 0.0
    if score.max() <= 0.0:
        return np.eye(3)
    nb = n[int(score.argmax())]
    target = np.array([0.0, 0.0, -1.0])
    axis = np.cross(nb, target)
    s    = np.linalg.norm(axis)
    c    = float(nb @ target)
    # Only CORRECT a nearly-stable stone; a large rotation would lay a
    # planted monolith down on its flank (shards stand because they are
    # bedded, not because they balance).
    if np.degrees(np.arccos(np.clip(c, -1.0, 1.0))) > 32.0:
        return np.eye(3)
    if s < 1e-9:
        return np.eye(3)
    axis /= s
    K = np.array([[0.0, -axis[2], axis[1]],
                  [axis[2], 0.0, -axis[0]],
                  [-axis[1], axis[0], 0.0]])
    return np.eye(3) + s * K + (1.0 - c) * (K @ K)


def _spall_blob(r: float, depth: float, n: np.ndarray,
                rng: np.random.Generator) -> trimesh.Trimesh:
    """Organic spall cutter: a lumpified oblate blob, not a sphere.

    A perfect icosphere bite leaves a circular scallop with a compass-
    drawn rim (Shawn's MeshLab find).  Low-frequency radial noise makes
    the rim wander and the scar floor undulate like a real spall."""
    bite = trimesh.creation.icosphere(subdivisions=3, radius=1.0)
    p = np.asarray(bite.vertices)
    fld = np.zeros(len(p))
    for _ in range(4):
        d = rng.normal(size=3)
        d /= np.linalg.norm(d) + 1e-12
        fld += np.cos(2.0 * np.pi / rng.uniform(0.9, 1.6) * (p @ d)
                      + rng.uniform(0.0, 2.0 * np.pi))
    fld /= 4.0
    bite.vertices = p * (1.0 + 0.5 * fld)[:, None]
    bite.apply_scale([r, r * rng.uniform(0.75, 1.0), depth])
    bite.apply_transform(trimesh.transformations.rotation_matrix(
        rng.uniform(0.0, 2.0 * np.pi), [0.0, 0.0, 1.0]))
    z = np.array([0.0, 0.0, 1.0])
    ax = np.cross(z, n)
    s  = np.linalg.norm(ax)
    if s > 1e-9:
        ang = float(np.arccos(np.clip(z @ n, -1.0, 1.0)))
        bite.apply_transform(
            trimesh.transformations.rotation_matrix(ang, ax / s))
    return bite


def _weather_bites(crisp_v: np.ndarray, crisp_f: np.ndarray,
                   spec: StoneSpec, rng: np.random.Generator,
                   ) -> list[trimesh.Trimesh]:
    """Concave weathering cutters, anchored on the crisp hull.

    Two families (spheroidal-weathering morphology):
    - face dishing: a large sphere pressed shallowly into every big face,
      so no face is glass-flat;
    - spall scars: oblate bites at random corners — the concave curved
      scars with crisp rims that exfoliation shells leave behind.
    """
    m  = trimesh.Trimesh(vertices=crisp_v, faces=crisp_f, process=False)
    fn, areas, tris = m.face_normals, m.area_faces, m.triangles
    cutters: list[trimesh.Trimesh] = []

    # Face dishing scales with the weathering age (roundover): fresh rock
    # keeps glass-flat joint faces (the reference monolith does); only
    # weathered stones get retreated, gently concave faces.
    age = float(np.clip(spec.roundover_mm / 1.0, 0.0, 1.0))
    if age > 0.15:
        _, inv = np.unique(np.round(fn, 2), axis=0, return_inverse=True)
        for g in np.unique(inv):
            sel  = inv == g
            area = float(areas[sel].sum())
            if area < _DISH_MIN_FACE_MM2:
                continue
            n = fn[sel].mean(axis=0)
            n /= np.linalg.norm(n) + 1e-12
            c = (tris[sel].mean(axis=1) * areas[sel, None]).sum(axis=0) / area
            # Dish rim narrower than the face: a full-width dish cuts into
            # the edge fillet and re-sharpens the face-to-face transition
            # (dishes are subtracted AFTER the roundover).
            w   = 0.62 * np.sqrt(area)
            sag = rng.uniform(*_DISH_SAG_FRAC) * w * age
            # Sphere through a rim of width ~w at depth sag.
            R = min((w * w / 4.0 + sag * sag) / (2.0 * sag), 60.0)
            ball = trimesh.creation.icosphere(subdivisions=2, radius=R)
            ball.apply_translation(c + n * (R - sag))
            cutters.append(ball)

    # Scars scale with the EXPOSED height (bed-to-widest hides ~40 %):
    # a mostly-buried stone shows only a crest, and a full scar budget
    # Swiss-cheeses it into an arch.
    exp_h = spec.height_mm * (1.0 - 0.375 * min(spec.burial, 1.2))
    if spec.footprint_mm >= _SPALL_MIN_FOOT_MM and exp_h >= 2.5:
        k = spec.spall_scars
        if k < 0:
            k = int(rng.integers(1, 4)) if spec.footprint_mm > 5.0 \
                else int(rng.integers(0, 2))
        if exp_h < 5.0:
            k = min(k, 1)
        if spec.roundover_mm > 0.15:
            # Aged stones get one CRISP scar cut after the aging pass
            # (E11); these pre-aging bites soften into background dents,
            # so hand one of the budget to the post-aging cut.
            k = max(k - 1, 0)
        vn  = np.asarray(m.vertex_normals)
        # Bite only the upper band: bed-to-widest buries roughly the bottom
        # 40 % of the stone, so lower scars carve arches at the soil line;
        # an apex scar truncates a monolith's tip.
        z0, z1 = crisp_v[:, 2].min(), crisp_v[:, 2].max()
        t = (crisp_v[:, 2] - z0) / max(z1 - z0, 1e-9)
        band = np.flatnonzero((t > 0.55) & (t < 0.88))
        idx = band[rng.permutation(len(band))[:k]] if len(band) else []
        for i in idx:
            n = vn[i]
            r     = min(rng.uniform(*_SPALL_R_FRAC) * spec.footprint_mm / 2.0,
                        0.45 * exp_h)
            depth = rng.uniform(*_SPALL_DEPTH_FRAC) * r
            bite = _spall_blob(r, depth, n, rng)
            bite.apply_translation(crisp_v[i] + n * depth * 0.35)
            cutters.append(bite)
    return cutters


def _rotation(spec: StoneSpec, lean_deg: float) -> np.ndarray:
    yaw = np.radians(spec.yaw_deg)
    cz, sz = np.cos(yaw), np.sin(yaw)
    Rz = np.array([[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]])

    lean = np.radians(lean_deg)
    ld   = np.radians(spec.lean_dir_deg)
    # Rotate about the horizontal axis perpendicular to the lean direction,
    # so the top tips toward lean_dir.
    ax = np.array([-np.sin(ld), np.cos(ld), 0.0])
    K  = np.array([[0.0, -ax[2], ax[1]],
                   [ax[2], 0.0, -ax[0]],
                   [-ax[1], ax[0], 0.0]])
    Rl = np.eye(3) + np.sin(lean) * K + (1.0 - np.cos(lean)) * (K @ K)
    return Rl @ Rz


def _facet_chords(mesh_normals: np.ndarray, tris: np.ndarray) -> np.ndarray:
    """Horizontal extent per face, merged over coplanar triangle groups.

    qhull triangulates planar facets; the printable-chord allowance must
    measure the whole facet, so triangles are grouped by rounded normal.
    Returns one chord value per triangle (its group's extent).
    """
    keys = np.round(mesh_normals, 2)
    chords = np.empty(len(tris))
    _, inverse = np.unique(keys, axis=0, return_inverse=True)
    for g in np.unique(inverse):
        sel = inverse == g
        pts = tris[sel].reshape(-1, 3)[:, :2]
        span = pts.max(axis=0) - pts.min(axis=0)
        chords[sel] = np.hypot(span[0], span[1])
    return chords


def build_stone(spec: StoneSpec, terrain_center_z: float,
                ) -> tuple[np.ndarray, np.ndarray, float, float]:
    """Build one stone; returns (verts, faces, lean_used, burial_used).

    Runs the R4 corrective loop: reduce lean, then deepen burial, until
    the overhang audit passes.  Warns if the budget is exhausted.
    """
    local = trimesh.convex.convex_hull(_support_points(spec))
    v_loc, faces = np.asarray(local.vertices), np.asarray(local.faces)
    # Seat on a stable face before yaw/lean — never balance on an edge.
    v_loc = v_loc @ _seat_rotation(v_loc, faces).T
    # Weathering (fillet + concave bites) BEFORE the bedding loop, so
    # widest-slice, burial, overhang audit, and the terrain seal all see
    # the final geometry — modifying afterwards eats the bottom rim and
    # re-opens daylight under the stone.  Bites are anchored on the crisp
    # hull (rounding preserves the silhouette, so the anchors stay valid).
    # Protected "hero face" (E11): the references' essence is CALM — one
    # big quiet plane that catches light as a single tone (Syltstenen's
    # left face).  Record the largest non-bottom facet of the crisp hull;
    # weathering displacement is damped there so it survives aging.
    _hm = trimesh.Trimesh(vertices=v_loc, faces=faces, process=False)
    _hn, _ha, _ht = _hm.face_normals, _hm.area_faces, _hm.triangles
    _, _hinv = np.unique(np.round(_hn, 2), axis=0, return_inverse=True)
    face_n = face_c = None
    _best = -1.0
    for _g in np.unique(_hinv):
        _sel = _hinv == _g
        _n = _hn[_sel].mean(axis=0)
        _n /= np.linalg.norm(_n) + 1e-12
        if _n[2] < -0.2:          # never protect the underside
            continue
        _a = float(_ha[_sel].sum())
        if _a > _best:
            _best  = _a
            face_n = _n
            face_c = (_ht[_sel].mean(axis=1)
                      * _ha[_sel, None]).sum(axis=0) / _a

    r_rng = np.random.default_rng((spec.seed ^ 0x2CAFE) & 0x7FFFFFFF)
    bites = _weather_bites(v_loc, faces, spec, r_rng)
    # Ball fillet only for LIGHT weathering.  On heavy weathering the
    # Taubin aging dominates the rounding anyway, and the fillet's wobble
    # (± roundover × 1.6) exceeds the bite depths — a bite rim then
    # crosses the bumpy surface several times and strands islands of
    # original stone inside the scar, which aging turns into protruding
    # nodules (Shawn's MeshLab find).  Heavy stones bite the crisp hull:
    # flat faces guarantee a single clean rim loop.
    if 0.0 < spec.roundover_mm <= 0.8:
        v_loc, faces = round_edges(v_loc, faces, spec.roundover_mm, r_rng)
    # The FDM audit runs on the CONVEX body: printability is set by the
    # overall mass, and concave bite interiors self-support (auditing them
    # force-buried the trio monolith to max depth).
    v_conv, f_conv = v_loc, faces

    # Facet-edge warp (E12): bend the whole body with a footprint-scale
    # field so arrises wander like curved fracture intersections instead
    # of ruler lines.  Runs on ALL stones above pebble size — fresh
    # shards otherwise carry dead-straight edges that read cut, not
    # cleaved.  The audit keeps the unwarped convex body (the warp is a
    # few percent and self-supporting).
    if spec.footprint_mm >= _WARP_MIN_FOOT:
        w_rng = np.random.default_rng((spec.seed ^ 0x3AB5) & 0x7FFFFFFF)
        wm = trimesh.Trimesh(vertices=v_loc, faces=faces,
                             process=False).subdivide()
        if spec.footprint_mm >= 10.0:   # heroes need finer edge polylines
            wm = wm.subdivide()
        p = np.asarray(wm.vertices)
        # DOMAIN warp, not normal displacement: each wave displaces along
        # its own constant direction, a smooth deformation of SPACE that
        # is injective while amplitude x frequency < 1 (ours ~0.38) — it
        # cannot fold any surface, however tight the local curvature.
        # Normal-based displacement folded at concave fillet grooves
        # whose radius was smaller than the amplitude (the sliver pleats
        # in Shawn's MeshLab, stage-bisected to this warp).
        disp = np.zeros_like(p)
        for _ in range(3):
            d = w_rng.normal(size=3)
            d /= np.linalg.norm(d) + 1e-12
            wwl = spec.footprint_mm / w_rng.uniform(1.2, 1.8)
            ph  = np.cos(2.0 * np.pi / wwl * (p @ d)
                         + w_rng.uniform(0.0, 2.0 * np.pi))
            disp += d[None, :] * (ph * (_WARP_FRAC * spec.footprint_mm
                                        / 3.0))[:, None]
        v_loc = p + disp
        faces = np.asarray(wm.faces)
        if spec.roundover_mm <= 0.15:
            # Fresh stones get no aging pass, so they take the same
            # stable-mesh remesh with a LIGHTER blur (arrises stay crisp,
            # ~0.2 mm micro-rounding that even fresh cleaved rock has) —
            # Taubin on the warped needle tessellation pleated slivers
            # here exactly like the aged path (field shard, ro=0.05).
            dq = blur_remesh(
                trimesh.Trimesh(vertices=v_loc, faces=faces,
                                process=False),
                spec.footprint_mm, sigma=0.9)
            if dq is None:
                dq = trimesh.Trimesh(vertices=v_loc, faces=faces,
                                     process=False)
                if spec.footprint_mm >= 10.0:
                    dq = dq.subdivide()
                trimesh.smoothing.filter_taubin(dq, iterations=7)
            if dq.is_watertight:
                # Light worn patches even on fresh rock: the warp's
                # smooth curvature flat-shades as regular diamond
                # banding on a regular tessellation; a whisper of the
                # common carve breaks the regularity.
                amp_q = 0.5 * float(np.clip(
                    _MICRO_AMP_FRAC * spec.footprint_mm, *_MICRO_AMP_MM))
                v_loc = stone_relief(dq, w_rng,
                                     scale_mm=max(spec.footprint_mm / 3.0,
                                                  1.5),
                                     carve_mm=amp_q, band=0.5,
                                     dish_mm=0.5 * amp_q)
                faces = np.asarray(dq.faces)

    if bites:
        body = trimesh.Trimesh(vertices=v_loc, faces=faces, process=False)
        try:
            out = trimesh.boolean.difference([body] + bites,
                                             engine='manifold')
            if (len(out.faces) > 0 and out.is_watertight
                    and survives_stl32(out)):
                v_loc = np.asarray(out.vertices)
                faces = np.asarray(out.faces)
            else:
                warnings.warn('weathering bites produced a non-watertight '
                              'stone; left unbitten', RuntimeWarning)
        except Exception as exc:                    # noqa: BLE001
            warnings.warn(f'weathering bites failed: {exc}', RuntimeWarning)

    # Age the whole surface after the chunks are removed (scar/dish rims
    # round over with everything else).  Heavy weathering gets a second
    # subdivision — Taubin can't bend a crease flanked by coarse flat
    # triangles.  Cracks are engraved later and stay crisp by design.
    if spec.roundover_mm > 0.15 and spec.footprint_mm >= _SPALL_MIN_FOOT_MM:
        # Voxel-remesh before smoothing: the fillet hull (and bite-rim
        # triangulations) are full of needle triangles, and Laplacian-
        # family smoothing is unstable on needles — it spikes them into
        # the fold slivers Shawn kept finding.  Marching cubes gives
        # uniform triangles by construction; Taubin is then stable.
        body = trimesh.Trimesh(vertices=v_loc, faces=faces, process=False)
        aged = blur_remesh(body, spec.footprint_mm, sigma=1.2)
        if aged is None:
            aged = body.subdivide()
        # Floor at 16: marching cubes leaves voxel stairsteps (z-contour
        # terraces); anything less leaves them visible on light roundover.
        iters = int(np.clip(2 + 11 * spec.roundover_mm, 16, 30))
        trimesh.smoothing.filter_taubin(aged, iterations=iters)

        # One crisp scar per aged stone (E11), cut BEFORE the relief pass:
        # undulation+grain then texture the scar rim and floor along with
        # everything else, so the rim wanders instead of reading as a
        # compass curve (Shawn's MeshLab find, round two).  Cut after the
        # relief it sat as an untextured geometric dish.
        if spec.spall_scars != 0 and aged.is_watertight:
            sv  = np.asarray(aged.vertices)
            svn = np.asarray(aged.vertex_normals)
            z0, z1 = sv[:, 2].min(), sv[:, 2].max()
            t = (sv[:, 2] - z0) / max(z1 - z0, 1e-9)
            band = (t > 0.55) & (t < 0.88)
            if face_n is not None:
                band &= (svn @ face_n) < 0.7   # keep off the calm face
            idx = np.flatnonzero(band)
            exp_h = spec.height_mm * (1.0 - 0.375 * min(spec.burial, 1.2))
            if len(idx) and exp_h > 2.5:
                i = int(idx[r_rng.integers(0, len(idx))])
                n = svn[i]
                r = min(r_rng.uniform(*_SPALL_R_FRAC)
                        * spec.footprint_mm / 2.0, 0.45 * exp_h)
                depth = r_rng.uniform(*_SPALL_DEPTH_FRAC) * r
                bite = _spall_blob(r, depth, n, r_rng)
                bite.apply_translation(sv[i] + n * depth * 0.35)
                try:
                    out = trimesh.boolean.difference([aged, bite],
                                                     engine='manifold')
                    if (len(out.faces) > 0 and out.is_watertight
                            and out.euler_number == 2):
                        aged = out
                except Exception as exc:            # noqa: BLE001
                    warnings.warn(f'scar cut failed: {exc}', RuntimeWarning)

        if aged.is_watertight:
            # The common stone relief (stone/finish.py, E36): plateau
            # carved into worn recesses + dish, patchy calm/incident
            # envelope, protected hero face kept calm, curvature damp.
            # Depth scales with footprint (E9): fixed-mm features
            # vanish on hero-sized stones.
            u_rng = np.random.default_rng((spec.seed ^ 0x0DDA) & 0x7FFFFFFF)
            amp = (_UNDULATE_FRAC * spec.roundover_mm
                   + _UNDULATE_FOOT * max(spec.footprint_mm - 8.0, 0.0)
                     * min(spec.roundover_mm, 1.0))
            v_loc = stone_relief(
                aged, u_rng,
                scale_mm=max(spec.footprint_mm / 2.4, 2.0),
                carve_mm=max(1.6 * amp, 0.2), band=0.45,
                dish_mm=amp,
                env=(_ENV_FLOOR, spec.footprint_mm),
                hero=(None if face_n is None else
                      (face_n, face_c, _FACE_CALM,
                       0.18 * spec.footprint_mm)))
            faces = np.asarray(aged.faces)

    lean, burial = spec.lean_deg, spec.burial
    while True:
        R  = _rotation(spec, lean)
        v  = v_conv @ R.T
        # Widest horizontal cross-section (searched in the bottom zone so a
        # leaning shard's tip doesn't win): burial is measured against it,
        # so burial ≥ 1 guarantees the visible flanks slope outward into
        # the soil — the bedded read, not the perched one.
        zmin, zmax = v[:, 2].min(), v[:, 2].max()
        zone   = v[:, 2] <= zmin + _WIDEST_ZONE * (zmax - zmin)
        radial = np.hypot(v[:, 0] - v[:, 0].mean(), v[:, 1] - v[:, 1].mean())
        z_wide = v[zone, 2][np.argmax(radial[zone])]
        tz = terrain_center_z - burial * (z_wide - zmin) - zmin
        offset = np.array([spec.x, spec.y, tz])
        v = v + offset

        tris    = v[f_conv]
        e1, e2  = tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0]
        n       = np.cross(e1, e2)
        n      /= np.linalg.norm(n, axis=1, keepdims=True) + 1e-12
        cz      = tris[:, :, 2].mean(axis=1)

        above   = cz > terrain_center_z + _GROUND_MARGIN
        steep   = n[:, 2] < _OVERHANG_NZ
        wide    = _facet_chords(n, tris) > _OVERHANG_CHORD
        bad     = above & steep & wide
        done    = not bad.any()
        if not done:
            if lean > 0.0:
                lean = max(0.0, lean - _LEAN_STEP_DEG)
                continue
            if burial < _BURIAL_MAX:
                burial = min(_BURIAL_MAX, burial + _BURIAL_STEP)
                continue
            warnings.warn(
                f'stone at ({spec.x:.1f},{spec.y:.1f}): '
                f'{int(bad.sum())} overhang faces remain at lean=0, '
                f'burial={burial:.2f}', RuntimeWarning)
        # Ship the bitten body under the audited transform.
        return v_loc @ R.T + offset, faces, lean, burial


# ── Shared build path ─────────────────────────────────────────────────────────

def _build_and_stamp(scene, specs: list[StoneSpec]) -> list[trimesh.Trimesh]:
    """Build all stones big→small, stamp scene fields, return mesh parts."""
    parts = []
    for spec in sorted(specs, key=lambda s: -s.footprint_mm):
        # Seat against the MAX soil height over the footprint, not the
        # centre sample — soil-carpet mounds must never entomb a stone.
        tz0 = _footprint_max_z(scene, spec)
        v, f, _, _ = build_stone(spec, tz0)
        mesh = trimesh.Trimesh(vertices=v, faces=f, process=False)
        if v[:, 2].min() < _FLOOR_MM:
            # Manifold boolean, not slice_mesh_plane: the plane cap fails
            # to triangulate the wiggly boundary loop of an aged/undulated
            # surface and ships an open mesh (letipea hero caught it).
            ext = float(np.abs(v).max()) * 2.0 + 10.0
            box = trimesh.creation.box(
                extents=[ext, ext, ext],
                transform=trimesh.transformations.translation_matrix(
                    [spec.x, spec.y, _FLOOR_MM + ext / 2.0]))
            mesh = clip_to_box(
                mesh, box, f'stone floor at ({spec.x:.1f},{spec.y:.1f})')
        # Stamp from the convex body (the stamp math assumes convexity),
        # then engrave — grooves are too small to matter for support/masks.
        _stamp_stone(scene, mesh)
        mesh = engrave_cracks(
            mesh, np.random.default_rng((spec.seed ^ 0x5EED) & 0x7FFFFFFF),
            ground_z=tz0, footprint_mm=spec.footprint_mm,
            seam_z=spec.seam_z,
            # Proud height must clear the undulation bulges, or the
            # groove chords under a bump and tunnels through it (genus 1).
            proud_mm=_CRACK_PROUD_MM
                     + _UNDULATE_FRAC * spec.roundover_mm)
        parts.append(mesh)

    if not parts:
        return []
    combined = trimesh.util.concatenate(parts)
    _tag(combined, Material.ROCK)
    return [combined]


# ── Layers ────────────────────────────────────────────────────────────────────

class FacetedStones:
    """Direct TileLayer: explicitly authored faceted stones.

    Stamps ``terrain_support_z`` and ``obstacle_mask`` (same contracts as
    ``Rocks``) so grass/tree ordering rules apply unchanged.
    """

    height_default_mm: float = 5.0

    def __init__(self, stones: list[StoneSpec]):
        self.stones = stones

    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        return _build_and_stamp(scene, self.stones)


def _footprint_max_z(scene, spec: StoneSpec) -> float:
    surface = scene.surface
    cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
    r  = spec.footprint_mm / 2.0
    i0 = max(0, int((spec.x - r) / cw));  i1 = min(gw, int((spec.x + r) / cw) + 1)
    j0 = max(0, int((spec.y - r) / cw));  j1 = min(gh, int((spec.y + r) / cw) + 1)
    if i0 >= i1 or j0 >= j1:
        return float(sample_grid(scene.terrain_z, surface,
                                 np.array([spec.x]), np.array([spec.y]))[0])
    X, Y = np.meshgrid(np.arange(i0, i1) * cw, np.arange(j0, j1) * cw)
    disk = (X - spec.x) ** 2 + (Y - spec.y) ** 2 <= r * r
    patch = scene.terrain_z[j0:j1, i0:i1]
    if not disk.any():
        return float(patch.max())
    # High percentile, not max: seating on the single highest mound cell
    # leaves the downslope side of the stone hanging in the air.
    return float(np.percentile(patch[disk], 80.0))

def _stamp_stone(scene, mesh: trimesh.Trimesh) -> None:
    """Stamp one stone into the scene fields (R5 bedding by construction).

    - support_z: stone top = min over upward-face planes (convex body),
      valid inside the 2-D silhouette hull.
    - terrain seal: terrain_z raised to the stone underside (+ lip)
      inside the footprint, so no waterline gap can exist; capped so
      real overhangs keep their shadow instead of growing soil walls.
    - skirt: a lapping-soil annulus blends the sealed contact ring
      back down to the surrounding terrain.
    - obstacle_mask: footprint, unchanged contract for grass/trees.
    """
    surface = scene.surface
    cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
    # Concave bites break the convex plane-field math; stamp from the
    # stone's convex hull instead (over-estimates by the bite depth,
    # which is the safe direction for support/obstacle fields).
    hull = trimesh.convex.convex_hull(mesh.vertices)
    V, F = np.asarray(hull.vertices), np.asarray(hull.faces)
    N    = np.asarray(hull.face_normals)

    pad = int(np.ceil(_SKIRT_W_MM / cw)) + 1
    i0 = max(0, int(np.floor(V[:, 0].min() / cw)) - pad)
    i1 = min(gw, int(np.ceil(V[:, 0].max() / cw)) + 1 + pad)
    j0 = max(0, int(np.floor(V[:, 1].min() / cw)) - pad)
    j1 = min(gh, int(np.ceil(V[:, 1].max() / cw)) + 1 + pad)
    if i0 >= i1 or j0 >= j1:
        return
    X, Y = np.meshgrid(np.arange(i0, i1) * cw, np.arange(j0, j1) * cw)

    # 2-D silhouette inside-test via the hull of projected vertices.
    from scipy.spatial import ConvexHull
    hull2 = ConvexHull(V[:, :2])
    A, b  = hull2.equations[:, :2], hull2.equations[:, 2]
    inside = np.ones(X.shape, dtype=bool)
    for k in range(len(b)):
        inside &= (A[k, 0] * X + A[k, 1] * Y + b[k]) <= 1e-9
    if not inside.any():
        return

    def _plane_field(sel):
        n_s = N[sel]
        d_s = (n_s * V[F[sel][:, 0]]).sum(axis=1)
        return (d_s[:, None, None]
                - n_s[:, 0, None, None] * X
                - n_s[:, 1, None, None] * Y) / n_s[:, 2, None, None]

    up, down = N[:, 2] > 1e-6, N[:, 2] < -1e-6
    if not up.any() or not down.any():
        return
    z_top = _plane_field(up).min(axis=0)     # convex top surface
    z_bot = _plane_field(down).max(axis=0)   # convex underside

    terrain = scene.terrain_z[j0:j1, i0:i1]

    # Seal: raise terrain toward the underside inside the footprint,
    # clamped to a max lift so real overhangs keep their shadow.  The
    # clamp (not a skip) keeps the field continuous — no sawtooth where
    # the lift crosses the cap across soil blobs.  The cap grows with
    # distance INSIDE the silhouette: rim shadows stay shallow, but the
    # stone's interior fills completely — otherwise a neighbour's skirt
    # ridge under the stone leaves a see-through arch (trio lump).
    import scipy.ndimage as ndi
    dist_in = ndi.distance_transform_edt(inside, sampling=cw)
    cap = _MAX_SEAL_LIFT_MM + np.clip((dist_in - 0.8) * 2.5, 0.0, 4.0)
    target = np.minimum(z_bot + _SEAL_LIP_MM, terrain + cap)
    seal   = inside & (target > terrain)
    terrain[seal] = target[seal]

    # Skirt: lap the sealed contact ring outward, smoothstep falloff.
    dist, (jn, in_) = ndi.distance_transform_edt(
        ~inside, sampling=cw, return_indices=True)
    ring = (dist > 0.0) & (dist <= _SKIRT_W_MM)
    if ring.any():
        seal_h = terrain[jn, in_]
        t      = 1.0 - dist / _SKIRT_W_MM
        t      = t * t * (3.0 - 2.0 * t)
        targ_r = terrain + np.clip((seal_h - terrain) * t,
                                   0.0, _MAX_SKIRT_LIFT_MM)
        terrain[ring] = np.maximum(terrain[ring], targ_r[ring])

    sl = scene.terrain_support_z[j0:j1, i0:i1]
    np.maximum(sl, np.where(inside, z_top, -np.inf), out=sl)
    np.maximum(sl, terrain, out=sl)
    scene.obstacle_mask[j0:j1, i0:i1] |= inside


# ── E4: sampled clustered placement ──────────────────────────────────────────

class StoneField:
    """Direct TileLayer: sampled clusters of faceted stones (R6).

    Group-first sampling: seats come from the placement machinery
    (``Uniform``/``Grouped`` both work); each seat gets a dominant stone
    plus 0-3 size-decayed companions at touching distance with a shared
    group yaw.  Loners happen when the companion count rolls 0.  Class
    mix, size ranges, and spacing live in the module-constant tables
    above while the look is iterated.
    """

    height_default_mm: float = 5.0

    def __init__(self, *, placement: Uniform | None = None):
        self.placement = placement or Uniform(count_per_square=2)

    def footprint_mm(self) -> float:
        # Placement margin: the dominant stone's RADIUS is enough to keep
        # stones off the tile seam — the full diameter starves a 1x1 tile
        # of usable seat area.
        return _DOM_FOOT_RANGE[1] / 2.0

    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        rng = np.random.default_rng(
            derive_seed(surface.seed, 'stones-scatter', 0)
            ^ self.placement.seed)

        # Oversample seat candidates (3×), then keep a farthest-point subset:
        # uniform sampling alone loves to dump every cluster in one corner.
        n_sq   = surface.cols * surface.rows
        batches = [scatter_positions(self.placement, n_sq, self.footprint_mm(),
                                     placement_mask, scene, surface, rng)
                   for _ in range(3)]
        target = len(batches[0])
        cands  = [(x, y) for batch in batches for x, y, _gd in batch]

        accepted: list[tuple[float, float]] = cands[:1]
        while len(accepted) < target and len(accepted) < len(cands):
            best, best_d2 = None, -1.0
            for c in cands:
                d2 = min((c[0] - ax) ** 2 + (c[1] - ay) ** 2
                         for ax, ay in accepted)
                if d2 > best_d2:
                    best, best_d2 = c, d2
            if best is None or best_d2 < _SEAT_MIN_SEP_MM ** 2:
                break
            accepted.append(best)

        specs: list[StoneSpec] = []
        for sx, sy in accepted:
            specs.extend(self._sample_cluster(sx, sy, rng,
                                              placement_mask, surface))
        return _build_and_stamp(scene, specs)

    def _sample_cluster(self, sx, sy, rng, placement_mask, surface,
                        ) -> list[StoneSpec]:
        lo, hi = _DOM_FOOT_RANGE
        foot   = lo + (hi - lo) * rng.random() ** _DOM_FOOT_SKEW
        gyaw   = rng.uniform(0.0, 360.0)
        cls    = _CLASS_NAMES[rng.choice(3, p=_DOM_CLASS_W)]
        out    = [self._sample_stone(sx, sy, foot, cls, gyaw, rng)]

        n_comp = int(rng.choice(4, p=_COMP_COUNT_W))
        for _ in range(n_comp):
            cfoot = foot * rng.uniform(*_COMP_DECAY)
            if cfoot < _COMP_MIN_FOOT_MM:
                continue
            ang  = rng.uniform(0.0, 2.0 * np.pi)
            dist = 0.5 * (foot + cfoot) * rng.uniform(*_COMP_DIST)
            cx, cy = sx + np.cos(ang) * dist, sy + np.sin(ang) * dist
            if not self._in_region(cx, cy, cfoot / 2.0,
                                   placement_mask, surface):
                continue
            ccls = _CLASS_NAMES[rng.choice(3, p=_COMP_CLASS_W)]
            cyaw = gyaw + rng.uniform(-_YAW_JITTER_DEG, _YAW_JITTER_DEG)
            out.append(self._sample_stone(cx, cy, cfoot, ccls, cyaw, rng))
        return out

    @staticmethod
    def _sample_stone(x, y, foot, cls, yaw, rng) -> StoneSpec:
        hr, ar, fr, lr, br, egg, rr = _CLASS_PARAMS[cls]
        return StoneSpec(
            x=float(x), y=float(y),
            footprint_mm=float(foot),
            height_mm=float(foot * rng.uniform(*hr)),
            aspect=float(rng.uniform(*ar)),
            facets=int(rng.integers(fr[0], fr[1] + 1)),
            yaw_deg=float(yaw),
            lean_deg=float(rng.uniform(*lr)),
            lean_dir_deg=float(rng.uniform(0.0, 360.0)),
            burial=float(rng.uniform(*br)),
            egg=egg,
            roundover_mm=float(rng.uniform(*rr)),
            seed=int(rng.integers(0, 2**31)),
        )

    @staticmethod
    def _in_region(x, y, margin_mm, placement_mask, surface) -> bool:
        """Companion placement check: inside the tile by its own radius
        (never overhang the tile seam) and inside the region mask."""
        if not (margin_mm <= x <= surface.tile_w - margin_mm
                and margin_mm <= y <= surface.tile_h - margin_mm):
            return False
        cw = surface.cell_w
        i, j = int(x / cw), int(y / cw)
        if not (0 <= i < surface.grid_w and 0 <= j < surface.grid_h):
            return False
        return placement_mask is None or bool(placement_mask[j, i])
