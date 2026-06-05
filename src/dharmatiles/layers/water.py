"""Water surface displacement and volume mesh."""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig


WATER_RENDER_LIFT_MM = 0.10


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
