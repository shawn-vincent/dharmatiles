"""
Deciduous tree trunk mesh builder.

A trunk is a swept cross-section along a gently bent spine:

  * **Spine** — random walk, Laplacian-smoothed, clamped to lean_max_mm
  * **Rings** — elliptical cross-sections with taper + root flare + axial
    bark ridges (sum of angle harmonics) + horizontal wrinkles (Z offset)
  * **Mesh** — rings stitched into quad strips; apex fan at top; capped base
  * **Stubs** — short upward-leaning frustum cones for branch attachment hints

Public API
----------
``build_trunk(cx, cy, tz, angle, cfg, rng)``
    Build trunk mesh.  Returns ``(mesh, apex_pos, apex_dir, height_mm)``.

``stamp_trunk(cx, cy, tz, cfg, height_mm, support_z, obstacle_mask, surface)``
    Rasterise trunk base into support_z and obstacle_mask.

``_build_frustum(p0, p1, r0, r1, az_segs)``
    Shared by ``branches.py`` — a closed, watertight truncated cone.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..dist import sample, bounds as _bounds


# ── Spine ─────────────────────────────────────────────────────────────────────

def _build_spine(
    cx: float, cy: float, tz: float,
    height: float,
    n_seg: int,
    lean_mm: float,
    lean_max_mm: float,
    rng: np.random.Generator,
) -> np.ndarray:
    """Return a (n_seg+1, 3) array: spine control points from base to apex.

    A random walk provides a natural lean; 2 passes of Laplacian smoothing
    remove kinks without moving the fixed endpoints.
    """
    seg_len = height / n_seg
    pts = np.empty((n_seg + 1, 3))
    pts[0] = [cx, cy, tz]

    for i in range(1, n_seg + 1):
        prev = pts[i - 1]
        dx = float(rng.normal(0.0, lean_mm))
        dy = float(rng.normal(0.0, lean_mm))
        # Clamp cumulative horizontal offset from base
        new_x = prev[0] + dx
        new_y = prev[1] + dy
        off = np.sqrt((new_x - cx) ** 2 + (new_y - cy) ** 2)
        if off > lean_max_mm and off > 1e-8:
            scale = lean_max_mm / off
            new_x = cx + (new_x - cx) * scale
            new_y = cy + (new_y - cy) * scale
        pts[i] = [new_x, new_y, prev[2] + seg_len]

    # 2 passes of Laplacian smoothing (fixed endpoints)
    for _ in range(2):
        sp = pts.copy()
        pts[1:-1] = 0.5 * sp[1:-1] + 0.25 * (sp[:-2] + sp[2:])

    return pts


# ── Frenet frames via parallel transport ──────────────────────────────────────

def _compute_frames(
    spine: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    """Parallel-transport (tangent, normal) frames along *spine*.

    Returns ``(tangents, normals)``, both (N, 3).  ``binormal = cross(t, n)``.
    Parallel transport avoids the discontinuities and spin of Frenet-Serret.
    """
    N = len(spine)
    tangents = np.empty((N, 3))
    normals  = np.empty((N, 3))

    # Tangents via finite differences
    for i in range(N - 1):
        d  = spine[i + 1] - spine[i]
        dn = np.linalg.norm(d)
        tangents[i] = d / dn if dn > 1e-10 else np.array([0.0, 0.0, 1.0])
    tangents[-1] = tangents[-2]

    # Initial normal: perpendicular to first tangent
    ref = np.array([1.0, 0.0, 0.0])
    if abs(np.dot(tangents[0], ref)) > 0.9:
        ref = np.array([0.0, 1.0, 0.0])
    n0 = np.cross(tangents[0], ref)
    normals[0] = n0 / np.linalg.norm(n0)

    # Parallel transport: project each successive normal onto the current tangent plane
    for i in range(1, N):
        t = tangents[i]
        n = normals[i - 1] - np.dot(normals[i - 1], t) * t
        nn = np.linalg.norm(n)
        normals[i] = n / nn if nn > 1e-10 else normals[i - 1]

    return tangents, normals


# ── Frustum primitive (shared with branches.py) ───────────────────────────────

def _build_frustum(
    p0: np.ndarray,
    p1: np.ndarray,
    r0: float,
    r1: float,
    az_segs: int,
) -> trimesh.Trimesh:
    """Closed watertight truncated cone (frustum) from *p0* to *p1*.

    ``r0`` / ``r1`` are the radii at the two ends.  A zero-radius end degenerates
    to a cone tip (single vertex), which is handled cleanly.

    Used for branch stubs on the trunk and for branch segments.
    """
    direction = p1 - p0
    length = float(np.linalg.norm(direction))
    if length < 1e-6 or max(r0, r1) < 1e-6:
        return trimesh.Trimesh(process=False)

    dir_hat = direction / length

    # Local frame perpendicular to direction
    ref = np.array([1.0, 0.0, 0.0])
    if abs(np.dot(dir_hat, ref)) > 0.9:
        ref = np.array([0.0, 1.0, 0.0])
    n0 = np.cross(dir_hat, ref)
    n0 /= np.linalg.norm(n0)
    b0 = np.cross(dir_hat, n0)

    az  = az_segs
    th  = 2.0 * np.pi * np.arange(az) / az
    ca  = np.cos(th)
    sa  = np.sin(th)
    nv  = n0[None, :] * ca[:, None] + b0[None, :] * sa[:, None]  # (az, 3)

    bot_ring = p0 + r0 * nv   # (az, 3)
    top_ring = p1 + r1 * nv   # (az, 3)

    # Vertex layout: [bot_center, bot_ring x az, top_ring x az, top_center]
    verts = np.vstack([
        p0.reshape(1, 3),
        bot_ring,
        top_ring,
        p1.reshape(1, 3),
    ])  # (2*az + 2, 3)

    bc = 0
    tc = 2 * az + 1
    faces: list[list[int]] = []

    # Bottom cap (outward normal = downward, winding CW from outside)
    for ai in range(az):
        faces.append([bc, 1 + (ai + 1) % az, 1 + ai])

    # Side quads
    for ai in range(az):
        b_a = 1 + ai;            b_b = 1 + (ai + 1) % az
        t_a = az + 1 + ai;       t_b = az + 1 + (ai + 1) % az
        faces += [[b_a, t_a, b_b], [b_b, t_a, t_b]]

    # Top cap
    for ai in range(az):
        faces.append([tc, az + 1 + ai, az + 1 + (ai + 1) % az])

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


# ── Branch stubs ──────────────────────────────────────────────────────────────

def _build_stubs(
    spine:    np.ndarray,
    tangents: np.ndarray,
    normals:  np.ndarray,
    cfg,
    r_base:   float,
    rng:      np.random.Generator,
) -> list[trimesh.Trimesh]:
    """Short upward-angled frustum cones: branch attachment hints on the trunk."""
    N = len(spine) - 1
    min_idx = max(1, int(cfg.stub_min_height_frac * N))
    az = 8

    stubs: list[trimesh.Trimesh] = []
    for _ in range(cfg.n_stubs):
        i       = int(rng.integers(min_idx, max(min_idx + 1, N)))
        t_frac  = i / N
        r_here  = r_base * max(0.0, 1.0 - t_frac) ** cfg.taper_power

        t_vec   = tangents[i]
        n_vec   = normals[i]
        b_vec   = np.cross(t_vec, n_vec)
        bn      = np.linalg.norm(b_vec)
        if bn < 1e-8:
            continue
        b_vec /= bn

        phi         = float(rng.uniform(0.0, 2.0 * np.pi))
        lateral     = np.cos(phi) * n_vec + np.sin(phi) * b_vec
        angle_up    = float(sample(cfg.stub_angle_up, rng))
        stub_dir    = np.cos(angle_up) * lateral + np.sin(angle_up) * t_vec
        stub_dir   /= np.linalg.norm(stub_dir)

        stub_len    = float(sample(cfg.stub_length_mm, rng))
        stub_r      = min(float(sample(cfg.stub_r_base_mm, rng)), r_here * 0.75)
        if stub_r < 0.2:
            continue

        base_pos = spine[i]
        end_pos  = base_pos + stub_dir * stub_len
        stubs.append(_build_frustum(base_pos, end_pos, stub_r, stub_r * 0.1, az))

    return [m for m in stubs if len(m.vertices) > 0]


# ── Trunk ring builder ────────────────────────────────────────────────────────

def _build_rings(
    spine:       np.ndarray,
    tangents:    np.ndarray,
    normals:     np.ndarray,
    height:      float,
    r_base:      float,
    aspect:      float,
    tz:          float,
    cfg,
    rng:         np.random.Generator,
) -> list[np.ndarray]:
    """Return one ring (az_segs, 3) per spine point.

    Radius profile: taper + root flare + axial ridge harmonics + horizontal wrinkle.
    """
    N_seg   = len(spine) - 1
    az      = cfg.az_segs

    wrinkle_amp    = float(sample(cfg.wrinkle_amp,    rng))
    wrinkle_period = float(sample(cfg.wrinkle_period, rng))
    wrinkle_phase  = float(rng.uniform(0.0, 2.0 * np.pi))

    # Pre-compute per-harmonic: (k, amplitude, base_phase, z_drift_rate)
    harmonics: list[tuple[int, float, float, float]] = []
    for k in range(2, cfg.ridge_harmonics + 2):
        amp       = cfg.ridge_amp / k        # higher harmonics: less amplitude
        base_ph   = float(rng.uniform(0.0, 2.0 * np.pi))
        drift_rt  = 2.0 * np.pi / cfg.ridge_drift_mm  # radians per mm of Z
        harmonics.append((k, amp, base_ph, drift_rt))

    theta = 2.0 * np.pi * np.arange(az) / az  # (az,)

    rings: list[np.ndarray] = []
    for i, center in enumerate(spine):
        t     = i / N_seg          # 0 = base, 1 = apex
        dz_spine = center[2] - tz  # mm above terrain

        # Taper (power-law)
        r = r_base * max(0.0, 1.0 - t) ** cfg.taper_power

        # Root flare
        if t < cfg.flare_fraction:
            t_low  = (cfg.flare_fraction - t) / cfg.flare_fraction   # 1 at base, 0 at flare top
            flare  = cfg.flare_amp * (t_low ** cfg.flare_power)
            r     *= (1.0 + flare)

        # Cumulative twist angle
        twist = i * cfg.twist_per_seg

        # Ridge-noise factor per azimuth angle: (az,)
        ridge = np.ones(az)
        for k, amp, base_ph, drift_rt in harmonics:
            phase   = base_ph + drift_rt * dz_spine
            ridge  += amp * np.cos(k * (theta + twist) + phase)

        # Effective per-vertex radii (elliptical: scale one axis by aspect)
        cos_th = np.cos(theta + twist)
        sin_th = np.sin(theta + twist)

        # Tangent and frame at this spine point
        n_vec  = normals[i]
        b_vec  = np.cross(tangents[i], n_vec)
        bn     = np.linalg.norm(b_vec)
        if bn < 1e-8:
            b_vec = np.array([0.0, 1.0, 0.0])
        else:
            b_vec /= bn

        # Wrinkle: uniform Z-offset per ring
        dz = wrinkle_amp * np.sin(2.0 * np.pi * dz_spine / max(wrinkle_period, 0.1) + wrinkle_phase)

        r_eff = r * ridge  # (az,)
        ring_pts = np.empty((az, 3))
        ring_pts[:, 0] = center[0] + r_eff * (cos_th * n_vec[0] + sin_th * b_vec[0] * aspect)
        ring_pts[:, 1] = center[1] + r_eff * (cos_th * n_vec[1] + sin_th * b_vec[1] * aspect)
        ring_pts[:, 2] = center[2] + r_eff * (cos_th * n_vec[2] + sin_th * b_vec[2] * aspect) + dz

        rings.append(ring_pts)

    return rings


# ── Mesh stitcher ─────────────────────────────────────────────────────────────

def _stitch_rings(
    rings:    list[np.ndarray],
    spine:    np.ndarray,
    az:       int,
    base_z:   float,
    cx:       float,
    cy:       float,
) -> tuple[np.ndarray, np.ndarray]:
    """Stitch rings into a closed mesh.

    Vertex layout (indices 0-based):
      0                  apex (single point, top of trunk)
      1 … az             rings[0]  (base ring)
      az+1 … 2az         rings[1]
      …
      N*az+1 … (N+1)*az  rings[N]  (top ring)
      (N+1)*az+1         base centre (below terrain)
    """
    N = len(rings)  # = n_seg + 1
    n_verts  = 1 + N * az + 1
    verts    = np.empty((n_verts, 3))

    apex_idx       = 0
    base_ctr_idx   = 1 + N * az

    verts[apex_idx] = spine[-1]
    for i, ring in enumerate(rings):
        verts[1 + i * az : 1 + (i + 1) * az] = ring
    verts[base_ctr_idx] = [cx, cy, base_z]

    faces: list[list[int]] = []

    # Top fan: apex → top ring (rings[-1])
    top_start = 1 + (N - 1) * az
    for ai in range(az):
        a0 = top_start + ai
        a1 = top_start + (ai + 1) % az
        faces.append([apex_idx, a1, a0])

    # Side quad strips: rings[i] → rings[i+1]
    for i in range(N - 1):
        ra = 1 + i * az
        rb = 1 + (i + 1) * az
        for ai in range(az):
            a0 = ra + ai;     a1 = ra + (ai + 1) % az
            b0 = rb + ai;     b1 = rb + (ai + 1) % az
            faces += [[a0, a1, b0], [a1, b1, b0]]

    # Bottom fan: rings[0] → base centre
    for ai in range(az):
        a0 = 1 + ai
        a1 = 1 + (ai + 1) % az
        faces.append([a0, base_ctr_idx, a1])

    return verts, np.array(faces, dtype=np.int32)


# ── Public API ────────────────────────────────────────────────────────────────

def build_trunk(
    cx:    float,
    cy:    float,
    tz:    float,
    angle: float,
    cfg,
    rng:   np.random.Generator,
) -> tuple[trimesh.Trimesh, np.ndarray, np.ndarray, float]:
    """Build a trunk mesh and return ``(mesh, apex_pos, apex_dir, height_mm)``.

    *angle* rotates the initial Frenet frame so bark ridges face a random
    direction; it also controls the stub azimuth seeding.

    The caller (``scatter/trees.py``) passes the returned ``apex_pos``,
    ``apex_dir``, and ``height_mm`` into ``build_branches``.
    """
    height  = float(sample(cfg.height_mm,  rng))
    r_base  = float(sample(cfg.r_base_mm,  rng))
    aspect  = float(sample(cfg.aspect,     rng))

    spine    = _build_spine(cx, cy, tz, height, cfg.n_seg, cfg.lean_mm,
                             cfg.lean_max_mm, rng)
    tangents, normals_raw = _compute_frames(spine)

    # Rotate initial frame by *angle* so features face a deterministic direction
    # (avoids all trunks having ridges aligned the same way on a tile)
    cos_a, sin_a = np.cos(angle), np.sin(angle)
    b0 = np.cross(tangents[0], normals_raw[0])
    normals = np.empty_like(normals_raw)
    normals[0] = cos_a * normals_raw[0] + sin_a * b0
    normals[0] /= np.linalg.norm(normals[0])
    # Re-transport from rotated seed
    for i in range(1, len(spine)):
        t = tangents[i]
        n = normals[i - 1] - np.dot(normals[i - 1], t) * t
        nn = np.linalg.norm(n)
        normals[i] = n / nn if nn > 1e-10 else normals[i - 1]

    rings    = _build_rings(spine, tangents, normals, height, r_base,
                            aspect, tz, cfg, rng)
    verts, faces = _stitch_rings(rings, spine, cfg.az_segs,
                                 tz - cfg.sink, cx, cy)

    parts: list[trimesh.Trimesh] = [
        trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    ]

    if cfg.n_stubs > 0:
        parts.extend(_build_stubs(spine, tangents, normals, cfg, r_base, rng))

    mesh = trimesh.util.concatenate(parts)
    mesh.fix_normals()

    return mesh, spine[-1].copy(), tangents[-1].copy(), height


def stamp_trunk(
    cx:          float,
    cy:          float,
    tz:          float,
    cfg,
    height_mm:   float,
    support_z:   np.ndarray,
    obstacle_mask: np.ndarray | None,
    surface,
) -> None:
    """Rasterise trunk base footprint into *support_z* and *obstacle_mask*.

    Stamps a circle of radius ``r_base_max * (1 + flare_amp)`` to height
    ``tz + height_mm``.  Grass steers around this footprint; no blades will
    seed inside the trunk.
    """
    r_max  = float(_bounds(cfg.r_base_mm)[1]) * (1.0 + cfg.flare_amp)
    block_z = tz + height_mm

    cw = surface.cell_w
    gw = surface.grid_w
    gh = surface.grid_h

    i_lo = max(0,      int((cx - r_max) / cw))
    i_hi = min(gw - 1, int((cx + r_max) / cw) + 1)
    j_lo = max(0,      int((cy - r_max) / cw))
    j_hi = min(gh - 1, int((cy + r_max) / cw) + 1)
    if i_lo > i_hi or j_lo > j_hi:
        return

    ii = np.arange(i_lo, i_hi + 1)
    jj = np.arange(j_lo, j_hi + 1)
    II, JJ = np.meshgrid(ii, jj)
    dx  = II * cw - cx
    dy  = JJ * cw - cy
    inside = (dx ** 2 + dy ** 2) <= r_max ** 2

    if not np.any(inside):
        return

    sl = support_z[j_lo:j_hi + 1, i_lo:i_hi + 1]
    np.maximum(sl, np.where(inside, block_z, -np.inf), out=sl)

    if obstacle_mask is not None:
        obstacle_mask[j_lo:j_hi + 1, i_lo:i_hi + 1] |= inside
