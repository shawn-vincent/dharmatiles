#!/usr/bin/env python3
"""
Procedural grass heightmap generator.
Produces a grayscale PNG suitable for use with DungeonBlocks surface().

Layers (bottom to top):
  1. Edge    — short blades ringing the perimeter (fixed low height, taper to edges)
  2. Blades  — randomly ordered interior blades; each successive blade gets a higher
               layer_base, so overlapping blades show visual separation. A darkening
               rim shadow is cast just outside each blade's edge to outline it without
               covering what's beneath.

Usage:
  python3 generate-grass-heightmap.py [output.png] [--seed N] [--size N]
"""

import argparse
import math
import os
import random
import numpy as np
from PIL import Image

# ── Tuneable constants ────────────────────────────────────────────────────────

SIZE = 512

CLUMP_COUNT      = 120
BLADES_PER_CLUMP = 22
CLUMP_RADIUS     = 28
BLADE_BASE_W     = 6.0
BLADE_LENGTH_MIN = 40
BLADE_LENGTH_MAX = 85

# Scatter fill: extra individual blades dropped randomly to cover gaps
SCATTER_COUNT    = 400

# Dirt rock distribution (power-law: r = r_min * (r_max/r_min)^(u^ROCK_POWER))
ROCK_ATTEMPTS    = 120    # total random draws; most yield tiny pebbles
ROCK_POWER       = 3.5    # higher = stronger small-rock dominance

# Height values (all relative; final image is normalized to 0–255)
DIRT_MAX         = 0.32   # dirt layer ceiling
GRASS_BOTTOM     = 0.36   # all grass starts here — always above dirt so blades are opaque
EDGE_BLADE_ARC   = 0.10   # edge blades are short; GRASS_BOTTOM + this keeps them low
BLADE_ARC_HEIGHT = 0.20
LAYER_STACK_H    = 0.75

# Rim shadow cast just outside each blade
RIM_DARKEN       = 0.05
RIM_WIDTH_FACTOR = 1.8

# Edge fill — perimeter walk
EDGE_STRIP_W = 38
EDGE_SPACING = 5

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


def draw_blade(canvas, cx_pts, cy_pts, base_width, layer_base, arc_height, rim_darken=0.0):
    """Draw one blade: rim shadow first (darkens what's outside the blade), then fill."""
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
        h_along = layer_base + arc_height * math.sin(math.pi * t)

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

        # Blade fill: raise interior pixels to this blade's height
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
    """Irregular rock/pebble: elliptical base with angular wobble.

    smoothness=0  →  small rocks: two harmonics, 2–5 lobes (lumpy)
    smoothness=1  →  large rocks: at most one protuberance on an ellipsoid

    Uses direct stamping so rocks fully cover grit and smaller rocks beneath.
    """
    size = canvas.shape[0]
    pad  = int(r_base * 1.8) + 3
    x0, x1 = max(0, int(cx) - pad), min(size, int(cx) + pad + 1)
    y0, y1 = max(0, int(cy) - pad), min(size, int(cy) + pad + 1)
    if x0 >= x1 or y0 >= y1:
        return

    gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
    dx = gx - cx
    dy = gy - cy

    # Elliptical base: random aspect ratio and orientation
    aspect = rng.uniform(0.55, 0.92)
    rot    = rng.uniform(0, 2 * math.pi)
    dx_r   =  dx * math.cos(rot) + dy * math.sin(rot)
    dy_r   = -dx * math.sin(rot) + dy * math.cos(rot)
    dist_e = np.sqrt(dx_r ** 2 + (dy_r / aspect) ** 2)

    angle_r = np.arctan2(dy_r, dx_r)

    if smoothness >= 0.25:
        # Large rocks: smooth ellipsoid, optionally one protuberance (n=1)
        # n=1 gives a single asymmetric bulge on one side — never multi-lobed.
        ph1 = rng.uniform(0, 2 * math.pi)
        a1  = rng.uniform(0.0, 0.18) if rng.random() < 0.6 else 0.0
        # Barely-there fine texture so it doesn't look computer-perfect
        ph2 = rng.uniform(0, 2 * math.pi)
        a2  = rng.uniform(0.01, 0.03)
        r_local = r_base * (1.0 + a1 * np.sin(1 * angle_r + ph1)
                                 + a2 * np.sin(2 * angle_r + ph2))
    else:
        # Small rocks: two harmonics, more irregular
        n1, ph1, a1 = rng.randint(2, 5), rng.uniform(0, 2*math.pi), rng.uniform(0.06, 0.15)
        n2, ph2, a2 = rng.randint(5, 9), rng.uniform(0, 2*math.pi), rng.uniform(0.02, 0.07)
        r_local = r_base * (1.0 + a1 * np.sin(n1 * angle_r + ph1)
                                 + a2 * np.sin(n2 * angle_r + ph2))

    ratio  = np.where(r_local > 0, dist_e / r_local, 1.0)
    inside = ratio <= 1.0
    # Flat-topped: stays high across the rock, slight rounding, sharp edge
    bump   = h * (0.65 + 0.35 * (1.0 - ratio ** 1.5))
    # Direct stamp: last painter wins — big rocks fully cover grit and smaller rocks.
    canvas[y0:y1, x0:x1] = np.where(inside, bump, canvas[y0:y1, x0:x1])


def generate_dirt(size, rng):
    """Multi-scale bumps for a rough dirt texture.

    Fine grit covers the whole tile via a jittered grid (tiny partial bumps at
    the very edge are imperceptible).  Rocks from pebble to boulder are drawn
    with a single power-law distribution — r = r_min * (r_max/r_min)^(u^power)
    — so small rocks overwhelmingly dominate while large ones are rare.  Any
    rock whose bounding box would overlap the tile edge is silently skipped.
    Rocks are drawn smallest-first so larger rocks always stamp over smaller ones.
    """
    sf     = size / 512
    canvas = np.zeros((size, size), dtype=float)

    # ── Fine grit: jittered grid for full base coverage ──
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

    # ── Rocks: single power-law distribution from pebble to boulder ──
    # r = r_min * (r_max/r_min)^(u^ROCK_POWER), u ~ Uniform(0,1)
    # High ROCK_POWER skews heavily toward small; every rock attempt yields
    # a size somewhere in the continuous range — no discrete tiers.
    r_min = 4.5  * sf
    r_max = 44.0 * sf
    # Grit bumps reach at most 0.90 * DIRT_MAX at their centres.
    # Rock height must always exceed that so stamping never creates dark holes.
    # We scale h with rock size so larger rocks are visibly taller.
    #   t=0 (smallest pebble): h in [0.50, 0.70]  → interior min = 0.50*0.65 = 0.325 > grit max 0.288
    #   t=1 (largest boulder): h in [0.90, 1.00]  → interior min = 0.90*0.65 = 0.585
    # Heights are absolute (not scaled by DIRT_MAX); normalization handles the range.
    rocks = []
    for _ in range(ROCK_ATTEMPTS):
        u  = rng.random()
        r  = r_min * (r_max / r_min) ** (u ** ROCK_POWER)
        cx = rng.uniform(0, size)
        cy = rng.uniform(0, size)
        t  = (r - r_min) / (r_max - r_min)   # 0 = tiny pebble, 1 = giant boulder
        # Height scales with size.  At t=1 the edge value (h*0.65) just clears
        # grit max (0.90*DIRT_MAX), so large rocks never stamp darker than grit.
        # Small pebbles stay in the old brightness range; they're too small to matter.
        h  = rng.uniform(0.55 + 0.85 * t, 0.75 + 0.65 * t) * DIRT_MAX
        rocks.append((r, cx, cy, h))

    rocks.sort(key=lambda x: x[0])   # smallest first → big rocks render on top

    for r, cx, cy, h in rocks:
        if r >= 6.0 * sf:
            # Irregular shape — skip entirely if it would overlap the edge
            pad = int(r * 1.8) + 3
            if cx - pad < 0 or cx + pad >= size or cy - pad < 0 or cy + pad >= size:
                continue
            _draw_irregular_bump(canvas, cx, cy, r, h, rng, smoothness=t)
        else:
            # Small enough that smooth dome + natural edge clamping is fine
            _draw_bump(canvas, cx, cy, r, h)

    return canvas


# ── Main ──────────────────────────────────────────────────────────────────────

def generate(size, seed, dirt_only=False):
    rng = random.Random(seed)
    S = size
    scale_f = S / 512
    bw = BLADE_BASE_W * scale_f

    # Dirt base — fills every gap between grass blades
    canvas = generate_dirt(S, rng)

    if dirt_only:
        lo, hi = canvas.min(), canvas.max()
        composite = (canvas - lo) / (hi - lo + 1e-9)
        return (composite * 255).astype(np.uint8)

    # 1. Edge blades — perimeter walk, just above dirt level
    strip_w = EDGE_STRIP_W * scale_f
    step    = EDGE_SPACING * scale_f
    perimeter = 4 * S
    pos = rng.uniform(0, step)

    while pos < perimeter:
        jit = rng.uniform(-step * 0.2, step * 0.2)
        p   = pos + jit
        pos += step

        depth = rng.uniform(1, strip_w)

        if p < S:
            x = float(np.clip(p, 1, S - 2))
            y = depth
        elif p < 2 * S:
            y = float(np.clip(p - S, 1, S - 2))
            x = float(S - 1 - depth)
        elif p < 3 * S:
            x = float(np.clip(3 * S - 1 - p, 1, S - 2))
            y = float(S - 1 - depth)
        else:
            y = float(np.clip(4 * S - 1 - p, 1, S - 2))
            x = depth

        blade_dir = rng.uniform(0, 2 * math.pi)
        length    = rng.uniform(BLADE_LENGTH_MIN, BLADE_LENGTH_MAX) * scale_f

        ctrl_fwd = length * rng.uniform(0.30, 0.60)
        ctrl_lat = length * rng.gauss(0, 0.08)
        perp_dir = blade_dir + math.pi / 2

        p1x = x + ctrl_fwd * math.cos(blade_dir) + ctrl_lat * math.cos(perp_dir)
        p1y = y + ctrl_fwd * math.sin(blade_dir) + ctrl_lat * math.sin(perp_dir)
        p2x = x + length * math.cos(blade_dir)
        p2y = y + length * math.sin(blade_dir)

        bx_pts, by_pts = quadratic_bezier((x, y), (p1x, p1y), (p2x, p2y))

        dx, dy = 0.0, 0.0
        if   bx_pts.min() < 0:      dx =  -bx_pts.min()
        elif bx_pts.max() > S - 1:  dx = (S - 1) - bx_pts.max()
        if   by_pts.min() < 0:      dy =  -by_pts.min()
        elif by_pts.max() > S - 1:  dy = (S - 1) - by_pts.max()
        bx_pts += dx
        by_pts += dy

        draw_blade(canvas, bx_pts, by_pts, bw, GRASS_BOTTOM, EDGE_BLADE_ARC, RIM_DARKEN)

    # 2. Interior blades — collect all geometries, shuffle, then draw with
    #    increasing layer_base so later-drawn blades are always higher.
    clump_centers = jittered_grid(CLUMP_COUNT, 0, S, rng)
    all_blades = []

    for (bx, by) in clump_centers:
        for _ in range(BLADES_PER_CLUMP):
            spread_r  = abs(rng.gauss(0, CLUMP_RADIUS * scale_f))
            base_ang  = rng.uniform(0, 2 * math.pi)
            p0x = bx + spread_r * math.cos(base_ang)
            p0y = by + spread_r * math.sin(base_ang)

            blade_dir = math.atan2(p0y - by, p0x - bx) + rng.gauss(0, 0.7)
            length    = rng.uniform(BLADE_LENGTH_MIN, BLADE_LENGTH_MAX) * scale_f

            ctrl_fwd = length * rng.uniform(0.30, 0.60)
            ctrl_lat = length * rng.gauss(0, 0.08)
            perp     = blade_dir + math.pi / 2
            p1x = p0x + ctrl_fwd * math.cos(blade_dir) + ctrl_lat * math.cos(perp)
            p1y = p0y + ctrl_fwd * math.sin(blade_dir) + ctrl_lat * math.sin(perp)
            p2x = p0x + length * math.cos(blade_dir)
            p2y = p0y + length * math.sin(blade_dir)

            bx_pts, by_pts = quadratic_bezier((p0x, p0y), (p1x, p1y), (p2x, p2y))

            if (bx_pts.min() < bw or bx_pts.max() > S - bw or
                    by_pts.min() < bw or by_pts.max() > S - bw):
                continue

            all_blades.append((bx_pts, by_pts))

    # 3. Scatter fill — random individual blades across the whole tile to fill bare patches
    for _ in range(SCATTER_COUNT):
        p0x = rng.uniform(bw, S - bw)
        p0y = rng.uniform(bw, S - bw)
        blade_dir = rng.uniform(0, 2 * math.pi)
        length    = rng.uniform(BLADE_LENGTH_MIN * 0.7, BLADE_LENGTH_MAX * 0.85) * scale_f
        ctrl_fwd  = length * rng.uniform(0.30, 0.60)
        ctrl_lat  = length * rng.gauss(0, 0.08)
        perp      = blade_dir + math.pi / 2
        p1x = p0x + ctrl_fwd * math.cos(blade_dir) + ctrl_lat * math.cos(perp)
        p1y = p0y + ctrl_fwd * math.sin(blade_dir) + ctrl_lat * math.sin(perp)
        p2x = p0x + length * math.cos(blade_dir)
        p2y = p0y + length * math.sin(blade_dir)
        bx_pts, by_pts = quadratic_bezier((p0x, p0y), (p1x, p1y), (p2x, p2y))
        if (bx_pts.min() < bw or bx_pts.max() > S - bw or
                by_pts.min() < bw or by_pts.max() > S - bw):
            continue
        all_blades.append((bx_pts, by_pts))

    rng.shuffle(all_blades)
    total = len(all_blades)
    for i, (bx_pts, by_pts) in enumerate(all_blades):
        layer_base = GRASS_BOTTOM + LAYER_STACK_H * (i / max(total - 1, 1))
        draw_blade(canvas, bx_pts, by_pts, bw, layer_base, BLADE_ARC_HEIGHT, RIM_DARKEN)

    lo, hi = canvas.min(), canvas.max()
    composite = (canvas - lo) / (hi - lo + 1e-9)
    return (composite * 255).astype(np.uint8)


def main():
    parser = argparse.ArgumentParser(description="Procedural grass heightmap")
    here = os.path.dirname(os.path.abspath(__file__))
    default_out = os.path.join(here, "..", "textures", "grass-procedural.png")
    parser.add_argument("output", nargs="?", default=default_out)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--size", type=int, default=SIZE)
    parser.add_argument("--dirt-only", action="store_true", help="Output only the dirt layer, no grass blades")
    args = parser.parse_args()

    label = "dirt-only " if args.dirt_only else ""
    print(f"Generating {args.size}×{args.size} {label}grass heightmap (seed={args.seed})...")
    img_arr = generate(args.size, args.seed, dirt_only=args.dirt_only)
    Image.fromarray(img_arr).save(args.output)
    print(f"Saved {args.output}")


if __name__ == "__main__":
    main()
