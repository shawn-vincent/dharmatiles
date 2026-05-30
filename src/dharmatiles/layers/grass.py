"""
GrassLayer: terrain-following grass blade placement and mesh generation.

Algorithm
─────────
  Blades are placed on a jittered grid, sorted downstream-first (exit edge
  first so upstream blades naturally arch over already-placed ones).

  For each blade:
    • XY path  — chord-preserving 2D arc with variable lean driven by a flow field.
    • Z path   — least-concave-majorant (LCM) envelope that rides the current
                 support field with a small clearance gap.
    • Strict intersection repair — up to COLLISION_REPAIR_PASSES attempts to
      raise the z-floor at detected hit sites, then up to MAX_BOUNDARY_RETRIES
      direction retries if the blade exits the tile.

  Accepted blades are rasterised into support_z so subsequent blades see them.
"""
from __future__ import annotations

from typing import List, Optional, Tuple

import numpy as np
import trimesh
from scipy.interpolate import PchipInterpolator

from ..core.tile import TileConfig, TileScene
from ..core.grid import sample_grid, rasterise_into_support
from ..core.mesh import compute_up_locs, build_tube_mesh, blade_frame
from ..core.collision import (collect_strict_hits, log_strict_hits,
                               add_collision_repairs)


# ── Blade placement ───────────────────────────────────────────────────────────

def place_blades(cfg: TileConfig, rng: np.random.Generator,
                 flow_angle_field: np.ndarray,
                 flow_curv_field: np.ndarray,
                 n: int,
                 w_min: float, w_max: float,
                 l_min: float, l_max: float,
                 tl_min: float, tl_max: float) -> list:
    """Place *n* blades on a jittered grid; return list of blade parameter dicts."""
    if n == 0:
        return []

    cols = int(np.ceil(np.sqrt(n)))
    rows = int(np.ceil(n / cols))
    cw   = cfg.tile_w / cols
    ch   = cfg.tile_h / rows
    cells = [(c, r) for c in range(cols) for r in range(rows)]
    rng.shuffle(cells)
    edge = w_max / 2 + 0.2

    out = []
    for c, r in cells:
        if len(out) >= n:
            break
        bx = float(np.clip((c + rng.uniform(0.1, 0.9)) * cw, edge, cfg.tile_w - edge))
        by = float(np.clip((r + rng.uniform(0.1, 0.9)) * ch, edge, cfg.tile_h - edge))

        base_angle = float(sample_grid(flow_angle_field, cfg, bx, by))
        direction  = base_angle + float(rng.normal(0, cfg.dir_spread))

        kappa     = float(sample_grid(flow_curv_field, cfg, bx, by))
        rand_curl = float(rng.uniform(-cfg.curl_max, cfg.curl_max))
        curv_curl = float(np.sign(kappa) * (kappa ** 2) * cfg.curl_max *
                          rng.uniform(0.4, 1.0))
        curl = float(np.clip(
            cfg.curl_from_curv * curv_curl + (1 - cfg.curl_from_curv) * rand_curl,
            -cfg.curl_max, cfg.curl_max,
        ))

        out.append(dict(
            base_x    = bx,
            base_y    = by,
            width     = float(rng.uniform(w_min, w_max)),
            length    = float(rng.uniform(l_min, l_max)),
            tip_len   = float(rng.uniform(tl_min, tl_max)),
            direction = direction,
            curl      = curl,
        ))
    return out


# ── Z-solver: least-concave-majorant ─────────────────────────────────────────

def _upper_concave_envelope(t_arr, height_arr) -> list:
    """Least concave majorant through ordered obstacle points.

    Returns a minimal list of (t, z, original_index) control points whose
    piecewise-linear upper envelope is concave — i.e. slopes are non-increasing.
    This is the correct shape for the 'lowest possible curve that clears all
    obstacles': it hugs obstacle tops without unnecessary up-arching.
    """
    points = [(float(t_arr[0]), float(height_arr[0]), 0)]
    points.extend(
        (float(t_arr[i]), float(height_arr[i]), i)
        for i in range(1, len(t_arr) - 1)
        if np.isfinite(height_arr[i])
    )
    if np.isfinite(height_arr[-1]):
        points.append((float(t_arr[-1]), float(height_arr[-1]), len(t_arr) - 1))

    def slope(a, b):
        return (b[1] - a[1]) / (b[0] - a[0])

    stack = []
    for pt in points:
        stack.append(pt)
        while len(stack) >= 3:
            a, b, c = stack[-3], stack[-2], stack[-1]
            if slope(b, c) > slope(a, b):
                stack.pop(-2)
            else:
                break
    return stack


def _smooth_contact_curve(t_arr, contacts) -> np.ndarray:
    """Shape-preserving C¹ cubic through the LCM contact points."""
    ctrl_t = np.array([p[0] for p in contacts], dtype=float)
    ctrl_z = np.array([p[1] for p in contacts], dtype=float)
    if len(ctrl_t) <= 2:
        return np.interp(t_arr, ctrl_t, ctrl_z)
    return PchipInterpolator(ctrl_t, ctrl_z)(t_arr)


def _fit_envelope_spine(cfg: TileConfig, t_arr, floor_z,
                         terrain_z_path) -> Optional[np.ndarray]:
    """Return the LCM spine z, or None if the floor exceeds the stack-height cap."""
    base_z    = float(floor_z[0])
    ceiling_z = base_z + cfg.max_stack_height
    if np.any(np.asarray(floor_z)[np.isfinite(floor_z)] > ceiling_z + 1e-6):
        return None

    contacts = _upper_concave_envelope(t_arr, floor_z)
    spine_z  = _smooth_contact_curve(t_arr, contacts)
    if np.any(spine_z < floor_z - 1e-6) or np.any(spine_z > ceiling_z + 1e-6):
        return None
    return spine_z


# ── Sub-hull (printable support under each blade) ─────────────────────────────

def _drop_to_support(point, down_vec, support_z: np.ndarray,
                     cfg: TileConfig) -> np.ndarray:
    """Bisect along *down_vec* from *point* until hitting support_z."""
    start = np.asarray(point, dtype=float)
    down  = np.asarray(down_vec, dtype=float)
    if down[2] >= -1e-6:
        down = np.array([0.0, 0.0, -1.0])

    def clearance(dist):
        p = start + dist * down
        return p[2] - sample_grid(support_z, cfg, p[0], p[1])

    if clearance(0.0) <= 0.0:
        return start

    hi = 0.25
    search_limit = cfg.base_h + cfg.max_stack_height + cfg.grass_thickness + 2.0
    while hi < search_limit and clearance(hi) > 0.0:
        hi *= 2.0
    if clearance(hi) > 0.0:
        return start + hi * down

    lo = 0.0
    for _ in range(16):
        mid = 0.5 * (lo + hi)
        if clearance(mid) > 0.0:
            lo = mid
        else:
            hi = mid
    return start + hi * down


def _build_sub_hull_mesh(cfg: TileConfig, spine_3d: np.ndarray,
                          widths: np.ndarray,
                          support_z: np.ndarray) -> trimesh.Trimesh:
    """Separate printable support hull that bridges under each blade to the terrain.

    Builds a triangular-prism strut running the length of the blade spine.
    Two side vertices attach to the blade's underside; a third vertex is
    dropped along the local down direction until it touches the support surface.

    triangle cross-section
        side_r / side_l sit halfway down the two triangle sides (fraction
        *grass_sub_hull_fraction* from each top edge toward the apex).

    circle cross-section
        side_r / side_l sit at ± (frac × 90°) from the top of the circle,
        placing them on the lower half of the circle at an equivalent depth.
        The same drop-to-terrain logic applies.
    """
    path  = np.asarray(spine_3d, dtype=float)
    W_arr = np.asarray(widths, dtype=float)
    n_pts = len(path)
    n     = 3

    _, up_locs, down_locs = blade_frame(path)
    half_W = (W_arr / 2.0)[:, None]
    frac   = cfg.grass_sub_hull_fraction

    if cfg.blade_cross_section == 'triangle':
        apex   = path + cfg.grass_thickness * down_locs
        right  = path + half_W * up_locs
        left   = path - half_W * up_locs
        side_r = right + frac * (apex - right)
        side_l = left  + frac * (apex - left)

    else:  # 'circle' — attach at ±angle from top, on the lower half of the circle
        # theta = frac × π/2:  0 = top (up_loc), π/2 = down_loc side
        # At frac=0.5 this gives 45° — midway into the lower hemisphere.
        theta_r = frac * np.pi / 2        # right attachment angle
        theta_l = np.pi - theta_r         # left  attachment angle (symmetric)
        side_r  = (path +
                   half_W * (np.cos(theta_r) * up_locs + np.sin(theta_r) * down_locs))
        side_l  = (path +
                   half_W * (np.cos(theta_l) * up_locs + np.sin(theta_l) * down_locs))

    centers = 0.5 * (side_r + side_l)

    lower = np.empty_like(path)
    for idx in range(n_pts):
        lower[idx] = _drop_to_support(centers[idx], down_locs[idx], support_z, cfg)

    ring_v = np.stack([lower, side_r, side_l], axis=1)   # (n_pts, 3, 3)

    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = fi = 0

    for idx in range(n_pts):
        verts[vi:vi + n] = ring_v[idx];  vi += n

    v_base = vi;  verts[vi] = np.mean(ring_v[0],  axis=0);  vi += 1
    v_tip  = vi;  verts[vi] = np.mean(ring_v[-1], axis=0);  vi += 1

    for idx in range(n):
        faces[fi] = [v_base, (idx + 1) % n, idx];  fi += 1

    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for idx in range(n):
            i1 = (idx + 1) % n
            faces[fi] = [ra + idx, rb + idx, ra + i1];  fi += 1
            faces[fi] = [ra + i1, rb + idx, rb + i1];   fi += 1

    rl = (n_pts - 1) * n
    for idx in range(n):
        faces[fi] = [rl + idx, rl + (idx + 1) % n, v_tip];  fi += 1

    mesh = trimesh.Trimesh(vertices=verts[:vi],
                           faces=faces[:fi].astype(int),
                           process=False)
    mesh.fix_normals()
    return mesh


# ── Tile footprint check ──────────────────────────────────────────────────────

def blade_footprint_inside_tile(cfg: TileConfig, spine_3d, widths) -> bool:
    """True iff the blade's XY footprint (spine ± half_width) is inside the tile."""
    path = np.asarray(spine_3d)
    hws  = np.asarray(widths) / 2.0
    if np.any(path[:, 0] - hws < 0.0) or np.any(path[:, 0] + hws > cfg.tile_w):
        return False
    if np.any(path[:, 1] - hws < 0.0) or np.any(path[:, 1] + hws > cfg.tile_h):
        return False
    return True


# ── Blade builder ─────────────────────────────────────────────────────────────

def make_grass_blade(
    cfg: TileConfig,
    support_z: np.ndarray,
    terrain_z: np.ndarray,
    base_pos: Tuple[float, float],
    azimuth: float,
    length: float,
    width: float,
    tip_length: float,
    curl: float = 0.0,
    extra_floor_z: Optional[np.ndarray] = None,
) -> Tuple[trimesh.Trimesh, trimesh.Trimesh, np.ndarray, np.ndarray]:
    """Build one terrain-following grass blade.

    Returns
    -------
    (blade_mesh, sub_hull_mesh, spine_3d, widths_arr)

    Raises
    ------
    RuntimeError if the LCM envelope fit fails (blade cannot fit without
    exceeding MAX_STACK_HEIGHT).
    """
    bx, by  = float(base_pos[0]), float(base_pos[1])
    total_l = length + tip_length
    dt      = 1.0 / (cfg.n_path - 1)
    _CURL_SWEEP = np.pi    # |curl|=1 → ±180° lateral sweep

    # ── XY path: chord-preserving 2D arc ─────────────────────────────────────
    k_arr  = np.arange(1, cfg.n_path)
    t_mid  = (k_arr - 0.5) * dt
    lean_v = (cfg.base_lean_angle +
              (cfg.lean_angle - cfg.base_lean_angle) * (1.0 - np.cos(t_mid * np.pi / 2.0)))
    az_v   = azimuth + curl * _CURL_SWEEP * t_mid
    ds     = total_l * dt
    dxr    = np.sin(az_v) * np.sin(lean_v) * ds
    dyr    = np.cos(az_v) * np.sin(lean_v) * ds
    xr     = np.concatenate([[0.0], np.cumsum(dxr)])
    yr     = np.concatenate([[0.0], np.cumsum(dyr)])

    # Rotate so base→tip chord aligns with azimuth
    tip_dist = np.hypot(xr[-1], yr[-1])
    if tip_dist > 1e-6:
        tip_angle    = np.arctan2(xr[-1], yr[-1])
        rot          = tip_angle - azimuth
        cos_r, sin_r = np.cos(rot), np.sin(rot)
        xrot = xr * cos_r - yr * sin_r
        yrot = xr * sin_r + yr * cos_r
    else:
        xrot, yrot = xr, yr

    # ── XY world positions & taper widths ─────────────────────────────────────
    xs_arr = bx + xrot                                          # (n_path,)
    ys_arr = by + yrot
    tz_arr = sample_grid(terrain_z, cfg, xs_arr, ys_arr)       # terrain z along spine

    k_arr      = np.arange(cfg.n_path)
    s_arr      = k_arr * dt * total_l
    t_tip_arr  = np.clip((s_arr - length) / (tip_length + 1e-9), 0.0, 1.0)
    widths_arr = width * np.cos(t_tip_arr * np.pi / 2.0)       # cosine taper
    hw_arr     = widths_arr / 2.0

    # Lateral top-edge positions (needed to sample support under both edges)
    up_pre = compute_up_locs(
        np.stack([xs_arr, ys_arr, np.zeros(cfg.n_path)], axis=1)
    )
    v1_xs = xs_arr + hw_arr * up_pre[:, 0]
    v1_ys = ys_arr + hw_arr * up_pre[:, 1]
    v2_xs = xs_arr - hw_arr * up_pre[:, 0]
    v2_ys = ys_arr - hw_arr * up_pre[:, 1]

    # Support heights under each top edge; take the max
    sz_v1        = sample_grid(support_z, cfg, v1_xs, v1_ys)
    sz_v2        = sample_grid(support_z, cfg, v2_xs, v2_ys)
    edge_support = np.maximum(sz_v1, sz_v2)

    # ── Z floor construction ───────────────────────────────────────────────────
    t_arr       = np.linspace(0.0, 1.0, cfg.n_path)
    floor_z     = edge_support + cfg.clearance
    floor_z[t_arr < cfg.base_obstacle_ignore_t] = -np.inf   # eruption zone
    floor_z[0]  = float(tz_arr[0]) - cfg.base_sink          # base pinned to terrain

    if extra_floor_z is not None:
        floor_z = np.maximum(floor_z, np.asarray(extra_floor_z, dtype=float))
        floor_z[0] = float(tz_arr[0]) - cfg.base_sink       # keep base pinned

    # ── LCM envelope fit ───────────────────────────────────────────────────────
    spine_z = _fit_envelope_spine(cfg, t_arr, floor_z, tz_arr)
    if spine_z is None:
        raise RuntimeError("LCM envelope fit failed: floor exceeds stack-height cap")

    path_xyz = np.stack([xs_arr, ys_arr, spine_z], axis=1)   # (n_path, 3)

    blade_mesh    = build_tube_mesh(path_xyz, widths_arr, cfg.grass_thickness,
                                    cross_section=cfg.blade_cross_section,
                                    n_segs=cfg.blade_circle_segs)
    sub_hull_mesh = _build_sub_hull_mesh(cfg, path_xyz, widths_arr, support_z)

    return blade_mesh, sub_hull_mesh, path_xyz, widths_arr


# ── GrassLayer ────────────────────────────────────────────────────────────────

class GrassLayer:
    """Place and build all grass blades on the scene."""

    def __init__(self, cfg: TileConfig) -> None:
        self.cfg = cfg

    def build(self, scene: TileScene,
              flow_angle_field: np.ndarray,
              flow_curv_field: np.ndarray,
              verbose: bool = True) -> List[trimesh.Trimesh]:
        """Place blades, build meshes, update *scene.support_z*.

        Returns the list of blade + sub-hull meshes.
        """
        cfg = self.cfg
        rng = np.random.default_rng(cfg.seed)

        # ── Blade seeds ────────────────────────────────────────────────────────
        tall  = place_blades(cfg, rng, flow_angle_field, flow_curv_field,
                             cfg.n_blades,
                             cfg.tall_w_min,  cfg.tall_w_max,
                             cfg.tall_l_min,  cfg.tall_l_max,
                             cfg.tall_tl_min, cfg.tall_tl_max)
        fills = place_blades(cfg, rng, flow_angle_field, flow_curv_field,
                             cfg.n_fill,
                             cfg.fill_w_min,  cfg.fill_w_max,
                             cfg.fill_l_min,  cfg.fill_l_max,
                             cfg.fill_tl_min, cfg.fill_tl_max)
        blades = tall + fills

        # Downstream-first sort: exit-edge blades placed first so upstream
        # blades arch over them naturally.
        mfx = float(np.mean(np.sin(flow_angle_field)))
        mfy = float(np.mean(np.cos(flow_angle_field)))
        blades.sort(key=lambda b: -(mfx * b['base_x'] + mfy * b['base_y']))

        if verbose:
            print(f"Placed {len(blades)} blades  (flow sort: fx={mfx:.2f} fy={mfy:.2f})")

        # ── Build loop ─────────────────────────────────────────────────────────
        parts: List[trimesh.Trimesh] = []
        built_blades   = 0
        skipped_blades = 0
        MAX_RETRIES    = 32
        placed_data: list = []   # (blade_idx, spine, hw, up_locs) for strict check
        retry_rng = np.random.default_rng(cfg.seed + 424242)

        for i, bl in enumerate(blades):
            accepted = None

            for attempt in range(MAX_RETRIES + 1):
                direction = bl['direction'] if attempt == 0 else retry_rng.uniform(0, 2 * np.pi)
                curl      = bl['curl']      if attempt == 0 else retry_rng.uniform(-cfg.curl_max, cfg.curl_max)
                repair_floor = None

                for _rep in range(cfg.collision_repair_passes + 1):
                    try:
                        blade_mesh, sub_hull, spine, widths = make_grass_blade(
                            cfg          = cfg,
                            support_z    = scene.support_z,
                            terrain_z    = scene.terrain_z,
                            base_pos     = (bl['base_x'], bl['base_y']),
                            azimuth      = direction,
                            length       = bl['length'],
                            width        = bl['width'],
                            tip_length   = bl['tip_len'],
                            curl         = curl,
                            extra_floor_z = repair_floor,
                        )
                    except RuntimeError:
                        break

                    if not blade_footprint_inside_tile(cfg, spine, widths):
                        break

                    hw       = widths / 2.0
                    up_locs  = compute_up_locs(spine)
                    hits     = (collect_strict_hits(spine, hw, up_locs, placed_data,
                                                    cfg.strict_base_t)
                                if cfg.strict_mode else [])

                    if not hits:
                        accepted = (blade_mesh, sub_hull, spine, widths, up_locs)
                        break

                    if repair_floor is None:
                        repair_floor = np.full(len(spine), -np.inf, dtype=float)
                    add_collision_repairs(repair_floor, spine, hits, cfg.clearance)

                if accepted is not None:
                    break

            if accepted is None:
                skipped_blades += 1
                continue

            blade_mesh, sub_hull, spine, widths, up_locs = accepted
            parts.append(blade_mesh)
            parts.append(sub_hull)
            built_blades += 1

            hw = widths / 2.0
            if cfg.strict_mode:
                log_strict_hits(i, bl['base_x'], bl['base_y'], spine,
                                collect_strict_hits(spine, hw, up_locs, placed_data,
                                                    cfg.strict_base_t))
                placed_data.append((i, spine, hw, up_locs))

            rasterise_into_support(scene.support_z, cfg, spine, hw)

            if verbose and ((i + 1) % 20 == 0 or (i + 1) == len(blades)):
                print(f"  {i + 1}/{len(blades)} blades done")

        if verbose:
            if skipped_blades:
                print(f"  skipped {skipped_blades} blade(s) that could not fit")
            print(f"  built {built_blades}/{len(blades)} blades")
            _print_height_audit(blades, placed_data, scene.terrain_z, cfg)

        return parts


def _print_height_audit(blades: list, placed_data: list,
                         terrain_z: np.ndarray, cfg: TileConfig) -> None:
    """Print a percentile summary of blade rise heights."""
    from ..core.grid import sample_grid as sg
    rises = []
    for blade_idx, spine, hw, up_locs in placed_data:
        bl      = blades[blade_idx]
        base_tz = float(sg(terrain_z, cfg, bl['base_x'], bl['base_y']))
        rises.append(float(np.max(spine[:, 2])) - base_tz)

    if not rises:
        print("  no blades built")
        return

    rises = np.array(rises)
    print("\nBlade height audit (spine z above local terrain):")
    print(f"  n={len(rises)}  min={rises.min():.1f}mm  "
          f"p25={np.percentile(rises, 25):.1f}mm  "
          f"median={np.median(rises):.1f}mm  "
          f"p75={np.percentile(rises, 75):.1f}mm  "
          f"p90={np.percentile(rises, 90):.1f}mm  "
          f"p99={np.percentile(rises, 99):.1f}mm  "
          f"max={rises.max():.1f}mm")
    over = int(np.sum(rises > cfg.max_stack_height + 1e-6))
    print(f"  blades rising > {cfg.max_stack_height:.0f}mm: {over}")
