"""
GrassLayer: grass that grows segment-by-segment from planted seeds.

Instead of placing a complete blade all at once, each blade starts from a seed
point and advances one segment per growth round according to these rules:

  1. Try to continue in the current direction, staying at terrain/occupancy level.
     Z at each new point = max(terrain_z, occupancy_z) + clearance.

  2. If the required rise for the preferred direction exceeds ``rise_cap``, try
     turning left and right (±15°, ±30°, ±45°) to find a less-blocked path.
     Blades steer around obstacles rather than piling straight up.

  3. If no direction is viable (all require too much rise), the blade stops.

  4. Blades stop at the tile boundary.

All blades grow simultaneously, round-by-round.  Within a round the occupancy
field is updated after each blade's step, so later blades in the same round
already see earlier blades.  Processing order is shuffled each round.

Each blade is grown from a :class:`~dharmatiles.core.seed.GrassSeed` which
carries every parameter the growth algorithm needs.  The layer does not read
from ``SceneConfig`` during growth.
"""
from __future__ import annotations

import numpy as np
import trimesh
from typing import List

from scipy.interpolate import PchipInterpolator
from scipy.ndimage import gaussian_filter1d

from ..core.config import SceneConfig, SurfaceConfig, GrassConfig, SolverConfig
from ..core.tile import TileScene
from ..core.grid import sample_grid, rasterise_into_support
from ..core.mesh import build_tube_mesh, blade_frame
from ..core.seed import GrassSeed, make_seed


# ── Path smoother ─────────────────────────────────────────────────────────────

def _smooth_path(path_arr: np.ndarray, n_out: int, sigma: float) -> np.ndarray:
    """Gaussian-smooth a grown path then resample as a C¹ PCHIP spline."""
    if len(path_arr) < 2:
        return path_arr

    smoothed = np.stack([
        gaussian_filter1d(path_arr[:, i], sigma=sigma, mode='nearest')
        for i in range(3)
    ], axis=1)
    smoothed[0] = path_arr[0]   # pin base

    diffs   = np.diff(smoothed, axis=0)
    seg_len = np.linalg.norm(diffs, axis=1)
    t_knots = np.concatenate([[0.0], np.cumsum(seg_len)])
    if t_knots[-1] < 1e-9:
        return smoothed
    t_knots /= t_knots[-1]
    t_out = np.linspace(0.0, 1.0, n_out)

    cols = [PchipInterpolator(t_knots, smoothed[:, i])(t_out) for i in range(3)]
    return np.stack(cols, axis=1)


# ── Tip upturn ───────────────────────────────────────────────────────────────

def _apply_tip_upturn(path_arr: np.ndarray, taper_idx: int) -> np.ndarray:
    """If the blade tip points downward, curl the taper section upward."""
    n = len(path_arr)
    if taper_idx < 1 or taper_idx >= n - 2:
        return path_arr

    tip_raw = path_arr[-1] - path_arr[-2]
    tip_len = float(np.linalg.norm(tip_raw))
    if tip_len < 1e-9 or tip_raw[2] / tip_len >= 0.0:
        return path_arr

    t0 = path_arr[taper_idx + 1] - path_arr[taper_idx]
    t0_len = float(np.linalg.norm(t0))
    if t0_len < 1e-9:
        return path_arr
    t0 = t0 / t0_len

    t1 = np.array([t0[0], t0[1], 0.1], dtype=float)
    t1 /= np.linalg.norm(t1)

    L = float(np.sum(np.linalg.norm(np.diff(path_arr[taper_idx:], axis=0), axis=1)))
    if L < 1e-6:
        return path_arr

    p0   = path_arr[taper_idx].astype(float)
    avg  = (t0 + t1) / 2.0
    avg /= np.linalg.norm(avg)
    p1   = p0 + avg * L

    n_taper = n - taper_idx
    tv  = np.linspace(0.0, 1.0, n_taper)
    m0  = t0 * L
    m1  = t1 * L
    h00 =  2*tv**3 - 3*tv**2 + 1
    h10 =    tv**3 - 2*tv**2 + tv
    h01 = -2*tv**3 + 3*tv**2
    h11 =    tv**3 - tv**2

    new_taper = (h00[:, None] * p0 + h10[:, None] * m0 +
                 h01[:, None] * p1 + h11[:, None] * m1)

    out = path_arr.copy()
    out[taper_idx:] = new_taper
    return out


# ── Bridge-support posts ──────────────────────────────────────────────────────

_HORIZ_THRESHOLD = float(np.sin(np.radians(45 * 0.9)))


def _make_support_post(surface: SurfaceConfig, grass: GrassConfig,
                       cx: float, cy: float,
                       z_top: float,
                       blade_width: float,
                       terrain_z: np.ndarray,
                       rng: np.random.Generator) -> trimesh.Trimesh:
    """Curved grass-blade tip rising from the terrain to the supported blade."""
    n_pts = 20

    angle  = rng.uniform(0.0, 2.0 * np.pi)
    offset = rng.uniform(0.3, 2.0)
    hw = blade_width / 2.0
    bx = float(np.clip(cx + offset * np.cos(angle), hw, surface.tile_w - hw))
    by = float(np.clip(cy + offset * np.sin(angle), hw, surface.tile_h - hw))

    sink   = blade_width / 2.0 + grass.thickness
    z_base = float(sample_grid(terrain_z, surface, bx, by)) - sink

    p0 = np.array([bx, by, z_base], dtype=float)
    p1 = np.array([cx, cy, z_top],  dtype=float)

    chord = float(np.linalg.norm(p1 - p0)) or 1e-6
    m1 = np.array([0.0, 0.0, chord])

    to_tip      = p1 - p0
    to_tip_norm = to_tip / (np.linalg.norm(to_tip) + 1e-9)
    h_len = float(np.hypot(to_tip[0], to_tip[1]))
    if h_len > 1e-6:
        perp = np.array([-to_tip[1] / h_len, to_tip[0] / h_len, 0.0])
    else:
        perp = np.array([1.0, 0.0, 0.0])

    curl = rng.uniform(-1.0, 1.0)
    m0   = chord * (to_tip_norm + perp * curl * 0.5)
    m0[2] = max(float(m0[2]), chord * 0.25)

    t   = np.linspace(0.0, 1.0, n_pts)
    h00 =  2*t**3 - 3*t**2 + 1
    h10 =    t**3 - 2*t**2 + t
    h01 = -2*t**3 + 3*t**2
    h11 =    t**3 - t**2

    path   = (h00[:, None] * p0 + h10[:, None] * m0 +
              h01[:, None] * p1 + h11[:, None] * m1)
    widths = blade_width * np.cos(t * np.pi / 2.0)

    return build_tube_mesh(path, widths, grass.thickness,
                           cross_section=grass.cross_section,
                           n_segs=grass.circle_segs,
                           diamond_equator=grass.diamond_equator)


def _blade_tip_cone(surface: SurfaceConfig, grass: GrassConfig,
                    solver: SolverConfig,
                    path_arr: np.ndarray,
                    widths: np.ndarray,
                    terrain_z: np.ndarray,
                    taper_idx: int,
                    rng: np.random.Generator) -> trimesh.Trimesh | None:
    tangs, _, down_locs = blade_frame(path_arr)
    cx  = float(path_arr[taper_idx, 0])
    cy_ = float(path_arr[taper_idx, 1])
    if abs(tangs[taper_idx, 2]) >= _HORIZ_THRESHOLD:
        return None
    z_ground    = float(sample_grid(terrain_z, surface, cx, cy_))
    z_underside = float(path_arr[taper_idx, 2]
                        + grass.thickness * down_locs[taper_idx, 2])
    z_spine     = float(path_arr[taper_idx, 2])
    if z_underside <= z_ground + solver.clearance:
        return None
    return _make_support_post(surface, grass, cx, cy_, z_spine,
                              float(widths[taper_idx]), terrain_z, rng)


def _blade_support_cones(surface: SurfaceConfig, grass: GrassConfig,
                          solver: SolverConfig,
                          path_arr: np.ndarray,
                          widths: np.ndarray,
                          terrain_z: np.ndarray,
                          max_bridge_mm: float,
                          rng: np.random.Generator) -> List[trimesh.Trimesh]:
    n_pts    = len(path_arr)
    tangs, up_locs, down_locs = blade_frame(path_arr)
    underside_z = path_arr[:, 2] + grass.thickness * down_locs[:, 2]
    ground_z = np.array([
        sample_grid(terrain_z, surface, float(path_arr[i, 0]), float(path_arr[i, 1]))
        for i in range(n_pts)
    ])
    gap = underside_z - ground_z

    seg_lens = np.linalg.norm(np.diff(path_arr, axis=0), axis=1)
    arc_s    = np.concatenate([[0.0], np.cumsum(seg_lens)])
    airborne = gap > solver.clearance + 0.05

    cones: List[trimesh.Trimesh] = []
    i = 0
    while i < n_pts:
        if not airborne[i]:
            i += 1
            continue
        j = i + 1
        while j < n_pts and airborne[j]:
            j += 1

        span_start_s = arc_s[i]
        span_end_s   = arc_s[min(j, n_pts - 1)]
        span_len     = span_end_s - span_start_s

        if span_len > max_bridge_mm:
            n_cones = int(span_len / max_bridge_mm)
            for k in range(1, n_cones + 1):
                target_s = span_start_s + (k - 0.5) * (span_len / n_cones)
                ci = int(np.searchsorted(arc_s, target_s))
                ci = int(np.clip(ci, 0, n_pts - 1))
                cx       = float(path_arr[ci, 0])
                cy_      = float(path_arr[ci, 1])
                z_ground = float(sample_grid(terrain_z, surface, cx, cy_))
                if (underside_z[ci] > z_ground + solver.clearance
                        and abs(tangs[ci, 2]) < _HORIZ_THRESHOLD):
                    cones.append(_make_support_post(
                        surface, grass, cx, cy_, float(path_arr[ci, 2]),
                        float(widths[ci]), terrain_z, rng,
                    ))
        i = j
    return cones


# ── Jittered-grid group placement ─────────────────────────────────────────────

def _jittered_group_centers(n: int, surface: SurfaceConfig, grass: GrassConfig,
                             flow_angle_field: np.ndarray,
                             rng: np.random.Generator,
                             margin: float) -> list:
    """Place *n* group centers on a jittered grid."""
    tw = surface.tile_w
    th = surface.tile_h
    cols = max(1, round(np.sqrt(n * tw / th)))
    rows = int(np.ceil(n / cols))
    cw   = (tw - 2 * margin) / cols
    ch   = (th - 2 * margin) / rows

    candidates = []
    for r in range(rows):
        for c in range(cols):
            x = margin + (c + rng.uniform(0.1, 0.9)) * cw
            y = margin + (r + rng.uniform(0.1, 0.9)) * ch
            candidates.append((float(x), float(y)))

    chosen = [candidates[i] for i in rng.permutation(len(candidates))[:n]]
    width  = float(rng.uniform(grass.width_min, grass.width_max))

    return [
        {'base_x': x, 'base_y': y,
         'direction': float(sample_grid(flow_angle_field, surface, x, y)),
         'width': width}
        for x, y in chosen
    ]


# ── Occupancy stamp ───────────────────────────────────────────────────────────

def _stamp(occ_z: np.ndarray, surface: SurfaceConfig,
           x: float, y: float, z: float, hw: float) -> None:
    ix0 = max(0, int((x - hw) / surface.cell_w))
    ix1 = min(surface.grid_w - 1, int((x + hw) / surface.cell_w) + 1)
    iy0 = max(0, int((y - hw) / surface.cell_h))
    iy1 = min(surface.grid_h - 1, int((y + hw) / surface.cell_h) + 1)
    np.maximum(occ_z[iy0:iy1 + 1, ix0:ix1 + 1], z,
               out=occ_z[iy0:iy1 + 1, ix0:ix1 + 1])


# ── GrassLayer ───────────────────────────────────────────────────────────

class GrassLayer:
    """Grow grass blades horizontally, segment-by-segment.

    Reads from ``SceneConfig`` only at construction (to extract sub-configs).
    Growth uses only the planted ``GrassSeed`` objects — no live config reads.
    """

    # Growth steering constants (not in GrassConfig — layer-level algorithm knobs)
    turn_step_deg: float = 15.0
    n_turn_tries:  int   = 6

    def __init__(self, cfg: SceneConfig) -> None:
        self.surface = cfg.surface
        self.grass   = cfg.grass
        self.solver  = cfg.solver
        # Layer-level group params may be overridden after construction
        n_squares            = cfg.surface.cols * cfg.surface.rows
        self.n_groups        = cfg.grass.groups_per_square * n_squares
        self.group_min       = cfg.grass.group_min
        self.group_max       = cfg.grass.group_max
        self.group_spread_mm = cfg.grass.group_spread_mm
        self.group_dir_jitter= cfg.grass.group_dir_jitter
        self.max_bridge_mm   = cfg.grass.max_bridge_mm

    def build(self,
              scene: TileScene,
              flow_angle_field: np.ndarray,
              flow_curv_field: np.ndarray,
              verbose: bool = True) -> List[trimesh.Trimesh]:

        surface = self.surface
        grass   = self.grass
        solver  = self.solver
        rng     = np.random.default_rng(surface.seed ^ 0x47524F57)   # 'GROW'

        # ── Plant seeds in groups ──────────────────────────────────────────────
        if self.n_groups == 0:
            return []
        edge = grass.width_max / 2.0 + 0.5
        group_centers = _jittered_group_centers(
            self.n_groups, surface, grass, flow_angle_field, rng, edge,
        )

        min_curl = grass.curl_min_fraction * grass.curl_max
        occ_z    = scene.support_z.copy()

        # Each entry: seed + live growth state
        live: list = []   # list of {'seed': GrassSeed, 'path': list, 'dir': float, 'alive': bool, 'base_tz': float}

        for gc in group_centers:
            n_in_group = int(rng.integers(self.group_min, self.group_max + 1))
            group_dir  = gc['direction']
            group_curl = float(rng.uniform(-grass.curl_max, grass.curl_max))
            if abs(group_curl) < min_curl:
                group_curl = (float(np.sign(group_curl) * min_curl) if group_curl != 0.0
                              else float(rng.choice([-min_curl, min_curl])))

            for _ in range(n_in_group):
                ang  = rng.uniform(0.0, 2.0 * np.pi)
                dist = rng.uniform(0.0, self.group_spread_mm)
                bx   = float(np.clip(gc['base_x'] + dist * np.cos(ang),
                                     edge, surface.tile_w - edge))
                by   = float(np.clip(gc['base_y'] + dist * np.sin(ang),
                                     edge, surface.tile_h - edge))

                # Reject seeds under stones or outside the grass region
                ix = int(np.clip(int(bx / surface.cell_w), 0, surface.grid_w - 1))
                iy = int(np.clip(int(by / surface.cell_h), 0, surface.grid_h - 1))
                if scene.stone_mask is not None and scene.stone_mask[iy, ix]:
                    continue
                if scene.grass_mask is not None and not scene.grass_mask[iy, ix]:
                    continue

                blade_dir  = group_dir + float(rng.normal(0.0, self.group_dir_jitter))
                blade_curl = float(np.clip(
                    group_curl + rng.normal(0.0, grass.curl_max * 0.08),
                    -grass.curl_max, grass.curl_max,
                ))

                seed = make_seed(blade_curl, grass, solver, rng)
                # Override width from group width for cluster coherence
                seed = GrassSeed(
                    **{**seed.__dict__,
                       'width': gc['width']}
                )

                tz   = float(sample_grid(scene.terrain_z, surface, bx, by))
                sz   = float(sample_grid(occ_z,           surface, bx, by))
                sink = seed.width * seed.spine_sink_fraction
                z0   = max(tz, sz) + solver.clearance - sink

                live.append({
                    'seed':    seed,
                    'path':    [(bx, by, z0)],
                    'dir':     blade_dir,
                    'alive':   True,
                    'base_tz': tz,
                })
                # Stamp the actual blade top so stacking works correctly
                _stamp(occ_z, surface, bx, by,
                       z0 + seed.width / 2.0, seed.width / 2.0)

        if verbose:
            print(f"  Planted {len(live)} blades in {len(group_centers)} groups")

        # Precompute turn offsets: [+15°, −15°, +30°, −30°, +45°, −45°]
        turn_offsets: list = []
        for k in range(1, self.n_turn_tries // 2 + 1):
            a = np.radians(self.turn_step_deg * k)
            turn_offsets += [a, -a]

        # ── Growth rounds ──────────────────────────────────────────────────────
        # Each blade uses its own seed's rise_cap and seg_len.
        # max_segs is the global round limit (use the grass config value).
        for round_idx in range(grass.max_segs):
            grown = 0
            for bi in rng.permutation(len(live)):
                entry = live[bi]
                if not entry['alive']:
                    continue

                seed = entry['seed']
                cx, cy, cz = entry['path'][-1]
                hw         = seed.width / 2.0

                entry['dir'] += seed.curl * np.pi / grass.max_segs
                direction = entry['dir']

                accepted = None
                for offset in [0.0] + turn_offsets:
                    d  = direction + offset
                    tx = cx + seed.seg_len * np.sin(d)
                    ty = cy + seed.seg_len * np.cos(d)

                    if not (hw < tx < surface.tile_w - hw and
                            hw < ty < surface.tile_h - hw):
                        continue

                    # Treat stone footprint as a hard wall — steer around it
                    if scene.stone_mask is not None:
                        s_ix = int(np.clip(int(tx / surface.cell_w), 0, surface.grid_w - 1))
                        s_iy = int(np.clip(int(ty / surface.cell_h), 0, surface.grid_h - 1))
                        if scene.stone_mask[s_iy, s_ix]:
                            continue

                    tz_t = float(sample_grid(scene.terrain_z, surface, tx, ty))
                    sz_t = float(sample_grid(occ_z,           surface, tx, ty))
                    sink = seed.width * seed.spine_sink_fraction
                    nz   = max(tz_t, sz_t) + seed.clearance - sink

                    if nz - cz <= seed.rise_cap:
                        accepted = (tx, ty, nz, d)
                        break

                if accepted is None:
                    entry['alive'] = False
                    continue

                tx, ty, nz, nd = accepted
                entry['path'].append((tx, ty, nz))
                entry['dir'] = nd
                grown += 1
                # Stamp the actual blade top for occupancy
                _stamp(occ_z, surface, tx, ty, nz + seed.width / 2.0, hw)

            if verbose:
                alive = sum(1 for b in live if b['alive'])
                print(f"  Round {round_idx + 1:2d}: "
                      f"{grown:3d} segments grown, {alive:3d} blades still alive")
            if grown == 0:
                break

        # ── Build meshes ───────────────────────────────────────────────────────
        parts: List[trimesh.Trimesh] = []
        for entry in live:
            path  = entry['path']
            seed  = entry['seed']
            if len(path) < 2:
                continue

            raw_arr  = np.array(path, dtype=float)
            n_raw    = len(raw_arr)
            total_l  = seed.seg_len * (n_raw - 1)

            # Underground anchor
            root_x, root_y = raw_arr[0, 0], raw_arr[0, 1]
            underground = np.array([[root_x, root_y,
                                     entry['base_tz'] - seed.root_depth]])
            raw_arr = np.vstack([underground, raw_arr])

            n_smooth = max(seed.n_path, len(raw_arr))
            path_arr = _smooth_path(raw_arr, n_smooth, seed.smooth_sigma)

            tip_l  = max(total_l * 0.25, seed.seg_len)
            body_l = total_l - tip_l
            s_arr  = np.linspace(0.0, total_l, n_smooth)
            t_tip  = np.clip((s_arr - body_l) / (tip_l + 1e-9), 0.0, 1.0)
            widths = seed.width * np.cos(t_tip * np.pi / 2.0)

            # Per-point sink correction: growth used a fixed seed.width/2 sink,
            # but the desired sink is widths[i]/2 at each point — so the tip
            # (width→0) is raised back up, keeping the blade top visible throughout.
            # Underground anchor (index 0) has widths[0] ≈ seed.width → correction ≈ 0.
            path_arr = path_arr.copy()
            path_arr[:, 2] += (seed.width / 2.0 - widths / 2.0)

            upturn_idx = int(np.clip(
                np.searchsorted(s_arr, body_l), 1, n_smooth - 2))
            path_arr = _apply_tip_upturn(path_arr, upturn_idx)

            mesh = build_tube_mesh(path_arr, widths, seed.thickness,
                                   cross_section=seed.cross_section,
                                   n_segs=seed.circle_segs,
                                   diamond_equator=seed.diamond_equator)
            parts.append(mesh)

            cones = _blade_support_cones(
                surface, grass, solver, path_arr, widths,
                scene.terrain_z, self.max_bridge_mm, rng,
            )
            parts.extend(cones)

            taper_idx = int(np.searchsorted(s_arr, body_l + 0.05 * total_l))
            taper_idx = int(np.clip(taper_idx, 0, len(path_arr) - 1))
            tip_cone  = _blade_tip_cone(
                surface, grass, solver, path_arr, widths,
                scene.terrain_z, taper_idx, rng,
            )
            if tip_cone is not None:
                parts.append(tip_cone)

            rasterise_into_support(scene.support_z, surface,
                                   path_arr, widths / 2.0)

        if verbose:
            living = [e for e in live if len(e['path']) >= 2]
            segs   = [len(e['path']) - 1 for e in living]
            if segs:
                seg_len = self.grass.seg_len
                print(f"  Built {len(living)} blades — "
                      f"avg {np.mean(segs):.1f} segs "
                      f"({np.mean(segs) * seg_len:.1f} mm), "
                      f"max {max(segs)} segs "
                      f"({max(segs) * seg_len:.1f} mm)")

        return parts
