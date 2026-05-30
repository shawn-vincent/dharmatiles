#!/usr/bin/env python3
"""
Grass tile scene — 700×700 px (35×35 mm at 20 px/mm).

Rules:
  - No blade clipped by any canvas edge (rejection sampling).
  - Background blades (smallest base_y) have base_y ≈ total_length,
    so their tips naturally reach the top edge.
  - Painter's algorithm: sort by base_y ascending (back → front).
"""
import numpy as np
from PIL import Image, ImageDraw
import sys, pathlib, os, subprocess

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from blade import render_blade

# ── Config ────────────────────────────────────────────────────────────────────
CANVAS_W   = CANVAS_H = 700
N_BLADES   = 62
N_FILL     = 150          # short fill blades for the ground layer
SEED       = 42
CAP_RATIO  = 0.6
DIR_SPREAD = np.radians(8)    # per-blade random jitter around flow direction
MAX_CURVE  = 0.70
OUTPUT     = "grass_scene.png"
DEBUG      = True             # generate -debug PNG with order-colored arrows

# ── Flow field ────────────────────────────────────────────────────────────────
# FLOW_TYPE:     'circle' | 'vertical' | 'swirl' | 'radial' | 'diagonal'
# FLOW_STRENGTH: 0 = pure vertical, 1 = full flow direction (ignored for 'circle')
#
# 'circle' — pure CW tangential rotation.  The canonical circle has radius =
#             min(W,H)/2, touching all four tile edges at their midpoints.
#             No blending with vertical: blades point wherever the circle goes.
FLOW_TYPE     = 'circle'
FLOW_STRENGTH = 0.45   # used only by 'swirl' / 'radial' / 'diagonal'


def build_flow_field():
    """Return a (CANVAS_H, CANVAS_W) array of blade-direction angles (radians
    from vertical, CW positive)."""
    yy, xx = np.mgrid[0:CANVAS_H, 0:CANVAS_W]
    xn = (xx / CANVAS_W - 0.5).astype(np.float32)   # normalised [-0.5, 0.5]
    yn = (yy / CANVAS_H - 0.5).astype(np.float32)
    r  = np.sqrt(xn**2 + yn**2) + 1e-9

    if FLOW_TYPE == 'vertical':
        return np.zeros((CANVAS_H, CANVAS_W), dtype=np.float32)

    elif FLOW_TYPE == 'circle':         # pure CW tangential — no vertical blend
        fx, fy = yn / r, -xn / r
        return np.arctan2(fx, -fy).astype(np.float32)

    elif FLOW_TYPE == 'swirl':          # CW rotation blended with vertical
        fx, fy = yn / r, -xn / r
    elif FLOW_TYPE == 'radial':         # fan outward from centre
        fx, fy = xn / r,  yn / r
    elif FLOW_TYPE == 'diagonal':       # uniform 45° sweep
        a  = np.radians(45)
        fx = np.full((CANVAS_H, CANVAS_W), np.sin(a),  dtype=np.float32)
        fy = np.full((CANVAS_H, CANVAS_W), -np.cos(a), dtype=np.float32)
    else:
        return np.zeros((CANVAS_H, CANVAS_W), dtype=np.float32)

    # Blend flow vector with straight-up (0, −1) then convert to angle.
    s   = FLOW_STRENGTH
    bfx =  fx * s
    bfy =  fy * s + (-1.0) * (1.0 - s)
    mag = np.sqrt(bfx**2 + bfy**2) + 1e-9
    return np.arctan2(bfx / mag, -bfy / mag).astype(np.float32)


def build_curvature_field(angle_field):
    """Signed curvature of the flow streamlines, normalised to [-1, 1].

    κ = ∇θ · f̂  — the directional derivative of the angle θ along the flow
    direction f̂ = (sin θ, −cos θ).  Positive = streamline bending CW,
    negative = CCW.  Robust 95th-percentile normalisation keeps outliers
    (e.g. near a swirl centre) from swamping the rest of the field.
    """
    dθ_dy, dθ_dx = np.gradient(angle_field.astype(np.float64))
    fx    =  np.sin(angle_field)   # flow unit vector x
    fy    = -np.cos(angle_field)   # flow unit vector y
    kappa = dθ_dx * fx + dθ_dy * fy
    scale = np.percentile(np.abs(kappa), 95) + 1e-9
    return np.clip(kappa / scale, -1.0, 1.0).astype(np.float32)


def fits(bx, by, w, L, tl, cur, cs, d):
    """True if the blade's full unclipped bbox lies within the canvas."""
    a   = w / 2
    b   = a * CAP_RATIO
    tot = tl + L + b
    scs = tot * cs
    ycs = by - scs           # y_curve_start in natural frame

    if abs(cur) < 0.005:
        y_apex = ycs - (tot - scs)
        chord  = 0.0
        nx0, nx1 = bx - a, bx + a
    else:
        th  = abs(cur) * (np.pi / 2)
        R   = (tot - scs) / th
        sgn = np.sign(cur)
        lat = R * (1 - np.cos(th))
        y_apex = ycs - R * np.sin(th)
        chord  = np.arctan2(sgn * lat, by - y_apex)
        if sgn > 0:
            nx0, nx1 = bx - a, bx + lat + a
        else:
            nx0, nx1 = bx - lat - a, bx + a

    ny0, ny1 = y_apex - a, by + b

    rot = d - chord
    if abs(rot) < 0.001:
        return nx0 >= 0 and nx1 <= CANVAS_W and ny0 >= 0 and ny1 <= CANVAS_H

    cr, sr = np.cos(rot), np.sin(rot)
    cxc = np.array([nx0-bx, nx1-bx, nx0-bx, nx1-bx])
    cyc = np.array([ny0-by, ny0-by, ny1-by, ny1-by])
    sx  = cxc*cr - cyc*sr + bx
    sy  = cxc*sr + cyc*cr + by
    return (sx.min() >= 0 and sx.max() <= CANVAS_W and
            sy.min() >= 0 and sy.max() <= CANVAS_H)


# ── Generate blades via jittered grid ─────────────────────────────────────────
rng        = np.random.default_rng(SEED)
blades     = []
tries      = 0
flow_field      = build_flow_field()
curvature_field = build_curvature_field(flow_field)
print(f"Flow: {FLOW_TYPE}  strength={FLOW_STRENGTH}")

# Full canvas coverage — fits() handles all flow directions (up, sideways, down).
valid_y0 = 0
valid_y1 = CANVAS_H

grid_cols = int(np.ceil(np.sqrt(N_BLADES)))
grid_rows = int(np.ceil(N_BLADES / grid_cols))
cell_w = CANVAS_W / grid_cols
cell_h = (valid_y1 - valid_y0) / grid_rows

cells = [(c, r) for c in range(grid_cols) for r in range(grid_rows)]
rng.shuffle(cells)

cell_idx = 0
while len(blades) < N_BLADES and cell_idx < len(cells):
    cell_c, cell_r = cells[cell_idx]
    cell_idx += 1
    for _ in range(50):          # up to 50 attempts per cell
        tries += 1
        w   = rng.uniform(17,  38)
        L   = rng.uniform(100, 200)
        tl  = rng.uniform(28,  56)
        cs  = rng.uniform(0.08, 0.22)
        pw  = rng.uniform(0.40, 0.75)

        # Sample the blade MIDPOINT within the cell, then offset base upstream.
        # This works for any flow direction — up, sideways, or down.
        mx = (cell_c + rng.uniform(0.0, 1.0)) * cell_w
        my = valid_y0 + (cell_r + rng.uniform(0.0, 1.0)) * cell_h

        iy  = int(np.clip(my, 0, CANVAS_H - 1))
        ix  = int(np.clip(mx, 0, CANVAS_W - 1))
        d0  = float(flow_field[iy, ix])
        kap = float(curvature_field[iy, ix])

        d   = d0 + rng.uniform(-DIR_SPREAD, DIR_SPREAD)
        cur = kap * rng.uniform(0.0, 2.0 * MAX_CURVE)

        tot_est = tl + L + (w / 2) * CAP_RATIO
        bx = mx - np.sin(d0) * tot_est * 0.5
        by = my + np.cos(d0) * tot_est * 0.5

        if fits(bx, by, w, L, tl, cur, cs, d):
            blades.append(dict(base_x=bx, base_y=by,
                               width=w, length=L, tip_length=tl,
                               curve=cur, curve_start=cs, direction=d, power=pw))
            break

accept_rate = len(blades) / tries * 100
print(f"Placed {len(blades)}/{N_BLADES} tall blades in {tries} attempts  ({accept_rate:.0f}% accept rate)")

# ── Fill layer: short blades across the full canvas ───────────────────────────
FILL_L_MIN, FILL_L_MAX   = 40, 90
FILL_TL_MIN, FILL_TL_MAX = 20, 40
FILL_W_MIN, FILL_W_MAX   = 10, 24
fill_y0 = 0
fill_y1 = CANVAS_H

fill_cols = int(np.ceil(np.sqrt(N_FILL)))
fill_rows = int(np.ceil(N_FILL / fill_cols))
fill_cw   = CANVAS_W / fill_cols
fill_ch   = (fill_y1 - fill_y0) / fill_rows

fill_cells = [(c, r) for c in range(fill_cols) for r in range(fill_rows)]
rng.shuffle(fill_cells)

fill_tries = 0
fill_idx   = 0
while len(blades) < N_BLADES + N_FILL and fill_idx < len(fill_cells):
    cell_c, cell_r = fill_cells[fill_idx]
    fill_idx += 1
    for _ in range(50):
        fill_tries += 1
        w   = rng.uniform(FILL_W_MIN,  FILL_W_MAX)
        L   = rng.uniform(FILL_L_MIN,  FILL_L_MAX)
        tl  = rng.uniform(FILL_TL_MIN, FILL_TL_MAX)
        cs  = rng.uniform(0.08, 0.22)
        pw  = rng.uniform(0.40, 0.75)

        mx = (cell_c + rng.uniform(0.0, 1.0)) * fill_cw
        my = fill_y0 + (cell_r + rng.uniform(0.0, 1.0)) * fill_ch

        iy  = int(np.clip(my, 0, CANVAS_H - 1))
        ix  = int(np.clip(mx, 0, CANVAS_W - 1))
        d0  = float(flow_field[iy, ix])
        kap = float(curvature_field[iy, ix])

        d   = d0 + rng.uniform(-DIR_SPREAD, DIR_SPREAD)
        cur = kap * rng.uniform(0.0, 2.0 * MAX_CURVE)

        tot_est = tl + L + (w / 2) * CAP_RATIO
        bx = mx - np.sin(d0) * tot_est * 0.5
        by = my + np.cos(d0) * tot_est * 0.5

        if fits(bx, by, w, L, tl, cur, cs, d):
            blades.append(dict(base_x=bx, base_y=by,
                               width=w, length=L, tip_length=tl,
                               curve=cur, curve_start=cs, direction=d, power=pw))
            break

n_fill_placed = len(blades) - N_BLADES
print(f"Placed {n_fill_placed}/{N_FILL} fill blades in {fill_tries} attempts")

# Sort back-to-front by the blade's frontmost screen-y: the bottom of its
# bounding box in canvas space.  For upward blades (d≈0) that is the base;
# for downward blades (d≈π) it is the tip.
# key = base_y + max(0, −cos(d)) × tot  (≈ tip_y when d=π, ≈ base_y when d=0)
def front_y(bl):
    tot = bl['tip_length'] + bl['length'] + bl['width'] / 2 * CAP_RATIO
    return bl['base_y'] + max(0.0, -np.cos(bl['direction'])) * tot

blades.sort(key=front_y)

# ── Render ────────────────────────────────────────────────────────────────────
canvas = np.zeros((CANVAS_H, CANVAS_W), dtype=np.float32)

BASE_COVER_THRESH = 0.15   # canvas brightness above which a base is "covered"
DARK_SNAP_THRESH  = 0.08   # pixels below this are valid gap targets for snapping

for i, bl in enumerate(blades):
    # Detect whether the blade's base lands on top of an existing blade.
    # If so, render with a pointed base so it reads as passing through the grass.
    bxi = int(np.clip(bl['base_x'], 0, CANVAS_W - 1))
    byi = int(np.clip(bl['base_y'], 0, CANVAS_H - 1))
    rc  = int(bl['width'] / 2) + 2
    base_patch = canvas[max(0, byi-rc):min(CANVAS_H, byi+rc+1),
                        max(0, bxi-rc):min(CANVAS_W, bxi+rc+1)]
    pointed = bool(base_patch.mean() > BASE_COVER_THRESH)

    base_x, base_y = bl['base_x'], bl['base_y']

    if pointed:
        # Snap the base tip to the nearest dark pixel (a gap between blades).
        # Search within 1.5× blade widths of the original base position.
        sr  = int(bl['width'] * 1.5)
        x0s = max(0, bxi - sr);  x1s = min(CANVAS_W, bxi + sr + 1)
        y0s = max(0, byi - sr);  y1s = min(CANVAS_H, byi + sr + 1)
        snap_patch = canvas[y0s:y1s, x0s:x1s]
        dark_mask  = snap_patch < DARK_SNAP_THRESH
        if dark_mask.any():
            dys, dxs = np.where(dark_mask)
            dists    = np.sqrt((dxs + x0s - bxi) ** 2 + (dys + y0s - byi) ** 2)
            ni       = np.argmin(dists)
            base_x   = float(dxs[ni] + x0s)
            base_y   = float(dys[ni] + y0s)
            # Update in list so debug arrows reflect snapped position
            bl['base_x'] = base_x
            bl['base_y'] = base_y

    (y0, y1, x0, x1), B, A = render_blade(
        canvas_w=CANVAS_W, canvas_h=CANVAS_H,
        width=bl['width'],      length=bl['length'],
        tip_length=bl['tip_length'],
        curve=bl['curve'],      curve_start=bl['curve_start'],
        direction=bl['direction'], power=bl['power'],
        base_x=base_x,          base_y=base_y,
        pointed_base=pointed,
        return_layers=True,
    )
    roi = canvas[y0:y1, x0:x1]
    canvas[y0:y1, x0:x1] = roi * (1.0 - A) + B * A

    if (i + 1) % 100 == 0:
        print(f"  {i+1}/{len(blades)}…")

Image.fromarray(np.clip(canvas * 255, 0, 255).astype(np.uint8), 'L').save(OUTPUT)
print(f"Saved {OUTPUT}")

# ── Debug overlay: order-colored arrows at each blade base ────────────────────
if DEBUG:
    gray8 = np.clip(canvas * 255, 0, 255).astype(np.uint8)
    rgb   = np.stack([gray8, gray8, gray8], axis=2)
    dbg   = Image.fromarray(rgb, 'RGB')
    draw  = ImageDraw.Draw(dbg)

    n          = len(blades)
    ARROW_LEN  = 32          # shaft length in pixels
    HEAD_LEN   = 10          # arrowhead depth in pixels
    HEAD_W     = 6           # arrowhead half-width in pixels

    # Color ramp: blue-green → red-purple
    C_FIRST = np.array([0,   210, 170], dtype=float)   # blue-green
    C_LAST  = np.array([210,   0, 170], dtype=float)   # red-purple

    for i, bl in enumerate(blades):
        t     = i / max(n - 1, 1)
        color = tuple((C_FIRST * (1 - t) + C_LAST * t).astype(int))

        bx, by = bl['base_x'], bl['base_y']
        d      = bl['direction']

        # Unit vector along blade direction (CW from straight-up)
        ux, uy = np.sin(d), -np.cos(d)
        # Perpendicular (90° CW)
        px, py = -uy, ux

        # Arrow tip point
        tx = bx + ux * ARROW_LEN
        ty = by + uy * ARROW_LEN

        # Arrowhead base centre (step back HEAD_LEN from tip)
        hbx = tx - ux * HEAD_LEN
        hby = ty - uy * HEAD_LEN

        # Arrowhead wing points
        w1 = (hbx + px * HEAD_W, hby + py * HEAD_W)
        w2 = (hbx - px * HEAD_W, hby - py * HEAD_W)

        draw.line([(bx, by), (tx, ty)], fill=color, width=2)
        draw.polygon([(tx, ty), w1, w2], fill=color)

    base, ext   = os.path.splitext(OUTPUT)
    debug_out   = base + '-debug' + ext
    dbg.save(debug_out)
    print(f"Saved {debug_out}")
    subprocess.Popen(['open', debug_out])
