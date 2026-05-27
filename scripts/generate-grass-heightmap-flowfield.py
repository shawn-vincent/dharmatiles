#!/usr/bin/env python3
"""
Procedural grass heightmap generator — flow-field / streamline edition.

Instead of a jittered grid of independent tufts, grass blades grow along
streamlines traced through a per-tile vector field, giving each tile a
readable "personality":

  linear   — uniform sweep in one direction (wind)
  swirl    — grass curls around a centre (cowlick / whirlpool)
  radial   — grass fans outward from a source point (burst)
  inward   — grass funnels toward a sink point (drain)
  corner   — diagonal sweep from corner to corner
  dipole   — grass curves between two opposing poles

The base field is perturbed with divergence-free curl noise so no blade
path is ever perfectly straight.  Streamlines branch occasionally, producing
organic side-shoots.  Adjacent blades naturally align without any explicit
neighbour-lookup because they sample nearby field values.

The dirt + rock layers from the tuft-based generator are retained unchanged.

Usage:
  python3 generate-grass-heightmap-flowfield.py [output.png] [--seed N] [--size N]
  python3 generate-grass-heightmap-flowfield.py [output.png] --dirt-only
"""

import argparse
import math
import os
import random

import numpy as np
from PIL import Image

# ── Canvas ────────────────────────────────────────────────────────────────────
DEFAULT_SIZE       = 512

# ── Flow field ─────────────────────────────────────────────────────────────────
FIELD_TYPES        = ['linear', 'swirl', 'radial', 'inward', 'corner', 'dipole']
CURL_STRENGTH      = 0.55   # 0 = pure base field, 1 = dominated by curl noise
CURL_COARSE        = 14     # coarse-grid resolution for the curl noise scalar field

# ── Streamline tracing ─────────────────────────────────────────────────────────
N_PRIMARY_SEEDS    = 40     # number of primary streamline seeds (jittered grid)
STEP_SIZE          = 5      # pixels per Euler integration step
MAX_STEPS          = 160    # max steps per streamline before stopping
BRANCH_PROB        = 0.022  # per-step probability of spawning a branch
BRANCH_MAX_ANGLE   = 0.45   # max branch angular offset from parent (radians)
BRANCH_MAX_DEPTH   = 2      # max nesting depth for branches

# ── Blade placement ────────────────────────────────────────────────────────────
BLADE_SPACING      = 15     # streamline steps between consecutive blade placements
BLADE_LENGTH_MIN   = 0.24   # blade length as fraction of S  (≈ 123px / ~8mm at S=512)
BLADE_LENGTH_MAX   = 0.50   # blade length as fraction of S  (≈ 256px / ~17mm at S=512)
BLADE_BASE_W       = 20.0   # base width in pixels at S=512
BLADE_ANGLE_JITTER = 0.12   # per-blade direction noise, std-dev (radians)
BLADE_CURVE_MAX    = 0.28   # max lateral Bezier offset as fraction of blade length
LAYER_RANGE        = 0.50   # per-streamline layer-offset range (depth separation)

# ── Blade cross-section ridge ─────────────────────────────────────────────────
# Sharp Gaussian spine: height = h_along * exp(-k*(dist/w)^2).
# Peaks at the centreline, drops to ~1% at the blade edge (dist=w).
# Larger k = narrower/sharper spike.
BLADE_RIDGE_K      = 10.0

# ── Blade shadow groove ────────────────────────────────────────────────────────
# A narrow trench is carved just outside each blade edge before the blade
# fill is drawn.  This physically separates adjacent blades in the 3D print.
# Drawing shadow BEFORE fill means any later blade's body overwrites shadow
# that lands inside it — grooves survive only in the inter-blade gaps.
BLADE_SHADOW_W     = 1.9   # shadow outer radius as multiple of blade half-width
BLADE_SHADOW_DEPTH = 0.15  # height subtracted in shadow zone (pre-normalisation)

# ── Blade height profile ───────────────────────────────────────────────────────
GRASS_BOTTOM       = 0.10   # blade base height (soil level)
LAYER_STACK_H      = 0.75   # total height range base→tip
BLADE_ARC_HEIGHT   = 0.20   # small mid-blade height bonus

# ── Dirt layer (unchanged from tuft generator) ────────────────────────────────
DIRT_MAX           = 0.32
ROCK_ATTEMPTS      = 120
ROCK_POWER         = 3.5


# ═══════════════════════════════════════════════════════════════════════════════
# Flow field construction
# ═══════════════════════════════════════════════════════════════════════════════

def _smooth_scalar_field(S, seed):
    """
    Smooth scalar field for curl-noise: random coarse grid bicubic-upsampled to S×S.
    Uses a numpy rng seeded independently so dirt-layer rng consumption doesn't
    alter the field shape.
    """
    nrng = np.random.default_rng(seed ^ 0xDEADBEEF)
    raw  = nrng.random((CURL_COARSE, CURL_COARSE)).astype(np.float32)
    img  = Image.fromarray((raw * 255).astype(np.uint8), mode='L')
    img  = img.resize((S, S), Image.BICUBIC)
    return np.array(img, dtype=np.float32) / 127.5 - 1.0   # → [-1, 1]


def _curl_of_scalar(P):
    """
    Divergence-free 2D curl of scalar stream function P.
    curl(P) = (∂P/∂y, −∂P/∂x)  — always zero divergence.
    """
    dPdy = np.empty_like(P)
    dPdx = np.empty_like(P)

    dPdy[1:-1, :] = (P[2:, :]  - P[:-2, :])  * 0.5
    dPdy[0,    :] = P[1, :]    - P[0,  :]
    dPdy[-1,   :] = P[-1, :]   - P[-2, :]

    dPdx[:, 1:-1] = (P[:, 2:]  - P[:, :-2])  * 0.5
    dPdx[:, 0   ] = P[:,  1]   - P[:,   0]
    dPdx[:, -1  ] = P[:, -1]   - P[:,  -2]

    return dPdy, -dPdx   # (curl_x, curl_y)


def _base_field(S, field_type, rng):
    """
    Analytically-defined base vector field for field_type.
    Returns (dx, dy) each (S, S) float32, not yet normalised.
    """
    yy, xx = np.mgrid[0:S, 0:S]
    xn = (xx / S - 0.5).astype(np.float32)   # normalised to [-0.5, 0.5]
    yn = (yy / S - 0.5).astype(np.float32)

    if field_type == 'linear':
        a  = rng.uniform(0, 2 * math.pi)
        dx = np.full((S, S), math.cos(a), dtype=np.float32)
        dy = np.full((S, S), math.sin(a), dtype=np.float32)

    elif field_type == 'corner':
        a  = rng.choice([math.pi/4, 3*math.pi/4, 5*math.pi/4, 7*math.pi/4])
        dx = np.full((S, S), math.cos(a), dtype=np.float32)
        dy = np.full((S, S), math.sin(a), dtype=np.float32)

    elif field_type == 'radial':
        cx = rng.uniform(-0.18, 0.18)
        cy = rng.uniform(-0.18, 0.18)
        dx = (xn - cx).copy()
        dy = (yn - cy).copy()

    elif field_type == 'inward':
        cx = rng.uniform(-0.18, 0.18)
        cy = rng.uniform(-0.18, 0.18)
        dx = -(xn - cx).copy()
        dy = -(yn - cy).copy()

    elif field_type == 'swirl':
        cx   = rng.uniform(-0.12, 0.12)
        cy   = rng.uniform(-0.12, 0.12)
        sign = rng.choice([-1.0, 1.0])
        dx   = (-sign * (yn - cy)).astype(np.float32)
        dy   = ( sign * (xn - cx)).astype(np.float32)

    elif field_type == 'dipole':
        sep    = rng.uniform(0.15, 0.28)
        a      = rng.uniform(0, 2 * math.pi)
        cx1, cy1 =  math.cos(a) * sep,  math.sin(a) * sep
        cx2, cy2 = -cx1, -cy1
        r1sq   = (xn - cx1)**2 + (yn - cy1)**2 + 1e-4
        r2sq   = (xn - cx2)**2 + (yn - cy2)**2 + 1e-4
        dx     = ((xn - cx1) / r1sq - (xn - cx2) / r2sq).astype(np.float32)
        dy     = ((yn - cy1) / r1sq - (yn - cy2) / r2sq).astype(np.float32)

    else:
        raise ValueError(f"Unknown field type: {field_type!r}")

    return dx, dy


def make_flow_field(S, seed, rng):
    """
    Build the complete flow field: base field blended with curl noise.
    Returns (field, field_type) where field is float32 (S, S, 2) unit vectors.
    """
    field_type = rng.choice(FIELD_TYPES)

    bx, by = _base_field(S, field_type, rng)
    bmag   = np.sqrt(bx**2 + by**2) + 1e-9
    bx    /= bmag
    by    /= bmag

    P      = _smooth_scalar_field(S, seed)
    cx, cy = _curl_of_scalar(P)
    cmag   = np.sqrt(cx**2 + cy**2) + 1e-9
    cx    /= cmag
    cy    /= cmag

    s  = CURL_STRENGTH
    fx = (1.0 - s) * bx + s * cx
    fy = (1.0 - s) * by + s * cy
    mag = np.sqrt(fx**2 + fy**2) + 1e-9

    field = np.stack([fx / mag, fy / mag], axis=-1).astype(np.float32)
    return field, field_type


# ═══════════════════════════════════════════════════════════════════════════════
# Streamline tracing
# ═══════════════════════════════════════════════════════════════════════════════

def _sample(field, x, y):
    """Bilinear-sample field[S, S, 2] at float position (x, y)."""
    S = field.shape[0]
    x = max(0.0, min(x, S - 1.001))
    y = max(0.0, min(y, S - 1.001))
    ix, iy = int(x), int(y)
    fx, fy = x - ix, y - iy
    ix1, iy1 = min(ix + 1, S - 1), min(iy + 1, S - 1)
    v = (field[iy,  ix]  * (1 - fx) * (1 - fy)
       + field[iy,  ix1] *      fx  * (1 - fy)
       + field[iy1, ix]  * (1 - fx) *      fy
       + field[iy1, ix1] *      fx  *      fy)
    return float(v[0]), float(v[1])


def _jittered_seeds(S, n, rng):
    """Evenly-distributed seed positions via jittered grid."""
    cols = max(1, round(math.sqrt(n)))
    rows = max(1, math.ceil(n / cols))
    cw, rh = S / cols, S / rows
    indices = list(range(cols * rows))
    rng.shuffle(indices)
    pts = []
    for k in indices[:n]:
        ci, ri = k % cols, k // cols
        pts.append(((ci + rng.uniform(0.1, 0.9)) * cw,
                    (ri + rng.uniform(0.1, 0.9)) * rh))
    return pts


def _trace(field, x0, y0, S, rng, depth):
    """
    Euler-integrate from (x0, y0) through field.
    Returns:
        points  — list of (x, y, dx, dy)
        pending — list of (x, y, depth) branch seeds to queue
    """
    x, y    = float(x0), float(y0)
    points  = []
    pending = []
    for _ in range(MAX_STEPS):
        if not (1 <= x < S - 1 and 1 <= y < S - 1):
            break
        dx, dy = _sample(field, x, y)
        points.append((x, y, dx, dy))
        if depth < BRANCH_MAX_DEPTH and rng.random() < BRANCH_PROB:
            ba = math.atan2(dy, dx) + rng.uniform(-BRANCH_MAX_ANGLE, BRANCH_MAX_ANGLE)
            pending.append((x, y, depth + 1))
            # stash the branched angle implicitly — branches use field at their start
            # (we just seed them at the branch point; field guides them from there)
        x += dx * STEP_SIZE
        y += dy * STEP_SIZE
    return points, pending


def collect_streamlines(field, S, rng):
    """
    Trace all primary streamlines plus any branches they spawn.
    Returns a flat list of streamlines; each is a list of (x, y, dx, dy).
    """
    seeds  = _jittered_seeds(S, N_PRIMARY_SEEDS, rng)
    queue  = [(x, y, 0) for x, y in seeds]
    result = []
    while queue:
        x, y, depth = queue.pop()
        pts, pending = _trace(field, x, y, S, rng, depth)
        if len(pts) >= BLADE_SPACING:
            result.append(pts)
        queue.extend(pending)
    return result


# ═══════════════════════════════════════════════════════════════════════════════
# Blade geometry
# ═══════════════════════════════════════════════════════════════════════════════

def quadratic_bezier(p0, p1, p2, n=80):
    t = np.linspace(0, 1, n)
    x = (1-t)**2 * p0[0] + 2*(1-t)*t * p1[0] + t**2 * p2[0]
    y = (1-t)**2 * p0[1] + 2*(1-t)*t * p1[1] + t**2 * p2[1]
    return x, y


def make_blade_h():
    """Strictly-increasing height function shared by all blades.
    h(t) = GRASS_BOTTOM + t·LAYER_STACK_H + BLADE_ARC_HEIGHT·t·(1−t)
    Strictly increasing because h′(t) = LAYER_STACK_H + BLADE_ARC_HEIGHT·(1−2t);
    worst case at t=1: 0.75 − 0.20 = 0.55 > 0.
    """
    def h(t):
        return GRASS_BOTTOM + t * LAYER_STACK_H + BLADE_ARC_HEIGHT * t * (1.0 - t)
    return h


def make_blade_from_flow(base_x, base_y, flow_dx, flow_dy, length, bw, S, rng):
    """
    One blade growing from (base_x, base_y) in the flow direction.
    Adds small angular jitter and a gentle random curve.
    Returns (cx_pts, cy_pts) clipped to canvas, or None if too short.
    """
    angle  = math.atan2(flow_dy, flow_dx)
    angle += rng.gauss(0, BLADE_ANGLE_JITTER)

    tip_x = base_x + math.cos(angle) * length
    tip_y = base_y + math.sin(angle) * length

    # Lateral control-point offset → gentle organic curve
    lateral = rng.uniform(-BLADE_CURVE_MAX, BLADE_CURVE_MAX) * length
    perp    = angle + math.pi / 2
    ctrl_x  = (base_x + tip_x) * 0.5 + math.cos(perp) * lateral
    ctrl_y  = (base_y + tip_y) * 0.5 + math.sin(perp) * lateral

    cx_pts, cy_pts = quadratic_bezier((base_x, base_y), (ctrl_x, ctrl_y), (tip_x, tip_y))

    mask = (cx_pts >= 0) & (cx_pts < S) & (cy_pts >= 0) & (cy_pts < S)
    if mask.sum() < 6:
        return None
    return cx_pts[mask], cy_pts[mask]


def draw_blade(canvas, cx_pts, cy_pts, base_width, h_func, layer_offset=0.0):
    """Draw one blade: shadow groove first, then Gaussian ridge fill.

    Shadow is carved with np.minimum before the fill so any later blade's
    body (np.maximum) overwrites shadow that falls inside it.  Grooves
    survive only in the inter-blade gaps, physically separating neighbours.
    """
    H, W = canvas.shape
    n    = len(cx_pts)
    for i, (cx, cy) in enumerate(zip(cx_pts, cy_pts)):
        t     = i / max(n - 1, 1)
        ramp  = min(t / 0.06, 1.0)
        taper = (1.0 - t) ** 0.85
        w     = base_width * ramp * taper
        if w < 0.25:
            continue

        h_along = h_func(t) + layer_offset * t

        # Bounding box must cover shadow rim, not just blade body
        wi      = int(w * BLADE_SHADOW_W) + 2
        x0, x1  = max(0, int(cx) - wi), min(W, int(cx) + wi + 1)
        y0, y1  = max(0, int(cy) - wi), min(H, int(cy) + wi + 1)
        if x0 >= x1 or y0 >= y1:
            continue

        gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
        dist   = np.sqrt((gx - cx)**2 + (gy - cy)**2)

        # ── Shadow groove ────────────────────────────────────────────────────
        # Carve a trench just outside the blade edge.  Drawn before fill so
        # later blade bodies overwrite any shadow that lands inside them.
        if BLADE_SHADOW_DEPTH > 0 and w > 0:
            rim_outer  = w * BLADE_SHADOW_W
            rim_zone   = (dist > w) & (dist < rim_outer)
            rim_fade   = np.clip(
                1.0 - (dist - w) / (rim_outer - w + 1e-6), 0.0, 1.0)
            shadow_sub = BLADE_SHADOW_DEPTH * rim_fade * ramp  # zero at blade base
            canvas[y0:y1, x0:x1] = np.where(
                rim_zone,
                np.maximum(0.0, canvas[y0:y1, x0:x1] - shadow_sub),
                canvas[y0:y1, x0:x1])

        # ── Gaussian ridge fill ──────────────────────────────────────────────
        # Sharp spike at centreline, ~1% at blade edge.
        cross  = np.exp(-BLADE_RIDGE_K * (dist / w) ** 2)
        fill_h = h_along * cross
        fill   = np.where(dist <= w, fill_h, 0.0)
        canvas[y0:y1, x0:x1] = np.maximum(canvas[y0:y1, x0:x1], fill)


# ═══════════════════════════════════════════════════════════════════════════════
# Dirt / rock layer  (unchanged from tuft generator)
# ═══════════════════════════════════════════════════════════════════════════════

def _draw_bump(canvas, cx, cy, r, h):
    size = canvas.shape[0]
    pad  = int(r) + 2
    x0, x1 = max(0, int(cx) - pad), min(size, int(cx) + pad + 1)
    y0, y1 = max(0, int(cy) - pad), min(size, int(cy) + pad + 1)
    if x0 >= x1 or y0 >= y1:
        return
    gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
    dist   = np.sqrt((gx - cx) ** 2 + (gy - cy) ** 2)
    bump   = np.where(dist <= r, h * (1.0 - (dist / r) ** 2), 0.0)
    canvas[y0:y1, x0:x1] = np.maximum(canvas[y0:y1, x0:x1], bump)


def _draw_irregular_bump(canvas, cx, cy, r_base, h, rng, smoothness=0.0):
    size = canvas.shape[0]
    pad  = int(r_base * 1.8) + 3
    x0, x1 = max(0, int(cx) - pad), min(size, int(cx) + pad + 1)
    y0, y1 = max(0, int(cy) - pad), min(size, int(cy) + pad + 1)
    if x0 >= x1 or y0 >= y1:
        return
    gx, gy  = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
    dx      = gx - cx
    dy      = gy - cy
    aspect  = rng.uniform(0.55, 0.92)
    rot     = rng.uniform(0, 2 * math.pi)
    dx_r    =  dx * math.cos(rot) + dy * math.sin(rot)
    dy_r    = -dx * math.sin(rot) + dy * math.cos(rot)
    dist_e  = np.sqrt(dx_r**2 + (dy_r / aspect)**2)
    angle_r = np.arctan2(dy_r, dx_r)
    if smoothness >= 0.25:
        ph1     = rng.uniform(0, 2 * math.pi)
        a1      = rng.uniform(0.0, 0.18) if rng.random() < 0.6 else 0.0
        ph2     = rng.uniform(0, 2 * math.pi)
        a2      = rng.uniform(0.01, 0.03)
        r_local = r_base * (1.0 + a1 * np.sin(1 * angle_r + ph1)
                                 + a2 * np.sin(2 * angle_r + ph2))
    else:
        n1, ph1, a1 = rng.randint(2, 5), rng.uniform(0, 2*math.pi), rng.uniform(0.06, 0.15)
        n2, ph2, a2 = rng.randint(5, 9), rng.uniform(0, 2*math.pi), rng.uniform(0.02, 0.07)
        r_local = r_base * (1.0 + a1 * np.sin(n1 * angle_r + ph1)
                                 + a2 * np.sin(n2 * angle_r + ph2))
    ratio  = np.where(r_local > 0, dist_e / r_local, 1.0)
    inside = ratio <= 1.0
    bump   = h * (0.65 + 0.35 * (1.0 - ratio ** 1.5))
    canvas[y0:y1, x0:x1] = np.where(inside, bump, canvas[y0:y1, x0:x1])


def generate_dirt(size, rng):
    sf     = size / 512
    canvas = np.zeros((size, size), dtype=float)
    cell   = 4.0 * sf
    gcols  = int(math.ceil(size / cell)) + 2
    grows  = int(math.ceil(size / cell)) + 2
    for row in range(-1, grows):
        for col in range(-1, gcols):
            cx = (col + rng.uniform(0.05, 0.95)) * cell
            cy = (row + rng.uniform(0.05, 0.95)) * cell
            r  = rng.uniform(2.2, 4.5) * sf
            h  = rng.uniform(0.30, 0.90) * DIRT_MAX
            _draw_bump(canvas, cx, cy, r, h)
    r_min = 4.5  * sf
    r_max = 44.0 * sf
    rocks = []
    for _ in range(ROCK_ATTEMPTS):
        u  = rng.random()
        r  = r_min * (r_max / r_min) ** (u ** ROCK_POWER)
        cx = rng.uniform(0, size)
        cy = rng.uniform(0, size)
        t  = (r - r_min) / (r_max - r_min)
        h  = rng.uniform(0.55 + 0.85 * t, 0.75 + 0.65 * t) * DIRT_MAX
        rocks.append((r, cx, cy, h))
    rocks.sort(key=lambda x: x[0])
    for r, cx, cy, h in rocks:
        t = (r - r_min) / (r_max - r_min)
        if r >= 6.0 * sf:
            pad = int(r * 1.8) + 3
            if cx - pad < 0 or cx + pad >= size or cy - pad < 0 or cy + pad >= size:
                continue
            _draw_irregular_bump(canvas, cx, cy, r, h, rng, smoothness=t)
        else:
            _draw_bump(canvas, cx, cy, r, h)
    return canvas


# ═══════════════════════════════════════════════════════════════════════════════
# Main generator
# ═══════════════════════════════════════════════════════════════════════════════

def generate(size, seed, detail_scale=1.0, dirt_only=False):
    rng = random.Random(seed)
    S   = size
    sf  = S / 512               # resolution scale factor
    sp  = sf * detail_scale     # combined spatial scale

    bw  = BLADE_BASE_W * sp     # blade base width, scaled

    # ── Dirt / rock base ──────────────────────────────────────────────────────
    canvas = generate_dirt(S, rng)
    if dirt_only:
        lo, hi = canvas.min(), canvas.max()
        return ((canvas - lo) / (hi - lo + 1e-9) * 255).astype(np.uint8)

    # ── Flow field ────────────────────────────────────────────────────────────
    field, field_type = make_flow_field(S, seed, rng)
    print(f"  field: {field_type}")

    # ── Trace streamlines ─────────────────────────────────────────────────────
    streamlines = collect_streamlines(field, S, rng)
    print(f"  streamlines: {len(streamlines)} (incl. branches)")

    # ── Place blades along streamlines ────────────────────────────────────────
    blade_h    = make_blade_h()
    all_blades = []

    for pts in streamlines:
        layer_offset = rng.uniform(0, LAYER_RANGE)
        for step_i, (x, y, dx, dy) in enumerate(pts):
            if step_i % BLADE_SPACING != 0:
                continue
            length = rng.uniform(BLADE_LENGTH_MIN, BLADE_LENGTH_MAX) * S * detail_scale
            blade  = make_blade_from_flow(x, y, dx, dy, length, bw, S, rng)
            if blade is not None:
                all_blades.append((*blade, layer_offset))

    print(f"  blades: {len(all_blades)}")

    rng.shuffle(all_blades)
    for bx_pts, by_pts, layer_offset in all_blades:
        draw_blade(canvas, bx_pts, by_pts, bw, blade_h, layer_offset)

    lo, hi = canvas.min(), canvas.max()
    return ((canvas - lo) / (hi - lo + 1e-9) * 255).astype(np.uint8)


def main():
    parser = argparse.ArgumentParser(description="Procedural grass heightmap (flow-field)")
    here        = os.path.dirname(os.path.abspath(__file__))
    default_out = os.path.join(here, "..", "textures", "grass-flowfield.png")
    parser.add_argument("output",      nargs="?",      default=default_out)
    parser.add_argument("--seed",      type=int,        default=42)
    parser.add_argument("--size",      type=int,        default=DEFAULT_SIZE)
    parser.add_argument("--scale",     type=float,      default=1.0,
                        help=">1 = bigger/fewer blades, <1 = smaller/more (default 1.0)")
    parser.add_argument("--dirt-only", action="store_true")
    args = parser.parse_args()

    label = "dirt-only " if args.dirt_only else ""
    print(f"Generating {args.size}×{args.size} {label}flow-field grass heightmap "
          f"(seed={args.seed}, scale={args.scale})...")

    img_arr = generate(args.size, args.seed,
                       detail_scale=args.scale, dirt_only=args.dirt_only)
    Image.fromarray(img_arr).save(args.output)
    print(f"  saved → {args.output}")


if __name__ == "__main__":
    main()
