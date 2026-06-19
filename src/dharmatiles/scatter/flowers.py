"""
Flowers: a scatter thing that places 3D flower meshes into a region.

Each flower has:
  * A **centre dome** — a thin half-sphere cap (``dome_thickness_mm`` tall)
    on an inverted-cone support column.
  * **N petal domes** — thin elongated half-ellipsoid caps
    (``petal_thickness_mm`` tall), each on an inverted-cone support column.
    Petals are elongated radially (perpendicular to the tangent).

Each support column is an **inverted cone**: base = the dome's equatorial
footprint at the top of the column (``col_top_z``), apex = a single point at
terrain level (``base_z``) directly below the dome centre.  No prism walls —
the cone surface IS the column.  The cone angle is determined by the ratio of
the dome footprint radius to ``column_height_mm``; set
``column_height_mm ≈ outer_half_axis`` to get ~45°.

All shapes are watertight solids.  Flower footprints are stamped into
``terrain_support_z`` and ``obstacle_mask`` so grass steers around them.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import FlowerConfig
from ..core.grid import sample_grid
from ..core.tile import derive_seed
from .config import Uniform
from .distribute import scatter_positions


# ── Dome-on-inverted-cone mesh primitive ──────────────────────────────────────

def _build_dome_on_column(
    cx: float, cy: float, base_z: float,
    rx: float, ry: float,
    dome_h: float, column_h: float,
    angle: float,
    AZ: int, EL: int,
    *,
    cone_apex_xy: tuple[float, float] | None = None,
    cylinder_column: bool = False,
) -> trimesh.Trimesh:
    """Thin dome cap atop a support column.

    Two column modes (mutually exclusive):

    * **Cylinder** (``cylinder_column=True``): straight prism, same cross-
      section from ``base_z`` to ``base_z + column_h``.  Used for the centre
      dome so it forms a solid base the petal cones can union into.
    * **Inverted cone** (default): single apex point at
      ``(cone_apex_xy or (cx,cy), base_z)``, fanning out to the dome's
      equatorial ellipse at ``base_z + column_h``.  Setting *cone_apex_xy*
      to the **flower centre** makes every generator line on the petal
      perimeter point directly toward the flower centre as it descends.

    Dome cap: half-ellipsoid, half-axes *rx* × *ry* × *dome_h*, from
    ``base_z + column_h`` up to ``base_z + column_h + dome_h``.
    All lateral geometry is rotated by *angle* in XY.
    """
    ca, sa = np.cos(angle), np.sin(angle)
    col_top_z = base_z + column_h

    # Vertex count depends on column mode:
    #   cone:     apex + EL rings + 1 cone-apex point
    #   cylinder: apex + EL rings + 1 bottom ring (AZ verts) + 1 bottom centre
    vps = (1 + EL * AZ + 1) if not cylinder_column else (1 + (EL + 1) * AZ + 1)
    verts = np.empty((vps, 3), dtype=float)

    # ── Dome apex ─────────────────────────────────────────────────────────────
    verts[0] = [cx, cy, col_top_z + dome_h]

    # ── Dome rings ────────────────────────────────────────────────────────────
    ei = np.arange(1, EL + 1, dtype=float)
    phi = ei / EL * (np.pi / 2.0)
    r_frac = np.sin(phi)
    z_frac = np.cos(phi)

    ai = np.arange(AZ, dtype=float)
    th = 2.0 * np.pi * ai / AZ
    cos_th = np.cos(th)
    sin_th = np.sin(th)

    lx = rx * r_frac[:, None] * cos_th[None, :]
    ly = ry * r_frac[:, None] * sin_th[None, :]

    wx = cx + ca * lx - sa * ly
    wy = cy + sa * lx + ca * ly
    wz = (col_top_z + dome_h * z_frac[:, None]) * np.ones((1, AZ))

    dome_start = 1
    verts[dome_start:dome_start + EL * AZ, 0] = wx.ravel()
    verts[dome_start:dome_start + EL * AZ, 1] = wy.ravel()
    verts[dome_start:dome_start + EL * AZ, 2] = wz.ravel()

    # ── Column geometry ───────────────────────────────────────────────────────
    lx_eq = rx * cos_th   # equator local x
    ly_eq = ry * sin_th
    wx_eq = cx + ca * lx_eq - sa * ly_eq
    wy_eq = cy + sa * lx_eq + ca * ly_eq

    if cylinder_column:
        # Straight cylinder: bottom ring at base_z (same XY as dome equator)
        col_bot_start = dome_start + EL * AZ
        verts[col_bot_start:col_bot_start + AZ, 0] = wx_eq
        verts[col_bot_start:col_bot_start + AZ, 1] = wy_eq
        verts[col_bot_start:col_bot_start + AZ, 2] = base_z
        bot_centre = col_bot_start + AZ
        verts[bot_centre] = [cx, cy, base_z]
    else:
        # Inverted cone: single apex point
        apex_x, apex_y = cone_apex_xy if cone_apex_xy is not None else (cx, cy)
        cone_apex_idx = dome_start + EL * AZ
        verts[cone_apex_idx] = [apex_x, apex_y, base_z]

    # ── Faces ─────────────────────────────────────────────────────────────────
    faces: list[list[int]] = []

    # Top fan: dome apex → first dome ring
    for ai_i in range(AZ):
        faces.append([0, dome_start + ai_i, dome_start + (ai_i + 1) % AZ])

    # Dome side quads
    for ei_i in range(1, EL):
        ra = dome_start + (ei_i - 1) * AZ
        rb = dome_start + ei_i * AZ
        for ai_i in range(AZ):
            a0 = ra + ai_i;       a1 = ra + (ai_i + 1) % AZ
            b0 = rb + ai_i;       b1 = rb + (ai_i + 1) % AZ
            faces += [[a0, b0, a1], [a1, b0, b1]]

    dome_eq_start = dome_start + (EL - 1) * AZ

    if cylinder_column:
        # Cylinder walls: dome equator → bottom ring
        for ai_i in range(AZ):
            a0 = dome_eq_start + ai_i;     a1 = dome_eq_start + (ai_i + 1) % AZ
            b0 = col_bot_start + ai_i;     b1 = col_bot_start + (ai_i + 1) % AZ
            faces += [[a0, b0, a1], [a1, b0, b1]]
        # Bottom cap fan
        for ai_i in range(AZ):
            faces.append([col_bot_start + ai_i, bot_centre,
                          col_bot_start + (ai_i + 1) % AZ])
    else:
        # Cone: dome equator → single apex point
        for ai_i in range(AZ):
            faces.append([dome_eq_start + ai_i, cone_apex_idx,
                          dome_eq_start + (ai_i + 1) % AZ])

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


# ── Petal geometry helpers ────────────────────────────────────────────────────

def _petal_dimensions(cfg: FlowerConfig) -> tuple[float, float, float]:
    """Return (petal_hl, petal_hw, petal_ctr_r) for the given config.

    *petal_hw* is chosen so adjacent petals **touch but do not overlap** at the
    outer edge of the centre dome.  The midpoint between adjacent petals at
    radius ``center_radius_mm`` lies at angle ``π/n`` from each petal's axis;
    solving the ellipse equation at that contact point gives::

        lx_c = center_radius_mm * cos(π/n) − petal_ctr_r
        ly_c = center_radius_mm * sin(π/n)
        petal_hw = ly_c / sqrt(1 − (lx_c / petal_hl)²)
    """
    n = cfg.n_petals
    petal_base_r = cfg.center_radius_mm * (1.0 - cfg.overlap)
    petal_tip_r  = cfg.outer_radius_mm
    petal_hl     = (petal_tip_r - petal_base_r) / 2.0
    petal_ctr_r  = (petal_base_r + petal_tip_r) / 2.0

    if n < 1 or petal_hl <= 0:
        return petal_hl, 0.0, petal_ctr_r

    # Contact point in petal-local coordinates
    lx_c = cfg.center_radius_mm * np.cos(np.pi / n) - petal_ctr_r
    ly_c = cfg.center_radius_mm * np.sin(np.pi / n)
    denom2 = 1.0 - (lx_c / petal_hl) ** 2
    if denom2 <= 0.0:
        # Contact point is outside petal radial extent; fall back to arc spacing
        petal_hw = cfg.center_radius_mm * np.sin(np.pi / n)
    else:
        petal_hw = ly_c / np.sqrt(denom2)

    return petal_hl, float(petal_hw), petal_ctr_r


# ── Flower mesh builder ────────────────────────────────────────────────────────

def _build_flower_mesh(
    cx: float, cy: float, tz: float,
    angle: float,
    cfg: FlowerConfig,
) -> trimesh.Trimesh:
    """Build a single flower at (cx, cy) on terrain height tz."""
    base_z   = tz - cfg.sink
    column_h = cfg.column_height_mm
    AZ       = cfg.az_segs
    EL       = cfg.el_segs

    # Centre dome: straight cylinder column — petal cones union into this base
    parts: list[trimesh.Trimesh] = [
        _build_dome_on_column(
            cx, cy, base_z,
            rx=cfg.center_radius_mm, ry=cfg.center_radius_mm,
            dome_h=cfg.dome_thickness_mm, column_h=column_h,
            angle=0.0, AZ=AZ, EL=EL,
            cylinder_column=True,
        )
    ]

    # Petals: elongated radially (rx along radial axis, ry tangential)
    # petal_hw sized so petals touch (not overlap) at the centre-dome edge.
    n = cfg.n_petals
    if n > 0:
        petal_hl, petal_hw, petal_ctr_r = _petal_dimensions(cfg)

        if petal_hl > 0:
            for i in range(n):
                petal_angle = angle + i * 2.0 * np.pi / n
                px = cx + petal_ctr_r * np.cos(petal_angle)
                py = cy + petal_ctr_r * np.sin(petal_angle)
                parts.append(
                    _build_dome_on_column(
                        px, py, base_z,
                        rx=petal_hl, ry=petal_hw,
                        dome_h=cfg.petal_thickness_mm, column_h=column_h,
                        angle=petal_angle, AZ=AZ, EL=EL,
                        cone_apex_xy=(cx, cy),   # every edge points to flower centre
                    )
                )

    if len(parts) == 0:
        return trimesh.Trimesh(process=False)
    if len(parts) == 1:
        return parts[0]
    return trimesh.util.concatenate(parts)


# ── Support stamping ──────────────────────────────────────────────────────────

def _stamp_flower(
    cx: float, cy: float, tz: float,
    cfg: FlowerConfig,
    support_z: np.ndarray,
    obstacle_mask: np.ndarray | None,
    surface,
) -> None:
    """Rasterise the flower footprint into *support_z* and *obstacle_mask*."""
    block_z = tz + cfg.height_mm

    cw = surface.cell_w
    gw = surface.grid_w
    gh = surface.grid_h

    def _stamp_ellipse(ex, ey, erx, ery, angle_e):
        r_max = max(erx, ery)
        i_lo = max(0,      int((ex - r_max) / cw))
        i_hi = min(gw - 1, int((ex + r_max) / cw) + 1)
        j_lo = max(0,      int((ey - r_max) / cw))
        j_hi = min(gh - 1, int((ey + r_max) / cw) + 1)
        if i_lo > i_hi or j_lo > j_hi:
            return

        ii = np.arange(i_lo, i_hi + 1)
        jj = np.arange(j_lo, j_hi + 1)
        II, JJ = np.meshgrid(ii, jj)
        dx_g = II * cw - ex
        dy_g = JJ * cw - ey

        ca, sa = np.cos(angle_e), np.sin(angle_e)
        lx_g =  ca * dx_g + sa * dy_g
        ly_g = -sa * dx_g + ca * dy_g

        inside = (lx_g / erx) ** 2 + (ly_g / ery) ** 2 <= 1.0
        if not np.any(inside):
            return

        sl = support_z[j_lo:j_hi + 1, i_lo:i_hi + 1]
        np.maximum(sl, np.where(inside, block_z, -np.inf), out=sl)
        if obstacle_mask is not None:
            obstacle_mask[j_lo:j_hi + 1, i_lo:i_hi + 1] |= inside

    _stamp_ellipse(cx, cy, cfg.center_radius_mm, cfg.center_radius_mm, 0.0)

    n = cfg.n_petals
    if n > 0:
        petal_hl, petal_hw, petal_ctr_r = _petal_dimensions(cfg)

        if petal_hl > 0:
            for i in range(n):
                petal_angle = i * 2.0 * np.pi / n
                px = cx + petal_ctr_r * np.cos(petal_angle)
                py = cy + petal_ctr_r * np.sin(petal_angle)
                _stamp_ellipse(px, py, petal_hl, petal_hw, petal_angle)


# ── Flowers scatter thing ─────────────────────────────────────────────────────

class Flowers:
    """Scatter 3D flowers directly into a region layer list.

    Each flower has a thin dome cap atop an inverted-cone support column, plus
    N petal domes.  Flower footprints are stamped into ``terrain_support_z``
    and ``obstacle_mask`` so subsequent ``Grass`` blades steer around them.
    Place ``Flowers`` before ``Grass`` in ``Region.layers``.
    """

    height_default_mm: float = 5.0

    def __init__(self, *, placement: Uniform | None = None, **flower_kwargs) -> None:
        self.cfg       = FlowerConfig(**flower_kwargs)
        self.placement = placement or Uniform(count_per_square=5)

    def footprint_mm(self) -> float:
        return float(self.cfg.outer_radius_mm)

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx: int = 0,
    ) -> list[trimesh.Trimesh]:
        surface  = scene.surface
        rng_seed = (derive_seed(surface.seed, 'flowers-scatter', layer_idx)
                    ^ self.placement.seed)
        rng = np.random.default_rng(rng_seed)

        positions = scatter_positions(
            self.placement,
            surface.cols * surface.rows,
            self.footprint_mm(),
            placement_mask,
            scene,
            surface,
            rng,
        )
        if not positions:
            return []

        cfg    = self.cfg
        meshes: list[trimesh.Trimesh] = []

        for x, y, _gd in positions:
            tz    = float(sample_grid(scene.terrain_z, surface,
                                      np.array([x]), np.array([y]))[0])
            angle = float(rng.uniform(0.0, 2.0 * np.pi))
            flower = _build_flower_mesh(x, y, tz, angle, cfg)
            if len(flower.vertices) > 0:
                meshes.append(flower)
            _stamp_flower(x, y, tz, cfg,
                          scene.terrain_support_z, scene.obstacle_mask, surface)

        if not meshes:
            return []

        combined = trimesh.util.concatenate(meshes)
        from ..core.color import Material, tag as _tag
        _tag(combined, Material.FLOWER)
        return [combined]

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        """``TileLayer`` entry point — delegates to ``scatter()``."""
        return self.scatter(scene, placement_mask=placement_mask)
