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
_ROUND_JITTER     = (0.3, 1.6)    # roundover randomization range —
                                  # uniform fillets read CNC, not geology
_ROUND_EDGE_STEP_MM = 1.2         # ball spacing along edges: the radius is
                                  # re-rolled at each sample, so the fillet
                                  # wobbles ALONG an edge (per-corner-only
                                  # radii read as rounded dice)

# ── Concave weathering bites (spheroidal-weathering morphology) ──────────────
# A convex hull can never show a hollow, and real rocks are defined by their
# concavities: faces retreat non-planar, and exfoliation spalls leave curved
# concave scars with crisp rims, biased to edges/corners.
_DISH_MIN_FACE_MM2 = 5.0          # faces at least this big get dished
_DISH_SAG_FRAC     = (0.05, 0.13) # dish depth × sqrt(face area)
_SPALL_R_FRAC      = (0.22, 0.42) # scar radius × footprint
_SPALL_DEPTH_FRAC  = (0.25, 0.45) # scar depth × scar radius
_SPALL_MIN_FOOT_MM = 3.5          # stones smaller than this get no scars
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
    #         height/foot    aspect        facets    lean °     burial       egg
    'lump':  ((0.90, 1.30), (0.75, 0.95), (11, 14), (0.0,  6.0), (0.90, 1.05), 0.35),
    'slab':  ((0.55, 0.75), (0.80, 0.95), ( 8, 12), (0.0,  5.0), (0.95, 1.10), 0.35),
    'shard': ((1.50, 2.10), (0.60, 0.80), (10, 13), (4.0, 14.0), (0.85, 1.00), 0.12),
}
_MAX_SEAL_LIFT_MM = 1.3     # don't build soil walls under real overhangs
_MAX_SKIRT_LIFT_MM = 2.4    # the skirt may bank higher than the rim seal —
                            # soil piled against a stone flank reads natural,
                            # and a lower cap leaves arch-shaped voids where
                            # a stone rim overhangs a terrain pocket
_SKIRT_W_MM       = 3.0     # lapping-soil annulus width around the footprint

# ── E5 crack engraving (R11) ──────────────────────────────────────────────────
# A crack is a surface-projected random walk of thin tapered wedges: long,
# meandering, fading out at both ends.  Proportions matter — a wide short
# groove reads as a router slot, not a crack (Shawn, 2026-07-03).
_CRACK_PROB         = 0.5   # chance a SECONDARY crack appears; the primary
                            # crack is unconditional (a big stone must never
                            # roll itself bald — seed 3 taught us)
_CRACK_MAX_PER_STONE = 2
_CRACK_WIDTH_MM     = 0.5   # groove width at the crack's midpoint
_CRACK_DEPTH_MM     = 0.55  # groove depth at the crack's midpoint
_CRACK_PROUD_MM     = 0.55  # wedge top floats this far outside the surface —
                            # must exceed the worst chord dip when a segment
                            # crosses an arris, or the groove tunnels under
                            # the corner and leaves a stone flap bridging it
_CRACK_SEGS         = 9     # random-walk segments per crack
_CRACK_STEP_MM      = (0.7, 1.3)   # length of each walk segment — short
                            # steps hug corners and wiggle more per mm
_CRACK_JITTER_DEG   = 30.0  # per-segment heading jitter (meander)
_CRACK_BRANCH_PROB  = 1.0   # the primary crack always forks once (Shawn:
                            # "I'd expect a tiny bit of branching")
_CRACK_BRANCH_DEG   = (35.0, 60.0)  # fork angle off the parent heading


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
    seed:         int   = 0


# ── Stone body ────────────────────────────────────────────────────────────────

def _support_points(spec: StoneSpec) -> np.ndarray:
    """M lumpy support points on the stone's local ellipsoid (z = up)."""
    M   = spec.facets
    rng = np.random.default_rng(spec.seed)

    i     = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)           # polar
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i        # golden-angle azimuth
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta),
                  np.cos(phi)], axis=1)

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


def _round_edges(verts: np.ndarray, faces: np.ndarray,
                 roundover_mm: float, rng: np.random.Generator,
                 ) -> tuple[np.ndarray, np.ndarray]:
    """Weathering fillet: a rolling ball whose radius is re-rolled at every
    corner AND at ~1 mm intervals along every edge, eroded inward and
    hulled.  The silhouette stays; the fillet radius wobbles along each
    edge — one edge can stay sharp at one end and round over in the middle
    (constant-radius fillets read as rounded dice, per Shawn)."""
    m   = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    cap = 0.3 * float(m.extents.min())

    cores, rhos = [], []
    # Corner balls.
    vn = np.asarray(m.vertex_normals)
    rv = np.minimum(roundover_mm * rng.uniform(*_ROUND_JITTER, len(verts)),
                    cap)
    cores.append(verts - vn * rv[:, None])
    rhos.append(rv)
    # Edge balls, radius independently re-rolled per sample.
    fn = m.face_normals
    for (f1, f2), (a, b) in zip(m.face_adjacency, m.face_adjacency_edges):
        va, vb = verts[a], verts[b]
        L = float(np.linalg.norm(vb - va))
        k = int(L / _ROUND_EDGE_STEP_MM)
        if k < 1:
            continue
        nd = fn[f1] + fn[f2]
        nd = nd / (np.linalg.norm(nd) + 1e-12)
        ts = (np.arange(1, k + 1) + rng.uniform(-0.3, 0.3, k)) / (k + 1)
        ts = np.clip(ts, 0.08, 0.92)
        p  = va[None, :] + ts[:, None] * (vb - va)[None, :]
        r  = np.minimum(roundover_mm * rng.uniform(*_ROUND_JITTER, k), cap)
        cores.append(p - nd[None, :] * r[:, None])
        rhos.append(r)

    core = np.vstack(cores)
    rho  = np.concatenate(rhos)
    # subdivisions=2 (162 dirs): coarser balls turn fillets into single
    # chamfer strips — reads as a bevelled box, not weathering.
    dirs = trimesh.creation.icosphere(subdivisions=2, radius=1.0).vertices
    pts  = (core[:, None, :] + rho[:, None, None] * dirs[None, :, :])
    h = trimesh.convex.convex_hull(pts.reshape(-1, 3))
    return np.asarray(h.vertices), np.asarray(h.faces)


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
            w   = np.sqrt(area)
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
            bite = trimesh.creation.icosphere(subdivisions=2, radius=1.0)
            bite.apply_scale([r, r, depth])
            # Orient the oblate axis along the corner normal, sink partway.
            z = np.array([0.0, 0.0, 1.0])
            ax = np.cross(z, n); s = np.linalg.norm(ax)
            if s > 1e-9:
                ang = float(np.arccos(np.clip(z @ n, -1.0, 1.0)))
                bite.apply_transform(
                    trimesh.transformations.rotation_matrix(ang, ax / s))
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
    r_rng = np.random.default_rng((spec.seed ^ 0x2CAFE) & 0x7FFFFFFF)
    bites = _weather_bites(v_loc, faces, spec, r_rng)
    if spec.roundover_mm > 0.0:
        v_loc, faces = _round_edges(v_loc, faces, spec.roundover_mm, r_rng)
    # The FDM audit runs on the CONVEX body: printability is set by the
    # overall mass, and concave bite interiors self-support (auditing them
    # force-buried the trio monolith to max depth).
    v_conv, f_conv = v_loc, faces
    if bites:
        body = trimesh.Trimesh(vertices=v_loc, faces=faces, process=False)
        try:
            out = trimesh.boolean.difference([body] + bites,
                                             engine='manifold')
            v32 = out.vertices.astype(np.float32).astype(np.float64)
            chk = trimesh.Trimesh(vertices=v32, faces=out.faces.copy(),
                                  process=True)
            if len(out.faces) > 0 and out.is_watertight and chk.is_watertight:
                # Age the WHOLE surface after the chunks are removed: on a
                # weathered stone the scar and dish rims must round over
                # too — cutting them after the fillet left crisp rims on
                # the most weathered stones (Shawn).  Taubin smoothing on
                # the subdivided bitten mesh; fresh stones (roundover 0)
                # keep crisp spall rims, which is the correct geology.
                if spec.roundover_mm > 0.15:
                    out = out.subdivide()
                    iters = int(np.clip(2 + 8 * spec.roundover_mm / 1.5,
                                        2, 12))
                    trimesh.smoothing.filter_taubin(out, iterations=iters)
                v_loc = np.asarray(out.vertices)
                faces = np.asarray(out.faces)
            else:
                warnings.warn('weathering bites produced a non-watertight '
                              'stone; left unbitten', RuntimeWarning)
        except Exception as exc:                    # noqa: BLE001
            warnings.warn(f'weathering bites failed: {exc}', RuntimeWarning)

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


# ── Crack engraving ───────────────────────────────────────────────────────────

def _crack_solid(pts: list, nms: list, widths: np.ndarray,
                 depths: np.ndarray) -> trimesh.Trimesh:
    """ONE lofted V-channel along the crack polyline.

    A single swept solid replaces the old chain of overlapping frusta —
    mutually intersecting cutters made the manifold boolean emit sliver
    triangles that collapsed under STL float32 quantization and opened
    the mesh.  Ring per station: two proud top corners + the apex."""
    K = len(pts)
    rings = []
    for k in range(K):
        prev_p = pts[max(k - 1, 0)]
        next_p = pts[min(k + 1, K - 1)]
        d = np.asarray(next_p) - np.asarray(prev_p)
        d = d / (np.linalg.norm(d) + 1e-12)
        n = nms[k]
        side = np.cross(d, n)
        side = side / (np.linalg.norm(side) + 1e-12)
        p = np.asarray(pts[k])
        rings.append([p + n * _CRACK_PROUD_MM + side * (widths[k] / 2.0),
                      p + n * _CRACK_PROUD_MM - side * (widths[k] / 2.0),
                      p - n * depths[k]])
    verts = np.array([v for ring in rings for v in ring])
    faces = []
    for k in range(K - 1):
        a = 3 * k
        for i in range(3):
            j = (i + 1) % 3
            faces += [[a + i, a + 3 + i, a + j],
                      [a + j, a + 3 + i, a + 3 + j]]
    faces += [[0, 2, 1], [3 * (K - 1), 3 * (K - 1) + 1, 3 * (K - 1) + 2]]
    m = trimesh.Trimesh(vertices=verts, faces=np.array(faces), process=False)
    m.fix_normals()
    return m


def _crack_walk(mesh: trimesh.Trimesh, N: np.ndarray,
                p0: np.ndarray, n0: np.ndarray, d0: np.ndarray,
                n_segs: int, rng: np.random.Generator,
                ground_z: float) -> tuple[list, list]:
    """Random-walk *n_segs* steps across the hull surface from *p0*.

    Steps in the local tangent plane with heading jitter, reprojecting
    each point onto the mesh.  Returns the walked points and their local
    face normals (excluding the start point)."""
    pts, nms, d = [], [], d0
    p, n = p0, n0
    for _ in range(n_segs):
        step = rng.uniform(*_CRACK_STEP_MM)
        jit  = np.radians(rng.uniform(-_CRACK_JITTER_DEG, _CRACK_JITTER_DEG))
        d = np.cos(jit) * d + np.sin(jit) * np.cross(n, d)
        q = p + d * step
        qs, _dist, tid = trimesh.proximity.closest_point(mesh, [q])
        n_new = N[tid[0]]
        # Stop at strong arrises: wrapping a sharp corner leaves uncut
        # stone slivers standing in the groove (and real cracks terminate
        # at hard edges anyway).  Stop before the soil line too — a groove
        # descending into the skirt reads as an arch-shaped hole.
        if float(n_new @ n) < 0.6 or qs[0][2] < ground_z + 0.6:
            break
        p, n = qs[0], n_new
        pts.append(p)
        nms.append(n)
    return pts, nms


def _engrave_cracks(mesh: trimesh.Trimesh, rng: np.random.Generator,
                    ground_z: float, footprint_mm: float) -> trimesh.Trimesh:
    """Subtract kinked wedge grooves seeded on prominent triangles.

    Cracks are a wash-paintability feature (R11); they meander with
    jitter and may cross arrises, like the reference outcrops.  Seeding
    is per-triangle (concave weathering bites fragment coplanar facet
    groups): score = area x upward-visibility, seeds kept apart, faces
    below *ground_z* skipped.  Stones under _SPALL_MIN_FOOT_MM x1.4 get
    no cracks.  Falls back to the uncracked stone if the boolean fails.
    """
    if footprint_mm < 5.0:
        return mesh
    exposed = float(mesh.vertices[:, 2].max()) - ground_z
    if exposed < 3.0:
        return mesh
    n_cracks = 1 if exposed < 5.0 else _CRACK_MAX_PER_STONE
    N     = mesh.face_normals
    tris  = mesh.triangles
    areas = mesh.area_faces

    cz    = tris[:, :, 2].mean(axis=1)
    score = areas * (0.4 + np.clip(N[:, 2], 0.0, None))
    score[(cz < ground_z + 0.5) | (N[:, 2] < -0.3)] = 0.0
    order = np.argsort(-score)

    seeds: list[int] = []
    for i in order:
        if score[i] <= 0.0 or len(seeds) >= n_cracks:
            break
        c_i = tris[i].mean(axis=0)
        if any(np.linalg.norm(tris[j].mean(axis=0)[:2] - c_i[:2]) < 3.0
               for j in seeds):
            continue
        seeds.append(int(i))

    cutters = []
    for idx, i in enumerate(seeds):
        is_primary = idx == 0
        if not is_primary and rng.random() > _CRACK_PROB:
            continue
        n = N[i]
        c = tris[i].mean(axis=0)

        # Heading: horizontal-ish in the face plane (stratification read).
        h = np.cross(n, np.array([0.0, 0.0, 1.0]))
        hn = np.linalg.norm(h)
        if hn < 0.3:                      # near-horizontal face: any dir works
            h = np.cross(n, np.array([1.0, 0.0, 0.0]))
            hn = np.linalg.norm(h)
        t1 = h / (hn + 1e-12)
        ang  = np.radians(rng.uniform(-30.0, 30.0))
        dirv = np.cos(ang) * t1 + np.sin(ang) * np.cross(n, t1)

        # Random-walk the crack path across the surface, both directions
        # from the seed so the crack straddles it.
        seed_p, seed_n = c + dirv * 0.1, n
        back_p, back_n = _crack_walk(mesh, N, seed_p, seed_n, -dirv,
                                     _CRACK_SEGS // 2, rng, ground_z)
        fwd_p,  fwd_n  = _crack_walk(mesh, N, seed_p, seed_n, dirv,
                                     _CRACK_SEGS - _CRACK_SEGS // 2, rng,
                                     ground_z)
        walk_p = back_p[::-1] + [seed_p] + fwd_p
        walk_n = back_n[::-1] + [seed_n] + fwd_n

        if len(walk_p) < 3:
            continue
        # One lofted V-channel: width/depth peak mid-crack, fade at ends.
        # Floor keeps the tips stubby — needle tips can't swallow the
        # stone slivers they graze near corners.
        K = len(walk_p) - 1
        prof = np.sin(np.pi * np.linspace(0.0, 1.0, K + 1)) ** 0.6
        prof = np.maximum(prof, 0.4)
        cutters.append(_crack_solid(walk_p, walk_n,
                                    _CRACK_WIDTH_MM * prof,
                                    _CRACK_DEPTH_MM * prof))

        # Real cracks fork: the stone's primary crack gets one branch,
        # leaving the fork at a natural angle and fading out.
        if is_primary and rng.random() < _CRACK_BRANCH_PROB:
            j = int(rng.integers(K // 3, 2 * K // 3 + 1))
            pd = walk_p[j + 1] - walk_p[j]
            pd = pd / (np.linalg.norm(pd) + 1e-12)
            fork = np.radians(rng.uniform(*_CRACK_BRANCH_DEG)
                              * (1 if rng.random() < 0.5 else -1))
            bd = np.cos(fork) * pd + np.sin(fork) * np.cross(walk_n[j], pd)
            br_p, br_n = _crack_walk(mesh, N, walk_p[j], walk_n[j], bd,
                                     int(rng.integers(2, 4)), rng, ground_z)
            bp = [walk_p[j]] + br_p
            bn = [walk_n[j]] + br_n
            if len(bp) >= 2:
                bprof = np.linspace(prof[j] * 0.8, 0.4, len(bp))
                cutters.append(_crack_solid(bp, bn,
                                            _CRACK_WIDTH_MM * bprof,
                                            _CRACK_DEPTH_MM * bprof))

    if not cutters:
        return mesh
    try:
        out = trimesh.boolean.difference([mesh] + cutters, engine='manifold')
        if len(out.faces) > 0 and out.is_watertight:
            # STL is float32: crack-tip sliver triangles can collapse under
            # quantization and open the mesh even though the float64 result
            # is watertight.  Accept only if the stone survives the same
            # round-trip the exported STL will take.
            v32 = out.vertices.astype(np.float32).astype(np.float64)
            chk = trimesh.Trimesh(vertices=v32, faces=out.faces.copy(),
                                  process=True)
            if chk.is_watertight:
                return out
        warnings.warn('crack engraving produced a non-watertight stone; '
                      'left uncracked', RuntimeWarning)
    except Exception as exc:                        # noqa: BLE001
        warnings.warn(f'crack engraving failed, stone left uncracked: {exc}',
                      RuntimeWarning)
    return mesh


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
            mesh = trimesh.intersections.slice_mesh_plane(
                mesh, plane_normal=[0.0, 0.0, 1.0],
                plane_origin=[0.0, 0.0, _FLOOR_MM], cap=True)
        # Stamp from the convex body (the stamp math assumes convexity),
        # then engrave — grooves are too small to matter for support/masks.
        _stamp_stone(scene, mesh)
        mesh = _engrave_cracks(
            mesh, np.random.default_rng((spec.seed ^ 0x5EED) & 0x7FFFFFFF),
            ground_z=tz0, footprint_mm=spec.footprint_mm)
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
        hr, ar, fr, lr, br, egg = _CLASS_PARAMS[cls]
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
