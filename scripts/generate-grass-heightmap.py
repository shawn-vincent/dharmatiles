#!/usr/bin/env python3
"""
Procedural grass heightmap generator (tip-priority compositing).

Every blade's BASE is lower (in height) than any other blade's BODY, and every
blade's TIP is higher than any other blade's BODY.  This is guaranteed by using
a single, strictly-increasing height function of position-along-blade (t ∈ [0,1])
shared by ALL interior blades:

    blade_h(t) = GRASS_BOTTOM + t·LAYER_STACK_H + BLADE_ARC_HEIGHT·t·(1−t)

Because blade_h is strictly increasing (LAYER_STACK_H > BLADE_ARC_HEIGHT ensures
h′(t) > 0 everywhere) and every blade uses the same function, np.maximum
compositing guarantees: wherever two blades overlap, the one closer to its TIP
wins regardless of which blade it is.

Bases are always buried under crossing blades; tips always poke above them.
3-way (and N-way) cyclic overlaps are fully supported.

Blades that would clip the canvas edge are shifted (not discarded) so they
just touch the boundary.  This fills the tile edge-to-edge with no mechanical
perimeter strip.

Usage:
  python3 generate-grass-heightmap.py [output.png] [--seed N] [--size N]
  python3 generate-grass-heightmap.py [output.png] --dirt-only
"""

import argparse
import math
import os
import random
import numpy as np
from PIL import Image

# ── Tuneable constants ────────────────────────────────────────────────────────

SIZE = 512

TUFT_COUNT       = 50     # tuft base points, distributed via jittered grid
BLADE_BASE_W     = 12.0
BLADE_LENGTH_MIN = 120
BLADE_LENGTH_MAX = 255

# Dirt rock distribution (power-law: r = r_min * (r_max/r_min)^(u^ROCK_POWER))
ROCK_ATTEMPTS    = 120    # total random draws; most yield tiny pebbles
ROCK_POWER       = 3.5    # higher = stronger small-rock dominance

# Height values (all relative; final image is normalized to 0–255)
DIRT_MAX         = 0.32   # dirt layer ceiling
GRASS_BOTTOM     = 0.10   # blade base height — 0 = black, emerges from ground level

# Interior blade height profile — shared monotone function h(t):
#   h(t) = GRASS_BOTTOM + t·LAYER_STACK_H + BLADE_ARC_HEIGHT·t·(1−t)
# LAYER_STACK_H > BLADE_ARC_HEIGHT ensures h′(t) > 0 for all t, so the function
# is strictly increasing: base (t=0) is always lowest, tip (t=1) is always highest.
LAYER_STACK_H    = 0.75   # total height range from base to tip
BLADE_ARC_HEIGHT = 0.20   # small mid-blade bonus (quadratic bump, not a hump back down)

# Tuft: multiple blades from a single base point (clump + scatter blades only)
# Only applied when the primary blade fits without rotation (edge blades get 1).
TUFT_SPREAD      = 0.40   # std-dev in radians of per-sibling direction jitter (~23°)
TUFT_MIN         = 6
TUFT_WEIGHTS     = (0.45, 0.35, 0.20)  # P(6), P(7), P(8 blades)

# Rim shadow cast just outside each blade
RIM_DARKEN       = 0.05
RIM_WIDTH_FACTOR = 1.8

# ── Height functions ──────────────────────────────────────────────────────────

def make_blade_h():
    """Return the shared interior-blade height function.

    h(t) = GRASS_BOTTOM + t·LAYER_STACK_H + BLADE_ARC_HEIGHT·t·(1−t)

    Strictly increasing because h′(t) = LAYER_STACK_H + BLADE_ARC_HEIGHT·(1−2t)
    and at t=1 (the worst case): h′(1) = LAYER_STACK_H − BLADE_ARC_HEIGHT
                                        = 0.75 − 0.20 = 0.55 > 0.
    """
    def h(t):
        return GRASS_BOTTOM + t * LAYER_STACK_H + BLADE_ARC_HEIGHT * t * (1.0 - t)
    return h


# ── Helpers ───────────────────────────────────────────────────────────────────

def jittered_grid(n, lo, hi, rng):
    span = hi - lo
    cols = max(1, round(math.sqrt(n)))
    rows = max(1, math.ceil(n / cols))
    cw = span / cols
    rh = span / rows
    pts = []
    indices = list(range(cols * rows))
    rng.shuffle(indices)
    for k in indices[:n]:
        ci, ri = k % cols, k // cols
        x = lo + (ci + rng.uniform(0.15, 0.85)) * cw
        y = lo + (ri + rng.uniform(0.15, 0.85)) * rh
        pts.append((x, y))
    return pts


def quadratic_bezier(p0, p1, p2, n=80):
    t = np.linspace(0, 1, n)
    x = (1-t)**2 * p0[0] + 2*(1-t)*t * p1[0] + t**2 * p2[0]
    y = (1-t)**2 * p0[1] + 2*(1-t)*t * p1[1] + t**2 * p2[1]
    return x, y


def draw_blade(canvas, cx_pts, cy_pts, base_width, h_func, rim_darken=0.0):
    """Draw one blade using a shared height function h_func(t) → height.

    h_func must be monotonically increasing in t so that np.maximum compositing
    preserves the base-behind / tip-in-front depth ordering across all blades.

    Rim shadow (optional): darkens pixels just outside the blade edge to cast
    a subtle outline shadow.  Shadows are applied before the fill so later-drawn
    blade fills can overwrite them (painter's order for shadows only).
    """
    H, W = canvas.shape
    n = len(cx_pts)
    for i, (cx, cy) in enumerate(zip(cx_pts, cy_pts)):
        t = i / max(n - 1, 1)
        # Blade profile: sharp ramp at base, steady taper to pointed tip
        ramp = min(t / 0.06, 1.0)
        taper = (1.0 - t) ** 0.85
        w = base_width * ramp * taper
        if w < 0.25:
            continue

        # Height from shared monotone function — base of this blade is lower than
        # ANY other blade's body; tip is higher than ANY other blade's body.
        h_along = h_func(t)

        rim_w = w * RIM_WIDTH_FACTOR
        wi = int(rim_w) + 2
        x0, x1 = max(0, int(cx) - wi), min(W, int(cx) + wi + 1)
        y0, y1 = max(0, int(cy) - wi), min(H, int(cy) + wi + 1)
        if x0 >= x1 or y0 >= y1:
            continue

        gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
        dist = np.sqrt((gx - cx)**2 + (gy - cy)**2)

        # Shadow rim: darken pixels just outside the blade edge (not inside)
        if rim_darken > 0:
            rim_zone = (dist > w) & (dist < rim_w)
            rim_fade = np.clip(1.0 - (dist - w) / max(rim_w - w, 1e-6), 0.0, 1.0)
            canvas[y0:y1, x0:x1] = np.where(
                rim_zone,
                np.maximum(0.0, canvas[y0:y1, x0:x1] - rim_darken * rim_fade),
                canvas[y0:y1, x0:x1]
            )

        # Blade fill: raise interior pixels to this t-position's height.
        # np.maximum means: the pixel keeps whichever blade's t is higher —
        # i.e. the blade that is "closer to its tip" at this (x,y) wins.
        fill = np.where(dist <= w, h_along, 0.0)
        canvas[y0:y1, x0:x1] = np.maximum(canvas[y0:y1, x0:x1], fill)


# ── Dirt ──────────────────────────────────────────────────────────────────────

def _draw_bump(canvas, cx, cy, r, h):
    size = canvas.shape[0]
    pad = int(r) + 2
    x0, x1 = max(0, int(cx) - pad), min(size, int(cx) + pad + 1)
    y0, y1 = max(0, int(cy) - pad), min(size, int(cy) + pad + 1)
    if x0 >= x1 or y0 >= y1:
        return
    gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
    dist = np.sqrt((gx - cx) ** 2 + (gy - cy) ** 2)
    bump = np.where(dist <= r, h * (1.0 - (dist / r) ** 2), 0.0)
    canvas[y0:y1, x0:x1] = np.maximum(canvas[y0:y1, x0:x1], bump)


def _draw_irregular_bump(canvas, cx, cy, r_base, h, rng, smoothness=0.0):
    """Irregular rock/pebble: elliptical base with angular wobble."""
    size = canvas.shape[0]
    pad  = int(r_base * 1.8) + 3
    x0, x1 = max(0, int(cx) - pad), min(size, int(cx) + pad + 1)
    y0, y1 = max(0, int(cy) - pad), min(size, int(cy) + pad + 1)
    if x0 >= x1 or y0 >= y1:
        return

    gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
    dx = gx - cx
    dy = gy - cy

    aspect = rng.uniform(0.55, 0.92)
    rot    = rng.uniform(0, 2 * math.pi)
    dx_r   =  dx * math.cos(rot) + dy * math.sin(rot)
    dy_r   = -dx * math.sin(rot) + dy * math.cos(rot)
    dist_e = np.sqrt(dx_r ** 2 + (dy_r / aspect) ** 2)

    angle_r = np.arctan2(dy_r, dx_r)

    if smoothness >= 0.25:
        ph1 = rng.uniform(0, 2 * math.pi)
        a1  = rng.uniform(0.0, 0.18) if rng.random() < 0.6 else 0.0
        ph2 = rng.uniform(0, 2 * math.pi)
        a2  = rng.uniform(0.01, 0.03)
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
    """Multi-scale bumps for a rough dirt texture."""
    sf     = size / 512
    canvas = np.zeros((size, size), dtype=float)

    # Fine grit: jittered grid for full base coverage
    cell  = 4.0 * sf
    gcols = int(math.ceil(size / cell)) + 2
    grows = int(math.ceil(size / cell)) + 2
    for row in range(-1, grows):
        for col in range(-1, gcols):
            cx = (col + rng.uniform(0.05, 0.95)) * cell
            cy = (row + rng.uniform(0.05, 0.95)) * cell
            r  = rng.uniform(2.2, 4.5) * sf
            h  = rng.uniform(0.30, 0.90) * DIRT_MAX
            _draw_bump(canvas, cx, cy, r, h)

    # Rocks: single power-law distribution from pebble to boulder
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

    rocks.sort(key=lambda x: x[0])   # smallest first → big rocks render on top

    for r, cx, cy, h in rocks:
        if r >= 6.0 * sf:
            pad = int(r * 1.8) + 3
            if cx - pad < 0 or cx + pad >= size or cy - pad < 0 or cy + pad >= size:
                continue
            _draw_irregular_bump(canvas, cx, cy, r, h, rng, smoothness=t)
        else:
            _draw_bump(canvas, cx, cy, r, h)

    return canvas


# ── Main ──────────────────────────────────────────────────────────────────────

def blade_fits(bx_pts, by_pts, bw, S):
    """True if the blade centerline stays within [0, S).

    No margin: tips taper to zero width so they need no clearance, and bases
    are placed by jittered grid (not forced to the edge) so body clipping is rare.
    draw_blade clamps all pixel writes to [0, S) for any residual overflow.
    """
    return (bx_pts.min() >= 0 and bx_pts.max() <= S - 1 and
            by_pts.min() >= 0 and by_pts.max() <= S - 1)


def rotate_to_fit(bx_pts, by_pts, bw, S, rng, n_angles=360):
    """Find a valid orientation by scanning the full rotation space analytically.

    Rotates the blade template at n_angles evenly-spaced orientations in one
    vectorised numpy pass, finds which angles keep every bezier point within
    [0, S), then picks uniformly at random from the valid arc(s).

    Returns (bx_pts_rotated, by_pts_rotated) or None if no angle fits.
    """
    if blade_fits(bx_pts, by_pts, bw, S):
        return bx_pts, by_pts

    p0x, p0y = float(bx_pts[0]), float(by_pts[0])
    dx = bx_pts - p0x   # offsets from base, shape (n_pts,)
    dy = by_pts - p0y

    thetas  = np.linspace(0, 2 * math.pi, n_angles, endpoint=False)
    cos_t   = np.cos(thetas)[:, None]  # (n_angles, 1) — broadcasts over n_pts
    sin_t   = np.sin(thetas)[:, None]

    rx = dx * cos_t - dy * sin_t   # (n_angles, n_pts)
    ry = dx * sin_t + dy * cos_t

    valid = ((p0x + rx.min(axis=1) >= 0) &
             (p0x + rx.max(axis=1) <= S - 1) &
             (p0y + ry.min(axis=1) >= 0) &
             (p0y + ry.max(axis=1) <= S - 1))

    valid_idx = np.where(valid)[0]
    if len(valid_idx) == 0:
        return None

    # Pick the angle where the TIP ends up closest to any canvas edge.
    # This guarantees the blade points outward; maybe_reverse then needs
    # to do nothing for rotated blades (tip is already the edge-side).
    tip_x = p0x + rx[valid_idx, -1]
    tip_y = p0y + ry[valid_idx, -1]
    tip_edge_dist = np.minimum(np.minimum(tip_x, S - 1 - tip_x),
                               np.minimum(tip_y, S - 1 - tip_y))
    best = valid_idx[int(np.argmin(tip_edge_dist))]
    a = float(thetas[best])
    cos_a, sin_a = math.cos(a), math.sin(a)
    return (p0x + dx * cos_a - dy * sin_a,
            p0y + dx * sin_a + dy * cos_a)


def maybe_reverse(bx_pts, by_pts, bw, S):
    """Reverse the blade if its base is closer to any canvas edge than its tip.

    Compares the minimum distance-to-any-edge for the base vs the tip.
    If the base is the 'edge side', reverse so the tip points outward instead.
    No fixed threshold: works at any scale and naturally catches all edge-adjacent
    blades without affecting blades where the tip is already the outer end.
    """
    p0x, p0y = float(bx_pts[0]),  float(by_pts[0])   # base
    p1x, p1y = float(bx_pts[-1]), float(by_pts[-1])  # tip

    base_dist = min(p0x, S - 1 - p0x, p0y, S - 1 - p0y)
    tip_dist  = min(p1x, S - 1 - p1x, p1y, S - 1 - p1y)

    if base_dist <= tip_dist:
        return bx_pts[::-1], by_pts[::-1]
    return bx_pts, by_pts


def tuft_count(rng):
    """Return TUFT_MIN–(TUFT_MIN+len(TUFT_WEIGHTS)-1) blades per base."""
    r = rng.random()
    cumulative = 0.0
    for i, w in enumerate(TUFT_WEIGHTS):
        cumulative += w
        if r < cumulative:
            return TUFT_MIN + i
    return TUFT_MIN + len(TUFT_WEIGHTS) - 1


def make_blade_bezier(p0x, p0y, blade_dir, length, rng):
    """Build a quadratic bezier for one blade from its base, direction, and length."""
    ctrl_fwd = length * rng.uniform(0.30, 0.60)
    ctrl_lat = length * rng.gauss(0, 0.08)
    perp     = blade_dir + math.pi / 2
    p1x = p0x + ctrl_fwd * math.cos(blade_dir) + ctrl_lat * math.cos(perp)
    p1y = p0y + ctrl_fwd * math.sin(blade_dir) + ctrl_lat * math.sin(perp)
    p2x = p0x + length * math.cos(blade_dir)
    p2y = p0y + length * math.sin(blade_dir)
    return quadratic_bezier((p0x, p0y), (p1x, p1y), (p2x, p2y))


def blades_from_base(p0x, p0y, blade_dir, length, bw, S, rng):
    """Generate 1–3 blades from a single base point.

    If the primary blade fits without rotation: generate a tuft (1–3 blades
    in nearly the same direction, TUFT_SPREAD jitter between siblings).
    If the primary blade needs rotation to fit (edge case): generate exactly
    one rotated blade — no tuft, since the rotated direction is arbitrary.
    Sibling blades that fail the fit check are silently skipped.
    """
    bx_pts, by_pts = make_blade_bezier(p0x, p0y, blade_dir, length, rng)

    if not blade_fits(bx_pts, by_pts, bw, S):
        # Edge blade — rotate to fit, no tuft
        result = rotate_to_fit(bx_pts, by_pts, bw, S, rng)
        return [maybe_reverse(*result, bw, S)] if result is not None else []

    # Primary fits — all blades share p0x,p0y as base; no reversal.
    blades = [(bx_pts, by_pts)]
    for _ in range(tuft_count(rng) - 1):
        sib_dir = blade_dir + rng.gauss(0, TUFT_SPREAD)
        sib_len = length * rng.uniform(0.85, 1.15)
        sbx, sby = make_blade_bezier(p0x, p0y, sib_dir, sib_len, rng)
        if blade_fits(sbx, sby, bw, S):
            blades.append((sbx, sby))
    return blades


def generate(size, seed, detail_scale=1.0, dirt_only=False):
    rng = random.Random(seed)
    S = size
    scale_f = S / 512          # canvas-resolution scale (keeps proportions at any output size)
    sp = scale_f * detail_scale  # combined spatial scale applied to all feature sizes

    bw = BLADE_BASE_W * sp

    # Tuft count scales inversely with area so density stays constant at any scale.
    tuft_count_n = max(1, int(round(TUFT_COUNT / detail_scale ** 2)))

    # Dirt base — fills every gap between grass blades
    canvas = generate_dirt(S, rng)

    if dirt_only:
        lo, hi = canvas.min(), canvas.max()
        composite = (canvas - lo) / (hi - lo + 1e-9)
        return (composite * 255).astype(np.uint8)

    # Shared height function — strictly increasing in t
    blade_h = make_blade_h()

    # Tuft bases — jittered grid for even surface coverage, no clumping.
    # Each base spawns 3–5 blades in roughly the same direction.
    tuft_bases = jittered_grid(tuft_count_n, 0, S, rng)
    all_blades = []

    for (p0x, p0y) in tuft_bases:
        blade_dir = rng.uniform(0, 2 * math.pi)
        length    = rng.uniform(BLADE_LENGTH_MIN, BLADE_LENGTH_MAX) * sp
        all_blades.extend(blades_from_base(p0x, p0y, blade_dir, length, bw, S, rng))

    # Shuffle for rim-shadow draw order variety (no effect on blade heights)
    rng.shuffle(all_blades)

    for bx_pts, by_pts in all_blades:
        draw_blade(canvas, bx_pts, by_pts, bw, blade_h, RIM_DARKEN)

    lo, hi = canvas.min(), canvas.max()
    composite = (canvas - lo) / (hi - lo + 1e-9)
    return (composite * 255).astype(np.uint8)


def main():
    parser = argparse.ArgumentParser(description="Procedural grass heightmap (tip-priority)")
    here = os.path.dirname(os.path.abspath(__file__))
    default_out = os.path.join(here, "..", "textures", "grass-procedural.png")
    parser.add_argument("output", nargs="?", default=default_out)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--size", type=int, default=SIZE)
    parser.add_argument("--scale", type=float, default=1.0,
                        help="Detail scale: >1 = bigger/fewer blades, <1 = smaller/more blades (default 1.0 = large lush blades)")
    parser.add_argument("--dirt-only", action="store_true",
                        help="Output only the dirt layer, no grass blades")
    args = parser.parse_args()

    label = "dirt-only " if args.dirt_only else ""
    print(f"Generating {args.size}×{args.size} {label}grass heightmap "
          f"(seed={args.seed}, scale={args.scale})...")
    img_arr = generate(args.size, args.seed, detail_scale=args.scale, dirt_only=args.dirt_only)
    Image.fromarray(img_arr).save(args.output)
    print(f"Saved {args.output}")


if __name__ == "__main__":
    main()
