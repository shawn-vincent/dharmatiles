#!/usr/bin/env python3
"""
Terrain-following grass STL via a 2.5D support field.

Algorithm
─────────
  support_z starts as the terrain surface.  Blades are painted onto it in
  order (back→front).  Each blade's footprint is rasterised back into
  support_z after it is built, so the next blade rests on top.

Blade geometry  (two independent curves)
────────────────────────────────────────
  XY plane — chord-preserving 2D arc, exactly like blade.py:
    `azimuth` sets the fall direction (base→tip chord in XY).
    `curl` (±1) bends the XY path left/right while keeping that chord fixed.
    Variable lean — lean(t) = LEAN_ANGLE × (1 − cos(t·π/2)) — gives the
    horizontal speed a zero start, so the blade grows straight up at the
    base before spreading out.

  Z — completely independent sin bow on top of terrain:
    z(t) = terrain_at_xy(t)  +  arc_h × sin(t·π)  +  CLEARANCE
    t=0 → base flush on ground, t=0.5 → peak, t=1 → tip back on ground.
    Terrain is sampled at every spine XY point, so the tip always lands
    at the correct ground height.
"""

import numpy as np
import trimesh
import pathlib

# ── Config ────────────────────────────────────────────────────────────────────
TILE_W = TILE_H = 35.0          # mm
BASE_H          =  3.0          # mm — solid slab below terrain
GRID_RES        = 256           # support-field resolution (cells per side)

# Terrain
TERRAIN_AMP     = 0.8           # mm — sinusoidal bump amplitude
TERRAIN_FREQ    = 1.5           # cycles across tile

# Blade population
N_BLADES        = 50            # tall blades
N_FILL          = 0             # short filler blades
SEED            = 42
CURL_MAX        = 0.6           # max lateral curl magnitude (±)

# Blade geometry (mm)
TALL_W_MIN,  TALL_W_MAX  = 1.5, 2.5    # flat face width at base
TALL_L_MIN,  TALL_L_MAX  = 10.0, 18.0  # body arc length
TALL_TL_MIN, TALL_TL_MAX = 3.0, 6.0   # tip taper arc length
FILL_W_MIN,  FILL_W_MAX  = 0.7, 1.4
FILL_L_MIN,  FILL_L_MAX  = 2.0, 4.5
FILL_TL_MIN, FILL_TL_MAX = 0.8, 1.8

LEAN_ANGLE      = np.radians(80)  # max lean at tip (nearly horizontal)
ARC_FRACTION    = 0.5             # bow height = ARC_FRACTION × blade width
BLADE_CURL      = 1.0             # lateral curl (0=straight, ±1=±180 deg sweep)
N_PATH          = 50              # spine sample points (more = smoother curve)
CREASE_DEPTH    = 0.5             # mm — concave dip at centre of top face (0 = flat)
TIP_LIFT_FRAC   = 0.25            # tip raised by this fraction of blade width (0 = flush)

# Terrain-following
CLEARANCE       = 0.04          # mm — gap above support surface
BASE_INSET      = 0.6           # mm — spine base sunk into terrain; also keel inset

OUTPUT = pathlib.Path("stl/grass-support-field.stl")

# ── Grid helpers ──────────────────────────────────────────────────────────────
GX = TILE_W / GRID_RES          # mm per cell
GY = TILE_H / GRID_RES

def sample_grid(grid, x_mm, y_mm):
    """Bilinear sample of a (GRID_RES × GRID_RES) array at (x_mm, y_mm)."""
    i = np.clip(x_mm / GX, 0, GRID_RES - 1.001)
    j = np.clip(y_mm / GY, 0, GRID_RES - 1.001)
    i0, j0 = int(i), int(j)
    i1, j1 = i0 + 1, j0 + 1
    fi, fj  = i - i0, j - j0
    return (grid[j0, i0] * (1-fi) * (1-fj) +
            grid[j0, i1] *    fi  * (1-fj) +
            grid[j1, i0] * (1-fi) *    fj  +
            grid[j1, i1] *    fi  *    fj)

def rasterise_into_support(support_z, path_xyz, half_widths):
    """Paint the blade's V1/V3 height (spine z) into support_z.
    V1 and V3 are the top-face edges, both at the spine centre height.
    The crease (V2) is below them, so the next blade only needs to clear
    this height by its own crease depth — handled in make_grass_blade Pass 2.
    """
    for pt, hw in zip(path_xyz, half_widths):
        x, y, z = float(pt[0]), float(pt[1]), float(pt[2])
        r_cells = max(1, int(hw / GX) + 2)
        ic = int(x / GX)
        jc = int(y / GY)
        for di in range(-r_cells, r_cells + 1):
            for dj in range(-r_cells, r_cells + 1):
                ii = int(np.clip(ic + di, 0, GRID_RES - 1))
                jj = int(np.clip(jc + dj, 0, GRID_RES - 1))
                if (di * GX) ** 2 + (dj * GY) ** 2 <= hw ** 2:
                    support_z[jj, ii] = max(support_z[jj, ii], z)  # V1/V3 are at spine height

def terrain_normal_at(x_mm, y_mm):
    """Outward (upward) unit normal of terrain_z at (x_mm, y_mm) via central differences."""
    eps  = GX * 2
    dzdx = (sample_grid(terrain_z, x_mm + eps, y_mm) -
            sample_grid(terrain_z, x_mm - eps, y_mm)) / (2 * eps)
    dzdy = (sample_grid(terrain_z, x_mm, y_mm + eps) -
            sample_grid(terrain_z, x_mm, y_mm - eps)) / (2 * eps)
    n = np.array([-dzdx, -dzdy, 1.0])
    return n / np.linalg.norm(n)

# ── Terrain ───────────────────────────────────────────────────────────────────
print("Building terrain...")
iy, ix = np.mgrid[0:GRID_RES, 0:GRID_RES]
x_grid = ix * GX
y_grid = iy * GY
terrain_z = (TERRAIN_AMP *
             np.sin(2 * np.pi * TERRAIN_FREQ * x_grid / TILE_W) *
             np.cos(2 * np.pi * TERRAIN_FREQ * y_grid / TILE_H)).astype(float)
terrain_z -= terrain_z.min()   # shift so lowest point = 0

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
    edge = w_max / 2 + 0.2          # keep bases at least one half-width from tile edge
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

def _build_tube_mesh(spine_3d, widths, keels, creases):
    """
    Watertight 4-vertex prism tube following spine_3d.

    Cross-section at each ring — world-locked, zero twist:
      • V0 — keel:         keels[i]  — world position on/below terrain surface
      • V1 — top-right:    spine[i] + (W/2) * up_loc
      • V2 — crease ridge: spine[i] + C_eff * normalize(keel − spine)
                           C_eff = min(creases[i], dist(keel,spine) * 0.4)
      • V3 — top-left:     spine[i] − (W/2) * up_loc

    up_loc = normalize(cross(Ẑ, tang_xy)) — always horizontal, always
    perpendicular to the blade's XY travel direction.  No parallel
    transport; no accumulated twist from curl or lean.  The flat top face
    (V1-V2-V3) stays aligned with the XY plane the whole length of the blade.
    """
    n     = 4
    path  = np.array(spine_3d, dtype=float)
    W_arr = np.array(widths,   dtype=float)
    K_arr = np.array(keels,    dtype=float)
    C_arr = np.array(creases,  dtype=float)
    n_pts = len(path)

    verts, faces = [], []

    def add_pt(p):
        idx = len(verts); verts.append(p.tolist()); return idx

    def add_ring(spine_pt, W, keel_pt, C, up_loc):
        idx       = len(verts)
        keel_vec  = keel_pt - spine_pt
        keel_dist = np.linalg.norm(keel_vec) + 1e-9
        C_eff     = min(C, keel_dist * 0.4)
        crease_pt = spine_pt + C_eff * (keel_vec / keel_dist)
        verts.append(keel_pt.tolist())                           # V0: keel (terrain)
        verts.append((spine_pt + (W/2) * up_loc).tolist())      # V1: top-right
        verts.append(crease_pt.tolist())                         # V2: crease ridge
        verts.append((spine_pt - (W/2) * up_loc).tolist())      # V3: top-left
        return idx

    rings = []
    for i in range(n_pts):
        tang = path[i+1] - path[i] if i < n_pts-1 else path[i] - path[i-1]
        tang = tang / (np.linalg.norm(tang) + 1e-9)

        # World-locked up_loc: cross(Ẑ, tang_xy) = (-tang_y, tang_x, 0).
        # Always horizontal, always ⊥ to the blade's XY travel direction.
        # Never accumulates twist regardless of curl or lean.
        tang_xy_norm = np.sqrt(tang[0]**2 + tang[1]**2)
        if tang_xy_norm > 1e-6:
            up_loc = np.array([-tang[1], tang[0], 0.0]) / tang_xy_norm
        else:
            up_loc = np.array([1.0, 0.0, 0.0])   # vertical spine: any horizontal works

        rings.append(add_ring(path[i], W_arr[i], K_arr[i], C_arr[i], up_loc))

    v_base = add_pt(path[0])
    v_tip  = add_pt(path[-1])

    r0 = rings[0]
    for i in range(n):
        faces.append([v_base, r0 + (i+1) % n, r0 + i])

    for k in range(len(rings) - 1):
        ra, rb = rings[k], rings[k+1]
        for i in range(n):
            i1 = (i+1) % n
            faces.append([ra+i,  rb+i,  ra+i1])
            faces.append([ra+i1, rb+i,  rb+i1])

    rl = rings[-1]
    for i in range(n):
        faces.append([rl+i, rl+(i+1) % n, v_tip])

    mesh = trimesh.Trimesh(
        vertices = np.array(verts, dtype=float),
        faces    = np.array(faces,  dtype=int),
        process  = False,
    )
    mesh.fix_normals()
    return mesh


# ── Parameterised grass blade ─────────────────────────────────────────────────

def make_grass_blade(support_z, base_pos, azimuth, length, width, tip_length,
                     lean_angle=LEAN_ANGLE, arc_fraction=ARC_FRACTION,
                     curl=0.0, crease=CREASE_DEPTH,
                     tip_lift_frac=TIP_LIFT_FRAC, n_path=N_PATH):
    """
    Build a terrain-following floppy grass blade.
    Returns (mesh, spine_3d, half_widths).

    Two independent curves
    ──────────────────────
    XY (plan view):
      Variable lean — lean(t) = lean_angle * (1 - cos(t*pi/2)) — starts at
      zero so the blade's initial 3D tangent is vertical (base disk flat on
      ground).  `curl` tilts the lean direction smoothly with t; a
      chord-preserving 2D rotation then snaps the XY chord back onto the
      `azimuth` direction, so curl never shifts the fall direction.

    Z (height):
      z(t) = terrain_at_xy(t) + arc_h * sin(t*pi) + CLEARANCE
      Completely independent of XY.  Terrain is sampled at each spine XY
      point, so the tip always lands at the correct ground height.

    Parameters
    ----------
    support_z    : (GRID_RES x GRID_RES) height grid — current top surface
    base_pos     : (bx, by, ...)  world XY of blade base (Z is recomputed)
    azimuth      : fall direction, radians from +Y  (0=north, pi/2=east)
    length       : body arc length (mm)
    radius       : base cross-section radius (mm)
    tip_length   : cosine-taper tip arc length (mm)
    lean_angle   : max lean at tip in radians (default 80 deg)
    arc_fraction : bow height = arc_fraction * diameter (default 0.5)
    curl         : -1..+1, lateral sweep of XY path; chord-preserving
                   (base->tip azimuth is preserved regardless of curl)
    """
    bx, by  = float(base_pos[0]), float(base_pos[1])
    total_l = length + tip_length
    arc_h   = width * arc_fraction          # minimum bow height above obstacles
    dt      = 1.0 / (n_path - 1)
    CURL_MAX = np.pi                  # |curl|=1 gives ±180 deg lateral sweep

    # ── XY path: variable lean + chord-preserving curl ───────────────────────
    # lean(t) = lean_angle * (1 - cos(t*pi/2))
    #   t=0 -> lean=0  => zero horizontal speed => initial tangent is +Z
    #   t=1 -> lean=lean_angle (~80 deg) => mostly horizontal
    #
    # curl rotates the lean direction by curl*CURL_MAX*t as t increases,
    # causing the XY path to arc left or right.  The chord-preserving
    # rotation at the end keeps the overall chord aligned with `azimuth`.

    xr, yr = [0.0], [0.0]
    for k in range(1, n_path):
        t_mid    = (k - 0.5) * dt
        lean_now = lean_angle * (1.0 - np.cos(t_mid * np.pi / 2.0))
        ds       = total_l * dt
        az_now   = azimuth + curl * CURL_MAX * t_mid
        xr.append(xr[-1] + np.sin(az_now) * np.sin(lean_now) * ds)
        yr.append(yr[-1] + np.cos(az_now) * np.sin(lean_now) * ds)

    # Chord-preserving 2D rotation — realign XY tip with azimuth.
    # CCW rotation by θ maps bearing β → β − θ, so to send
    # tip_angle → azimuth we need θ = tip_angle − azimuth.
    tip_dist = np.sqrt(xr[-1]**2 + yr[-1]**2)
    if tip_dist > 1e-6:
        tip_angle    = np.arctan2(xr[-1], yr[-1])
        rot          = tip_angle - azimuth          # was azimuth - tip_angle (wrong sign)
        cos_r, sin_r = np.cos(rot), np.sin(rot)
        xrot = [x * cos_r - y * sin_r for x, y in zip(xr, yr)]
        yrot = [x * sin_r + y * cos_r for x, y in zip(xr, yr)]
    else:
        xrot, yrot = xr, yr

    # ── Z profile: one smooth arch from base → peak obstacle → tip ───────────────
    # Pass 1 — XY positions, terrain height, and support height at every point.
    xs_path, ys_path, tz_path, sz_path = [], [], [], []
    for k in range(n_path):
        x = float(bx + xrot[k])
        y = float(by + yrot[k])
        xs_path.append(x)
        ys_path.append(y)
        # sample_grid clips internally, so out-of-tile positions return edge values
        tz_path.append(sample_grid(terrain_z, x, y))
        sz_path.append(sample_grid(support_z,  x, y))

    # Pass 2 — minimum obstacle height (above terrain) at each spine point.
    #   The arch must exceed obstacle[k] at every interior k.
    #   Endpoints are pinned (base flush, tip at tip_lift) and not constrained here.
    #   The first T_BASE fraction of the path is ignored: the blade is still
    #   emerging from the terrain there and minor base-overlap is acceptable
    #   (and invisible below the surface).
    T_BASE = 0.20    # ignore obstacles in first 20% of blade path
    obstacle = np.zeros(n_path)
    for k in range(1, n_path - 1):
        t     = k * dt
        if t < T_BASE:
            continue   # blade still exiting terrain — don't force a leap here
        sin_t = np.sin(t * np.pi)
        # Raise spine so this blade's crease (V2) clears the previous blade's
        # top edges (V1/V3 recorded in support_z at spine height).
        support_extra = max(0.0, sz_path[k] - tz_path[k] + crease) + CLEARANCE * sin_t
        obstacle[k] = support_extra

    # Pass 3 — two-segment cubic Hermite arch, minimising peak height H.
    #
    # The arch is parameterised by t ∈ [0, 1] (fraction of n_path − 1).
    # A join point splits it into two Hermite segments:
    #
    #   Seg 1 (0 ≤ t ≤ t_join):
    #     y0 = 0, y1 = H, tangent at 0 = m0, tangent at 1 = 0.
    #     arch(u) = H·h01(u) + m0·h10(u)     u = t / t_join
    #
    #   Seg 2 (t_join ≤ t ≤ 1):
    #     y0 = H, y1 = tip_lift, tangent at 0 = 0, tangent at 1 = 0.
    #     arch(v) = H·h00(v) + tip_lift·h01(v)   v = (t − t_join) / (1 − t_join)
    #
    # C1 at the join: both segments have zero slope there (m1 of seg1 = m0 of seg2 = 0).
    #
    # m0 = t_join * total_l  sets the initial tangent so the blade shoots orthogonal
    # to the terrain at the base (darch/ds ≈ 1 mm/mm at s = 0).
    #
    # Hermite basis functions on [0, 1]:
    #   h00(u) =  2u³ − 3u² + 1    (value at u=0)
    #   h01(u) = −2u³ + 3u²        (value at u=1)
    #   h10(u) =   u³ − 2u² + u    (slope at u=0)
    #
    # For each candidate join point we analytically solve for the minimum H
    # that clears every obstacle, then keep the join with the smallest H.
    tip_lift = width * tip_lift_frac

    # Minimum Hermite basis amplitude before we enforce a clearance constraint.
    # h01(u) → 0 near the blade base (u→0) and h00(v) → 0 near the tip (v→1);
    # dividing by a near-zero basis gives astronomically large H requirements.
    # A threshold of 0.15 limits amplification to ≤ 6.7×, keeping H within
    # reason.  The skipped near-base/near-tip regions are at or below the
    # terrain surface anyway, so minor overlap there is invisible.
    BASIS_MIN = 0.15

    best_H      = np.inf
    best_t_join = 0.5
    for k_join in range(1, n_path - 1):
        t_j = k_join / (n_path - 1)
        m0  = t_j * total_l      # initial slope: 1 mm arch / 1 mm arc at base
        H_j = arc_h              # minimum aesthetic arc (arc_fraction × width)
        for k in range(1, n_path - 1):
            obs = obstacle[k]
            if obs <= 0.0:
                continue
            t_k = k / (n_path - 1)
            if t_k <= t_j:
                u     = t_k / t_j
                h01_u = -2*u**3 + 3*u**2
                h10_u =    u**3 - 2*u**2 + u
                if h01_u >= BASIS_MIN:   # skip near-base where curve barely lifts
                    H_j = max(H_j, (obs - m0 * h10_u) / h01_u)
            else:
                v     = (t_k - t_j) / (1.0 - t_j)
                h00_v =  2*v**3 - 3*v**2 + 1
                h01_v = -2*v**3 + 3*v**2
                if h00_v >= BASIS_MIN:   # skip near-tip where curve barely lifts
                    H_j = max(H_j, (obs - tip_lift * h01_v) / h00_v)
        if H_j < best_H:
            best_H      = H_j
            best_t_join = t_j

    H_join = best_H
    t_join = best_t_join
    m0     = t_join * total_l

    arch = np.zeros(n_path)
    for k in range(n_path):
        t_k = k / (n_path - 1)
        if t_k <= t_join:
            u       = t_k / t_join if t_join > 1e-9 else 0.0
            arch[k] = H_join * (-2*u**3 + 3*u**2) + m0 * (u**3 - 2*u**2 + u)
        else:
            v       = (t_k - t_join) / (1.0 - t_join) if t_join < 1.0 - 1e-9 else 1.0
            arch[k] = H_join * (2*v**3 - 3*v**2 + 1) + tip_lift * (-2*v**3 + 3*v**2)
    arch[0]  = 0.0       # base stays flush with terrain
    arch[-1] = tip_lift  # tip hovers tip_lift_frac × width above terrain

    # Pass 4 — build spine + taper.
    # Keel (V0) is pinned to the BASE terrain at each XY point, sunk in by
    # BASE_INSET.  Depth is implicit: spine_z − keel_z = arch[k] + BASE_INSET,
    # which is zero at base/tip and maximum at the arch peak.
    path_xyz    = []
    widths_arr  = []
    keels_arr   = []
    creases_arr = []

    for k in range(n_path):
        x = xs_path[k]
        y = ys_path[k]
        z = tz_path[k] + float(arch[k])

        s     = k * dt * total_l
        t_tip = float(np.clip((s - length) / (tip_length + 1e-9), 0.0, 1.0))
        taper = np.cos(t_tip * np.pi / 2.0)

        keel_z = tz_path[k] - BASE_INSET   # base terrain − inset (independent of arch)

        path_xyz.append(np.array([x, y, z]))
        widths_arr.append(width * taper)
        keels_arr.append(np.array([x, y, keel_z]))
        creases_arr.append(crease)          # absolute mm; clamped in add_ring at shallow ends

    # ── Base inset: sink ring-0 into the terrain along the terrain normal ────────
    # This makes the base disk lie in (approximately) the terrain tangent plane
    # and eliminates edge gaps caused by terrain curvature.
    tn = terrain_normal_at(bx, by)
    path_xyz[0] = path_xyz[0] - BASE_INSET * tn

    mesh = _build_tube_mesh(path_xyz, widths_arr, keels_arr, creases_arr)
    return mesh, path_xyz, widths_arr


# ── Terrain mesh ──────────────────────────────────────────────────────────────

def make_heightmap_solid(z_grid, tile_w, tile_h, base_h, subsample=4):
    """Watertight solid: top = sinusoidal surface, bottom = flat at -base_h."""
    res     = z_grid.shape[0]
    sr_list = list(range(0, res, subsample))
    if sr_list[-1] != res - 1:
        sr_list.append(res - 1)   # always include far edge -> exact 35x35 mm
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
print("Building blade meshes...")
parts = []

for i, bl in enumerate(blades):
    mesh, spine, blade_widths = make_grass_blade(
        support_z  = support_z,
        base_pos   = (bl['base_x'], bl['base_y'], 0),
        azimuth    = bl['direction'],
        length     = bl['length'],
        width      = bl['width'],
        tip_length = bl['tip_len'],
        curl       = bl['curl'],
    )
    parts.append(mesh)
    # footprint radius = half the flat-face width
    rasterise_into_support(support_z, spine, [W / 2 for W in blade_widths])
    if (i + 1) % 20 == 0 or (i + 1) == len(blades):
        print(f"  {i+1}/{len(blades)} blades done")

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

# open manually when ready
