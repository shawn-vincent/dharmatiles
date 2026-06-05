"""
WaterLayer: water surface with point-source ripple interference.

Ripple model
------------
Rather than computing an EDT from the full shoreline (which makes wavefronts
that exactly mirror the shore shape), N discrete point sources are sampled
along the boundary.  Each source emits a damped circular wave:

    z_i(d) = A · exp(−max(0, d−s) / decay) · cos(k · max(0, d−s) + φ_i)

where d is distance from source i, s is a calm-zone start offset (ripples
build up rather than beginning right at the waterline), and φ_i is a small
per-source phase jitter.  Superimposing these waves creates interference
patterns whose wavefronts are not tied to fine shoreline detail.

Source types
------------
Shore     Evenly spaced along the inner ring of the water boundary (water
          cells adjacent to land within the tile, not tile edges).
Grass     Inner-ring cells nearest the grass region — represent blade tips
          that overhang or dip into the water.
Stones    Centroid of each stone footprint that overlaps the water mask.

Mesh
----
A shared-vertex grid (grid_h+1) × (grid_w+1) is built.  Cell-centre
displacement is bilinear-interpolated to vertex corners.  A Gaussian blur
is applied to the displacement field before interpolation to ensure smooth
surface transitions.  Only water cells emit faces.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig, WaterRippleConfig


WATER_RENDER_LIFT_MM = 0.10   # lift water mesh above terrain floor to avoid z-fight


def _make_water_volume_adaptive(
    terrain_z:    np.ndarray,
    water_mask:   np.ndarray,
    water_height: float,
    tile_w:       float,
    tile_h:       float,
    threshold:    float,          # kept for API compatibility; not used
    stride:       int = 16,
) -> trimesh.Trimesh:
    """Water volume: flat top at *water_height*, flat bottom at z = 0.

    Design
    ------
    The terrain solid already represents the sloped riverbed precisely (it
    covers z = -base_h to terrain_z everywhere, including the pool).  The
    water solid only needs to fill the gap from z = 0 up to *water_height*
    above the pool.  The two solids overlap inside the pool; slicers resolve
    the union correctly.

    The top face is flat at *water_height* — no curvature, no Laplacian
    points needed.  A sparse background grid (one vertex every *stride*
    cells) gives a few dozen triangles for the open water.

    Expansion
    ---------
    The water mask is dilated by 2 cells before building the solid.  This
    makes the water volume lap slightly over the shoreline transition strip,
    filling any micro-gap that would otherwise appear between the terrain
    solid's top surface and the water column.  The overlap region is interior
    to the terrain solid and invisible.

    Topology
    --------
    Top and bottom are both flat (different z, same xy).  Every wall vertex
    has a guaranteed non-zero height (water_height > 0 = z_bot), so no
    degenerate wall faces can form.  No vertex merging or degenerate-face
    filtering is needed.
    """
    from scipy.ndimage import binary_dilation
    from scipy.spatial import Delaunay

    gh, gw = terrain_z.shape
    cell_x = tile_w / gw
    cell_y = tile_h / gh

    # ── Expand mask by 2 cells: overlaps terrain, fills boundary gap ──────────
    mask = binary_dilation(water_mask, iterations=2)

    # ── Sparse point set: background grid + tile-edge cells ───────────────────
    # The surface is flat — no curvature signal needed.  A coarse grid bounds
    # the maximum triangle size; tile-edge cells are included so the water
    # solid meets the tile boundary cleanly when the pool reaches an edge.
    bg_pts: set[tuple[int, int]] = {
        (r, c)
        for r in range(0, gh, stride)
        for c in range(0, gw, stride)
        if mask[r, c]
    }
    edge_pts: set[tuple[int, int]] = {
        (int(r), int(c))
        for r, c in np.argwhere(mask)
        if r == 0 or r == gh - 1 or c == 0 or c == gw - 1
    }
    all_pts = sorted(bg_pts | edge_pts)
    n_pts   = len(all_pts)
    xy      = np.array([(c * cell_x, r * cell_y) for r, c in all_pts])

    # ── Delaunay → filter to expanded mask ────────────────────────────────────
    faces = Delaunay(xy).simplices
    cxy   = xy[faces].mean(axis=1)
    cr    = np.clip((cxy[:, 1] / cell_y).astype(int), 0, gh - 1)
    cc    = np.clip((cxy[:, 0] / cell_x).astype(int), 0, gw - 1)
    faces = faces[mask[cr, cc]]

    # ── Vertex buffers: top (flat) + bottom (flat at z = 0) ───────────────────
    verts        = np.empty((2 * n_pts, 3))
    verts[:n_pts,  :2] = xy;  verts[:n_pts,  2] = water_height   # top
    verts[n_pts:,  :2] = xy;  verts[n_pts:,  2] = 0.0            # bottom

    # ── Top / bottom faces ─────────────────────────────────────────────────────
    top_f = faces.astype(np.int32)
    bot_f = (faces[:, ::-1] + n_pts).astype(np.int32)

    # ── Perimeter walls from boundary Delaunay edges ───────────────────────────
    edge_cnt:    dict[tuple[int, int], int]            = {}
    edge_orient: dict[tuple[int, int], tuple[int, int]] = {}
    for f in faces:
        for i in range(3):
            ea, eb = int(f[i]), int(f[(i + 1) % 3])
            key               = (min(ea, eb), max(ea, eb))
            edge_cnt[key]     = edge_cnt.get(key, 0) + 1
            edge_orient[key]  = (ea, eb)

    oriented_walls = [v for k, v in edge_orient.items() if edge_cnt[k] == 1]
    if oriented_walls:
        ow   = np.array(oriented_walls, dtype=np.int32)
        ea, eb = ow[:, 0], ow[:, 1]
        wall_f = np.empty((2 * len(ow), 3), dtype=np.int32)
        wall_f[0::2] = np.stack([eb,          ea,          n_pts + ea], axis=1)
        wall_f[1::2] = np.stack([eb,          n_pts + ea,  n_pts + eb], axis=1)
    else:
        wall_f = np.empty((0, 3), dtype=np.int32)

    all_faces = np.vstack([top_f, bot_f, wall_f])
    mesh = trimesh.Trimesh(vertices=verts, faces=all_faces, process=False)
    mesh.fix_normals()
    return mesh


def make_water_volume(terrain_z: np.ndarray,
                      water_mask: np.ndarray,
                      water_height: float,
                      tile_w: float,
                      tile_h: float,
                      z_disp: np.ndarray | None = None,
                      error_threshold: float | None = None,
                      simplify_stride: int = 16) -> trimesh.Trimesh:
    """Closed solid spanning the water volume: flat top at *water_height*, riverbed bottom.

    Faces emitted
    -------------
    Top face     flat grid at *water_height* over every water cell.
    Bottom face  terrain_z surface (the textured riverbed) under every water cell.
    Perimeter    walls connecting top and bottom edges wherever a water cell
                 borders a non-water cell or the tile boundary.

    The solid is standalone.  When concatenated with the terrain solid (which
    covers the full tile from z=0 up to terrain_z), the two volumes together
    fill 0→water_height over the water region.  Interior faces at terrain_z are
    redundant but harmless — slicers resolve the union by volume.

    error_threshold / simplify_stride
        When *error_threshold* is set, an adaptive Delaunay mesh is built
        instead of a full grid.  Interior vertices are kept only where
        |∇²z| > threshold (mm) so flat areas collapse to a few triangles
        while sloped or textured zones stay dense — exactly matching the
        behaviour of the terrain solid's adaptive path.
    """
    if error_threshold is not None and z_disp is None:
        return _make_water_volume_adaptive(
            terrain_z, water_mask, water_height, tile_w, tile_h,
            threshold=error_threshold, stride=simplify_stride,
        )

    gh, gw = terrain_z.shape
    cell_x = tile_w / gw
    cell_y = tile_h / gh

    nv_r = gh + 1   # vertex grid rows
    nv_c = gw + 1   # vertex grid cols
    n_half = nv_r * nv_c

    # ── Bottom corner z: bilinear avg of surrounding water cells only ──────────
    # Using only water cells prevents non-water terrain heights bleeding into
    # the riverbed corners at the shoreline.
    pad_z = np.pad(terrain_z,              1, mode='edge')
    pad_m = np.pad(water_mask.astype(float), 1, mode='constant')  # 0 outside
    sum_z = (pad_z[:-1, :-1] * pad_m[:-1, :-1] + pad_z[:-1, 1:] * pad_m[:-1, 1:] +
             pad_z[1:,  :-1] * pad_m[1:,  :-1] + pad_z[1:,  1:] * pad_m[1:,  1:])
    sum_w = (pad_m[:-1, :-1] + pad_m[:-1, 1:] +
             pad_m[1:,  :-1] + pad_m[1:,  1:])
    bot_z = np.where(sum_w > 0, sum_z / np.maximum(sum_w, 1e-9), water_height)
    bot_z = np.minimum(bot_z, water_height)   # bottom never above water surface

    # ── Top surface z: water_height + optional displacement ───────────────────
    if z_disp is not None:
        # Bilinear-interpolate cell-centre displacement to vertex corners
        pad    = np.pad(z_disp, 1, mode='edge')          # (gh+2, gw+2)
        top_z  = water_height + 0.25 * (
            pad[ :-1,  :-1] + pad[ :-1, 1:] +
            pad[1:  ,  :-1] + pad[1:  , 1:])             # (nv_r, nv_c)
        # Clamp: surface must sit above the riverbed at every vertex
        top_z  = np.maximum(top_z, bot_z)
    else:
        top_z  = np.full((nv_r, nv_c), water_height)

    # ── Vertex buffer ─────────────────────────────────────────────────────────
    x_v = np.broadcast_to((np.arange(nv_c) * cell_x)[None, :], (nv_r, nv_c))
    y_v = np.broadcast_to((np.arange(nv_r) * cell_y)[:, None], (nv_r, nv_c))

    verts = np.empty((2 * n_half, 3))
    verts[:n_half, 0] = x_v.ravel()
    verts[:n_half, 1] = y_v.ravel()
    verts[:n_half, 2] = top_z.ravel()         # top: water_height + displacement
    verts[n_half:, 0] = x_v.ravel()
    verts[n_half:, 1] = y_v.ravel()
    verts[n_half:, 2] = bot_z.ravel()         # bottom: riverbed

    def tv(r, c): return np.asarray(r) * nv_c + np.asarray(c)
    def bv(r, c): return n_half + tv(r, c)

    face_list: list[np.ndarray] = []

    # ── Top and bottom faces (vectorised) ─────────────────────────────────────
    wr, wc = np.where(water_mask)
    t00 = tv(wr,   wc);   t01 = tv(wr,   wc + 1)
    t10 = tv(wr+1, wc);   t11 = tv(wr+1, wc + 1)
    b00 = bv(wr,   wc);   b01 = bv(wr,   wc + 1)
    b10 = bv(wr+1, wc);   b11 = bv(wr+1, wc + 1)

    # Top: CCW from above → normal +z
    top_f = np.empty((2 * len(wr), 3), dtype=np.int32)
    top_f[0::2] = np.stack([t00, t01, t11], axis=1)
    top_f[1::2] = np.stack([t00, t11, t10], axis=1)
    face_list.append(top_f)

    # Bottom: CW from above → normal −z
    bot_f = np.empty((2 * len(wr), 3), dtype=np.int32)
    bot_f[0::2] = np.stack([b00, b11, b01], axis=1)
    bot_f[1::2] = np.stack([b00, b10, b11], axis=1)
    face_list.append(bot_f)

    # ── Perimeter walls ───────────────────────────────────────────────────────
    # Extend mask with False border so boundary cells always trigger a wall.
    ext = np.zeros((gh + 2, gw + 2), dtype=bool)
    ext[1:-1, 1:-1] = water_mask

    # South wall (normal −y): water cell with no water neighbour at r−1
    s_mask = water_mask & ~ext[:-2, 1:-1]
    sr, sc = np.where(s_mask)
    if len(sr):
        st0 = tv(sr, sc);     st1 = tv(sr, sc + 1)
        sb0 = bv(sr, sc);     sb1 = bv(sr, sc + 1)
        sw_s = np.empty((2 * len(sr), 3), dtype=np.int32)
        sw_s[0::2] = np.stack([st0, sb0, st1], axis=1)
        sw_s[1::2] = np.stack([st1, sb0, sb1], axis=1)
        face_list.append(sw_s)

    # North wall (normal +y): water cell with no water neighbour at r+1
    n_mask = water_mask & ~ext[2:, 1:-1]
    nr, nc_ = np.where(n_mask)
    if len(nr):
        nt0 = tv(nr + 1, nc_);    nt1 = tv(nr + 1, nc_ + 1)
        nb0 = bv(nr + 1, nc_);    nb1 = bv(nr + 1, nc_ + 1)
        sw_n = np.empty((2 * len(nr), 3), dtype=np.int32)
        sw_n[0::2] = np.stack([nt0, nt1, nb0], axis=1)
        sw_n[1::2] = np.stack([nt1, nb1, nb0], axis=1)
        face_list.append(sw_n)

    # West wall (normal −x): water cell with no water neighbour at c−1
    w_mask = water_mask & ~ext[1:-1, :-2]
    wr2, wc2 = np.where(w_mask)
    if len(wr2):
        wt0 = tv(wr2,     wc2);   wt1 = tv(wr2 + 1, wc2)
        wb0 = bv(wr2,     wc2);   wb1 = bv(wr2 + 1, wc2)
        sw_w = np.empty((2 * len(wr2), 3), dtype=np.int32)
        sw_w[0::2] = np.stack([wt0, wt1, wb0], axis=1)
        sw_w[1::2] = np.stack([wt1, wb1, wb0], axis=1)
        face_list.append(sw_w)

    # East wall (normal +x): water cell with no water neighbour at c+1
    e_mask = water_mask & ~ext[1:-1, 2:]
    er, ec = np.where(e_mask)
    if len(er):
        et0 = tv(er,     ec + 1);  et1 = tv(er + 1, ec + 1)
        eb0 = bv(er,     ec + 1);  eb1 = bv(er + 1, ec + 1)
        sw_e = np.empty((2 * len(er), 3), dtype=np.int32)
        sw_e[0::2] = np.stack([et0, eb0, et1], axis=1)
        sw_e[1::2] = np.stack([et1, eb0, eb1], axis=1)
        face_list.append(sw_e)

    all_faces = np.concatenate(face_list)
    mesh = trimesh.Trimesh(vertices=verts, faces=all_faces, process=False)
    mesh.fix_normals()
    return mesh


class WaterLayer:
    """Build a water-surface mesh, optionally with point-source ripples.

    The mesh sits *render_lift_mm* above the terrain floor so that the
    terrain top faces (pool floor, boundary) are never occluded when the
    water surface is calm.  When a ripple trough drops the surface below
    the lift offset, the terrain face shows through — revealing the pool
    floor or shore as land.

    Parameters
    ----------
    surface        : SurfaceConfig
    height_mm      : float — water-surface z level (mm).
    render_lift_mm : float — gap between terrain floor and calm water (mm).
    ripple_cfg     : WaterRippleConfig | None — ripple parameters; None = flat.
    """

    def __init__(self, surface: SurfaceConfig, height_mm: float,
                 render_lift_mm: float = WATER_RENDER_LIFT_MM,
                 ripple_cfg: WaterRippleConfig | None = None) -> None:
        self.surface        = surface
        self.height_mm      = height_mm
        self.render_lift_mm = render_lift_mm
        self.ripple_cfg     = ripple_cfg

    def build(self, water_mask: np.ndarray,
              stone_mask:    np.ndarray | None = None,
              grass_mask:    np.ndarray | None = None,
              effective_mask: np.ndarray | None = None,
              z_disp_pre:    np.ndarray | None = None) -> list[trimesh.Trimesh]:
        """Return a water-surface mesh.

        Parameters
        ----------
        water_mask     : bool (grid_h, grid_w) — core water region.
        stone_mask     : optional bool — stone-contact ripple sources.
        grass_mask     : optional bool — grass-tip ripple sources.
        effective_mask : optional bool — actual cells to emit faces for.
                         Defaults to *water_mask*.  Pass the expanded mask
                         (water + overflow) from the caller.
        z_disp_pre     : optional pre-computed (gh, gw) displacement array.
                         When provided, skips the internal ripple computation.
        """
        surface = self.surface
        h       = self.height_mm + self.render_lift_mm
        gh, gw  = water_mask.shape

        face_mask = effective_mask if effective_mask is not None else water_mask
        if not np.any(face_mask):
            return []

        # ── Ripple displacement (cell-centre grid) ────────────────────────────
        if z_disp_pre is not None:
            z_disp = z_disp_pre
        elif self.ripple_cfg is not None:
            z_disp = _build_ripple_displacement(
                surface, water_mask, stone_mask, grass_mask, self.ripple_cfg,
            )
        else:
            z_disp = np.zeros((gh, gw), dtype=float)

        # ── Bilinear interpolation: cell centres → vertex corners ─────────────
        pad     = np.pad(z_disp, 1, mode='edge')          # (gh+2, gw+2)
        vz_disp = 0.25 * (
            pad[ :-1,  :-1] + pad[ :-1, 1:  ] +
            pad[1:  ,  :-1] + pad[1:  , 1:  ]
        )                                                  # (gh+1, gw+1)

        # ── Vertex positions ──────────────────────────────────────────────────
        x_v, y_v = np.meshgrid(
            np.arange(gw + 1, dtype=float) * surface.cell_w,
            np.arange(gh + 1, dtype=float) * surface.cell_w,
        )
        z_v   = h + vz_disp
        verts = np.stack([x_v.ravel(), y_v.ravel(), z_v.ravel()], axis=1)

        # ── Faces: two triangles per cell in face_mask ───────────────────────
        water_r, water_c = np.where(face_mask)
        nf  = len(water_r)
        v00 = water_r       * (gw + 1) + water_c
        v01 = water_r       * (gw + 1) + water_c + 1
        v10 = (water_r + 1) * (gw + 1) + water_c
        v11 = (water_r + 1) * (gw + 1) + water_c + 1

        faces = np.empty((2 * nf, 3), dtype=np.int32)
        faces[:nf, 0] = v00;  faces[:nf, 1] = v01;  faces[:nf, 2] = v11
        faces[nf:, 0] = v00;  faces[nf:, 1] = v11;  faces[nf:, 2] = v10

        mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
        mesh.fix_normals()
        return [mesh]


# ── Ripple displacement ───────────────────────────────────────────────────────

def _build_ripple_displacement(
    surface:      SurfaceConfig,
    water_mask:   np.ndarray,
    stone_mask:   np.ndarray | None,
    grass_mask:   np.ndarray | None,
    cfg:          WaterRippleConfig,
    compute_mask: np.ndarray | None = None,
) -> np.ndarray:
    """Return (gh, gw) cell-centre displacement array (mm).

    Sources are always derived from *water_mask*'s inner ring.
    Displacement is evaluated for every cell in *compute_mask* (defaults to
    *water_mask*), allowing the caller to extend evaluation into a border zone.
    """
    from scipy.ndimage import binary_erosion, distance_transform_edt, label

    gh, gw      = water_mask.shape
    eval_mask   = compute_mask if compute_mask is not None else water_mask
    rng         = np.random.default_rng(surface.seed ^ 0xC0A574)
    cell_mm     = surface.cell_w

    # ── Shore boundary ────────────────────────────────────────────────────────
    # border_value=1 → outside tile treated as water → only cells adjacent to
    # actual land within the tile become inner-ring cells.
    water_eroded = binary_erosion(water_mask, border_value=1)
    inner_ring   = water_mask & ~water_eroded

    # ── Sample shore sources (evenly spaced, not random) ─────────────────────
    shore_cells  = np.argwhere(inner_ring)
    n_shore      = min(cfg.n_shore_sources, len(shore_cells))
    idx          = np.linspace(0, len(shore_cells) - 1, n_shore).astype(int)
    shore_pts    = shore_cells[idx]

    # ── Grass-tip sources ─────────────────────────────────────────────────────
    # Inner-ring cells whose nearest grass cell is within ~6 mm (covers the
    # boundary-strip width plus a small margin regardless of spec settings).
    grass_pts = np.empty((0, 2), dtype=int)
    if grass_mask is not None and grass_mask.any():
        dist_to_grass_mm = distance_transform_edt(~grass_mask) * cell_mm
        grass_ring       = inner_ring & (dist_to_grass_mm < 6.0)
        gcells           = np.argwhere(grass_ring)
        if len(gcells) > 0:
            n_g      = min(cfg.n_grass_sources, len(gcells))
            idx_g    = np.linspace(0, len(gcells) - 1, n_g).astype(int)
            grass_pts = gcells[idx_g]

    # ── Stone sources (one per stone that overlaps water) ─────────────────────
    stone_pts = np.empty((0, 2), dtype=int)
    if stone_mask is not None:
        stone_in_water = stone_mask & water_mask
        if stone_in_water.any():
            labeled, n_comp = label(stone_in_water)
            centers = []
            for i in range(1, n_comp + 1):
                rr, cc = np.where(labeled == i)
                centers.append([int(rr.mean()), int(cc.mean())])
            stone_pts = np.array(centers, dtype=int)

    # ── Coordinate grids (mm) ─────────────────────────────────────────────────
    rows_mm = np.arange(gh, dtype=float)[:, None] * surface.cell_w
    cols_mm = np.arange(gw, dtype=float)[None, :] * surface.cell_w

    # ── Accumulate circular waves ─────────────────────────────────────────────
    z_disp = np.zeros((gh, gw), dtype=float)
    k      = 2.0 * np.pi / cfg.wavelength_mm

    def _add_sources(pts, amplitude):
        for (sr, sc) in pts:
            sr_mm  = sr * surface.cell_w
            sc_mm  = sc * surface.cell_w
            dist   = np.sqrt((rows_mm - sr_mm) ** 2 + (cols_mm - sc_mm) ** 2)
            d_eff  = np.maximum(0.0, dist - cfg.start_offset_mm)
            phase  = rng.normal(0.0, cfg.phase_spread)
            wave   = amplitude * np.exp(-d_eff / cfg.decay_mm) * np.cos(k * d_eff + phase)
            z_disp[eval_mask] += wave[eval_mask]

    _add_sources(shore_pts,  cfg.amplitude_mm)
    _add_sources(grass_pts,  cfg.amplitude_mm * cfg.grass_amplitude)
    _add_sources(stone_pts,  cfg.amplitude_mm * 0.6)

    z_disp[~eval_mask] = 0.0
    return z_disp
