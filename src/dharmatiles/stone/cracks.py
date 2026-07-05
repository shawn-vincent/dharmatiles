"""Crack engraving — surface-projected random-walk V-channels.

The standard crack config (R11, rocks campaign): a crack is a walk of
thin tapered wedges across the surface — long, meandering, fading out
at both ends, forking once on the primary.  Proportions matter: a wide
short groove reads as a router slot, not a crack (Shawn, 2026-07-03).
Shared by scatter stones and the stone floor slabs.
"""
from __future__ import annotations

import warnings

import numpy as np
import trimesh

from .solidify import survives_stl32

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


def _crack_solid(pts: list, nms: list, widths: np.ndarray,
                 depths: np.ndarray,
                 proud_mm: float = _CRACK_PROUD_MM) -> trimesh.Trimesh:
    """ONE lofted V-channel along the crack polyline.

    A single swept solid replaces the old chain of overlapping frusta —
    mutually intersecting cutters made the manifold boolean emit sliver
    triangles that collapsed under STL float32 quantization and opened
    the mesh.  Ring per station: two proud top corners + the apex."""
    K = len(pts)
    # Smooth the normals along the path: on an undulated surface the raw
    # face normals swing enough that consecutive rings cross, and the
    # everted loft turns into ADDED volume after the boolean (the nodule
    # growths on heavily weathered stones).
    nsm = []
    for k in range(K):
        n = (np.asarray(nms[max(k - 1, 0)]) + np.asarray(nms[k])
             + np.asarray(nms[min(k + 1, K - 1)]))
        nsm.append(n / (np.linalg.norm(n) + 1e-12))
    rings = []
    side_prev = None
    for k in range(K):
        prev_p = pts[max(k - 1, 0)]
        next_p = pts[min(k + 1, K - 1)]
        d = np.asarray(next_p) - np.asarray(prev_p)
        d = d / (np.linalg.norm(d) + 1e-12)
        n = nsm[k]
        if side_prev is None:
            side = np.cross(d, n)
        else:
            # Parallel transport: keep the ring orientation continuous
            # instead of re-deriving it from a wobbling normal.
            side = side_prev - d * (side_prev @ d)
        side = side / (np.linalg.norm(side) + 1e-12)
        side_prev = side
        p = np.asarray(pts[k])
        rings.append([p + n * proud_mm + side * (widths[k] / 2.0),
                      p + n * proud_mm - side * (widths[k] / 2.0),
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
    # Manifold self-union resolves any residual self-intersection — an
    # everted loft section must never reach the difference as raw geometry.
    try:
        m = trimesh.boolean.union([m], engine='manifold')
    except Exception:                               # noqa: BLE001
        pass
    return m


def _crack_walk(mesh: trimesh.Trimesh, N: np.ndarray,
                p0: np.ndarray, n0: np.ndarray, d0: np.ndarray,
                n_segs: int, rng: np.random.Generator,
                ground_z: float, pull_z: float | None = None,
                ) -> tuple[list, list]:
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
        if pull_z is not None:
            q[2] = 0.55 * q[2] + 0.45 * pull_z   # seam mode: hug the plane
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


def engrave_cracks(mesh: trimesh.Trimesh, rng: np.random.Generator,
                    ground_z: float, footprint_mm: float,
                    seam_z: float | None = None,
                    proud_mm: float = _CRACK_PROUD_MM,
                    n_segs: int = _CRACK_SEGS) -> trimesh.Trimesh:
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
                                     n_segs // 2, rng, ground_z)
        fwd_p,  fwd_n  = _crack_walk(mesh, N, seed_p, seed_n, dirv,
                                     n_segs - n_segs // 2, rng,
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
                                    _CRACK_DEPTH_MM * prof, proud_mm))

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
                                            _CRACK_DEPTH_MM * bprof,
                                            proud_mm))

    # Fracture seam: one long near-horizontal groove wrapping the stone
    # (bedding read, per the Letipea erratic) — walked both ways from a
    # random azimuth, pulled toward the seam plane at every step.
    if seam_z is not None and exposed > 4.0:
        zs = ground_z + seam_z * exposed
        c  = np.asarray(mesh.vertices).mean(axis=0)
        az = rng.uniform(0.0, 2.0 * np.pi)
        q  = c + np.array([np.cos(az), np.sin(az), 0.0]) * footprint_mm
        q[2] = zs
        qs, _dist, tid = trimesh.proximity.closest_point(mesh, [q])
        p0, n0 = qs[0], N[tid[0]]
        d0 = np.cross(n0, np.array([0.0, 0.0, 1.0]))
        d0 = d0 / (np.linalg.norm(d0) + 1e-12)
        # Seam length and cross-section scale with the stone (E9): a
        # fixed-mm seam is proportionally thread-thin on a hero boulder.
        fs      = float(np.clip(footprint_mm / 12.0, 1.0, 2.0))
        n_segs  = int(np.clip(footprint_mm * 0.55, 7, 14))
        s_back, sn_back = _crack_walk(mesh, N, p0, n0, -d0, n_segs, rng,
                                      ground_z, pull_z=zs)
        s_fwd,  sn_fwd  = _crack_walk(mesh, N, p0, n0, d0, n_segs, rng,
                                      ground_z, pull_z=zs)
        sp = s_back[::-1] + [p0] + s_fwd
        sn = sn_back[::-1] + [n0] + sn_fwd
        if len(sp) >= 4:
            prof = np.sin(np.pi * np.linspace(0.0, 1.0, len(sp))) ** 0.4
            prof = np.maximum(prof, 0.55)
            cutters.append(_crack_solid(sp, sn,
                                        1.4 * fs * _CRACK_WIDTH_MM * prof,
                                        1.5 * fs * _CRACK_DEPTH_MM * prof,
                                        proud_mm))

    if not cutters:
        return mesh
    try:
        out = trimesh.boolean.difference([mesh] + cutters, engine='manifold')
        if len(out.faces) > 0 and out.is_watertight:
            # Genus check: a groove that tunnels under a bump leaves a
            # bridge (a handle) — reject anything that isn't sphere-like.
            if survives_stl32(out) and out.euler_number == 2:
                return out
        warnings.warn('crack engraving produced a non-watertight or '
                      'non-spherical stone; left uncracked', RuntimeWarning)
    except Exception as exc:                        # noqa: BLE001
        warnings.warn(f'crack engraving failed, stone left uncracked: {exc}',
                      RuntimeWarning)
    return mesh

