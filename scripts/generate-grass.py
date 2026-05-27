#!/usr/bin/env python3
"""
Generate 9 grass floor tile STLs with random texture subsets and rotations.
Output: stl/grass-1.stl through stl/grass-9.stl
"""

import math
import os
import random
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
import numpy as np
from PIL import Image, ImageFilter

OPENSCAD    = "/Applications/OpenSCAD.app/Contents/MacOS/openscad"
PROJECT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEXTURES_DIR = os.path.join(PROJECT_DIR, "textures")
STL_DIR      = os.path.join(PROJECT_DIR, "stl")
SCAD_FILE    = os.path.join(PROJECT_DIR, "scad", "dungeonblocks-blank.scad")
SOURCE_IMG   = os.path.join(TEXTURES_DIR, "grass-foliage.png")

CROP_FRACTION  = 0.5
FADE_FRACTION  = 0.20
FADE_FLOOR     = 0.40
BLUR_RADIUS    = 0.75
OUTPUT_SIZE    = 128
TEXTURE_DEPTH  = 4


def safe_crop_region(S, rotation_deg):
    """Return (offset_x, offset_y, safe_w, safe_h) of the axis-aligned region
    inside a rotation-expanded square image that contains no black fill corners."""
    theta = math.radians(rotation_deg % 90)
    if theta < 1e-9:
        return 0, 0, S, S
    cos_t, sin_t = math.cos(theta), math.sin(theta)
    bb   = S * (cos_t + sin_t)   # bounding box side after expand
    safe = S / (cos_t + sin_t)   # inscribed axis-aligned square side
    off  = (bb - safe) / 2
    return int(off), int(off), int(safe), int(safe)


def process_texture(output_path, rotation, crop_x, crop_y):
    src = Image.open(SOURCE_IMG)
    S = src.size[0]  # square source
    img = src.convert("L")
    if rotation:
        img = img.rotate(rotation, expand=True)

    _, _, safe_w, safe_h = safe_crop_region(S, rotation)
    cw, ch = int(safe_w * CROP_FRACTION), int(safe_h * CROP_FRACTION)
    img = img.crop((crop_x, crop_y, crop_x + cw, crop_y + ch))
    img = img.resize((OUTPUT_SIZE, OUTPUT_SIZE), Image.BICUBIC)
    if BLUR_RADIUS > 0:
        img = img.filter(ImageFilter.GaussianBlur(radius=BLUR_RADIUS))

    arr = np.array(img, dtype=float)
    h, w = arr.shape

    def edge_ramp(n, fade_n):
        mask = np.ones(n)
        for i in range(fade_n):
            t = i / fade_n
            v = FADE_FLOOR + (1.0 - FADE_FLOOR) * np.sin(np.pi / 2 * t)
            mask[i] = v
            mask[n - 1 - i] = v
        return mask

    fade_px = max(1, int(min(w, h) * FADE_FRACTION))
    arr *= np.outer(edge_ramp(h, fade_px), edge_ramp(w, fade_px))
    arr -= arr.min()

    Image.fromarray(arr.astype(np.uint8)).save(output_path)


def render_one(i, rotation, crop_x, crop_y):
    tex_path = os.path.join(TEXTURES_DIR, f"grass-{i}.png")
    stl_path = os.path.join(STL_DIR, f"grass-{i}.stl")
    process_texture(tex_path, rotation, crop_x, crop_y)
    result = subprocess.run([
        OPENSCAD, "--render", "-o", stl_path,
        "-D", f"texture_depth={TEXTURE_DEPTH}",
        "-D", f'texture_file="{tex_path}"',
        "-D", "floor_preset=6",
        "-D", "peg_height=5.7",
        SCAD_FILE
    ], capture_output=True, text=True)
    if result.returncode != 0:
        return i, False, result.stderr
    return i, True, stl_path


def main():
    os.makedirs(STL_DIR, exist_ok=True)

    src = Image.open(SOURCE_IMG)
    sw, sh = src.size

    assert sw == sh, "Source image must be square for arbitrary rotation"
    jobs = []
    for i in range(1, 10):
        rotation = random.uniform(0, 360)
        off_x, off_y, safe_w, safe_h = safe_crop_region(sw, rotation)
        cw = int(safe_w * CROP_FRACTION)
        ch = int(safe_h * CROP_FRACTION)
        crop_x = off_x + random.randint(0, safe_w - cw)
        crop_y = off_y + random.randint(0, safe_h - ch)
        jobs.append((i, rotation, crop_x, crop_y))
        print(f"  grass-{i}: rot={rotation:.1f}° crop=({crop_x},{crop_y})")

    print(f"\nRendering {len(jobs)} tiles in parallel...")
    with ProcessPoolExecutor() as executor:
        futures = {executor.submit(render_one, *job): job[0] for job in jobs}
        for future in as_completed(futures):
            i, ok, info = future.result()
            if ok:
                print(f"  [{i}/9] done → {info}")
            else:
                print(f"  [{i}/9] ERROR: {info}")
                sys.exit(1)


if __name__ == "__main__":
    main()
