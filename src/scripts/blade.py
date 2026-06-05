#!/usr/bin/env python3
"""
Grass blade renderer — analytical SDF with circular-arc spine.

For every pixel in the bounding box we compute two scalars analytically:
  d_perp : perpendicular distance from the spine
  s      : arc-length from the base

From (d_perp, s) we derive alpha, cross-section shading, and fades.
Circular arcs are the only spine shape with an O(1) closed-form inverse
(just polar coords around the arc centre), so this is as fast as a scanline
approach while being correct at any curve angle.
"""
import numpy as np
from PIL import Image

CANVAS_W = 512
CANVAS_H = 512
OUTPUT   = "blade.png"


def render_blade(canvas_w=CANVAS_W, canvas_h=CANVAS_H,
                 width=20, length=200, cap_ratio=0.6, tip_length=40, power=0.5,
                 base_fade_length=0.50, tip_fade_length=0.25, tip_fade_height=0.50,
                 curve=0.0, curve_start=0.15, direction=0.0,
                 base_x=None, base_y=None, return_layers=False,
                 pointed_base=False):
    """
    Spine layout (arc-length s from base):
      [0,  b]          : semi-ellipse cap  (grows 0 → width/2)
      [b,  b+length]   : body              (constant width/2)
      [b+length, total]: tip               (cosine taper → 0)

    direction     : radians CW from straight-up.  Chord (base→tip) angle.
    curve         : ±1 → ±90° arc sweep, held at constant arc-length.
    base_x/base_y : explicit base position; default = canvas centre-bottom.
    return_layers : if True, return ((y0,y1,x0,x1), brightness, alpha) over
                    the bounding box only — for efficient scene compositing.
    """
    a  = width / 2.0
    b  = a * cap_ratio
    base_x = float(base_x) if base_x is not None else float(canvas_w // 2)
    base_y = float(base_y) if base_y is not None else float(canvas_h - 40) + b
    cx, bottom_y = base_x, base_y

    total_length  = tip_length + length + b
    s_curve_start = total_length * curve_start
    curve_span    = total_length - s_curve_start
    y_curve_start = bottom_y - s_curve_start   # row where arc begins (natural frame)

    base_fade_px = base_fade_length * total_length
    tip_fade_px  = tip_fade_length  * total_length
    s_body_end   = b + length

    # ── Arc geometry (natural frame — blade pointing straight up) ─────────────
    straight = abs(curve) < 0.005
    if straight:
        y_apex_nat  = y_curve_start - curve_span
        chord_angle = 0.0
        nat_x0 = int(cx - a)   - 1
        nat_x1 = int(cx + a)   + 2
    else:
        theta_max = abs(curve) * (np.pi / 2)
        R         = curve_span / theta_max      # arc-length = curve_span ✓
        sgn       = np.sign(curve)
        arc_cx    = cx + sgn * R                # centre of curvature
        arc_cy    = float(y_curve_start)
        lateral   = R * (1.0 - np.cos(theta_max))
        y_apex_nat = arc_cy - R * np.sin(theta_max)
        # Chord angle: CW from "up" in image = atan2(tip_dx, tip_dy_upward)
        chord_angle = np.arctan2(sgn * lateral, bottom_y - y_apex_nat)
        if sgn > 0:
            nat_x0 = int(cx - a)           - 1
            nat_x1 = int(cx + lateral + a) + 2
        else:
            nat_x0 = int(cx - lateral - a) - 1
            nat_x1 = int(cx + a)           + 2

    nat_y0 = int(y_apex_nat) - int(a) - 2
    nat_y1 = int(bottom_y)             + 2

    # ── Rotation: align chord with `direction` ────────────────────────────────
    # total_rot is the CW rotation applied to the whole blade.
    # Forward (natural → screen):  x' =  dx*cos(r) - dy*sin(r) + base_x
    #                               y' =  dx*sin(r) + dy*cos(r) + base_y
    # Inverse (screen → natural):  dx_n = dx*cos(r) + dy*sin(r)
    #                               dy_n = -dx*sin(r) + dy*cos(r)
    total_rot = direction - chord_angle
    use_rot   = abs(total_rot) > 0.001

    if use_rot:
        cos_r = np.cos(total_rot)
        sin_r = np.sin(total_rot)
        # Rotate natural bbox corners to find screen bbox
        cx_c = np.array([nat_x0 - base_x, nat_x1 - base_x,
                         nat_x0 - base_x, nat_x1 - base_x])
        cy_c = np.array([nat_y0 - base_y, nat_y0 - base_y,
                         nat_y1 - base_y, nat_y1 - base_y])
        sx = cx_c * cos_r - cy_c * sin_r + base_x
        sy = cx_c * sin_r + cy_c * cos_r + base_y
        x0 = max(0,        int(sx.min()) - 1)
        x1 = min(canvas_w, int(sx.max()) + 2)
        y0 = max(0,        int(sy.min()) - 1)
        y1 = min(canvas_h, int(sy.max()) + 2)
    else:
        x0 = max(0,        nat_x0)
        x1 = min(canvas_w, nat_x1)
        y0 = max(0,        nat_y0)
        y1 = min(canvas_h, nat_y1)

    # ── Pixel grid ────────────────────────────────────────────────────────────
    Y, X = np.mgrid[y0:y1, x0:x1].astype(float)

    # Map screen pixels back to natural (unrotated) frame for SDF evaluation
    if use_rot:
        dx = X - base_x;  dy = Y - base_y
        X_n = base_x + dx * cos_r + dy * sin_r
        Y_n = base_y - dx * sin_r + dy * cos_r
    else:
        X_n, Y_n = X, Y

    # ── (d_perp, s) ───────────────────────────────────────────────────────────
    # Straight section: spine at (cx, Y_n)
    d_str = X_n - cx
    s_str = bottom_y - Y_n

    if straight:
        d_perp = d_str
        s      = s_str
    else:
        # Circular-arc section
        # d_arc = r_px − R  (perpendicular distance; only |d| and d² are used)
        # Arc angle θ of nearest spine point:
        #   rightward (sgn +1): θ = atan2(arc_cy − Y_n,  arc_cx − X_n)
        #   leftward  (sgn −1): θ = atan2(arc_cy − Y_n,  X_n − arc_cx)
        #   unified:            θ = atan2(arc_cy − Y_n,  sgn*(arc_cx − X_n))
        dx_a  = X_n - arc_cx
        dy_a  = Y_n - arc_cy
        r_px  = np.sqrt(dx_a*dx_a + dy_a*dy_a)
        d_arc = r_px - R

        theta_px = np.arctan2(arc_cy - Y_n, sgn * (arc_cx - X_n))
        theta_px = np.clip(theta_px, 0.0, theta_max)
        s_arc    = s_curve_start + R * theta_px

        in_arc = (Y_n < y_curve_start)
        d_perp = np.where(in_arc, d_arc, d_str)
        s      = np.where(in_arc, s_arc, s_str)

    # ── Radius profile r(s) ───────────────────────────────────────────────────
    t_tip = np.clip((s - s_body_end) / tip_length, 0.0, 1.0)
    r_tip = a * np.cos(t_tip * np.pi / 2)

    if pointed_base:
        # Cosine taper over 2× tip_length pixels — double the tip so the base
        # reads as a long, sharp point emerging between blades.
        # b is still used for total_length / curve geometry etc.
        b_base = min(float(tip_length) * 3.0, float(s_body_end) - 2.0)
        s_cap_p = np.clip(s, 0.0, b_base)
        r_cap = a * np.cos((1.0 - s_cap_p / b_base) * np.pi / 2)
        r_s = np.where(s < b_base, r_cap,
              np.where(s < s_body_end, a, r_tip))
    else:
        # Semi-ellipse cap — blunt rounded base (default).
        s_cap = np.clip(s, 0.0, b)
        r_cap = a * np.sqrt(np.maximum(0.0, 1.0 - ((b - s_cap) / b) ** 2))
        r_s = np.where(s < b, r_cap,
              np.where(s < s_body_end, a, r_tip))

    # ── Alpha (anti-aliased edge) & shading ───────────────────────────────────
    abs_d = np.abs(d_perp)
    aa    = np.minimum(r_s, 0.5)        # margin shrinks to 0 at tip so no streak
    alpha = np.clip(r_s - abs_d + aa, 0.0, 1.0)
    d_norm = np.clip(d_perp / np.maximum(r_s, 1e-6), -1.0, 1.0)
    shade  = (1.0 - d_norm ** 2) ** power

    # ── Base & tip fades ──────────────────────────────────────────────────────
    t_base    = np.clip(s / base_fade_px, 0.0, 1.0)
    base_fade = 3*t_base**2 - 2*t_base**3

    # No brightness fade at the tip either — edge shading from `shade` alone.
    # (tip_fade_height used to floor brightness at 0.5, making tips darker than
    # pointed bases which have base_fade=1.0; remove for consistency.)
    tip_fade = 1.0

    brightness = shade * base_fade * tip_fade   # [0,1], alpha-separate

    if return_layers:
        # Return bbox + brightness/alpha over that bbox only.
        # Caller composites: canvas[y0:y1,x0:x1] = canvas[...] * (1-A) + B * A
        return (y0, y1, x0, x1), brightness, alpha

    # ── Composite (single-blade render) ──────────────────────────────────────
    canvas = np.zeros((canvas_h, canvas_w), dtype=np.uint8)
    canvas[y0:y1, x0:x1] = np.clip(
        brightness * alpha * 255, 0, 255
    ).astype(np.uint8)
    return canvas


def render_grid(curves=(-1.0, -0.5, 0.0, 0.5, 1.0)):
    cell_w, cell_h = 300, 380
    gap = 5
    n   = len(curves)

    grid = np.zeros((cell_h + 2*gap, n*(cell_w+gap)+gap), dtype=np.uint8)

    for i, c in enumerate(curves):
        x_off = gap + i * (cell_w + gap)
        cell  = render_blade(canvas_w=cell_w, canvas_h=cell_h,
                             width=18, length=200, tip_length=35, curve=c)
        grid[gap:gap+cell_h, x_off:x_off+cell_w] = cell

    return grid


if __name__ == "__main__":
    Image.fromarray(render_grid(), mode="L").save(OUTPUT)
    print(f"Saved {OUTPUT}")
