#!/usr/bin/env python3
"""
Terrain-following grass STL — keel-free experiment.
(Derived from generate-grass-support-stl.py.  Keel removed to diagnose
blade-height blowup: blades should sit ≤ 6 mm above the surface.)

Algorithm
─────────
  support_z starts as the terrain surface.  Blades are painted onto it in
  order (back→front).  Each blade's footprint is rasterised back into
  support_z after it is built, so the next blade rests on top.

Blade geometry
──────────────
  XY plane — chord-preserving 2D arc, exactly like blade.py:
    `azimuth` sets the fall direction (base→tip chord in XY).
    `curl` (±1) bends the XY path left/right while keeping that chord fixed.
    Variable lean — lean(t) = LEAN_ANGLE × (1 − cos(t·π/2)) — gives the
    horizontal speed a zero start, so the blade grows straight up at the
    base before spreading out.

  Z — least-concave-majorant support-clearing curve:
    The spine starts at ground.  For each blade, both top-edge support paths
    are sampled as absolute obstacle heights, and the upper concave envelope
    selects contact points.  The final spine is that envelope directly, capped
    at 6 mm above the blade base terrain.
"""

import numpy as np
import trimesh
import pathlib
from scipy.interpolate import PchipInterpolator

# ── Config ────────────────────────────────────────────────────────────────────
TILE_W = TILE_H = 35.0          # mm
BASE_H          =  6.0          # mm — solid slab below terrain (GROUND)
                                 #       other tile types: 3.0 (WATER), 9.5 (MANMADE)
GRID_RES        = 256           # support-field resolution (cells per side)

# Terrain
TERRAIN_AMP     = 1.0           # mm — sinusoidal bump amplitude
TERRAIN_FREQ    = 1.5           # cycles across tile

# Blade population
N_BLADES        = 200           # tall blades
N_FILL          = 0             # short filler blades
SEED            = 42
CURL_MAX        = 0.6           # max lateral curl magnitude (±)

# Blade geometry (mm)
TALL_W_MIN,  TALL_W_MAX  = 1.5, 2    # flat face width at base
TALL_L_MIN,  TALL_L_MAX  = 4.0, 14.4  # body arc length
TALL_TL_MIN, TALL_TL_MAX = 1.2, 4.8   # tip taper arc length
FILL_W_MIN,  FILL_W_MAX  = 0.3, 0.5
FILL_L_MIN,  FILL_L_MAX  = 4.0, 7.2
FILL_TL_MIN, FILL_TL_MAX = 1.2, 2.4
GRASS_THICKNESS = 0.5          # mm — inverted triangular hull depth
GRASS_SUB_HULL_FRACTION = 0.5  # start support hull halfway down triangle sides

BASE_LEAN_ANGLE = np.radians(8)   # initial forward lean at base
LEAN_ANGLE      = np.radians(80)  # max lean at tip (nearly horizontal)
N_PATH          = 50              # spine sample points (more = smoother curve)

# Flow field — controls blade fall direction
# FLOW_TYPE: 'swirl'  — blades orbit a central point (CW or CCW, randomly chosen)
#            'linear' — all blades swept in one direction (like wind)
#            'radial' — blades fan outward from a source point
#            'drain'  — blades converge toward a sink point
#            'dipole' — S-curve field between two opposing poles
#            'curl'   — pure divergence-free noise, most organic/natural
FLOW_TYPE       = 'linear'
FLOW_CURL_NOISE = 0.30          # organic perturbation: 0 = pure base field, 1 = all noise
DIR_SPREAD      = np.radians(15) # per-blade Gaussian angle jitter around flow direction
CURL_FROM_CURV  = 0.80          # how much blade curl follows streamline curvature
                                  #   0 = random curl as before, 1 = purely curvature-driven

# Terrain-following / knot fit
CLEARANCE           = 0.10       # mm — gap above support surface (previous blade tops)
BASE_SINK           = 0.05       # mm — base point is buried slightly under local terrain
BASE_OBSTACLE_IGNORE_T = 0.20    # ignore support obstacles over the first 20% of blade length
COLLISION_REPAIR_PASSES = 8      # per-blade strict-hit repair attempts
MAX_STACK_HEIGHT    = 6.0        # mm — hard cap: never force a blade above this height
                                 #       above local terrain.  Previous blades that
                                 #       cascaded above this level are simply ignored;
                                 #       some intersection at high z is accepted rather
                                 #       than letting the pile grow indefinitely.

# Strict intersection checking (expensive — disable for fast iteration)
STRICT_MODE      = True          # check each new blade against all previously placed blades
STRICT_BASE_T    = 0.25          # ignore new-blade hits at t <= this (blade is still erupting
                                  # from terrain)

# Gravel / stones  (placed before grass; updates support_z so grass sits on top)
N_GRAVEL         = 6000         # number of stones
GRAVEL_R_MIN     = 0.048        # mm — minimum horizontal semi-axis
GRAVEL_R_MAX     = 0.42         # mm — maximum horizontal semi-axis
GRAVEL_FLAT_MIN  = 0.40         # stone height = this fraction × mean radius (flattest)
GRAVEL_FLAT_MAX  = 1.30         # stone height = this fraction × mean radius (roundest)
GRAVEL_AZ_SEGS   = 7            # azimuth facets per stone
GRAVEL_EL_SEGS   = 3            # elevation rings per stone (above base)
GRAVEL_SINK      = 0.01         # mm — base sunk below terrain so stones look embedded

OUTPUT = pathlib.Path("stl/grass.stl")

# ── Grid helpers ──────────────────────────────────────────────────────────────
GX = TILE_W / (GRID_RES - 1)    # mm between height/support samples
GY = TILE_H / (GRID_RES - 1)

def sample_grid(grid, x_mm, y_mm):
    """Bilinear sample — accepts scalars or numpy arrays."""
    scalar = np.ndim(x_mm) == 0
    i  = np.clip(np.asarray(x_mm, dtype=float) / GX, 0, GRID_RES - 1)
    j  = np.clip(np.asarray(y_mm, dtype=float) / GY, 0, GRID_RES - 1)
    i0 = np.floor(i).astype(int)
    j0 = np.floor(j).astype(int)
    i1 = np.minimum(i0 + 1, GRID_RES - 1)
    j1 = np.minimum(j0 + 1, GRID_RES - 1)
    fi = i - i0;  fj = j - j0
    result = (grid[j0, i0] * (1-fi) * (1-fj) +
              grid[j0, i1] *    fi  * (1-fj) +
              grid[j1, i0] * (1-fi) *    fj  +
              grid[j1, i1] *    fi  *    fj)
    return float(result) if scalar else result

def rasterise_into_support(support_z, path_xyz, half_widths):
    """Paint the blade's top surface into support_z with segment-continuous samples."""
    path = np.asarray(path_xyz)        # (n_pts, 3)
    hws  = np.asarray(half_widths)     # (n_pts,)

    samples = []
    for idx in range(len(path) - 1):
        p0 = path[idx]
        p1 = path[idx + 1]
        hw0 = float(hws[idx])
        hw1 = float(hws[idx + 1])
        seg_len = float(np.linalg.norm(p1[:2] - p0[:2]))
        n_steps = max(1, int(np.ceil(seg_len / (0.5 * min(GX, GY)))))
        for step in range(n_steps):
            a = step / n_steps
            p = (1.0 - a) * p0 + a * p1
            hw = (1.0 - a) * hw0 + a * hw1
            samples.append((float(p[0]), float(p[1]), float(p[2]), float(hw)))
    samples.append((float(path[-1, 0]), float(path[-1, 1]), float(path[-1, 2]), float(hws[-1])))

    for x, y, z, hw in samples:
        r_cells = max(1, int(hw / GX) + 2)
        ic = int(np.clip(x / GX, 0, GRID_RES - 1))
        jc = int(np.clip(y / GY, 0, GRID_RES - 1))

        lo_i = max(0, ic - r_cells);  hi_i = min(GRID_RES - 1, ic + r_cells)
        lo_j = max(0, jc - r_cells);  hi_j = min(GRID_RES - 1, jc + r_cells)

        di = np.arange(lo_i - ic, hi_i - ic + 1)
        dj = np.arange(lo_j - jc, hi_j - jc + 1)
        DI, DJ = np.meshgrid(di, dj, indexing='ij')   # (ni, nj)
        mask = (DI * GX) ** 2 + (DJ * GY) ** 2 <= hw * hw

        ii = (ic + DI[mask])
        jj = (jc + DJ[mask])
        support_z[jj, ii] = np.maximum(support_z[jj, ii], z)  # fancy index → must assign back

def terrain_normal_at(x_mm, y_mm):
    """Outward (upward) unit normal of terrain_z at (x_mm, y_mm) via central differences."""
    eps  = GX * 2
    dzdx = (sample_grid(terrain_z, x_mm + eps, y_mm) -
            sample_grid(terrain_z, x_mm - eps, y_mm)) / (2 * eps)
    dzdy = (sample_grid(terrain_z, x_mm, y_mm + eps) -
            sample_grid(terrain_z, x_mm, y_mm - eps)) / (2 * eps)
    n = np.array([-dzdx, -dzdy, 1.0])
    return n / np.linalg.norm(n)

# ── Gravel / stones — batch-vectorised ────────────────────────────────────────

def add_gravel(gravel_rng, support_z):
    """
    Place N_GRAVEL random stones across the whole tile surface.
    All geometry is built with numpy broadcasting; returns a single
    trimesh.Trimesh instead of 6 000 small ones.
    Also rasterises each stone's smooth ellipsoid surface into support_z so
    that grass blades are forced to sit on top of the stones.
    """
    N  = N_GRAVEL
    AZ = GRAVEL_AZ_SEGS
    EL = GRAVEL_EL_SEGS

    # ── Random parameters (all N stones at once) ──────────────────────────────
    rx_arr = gravel_rng.uniform(GRAVEL_R_MIN, GRAVEL_R_MAX, N)
    ry_arr = gravel_rng.uniform(GRAVEL_R_MIN, GRAVEL_R_MAX, N)
    h_frac = gravel_rng.uniform(GRAVEL_FLAT_MIN, GRAVEL_FLAT_MAX, N)
    height = 0.5 * (rx_arr + ry_arr) * h_frac
    angle  = gravel_rng.uniform(0, np.pi, N)
    margin = np.maximum(rx_arr, ry_arr)
    # draw uniform [0,1] then scale to [margin, TILE-margin]
    span_x = np.maximum(TILE_W - 2 * margin, 0.0)
    span_y = np.maximum(TILE_H - 2 * margin, 0.0)
    cx = margin + gravel_rng.uniform(0, 1, N) * span_x
    cy = margin + gravel_rng.uniform(0, 1, N) * span_y

    ca = np.cos(angle);  sa = np.sin(angle)     # (N,)
    tz = sample_grid(terrain_z, cx, cy)          # (N,) vectorised
    base_z = tz - GRAVEL_SINK                    # (N,)

    # ── Vertices ─────────────────────────────────────────────────────────────
    # Layout per stone:  apex(1) + rings(EL × AZ) + bottom_centre(1)
    vps = 1 + EL * AZ + 1      # verts per stone
    fps = AZ + AZ * (EL - 1) * 2 + AZ   # faces per stone
    all_verts = np.empty((N * vps, 3), dtype=float)
    all_faces = np.empty((N * fps, 3), dtype=np.int32)

    # Apex  (index 0 within each stone block)
    apex_idx = np.arange(N) * vps           # (N,)
    all_verts[apex_idx, 0] = cx
    all_verts[apex_idx, 1] = cy
    all_verts[apex_idx, 2] = base_z + height

    # Rings  ei=1..EL
    ei_arr = np.arange(1, EL + 1)           # (EL,)
    u_arr  = ei_arr / EL                    # (EL,)
    r_frac = np.sin(u_arr * np.pi / 2)      # (EL,)
    z_off  = np.cos(u_arr * np.pi / 2)      # (EL,)  multiply by height later

    ai_arr  = np.arange(AZ)
    theta   = 2 * np.pi * ai_arr / AZ       # (AZ,)
    cos_th  = np.cos(theta);  sin_th = np.sin(theta)

    # local XY of ring vertices before rotation: (EL, AZ)
    # lx[ei, ai] = rx * r_frac[ei] * cos_th[ai]  (broadcast over N below)
    # For all N stones simultaneously: (N, EL, AZ)
    lx = rx_arr[:, None, None] * r_frac[None, :, None] * cos_th[None, None, :]
    ly = ry_arr[:, None, None] * r_frac[None, :, None] * sin_th[None, None, :]

    wx = cx[:, None, None] + ca[:, None, None] * lx - sa[:, None, None] * ly
    wy = cy[:, None, None] + sa[:, None, None] * lx + ca[:, None, None] * ly
    wz = (base_z[:, None, None] +
          height[:, None, None] * z_off[None, :, None] *
          np.ones((1, 1, AZ)))               # broadcast AZ

    # ring vertex block starts at stone_base + 1, laid out as (EL, AZ) row-major
    ring_base = (np.arange(N) * vps + 1)[:, None, None]  # (N, 1, 1)
    ei_off    = (np.arange(EL) * AZ)[None, :, None]       # (1, EL, 1)
    ai_off    = np.arange(AZ)[None, None, :]               # (1, 1, AZ)
    ring_idx  = ring_base + ei_off + ai_off                # (N, EL, AZ)

    all_verts[ring_idx.ravel(), 0] = wx.ravel()
    all_verts[ring_idx.ravel(), 1] = wy.ravel()
    all_verts[ring_idx.ravel(), 2] = wz.ravel()

    # Bottom centre (index vps-1 within each stone block)
    bot_idx = np.arange(N) * vps + vps - 1
    all_verts[bot_idx, 0] = cx
    all_verts[bot_idx, 1] = cy
    all_verts[bot_idx, 2] = base_z

    # ── Faces (topology identical for every stone; offset by stone_base) ─────
    # Build the canonical face list for stone 0, then broadcast offsets.
    canon_faces = []

    # Apex → first ring
    for ai in range(AZ):
        canon_faces.append([0,  1 + ai,  1 + (ai + 1) % AZ])

    # Side strips between rings
    for ei in range(1, EL):
        row_a = 1 + (ei - 1) * AZ
        row_b = 1 +  ei      * AZ
        for ai in range(AZ):
            a0 = row_a + ai;            a1 = row_a + (ai + 1) % AZ
            b0 = row_b + ai;            b1 = row_b + (ai + 1) % AZ
            canon_faces.append([a0, b0, a1])
            canon_faces.append([a1, b0, b1])

    # Base ring → bottom centre
    last_ring = 1 + (EL - 1) * AZ
    bot_local = vps - 1
    for ai in range(AZ):
        a = last_ring + ai
        b = last_ring + (ai + 1) % AZ
        canon_faces.append([a, bot_local, b])

    canon_faces = np.array(canon_faces, dtype=np.int32)  # (fps, 3)

    # Replicate for all N stones: offset each stone's faces by stone_base
    stone_bases = (np.arange(N) * vps).astype(np.int32)  # (N,)
    all_faces = (canon_faces[None, :, :] +
                 stone_bases[:, None, None]).reshape(-1, 3)

    mesh = trimesh.Trimesh(
        vertices = all_verts,
        faces    = all_faces,
        process  = False,
    )
    mesh.fix_normals()

    # ── Rasterise stone tops into support_z ──────────────────────────────────
    # For each stone, paint the smooth half-ellipsoid surface height into
    # support_z so the grass blade solver sees raised ground under each stone.
    # Formula: at world point (wx, wy), inverse-rotate to local (lx, ly), then
    #   d² = (lx/rx)² + (ly/ry)²
    #   z_top = base_z + height * sqrt(1 − d²)   when d² ≤ 1
    for s in range(N):
        _cx, _cy = cx[s], cy[s]
        _rx, _ry = rx_arr[s], ry_arr[s]
        _h       = height[s]
        _ca, _sa = ca[s], sa[s]
        _bz      = base_z[s]
        r_max    = max(_rx, _ry)

        # Grid cell bounding box
        i_lo = max(0,          int((_cx - r_max) / GX))
        i_hi = min(GRID_RES-1, int((_cx + r_max) / GX) + 1)
        j_lo = max(0,          int((_cy - r_max) / GY))
        j_hi = min(GRID_RES-1, int((_cy + r_max) / GY) + 1)
        if i_lo > i_hi or j_lo > j_hi:
            continue

        # World XY of every cell in the bounding box
        ii_g = np.arange(i_lo, i_hi + 1)   # (ni,)
        jj_g = np.arange(j_lo, j_hi + 1)   # (nj,)
        II, JJ = np.meshgrid(ii_g, jj_g)   # (nj, ni)
        wx_g = II * GX - _cx
        wy_g = JJ * GY - _cy

        # Inverse-rotate into local stone axes
        lx_g =  _ca * wx_g + _sa * wy_g
        ly_g = -_sa * wx_g + _ca * wy_g

        d2 = (lx_g / _rx) ** 2 + (ly_g / _ry) ** 2
        inside = d2 <= 1.0
        if not np.any(inside):
            continue

        z_top = np.where(inside, _bz + _h * np.sqrt(np.maximum(0.0, 1.0 - d2)),
                         -np.inf)
        sl = support_z[j_lo:j_hi+1, i_lo:i_hi+1]
        np.maximum(sl, z_top, out=sl)

    return [mesh]


# ── Terrain ───────────────────────────────────────────────────────────────────
print("Building terrain...")
iy, ix = np.mgrid[0:GRID_RES, 0:GRID_RES]
x_grid = ix * GX
y_grid = iy * GY
u_grid = x_grid / TILE_W
v_grid = y_grid / TILE_H
edge_envelope = np.sin(np.pi * u_grid) * np.sin(np.pi * v_grid)
undulation = (
    np.sin(2 * np.pi * TERRAIN_FREQ * u_grid) *
    np.cos(2 * np.pi * TERRAIN_FREQ * v_grid)
)
terrain_z = (TERRAIN_AMP * edge_envelope * undulation).astype(float)

support_z = terrain_z.copy()

# ── Flow field ────────────────────────────────────────────────────────────────

def build_flow_field():
    """
    Build a flow angle field and signed curvature field on the terrain grid.

    The field is a unit-vector field over the tile, expressed as a bearing
    angle (atan2(fx, fy), 0 = +Y / north, π/2 = +X / east).  It drives:
      • blade azimuth  — fall direction in XY (sampled + small jitter)
      • blade curl     — how much the blade sweeps left/right (signed curvature)

    Construction:
      1. Analytic base field (swirl / linear / radial / drain / dipole / curl).
      2. Blend with divergence-free curl noise for organic variation.
      3. Derive angle field θ = atan2(fx, fy).
      4. Derive signed curvature κ = ∇θ · f̂, normalised to [-1, 1].
    """
    frng = np.random.default_rng(SEED ^ 0x464C4F57)   # independent stream

    # Normalised XY coords  [-0.5, 0.5] × [-0.5, 0.5]
    xn = (x_grid / TILE_W - 0.5).astype(float)
    yn = (y_grid / TILE_H - 0.5).astype(float)

    # ── 1. Base field ─────────────────────────────────────────────────────────
    if FLOW_TYPE == 'swirl':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        sign = frng.choice([-1.0, 1.0])   # CW or CCW, random each seed
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx =  sign * dy / r             # tangent to radius circle
        bfy = -sign * dx / r

    elif FLOW_TYPE == 'linear':
        angle = frng.uniform(0, 2 * np.pi)
        bfx = np.full_like(xn, np.sin(angle))
        bfy = np.full_like(xn, np.cos(angle))

    elif FLOW_TYPE == 'radial':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx = dx / r;  bfy = dy / r

    elif FLOW_TYPE == 'drain':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx = -dx / r;  bfy = -dy / r

    elif FLOW_TYPE == 'dipole':
        sep  = frng.uniform(0.15, 0.25)
        ang  = frng.uniform(0, 2 * np.pi)
        cx1, cy1 =  np.cos(ang) * sep,  np.sin(ang) * sep
        cx2, cy2 = -cx1, -cy1
        r1sq = (xn - cx1)**2 + (yn - cy1)**2 + 1e-4
        r2sq = (xn - cx2)**2 + (yn - cy2)**2 + 1e-4
        bfx = (xn - cx1) / r1sq - (xn - cx2) / r2sq
        bfy = (yn - cy1) / r1sq - (yn - cy2) / r2sq

    else:  # 'curl' — pure curl noise; weak +Y bias just to orient it
        bfx = np.zeros_like(xn)
        bfy = np.ones_like(xn)

    mag = np.sqrt(bfx**2 + bfy**2) + 1e-9
    bfx, bfy = bfx / mag, bfy / mag

    # ── 2. Curl noise: divergence-free perturbation ───────────────────────────
    # Scalar stream-function P = sum of low-frequency sinusoids.
    # curl(P) = (∂P/∂y, −∂P/∂x) is always divergence-free.
    P = np.zeros_like(xn)
    for _ in range(4):
        fx_ = frng.uniform(1.5, 4.0)
        fy_ = frng.uniform(1.5, 4.0)
        phx = frng.uniform(0, 2 * np.pi)
        phy = frng.uniform(0, 2 * np.pi)
        amp = frng.uniform(0.3, 1.0)
        P  += amp * np.sin(fx_ * 2*np.pi * xn + phx) * np.cos(fy_ * 2*np.pi * yn + phy)

    dPdy, dPdx = np.gradient(P, GY, GX)     # ∂P/∂y along axis-0, ∂P/∂x along axis-1
    cnx, cny = dPdy, -dPdx                  # divergence-free 2D curl
    cmag = np.sqrt(cnx**2 + cny**2) + 1e-9
    cnx /= cmag;  cny /= cmag

    s  = FLOW_CURL_NOISE
    fx = (1 - s) * bfx + s * cnx
    fy = (1 - s) * bfy + s * cny
    mag = np.sqrt(fx**2 + fy**2) + 1e-9
    fx /= mag;  fy /= mag

    # ── 3. Angle field: bearing from +Y ──────────────────────────────────────
    angle_field = np.arctan2(fx, fy)   # 0 = +Y (north), π/2 = +X (east)

    # ── 4. Signed curvature: κ = ∇θ · f̂ ─────────────────────────────────────
    # Positive = streamline bends CW (increasing azimuth), negative = CCW.
    dθdy, dθdx = np.gradient(angle_field, GY, GX)
    kappa = dθdx * fx + dθdy * fy
    scale = np.percentile(np.abs(kappa), 95) + 1e-9
    curv_field = np.clip(kappa / scale, -1.0, 1.0)

    return angle_field.astype(float), curv_field.astype(float)

print("Building flow field...")
flow_angle_field, flow_curv_field = build_flow_field()
print(f"  type: {FLOW_TYPE}")

# ── Blade placement ───────────────────────────────────────────────────────────
rng = np.random.default_rng(SEED)

def place_blades(n, w_min, w_max, l_min, l_max, tl_min, tl_max):
    if n == 0:
        return []
    cols = int(np.ceil(np.sqrt(n)))
    rows = int(np.ceil(n / cols))
    cw, ch = TILE_W / cols, TILE_H / rows
    cells = [(c, r) for c in range(cols) for r in range(rows)]
    rng.shuffle(cells)
    edge = w_max / 2 + 0.2
    out = []
    for c, r in cells:
        if len(out) >= n:
            break
        bx = float(np.clip((c + rng.uniform(0.1, 0.9)) * cw, edge, TILE_W - edge))
        by = float(np.clip((r + rng.uniform(0.1, 0.9)) * ch, edge, TILE_H - edge))
        # Direction: flow angle at base + small Gaussian jitter
        base_angle = float(sample_grid(flow_angle_field, bx, by))
        direction  = base_angle + float(rng.normal(0, DIR_SPREAD))

        # Curl: sign and rough magnitude from local streamline curvature.
        # kappa ∈ [-1,1]: +1 = tight CW bend, -1 = tight CCW bend, 0 = straight.
        kappa     = float(sample_grid(flow_curv_field, bx, by))
        rand_curl = float(rng.uniform(-CURL_MAX, CURL_MAX))
        curv_curl = float(np.sign(kappa) * (kappa**2) * CURL_MAX *
                          rng.uniform(0.4, 1.0))  # quadratic: near-zero kappa → near-zero curl
        curl = float(np.clip(
            CURL_FROM_CURV * curv_curl + (1 - CURL_FROM_CURV) * rand_curl,
            -CURL_MAX, CURL_MAX,
        ))

        out.append(dict(
            base_x    = bx,
            base_y    = by,
            width     = rng.uniform(w_min,  w_max),
            length    = rng.uniform(l_min,  l_max),
            tip_len   = rng.uniform(tl_min, tl_max),
            direction = direction,
            curl      = curl,
        ))
    return out

tall  = place_blades(N_BLADES, TALL_W_MIN, TALL_W_MAX,
                     TALL_L_MIN, TALL_L_MAX, TALL_TL_MIN, TALL_TL_MAX)
fills = place_blades(N_FILL,   FILL_W_MIN, FILL_W_MAX,
                     FILL_L_MIN, FILL_L_MAX, FILL_TL_MIN, FILL_TL_MAX)
blades = tall + fills
# Sort downstream-first: start placing at the exit edge of the tile, work back upstream.
# Upstream blades lean over already-placed downstream blades and are forced to arch over them.
_mfx = float(np.mean(np.sin(flow_angle_field)))
_mfy = float(np.mean(np.cos(flow_angle_field)))
blades.sort(key=lambda b: -(_mfx * b['base_x'] + _mfy * b['base_y']))
print(f"Placed {len(blades)} blades  (flow sort: fx={_mfx:.2f} fy={_mfy:.2f})")

# ── Low-level tube mesh ────────────────────────────────────────────────────────

def _blade_frame(path):
    """Return tangent, width-axis, and down-axis vectors for each path ring."""
    path = np.asarray(path, dtype=float)
    tangs = np.empty_like(path)
    tangs[:-1] = path[1:] - path[:-1]
    tangs[-1]  = path[-1] - path[-2]
    t_norms    = np.linalg.norm(tangs, axis=1, keepdims=True) + 1e-9
    tangs     /= t_norms

    txy_norm = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2)
    has_xy   = txy_norm > 1e-6
    up_locs  = np.zeros_like(path)
    up_locs[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up_locs[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up_locs[~has_xy]   = [1.0, 0.0, 0.0]

    down_locs = np.cross(up_locs, tangs)
    down_norms = np.linalg.norm(down_locs, axis=1, keepdims=True) + 1e-9
    down_locs /= down_norms
    flip = down_locs[:, 2] > 0.0
    down_locs[flip] *= -1.0
    return tangs, up_locs, down_locs

def _build_tube_mesh(spine_3d, widths, thickness=GRASS_THICKNESS):
    """
    Watertight triangular-prism tube following spine_3d.

    Triangular cross-section (3 verts/ring):
      V0 — lower hull apex,  V1 — right edge,  V2 — left edge

    The width edge remains on the support curve.  The apex is offset by
    thickness along the local down vector perpendicular to the 3-D curve tangent.
    """
    path  = np.asarray(spine_3d, dtype=float)   # (n_pts, 3)
    W_arr = np.asarray(widths,   dtype=float)    # (n_pts,)
    n_pts = len(path)
    n     = 3                                    # verts per ring (triangular cross-section)

    # Pre-allocate
    nv = n * n_pts + 2           # rings + base_cap + tip_cap
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = 0;  fi = 0

    _, up_locs, down_locs = _blade_frame(path)

    # ── Fill ring vertices ────────────────────────────────────────────────────
    half_W = (W_arr / 2.0)[:, None]          # (n_pts, 1)
    # V0: lower apex,  V1/V2: width edge on the support curve.
    ring_v = np.stack([
        path + thickness * down_locs,
        path + half_W * up_locs,
        path - half_W * up_locs,
    ], axis=1)                             # (n_pts, 3, 3)

    # Write rings into pre-allocated array
    for i in range(n_pts):
        verts[vi:vi+n] = ring_v[i]
        vi += n

    # Base cap and tip cap vertices
    v_base = vi;  verts[vi] = path[0];   vi += 1
    v_tip  = vi;  verts[vi] = path[-1];  vi += 1

    # ── Faces ─────────────────────────────────────────────────────────────────
    # Base cap
    r0 = 0
    for i in range(n):
        faces[fi] = [v_base, r0 + (i+1) % n, r0 + i];  fi += 1

    # Side quads (two triangles each)
    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for i in range(n):
            i1 = (i + 1) % n
            faces[fi]   = [ra+i,  rb+i,  ra+i1];  fi += 1
            faces[fi]   = [ra+i1, rb+i,  rb+i1];  fi += 1

    # Tip cap
    rl = (n_pts - 1) * n
    for i in range(n):
        faces[fi] = [rl+i, rl+(i+1) % n, v_tip];  fi += 1

    mesh = trimesh.Trimesh(
        vertices = verts[:vi],
        faces    = faces[:fi].astype(int),
        process  = False,
    )
    mesh.fix_normals()
    return mesh


def _drop_to_support(point, down_vec, support_z):
    """Move point along down_vec until it intersects the current support field."""
    start = np.asarray(point, dtype=float)
    down = np.asarray(down_vec, dtype=float)
    if down[2] >= -1e-6:
        down = np.array([0.0, 0.0, -1.0])

    def clearance_at(dist):
        p = start + dist * down
        return p[2] - sample_grid(support_z, p[0], p[1])

    if clearance_at(0.0) <= 0.0:
        return start

    hi = 0.25
    while hi < BASE_H + MAX_STACK_HEIGHT + GRASS_THICKNESS + 2.0 and clearance_at(hi) > 0.0:
        hi *= 2.0
    if clearance_at(hi) > 0.0:
        return start + hi * down

    lo = 0.0
    for _ in range(16):
        mid = 0.5 * (lo + hi)
        if clearance_at(mid) > 0.0:
            lo = mid
        else:
            hi = mid
    return start + hi * down


def _build_sub_hull_mesh(spine_3d, widths, support_z,
                         thickness=GRASS_THICKNESS,
                         side_fraction=GRASS_SUB_HULL_FRACTION):
    """
    Separate printable support hull under the blade.

    Each cross-section starts halfway down the two triangle sides, then drops a
    third point in the same triangle plane until it touches the nearest support.
    """
    path = np.asarray(spine_3d, dtype=float)
    W_arr = np.asarray(widths, dtype=float)
    n_pts = len(path)
    n = 3
    _, up_locs, down_locs = _blade_frame(path)
    half_W = (W_arr / 2.0)[:, None]

    apex = path + thickness * down_locs
    right = path + half_W * up_locs
    left = path - half_W * up_locs
    side_r = right + side_fraction * (apex - right)
    side_l = left + side_fraction * (apex - left)
    centers = 0.5 * (side_r + side_l)

    lower = np.empty_like(path)
    for idx in range(n_pts):
        lower[idx] = _drop_to_support(centers[idx], down_locs[idx], support_z)

    ring_v = np.stack([lower, side_r, side_l], axis=1)
    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = 0
    fi = 0

    for idx in range(n_pts):
        verts[vi:vi+n] = ring_v[idx]
        vi += n

    v_base = vi; verts[vi] = np.mean(ring_v[0], axis=0); vi += 1
    v_tip = vi; verts[vi] = np.mean(ring_v[-1], axis=0); vi += 1

    r0 = 0
    for idx in range(n):
        faces[fi] = [v_base, r0 + (idx + 1) % n, r0 + idx]; fi += 1

    for k in range(n_pts - 1):
        ra = k * n
        rb = (k + 1) * n
        for idx in range(n):
            i1 = (idx + 1) % n
            faces[fi] = [ra + idx, rb + idx, ra + i1]; fi += 1
            faces[fi] = [ra + i1, rb + idx, rb + i1]; fi += 1

    rl = (n_pts - 1) * n
    for idx in range(n):
        faces[fi] = [rl + idx, rl + (idx + 1) % n, v_tip]; fi += 1

    mesh = trimesh.Trimesh(
        vertices=verts[:vi],
        faces=faces[:fi].astype(int),
        process=False,
    )
    mesh.fix_normals()
    return mesh


# ── Parameterised grass blade ─────────────────────────────────────────────────

def blade_footprint_inside_tile(spine_3d, widths):
    """Conservative XY footprint check: spine plus half width must stay inside."""
    path = np.asarray(spine_3d)
    hws  = np.asarray(widths) / 2.0
    if np.any(path[:, 0] - hws < 0.0) or np.any(path[:, 0] + hws > TILE_W):
        return False
    if np.any(path[:, 1] - hws < 0.0) or np.any(path[:, 1] + hws > TILE_H):
        return False
    return True

def _upper_concave_envelope(t_arr, height_arr):
    """Least concave majorant through ordered obstacle points."""
    points = [(float(t_arr[0]), float(height_arr[0]), 0)]
    points.extend(
        (float(t_arr[i]), float(height_arr[i]), i)
        for i in range(1, len(t_arr) - 1)
        if np.isfinite(height_arr[i])
    )
    if np.isfinite(height_arr[-1]):
        points.append((float(t_arr[-1]), float(height_arr[-1]), len(t_arr) - 1))

    def slope(a, b):
        return (b[1] - a[1]) / (b[0] - a[0])

    stack = []
    for point in points:
        stack.append(point)
        while len(stack) >= 3:
            a, b, c = stack[-3], stack[-2], stack[-1]
            if slope(b, c) > slope(a, b):
                stack.pop(-2)
            else:
                break
    return stack


def _smooth_contact_curve(t_arr, contacts):
    """Shape-preserving C1 cubic through the shrink-wrap contact points."""
    ctrl_t = np.array([p[0] for p in contacts], dtype=float)
    ctrl_z = np.array([p[1] for p in contacts], dtype=float)
    if len(ctrl_t) <= 2:
        return np.interp(t_arr, ctrl_t, ctrl_z)
    return PchipInterpolator(ctrl_t, ctrl_z)(t_arr)


def _fit_sample_envelope_spine(t_arr, floor_z, terrain_z_path):
    """Return absolute upper-concave-envelope spine z, or None if capped out."""
    base_z = float(floor_z[0])
    ceiling_z = base_z + MAX_STACK_HEIGHT
    obstacle_z = np.asarray(floor_z, dtype=float).copy()
    if np.any(obstacle_z > ceiling_z + 1e-6):
        return None

    contacts = _upper_concave_envelope(t_arr, obstacle_z)
    spine_z = _smooth_contact_curve(t_arr, contacts)
    if np.any(spine_z < obstacle_z - 1e-6) or np.any(spine_z > ceiling_z + 1e-6):
        return None
    return spine_z

def make_grass_blade(support_z, base_pos, azimuth, length, width, tip_length,
                     lean_angle=LEAN_ANGLE, curl=0.0, n_path=N_PATH,
                     extra_floor_z=None):
    """
    Build a terrain-following floppy grass blade.
    Returns (mesh, spine_3d, half_widths).
    """
    bx, by  = float(base_pos[0]), float(base_pos[1])
    total_l = length + tip_length
    dt      = 1.0 / (n_path - 1)
    _CURL_MAX = np.pi                  # |curl|=1 gives ±180 deg lateral sweep

    # ── XY path: vectorised cumsum ───────────────────────────────────────────
    k_arr    = np.arange(1, n_path)
    t_mid    = (k_arr - 0.5) * dt
    lean_v   = BASE_LEAN_ANGLE + (lean_angle - BASE_LEAN_ANGLE) * (1.0 - np.cos(t_mid * np.pi / 2.0))
    az_v     = azimuth + curl * _CURL_MAX * t_mid
    ds       = total_l * dt
    dxr      = np.sin(az_v) * np.sin(lean_v) * ds
    dyr      = np.cos(az_v) * np.sin(lean_v) * ds
    xr       = np.concatenate([[0.0], np.cumsum(dxr)])
    yr       = np.concatenate([[0.0], np.cumsum(dyr)])

    # Chord-preserving 2D rotation
    tip_dist = np.sqrt(xr[-1]**2 + yr[-1]**2)
    if tip_dist > 1e-6:
        tip_angle    = np.arctan2(xr[-1], yr[-1])
        rot          = tip_angle - azimuth
        cos_r, sin_r = np.cos(rot), np.sin(rot)
        xrot = xr * cos_r - yr * sin_r
        yrot = xr * sin_r + yr * cos_r
    else:
        xrot, yrot = xr, yr

    # ── Z profile: lowest smooth curve that clears current support ────────────
    # Pass 1 — XY path + support sampling
    xs_arr = bx + xrot       # (n_path,)
    ys_arr = by + yrot       # (n_path,)
    tz_arr = sample_grid(terrain_z, xs_arr, ys_arr)   # (n_path,)

    # Taper widths + lateral top-edge XY positions — all from XY geometry alone,
    # so computable before the z profile.
    k_arr      = np.arange(n_path)
    s_arr      = k_arr * dt * total_l
    t_tip_arr  = np.clip((s_arr - length) / (tip_length + 1e-9), 0.0, 1.0)
    widths_arr = width * np.cos(t_tip_arr * np.pi / 2.0)   # (n_path,)  tapered
    hw_arr     = widths_arr / 2.0

    up_pre = _compute_up_locs(np.stack([xs_arr, ys_arr, np.zeros(n_path)], axis=1))
    v1_xs  = xs_arr + hw_arr * up_pre[:, 0]   # right top-edge XY
    v1_ys  = ys_arr + hw_arr * up_pre[:, 1]
    v2_xs  = xs_arr - hw_arr * up_pre[:, 0]   # left  top-edge XY
    v2_ys  = ys_arr - hw_arr * up_pre[:, 1]

    # Sample support at both top-edge positions, then take the max: this is the
    # highest previously placed surface under either edge at each ring.
    sz_v1    = sample_grid(support_z, v1_xs,  v1_ys)
    sz_v2    = sample_grid(support_z, v2_xs,  v2_ys)
    edge_support = np.maximum(sz_v1, sz_v2)

    # Base is one buried contact point. Support obstacles are ignored over the
    # first BASE_OBSTACLE_IGNORE_T of the blade; after that they drive the curve.
    t_arr = np.linspace(0.0, 1.0, n_path)
    support_floor = edge_support + CLEARANCE
    support_floor[t_arr < BASE_OBSTACLE_IGNORE_T] = -np.inf
    floor_z = support_floor.copy()
    floor_z[t_arr < BASE_OBSTACLE_IGNORE_T] = -np.inf
    floor_z[0] = float(tz_arr[0] - BASE_SINK)
    if extra_floor_z is not None:
        floor_z = np.maximum(floor_z, np.asarray(extra_floor_z, dtype=float))
        floor_z[0] = float(tz_arr[0] - BASE_SINK)

    spine_z = _fit_sample_envelope_spine(
        t_arr,
        floor_z,
        tz_arr,
    )
    if spine_z is None:
        raise RuntimeError("knot curve fit failed")

    path_xyz = np.stack([xs_arr, ys_arr, spine_z], axis=1)   # (n_path, 3)

    mesh = _build_tube_mesh(path_xyz, widths_arr)
    sub_hull_mesh = _build_sub_hull_mesh(path_xyz, widths_arr, support_z)
    return mesh, sub_hull_mesh, path_xyz, widths_arr


# ── Terrain mesh ──────────────────────────────────────────────────────────────

def make_heightmap_solid(z_grid, tile_w, tile_h, base_h, subsample=4):
    """Watertight solid: top = sinusoidal surface, bottom = flat at -base_h."""
    res     = z_grid.shape[0]
    sr_list = list(range(0, res, subsample))
    if sr_list[-1] != res - 1:
        sr_list.append(res - 1)
    sr  = sr_list
    ns  = len(sr)

    gx, gy = tile_w / (res - 1), tile_h / (res - 1)
    verts, faces = [], []

    top_idx = {}
    for jj, j in enumerate(sr):
        for ii, i in enumerate(sr):
            top_idx[(ii, jj)] = len(verts)
            verts.append([i * gx, j * gy, z_grid[j, i]])

    bot_z   = -base_h
    bot_off = len(verts)
    for jj, j in enumerate(sr):
        for ii, i in enumerate(sr):
            verts.append([i * gx, j * gy, bot_z])

    def top(ii, jj): return top_idx[(ii, jj)]
    def bot(ii, jj): return bot_off + jj * ns + ii

    for jj in range(ns - 1):
        for ii in range(ns - 1):
            a, b = top(ii,jj), top(ii+1,jj)
            c, d = top(ii,jj+1), top(ii+1,jj+1)
            faces += [[a, b, d], [a, d, c]]

    for jj in range(ns - 1):
        for ii in range(ns - 1):
            a, b = bot(ii,jj), bot(ii+1,jj)
            c, d = bot(ii,jj+1), bot(ii+1,jj+1)
            faces += [[a, d, b], [a, c, d]]

    for ii in range(ns - 1):
        faces += [[top(ii,0),    bot(ii,0),    top(ii+1,0)],
                  [top(ii+1,0),  bot(ii,0),    bot(ii+1,0)]]
        faces += [[top(ii,ns-1), top(ii+1,ns-1), bot(ii,ns-1)],
                  [top(ii+1,ns-1), bot(ii+1,ns-1), bot(ii,ns-1)]]
    for jj in range(ns - 1):
        faces += [[top(0,jj),    top(0,jj+1),  bot(0,jj)],
                  [top(0,jj+1),  bot(0,jj+1),  bot(0,jj)]]
        faces += [[top(ns-1,jj), bot(ns-1,jj), top(ns-1,jj+1)],
                  [top(ns-1,jj+1), bot(ns-1,jj), bot(ns-1,jj+1)]]

    mesh = trimesh.Trimesh(
        vertices = np.array(verts, dtype=float),
        faces    = np.array(faces,  dtype=int),
        process  = False,
    )
    mesh.fix_normals()
    return mesh

# ── Strict intersection checker ───────────────────────────────────────────────

def _compute_up_locs(path_xyz):
    """
    Horizontal perpendicular-to-spine unit vectors, matching _build_tube_mesh.
    Returns (n_pts, 3) array; Z component is always 0.
    """
    path         = np.asarray(path_xyz, dtype=float)
    tangs        = np.empty_like(path)
    tangs[:-1]   = path[1:] - path[:-1]
    tangs[-1]    = path[-1] - path[-2]
    txy_norm     = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2) + 1e-9
    has_xy       = txy_norm > 1e-6
    up           = np.zeros_like(path)
    up[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up[~has_xy]   = [1.0, 0.0, 0.0]
    return up


def _seg_tri_batch(P, Q, A, B, C, eps=1e-8):
    """
    Vectorised Möller-Trumbore segment-triangle intersection test.

    P, Q : (n, 3) — segment start / end points
    A, B, C : (m, 3) — triangle vertices

    Returns bool (n, m) — True where segment [P[i],Q[i]] intersects
    triangle (A[j],B[j],C[j]).
    """
    D  = Q - P              # (n, 3)  segment direction
    AB = B - A              # (m, 3)  edge1
    AC = C - A              # (m, 3)  edge2

    # PV[i,j] = cross(D[i], AC[j])
    PV = np.cross(D[:, None, :], AC[None, :, :])        # (n, m, 3)

    # DT[i,j] = dot(AB[j], PV[i,j])
    DT = np.einsum('mk,nmk->nm', AB, PV)                # (n, m)

    valid   = np.abs(DT) >= eps
    inv_DT  = np.where(valid, 1.0 / np.where(valid, DT, 1.0), 0.0)

    # TV[i,j] = P[i] - A[j]
    TV = P[:, None, :] - A[None, :, :]                  # (n, m, 3)

    # U = dot(TV, PV) * inv_DT
    U = np.einsum('nmk,nmk->nm', TV, PV) * inv_DT       # (n, m)

    # QV[i,j] = cross(TV[i,j], AB[j])
    QV = np.cross(TV, AB[None, :, :])                   # (n, m, 3)

    # V = dot(D[i], QV[i,j]) * inv_DT
    V = np.einsum('nk,nmk->nm', D, QV) * inv_DT         # (n, m)

    # T = dot(AC[j], QV[i,j]) * inv_DT  (parameter along segment)
    T = np.einsum('mk,nmk->nm', AC, QV) * inv_DT        # (n, m)

    return (valid &
            (U >= -eps) & (U <= 1.0 + eps) &
            (V >= -eps) & (U + V <= 1.0 + eps) &
            (T >= -eps) & (T <= 1.0 + eps))


def _blade_top_intersections(spine_a, hw_a, up_a, spine_b, hw_b, up_b):
    """
    Geometric top-surface intersection test (vectorised Möller-Trumbore).

    The top surface of each blade is the strip of quads whose vertices are:
        V1[i] = spine[i] + hw[i] * up[i]   (right top edge)
        V2[i] = spine[i] - hw[i] * up[i]   (left  top edge)

    We test each cross-edge [V2_A[i], V1_A[i]] against every top-face
    triangle of B, and vice versa.  Returns list of (t_a, t_b) — parameter
    values along each blade where the surfaces actually intersect.
    """
    na = len(spine_a);  nb = len(spine_b)

    V1_A = spine_a + hw_a[:, None] * up_a   # (na, 3) right top edge
    V2_A = spine_a - hw_a[:, None] * up_a   # (na, 3) left  top edge
    V1_B = spine_b + hw_b[:, None] * up_b
    V2_B = spine_b - hw_b[:, None] * up_b

    # 3-D bounding-box early exit
    a_lo = np.minimum(V1_A.min(axis=0), V2_A.min(axis=0))
    a_hi = np.maximum(V1_A.max(axis=0), V2_A.max(axis=0))
    b_lo = np.minimum(V1_B.min(axis=0), V2_B.min(axis=0))
    b_hi = np.maximum(V1_B.max(axis=0), V2_B.max(axis=0))
    if np.any(a_hi < b_lo) or np.any(b_hi < a_lo):
        return []

    t_scale_a = 1.0 / max(na - 1, 1)
    t_scale_b = 1.0 / max(nb - 1, 1)

    # Triangles of B's top strip (two per quad):
    #   T1[j] = (V2_B[j], V1_B[j],   V1_B[j+1])
    #   T2[j] = (V2_B[j], V1_B[j+1], V2_B[j+1])
    tA_B = np.concatenate([V2_B[:-1], V2_B[:-1]], axis=0)  # (2*(nb-1), 3)
    tB_B = np.concatenate([V1_B[:-1], V1_B[1:]],  axis=0)
    tC_B = np.concatenate([V1_B[1:],  V2_B[1:]],  axis=0)
    qi_B = np.concatenate([np.arange(nb - 1), np.arange(nb - 1)])  # quad index in B

    # Triangles of A's top strip
    tA_A = np.concatenate([V2_A[:-1], V2_A[:-1]], axis=0)
    tB_A = np.concatenate([V1_A[:-1], V1_A[1:]],  axis=0)
    tC_A = np.concatenate([V1_A[1:],  V2_A[1:]],  axis=0)
    qi_A = np.concatenate([np.arange(na - 1), np.arange(na - 1)])

    results = set()

    # Cross-edges of A  vs  top-face triangles of B  →  (na, 2*(nb-1))
    hit_AB = _seg_tri_batch(V2_A, V1_A, tA_B, tB_B, tC_B)
    for ia, itri in zip(*np.where(hit_AB)):
        results.add((int(ia), int(qi_B[itri])))

    # Cross-edges of B  vs  top-face triangles of A  →  (nb, 2*(na-1))
    hit_BA = _seg_tri_batch(V2_B, V1_B, tA_A, tB_A, tC_A)
    for ib, itri in zip(*np.where(hit_BA)):
        results.add((int(qi_A[itri]), int(ib)))

    return sorted(
        [(ia * t_scale_a, ib * t_scale_b) for ia, ib in results],
        key=lambda x: x[0],
    )


def collect_strict_hits(spine, hw, up_locs, placed):
    hits_out = []
    for prev_idx, prev_spine, prev_hw, prev_up in placed:
        hits = _blade_top_intersections(spine, hw, up_locs,
                                        prev_spine, prev_hw, prev_up)
        for t_a, t_b in hits:
            if t_a <= STRICT_BASE_T:
                continue
            hits_out.append((prev_idx, prev_spine, t_a, t_b))
    return hits_out


def strict_check(blade_idx, bl, spine, hw, up_locs, placed):
    """
    Check blade_idx against all placed blades using geometric top-surface
    intersection test.  Print one line per hit, suppressing near-base
    overlaps (t_a <= STRICT_BASE_T).
    """
    bx, by = bl['base_x'], bl['base_y']
    reported = 0
    for prev_idx, prev_spine, t_a, t_b in collect_strict_hits(spine, hw, up_locs, placed):
        ia  = round(t_a * (len(spine)      - 1))
        ib  = round(t_b * (len(prev_spine) - 1))
        ix  = float(spine[ia, 0])
        iy  = float(spine[ia, 1])
        iz_a = float(spine[ia, 2])
        iz_b = float(prev_spine[ib, 2])
        print(f"  STRICT blade {blade_idx} (base {bx:.1f},{by:.1f}) "
              f"t={t_a:.2f} ↔ blade {prev_idx} t={t_b:.2f} "
              f"@ ({ix:.1f},{iy:.1f})  z_new={iz_a:.2f}  z_old={iz_b:.2f}  "
              f"TOP-SURFACE geometric hit")
        reported += 1
        if reported >= 8:
            print(f"  STRICT   ... (more hits suppressed)")
            return reported
    return reported


def add_collision_repairs(repair_floor, spine, strict_hits):
    """Raise only the path samples involved in strict top-surface hits."""
    n = len(spine)
    for _prev_idx, prev_spine, t_a, t_b in strict_hits:
        ia = int(np.clip(round(t_a * (n - 1)), 0, n - 1))
        ib = int(np.clip(round(t_b * (len(prev_spine) - 1)), 0, len(prev_spine) - 1))
        required_z = float(max(prev_spine[ib, 2] + CLEARANCE,
                               spine[ia, 2] + CLEARANCE))

        # Give the smoother a small local plateau instead of a one-sample spike.
        for offset, weight in ((-2, 0.35), (-1, 0.70), (0, 1.0), (1, 0.70), (2, 0.35)):
            j = ia + offset
            if 0 <= j < n:
                local_z = spine[j, 2] + (required_z - spine[ia, 2]) * weight
                repair_floor[j] = max(repair_floor[j], local_z)


# ── Main loop ─────────────────────────────────────────────────────────────────

def build_scene(output_path):
    print("\n=== Building sample-envelope grass ===")
    local_support_z = terrain_z.copy()

    print("Building gravel/stones...")
    gravel_rng = np.random.default_rng(SEED + 7919)
    parts = list(add_gravel(gravel_rng, local_support_z))
    print(f"  {N_GRAVEL} stones placed (support_z updated)")

    print("Building blade meshes...")
    built_blades   = 0
    skipped_blades = 0
    MAX_BOUNDARY_RETRIES = 32
    placed_blade_data = []   # (blade_idx, spine, hw, up_locs) — used by STRICT_MODE
    retry_rng = np.random.default_rng(SEED + 424242)

    for i, bl in enumerate(blades):
        accepted = None
        for attempt in range(MAX_BOUNDARY_RETRIES + 1):
            direction = bl['direction'] if attempt == 0 else retry_rng.uniform(0, 2 * np.pi)
            curl = bl['curl'] if attempt == 0 else retry_rng.uniform(-CURL_MAX, CURL_MAX)
            repair_floor = None
            for _repair_attempt in range(COLLISION_REPAIR_PASSES + 1):
                try:
                    mesh, sub_hull_mesh, spine, blade_widths = make_grass_blade(
                        support_z     = local_support_z,
                        base_pos      = (bl['base_x'], bl['base_y'], 0),
                        azimuth       = direction,
                        length        = bl['length'],
                        width         = bl['width'],
                        tip_length    = bl['tip_len'],
                        curl          = curl,
                        extra_floor_z = repair_floor,
                    )
                except RuntimeError:
                    break
                if not blade_footprint_inside_tile(spine, blade_widths):
                    break

                hw = blade_widths / 2.0
                up_locs_blade = _compute_up_locs(spine)
                strict_hits = (collect_strict_hits(spine, hw, up_locs_blade, placed_blade_data)
                               if STRICT_MODE else [])
                if not strict_hits:
                    accepted = (mesh, sub_hull_mesh, spine, blade_widths, up_locs_blade)
                    break

                if repair_floor is None:
                    repair_floor = np.full(len(spine), -np.inf, dtype=float)
                add_collision_repairs(repair_floor, spine, strict_hits)

            if accepted is not None:
                break
        if accepted is None:
            skipped_blades += 1
            continue
        mesh, sub_hull_mesh, spine, blade_widths, up_locs_blade = accepted
        parts.append(mesh)
        parts.append(sub_hull_mesh)
        built_blades += 1

        hw = blade_widths / 2.0

        if STRICT_MODE:
            strict_check(i, bl, spine, hw, up_locs_blade, placed_blade_data)
            placed_blade_data.append((i, spine, hw, up_locs_blade))

        rasterise_into_support(local_support_z, spine, hw)
        if (i + 1) % 20 == 0 or (i + 1) == len(blades):
            print(f"  {i+1}/{len(blades)} blades done")

    if skipped_blades:
        print(f"  skipped {skipped_blades} blade(s) that could not fit without intersections")
    print(f"  built {built_blades}/{len(blades)} blades")

    print("\nBlade height audit (spine z above local terrain):")
    rises = []
    for blade_idx, spine, hw, up_locs in placed_blade_data:
        bl = blades[blade_idx]
        base_tz = float(sample_grid(terrain_z, bl['base_x'], bl['base_y']))
        max_z   = float(np.max(spine[:, 2]))
        rises.append(max_z - base_tz)

    rises = np.array(rises)
    if len(rises):
        print(f"  n={len(rises)}  min={rises.min():.1f}mm  p25={np.percentile(rises,25):.1f}mm  "
              f"median={np.median(rises):.1f}mm  p75={np.percentile(rises,75):.1f}mm  "
              f"p90={np.percentile(rises,90):.1f}mm  p99={np.percentile(rises,99):.1f}mm  "
              f"max={rises.max():.1f}mm")
        over6 = int(np.sum(rises > MAX_STACK_HEIGHT + 1e-6))
        print(f"  blades rising > {MAX_STACK_HEIGHT:.0f}mm: {over6}")
    else:
        print("  no blades built")

    print("Building terrain solid...")
    terrain_mesh = make_heightmap_solid(terrain_z, TILE_W, TILE_H, BASE_H, subsample=4)
    parts.insert(0, terrain_mesh)

    print("Concatenating...")
    scene = trimesh.util.concatenate(parts)
    print(f"  vertices: {len(scene.vertices):,}   faces: {len(scene.faces):,}")
    print(f"  watertight: {scene.is_watertight}")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    scene.export(str(output_path))
    print(f"Saved {output_path}")


build_scene(OUTPUT)
