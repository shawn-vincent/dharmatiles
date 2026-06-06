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

Slope assumptions
-----------------
Two places in this layer assume the terrain is locally horizontal:

1. **Blade root sinking** — the seed's root is sunk a fixed distance below
   ``terrain_z`` along world -Z (``z_base = terrain_z - root_depth``).  On a
   slope the sink should instead be along the terrain-surface normal so the
   base does not poke out of the hillside.  Correction: subtract
   ``root_depth * terrain_normal(x, y)`` from the base position.

2. **rise_cap check** — the bridge/blocking test compares the absolute Δz
   between consecutive blade segments against ``rise_cap``.  On a slope the
   natural terrain already contributes a Δz per step (≈ seg_len × tan θ), so
   the effective cap is artificially tightened uphill and loosened downhill.
   Correction: compare Δ along the terrain-normal direction, not raw Δz.

For the grass-and-water tile these do not bite: grass only grows in the flat
5 mm zone; the slope strip is masked off via ``grass_mask``.

When slope-zone grass is needed, see ``TileScene.terrain_normal`` (to be
implemented) for the per-cell normal helper.
"""
from __future__ import annotations

import numpy as np
import trimesh
from typing import List

from scipy.ndimage import gaussian_filter1d, binary_erosion

from ..core.config import SceneConfig, SurfaceConfig, GrassConfig, SolverConfig
from ..core.tile import TileScene
from ..core.grid import sample_grid, rasterise_into_support
from ..core.mesh import build_tube_mesh
from ..core.seed import GrassSeed, make_seed


# ── Path smoother ─────────────────────────────────────────────────────────────

def _smooth_path(path_arr: np.ndarray, n_out: int, sigma: float) -> np.ndarray:
    """Gaussian-smooth a grown path then resample at *n_out* uniform arc points.

    The Gaussian filter provides C∞ smoothness; the resampling step uses
    piecewise-linear arc-length interpolation (``np.interp``), which is ~15×
    faster than a PCHIP spline and indistinguishable at 0.4 mm print resolution
    since the knot spacing (≈ 0.8 mm per growth segment) is already sub-nozzle.
    """
    if len(path_arr) < 2:
        return path_arr

    smoothed = np.stack([
        gaussian_filter1d(path_arr[:, i], sigma=sigma, mode='nearest')
        for i in range(3)
    ], axis=1)
    smoothed[0]  = path_arr[0]   # pin base
    smoothed[-1] = path_arr[-1]  # pin tip

    diffs   = np.diff(smoothed, axis=0)
    seg_len = np.linalg.norm(diffs, axis=1)
    t_knots = np.concatenate([[0.0], np.cumsum(seg_len)])
    if t_knots[-1] < 1e-9:
        return smoothed
    t_knots /= t_knots[-1]
    t_out = np.linspace(0.0, 1.0, n_out)

    cols = [np.interp(t_out, t_knots, smoothed[:, i]) for i in range(3)]
    return np.stack(cols, axis=1)




# ── Grass region edge fill ───────────────────────────────────────────────────

def _fill_boundary_seeds(live: list, occ_z: np.ndarray,
                         scene: TileScene,
                         surface: SurfaceConfig, grass: GrassConfig,
                         solver: SolverConfig,
                         flow_angle_field: np.ndarray,
                         rng: np.random.Generator) -> int:
    """Plant individual seeds along the inner edge of the grass region.

    The inner edge is the one-cell-thick ring of grass cells that are
    adjacent to at least one non-grass cell.  Seeds are spaced roughly one
    average blade-width apart so bare spots at the region boundary get filled
    without overcrowding.

    Returns the number of seeds planted.
    """
    grass_mask = scene.grass_mask
    if grass_mask is None:
        return 0

    # Inner edge: grass cells that disappear after a 4-connected erosion
    eroded = binary_erosion(grass_mask, structure=np.array([[0,1,0],[1,1,1],[0,1,0]]))
    edge_mask = grass_mask & ~eroded

    edge_rows, edge_cols = np.where(edge_mask)
    if len(edge_rows) == 0:
        return 0

    # Subsample: one seed per average-blade-width along the edge
    avg_blade_w = (grass.width_min + grass.width_max) / 2.0
    cell_size   = surface.cell_w
    step        = max(1, round(avg_blade_w / cell_size))

    # Shuffle so fill-seeds don't all point the same direction
    order = rng.permutation(len(edge_rows))

    n_planted = 0
    for i in order[::step]:
        r  = int(edge_rows[i])
        c  = int(edge_cols[i])
        bx = (c + 0.5) * surface.cell_w
        by = (r + 0.5) * surface.cell_w

        if scene.stone_mask is not None and scene.stone_mask[r, c]:
            continue

        blade_curl = float(rng.uniform(-grass.curl_max, grass.curl_max))
        seed       = make_seed(blade_curl, grass, solver, rng)
        blade_dir  = float(sample_grid(flow_angle_field, surface, bx, by))
        blade_dir += float(rng.normal(0.0, grass.group_dir_jitter))

        tz   = float(sample_grid(scene.terrain_z, surface, bx, by))
        sz   = float(sample_grid(occ_z,           surface, bx, by))
        if sz > tz + solver.max_stack_height:
            continue
        sink = seed.width * seed.spine_sink_fraction
        z0   = max(tz, sz) + solver.clearance - sink

        live.append({
            'seed':    seed,
            'path':    [(bx, by, z0)],
            'dir':     blade_dir,
            'alive':   True,
            'base_tz': tz,
        })
        _stamp(occ_z, surface, bx, by, z0 + seed.width / 2.0, seed.width / 2.0)
        n_planted += 1

    return n_planted


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
    iy0 = max(0, int((y - hw) / surface.cell_w))
    iy1 = min(surface.grid_h - 1, int((y + hw) / surface.cell_w) + 1)
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

        min_curl    = grass.curl_min_fraction * grass.curl_max
        max_stack_h = solver.max_stack_height
        occ_z       = scene.support_z.copy()

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
                iy = int(np.clip(int(by / surface.cell_w), 0, surface.grid_h - 1))
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
                if sz > tz + max_stack_h:
                    continue
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

        # ── Edge fill ──────────────────────────────────────────────────────────
        n_fill = _fill_boundary_seeds(live, occ_z, scene, surface, grass,
                                      solver, flow_angle_field, rng)
        if verbose and n_fill > 0:
            print(f"  Edge fill: {n_fill} extra blades along region boundary")

        # Precompute turn offsets: [+15°, −15°, +30°, −30°, +45°, −45°]
        turn_offsets: list = []
        for k in range(1, self.n_turn_tries // 2 + 1):
            a = np.radians(self.turn_step_deg * k)
            turn_offsets += [a, -a]

        # Cache frequently-accessed scalars to avoid repeated attribute lookups
        cw   = surface.cell_w
        tw   = surface.tile_w
        th   = surface.tile_h
        gw   = surface.grid_w
        gh   = surface.grid_h
        stone_mask  = scene.stone_mask
        terrain_z   = scene.terrain_z

        # ── Growth rounds ──────────────────────────────────────────────────────
        # Each blade uses its own seed's rise_cap, seg_len, and max_segs.
        for round_idx in range(grass.max_segs):
            grown = 0
            for bi in rng.permutation(len(live)):
                entry = live[bi]
                if not entry['alive']:
                    continue

                seed = entry['seed']
                if round_idx >= seed.max_segs:
                    entry['alive'] = False
                    continue

                cx, cy, cz = entry['path'][-1]
                hw         = seed.width / 2.0

                entry['dir'] += seed.curl * np.pi / seed.max_segs
                direction = entry['dir']

                accepted = None
                for offset in [0.0] + turn_offsets:
                    d  = direction + offset
                    tx = cx + seed.seg_len * np.sin(d)
                    ty = cy + seed.seg_len * np.cos(d)

                    if not (hw < tx < tw - hw and hw < ty < th - hw):
                        continue

                    if stone_mask is not None:
                        s_ix = int(np.clip(int(tx / cw), 0, gw - 1))
                        s_iy = int(np.clip(int(ty / cw), 0, gh - 1))
                        if stone_mask[s_iy, s_ix]:
                            continue

                    tz_t = float(sample_grid(terrain_z, surface, tx, ty))
                    sz_t = float(sample_grid(occ_z,     surface, tx, ty))
                    if sz_t > tz_t + max_stack_h:
                        continue
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

            # Underground anchor — placed root_depth below the blade's actual
            # starting z (not below terrain_z) so elevated blades don't produce
            # a near-vertical column between anchor and base.
            root_x, root_y = raw_arr[0, 0], raw_arr[0, 1]
            underground = np.array([[root_x, root_y,
                                     raw_arr[0, 2] - seed.root_depth]])
            raw_arr = np.vstack([underground, raw_arr])

            # Ground the tip to the surface it was growing on at the end of
            # growth.  nz = surface + clearance - sink, so surface = nz + sink
            # - clearance.  This is deterministic (derived from the blade's own
            # last z) and correct for both bottom blades (surface = terrain) and
            # blades that grew elevated on the occupancy pile (surface = pile top).
            raw_arr[-1, 2] = raw_arr[-1, 2] + (seed.width * seed.spine_sink_fraction) - seed.clearance

            n_smooth = max(seed.n_path, len(raw_arr))
            path_arr = _smooth_path(raw_arr, n_smooth, seed.smooth_sigma)

            tip_l  = max(total_l * 0.1875, seed.seg_len)
            body_l = total_l - tip_l
            s_arr  = np.linspace(0.0, total_l, n_smooth)
            t_tip  = np.clip((s_arr - body_l) / (tip_l + 1e-9), 0.0, 1.0)
            widths = seed.width * (0.25 + 0.75 * np.cos(t_tip * np.pi / 2.0))

            # Per-point sink correction: growth used a fixed seed.width/2 sink,
            # but the desired sink is widths[i]/2 at each point — so the tip
            # (width→0) is raised back up, keeping the blade top visible throughout.
            # Underground anchor (index 0) has widths[0] ≈ seed.width → correction ≈ 0.
            path_arr = path_arr.copy()
            path_arr[:, 2] += (seed.width / 2.0 - widths / 2.0)

            # Clip spine XY so the tube cross-section stays inside the tile
            # footprint.  The tube extends up to seed.width/2 from the spine
            # in the horizontal plane, so keep the spine at least that far
            # from every wall.  The main violators are edge-fill seeds planted
            # right on the region boundary and the underground anchor (which
            # inherits the seed's XY and sits below the terrain slab).  Both
            # are buried, so bending them inward is invisible and correct.
            _hw = seed.width / 2.0
            path_arr[:, 0] = np.clip(path_arr[:, 0], _hw, surface.tile_w - _hw)
            path_arr[:, 1] = np.clip(path_arr[:, 1], _hw, surface.tile_h - _hw)

            mesh = build_tube_mesh(path_arr, widths, seed.thickness,
                                   cross_section=seed.cross_section,
                                   n_segs=seed.circle_segs,
                                   diamond_equator=seed.diamond_equator,
                                   leaf_arch=seed.leaf_arch,
                                   leaf_ridge=seed.leaf_ridge)
            parts.append(mesh)

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

        # ── Hard tile-boundary enforcement ────────────────────────────────────
        # No grass vertex may protrude outside the tile's XY footprint.
        # The spine clip above handles most cases; this catches support-cone
        # Hermite spline overshoot, tip-cone drift, and any floating-point
        # residual from smoothing.  Violations are always tiny (< 0.1 mm) so
        # clamping is invisible at print resolution and correct by construction.
        tw, th = surface.tile_w, surface.tile_h
        n_clipped = 0
        for mesh in parts:
            v = mesh.vertices
            bad_x = (v[:, 0] < 0.0) | (v[:, 0] > tw)
            bad_y = (v[:, 1] < 0.0) | (v[:, 1] > th)
            n_clipped += int(bad_x.sum() + bad_y.sum())
            np.clip(v[:, 0], 0.0, tw, out=v[:, 0])
            np.clip(v[:, 1], 0.0, th, out=v[:, 1])
        if verbose and n_clipped:
            print(f"  Boundary clamp: {n_clipped} vertex coordinates clamped "
                  f"to tile footprint")

        return parts
