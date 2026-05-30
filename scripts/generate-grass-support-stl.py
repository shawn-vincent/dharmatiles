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
CREASE_DEPTH    = 0.0            # mm — concave dip at centre of top face (0 = flat)
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
    """Bilinear sample of a (GRID_RES × GRID_RES) array at (x_mm, y_mm)."""
    i = np.clip(x_mm / GX, 0, GRID_RES - 1)
    j = np.clip(y_mm / GY, 0, GRID_RES - 1)
    i0, j0 = int(i), int(j)
    i1, j1 = min(i0 + 1, GRID_RES - 1), min(j0 + 1, GRID_RES - 1)
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

# ── Gravel / stones ───────────────────────────────────────────────────────────

def make_stone(cx, cy, rx, ry, height, angle):
    """
    Watertight half-ellipsoid stone sitting on the terrain at (cx, cy).

    cx, cy  — centre XY (mm)
    rx, ry  — horizontal semi-axes before rotation (mm)
    height  — vertical semi-axis above terrain (mm)
    angle   — rotation around Z (radians)

    The base disk is sunk GRAVEL_SINK mm below terrain so the stone looks
    embedded rather than floating.  support_z is never touched.
    """
    tz   = sample_grid(terrain_z, cx, cy)
    base_z = tz - GRAVEL_SINK
    ca, sa = np.cos(angle), np.sin(angle)

    verts = []

    # Apex
    apex = 0
    verts.append([cx, cy, base_z + height])

    # Rings: ei=1 (just below apex) … ei=n_el (base ring at base_z)
    for ei in range(1, GRAVEL_EL_SEGS + 1):
        u      = ei / GRAVEL_EL_SEGS          # 0 → apex, 1 → base
        r_frac = np.sin(u * np.pi / 2)
        z_off  = height * np.cos(u * np.pi / 2)
        zv     = base_z + z_off
        for ai in range(GRAVEL_AZ_SEGS):
            theta = 2 * np.pi * ai / GRAVEL_AZ_SEGS
            lx = rx * r_frac * np.cos(theta)
            ly = ry * r_frac * np.sin(theta)
            wx = cx + ca * lx - sa * ly
            wy = cy + sa * lx + ca * ly
            verts.append([wx, wy, zv])

    # Bottom centre cap vertex
    bot = len(verts)
    verts.append([cx, cy, base_z])

    faces = []

    # Apex → first ring
    for ai in range(GRAVEL_AZ_SEGS):
        a = 1 + ai
        b = 1 + (ai + 1) % GRAVEL_AZ_SEGS
        faces.append([apex, a, b])

    # Side strips between rings
    for ei in range(1, GRAVEL_EL_SEGS):
        row_a = 1 + (ei - 1) * GRAVEL_AZ_SEGS
        row_b = 1 +  ei      * GRAVEL_AZ_SEGS
        for ai in range(GRAVEL_AZ_SEGS):
            a0 = row_a + ai;            a1 = row_a + (ai + 1) % GRAVEL_AZ_SEGS
            b0 = row_b + ai;            b1 = row_b + (ai + 1) % GRAVEL_AZ_SEGS
            faces.append([a0, b0, a1])
            faces.append([a1, b0, b1])

    # Base ring → bottom centre (cap faces outward downward)
    last_ring = 1 + (GRAVEL_EL_SEGS - 1) * GRAVEL_AZ_SEGS
    for ai in range(GRAVEL_AZ_SEGS):
        a = last_ring + ai
        b = last_ring + (ai + 1) % GRAVEL_AZ_SEGS
        faces.append([a, bot, b])

    mesh = trimesh.Trimesh(
        vertices = np.array(verts, dtype=float),
        faces    = np.array(faces,  dtype=int),
        process  = False,
    )
    mesh.fix_normals()
    return mesh


def add_gravel(gravel_rng):
    """
    Place N_GRAVEL random stones across the whole tile surface.
    Does NOT modify support_z — grass blades ignore stones entirely.
    Returns a list of trimesh.Trimesh objects to concatenate into the scene.
    """
    meshes = []
    for _ in range(N_GRAVEL):
        rx     = float(gravel_rng.uniform(GRAVEL_R_MIN, GRAVEL_R_MAX))
        ry     = float(gravel_rng.uniform(GRAVEL_R_MIN, GRAVEL_R_MAX))
        h_frac = float(gravel_rng.uniform(GRAVEL_FLAT_MIN, GRAVEL_FLAT_MAX))
        height = 0.5 * (rx + ry) * h_frac
        angle  = float(gravel_rng.uniform(0, np.pi))   # half-turn symmetry is enough
        # Keep stone footprint fully inside the tile
        margin = max(rx, ry)
        cx = float(gravel_rng.uniform(margin, TILE_W - margin))
        cy = float(gravel_rng.uniform(margin, TILE_H - margin))
        meshes.append(make_stone(cx, cy, rx, ry, height, angle))
    return meshes

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
# The envelope is zero at all four edges, so the terrain surface is constrained
# to meet the side-wall height there while varying smoothly in the interior.
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

def blade_footprint_inside_tile(spine_3d, widths):
    """Conservative XY footprint check: spine plus half width must stay inside."""
    for pt, W in zip(spine_3d, widths):
        margin = W / 2.0
        if pt[0] - margin < 0.0 or pt[0] + margin > TILE_W:
            return False
        if pt[1] - margin < 0.0 or pt[1] + margin > TILE_H:
            return False
    return True

def make_grass_blade(support_z, base_pos, azimuth, length, width, tip_length,
                     lean_angle=LEAN_ANGLE, arc_fraction=ARC_FRACTION,
                     curl=0.0, crease=CREASE_DEPTH,
                     tip_lift_frac=TIP_LIFT_FRAC, n_path=N_PATH):
    """
    Build a terrain-following floppy grass blade.
    Returns (mesh, spine_3d, half_widths).

    Two coupled curves
    ──────────────────
    XY (plan view):
      Variable lean — lean(t) = lean_angle * (1 - cos(t*pi/2)) — starts at
      zero so the blade's initial 3D tangent is vertical (base disk flat on
      ground).  `curl` tilts the lean direction smoothly with t; a
      chord-preserving 2D rotation then snaps the XY chord back onto the
      `azimuth` direction, so curl never shifts the fall direction.

    Z (height):
      The base spine height is pinned to terrain and starts with a fixed
      upward slope.  The tip height is pinned above support_z at the final
      XY coordinate.  The interior is a three-part C1 curve solved to be the
      lowest profile whose crease clears support_z along the blade.

    Parameters
    ----------
    support_z    : (GRID_RES x GRID_RES) height grid — current top surface
    base_pos     : (bx, by, ...)  world XY of blade base (Z is recomputed)
    azimuth      : fall direction, radians from +Y  (0=north, pi/2=east)
    length       : body arc length (mm)
    width        : base flat-face width (mm)
    tip_length   : cosine-taper tip arc length (mm)
    lean_angle   : max lean at tip in radians (default 80 deg)
    arc_fraction : optional extra interior bow above the lowest clearing curve
    curl         : -1..+1, lateral sweep of XY path; chord-preserving
                   (base->tip azimuth is preserved regardless of curl)
    """
    bx, by  = float(base_pos[0]), float(base_pos[1])
    total_l = length + tip_length
    dt      = 1.0 / (n_path - 1)
    CURL_MAX = np.pi                  # |curl|=1 gives ±180 deg lateral sweep

    # ── XY path: variable lean + chord-preserving curl ───────────────────────
    # lean(t) = base_lean + (lean_angle - base_lean) * (1 - cos(t*pi/2))
    #   t=0 -> lean=base_lean => initial tangent leans toward blade direction
    #   t=1 -> lean=lean_angle (~80 deg) => mostly horizontal
    #
    # curl rotates the lean direction by curl*CURL_MAX*t as t increases,
    # causing the XY path to arc left or right.  The chord-preserving
    # rotation at the end keeps the overall chord aligned with `azimuth`.

    xr, yr = [0.0], [0.0]
    for k in range(1, n_path):
        t_mid    = (k - 0.5) * dt
        lean_now = BASE_LEAN_ANGLE + (
            lean_angle - BASE_LEAN_ANGLE
        ) * (1.0 - np.cos(t_mid * np.pi / 2.0))
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

    # ── Z profile: lowest smooth curve that clears current support ────────────
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

    # Pass 2 — minimum absolute spine height at each sample.  support_z stores
    # the previous blade's top edges.  This blade's crease is below its spine,
    # so the spine itself must be higher by the crease depth plus clearance.
    #
    # The base and tip are excluded from interior constraints:
    #   • first T_CONSTRAINT_START: blade is still emerging from the terrain
    #   • last  T_CONSTRAINT_END:   tip is tapered to near-zero — tiny overlaps
    #                               are invisible and let the tip land naturally.
    min_spine_z = np.array(sz_path, dtype=float) + crease + CLEARANCE
    T_CONSTRAINT_START = 0.25
    T_CONSTRAINT_END   = 0.95
    for k in range(n_path):
        t_k = k / (n_path - 1)
        if t_k < T_CONSTRAINT_START or t_k > T_CONSTRAINT_END:
            min_spine_z[k] = -np.inf

    # Global ceiling: the curve must never exceed the highest support it actually
    # needs to clear.  No blade should arch higher than max(support) + crease room.
    global_max_z = float(np.max(sz_path)) + crease + CLEARANCE

    base_z = float(tz_path[0] - BASE_SINK)
    tip_z  = max(float(sz_path[-1] + crease + CLEARANCE),
                 float(tz_path[-1] + width * tip_lift_frac))

    # Normalized-t derivative.  The base tangent direction is upward because
    # the XY path has nearly zero horizontal speed at the root; the magnitude
    # should be local to the blade thickness, not proportional to blade length.
    base_slope = width * BASE_SLOPE_WIDTHS
    # Pass 3 — three-part smooth support-clearing height curve.
    #
    # This is a single C1 cubic Hermite spline with three equal spans.  The base
    # height and slope are fixed constraints, the tip height is fixed, and the
    # two interior knot heights are solved as a tiny linear envelope problem.
    # That gives the curve local freedom to clear obstacles without one global
    # Bezier handle pulling the whole blade high.
    h_span = 1.0 / 3.0

    def hermite(y0, m0, y1, m1, u):
        h00 = 2*u**3 - 3*u**2 + 1
        h10 = u**3 - 2*u**2 + u
        h01 = -2*u**3 + 3*u**2
        h11 = u**3 - u**2
        return h00*y0 + h10*h_span*m0 + h01*y1 + h11*h_span*m1

    def eval_spline(z1, z2):
        z0, z3 = base_z, tip_z
        m0 = base_slope
        m1 = (z2 - z0) / (2.0 * h_span)
        m2 = (z3 - z1) / (2.0 * h_span)
        m3 = (z3 - z2) / h_span

        out = np.zeros(n_path)
        for kk in range(n_path):
            t = kk / (n_path - 1)
            if t <= h_span:
                u = t / h_span
                out[kk] = hermite(z0, m0, z1, m1, u)
            elif t <= 2.0 * h_span:
                u = (t - h_span) / h_span
                out[kk] = hermite(z1, m1, z2, m2, u)
            else:
                u = (t - 2.0 * h_span) / h_span
                out[kk] = hermite(z2, m2, z3, m3, u)
        return out

    const_z = eval_spline(0.0, 0.0)
    z1_basis = eval_spline(1.0, 0.0) - const_z
    z2_basis = eval_spline(0.0, 1.0) - const_z

    terrain_arr = np.array(tz_path, dtype=float)
    support_floor = np.array(min_spine_z, dtype=float)
    support_floor[~np.isfinite(support_floor)] = -np.inf

    def eval_from_x(x):
        return const_z + x[0] * z1_basis + x[1] * z2_basis

    def curve_objective(x):
        z = eval_from_x(x)
        height = z - terrain_arr
        curvature = np.diff(z, n=2)
        support_violation = np.maximum(support_floor - z, 0.0)
        support_violation[~np.isfinite(support_violation)] = 0.0
        return float(
            np.mean(height * height) +
            0.35 * np.mean(curvature * curvature) +
            250.0 * np.mean(support_violation * support_violation)
        )

    constraints = []
    lower_knot = min(base_z, tip_z)
    constraints.append({'type': 'ineq', 'fun': lambda x: x[0] - lower_knot})
    constraints.append({'type': 'ineq', 'fun': lambda x: x[1] - lower_knot})

    # Upper bound: no point on the curve may exceed the highest support it needs
    # to clear.  This replaces the old terrain-relative MAX_HEIGHT_ABOVE_TERRAIN cap.
    for k in range(n_path):
        max_c = global_max_z - const_z[k]
        constraints.append({
            'type': 'ineq',
            'fun': (
                lambda x, a=z1_basis[k], b=z2_basis[k], c=max_c:
                c - a * x[0] - b * x[1]
            ),
        })

    result = minimize(
        curve_objective,
        np.array([max(base_z, tip_z), max(base_z, tip_z)], dtype=float),
        method='SLSQP',
        constraints=constraints,
        options={'ftol': 1e-9, 'maxiter': 200, 'disp': False},
    )
    if not result.success:
        raise RuntimeError(
            f"z-curve solve failed at base=({bx:.2f}, {by:.2f}): {result.message}"
        )

    spine_z = eval_from_x(result.x)
    spine_z[0] = base_z
    spine_z[-1] = tip_z

    # Pass 4 — build spine + taper.
    # Keel (V0) is pinned to the BASE terrain at each XY point, sunk in by
    # BASE_INSET.  Depth is implicit: spine_z − keel_z.
    path_xyz    = []
    widths_arr  = []
    keels_arr   = []
    creases_arr = []

    for k in range(n_path):
        x = xs_path[k]
        y = ys_path[k]
        z = float(spine_z[k])

        s     = k * dt * total_l
        t_tip = float(np.clip((s - length) / (tip_length + 1e-9), 0.0, 1.0))
        taper = np.cos(t_tip * np.pi / 2.0)

        keel_z = tz_path[k] - BASE_INSET
        if k == 0:
            keel_z -= BASE_SINK

        path_xyz.append(np.array([x, y, z]))
        widths_arr.append(width * taper)
        keels_arr.append(np.array([x, y, keel_z]))
        creases_arr.append(crease)          # absolute mm; clamped in add_ring at shallow ends

    # Ring 0 is fully below terrain; support constraints are ignored near the
    # root, so the blade can emerge smoothly without the first cap peeking out.

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
print("Building gravel/stones...")
gravel_rng = np.random.default_rng(SEED + 7919)   # independent stream; prime offset
parts = list(add_gravel(gravel_rng))
print(f"  {len(parts)} stones placed")

print("Building blade meshes...")
built_blades = 0
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
    # footprint radius = half the flat-face width
    rasterise_into_support(support_z, spine, [W / 2 for W in blade_widths])
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

# open manually when ready
