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
from ..core.mesh import compute_up_locs, build_tube_mesh, blade_frame, \
                        drop_to_support, build_sub_hull_mesh
from ..core.collision import (collect_strict_hits, log_strict_hits,
                               add_collision_repairs)


# ── Blade placement ───────────────────────────────────────────────────────────

def _placement_density_field(cfg: TileConfig,
                             flow_angle_field: np.ndarray) -> np.ndarray:
    """Return placement weights: higher at divergent flow, lower near edges."""
    fx = np.sin(flow_angle_field)
    fy = np.cos(flow_angle_field)
    _dfx_dy, dfx_dx = np.gradient(fx, cfg.gy, cfg.gx)
    dfy_dy, _dfy_dx = np.gradient(fy, cfg.gy, cfg.gx)
    positive_div = np.maximum(dfx_dx + dfy_dy, 0.0)

    scale = np.percentile(positive_div, 95)
    if scale > 1e-9:
        div_weight = (
            1.0 +
            cfg.divergence_density_gain * np.clip(positive_div / scale, 0.0, 1.0)
        )
    else:
        div_weight = np.ones_like(flow_angle_field)

    iy, ix = np.mgrid[0:cfg.grid_res, 0:cfg.grid_res]
    x = ix * cfg.gx
    y = iy * cfg.gy
    edge_dist = np.minimum.reduce([x, y, cfg.tile_w - x, cfg.tile_h - y])
    t = np.clip(edge_dist / max(cfg.edge_density_margin, 1e-9), 0.0, 1.0)
    smooth_t = t * t * (3.0 - 2.0 * t)
    edge_weight = cfg.edge_density_min + (1.0 - cfg.edge_density_min) * smooth_t

    return div_weight * edge_weight


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

    n_candidates = max(n, int(np.ceil(n * cfg.density_candidate_factor)))
    cols = int(np.ceil(np.sqrt(n_candidates)))
    rows = int(np.ceil(n_candidates / cols))
    cw   = cfg.tile_w / cols
    ch   = cfg.tile_h / rows
    cells = [(c, r) for c in range(cols) for r in range(rows)]
    edge = w_max / 2 + 0.2
    density_field = _placement_density_field(cfg, flow_angle_field)

    weights = []
    for c, r in cells:
        x = (c + 0.5) * cw
        y = (r + 0.5) * ch
        weights.append(float(sample_grid(density_field, cfg, x, y)))
    weights = np.maximum(np.asarray(weights, dtype=float), 1e-9)
    weights /= np.sum(weights)
    chosen = rng.choice(len(cells), size=min(n, len(cells)), replace=False, p=weights)

    out = []
    for cell_idx in chosen:
        if len(out) >= n:
            break
        c, r = cells[int(cell_idx)]
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
        min_curl = cfg.curl_min_fraction * cfg.curl_max
        if 0.0 < abs(curl) < min_curl:
            curl = float(np.sign(curl) * min_curl)
        elif curl == 0.0 and min_curl > 0.0:
            curl = float(rng.choice([-min_curl, min_curl]))

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


# ── Edge-rotation helper for tuft placement ──────────────────────────────────

def _find_edge_rotation(bx: float, by: float,
                        directions: np.ndarray,
                        reach: float,
                        cfg: TileConfig,
                        margin: float) -> float:
    """Return the minimum-magnitude rotation (radians) that keeps the estimated
    tip of every blade in *directions* inside the tile.

    Uses a ±2.5° scan up to ±180°.  Returns 0.0 if already inside, or if no
    rotation in the scan range fixes the violation (the actual footprint check
    will filter those blades out later).

    Parameters
    ----------
    bx, by     : blade base world position (mm).
    directions : tuft blade azimuths (radians, before rotation).
    reach      : estimated horizontal reach of each blade (mm).
    cfg        : TileConfig for tile dimensions.
    margin     : minimum distance from each tile edge (mm).
    """
    lo_x, hi_x = margin, cfg.tile_w - margin
    lo_y, hi_y = margin, cfg.tile_h - margin

    def all_inside(rot: float) -> bool:
        for d in directions:
            tx = bx + reach * np.sin(d + rot)
            ty = by + reach * np.cos(d + rot)
            if not (lo_x <= tx <= hi_x and lo_y <= ty <= hi_y):
                return False
        return True

    if all_inside(0.0):
        return 0.0

    # Scan increasing magnitude, trying both CW (+) and CCW (−) directions.
    for step in np.linspace(np.pi / 72, np.pi, 72):    # 2.5° → 180° in 2.5° steps
        if all_inside(step):
            return float(step)
        if all_inside(-step):
            return float(-step)

    return 0.0   # can't fix; footprint checks will drop individual blades


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

def make_vegetation_blade(
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
    vegetation_type: str = 'grass',
) -> Tuple[trimesh.Trimesh, trimesh.Trimesh, np.ndarray, np.ndarray]:
    """Build one terrain-following vegetation blade (grass or leaf).

    Parameters
    ----------
    vegetation_type : 'grass' — constant-width ribbon with cosine tip taper.
                      'leaf'  — ovate broadleaf: rises 0→max at leaf_peak_t,
                                falls max→0 at tip; uses cfg.leaf_lean_angle.

    Returns
    -------
    (blade_mesh, sub_hull_mesh, spine_3d, widths_arr)

    Raises
    ------
    RuntimeError if the LCM envelope fit fails.
    """
    bx, by  = float(base_pos[0]), float(base_pos[1])
    total_l = length + tip_length
    dt      = 1.0 / (cfg.n_path - 1)
    _CURL_SWEEP = np.pi    # |curl|=1 → ±180° lateral sweep

    # ── Lean angle: grass uses cfg.lean_angle, leaf uses cfg.leaf_lean_angle ──
    tip_lean = cfg.lean_angle if vegetation_type == 'grass' else cfg.leaf_lean_angle

    # ── XY path: chord-preserving 2D arc ─────────────────────────────────────
    k_arr  = np.arange(1, cfg.n_path)
    t_mid  = (k_arr - 0.5) * dt
    lean_v = (cfg.base_lean_angle +
              (tip_lean - cfg.base_lean_angle) * (1.0 - np.cos(t_mid * np.pi / 2.0)))
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

    # ── XY world positions & width profile ────────────────────────────────────
    xs_arr = bx + xrot                                          # (n_path,)
    ys_arr = by + yrot
    tz_arr = sample_grid(terrain_z, cfg, xs_arr, ys_arr)       # terrain z along spine

    k_arr = np.arange(cfg.n_path)
    s_arr = k_arr * dt * total_l

    if vegetation_type == 'grass':
        # Constant width along body, cosine taper in the tip section.
        t_tip_arr  = np.clip((s_arr - length) / (tip_length + 1e-9), 0.0, 1.0)
        widths_arr = width * np.cos(t_tip_arr * np.pi / 2.0)
    else:
        # Ovate broadleaf: two quarter-cosine phases joined at leaf_peak_t.
        #   Phase 1 (0 → peak_t):   sin rises  0 → max_width
        #   Phase 2 (peak_t → 1.0): cos falls  max_width → 0
        # Both halves are C¹ at the junction (derivative = 0 from each side).
        t_norm = s_arr / (total_l + 1e-9)           # 0 → 1 along blade
        peak_t = cfg.leaf_peak_t
        widths_arr = width * np.where(
            t_norm <= peak_t,
            np.sin(0.5 * np.pi * t_norm / (peak_t + 1e-9)),
            np.cos(0.5 * np.pi * (t_norm - peak_t) / (1.0 - peak_t + 1e-9)),
        )

    hw_arr = widths_arr / 2.0

    # ── Support sampling ──────────────────────────────────────────────────────
    # Sample at 5 positions across the blade width: -1, -½, 0, +½, +1 of half-
    # width.  Grass (≤2 mm wide) doesn't need this, but broad leaves (≤5.5 mm)
    # can have obstacles between the spine and the edge that 2-point sampling
    # would miss — e.g. a grass blade sitting 1 mm from the leaf spine would
    # not be visible at ±2.75 mm.  Taking the max across all 5 samples ensures
    # the Z-floor envelope sees the tallest obstacle anywhere under the blade.
    up_pre = compute_up_locs(
        np.stack([xs_arr, ys_arr, np.zeros(cfg.n_path)], axis=1)
    )
    _lat_fracs = (-1.0, -0.5, 0.0, 0.5, 1.0)
    edge_support = np.full(cfg.n_path, -np.inf, dtype=float)
    for frac in _lat_fracs:
        xs = xs_arr + frac * hw_arr * up_pre[:, 0]
        ys = ys_arr + frac * hw_arr * up_pre[:, 1]
        np.maximum(edge_support, sample_grid(support_z, cfg, xs, ys),
                   out=edge_support)

    # ── Z floor construction ───────────────────────────────────────────────────
    t_arr   = np.linspace(0.0, 1.0, cfg.n_path)
    floor_z = edge_support + cfg.clearance

    # Eruption-zone override: near the base where the blade punches out of the
    # terrain, relax the floor so nearby blades don't block it.
    # Grass uses a fixed t-fraction (blades erupt steeply and are narrow).
    # Leaves use a width-based threshold: only ignore where the leaf is still
    # truly narrow (< 0.4 mm), i.e. the first ~3% of blade length.  Beyond
    # that, the wide leaf body must properly clear whatever is below it.
    if vegetation_type == 'grass':
        floor_z[t_arr < cfg.base_obstacle_ignore_t] = -np.inf
    else:
        floor_z[widths_arr < 0.4] = -np.inf   # only the near-zero base

    floor_z[0] = float(tz_arr[0]) - cfg.base_sink   # base always pinned to terrain

    if extra_floor_z is not None:
        floor_z = np.maximum(floor_z, np.asarray(extra_floor_z, dtype=float))
        floor_z[0] = float(tz_arr[0]) - cfg.base_sink       # keep base pinned

    # ── LCM envelope fit ───────────────────────────────────────────────────────
    spine_z = _fit_envelope_spine(cfg, t_arr, floor_z, tz_arr)
    if spine_z is None:
        raise RuntimeError("LCM envelope fit failed: floor exceeds stack-height cap")

    path_xyz = np.stack([xs_arr, ys_arr, spine_z], axis=1)   # (n_path, 3)

    xsec = (cfg.leaf_cross_section if vegetation_type == 'leaf'
            else cfg.blade_cross_section)
    blade_mesh    = build_tube_mesh(path_xyz, widths_arr, cfg.grass_thickness,
                                    cross_section=xsec,
                                    n_segs=cfg.blade_circle_segs,
                                    diamond_equator=cfg.blade_diamond_equator)
    sub_hull_mesh = build_sub_hull_mesh(cfg, path_xyz, widths_arr, support_z,
                                         cross_section=xsec)

    return blade_mesh, sub_hull_mesh, path_xyz, widths_arr


# ── VegetationLayer ───────────────────────────────────────────────────────────

class VegetationLayer:
    """Place and build all grass blades on the scene."""

    def __init__(self, cfg: TileConfig) -> None:
        self.cfg = cfg

    def build(self, scene: TileScene,
              flow_angle_field: np.ndarray,
              flow_curv_field: np.ndarray,
              verbose: bool = True) -> List[trimesh.Trimesh]:
        """Place mixed grass/leaf tuft seeds and build all vegetation meshes.

        Seed counts are split by ``grass_ratio : leaf_ratio``.  Grass seeds
        expand to ``randint(tuft_min, tuft_max)`` blades fanned over
        ``±tuft_spread/2``; leaf seeds expand to
        ``randint(leaf_tuft_min, leaf_tuft_max)`` blades (default 1).
        The whole fan is rotated if any tip would exit the tile.

        Returns the list of blade + sub-hull meshes.
        """
        cfg      = self.cfg
        rng      = np.random.default_rng(cfg.seed)
        tuft_rng = np.random.default_rng(cfg.seed ^ 0x54554654)  # independent

        # ── Split n_blades into grass vs leaf seeds by ratio ───────────────────
        ratio_sum = max(cfg.grass_ratio + cfg.leaf_ratio, 1)
        n_grass   = round(cfg.n_blades * cfg.grass_ratio / ratio_sum)
        n_leaf    = cfg.n_blades - n_grass

        grass_tall = place_blades(cfg, rng, flow_angle_field, flow_curv_field,
                                  n_grass,
                                  cfg.tall_w_min,  cfg.tall_w_max,
                                  cfg.tall_l_min,  cfg.tall_l_max,
                                  cfg.tall_tl_min, cfg.tall_tl_max)
        grass_fill = place_blades(cfg, rng, flow_angle_field, flow_curv_field,
                                  cfg.n_fill,
                                  cfg.fill_w_min,  cfg.fill_w_max,
                                  cfg.fill_l_min,  cfg.fill_l_max,
                                  cfg.fill_tl_min, cfg.fill_tl_max)
        leaf_seeds = place_blades(cfg, rng, flow_angle_field, flow_curv_field,
                                  n_leaf,
                                  cfg.leaf_w_min,  cfg.leaf_w_max,
                                  cfg.leaf_l_min,  cfg.leaf_l_max,
                                  0.0, 0.0)   # tip_len=0: ovate profile spans full length

        for s in grass_tall: s['veg_type'] = 'grass'
        for s in grass_fill: s['veg_type'] = 'grass'
        for s in leaf_seeds: s['veg_type'] = 'leaf'

        seeds = grass_tall + grass_fill + leaf_seeds

        # Downstream-first sort
        mfx = float(np.mean(np.sin(flow_angle_field)))
        mfy = float(np.mean(np.cos(flow_angle_field)))
        seeds.sort(key=lambda b: -(mfx * b['base_x'] + mfy * b['base_y']))

        n_seeds = len(seeds)
        if verbose:
            spread_deg = np.degrees(cfg.tuft_spread) / 2.0
            print(f"Placed {n_seeds} seeds  "
                  f"({n_grass} grass [{cfg.tuft_min}–{cfg.tuft_max} blades, "
                  f"±{spread_deg:.0f}°], "
                  f"{n_leaf} leaf [{cfg.leaf_tuft_min}–{cfg.leaf_tuft_max} blades], "
                  f"ratio {cfg.grass_ratio}:{cfg.leaf_ratio})")

        # ── Build loop ─────────────────────────────────────────────────────────
        parts: List[trimesh.Trimesh] = []
        built_blades   = 0
        built_tufts    = 0
        skipped_tufts  = 0
        placed_data: list = []   # (blade_idx, spine, hw, up_locs) for strict check
        blade_global_idx  = 0

        for i, seed in enumerate(seeds):
            bx, by   = seed['base_x'], seed['base_y']
            veg_type = seed['veg_type']

            # ── Fan angles: type-specific tuft size and spread ─────────────────
            if veg_type == 'grass':
                n_in_tuft = int(tuft_rng.integers(cfg.tuft_min, cfg.tuft_max + 1))
                half_fan  = cfg.tuft_spread / 2.0
            else:  # 'leaf'
                n_in_tuft = int(tuft_rng.integers(cfg.leaf_tuft_min,
                                                   cfg.leaf_tuft_max + 1))
                half_fan  = cfg.leaf_tuft_spread / 2.0

            offsets    = (np.linspace(-half_fan, half_fan, n_in_tuft)
                          if n_in_tuft > 1 else np.array([0.0]))
            curls      = tuft_rng.uniform(-cfg.curl_max, cfg.curl_max, n_in_tuft)
            min_curl   = cfg.curl_min_fraction * cfg.curl_max
            if min_curl > 0.0:
                small = np.abs(curls) < min_curl
                signs = np.sign(curls)
                signs[signs == 0.0] = tuft_rng.choice([-1.0, 1.0], int(np.sum(signs == 0.0)))
                curls[small] = signs[small] * min_curl
            directions = seed['direction'] + offsets

            # ── Rotate whole fan away from any violated edge ───────────────────
            reach  = (seed['length'] + seed['tip_len']) * 0.65
            margin = seed['width'] / 2.0 + 0.5
            rot    = _find_edge_rotation(bx, by, directions, reach, cfg, margin)
            directions = directions + rot

            # ── Build each blade in the tuft (commit immediately) ─────────────
            tuft_blade_count = 0

            for direction, curl in zip(directions, curls):
                repair_floor = None
                accepted     = None

                for _rep in range(cfg.collision_repair_passes + 1):
                    try:
                        blade_mesh, sub_hull, spine, widths = make_vegetation_blade(
                            cfg             = cfg,
                            support_z       = scene.support_z,
                            terrain_z       = scene.terrain_z,
                            base_pos        = (bx, by),
                            azimuth         = float(direction),
                            length          = seed['length'],
                            width           = seed['width'],
                            tip_length      = seed['tip_len'],
                            curl            = float(curl),
                            extra_floor_z   = repair_floor,
                            vegetation_type = veg_type,
                        )
                    except RuntimeError:
                        break

                    if not blade_footprint_inside_tile(cfg, spine, widths):
                        break

                    hw      = widths / 2.0
                    up_locs = compute_up_locs(spine)
                    hits    = (collect_strict_hits(spine, hw, up_locs, placed_data,
                                                   cfg.strict_base_t)
                               if cfg.strict_mode else [])

                    if not hits:
                        accepted = (blade_mesh, sub_hull, spine, widths, up_locs)
                        break

                    if repair_floor is None:
                        repair_floor = np.full(len(spine), -np.inf, dtype=float)
                    add_collision_repairs(repair_floor, spine, hits, cfg.clearance)

                if accepted is None:
                    continue   # this fan member didn't fit; try next

                blade_mesh, sub_hull, spine, widths, up_locs = accepted
                parts.append(blade_mesh)
                parts.append(sub_hull)
                built_blades    += 1
                tuft_blade_count += 1

                hw = widths / 2.0
                if cfg.strict_mode:
                    log_strict_hits(blade_global_idx, bx, by, spine,
                                    collect_strict_hits(spine, hw, up_locs,
                                                        placed_data, cfg.strict_base_t))
                    placed_data.append((blade_global_idx, spine, hw, up_locs))

                rasterise_into_support(scene.support_z, cfg, spine, hw)
                blade_global_idx += 1

            # ── Tuft accounting ────────────────────────────────────────────────
            if tuft_blade_count > 0:
                built_tufts += 1
            else:
                skipped_tufts += 1

            if verbose and ((i + 1) % 10 == 0 or (i + 1) == n_seeds):
                print(f"  {i + 1}/{n_seeds} tufts  ({built_blades} blades built)")

        if verbose:
            if skipped_tufts:
                print(f"  skipped {skipped_tufts} tuft(s) that could not fit")
            print(f"  built {built_blades} blades in {built_tufts}/{n_seeds} tufts")
            _print_height_audit(placed_data, scene.terrain_z, cfg)

        return parts


def _print_height_audit(placed_data: list,
                         terrain_z: np.ndarray, cfg: TileConfig) -> None:
    """Print a percentile summary of blade rise heights above local terrain."""
    from ..core.grid import sample_grid as sg
    rises = []
    for _blade_idx, spine, hw, up_locs in placed_data:
        # spine[0] is the blade base — use its XY to sample local terrain height.
        base_tz = float(sg(terrain_z, cfg, float(spine[0, 0]), float(spine[0, 1])))
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
