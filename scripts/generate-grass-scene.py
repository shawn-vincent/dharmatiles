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
from PIL import Image
import sys, pathlib

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from blade import render_blade

# ── Config ────────────────────────────────────────────────────────────────────
CANVAS_W   = CANVAS_H = 700
N_BLADES   = 125
N_FILL     = 300          # short fill blades for the ground layer
SEED       = 42
CAP_RATIO  = 0.6
DIR_SPREAD = np.radians(8)    # per-blade random jitter around flow direction
MAX_CURVE  = 0.70
OUTPUT     = "grass_scene.png"

# ── Flow field ────────────────────────────────────────────────────────────────
# FLOW_TYPE:     'vertical' | 'swirl' | 'radial' | 'diagonal'
# FLOW_STRENGTH: 0 = pure vertical, 1 = full flow direction
FLOW_TYPE     = 'swirl'
FLOW_STRENGTH = 0.45


def build_flow_field():
    """Return a (CANVAS_H, CANVAS_W) array of blade-direction angles (radians
    from vertical, CW positive).  The flow vector is blended with straight-up
    so FLOW_STRENGTH controls how strongly the pattern asserts itself."""
    yy, xx = np.mgrid[0:CANVAS_H, 0:CANVAS_W]
    xn = (xx / CANVAS_W - 0.5).astype(np.float32)   # normalised [-0.5, 0.5]
    yn = (yy / CANVAS_H - 0.5).astype(np.float32)
    r  = np.sqrt(xn**2 + yn**2) + 1e-9

    if FLOW_TYPE == 'vertical':
        return np.zeros((CANVAS_H, CANVAS_W), dtype=np.float32)

    elif FLOW_TYPE == 'swirl':          # CW rotation around canvas centre
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

# Jittered grid is placed over the valid y range where blade bases can fit.
# Min tot uses smallest possible blade dimensions.
MIN_TOT  = 14 + 100 + (14 / 2) * CAP_RATIO   # ≈ 118 px
valid_y0 = MIN_TOT
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
        w   = rng.uniform(14,  32)
        L   = rng.uniform(100, 200)
        tl  = rng.uniform(14,  28)
        cs  = rng.uniform(0.08, 0.22)
        pw  = rng.uniform(0.40, 0.75)

        bx = (cell_c + rng.uniform(0.0, 1.0)) * cell_w
        by = valid_y0 + (cell_r + rng.uniform(0.0, 1.0)) * cell_h

        iy  = int(np.clip(by, 0, CANVAS_H - 1))
        ix  = int(np.clip(bx, 0, CANVAS_W - 1))
        d   = float(flow_field[iy, ix]) + rng.uniform(-DIR_SPREAD, DIR_SPREAD)
        # Curve follows flow curvature direction; magnitude is random.
        kap = float(curvature_field[iy, ix])
        cur = kap * rng.uniform(0.3 * MAX_CURVE, MAX_CURVE)

        if fits(bx, by, w, L, tl, cur, cs, d):
            blades.append(dict(base_x=bx, base_y=by,
                               width=w, length=L, tip_length=tl,
                               curve=cur, curve_start=cs, direction=d, power=pw))
            break

accept_rate = len(blades) / tries * 100
print(f"Placed {len(blades)}/{N_BLADES} tall blades in {tries} attempts  ({accept_rate:.0f}% accept rate)")

# ── Fill layer: short blades across the bottom half ───────────────────────────
# These are short enough that their bases fit anywhere in the lower canvas.
FILL_L_MIN, FILL_L_MAX   = 40, 90
FILL_TL_MIN, FILL_TL_MAX = 10, 20
FILL_W_MIN, FILL_W_MAX   =  8, 20
FILL_MIN_TOT = FILL_TL_MIN + FILL_L_MIN + (FILL_W_MIN / 2) * CAP_RATIO  # ≈ 27 px
fill_y0 = FILL_MIN_TOT
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
        cur = rng.uniform(-MAX_CURVE,  MAX_CURVE)
        cs  = rng.uniform(0.08, 0.22)
        pw  = rng.uniform(0.40, 0.75)

        bx = (cell_c + rng.uniform(0.0, 1.0)) * fill_cw
        by = fill_y0 + (cell_r + rng.uniform(0.0, 1.0)) * fill_ch

        iy = int(np.clip(by, 0, CANVAS_H - 1))
        ix = int(np.clip(bx, 0, CANVAS_W - 1))
        d  = float(flow_field[iy, ix]) + rng.uniform(-DIR_SPREAD, DIR_SPREAD)

        if fits(bx, by, w, L, tl, cur, cs, d):
            blades.append(dict(base_x=bx, base_y=by,
                               width=w, length=L, tip_length=tl,
                               curve=cur, curve_start=cs, direction=d, power=pw))
            break

n_fill_placed = len(blades) - N_BLADES
print(f"Placed {n_fill_placed}/{N_FILL} fill blades in {fill_tries} attempts")

# Back-to-front: smallest base_y first (furthest = background)
blades.sort(key=lambda b: b['base_y'])

# ── Render ────────────────────────────────────────────────────────────────────
canvas = np.zeros((CANVAS_H, CANVAS_W), dtype=np.float32)

for i, bl in enumerate(blades):
    (y0, y1, x0, x1), B, A = render_blade(
        canvas_w=CANVAS_W, canvas_h=CANVAS_H,
        width=bl['width'],      length=bl['length'],
        tip_length=bl['tip_length'],
        curve=bl['curve'],      curve_start=bl['curve_start'],
        direction=bl['direction'], power=bl['power'],
        base_x=bl['base_x'],   base_y=bl['base_y'],
        return_layers=True,
    )
    roi = canvas[y0:y1, x0:x1]
    canvas[y0:y1, x0:x1] = roi * (1.0 - A) + B * A

    if (i + 1) % 100 == 0:
        print(f"  {i+1}/{len(blades)}…")

Image.fromarray(np.clip(canvas * 255, 0, 255).astype(np.uint8), 'L').save(OUTPUT)
print(f"Saved {OUTPUT}")
