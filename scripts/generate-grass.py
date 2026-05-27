#!/usr/bin/env python3
"""
Generate 9 grass floor tile STLs with random texture subsets and rotations.
Output: stl/grass-1.stl through stl/grass-9.stl
"""

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


def process_texture(output_path, rotation, crop_x, crop_y):
    img = Image.open(SOURCE_IMG).convert("L")
    if rotation:
        img = img.rotate(rotation, expand=True)

    iw, ih = img.size
    cw, ch = int(iw * CROP_FRACTION), int(ih * CROP_FRACTION)
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

    jobs = []
    for i in range(1, 10):
        rotation = random.choice([0, 90, 180, 270])
        rw = sh if rotation in (90, 270) else sw
        rh = sw if rotation in (90, 270) else sh
        cw, ch = int(rw * CROP_FRACTION), int(rh * CROP_FRACTION)
        crop_x = random.randint(0, rw - cw)
        crop_y = random.randint(0, rh - ch)
        jobs.append((i, rotation, crop_x, crop_y))
        print(f"  grass-{i}: rot={rotation} crop=({crop_x},{crop_y})")

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
