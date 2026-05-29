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
N_BLADES        = 5             # tall blades
N_FILL          = 0             # short filler blades
SEED            = 42
CURL_MAX        = 0.6           # max lateral curl magnitude (±)

# Blade geometry (mm)
TALL_W_MIN,  TALL_W_MAX  = 1.5, 2.5    # flat face width at base
TALL_T_MIN,  TALL_T_MAX  = 0.4, 0.65   # thickness (keel depth) at base
TALL_L_MIN,  TALL_L_MAX  = 10.0, 18.0  # body arc length
TALL_TL_MIN, TALL_TL_MAX = 3.0, 6.0   # tip taper arc length
FILL_W_MIN,  FILL_W_MAX  = 0.7, 1.4
FILL_T_MIN,  FILL_T_MAX  = 0.2, 0.4
FILL_L_MIN,  FILL_L_MAX  = 2.0, 4.5
FILL_TL_MIN, FILL_TL_MAX = 0.8, 1.8

LEAN_ANGLE      = np.radians(80)  # max lean at tip (nearly horizontal)
ARC_FRACTION    = 0.5             # bow height = ARC_FRACTION × blade diameter
BLADE_CURL      = 1.0             # lateral curl (0=straight, ±1=±180 deg sweep)
N_PATH          = 50              # spine sample points (more = smoother curve)
N_SIDES         = 3               # polygon sides per cross-section ring (3 = triangular prism)

# Terrain-following
CLEARANCE       = 0.04          # mm — gap above support surface
BASE_INSET      = 0.6           # mm — how far to sink the base ring into the terrain

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
    """Paint the blade's top surface into support_z."""
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
                    support_z[jj, ii] = max(support_z[jj, ii], z + hw)  # store top surface, not spine centre

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

def place_blades(n, w_min, w_max, t_min, t_max, l_min, l_max, tl_min, tl_max):
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
            thickness = rng.uniform(t_min,  t_max),
            length    = rng.uniform(l_min,  l_max),
            tip_len   = rng.uniform(tl_min, tl_max),
            direction = rng.uniform(0, 2 * np.pi),
            curl      = rng.uniform(-CURL_MAX, CURL_MAX),
        ))
    return out

tall  = place_blades(N_BLADES, TALL_W_MIN, TALL_W_MAX, TALL_T_MIN, TALL_T_MAX,
                     TALL_L_MIN, TALL_L_MAX, TALL_TL_MIN, TALL_TL_MAX)
fills = place_blades(N_FILL,   FILL_W_MIN, FILL_W_MAX, FILL_T_MIN, FILL_T_MAX,
                     FILL_L_MIN, FILL_L_MAX, FILL_TL_MIN, FILL_TL_MAX)
blades = tall + fills
blades.sort(key=lambda b: -b['base_y'])
print(f"Placed {len(blades)} blades")

# ── Low-level tube mesh ────────────────────────────────────────────────────────

def _build_tube_mesh(spine_3d, widths, thicknesses, initial_right=None):
    """
    Watertight triangular-prism tube following spine_3d.

    Cross-section at each ring (n=3 vertices, parallel-transported frame):
      • Vertex 0  — keel:      centre + (2T/3) * right
      • Vertex 1  — top-right: centre + (-T/3) * right + (W/2) * up_loc
      • Vertex 2  — top-left:  centre + (-T/3) * right - (W/2) * up_loc
    where W = widths[i] (flat-face width) and T = thicknesses[i] (keel depth).

    `right` is seeded from `initial_right` (the blade's lean/azimuth direction)
    and parallel-transported along the spine.  By the time the blade flops
    horizontal, parallel transport has rotated `right` to point downward (−Z),
    so the keel is on the underside and the flat face is on top — automatically,
    with no extra rotation needed.
    """
    n     = 3
    path  = np.array(spine_3d,  dtype=float)
    W_arr = np.array(widths,     dtype=float)
    T_arr = np.array(thicknesses, dtype=float)
    n_pts = len(path)

    world_up = np.array([0.0, 0.0, 1.0])
    verts, faces = [], []

    def add_pt(p):
        idx = len(verts); verts.append(p.tolist()); return idx

    def add_ring(centre, W, T, right, up_loc):
        idx = len(verts)
        verts.append((centre + (2*T/3) * right                    ).tolist())  # keel
        verts.append((centre + (-T/3)  * right + (W/2) * up_loc  ).tolist())  # top-right
        verts.append((centre + (-T/3)  * right - (W/2) * up_loc  ).tolist())  # top-left
        return idx

    rings = []
    prev_right = None
    for i in range(n_pts):
        tang = path[i+1] - path[i] if i < n_pts-1 else path[i] - path[i-1]
        tang = tang / (np.linalg.norm(tang) + 1e-9)

        if prev_right is None:
            right = np.array(initial_right, dtype=float) if initial_right is not None \
                    else np.cross(world_up, tang)
            if np.linalg.norm(right) < 0.01:
                right = np.array([1.0, 0.0, 0.0])
            right -= np.dot(right, tang) * tang
            r_norm = np.linalg.norm(right)
            if r_norm < 1e-6:
                right = np.array([0.0, 1.0, 0.0])
                right -= np.dot(right, tang) * tang
                r_norm = np.linalg.norm(right) + 1e-9
            right = right / r_norm
        else:
            right = prev_right - np.dot(prev_right, tang) * tang
            r_norm = np.linalg.norm(right)
            if r_norm < 1e-6:
                right = np.array([1.0, 0.0, 0.0])
                right -= np.dot(right, tang) * tang
                r_norm = np.linalg.norm(right) + 1e-9
            right = right / r_norm

        up_loc = np.cross(tang, right)
        up_loc = up_loc / (np.linalg.norm(up_loc) + 1e-9)

        prev_right = right
        rings.append(add_ring(path[i], W_arr[i], T_arr[i], right, up_loc))

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

def make_grass_blade(support_z, base_pos, azimuth, length, width, thickness, tip_length,
                     lean_angle=LEAN_ANGLE, arc_fraction=ARC_FRACTION,
                     curl=0.0, n_path=N_PATH):
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
    arc_h   = thickness * arc_fraction * 2  # minimum bow height above obstacles
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

    # Pass 2 — required EXTRA height above terrain at each point.
    #   • Must clear the top surface of any existing blade at that XY.
    #   • Z-extent of this blade's own cross-section = hw × sin(lean), which is
    #     zero when the blade is vertical (base) and rises as it leans horizontal.
    #   • Tiny CLEARANCE gap, fading to zero at ends so base/tip are flush.
    #   • Minimum aesthetic arc (arc_fraction × diameter) so the blade lifts off.
    extra = np.zeros(n_path)
    for k in range(n_path):
        t       = k * dt
        sin_t   = np.sin(t * np.pi)
        s_k     = t * total_l
        t_tip_k = float(np.clip((s_k - length) / (tip_length + 1e-9), 0.0, 1.0))
        taper_k = np.cos(t_tip_k * np.pi / 2.0)
        T_k     = thickness * taper_k          # tapered keel depth at this point
        lean_t  = lean_angle * (1.0 - np.cos(t * np.pi / 2.0))
        hw_z    = T_k * np.sin(lean_t)         # Z-extent of cross-section due to lean
        support_extra = max(0.0, sz_path[k] - tz_path[k]) + CLEARANCE * sin_t
        min_arc_extra = arc_h * sin_t
        extra[k] = max(support_extra, min_arc_extra)
    extra[0] = extra[-1] = 0.0   # endpoints flush with terrain

    # Pass 3 — single piecewise-cosine arch anchored at the highest point.
    #   Rising half  (0 → k_peak): cosine ramp from 0 up to extra[k_peak].
    #   Falling half (k_peak → n-1): cosine ramp from extra[k_peak] back to 0.
    #   This gives ONE smooth, minimum-height arch that goes from ground, up over
    #   the highest obstacle, and back to ground — with zero slope at both ends
    #   and at the peak.  If extra is everywhere zero, arch is flat (on terrain).
    k_peak = int(np.argmax(extra))
    h_peak = float(extra[k_peak])

    arch = np.zeros(n_path)
    if h_peak > 0.0 and 0 < k_peak < n_path - 1:
        for k in range(k_peak + 1):
            frac    = k / k_peak
            arch[k] = h_peak * (1.0 - np.cos(frac * np.pi)) / 2.0
        for k in range(k_peak, n_path):
            frac    = (k - k_peak) / (n_path - 1 - k_peak)
            arch[k] = h_peak * (1.0 + np.cos(frac * np.pi)) / 2.0

    # Floor-clamp: if a secondary obstacle pokes above the arch, lift locally.
    np.maximum(arch, extra, out=arch)
    arch[0] = arch[-1] = 0.0    # restore flush endpoints

    # Pass 4 — build spine + taper.
    path_xyz      = []
    widths_arr    = []
    thicknesses_arr = []

    for k in range(n_path):
        x = xs_path[k]
        y = ys_path[k]
        z = tz_path[k] + float(arch[k])

        s     = k * dt * total_l
        t_tip = float(np.clip((s - length) / (tip_length + 1e-9), 0.0, 1.0))
        taper = np.cos(t_tip * np.pi / 2.0)

        path_xyz.append(np.array([x, y, z]))
        widths_arr.append(width     * taper)
        thicknesses_arr.append(thickness * taper)

    # ── Base inset: sink ring-0 into the terrain along the terrain normal ────────
    # This makes the base disk lie in (approximately) the terrain tangent plane
    # and eliminates edge gaps caused by terrain curvature.
    tn = terrain_normal_at(bx, by)
    path_xyz[0] = path_xyz[0] - BASE_INSET * tn

    # Seed the parallel-transport frame with a vector in the terrain tangent plane
    # (azimuth direction projected onto that plane).  The spine tangent at ring 0
    # now points roughly along tn (because path[0] is below terrain, path[1] is
    # just above), so the projected right vector stays in the tangent plane.
    az_vec  = np.array([np.sin(azimuth), np.cos(azimuth), 0.0])
    right_0 = az_vec - np.dot(az_vec, tn) * tn
    right_0_norm = np.linalg.norm(right_0)
    if right_0_norm < 1e-6:          # azimuth exactly along terrain normal (edge case)
        right_0 = np.array([1.0, 0.0, 0.0])
        right_0 -= np.dot(right_0, tn) * tn
    right_0 /= np.linalg.norm(right_0) + 1e-9

    mesh = _build_tube_mesh(path_xyz, widths_arr, thicknesses_arr, initial_right=right_0)
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
        thickness  = bl['thickness'],
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
