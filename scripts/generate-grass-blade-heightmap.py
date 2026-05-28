#!/usr/bin/env python3
"""
Single grass blade heightmap.

Cross-section:  half-cylinder  →  sqrt(r² − d_perp²)   (fades to 0 at both edges)
Base:           smooth fade from black via smoothstep
Tip:            geometric cone — radius tapers via cosine curve (organic, smooth join)
                sqrt(r(s)² − d_perp²) IS the correct upper-surface of a right circular
                cone; the cosine taper just makes the join with the body smooth.

Uses a KDTree nearest-spine-point lookup so each pixel only ever sees ONE spine
slice — this eliminates the brightness bleed at the outer edge of a banana curve.
"""

import numpy as np
from PIL import Image
from scipy.spatial import cKDTree
import argparse, pathlib

# ── Parameters ────────────────────────────────────────────────────────────────
W, H        = 256, 512
BASE_R      = 18          # cylinder radius (px)
BLADE_H     = 380         # world height — drives spine z, not encoded in output
CURL_X      = 65          # tip x-offset (curls right)
LEAN_Y      = 80          # tip y-offset (leans toward image-top)
TIP_FRAC    = 0.30        # fraction of spine length that becomes the cone tip
BASE_FADE_T = 0.28        # fraction over which the base fades from black
N_SPINE     = 8000

# ── Spine ─────────────────────────────────────────────────────────────────────
def make_cp(w, h):
    bx, by = w // 2, h - 65
    return np.array([
        [bx,               by,                   0          ],
        [bx,               by - LEAN_Y * 0.12,   BLADE_H*0.50],
        [bx + CURL_X*0.45, by - LEAN_Y * 0.72,   BLADE_H*0.88],
        [bx + CURL_X,      by - LEAN_Y,           BLADE_H    ],
    ], dtype=float)

def cubic_bezier(CP, t):
    t = t[:, None]
    return ((1-t)**3*CP[0] + 3*(1-t)**2*t*CP[1]
            + 3*(1-t)*t**2*CP[2] + t**3*CP[3])

# ── Main ──────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("output", nargs="?", default="grass_blade_heightmap.png")
    args = ap.parse_args()

    t      = np.linspace(0.0, 1.0, N_SPINE)
    spine  = cubic_bezier(make_cp(W, H), t)   # (N, 3)

    # ── Radius profile ────────────────────────────────────────────────────────
    # Body: constant.  Tip: cosine taper → smooth (zero-slope) join at junction,
    # converges to 0 at apex.  sqrt(r(s)²−d²) is the exact upper surface of a
    # right-circular cone, so we get a geometrically correct cone for free.
    tip_start  = 1.0 - TIP_FRAC
    s_tip      = np.clip((t - tip_start) / TIP_FRAC, 0.0, 1.0)
    radii      = BASE_R * np.where(t <= tip_start, 1.0, np.cos(s_tip * np.pi / 2.0))

    # ── Spine tangents & per-point normals (image-plane, x-y) ─────────────────
    tang         = np.zeros((N_SPINE, 2))
    tang[1:-1]   = spine[2:, :2] - spine[:-2, :2]
    tang[0]      = spine[1, :2]  - spine[0,  :2]
    tang[-1]     = spine[-1,:2]  - spine[-2, :2]
    tang        /= np.linalg.norm(tang, axis=1, keepdims=True).clip(1e-9)
    # perpendicular: rotate 90°  (tx,ty) → (−ty, tx)
    norms        = np.column_stack([-tang[:, 1], tang[:, 0]])

    # ── KDTree: each pixel gets exactly one (nearest) spine slice ─────────────
    # This prevents the outside-of-curve brightness bleed seen with slice splatting.
    tree = cKDTree(spine[:, :2])

    PX, PY   = np.meshgrid(np.arange(W, dtype=float), np.arange(H, dtype=float))
    pixels   = np.column_stack([PX.ravel(), PY.ravel()])

    print(f"KDTree query ({W}×{H} pixels, {N_SPINE} spine pts) …")
    _, idx   = tree.query(pixels)          # (H*W,)

    # Per-pixel values from nearest spine point
    r_px  = radii[idx]
    t_px  = t[idx]
    nx    = norms[idx, 0];  ny = norms[idx, 1]

    # Perpendicular offset across the blade (the ONLY distance that drives the profile)
    dx = pixels[:, 0] - spine[idx, 0]
    dy = pixels[:, 1] - spine[idx, 1]
    d_perp = dx * nx + dy * ny

    # ── Half-cylinder / cone surface profile ──────────────────────────────────
    # sqrt(r² − d_perp²)  gives the exact top surface of a right circular
    # cylinder (body) or cone (tip, r shrinking).  Naturally 0 at d_perp = r.
    inside  = (np.abs(d_perp) < r_px) & (r_px > 0.01)
    profile = np.where(inside, np.sqrt(np.maximum(0.0, r_px*r_px - d_perp*d_perp)), 0.0)

    # ── Base fade: smooth emergence from ground ───────────────────────────────
    f     = np.clip(t_px / BASE_FADE_T, 0.0, 1.0)
    fade  = 3.0*f*f - 2.0*f*f*f          # smoothstep

    height = (profile * fade).reshape(H, W)

    # ── Normalise → 8-bit ─────────────────────────────────────────────────────
    lo, hi = height.min(), height.max()
    img    = ((height - lo) / (hi - lo) * 255).clip(0, 255).astype(np.uint8)

    out = pathlib.Path(args.output)
    Image.fromarray(img, mode="L").save(out)
    print(f"Saved {out}  ({W}×{H})  range {lo:.2f}–{hi:.2f}")

if __name__ == "__main__":
    main()
