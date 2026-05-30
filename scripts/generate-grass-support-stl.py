#!/usr/bin/env python3
"""
Terrain-following grass STL via a 2.5D support field.

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

  Z — absolute support-clearing curve:
    The base is pinned to the terrain with an upward starting slope.  The tip
    is pinned above the current support field at its XY position.  Between
    those endpoints a three-part smooth curve is solved as low as possible
    while keeping the bottom of the top-face crease above support_z.
"""

import numpy as np
import trimesh
import pathlib
from scipy.optimize import minimize

# ── Config ────────────────────────────────────────────────────────────────────
TILE_W = TILE_H = 35.0          # mm
BASE_H          =  3.0          # mm — solid slab below terrain
GRID_RES        = 256           # support-field resolution (cells per side)

# Terrain
TERRAIN_AMP     = 1.0           # mm — sinusoidal bump amplitude
TERRAIN_FREQ    = 1.5           # cycles across tile

# Blade population
N_BLADES        = 400           # tall blades
N_FILL          = 0             # short filler blades
SEED            = 42
CURL_MAX        = 0.6           # max lateral curl magnitude (±)

# Blade geometry (mm)
TALL_W_MIN,  TALL_W_MAX  = 0.6, 1.5    # flat face width at base
TALL_L_MIN,  TALL_L_MAX  = 4.0, 14.4  # body arc length
TALL_TL_MIN, TALL_TL_MAX = 1.2, 4.8   # tip taper arc length
FILL_W_MIN,  FILL_W_MAX  = 0.3, 0.5
FILL_L_MIN,  FILL_L_MAX  = 4.0, 7.2
FILL_TL_MIN, FILL_TL_MAX = 1.2, 2.4

BASE_LEAN_ANGLE = np.radians(8)   # initial forward lean at base
LEAN_ANGLE      = np.radians(80)  # max lean at tip (nearly horizontal)
ARC_FRACTION    = 0.0             # extra interior bow above the lowest clearing curve
BLADE_CURL      = 1.0             # lateral curl (0=straight, ±1=±180 deg sweep)
N_PATH          = 50              # spine sample points (more = smoother curve)
CREASE_DEPTH    = 0.0             # mm — concave dip at centre of top face (0 = flat)
TIP_LIFT_FRAC   = 0.25            # tip raised by this fraction of blade width (0 = flush)
BASE_SLOPE_WIDTHS = 0.25          # normalized-t base dz/dt, in blade widths

# Terrain-following
CLEARANCE       = 0.04          # mm — gap above support surface
BASE_INSET      = 0.6           # mm — spine base sunk into terrain; also keel inset
BASE_SINK       = 0.25          # mm — keep first cap fully embedded below terrain

# Gravel / stones  (placed before grass; does NOT update support_z)
N_GRAVEL         = 6000         # number of stones
GRAVEL_R_MIN     = 0.048        # mm — minimum horizontal semi-axis
GRAVEL_R_MAX     = 0.42         # mm — maximum horizontal semi-axis
GRAVEL_FLAT_MIN  = 0.40         # stone height = this fraction × mean radius (flattest)
GRAVEL_FLAT_MAX  = 1.30         # stone height = this fraction × mean radius (roundest)
GRAVEL_AZ_SEGS   = 7            # azimuth facets per stone
GRAVEL_EL_SEGS   = 3            # elevation rings per stone (above base)
GRAVEL_SINK      = 0.01         # mm — base sunk below terrain so stones look embedded

OUTPUT = pathlib.Path("stl/grass-support-field.stl")

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
    """Paint the blade's spine z into support_z — vectorised per spine point."""
    path = np.asarray(path_xyz)        # (n_pts, 3)
    hws  = np.asarray(half_widths)     # (n_pts,)
    xs, ys, zs = path[:, 0], path[:, 1], path[:, 2]

    for idx in range(len(path)):
        x, y, z = float(xs[idx]), float(ys[idx]), float(zs[idx])
        hw = float(hws[idx])
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
        np.maximum(support_z[jj, ii], z, out=support_z[jj, ii])

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

def add_gravel(gravel_rng):
    """
    Place N_GRAVEL random stones across the whole tile surface.
    All geometry is built with numpy broadcasting; returns a single
    trimesh.Trimesh instead of 6 000 small ones.
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
        out.append(dict(
            base_x    = bx,
            base_y    = by,
            width     = rng.uniform(w_min,  w_max),
            length    = rng.uniform(l_min,  l_max),
            tip_len   = rng.uniform(tl_min, tl_max),
            direction = rng.uniform(0, 2 * np.pi),
            curl      = rng.uniform(-CURL_MAX, CURL_MAX),
        ))
    return out

tall  = place_blades(N_BLADES, TALL_W_MIN, TALL_W_MAX,
                     TALL_L_MIN, TALL_L_MAX, TALL_TL_MIN, TALL_TL_MAX)
fills = place_blades(N_FILL,   FILL_W_MIN, FILL_W_MAX,
                     FILL_L_MIN, FILL_L_MAX, FILL_TL_MIN, FILL_TL_MAX)
blades = tall + fills
blades.sort(key=lambda b: -b['base_y'])
print(f"Placed {len(blades)} blades")

# ── Low-level tube mesh ────────────────────────────────────────────────────────

def _build_tube_mesh(spine_3d, widths, keels, crease_val):
    """
    Watertight prism tube following spine_3d.

    When crease_val == 0 (flat top): triangular cross-section (3 verts/ring):
      V0 — keel,  V1 — top-right,  V2 — top-left
    When crease_val > 0: quad cross-section (4 verts/ring) with crease ridge V2.

    up_loc = normalize(cross(Ẑ, tang_xy)) — world-locked, zero twist.
    """
    path  = np.asarray(spine_3d, dtype=float)   # (n_pts, 3)
    W_arr = np.asarray(widths,   dtype=float)    # (n_pts,)
    K_arr = np.asarray(keels,    dtype=float)    # (n_pts, 3)
    n_pts = len(path)

    flat = (crease_val == 0.0)
    n    = 3 if flat else 4      # verts per ring

    # Pre-allocate
    nv = n * n_pts + 2           # rings + base_cap + tip_cap
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = 0;  fi = 0

    # ── Compute tangents and up_loc for all rings at once ────────────────────
    tangs = np.empty_like(path)
    tangs[:-1] = path[1:] - path[:-1]
    tangs[-1]  = path[-1] - path[-2]
    t_norms    = np.linalg.norm(tangs, axis=1, keepdims=True) + 1e-9
    tangs     /= t_norms

    txy_norm = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2)  # (n_pts,)
    has_xy   = txy_norm > 1e-6
    up_locs  = np.zeros((n_pts, 3), dtype=float)
    up_locs[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up_locs[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up_locs[~has_xy]   = [1.0, 0.0, 0.0]

    # ── Fill ring vertices ────────────────────────────────────────────────────
    half_W = (W_arr / 2.0)[:, None]          # (n_pts, 1)
    if flat:
        # V0: keel,  V1: spine + half_W*up,  V2: spine - half_W*up
        ring_v = np.stack([
            K_arr,
            path + half_W * up_locs,
            path - half_W * up_locs,
        ], axis=1)                             # (n_pts, 3, 3)
    else:
        keel_vec  = K_arr - path               # (n_pts, 3)
        keel_dist = np.linalg.norm(keel_vec, axis=1, keepdims=True) + 1e-9
        C_eff     = np.minimum(crease_val, keel_dist.squeeze() * 0.4)[:, None]
        crease_pt = path + C_eff * (keel_vec / keel_dist)
        ring_v = np.stack([
            K_arr,
            path + half_W * up_locs,
            crease_pt,
            path - half_W * up_locs,
        ], axis=1)                             # (n_pts, 4, 3)

    # Write rings into pre-allocated array
    ring_base_indices = np.arange(n_pts) * n  # start index of each ring in verts
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

def make_grass_blade(support_z, base_pos, azimuth, length, width, tip_length,
                     lean_angle=LEAN_ANGLE, arc_fraction=ARC_FRACTION,
                     curl=0.0, crease=CREASE_DEPTH,
                     tip_lift_frac=TIP_LIFT_FRAC, n_path=N_PATH):
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
    # Pass 1 — vectorised bilinear sampling for all path points at once
    xs_arr = bx + xrot       # (n_path,)
    ys_arr = by + yrot       # (n_path,)
    tz_arr = sample_grid(terrain_z, xs_arr, ys_arr)   # (n_path,)
    sz_arr = sample_grid(support_z,  xs_arr, ys_arr)  # (n_path,)

    # Pass 2 — minimum spine height constraints
    min_spine_z = sz_arr + crease + CLEARANCE
    T_CONSTRAINT_START = 0.25
    T_CONSTRAINT_END   = 0.95
    t_arr = np.linspace(0.0, 1.0, n_path)
    mask_off = (t_arr < T_CONSTRAINT_START) | (t_arr > T_CONSTRAINT_END)
    min_spine_z[mask_off] = -np.inf

    global_max_z = float(np.max(sz_arr)) + crease + CLEARANCE
    base_z = float(tz_arr[0] - BASE_SINK)
    tip_z  = max(float(sz_arr[-1] + crease + CLEARANCE),
                 float(tz_arr[-1] + width * tip_lift_frac))
    base_slope = width * BASE_SLOPE_WIDTHS

    # Pass 3 — three-part C1 cubic Hermite spline, vectorised eval
    h_span = 1.0 / 3.0
    t_pts  = np.linspace(0.0, 1.0, n_path)

    # Segment index and local u for each sample point
    seg    = np.minimum((t_pts / h_span).astype(int), 2)   # 0, 1, or 2
    u_pts  = t_pts / h_span - seg                           # local u in [0,1]

    # Hermite basis vectors (n_path,)
    u2 = u_pts ** 2;  u3 = u_pts ** 3
    H00 =  2*u3 - 3*u2 + 1
    H10 =    u3 - 2*u2 + u_pts
    H01 = -2*u3 + 3*u2
    H11 =    u3 -   u2

    def eval_spline_vec(z1, z2):
        """Evaluate the 3-segment Hermite spline at all n_path points."""
        z0, z3 = base_z, tip_z
        m0 = base_slope
        m1 = (z2 - z0) / (2.0 * h_span)
        m2 = (z3 - z1) / (2.0 * h_span)
        m3 = (z3 - z2) / h_span

        # Per-point (y0, m0_seg, y1, m1_seg) based on segment index
        Y0 = np.where(seg == 0, z0, np.where(seg == 1, z1, z2))
        M0 = np.where(seg == 0, m0, np.where(seg == 1, m1, m2))
        Y1 = np.where(seg == 0, z1, np.where(seg == 1, z2, z3))
        M1 = np.where(seg == 0, m1, np.where(seg == 1, m2, m3))

        return H00*Y0 + H10*h_span*M0 + H01*Y1 + H11*h_span*M1

    # Pre-compute basis vectors for the optimizer (linear in z1, z2)
    const_z  = eval_spline_vec(0.0, 0.0)
    z1_basis = eval_spline_vec(1.0, 0.0) - const_z
    z2_basis = eval_spline_vec(0.0, 1.0) - const_z

    terrain_arr   = tz_arr.astype(float)
    support_floor = min_spine_z.copy()
    support_floor[~np.isfinite(support_floor)] = -np.inf

    # Pre-compute second-difference matrices for curvature term
    d2_z1 = np.diff(z1_basis, n=2)
    d2_z2 = np.diff(z2_basis, n=2)
    d2_c  = np.diff(const_z,  n=2)

    def eval_from_x(x):
        return const_z + x[0] * z1_basis + x[1] * z2_basis

    _cache = [None, None, None]   # [x_key, f, jac]

    def _inner(x):
        """Returns (f, grad) together; memoised on x to avoid double eval."""
        key = x.tobytes()
        if _cache[0] == key:
            return _cache[1], _cache[2]
        z  = eval_from_x(x)
        h  = z - terrain_arr
        c  = d2_c + x[0] * d2_z1 + x[1] * d2_z2
        sv = np.maximum(support_floor - z, 0.0)
        sv[~np.isfinite(sv)] = 0.0
        cv = np.maximum(z - global_max_z, 0.0)   # ceiling — baked in as penalty
        f  = float(
            np.mean(h * h) +
            0.35 * np.mean(c * c) +
            250.0 * np.mean(sv * sv) +
            250.0 * np.mean(cv * cv)
        )
        jac = np.empty(2)
        for k, basis, d2b in ((0, z1_basis, d2_z1), (1, z2_basis, d2_z2)):
            dsv = np.where(sv > 0, -basis, 0.0)
            dcv = np.where(cv > 0,  basis, 0.0)
            jac[k] = (
                2.0       * np.mean(h  * basis) +
                0.35*2.0  * np.mean(c  * d2b  ) +
                250.0*2.0 * np.mean(sv * dsv  ) +
                250.0*2.0 * np.mean(cv * dcv  )
            )
        _cache[0] = key;  _cache[1] = f;  _cache[2] = jac
        return f, jac

    def curve_objective(x): return _inner(x)[0]
    def curve_jac(x):       return _inner(x)[1]

    # Only 2 cheap constraints — ceiling is now a penalty in the objective
    lower_knot  = min(base_z, tip_z)
    constraints = [
        {'type': 'ineq', 'fun': lambda x: x[0] - lower_knot,
                         'jac': lambda x: np.array([1.0, 0.0])},
        {'type': 'ineq', 'fun': lambda x: x[1] - lower_knot,
                         'jac': lambda x: np.array([0.0, 1.0])},
    ]

    x0 = np.array([max(base_z, tip_z), max(base_z, tip_z)], dtype=float)
    result = minimize(curve_objective, x0, jac=curve_jac, method='SLSQP',
                      constraints=constraints,
                      options={'ftol': 1e-9, 'maxiter': 200, 'disp': False})
    if not result.success:
        # Fallback: retry without analytical gradient (finite-diff is more robust
        # on degenerate problem instances)
        result = minimize(curve_objective, x0, method='SLSQP',
                          constraints=constraints,
                          options={'ftol': 1e-6, 'maxiter': 200, 'disp': False})
    if not result.success:
        raise RuntimeError(
            f"z-curve solve failed at base=({bx:.2f}, {by:.2f}): {result.message}"
        )

    spine_z = eval_from_x(result.x)
    spine_z[0]  = base_z
    spine_z[-1] = tip_z

    # Pass 4 — build spine + taper arrays (vectorised)
    k_arr   = np.arange(n_path)
    s_arr   = k_arr * dt * total_l
    t_tip   = np.clip((s_arr - length) / (tip_length + 1e-9), 0.0, 1.0)
    taper   = np.cos(t_tip * np.pi / 2.0)
    keel_z  = tz_arr - BASE_INSET
    keel_z[0] -= BASE_SINK

    path_xyz   = np.stack([xs_arr, ys_arr, spine_z], axis=1)   # (n_path, 3)
    widths_arr = width * taper                                   # (n_path,)
    keels_arr  = np.stack([xs_arr, ys_arr, keel_z], axis=1)    # (n_path, 3)

    mesh = _build_tube_mesh(path_xyz, widths_arr, keels_arr, crease)
    return mesh, path_xyz, widths_arr


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

# ── Main loop ─────────────────────────────────────────────────────────────────
print("Building gravel/stones...")
gravel_rng = np.random.default_rng(SEED + 7919)
parts = list(add_gravel(gravel_rng))
print(f"  {N_GRAVEL} stones placed (1 batch mesh)")

print("Building blade meshes...")
built_blades   = 0
skipped_blades = 0
MAX_BOUNDARY_RETRIES = 32

for i, bl in enumerate(blades):
    accepted = None
    for attempt in range(MAX_BOUNDARY_RETRIES + 1):
        direction = bl['direction'] if attempt == 0 else rng.uniform(0, 2 * np.pi)
        curl = bl['curl'] if attempt == 0 else rng.uniform(-CURL_MAX, CURL_MAX)
        try:
            mesh, spine, blade_widths = make_grass_blade(
                support_z  = support_z,
                base_pos   = (bl['base_x'], bl['base_y'], 0),
                azimuth    = direction,
                length     = bl['length'],
                width      = bl['width'],
                tip_length = bl['tip_len'],
                curl       = curl,
            )
        except RuntimeError:
            continue
        if blade_footprint_inside_tile(spine, blade_widths):
            accepted = (mesh, spine, blade_widths)
            break
    if accepted is None:
        skipped_blades += 1
        continue
    mesh, spine, blade_widths = accepted
    parts.append(mesh)
    built_blades += 1
    rasterise_into_support(support_z, spine, blade_widths / 2.0)
    if (i + 1) % 20 == 0 or (i + 1) == len(blades):
        print(f"  {i+1}/{len(blades)} blades done")

if skipped_blades:
    print(f"  skipped {skipped_blades} blade(s) that would cross tile bounds")
print(f"  built {built_blades}/{len(blades)} blades")

print("Building terrain solid...")
terrain_mesh = make_heightmap_solid(terrain_z, TILE_W, TILE_H, BASE_H, subsample=4)
parts.insert(0, terrain_mesh)

print("Concatenating...")
scene = trimesh.util.concatenate(parts)
print(f"  vertices: {len(scene.vertices):,}   faces: {len(scene.faces):,}")
print(f"  watertight: {scene.is_watertight}")

OUTPUT.parent.mkdir(parents=True, exist_ok=True)
scene.export(str(OUTPUT))
print(f"Saved {OUTPUT}")
