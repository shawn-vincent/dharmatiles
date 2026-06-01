"""
GrownGrassLayer: grass that grows segment-by-segment from planted seeds.

Instead of placing a complete blade all at once, each blade starts from a seed
point and advances one segment per growth round according to these rules:

  1. Try to continue in the current direction, staying at terrain/occupancy level.
     Z at each new point = max(terrain_z, occupancy_z) + clearance.

  2. If the required rise for the preferred direction exceeds `rise_cap`, try
     turning left and right (±15°, ±30°, ±45°) to find a less-blocked path.
     Blades steer around obstacles rather than piling straight up.

  3. If no direction is viable (all require too much rise), the blade stops.

  4. Blades stop at the tile boundary.

All blades grow simultaneously, round-by-round.  Within a round the occupancy
field is updated after each blade's step, so later blades in the same round
already see earlier blades — this drives the filling behaviour (blades grow into
empty spaces first, rising only when no horizontal path exists).  Processing
order is shuffled each round to prevent systematic bias.
"""
from __future__ import annotations

import numpy as np
import trimesh
from typing import List

from scipy.interpolate import PchipInterpolator
from scipy.ndimage import gaussian_filter1d

from ..core.tile import TileConfig, TileScene
from ..core.grid import sample_grid, rasterise_into_support
from ..core.mesh import build_tube_mesh, blade_frame
from .grass import place_blades          # reuse zone-weighted seed placement


# ── Path smoother ─────────────────────────────────────────────────────────────

def _smooth_path(path_arr: np.ndarray, n_out: int, sigma: float) -> np.ndarray:
    """Gaussian-smooth a grown path then resample as a C¹ PCHIP spline.

    Two-stage process:
      1. Gaussian-filter each coordinate (sigma in segment units).  This rounds
         out the per-segment kinks and terrain bumps into broad, sweeping arcs
         while preserving the overall shape of the grown path.
      2. Arc-length-parameterized PCHIP through the smoothed points, resampled
         at *n_out* for a clean, high-resolution tube mesh.

    Parameters
    ----------
    path_arr : (N, 3) float — raw grown path (piecewise-linear).
    n_out    : number of output sample points.
    sigma    : Gaussian smoothing width in segment units.  Larger = broader arcs.
    """
    if len(path_arr) < 2:
        return path_arr

    # Stage 1: Gaussian smooth — rounds out per-step noise into broad curves
    smoothed = np.stack([
        gaussian_filter1d(path_arr[:, i], sigma=sigma, mode='nearest')
        for i in range(3)
    ], axis=1)
    smoothed[0] = path_arr[0]   # pin base to terrain — blade grows from a fixed root

    # Stage 2: arc-length PCHIP through the smoothed control points
    diffs   = np.diff(smoothed, axis=0)
    seg_len = np.linalg.norm(diffs, axis=1)
    t_knots = np.concatenate([[0.0], np.cumsum(seg_len)])
    if t_knots[-1] < 1e-9:
        return smoothed
    t_knots /= t_knots[-1]
    t_out = np.linspace(0.0, 1.0, n_out)

    cols = [PchipInterpolator(t_knots, smoothed[:, i])(t_out) for i in range(3)]
    return np.stack(cols, axis=1)


# ── Bridge-support cones ──────────────────────────────────────────────────────

def _make_support_post(cfg: TileConfig,
                       cx: float, cy: float,
                       z_bottom: float, z_top: float,
                       blade_width: float) -> trimesh.Trimesh:
    """Vertical cylinder from terrain up to the blade underside.

    Diameter = 75% of the blade width, constant along the full height,
    centred on the blade spine.
    """
    n_pts  = 10
    diam   = blade_width * 0.75
    path   = np.column_stack([
        np.full(n_pts, cx),
        np.full(n_pts, cy),
        np.linspace(z_bottom, z_top, n_pts),
    ])
    widths = np.full(n_pts, diam)

    return build_tube_mesh(path, widths, cfg.grass_thickness,
                           cross_section='circle',
                           n_segs=cfg.blade_circle_segs)


def _blade_tip_cone(cfg: TileConfig,
                     path_arr: np.ndarray,
                     widths: np.ndarray,
                     terrain_z: np.ndarray,
                     taper_idx: int) -> trimesh.Trimesh | None:
    """One cone at the taper-start index — anchors the tip before it goes thin.

    Returns None if the blade is already sitting on terrain at that point
    (no floating region to support).
    """
    _, _, down_locs = blade_frame(path_arr)
    cx  = float(path_arr[taper_idx, 0])
    cy_ = float(path_arr[taper_idx, 1])
    z_ground    = float(sample_grid(terrain_z, cfg, cx, cy_))
    z_underside = float(path_arr[taper_idx, 2]
                        + cfg.grass_thickness * down_locs[taper_idx, 2])
    z_spine     = float(path_arr[taper_idx, 2])
    if z_underside <= z_ground + cfg.clearance:
        return None
    return _make_support_post(cfg, cx, cy_, z_ground, z_spine,
                               blade_width=float(widths[taper_idx]))


def _blade_support_cones(cfg: TileConfig,
                          path_arr: np.ndarray,
                          widths: np.ndarray,
                          terrain_z: np.ndarray,
                          max_bridge_mm: float) -> List[trimesh.Trimesh]:
    """Return support cones only where the blade spans more than *max_bridge_mm*
    above the terrain without a contact point.

    Algorithm
    ---------
    1. For every point along the smoothed spine, compute the gap between the
       blade underside and the terrain directly below.
    2. Walk the arc-length parameterisation to find contiguous "airborne" spans
       (gap > clearance).
    3. Any airborne span longer than *max_bridge_mm* gets one cone per
       max_bridge_mm interval, placed at the midpoints of each sub-interval.
    """
    n_pts    = len(path_arr)
    _, up_locs, down_locs = blade_frame(path_arr)

    # Blade underside: spine shifted by grass_thickness in the down direction
    underside_z = path_arr[:, 2] + cfg.grass_thickness * down_locs[:, 2]

    # Terrain Z directly below each spine point
    ground_z = np.array([
        sample_grid(terrain_z, cfg, float(path_arr[i, 0]), float(path_arr[i, 1]))
        for i in range(n_pts)
    ])

    gap = underside_z - ground_z

    # Arc-length parameterisation
    seg_lens = np.linalg.norm(np.diff(path_arr, axis=0), axis=1)
    arc_s    = np.concatenate([[0.0], np.cumsum(seg_lens)])

    # Airborne = blade is more than clearance above terrain
    airborne = gap > cfg.clearance + 0.05

    cones: List[trimesh.Trimesh] = []
    i = 0
    while i < n_pts:
        if not airborne[i]:
            i += 1
            continue

        # Find end of this airborne span
        j = i + 1
        while j < n_pts and airborne[j]:
            j += 1

        span_start_s = arc_s[i]
        span_end_s   = arc_s[min(j, n_pts - 1)]
        span_len     = span_end_s - span_start_s

        if span_len > max_bridge_mm:
            n_cones = int(span_len / max_bridge_mm)   # one per full interval
            for k in range(1, n_cones + 1):
                # Place each cone at the centre of its sub-interval
                target_s = span_start_s + (k - 0.5) * (span_len / n_cones)
                ci = int(np.searchsorted(arc_s, target_s))
                ci = int(np.clip(ci, 0, n_pts - 1))

                cx       = float(path_arr[ci, 0])
                cy_      = float(path_arr[ci, 1])
                z_ground = float(sample_grid(terrain_z, cfg, cx, cy_))

                if underside_z[ci] > z_ground + cfg.clearance:
                    cones.append(_make_support_post(
                        cfg, cx, cy_, z_ground, float(path_arr[ci, 2]),
                        blade_width=float(widths[ci]),
                    ))

        i = j

    return cones


# ── Jittered-grid group placement ────────────────────────────────────────────

def _jittered_group_centers(n: int, cfg: TileConfig,
                             flow_angle_field: np.ndarray,
                             rng: np.random.Generator,
                             margin: float) -> list:
    """Place *n* group centers on a jittered grid for even spatial coverage.

    Divides the tile into a cols×rows grid with ≥n cells, places one center
    per cell at a random position within that cell, then shuffles and trims to n.
    Direction is sampled from the flow field at each chosen position.
    """
    cols = max(1, round(np.sqrt(n * cfg.tile_w / cfg.tile_h)))
    rows = int(np.ceil(n / cols))
    cw   = (cfg.tile_w - 2 * margin) / cols
    ch   = (cfg.tile_h - 2 * margin) / rows

    candidates = []
    for r in range(rows):
        for c in range(cols):
            x = margin + (c + rng.uniform(0.1, 0.9)) * cw
            y = margin + (r + rng.uniform(0.1, 0.9)) * ch
            candidates.append((float(x), float(y)))

    chosen = [candidates[i] for i in rng.permutation(len(candidates))[:n]]
    width  = float(rng.uniform(cfg.tall_w_min, cfg.tall_w_max))

    return [
        {'base_x': x, 'base_y': y,
         'direction': float(sample_grid(flow_angle_field, cfg, x, y)),
         'width': width}
        for x, y in chosen
    ]


# ── Occupancy stamp ───────────────────────────────────────────────────────────

def _stamp(occ_z: np.ndarray, cfg: TileConfig,
           x: float, y: float, z: float, hw: float) -> None:
    """Raise occ_z to at least *z* within a square footprint of half-width *hw*."""
    ix0 = max(0, int((x - hw) / cfg.gx))
    ix1 = min(cfg.grid_res - 1, int((x + hw) / cfg.gx) + 1)
    iy0 = max(0, int((y - hw) / cfg.gy))
    iy1 = min(cfg.grid_res - 1, int((y + hw) / cfg.gy) + 1)
    np.maximum(occ_z[iy0:iy1 + 1, ix0:ix1 + 1], z,
               out=occ_z[iy0:iy1 + 1, ix0:ix1 + 1])


# ── GrownGrassLayer ───────────────────────────────────────────────────────────

class GrownGrassLayer:
    """Grow grass blades horizontally, segment-by-segment."""

    seg_len:           float = 0.8    # mm per segment step
    max_segs:          int   = 12     # max segments → up to ~10 mm blade
    rise_cap:          float = 0.8    # mm: max tolerated rise per step; beyond this, turn
    turn_step_deg:     float = 15.0   # degrees per turn-attempt
    n_turn_tries:      int   = 6      # try ±15°, ±30°, ±45° → 6 alternatives
    curl_max:          float = 0.8    # max curl (independent of cfg.curl_max)
    curl_min_fraction: float = 0.65   # every blade curves at least this fraction of max
    smooth_sigma:      float = 2.0    # Gaussian smoothing width in segment units
    root_depth:        float = 2.0    # mm below terrain for the underground anchor point
    n_groups:          int   = 41     # number of grass groups (evenly distributed)
    group_min:         int   = 10     # min blades per group
    group_max:         int   = 15     # max blades per group
    group_spread_mm:   float = 2.5    # radius (mm) to scatter blade bases around group centre
    group_dir_jitter:  float = 0.14   # per-blade direction jitter within group (radians σ)
    max_bridge_mm:     float = 10.0   # max unsupported span before a support cone is added

    def __init__(self, cfg: TileConfig) -> None:
        self.cfg = cfg

    def build(self,
              scene: TileScene,
              flow_angle_field: np.ndarray,
              flow_curv_field: np.ndarray,
              verbose: bool = True) -> List[trimesh.Trimesh]:

        cfg = self.cfg
        rng = np.random.default_rng(cfg.seed ^ 0x47524F57)   # 'GROW'

        # ── Plant seeds in groups ──────────────────────────────────────────────
        # Each group shares a direction and curl; individual blades get a slight
        # per-blade jitter around those shared values.
        edge     = cfg.tall_w_max / 2.0 + 0.5
        group_centers = _jittered_group_centers(
            self.n_groups, cfg, flow_angle_field, rng, edge,
        )

        min_curl = self.curl_min_fraction * self.curl_max
        occ_z    = scene.support_z.copy()
        blades: list = []

        for gc in group_centers:
            n_in_group = int(rng.integers(self.group_min, self.group_max + 1))

            # Group-level shared direction (from flow field) and curl
            group_dir  = gc['direction']
            group_curl = float(rng.uniform(-self.curl_max, self.curl_max))
            if abs(group_curl) < min_curl:
                group_curl = (float(np.sign(group_curl) * min_curl) if group_curl != 0.0
                              else float(rng.choice([-min_curl, min_curl])))

            for _ in range(n_in_group):
                # Scatter blade base around group centre
                ang = rng.uniform(0.0, 2.0 * np.pi)
                dist = rng.uniform(0.0, self.group_spread_mm)
                bx = float(np.clip(gc['base_x'] + dist * np.cos(ang),
                                   edge, cfg.tile_w - edge))
                by = float(np.clip(gc['base_y'] + dist * np.sin(ang),
                                   edge, cfg.tile_h - edge))

                # Small per-blade jitter around group direction and curl
                blade_dir  = group_dir + float(rng.normal(0.0, self.group_dir_jitter))
                blade_curl = float(np.clip(
                    group_curl + rng.normal(0.0, self.curl_max * 0.08),
                    -self.curl_max, self.curl_max,
                ))

                tz = float(sample_grid(scene.terrain_z, cfg, bx, by))
                sz = float(sample_grid(occ_z,           cfg, bx, by))
                z0 = max(tz, sz) + cfg.clearance

                blades.append({
                    'path':    [(bx, by, z0)],
                    'dir':     blade_dir,
                    'curl':    blade_curl,
                    'alive':   True,
                    'width':   gc['width'],
                    'base_tz': tz,
                })
                _stamp(occ_z, cfg, bx, by, z0 + cfg.grass_thickness, gc['width'] / 2.0)

        if verbose:
            print(f"  Planted {len(blades)} blades in {len(group_centers)} groups")

        # Precompute turn-offset sequence: [+15°, −15°, +30°, −30°, +45°, −45°]
        turn_offsets: list = []
        for k in range(1, self.n_turn_tries // 2 + 1):
            a = np.radians(self.turn_step_deg * k)
            turn_offsets += [a, -a]

        # ── Growth rounds ──────────────────────────────────────────────────────
        for round_idx in range(self.max_segs):
            grown = 0

            # Shuffle processing order so no blade has persistent priority
            for bi in rng.permutation(len(blades)):
                blade = blades[bi]
                if not blade['alive']:
                    continue

                cx, cy, cz = blade['path'][-1]
                hw         = blade['width'] / 2.0

                # Apply natural curl drift each step
                blade['dir'] += blade['curl'] * np.pi / self.max_segs
                direction = blade['dir']

                accepted = None
                for offset in [0.0] + turn_offsets:
                    d  = direction + offset
                    tx = cx + self.seg_len * np.sin(d)
                    ty = cy + self.seg_len * np.cos(d)

                    # Stay inside tile
                    if not (hw < tx < cfg.tile_w - hw and hw < ty < cfg.tile_h - hw):
                        continue

                    tz_t = float(sample_grid(scene.terrain_z, cfg, tx, ty))
                    sz_t = float(sample_grid(occ_z,           cfg, tx, ty))
                    nz   = max(tz_t, sz_t) + cfg.clearance

                    if nz - cz <= self.rise_cap:
                        accepted = (tx, ty, nz, d)
                        break   # take the first viable direction

                if accepted is None:
                    blade['alive'] = False
                    continue

                tx, ty, nz, nd = accepted
                blade['path'].append((tx, ty, nz))
                blade['dir'] = nd
                grown += 1

                # Update occupancy immediately so later blades this round see it
                _stamp(occ_z, cfg, tx, ty, nz + cfg.grass_thickness, hw)

            if verbose:
                alive = sum(1 for b in blades if b['alive'])
                print(f"  Round {round_idx + 1:2d}: "
                      f"{grown:3d} segments grown, {alive:3d} blades still alive")
            if grown == 0:
                break

        # ── Build meshes ───────────────────────────────────────────────────────
        parts: List[trimesh.Trimesh] = []
        for blade in blades:
            path = blade['path']
            if len(path) < 2:
                continue

            raw_arr  = np.array(path, dtype=float)
            n_raw    = len(raw_arr)
            total_l  = self.seg_len * (n_raw - 1)

            # Prepend an underground anchor so the smooth curve emerges from
            # below the terrain surface rather than starting flat at ground level.
            root_x, root_y = raw_arr[0, 0], raw_arr[0, 1]
            underground = np.array([[root_x, root_y,
                                     blade['base_tz'] - self.root_depth]])
            raw_arr = np.vstack([underground, raw_arr])

            # Gaussian-smooth then PCHIP-resample: rounds out per-segment terrain
            # bumps into broad arcs while keeping the overall grown shape.
            n_smooth = max(cfg.n_path, len(raw_arr))
            path_arr = _smooth_path(raw_arr, n_smooth, self.smooth_sigma)

            # Width profile over the smoothed point count
            tip_l  = max(total_l * 0.25, self.seg_len)
            body_l = total_l - tip_l
            s_arr  = np.linspace(0.0, total_l, n_smooth)
            t_tip  = np.clip((s_arr - body_l) / (tip_l + 1e-9), 0.0, 1.0)
            widths = blade['width'] * np.cos(t_tip * np.pi / 2.0)

            mesh = build_tube_mesh(path_arr, widths, cfg.grass_thickness,
                                   cross_section=cfg.blade_cross_section,
                                   n_segs=cfg.blade_circle_segs,
                                   diamond_equator=cfg.blade_diamond_equator)
            parts.append(mesh)

            # Span-based support cones — only where the blade spans too far
            # above the terrain without a contact point below it.
            cones = _blade_support_cones(cfg, path_arr, widths,
                                          scene.terrain_z, self.max_bridge_mm)
            parts.extend(cones)

            # Tip cone — one cone at the taper transition to anchor the tip
            # region before the blade width starts reducing toward zero.
            taper_idx = int(np.searchsorted(s_arr, body_l))
            taper_idx = int(np.clip(taper_idx, 0, len(path_arr) - 1))
            tip_cone = _blade_tip_cone(cfg, path_arr, widths,
                                        scene.terrain_z, taper_idx)
            if tip_cone is not None:
                parts.append(tip_cone)

            rasterise_into_support(scene.support_z, cfg, path_arr, widths / 2.0)

        if verbose:
            living = [b for b in blades if len(b['path']) >= 2]
            segs   = [len(b['path']) - 1 for b in living]
            if segs:
                print(f"  Built {len(living)} blades — "
                      f"avg {np.mean(segs):.1f} segs "
                      f"({np.mean(segs) * self.seg_len:.1f} mm), "
                      f"max {max(segs)} segs "
                      f"({max(segs) * self.seg_len:.1f} mm)")

        return parts
