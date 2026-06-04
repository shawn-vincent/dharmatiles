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
