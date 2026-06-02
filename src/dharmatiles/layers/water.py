"""
WaterLayer: flat water surface with optional concentric ripple displacement.

Ripple model
------------
Ripples radiate inward from the water boundary — where land (shoreline,
stones) meets open water.  For each water cell we compute the Euclidean
distance to the nearest boundary contact point (via scipy EDT), then
apply a damped cosine:

    z(d) = A · exp(−d / decay) · cos(2π·d / λ)
         + A·a₂ · exp(−d / (0.6·decay)) · cos(2π·d / λ₂ + φ₂)

The secondary term uses a shorter wavelength and a phase offset so its
crests do not align with the primary ring, creating a subtle interference
texture that breaks the too-perfect concentric pattern.

Edge clamping
-------------
The displacement is multiplied by a per-cell smoothstep fade that reaches
zero at the tile boundary, keeping every tile edge exactly flat for clean
interlocking and printability.

Mesh
----
A shared-vertex grid (grid_h+1) × (grid_w+1) is built for the entire tile.
Displacement is bilinear-interpolated from cell-centre values to vertex
corners.  Only cells inside *water_mask* emit faces, so non-water parts of
the vertex buffer are allocated but unused (harmless for export).
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig, WaterRippleConfig


WATER_RENDER_LIFT_MM = 0.0


class WaterLayer:
    """Build a water-surface mesh coloured blue, optionally with ripples.

    Parameters
    ----------
    surface        : SurfaceConfig — tile dimensions and grid resolution.
    height_mm      : float — water-surface z level (mm).
    render_lift_mm : float — extra z added on top of height_mm for rendering.
    ripple_cfg     : WaterRippleConfig | None — ripple parameters, or None
                     for a perfectly flat surface.
    """

    def __init__(self, surface: SurfaceConfig, height_mm: float,
                 render_lift_mm: float = WATER_RENDER_LIFT_MM,
                 ripple_cfg: WaterRippleConfig | None = None) -> None:
        self.surface        = surface
        self.height_mm      = height_mm
        self.render_lift_mm = render_lift_mm
        self.ripple_cfg     = ripple_cfg

    # ── Public API ────────────────────────────────────────────────────────────

    def build(self, water_mask: np.ndarray,
              stone_mask:  np.ndarray | None = None,
              grass_mask:  np.ndarray | None = None) -> list[trimesh.Trimesh]:
        """Return a water-surface mesh covering *water_mask* cells.

        Parameters
        ----------
        water_mask : bool (grid_h, grid_w) — True for water cells.
        stone_mask : optional bool — stone footprint cells; those that
                     overlap water become additional ripple sources.
        grass_mask : reserved for future grass-contact ripples (unused).

        Returns an empty list if the mask has no True cells.
        """
        surface = self.surface
        h       = self.height_mm + self.render_lift_mm
        gh, gw  = water_mask.shape

        if not np.any(water_mask):
            return []

        # ── Ripple displacement (cell-centre grid) ────────────────────────────
        if self.ripple_cfg is not None:
            z_disp = _build_ripple_displacement(
                surface, water_mask, stone_mask, self.ripple_cfg,
            )
        else:
            z_disp = np.zeros((gh, gw), dtype=float)

        # ── Bilinear interpolation: cell centres → vertex corners ─────────────
        # Each vertex (vr, vc) is at the corner shared by the four surrounding
        # cells.  We pad the displacement array (edge mode) so boundary
        # vertices reflect the nearest valid cell.
        pad    = np.pad(z_disp, 1, mode='edge')       # (gh+2, gw+2)
        vz_disp = 0.25 * (
            pad[ :-1,  :-1] +    # cell above-left  of vertex
            pad[ :-1, 1:  ] +    # cell above-right
            pad[1:  ,  :-1] +    # cell below-left
            pad[1:  , 1:  ]      # cell below-right
        )                                              # (gh+1, gw+1)

        # ── Vertex positions ──────────────────────────────────────────────────
        x_v, y_v = np.meshgrid(
            np.arange(gw + 1, dtype=float) * surface.cell_w,
            np.arange(gh + 1, dtype=float) * surface.cell_h,
        )                                                    # both (gh+1, gw+1)
        z_v = h + vz_disp

        verts = np.stack(
            [x_v.ravel(), y_v.ravel(), z_v.ravel()], axis=1
        )                                                   # (N_verts, 3)

        # ── Faces: one quad (2 triangles) per water cell ──────────────────────
        water_r, water_c = np.where(water_mask)
        nf = len(water_r)

        v00 = water_r       * (gw + 1) + water_c        # top-left  vertex
        v01 = water_r       * (gw + 1) + water_c + 1    # top-right
        v10 = (water_r + 1) * (gw + 1) + water_c        # bot-left
        v11 = (water_r + 1) * (gw + 1) + water_c + 1   # bot-right

        faces = np.empty((2 * nf, 3), dtype=np.int32)
        faces[:nf, 0] = v00;  faces[:nf, 1] = v01;  faces[:nf, 2] = v11
        faces[nf:, 0] = v00;  faces[nf:, 1] = v11;  faces[nf:, 2] = v10

        mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
        mesh.fix_normals()
        return [mesh]


# ── Ripple displacement ───────────────────────────────────────────────────────

def _build_ripple_displacement(
    surface:    SurfaceConfig,
    water_mask: np.ndarray,
    stone_mask: np.ndarray | None,
    cfg:        WaterRippleConfig,
) -> np.ndarray:
    """Return (gh, gw) displacement array (mm); zero outside water_mask."""
    from scipy.ndimage import binary_erosion, distance_transform_edt

    gh, gw = water_mask.shape

    # ── Ripple sources ────────────────────────────────────────────────────────
    # Primary source: water cells adjacent to land WITHIN the tile.
    # border_value=1 tells binary_erosion to treat cells outside the array as
    # water, so tile-edge water cells are eroded away and do NOT become sources.
    # Only cells neighbouring actual land (region boundaries) survive.
    water_eroded = binary_erosion(water_mask, border_value=1)
    inner_ring   = water_mask & ~water_eroded

    # Secondary source: stones that touch water add contact-point ripples.
    all_sources = inner_ring.copy()
    if stone_mask is not None:
        all_sources |= (stone_mask & water_mask)

    # ── Distance from sources (mm) ────────────────────────────────────────────
    # distance_transform_edt(~all_sources): distance to nearest source cell.
    # Source cells → 0; open water → positive.
    dist_cells = distance_transform_edt(~all_sources)
    cell_mm    = 0.5 * (surface.cell_w + surface.cell_h)
    dist_mm    = dist_cells * cell_mm
    dist_mm[~water_mask] = 0.0

    # ── Damped cosine ripple ──────────────────────────────────────────────────
    k1 = 2.0 * np.pi / cfg.wavelength_mm
    k2 = 2.0 * np.pi / (cfg.wavelength_mm * cfg.secondary_wavelength)

    primary   = (cfg.amplitude_mm *
                 np.exp(-dist_mm / cfg.decay_mm) *
                 np.cos(k1 * dist_mm))

    secondary = (cfg.amplitude_mm * cfg.secondary_amplitude *
                 np.exp(-dist_mm / (cfg.decay_mm * 0.6)) *
                 np.cos(k2 * dist_mm + cfg.secondary_phase))

    z_disp = primary + secondary
    z_disp[~water_mask] = 0.0

    return z_disp
