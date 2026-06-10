"""Water surface displacement and volume mesh."""
from __future__ import annotations

import numpy as np
import trimesh
from scipy.ndimage import binary_dilation, binary_erosion, distance_transform_edt

from ..core.config import SurfaceConfig


WATER_RENDER_LIFT_MM = 0.10


class WaterLayer:
    """Reshape the pool floor + bank slope and emit the water volume mesh.

    Used as ``Region(layers=[..., WaterLayer()])``.  Place after any rocks
    in the same region so the ripple displacement can react to rocks
    standing in the water.

    Parameters
    ----------
    embed_mm
        Distance (mm) by which the displacement texture extends past the
        water_mask into the shore zone — keeps the wavy water surface
        smoothly meeting the shore slope.
    height_mm
        Water surface height.  ``None`` → derive from the region's
        terrain_z (its max value over the placement mask, which the
        orchestrator sets to the region's effective height).
    """

    height_default_mm: float = 3.0

    def __init__(self, *, embed_mm: float = 2.0,
                 height_mm: float | None = None) -> None:
        self.embed_mm  = embed_mm
        self.height_mm = height_mm

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        if placement_mask is None or not placement_mask.any():
            return []
        surface = scene.config.surface

        # Region height = max terrain_z over the (still flat) pool region.
        water_height = (self.height_mm if self.height_mm is not None
                        else float(scene.terrain_z[placement_mask].max()))

        # ── Reshape pool floor: extend bank slope inward, then flatten ────────
        scene.terrain_z[:] = _extend_bank_slope_into_pool(
            scene.terrain_z, placement_mask, water_height, surface)
        scene.terrain_z[placement_mask] = 0.0
        scene.terrain_support_z[:] = scene.terrain_z

        # ── Build the water volume mesh ──────────────────────────────────────
        embed_cells = max(1, round(self.embed_mm / surface.cell_w))
        wm_disp_full = binary_dilation(placement_mask, iterations=embed_cells)

        z_disp = make_water_displacement(wm_disp_full, surface)

        # Downsample to ~128 cells/square for performance.
        s = max(1, surface.cells_per_square // 128)
        if s > 1:
            gh, gw = scene.terrain_z.shape
            hn, wn = gh // s, gw // s
            tz = scene.terrain_z[:hn*s, :wn*s].reshape(hn, s, wn, s).mean(axis=(1, 3))
            wm = placement_mask[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
            wm_disp = wm_disp_full[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
            zd = z_disp[:hn*s, :wn*s].reshape(hn, s, wn, s).mean(axis=(1, 3))
            sm = (scene.rock_mask[:hn*s, :wn*s].reshape(hn, s, wn, s).any(axis=(1, 3))
                  if scene.rock_mask is not None else None)
            ds_cell_w = surface.cell_w * s
        else:
            tz = scene.terrain_z
            wm = placement_mask
            wm_disp = wm_disp_full
            zd = z_disp
            sm = scene.rock_mask
            ds_cell_w = surface.cell_w

        zd = zd + make_water_ripple_displacement(
            wm_disp, sm, ds_cell_w, seed=surface.seed ^ 0xC4F7)

        # Outside the pool, raise the water surface to follow terrain_z so
        # the full-tile slab merges with the shore slope.
        h_base = water_height + WATER_RENDER_LIFT_MM
        zd = np.where(wm, zd, np.maximum(tz - h_base, zd))

        hn, wn = wm.shape
        wm_full = np.ones((hn, wn), dtype=bool)
        # Match the terrain's simplify threshold so the flat bottom and any
        # near-planar bands of the rippled top collapse to far fewer triangles.
        simplify_tol = surface.terrain_simplify_threshold or 0.0
        water_mesh = make_water_volume(
            tz, wm_full, water_height,
            surface.tile_w, surface.tile_h,
            z_disp=zd,
            simplify_tolerance=simplify_tol)
        return [water_mesh]


def _extend_bank_slope_into_pool(terrain_z: np.ndarray,
                                  water_mask: np.ndarray,
                                  water_height: float,
                                  surface: SurfaceConfig) -> np.ndarray:
    """Extrapolate the bank slope downward into the pool zone.

    Returns a copy of *terrain_z* with pool cells updated.
    """
    cell_mm = surface.cell_w

    dist_land_to_water = distance_transform_edt(~water_mask)
    band = ~water_mask & (dist_land_to_water >= 1) & (dist_land_to_water <= 5)
    if band.any():
        band_z_mean    = float(terrain_z[band].mean())
        band_dist_mean = float(dist_land_to_water[band].mean()) * cell_mm
        slope_rate     = (band_z_mean - water_height) / max(band_dist_mean, 1e-6)
        slope_rate     = float(np.clip(slope_rate, 0.1, 8.0))
    else:
        slope_rate = 0.8

    water_eroded = binary_erosion(water_mask, border_value=1)
    inner_ring   = water_mask & ~water_eroded

    dist_from_shore = distance_transform_edt(~inner_ring) * cell_mm
    dist_from_shore[~water_mask] = 0.0

    terrain_z_min = 0.0
    slope_dist    = (water_height - terrain_z_min) / max(slope_rate, 1e-6)

    t   = np.clip(dist_from_shore / slope_dist, 0.0, 1.0)
    t_s = t * t

    sloped = water_height - (water_height - terrain_z_min) * t_s

    out = terrain_z.copy()
    out[water_mask] = sloped[water_mask]
    return out


def make_water_displacement(
    water_mask:        np.ndarray,
    surface:           SurfaceConfig,
    feature_scale_mm:  float = 3.0,
    amplitude_mm:      float = 1.0,
) -> np.ndarray:
    """Return a (gh, gw) displacement array for the water surface.

    Generates Gaussian-filtered white noise blurred at feature_scale_mm,
    normalised to amplitude_mm RMS, then multiplied by a pyramid fade that
    is 1.0 at the tile centre and 0.0 at every tile edge.  Non-water cells
    are zero.
    """
    from scipy.ndimage import gaussian_filter

    gh, gw = water_mask.shape
    rng    = np.random.default_rng(surface.seed ^ 0xA9F3C7B1)

    sigma = feature_scale_mm / surface.cell_w
    noise = rng.standard_normal((gh, gw))
    z     = gaussian_filter(noise, sigma=sigma)

    water_vals = z[water_mask]
    std = water_vals.std()
    if std > 0:
        z = z / std * amplitude_mm

    # Conical fade: 1.0 at tile centre, 0.0 at corners
    r_mm      = np.arange(gh, dtype=float) * surface.cell_w
    c_mm      = np.arange(gw, dtype=float) * surface.cell_w
    tile_h_mm = gh * surface.cell_w
    tile_w_mm = gw * surface.cell_w
    dy        = (r_mm - tile_h_mm / 2)[:, None]
    dx        = (c_mm - tile_w_mm / 2)[None, :]
    dist      = np.sqrt(dx ** 2 + dy ** 2)
    max_dist  = np.sqrt((tile_w_mm / 2) ** 2 + (tile_h_mm / 2) ** 2)
    fade      = np.clip(1.0 - dist / max_dist, 0.0, 1.0)
    z        *= fade

    z[~water_mask] = 0.0
    return z


def make_water_ripple_displacement(
    water_mask:     np.ndarray,
    rock_mask:      np.ndarray | None,
    cell_w:         float,
    amplitude_mm:   float = 0.5,
    wavelength_mm:  float = 3.0,
    seg_len_min_mm: float = 10.0,
    seg_len_max_mm: float = 20.0,
    seg_fade_mm:    float = 15.0,
    max_offset_mm:  float = 20.0,
    edge_fade_mm:   float = 15.0,
    arc_smooth_mm:  float = 2.5,
    seed:           int   = 0xD3C2,
) -> np.ndarray:
    """Scattered-segment ripple displacement for the water surface.

    Each segment has a flat-top envelope: full amplitude across the arc body
    (seg_len), then a gradual Hann fade over seg_fade_mm on each end.
    The tile-edge envelope fades all displacement to zero over edge_fade_mm
    inside every tile boundary.  Amplitude scales linearly with proximity:
    100 % at 0 mm offset from source, 0 % at max_offset_mm.

    Segment count is auto-scaled: ~1 segment per 6 mm of source perimeter.
    """
    from scipy.ndimage import distance_transform_edt

    gh, gw = water_mask.shape
    rng    = np.random.default_rng(seed)

    # ── Source: internal shoreline + stone footprints in water ────────────────
    tile_edge = np.zeros((gh, gw), dtype=bool)
    tile_edge[0, :]  = True
    tile_edge[-1, :] = True
    tile_edge[:, 0]  = True
    tile_edge[:, -1] = True

    ext = np.zeros((gh + 2, gw + 2), dtype=bool)
    ext[1:-1, 1:-1] = water_mask
    adj_to_water = ~water_mask & (
        ext[:-2, 1:-1] | ext[2:, 1:-1] |
        ext[1:-1, :-2] | ext[1:-1, 2:]
    )
    internal_shore = adj_to_water & ~tile_edge
    stone_in_water = (rock_mask & water_mask) if rock_mask is not None \
                     else np.zeros((gh, gw), dtype=bool)

    source_mask = internal_shore | stone_in_water
    if not source_mask.any():
        return np.zeros((gh, gw))

    # ── Distance (mm) + nearest-source cell index for every cell ─────────────
    dist_arr, idx = distance_transform_edt(~source_mask, return_indices=True)
    dist_mm   = dist_arr * cell_w
    nearest_r = idx[0].astype(np.int32)
    nearest_c = idx[1].astype(np.int32)

    # ── Smooth the distance field and shore projections ───────────────────────
    # Gaussian blur within the water mask rounds the jagged iso-distance
    # contours (which otherwise exactly trace shore bumps) into smooth arcs.
    # Blurring nearest_r/nearest_c removes Voronoi-ridge seams in the arc
    # envelope so adjacent segments fade continuously rather than jumping.
    from scipy.ndimage import gaussian_filter
    if arc_smooth_mm > 0.0:
        sigma = arc_smooth_mm / cell_w
        w   = water_mask.astype(float)
        wg  = gaussian_filter(w, sigma=sigma)
        dist_smooth = np.where(
            water_mask,
            gaussian_filter(dist_mm * w, sigma=sigma) / np.maximum(wg, 1e-6),
            dist_mm,
        )
        nr_f = gaussian_filter(nearest_r.astype(float), sigma=sigma)
        nc_f = gaussian_filter(nearest_c.astype(float), sigma=sigma)
    else:
        dist_smooth = dist_mm
        nr_f = nearest_r.astype(float)
        nc_f = nearest_c.astype(float)

    # ── Auto-scale segment count to approximate shore perimeter ──────────────
    src_r, src_c = np.where(source_mask)
    n_src        = len(src_r)
    n_segments   = max(4, int(n_src * cell_w / 6.0))

    # ── Random segment parameters ─────────────────────────────────────────────
    chosen   = rng.integers(0, n_src, size=n_segments)
    center_r = src_r[chosen].astype(np.int32)
    center_c = src_c[chosen].astype(np.int32)
    offsets  = rng.uniform(0.0, max_offset_mm, size=n_segments)
    seg_lens = rng.uniform(seg_len_min_mm, seg_len_max_mm, size=n_segments)

    # ── Accumulate segment displacements ──────────────────────────────────────
    disp = np.zeros((gh, gw))

    for i in range(n_segments):
        d_0 = float(offsets[i])
        A   = amplitude_mm * max(0.0, 1.0 - d_0 / max_offset_mm)
        if A == 0.0:
            continue

        # Arc distance using smoothed shore projections — removes Voronoi seams.
        dr = (nr_f - float(center_r[i])) * cell_w
        dc = (nc_f - float(center_c[i])) * cell_w
        shore_dist = np.hypot(dr, dc)

        # Flat-top envelope: 1.0 across the segment body, then a gradual
        # Hann fade over seg_fade_mm on each end (zero-derivative at cutoff).
        half_L    = float(seg_lens[i]) * 0.5
        tail_dist = shore_dist - half_L          # negative inside body, positive in tail
        fade      = np.where(
            shore_dist <= half_L,
            1.0,
            np.where(
                tail_dist <= seg_fade_mm,
                (0.5 * (1.0 + np.cos(np.pi * tail_dist / seg_fade_mm))) ** 0.4,
                0.0,
            ),
        )

        # Full sine using smoothed distance — contours are gentle curves, not
        # shore-parallel zigzags.
        in_band = water_mask & (dist_smooth >= d_0) & (dist_smooth <= d_0 + wavelength_mm)
        disp   += np.where(
            in_band,
            A * fade * np.sin(2.0 * np.pi * (dist_smooth - d_0) / wavelength_mm),
            0.0,
        )

    # ── Edge envelope: fade to zero within edge_fade_mm of each tile boundary ─
    if edge_fade_mm > 0.0:
        row_dist = np.minimum(np.arange(gh), gh - 1 - np.arange(gh)).astype(float) * cell_w
        col_dist = np.minimum(np.arange(gw), gw - 1 - np.arange(gw)).astype(float) * cell_w
        edge_dist = np.minimum(row_dist[:, None], col_dist[None, :])
        t = np.clip(edge_dist / edge_fade_mm, 0.0, 1.0)
        disp *= 0.5 * (1.0 - np.cos(np.pi * t))

    return disp


def _simplify(mesh: trimesh.Trimesh, tolerance_mm: float) -> trimesh.Trimesh:
    """Simplify *mesh* via manifold3d, preserving geometry within tolerance_mm."""
    from manifold3d import Manifold, Mesh
    m = Manifold(mesh=Mesh(
        vert_properties=mesh.vertices.astype('f4'),
        tri_verts=mesh.faces.astype('u4'),
    ))
    m = m.simplify(tolerance_mm)
    r = m.to_mesh()
    return trimesh.Trimesh(vertices=r.vert_properties, faces=r.tri_verts, process=False)


def make_water_volume(
    terrain_z:          np.ndarray,
    water_mask:         np.ndarray,
    water_height:       float,
    tile_w:             float,
    tile_h:             float,
    z_disp:             np.ndarray | None = None,
    simplify_tolerance: float = 0.0,
) -> trimesh.Trimesh:
    """Closed solid spanning the water volume.

    Top face   flat at water_height + z_disp (displaced surface).
    Bottom face terrain_z surface under every water cell.
    Perimeter  walls wherever a water cell borders non-water or the tile edge.
    """
    gh, gw = terrain_z.shape
    cell_x = tile_w / gw
    cell_y = tile_h / gh

    nv_r   = gh + 1
    nv_c   = gw + 1
    n_half = nv_r * nv_c

    # ── Bottom corner z ───────────────────────────────────────────────────────
    # Flat floor at z=0 gives clean vertical perimeter walls.  The riverbed
    # slope (from _extend_bank_slope_into_pool) lives in terrain_z and is
    # present in the union result; having the water volume follow it only
    # created angled walls at the shoreline where the downsampled cells span
    # a range of depths.
    bot_z = np.zeros((nv_r, nv_c))

    # ── Top surface z ─────────────────────────────────────────────────────────
    h = water_height + WATER_RENDER_LIFT_MM
    if z_disp is not None:
        pad   = np.pad(z_disp, 1, mode='edge')
        top_z = h + 0.25 * (pad[:-1, :-1] + pad[:-1, 1:] +
                             pad[1:,  :-1] + pad[1:,  1:])
        top_z = np.maximum(top_z, bot_z)
    else:
        top_z = np.full((nv_r, nv_c), h)

    # ── Vertex buffer ─────────────────────────────────────────────────────────
    x_v = np.broadcast_to((np.arange(nv_c) * cell_x)[None, :], (nv_r, nv_c))
    y_v = np.broadcast_to((np.arange(nv_r) * cell_y)[:, None], (nv_r, nv_c))

    verts = np.empty((2 * n_half, 3))
    verts[:n_half, 0] = x_v.ravel()
    verts[:n_half, 1] = y_v.ravel()
    verts[:n_half, 2] = top_z.ravel()
    verts[n_half:, 0] = x_v.ravel()
    verts[n_half:, 1] = y_v.ravel()
    verts[n_half:, 2] = bot_z.ravel()

    def tv(r, c): return np.asarray(r) * nv_c + np.asarray(c)
    def bv(r, c): return n_half + tv(r, c)

    face_list: list[np.ndarray] = []

    # ── Top and bottom faces ──────────────────────────────────────────────────
    wr, wc = np.where(water_mask)
    t00 = tv(wr,   wc);   t01 = tv(wr,   wc + 1)
    t10 = tv(wr+1, wc);   t11 = tv(wr+1, wc + 1)
    b00 = bv(wr,   wc);   b01 = bv(wr,   wc + 1)
    b10 = bv(wr+1, wc);   b11 = bv(wr+1, wc + 1)

    top_f = np.empty((2 * len(wr), 3), dtype=np.int32)
    top_f[0::2] = np.stack([t00, t01, t11], axis=1)
    top_f[1::2] = np.stack([t00, t11, t10], axis=1)
    face_list.append(top_f)

    bot_f = np.empty((2 * len(wr), 3), dtype=np.int32)
    bot_f[0::2] = np.stack([b00, b11, b01], axis=1)
    bot_f[1::2] = np.stack([b00, b10, b11], axis=1)
    face_list.append(bot_f)

    # ── Perimeter walls ───────────────────────────────────────────────────────
    ext = np.zeros((gh + 2, gw + 2), dtype=bool)
    ext[1:-1, 1:-1] = water_mask

    s_mask = water_mask & ~ext[:-2, 1:-1]
    sr, sc = np.where(s_mask)
    if len(sr):
        sw_s = np.empty((2 * len(sr), 3), dtype=np.int32)
        sw_s[0::2] = np.stack([tv(sr, sc),     bv(sr, sc),     tv(sr, sc+1)], axis=1)
        sw_s[1::2] = np.stack([tv(sr, sc+1),   bv(sr, sc),     bv(sr, sc+1)], axis=1)
        face_list.append(sw_s)

    n_mask = water_mask & ~ext[2:, 1:-1]
    nr, nc_ = np.where(n_mask)
    if len(nr):
        sw_n = np.empty((2 * len(nr), 3), dtype=np.int32)
        sw_n[0::2] = np.stack([tv(nr+1, nc_),  tv(nr+1, nc_+1), bv(nr+1, nc_)],  axis=1)
        sw_n[1::2] = np.stack([tv(nr+1, nc_+1),bv(nr+1, nc_+1), bv(nr+1, nc_)],  axis=1)
        face_list.append(sw_n)

    w_mask = water_mask & ~ext[1:-1, :-2]
    wr2, wc2 = np.where(w_mask)
    if len(wr2):
        sw_w = np.empty((2 * len(wr2), 3), dtype=np.int32)
        sw_w[0::2] = np.stack([tv(wr2, wc2),   tv(wr2+1, wc2), bv(wr2, wc2)],   axis=1)
        sw_w[1::2] = np.stack([tv(wr2+1, wc2), bv(wr2+1, wc2), bv(wr2, wc2)],   axis=1)
        face_list.append(sw_w)

    e_mask = water_mask & ~ext[1:-1, 2:]
    er, ec = np.where(e_mask)
    if len(er):
        sw_e = np.empty((2 * len(er), 3), dtype=np.int32)
        sw_e[0::2] = np.stack([tv(er, ec+1),   bv(er, ec+1),   tv(er+1, ec+1)], axis=1)
        sw_e[1::2] = np.stack([tv(er+1, ec+1), bv(er, ec+1),   bv(er+1, ec+1)], axis=1)
        face_list.append(sw_e)

    all_faces = np.concatenate(face_list)
    mesh = trimesh.Trimesh(vertices=verts, faces=all_faces, process=False)
    mesh.fix_normals()
    if simplify_tolerance > 0:
        mesh = _simplify(mesh, simplify_tolerance)
    return mesh
