"""ThatchGrass — draped-blade grass dressing (mound-thatch design, E2/E3).

Experimental greenfield replacement for the FloppyGrass field simulation
(design: docs/design/grass-mound-thatch.md).  No growth simulation: blade
spine paths are *synthesized* — a curled 2-D arc draped on a smoothed copy
of the current terrain (the mound substrate) — then handed to the existing
``grass.mesh.build_meshes``, which lifts each blade over previously placed
blades (vegetation_support_z) and stamps its top surface back in.  Paths
are processed lowest-root-first so a higher-rooted blade can never end up
under a lower-rooted one (root-z-sorted standoff accumulation).

Direction = blend(down-slope of the smoothed substrate, coherent positional
angle field), so blades comb down mound flanks and swirl coherently on
flats.  Knobs are module constants while the look iterates (leaf-era rule).
"""
from __future__ import annotations

import numpy as np
from scipy.ndimage import distance_transform_edt, gaussian_filter

from ..core.config import GrassConfig, SpeciesConfig
from ..core.tile import derive_seed
from ..dist import D, sample
from ._geometry import _sample_grid
from .seed import GrassPath, GrassSeed


def _default_species() -> SpeciesConfig:
    """The accepted "bushy" default grass (Shawn, 2026-07-03): short plump
    curled blades with a long tip taper and a 4-facet peaked-but-plump top."""
    return SpeciesConfig(
        blade_width=D[1.4:2.0],
        blade_length=D[5:9],
        blade_curl=D[0.15:0.5],
        blade_clearance=0.15,
        blade_thickness=0.8,
        blade_taper=4.0,
        blade_top_facets=4,
    )

# ── Module constants (promote to config once the look settles) ───────────────
_SPACING_MM        = 3.0    # jittered-grid SHEAF-site spacing
_SHEAF_MIN         = 4      # blades per sheaf (inclusive range)
_SHEAF_MAX         = 9
_SHEAF_FAN_MM      = 1.0    # lateral root spacing between blades in a sheaf
_FIELD_SCALE_MM    = 6.0    # angle-field coherence wavelength (~sheaf width)
_DOWNSLOPE_WEIGHT  = 0.6    # 0 = pure angle field, 1 = pure down-slope
_SLOPE_REF         = 0.20   # slope magnitude that counts as "fully steep"
_SMOOTH_MM         = 1.0    # substrate smoothing for draping (bridges carpet texture)
_EDGE_MARGIN_MM    = 0.0    # spines may reach the tile edge exactly; blade
                            # side overhang is clamped to the tile wall after
                            # meshing (REQ-OUT-2), so grass runs edge-to-edge
_SIZE_JITTER_LO    = 0.75   # downward-only size jitter (leaf-era iter-3 lesson)
_SKIRT_HEIGHT_MM   = 0.8    # soil berm rising against rock bases (turf line climbs)
_SKIRT_WIDTH_MM    = 2.5    # gaussian falloff of the skirt from the footprint edge
_ROCK_CLIMB_MM     = 1.3    # blades may ride up a rock's lower slope this far
_ROCK_TIP_STEPS    = 2      # only the blade TIP may enter a footprint: at most
                            # this many steps (~1 mm) inside, then stop — the
                            # tip leans on the stone, the body never crosses it
_ROCK_TIP_RISE_MM  = 0.9    # tips may only enter footprint cells this low —
                            # steep flank cells would sink blade edges >1 mm in
_ROCK_STANDOFF_MM  = 0.2    # extra clearance riding the stone surface


class ThatchGrass:
    """Dense draped-blade dressing over the mound substrate."""

    height_default_mm: float = 5.0

    def __init__(self, species: SpeciesConfig | None = None, *,
                 max_stack_height: float = 1.2):
        self.species = species or _default_species()
        self.max_stack_height = max_stack_height

    def apply(self, scene, *, placement_mask: np.ndarray | None = None) -> list:
        surface = scene.surface
        species = self.species
        seed = derive_seed(surface.seed, 'thatch-grass')
        rng  = np.random.default_rng(seed)
        cfg  = GrassConfig(species=species,
                           max_stack_height=self.max_stack_height,
                           seed=seed)

        np.maximum(scene.vegetation_support_z, scene.terrain_support_z,
                   out=scene.vegetation_support_z)

        gh, gw  = scene.terrain_z.shape
        cell_w  = surface.cell_w
        tile_w  = (gw - 1) * cell_w
        tile_h  = (gh - 1) * cell_w

        # ── Soil skirt against obstacle bases (rock-lapping) ─────────────────
        # A feathered berm rises against each rock so the turf line climbs
        # the stone instead of meeting it at a flat waterline.  Raising the
        # cells under the footprint too buries the rock base slightly —
        # intended (iceberg read).  Max gradient ≈ H·0.86/W ≈ 15°: FDM-safe.
        dist_mm = None
        if scene.obstacle_mask is not None and scene.obstacle_mask.any():
            dist_mm = distance_transform_edt(~scene.obstacle_mask) * cell_w
            skirt = _SKIRT_HEIGHT_MM * np.exp(-(dist_mm / _SKIRT_WIDTH_MM) ** 2)
            skirt[skirt < 1e-3] = 0.0
            skirt[scene.obstacle_mask] = 0.0   # rim berm only — don't bury the rock further
            # displace_terrain resyncs support=terrain, which would erase the
            # rocks' stamped heights — preserve their absolute support.
            old_support = scene.terrain_support_z.copy()
            scene.displace_terrain(skirt, placement_mask)
            np.maximum(scene.terrain_support_z, old_support,
                       out=scene.terrain_support_z)
            np.maximum(scene.vegetation_support_z, scene.terrain_support_z,
                       out=scene.vegetation_support_z)

        # ── Drape surface + direction fields ─────────────────────────────────
        smooth = gaussian_filter(scene.terrain_z, sigma=_SMOOTH_MM / cell_w)

        # Rock climb: blades may walk up an obstacle's lower slope until it
        # rises _ROCK_CLIMB_MM above the substrate, then stop.  The tip rests
        # ON the stone (supported — FDM-safe) instead of stopping dead at a
        # bare moat.  terrain_support_z carries the rock surface heights.
        rock_rise = scene.terrain_support_z - scene.terrain_z
        passable  = rock_rise <= _ROCK_CLIMB_MM
        # "Wall" cells (too steep even for a leaning tip) deflect blades
        # instead of stopping them: the spine slides along the wall tangent,
        # flowing around the stone the way parted grass does.  (An earlier
        # dilated keep-out simply carved a bare moat around every rock.)
        wall = rock_rise > _ROCK_TIP_RISE_MM
        away_x = away_y = None
        if dist_mm is not None:
            ay, ax = np.gradient(dist_mm, cell_w, cell_w)   # points away from rock
            an = np.hypot(ax, ay) + 1e-9
            away_x, away_y = ax / an, ay / an
        gy, gx = np.gradient(smooth, cell_w, cell_w)

        s_field = max(1.0, _FIELD_SCALE_MM / cell_w)
        fa = gaussian_filter(rng.standard_normal((gh, gw)), sigma=s_field)
        fb = gaussian_filter(rng.standard_normal((gh, gw)), sigma=s_field)
        norm = np.hypot(fa, fb) + 1e-9
        fx, fy = fa / norm, fb / norm                     # coherent unit field

        slope = np.hypot(gx, gy)
        s_w   = _DOWNSLOPE_WEIGHT * np.clip(slope / _SLOPE_REF, 0.0, 1.0)
        dsx   = -gx / (slope + 1e-9)                      # down-slope unit
        dsy   = -gy / (slope + 1e-9)
        vx = s_w * dsx + (1.0 - s_w) * fx
        vy = s_w * dsy + (1.0 - s_w) * fy
        theta_field = np.arctan2(vy, vx)

        # ── Roots: jittered grid inside mask, off obstacles ──────────────────
        nx = max(2, int(round(tile_w / _SPACING_MM)))
        ny = max(2, int(round(tile_h / _SPACING_MM)))
        bx = (np.arange(nx) + 0.5) * (tile_w / nx)
        by = (np.arange(ny) + 0.5) * (tile_h / ny)
        rx, ry = np.meshgrid(bx, by)
        rx = (rx + rng.uniform(-0.5, 0.5, rx.shape) * (tile_w / nx)).ravel()
        ry = (ry + rng.uniform(-0.5, 0.5, ry.shape) * (tile_h / ny)).ravel()

        ci = np.clip((rx / cell_w).astype(int), 0, gw - 1)
        cj = np.clip((ry / cell_w).astype(int), 0, gh - 1)
        ok = np.ones(len(rx), dtype=bool)
        if placement_mask is not None:
            ok &= placement_mask[cj, ci]
        # A sheaf site on a rock is RELOCATED just outside the wall along
        # the outward gradient, not dropped — dropping deletes 4-9 blades
        # of coverage at once, which is what kept carving a moat.
        if away_x is not None:
            blocked = ok & ~passable[cj, ci]
            for k in np.where(blocked)[0]:
                x_, y_ = rx[k], ry[k]
                for _ in range(8):
                    i_ = int(np.clip(x_ / cell_w, 0, gw - 1))
                    j_ = int(np.clip(y_ / cell_w, 0, gh - 1))
                    if passable[j_, i_]:
                        break
                    x_ += away_x[j_, i_] * 0.5
                    y_ += away_y[j_, i_] * 0.5
                i_ = int(np.clip(x_ / cell_w, 0, gw - 1))
                j_ = int(np.clip(y_ / cell_w, 0, gh - 1))
                if passable[j_, i_] and (placement_mask is None or placement_mask[j_, i_]):
                    rx[k], ry[k], ci[k], cj[k] = x_, y_, i_, j_
                else:
                    ok[k] = False
        else:
            ok &= passable[cj, ci]
        rx, ry, ci, cj = rx[ok], ry[ok], ci[ok], cj[ok]

        # Sort roots by substrate height, lowest first (structural layering).
        order = np.argsort(smooth[cj, ci], kind='stable')
        rx, ry, ci, cj = rx[order], ry[order], ci[order], cj[order]

        # ── Synthesize sheaves: bundles of parallel blades per site ──────────
        # A sheaf shares direction, curl sign/strength, and base size, so the
        # bundle reads as one combed lock (the reference's visible unit).
        seg = species.blade_segment_length
        blade_specs = []          # (x0, y0, root_z, width, n_steps, curl_step, heading)
        for sx, sy, i0, j0 in zip(rx, ry, ci, cj):
            k = int(rng.integers(_SHEAF_MIN, _SHEAF_MAX + 1))
            sheaf_dir  = float(theta_field[j0, i0]
                               + sample(species.blade_direction_jitter, rng))
            sheaf_size = rng.uniform(_SIZE_JITTER_LO, 1.0)
            sheaf_len  = float(sample(species.blade_length, rng)) * sheaf_size
            sheaf_wid  = float(sample(species.blade_width, rng)) * sheaf_size
            curl_total = float(sample(species.blade_curl, rng)) * np.pi
            curl_sign  = 1.0 if rng.random() < 0.5 else -1.0
            px_, py_   = -np.sin(sheaf_dir), np.cos(sheaf_dir)   # perp unit
            offs = (np.arange(k) - (k - 1) / 2.0) * _SHEAF_FAN_MM
            for o in offs:
                x0 = sx + px_ * o + rng.normal(0, 0.25)
                y0 = sy + py_ * o + rng.normal(0, 0.25)
                if not (_EDGE_MARGIN_MM < x0 < tile_w - _EDGE_MARGIN_MM and
                        _EDGE_MARGIN_MM < y0 < tile_h - _EDGE_MARGIN_MM):
                    continue
                b_len = sheaf_len * rng.uniform(0.85, 1.05)
                n_steps = max(3, int(round(b_len / seg)))
                curl_step = curl_sign * (curl_total / n_steps) * rng.uniform(0.85, 1.15)
                heading = sheaf_dir + rng.normal(0, 0.05)
                cap_eff = self.max_stack_height * (
                    1.5 if (dist_mm is not None and dist_mm[j0, i0] < 3.0) else 1.0)
                blade_specs.append((x0, y0, sheaf_wid * rng.uniform(0.9, 1.1),
                                    n_steps, curl_step, heading, cap_eff))

        # ── Crowding ring: blades hugging each obstacle rim ──────────────────
        # Tangential blades lying ALONG the annulus cover the collar band;
        # blades aimed at the stone just deflect off it again.
        if dist_mm is not None:
            ring = ((dist_mm > 0.2) & (dist_mm < 3.0) & passable)
            if placement_mask is not None:
                ring &= placement_mask
            cells = np.argwhere(ring)
            if len(cells):
                n_ring = max(4, int(len(cells) * (cell_w ** 2) / 0.8))
                pick = cells[rng.choice(len(cells), size=min(n_ring, len(cells)),
                                        replace=False)]
                for j0, i0 in pick:
                    x0 = i0 * cell_w + rng.uniform(-0.5, 0.5) * cell_w
                    y0 = j0 * cell_w + rng.uniform(-0.5, 0.5) * cell_w
                    tangent = (np.arctan2(away_y[j0, i0], away_x[j0, i0])
                               + (np.pi / 2 if rng.random() < 0.5 else -np.pi / 2))
                    heading = tangent + rng.normal(0, 0.4)
                    b_len = float(sample(species.blade_length, rng)) * rng.uniform(0.75, 1.05)
                    n_steps = max(3, int(round(b_len / seg)))
                    curl_total = float(sample(species.blade_curl, rng)) * np.pi
                    curl_step = (curl_total / n_steps) * (1 if rng.random() < 0.5 else -1)
                    blade_specs.append((x0, y0,
                                        float(sample(species.blade_width, rng)) * rng.uniform(0.9, 1.1),
                                        n_steps, curl_step, heading,
                                        self.max_stack_height * 1.5))

        paths: list[GrassPath] = []
        for x0, y0, blade_width, n_steps, curl_step, heading, cap_eff in blade_specs:
            xs = [x0]; ys = [y0]
            h = heading
            for _ in range(n_steps):
                nxp = xs[-1] + seg * np.cos(h)
                nyp = ys[-1] + seg * np.sin(h)
                if not (_EDGE_MARGIN_MM < nxp < tile_w - _EDGE_MARGIN_MM and
                        _EDGE_MARGIN_MM < nyp < tile_h - _EDGE_MARGIN_MM):
                    break
                pi = int(nxp / cell_w); pj = int(nyp / cell_w)
                if placement_mask is not None and not placement_mask[pj, pi]:
                    break
                if wall[pj, pi]:
                    # Slide along the wall: deflect toward the tangent that
                    # best preserves the heading, biased slightly outward.
                    if away_x is None:
                        break
                    cpi = int(xs[-1] / cell_w); cpj = int(ys[-1] / cell_w)
                    awx, awy = away_x[cpj, cpi], away_y[cpj, cpi]
                    t1x, t1y = -awy, awx            # two tangents
                    t2x, t2y = awy, -awx
                    hx, hy = np.cos(h), np.sin(h)
                    tx_, ty_ = ((t1x, t1y) if (t1x*hx + t1y*hy) >= (t2x*hx + t2y*hy)
                                else (t2x, t2y))
                    h = np.arctan2(0.8*ty_ + 0.2*awy, 0.8*tx_ + 0.2*awx)
                    nxp = xs[-1] + seg * np.cos(h)
                    nyp = ys[-1] + seg * np.sin(h)
                    if not (_EDGE_MARGIN_MM < nxp < tile_w - _EDGE_MARGIN_MM and
                            _EDGE_MARGIN_MM < nyp < tile_h - _EDGE_MARGIN_MM):
                        break
                    pi = int(nxp / cell_w); pj = int(nyp / cell_w)
                    if ((placement_mask is not None and not placement_mask[pj, pi])
                            or wall[pj, pi]):
                        break
                xs.append(nxp); ys.append(nyp)
                h += curl_step
            if len(xs) < 4:
                continue

            pxs = np.asarray(xs); pys = np.asarray(ys)
            # Drape on the substrate; where the path enters a rock fringe it
            # rides ON the stone's surface (entry is gated by the climb cap
            # and tip-step limit, so this can't walk up the whole rock).
            # Capping below the surface instead would embed the blade INSIDE
            # the stone — the "grass growing into the rock" artifact.
            zs_sub = _sample_grid(smooth, surface, pxs, pys)
            zs_sup = _sample_grid(scene.terrain_support_z, surface, pxs, pys)
            on_rock = zs_sup > zs_sub + 0.05
            zs = (np.where(on_rock, zs_sup + _ROCK_STANDOFF_MM, zs_sub)
                  + species.blade_clearance)
            pts = list(zip(pxs.tolist(), pys.tolist(), zs.tolist()))
            # Root ring sits thickness below the surface (grow.py convention).
            pts[0] = (pts[0][0], pts[0][1],
                      float(zs[0] - species.blade_clearance - species.blade_thickness))

            gseed = GrassSeed(
                x=float(x0), y=float(y0),
                blade_direction=heading,
                blade_segment_length=seg,
                blade_n_steps=len(pts) - 1,
                blade_taper=float(species.blade_taper),
                blade_base_width=float(species.blade_base_width),
                blade_base_taper=float(species.blade_taper
                                       if species.blade_base_taper is None
                                       else species.blade_base_taper),
                blade_curl=curl_step,
                blade_width=blade_width,
                blade_rise_cap=float(cap_eff),   # per-blade stack cap (relaxed near rocks)
                blade_clearance=float(species.blade_clearance),
                species_id=species.name,
            )
            paths.append(GrassPath(seed=gseed, points=pts))

        return _build_draped(paths, cfg, scene, surface)


# ── Mesh build (variant of grass.mesh.build_meshes) ──────────────────────────
#
# Same per-blade lift → build → stamp loop, with one change: when a blade is
# lifted onto previously placed blades it rides a *clearance gap* above them
# (max(planned, support + lift_clearance)) instead of landing exactly on the
# support.  Exact contact made side-by-side blades coalesce into melted
# sheets; the gap keeps every blade distinct ("each leaf proud of its
# neighbours", leaf-era lesson).  Kept local so the shared build_meshes
# (still used by the old grower) is untouched during the experiment.

_LIFT_CLEARANCE_MM = 0.25


def _build_draped(paths, cfg, scene, surface):
    import trimesh
    from ._geometry import _spine_distances, _stamp_segment
    from .grower import FlatGrassGrower
    from .mesh import _lift_path_points
    from ..core.color import Material, tag as _tag

    import dataclasses

    species = cfg.species
    # NOTE on the crown rule ("stones sit higher than the grass"): it is
    # enforced structurally by the wall gate — no blade can climb more than
    # _ROCK_TIP_RISE_MM up a stone, so grass can never overtop a crown.
    # An explicit crown *ceiling* (truncating nearby blades below the crown)
    # was tried twice and re-carved the moat both times: the annulus blades
    # that cover the collar are exactly the ones such a ceiling kills.
    # Lateral de-penetration setup: how deep inside an obstacle footprint a
    # point is (mm), and the outward direction (gradient of that depth).
    inside_mm = go_x = go_y = None
    if scene.obstacle_mask is not None and scene.obstacle_mask.any():
        cw_ = surface.cell_w
        inside_mm = distance_transform_edt(scene.obstacle_mask) * cw_
        gi_y, gi_x = np.gradient(inside_mm, cw_, cw_)      # points inward
        gn = np.hypot(gi_x, gi_y) + 1e-9
        go_x, go_y = -gi_x / gn, -gi_y / gn                # outward unit
    meshes: list[trimesh.Trimesh] = []
    for path in paths:
        pts = np.asarray(path.points, dtype=float)
        planned = pts[1:, 2].copy()
        floor = _sample_grid(scene.vegetation_support_z, surface,
                             pts[1:, 0], pts[1:, 1])
        need_lift = floor > pts[1:, 2] - 1e-9
        pts[1:, 2] = np.where(need_lift, floor + _LIFT_CLEARANCE_MM, pts[1:, 2])

        # Stack cap (R6): ride height above the draped substrate is bounded.
        # Truncate at the first point that would exceed the cap; drop the
        # blade if too little remains.  Prevents lift-over-lift "tent peaks"
        # (the recurring height-blowup failure mode).
        over = (pts[1:, 2] - planned) > path.seed.blade_rise_cap
        if over.any():
            cut = int(np.argmax(over)) + 1        # first offending point index
            if cut < 4:
                continue
            pts = pts[:cut]
        seed_adj = dataclasses.replace(path.seed, blade_n_steps=len(pts) - 1)
        lifted = GrassPath(seed=seed_adj,
                           points=[tuple(p) for p in pts.tolist()])

        mesh = FlatGrassGrower.build_mesh(lifted, species, scene, surface)
        if mesh is not None:
            # Clamp side overhang to the tile wall: spines may run right to
            # the boundary, so blade flanks can poke past it — slice them
            # flat at the edge (stays closed; REQ-OUT-2).
            tw = (surface.grid_w - 1) * surface.cell_w
            th = (surface.grid_h - 1) * surface.cell_w
            v = mesh.vertices.copy()
            v[:, 0] = np.clip(v[:, 0], 0.0, tw)
            v[:, 1] = np.clip(v[:, 1], 0.0, th)
            # No grass inside stone: a blade-EDGE vertex that falls inside
            # an obstacle footprint below the stone's surface is pushed
            # horizontally OUT of the footprint (depth + margin along the
            # outward gradient) — the edge bends around the stone the way
            # parted grass does.  Lifting was wrong twice over: near a tall
            # wall it would hoist ground verts to the crest, and it put
            # grass above the crowns.  Verts riding ON the stone (tips) sit
            # above the surface sample and are untouched.
            if inside_mm is not None:
                depth = _sample_grid(inside_mm, surface, v[:, 0], v[:, 1])
                sup_v = _sample_grid(scene.terrain_support_z, surface,
                                     v[:, 0], v[:, 1])
                cw_ = surface.cell_w
                vi = np.clip((v[:, 0] / cw_).astype(int), 0, inside_mm.shape[1] - 1)
                vj = np.clip((v[:, 1] / cw_).astype(int), 0, inside_mm.shape[0] - 1)
                need = (depth > 0.01) & (v[:, 2] < sup_v - 0.05)
                if need.any():
                    push = np.clip(depth[need] + 0.15, 0.0, 1.8)
                    v[need, 0] += go_x[vj[need], vi[need]] * push
                    v[need, 1] += go_y[vj[need], vi[need]] * push
                    v[:, 0] = np.clip(v[:, 0], 0.0, tw)
                    v[:, 1] = np.clip(v[:, 1], 0.0, th)
            mesh.vertices = v
            _tag(mesh, Material.GRASS)
            meshes.append(mesh)

        path_dists = _spine_distances(pts)
        total_len  = float(path_dists[-1])
        tapers = path.seed.distance_taper_vec(path_dists, total_len)
        thick  = species.blade_thickness * tapers
        width  = path.seed.blade_width * tapers
        for idx in range(1, len(pts)):
            _stamp_segment(
                scene.vegetation_support_z, surface,
                float(pts[idx-1][0]), float(pts[idx-1][1]),
                float(pts[idx][0]),   float(pts[idx][1]),
                float(width[idx-1]),  float(width[idx]),
                float(pts[idx-1][2]), float(pts[idx][2]),
                float(thick[idx-1]),  float(thick[idx]),
                species.blade_top_facets,
            )
    return meshes
