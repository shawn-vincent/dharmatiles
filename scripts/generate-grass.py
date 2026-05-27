#!/usr/bin/env python3
"""
Generate 9 grass floor tile STLs, each from a freshly-generated procedural heightmap.
Output: stl/grass-1.stl through stl/grass-9.stl

Each tile gets its own unique seed so all 9 look distinct.  No rotation, no
random cropping, no edge fading — the procedural generator fills the tile
edge-to-edge naturally.
"""

import argparse
import importlib.util
import os
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
from PIL import Image, ImageFilter

OPENSCAD     = "/Applications/OpenSCAD.app/Contents/MacOS/openscad"
PROJECT_DIR  = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEXTURES_DIR = os.path.join(PROJECT_DIR, "textures")
STL_DIR      = os.path.join(PROJECT_DIR, "stl")
SCAD_FILE    = os.path.join(PROJECT_DIR, "scad", "dungeonblocks-blank.scad")

HEIGHTMAP_SIZE = 512   # procedural generator resolution
OUTPUT_SIZE    = 128   # OpenSCAD texture resolution (higher → heavier mesh)
BLUR_RADIUS    = 0.75  # light smoothing to soften pixel-step mesh edges
TEXTURE_DEPTH  = 2


def load_generator():
    """Import generate() from generate-grass-heightmap.py (hyphenated name)."""
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "generate-grass-heightmap.py")
    spec = importlib.util.spec_from_file_location("generate_grass_heightmap", path)
    mod  = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.generate


def render_one(i, seed, scale=1.0):
    generate = load_generator()

    tex_path = os.path.join(TEXTURES_DIR, f"grass-{i}.png")
    stl_path = os.path.join(STL_DIR,      f"grass-{i}.stl")

    # Fresh procedural heightmap for this tile
    img_arr = generate(HEIGHTMAP_SIZE, seed, detail_scale=scale)
    img = Image.fromarray(img_arr)

    # Resize to OpenSCAD resolution with bicubic smoothing
    img = img.resize((OUTPUT_SIZE, OUTPUT_SIZE), Image.BICUBIC)
    if BLUR_RADIUS > 0:
        img = img.filter(ImageFilter.GaussianBlur(radius=BLUR_RADIUS))
    img.save(tex_path)

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
    parser = argparse.ArgumentParser(
        description="Generate 9 grass floor tile STLs from procedural heightmaps")
    parser.add_argument("--base-seed", type=int, default=1,
                        help="Tile N uses seed base_seed+N-1 (default: 1)")
    parser.add_argument("--scale", type=float, default=1.0,
                        help="Blade detail scale passed to heightmap generator (default: 1.0)")
    args = parser.parse_args()

    os.makedirs(STL_DIR, exist_ok=True)

    jobs = [(i, args.base_seed + i - 1) for i in range(1, 10)]
    for i, seed in jobs:
        print(f"  grass-{i}: seed={seed}, scale={args.scale}")

    print(f"\nGenerating heightmaps and rendering {len(jobs)} tiles in parallel...")
    with ProcessPoolExecutor() as executor:
        futures = {executor.submit(render_one, i, seed, args.scale): i for i, seed in jobs}
        for future in as_completed(futures):
            i, ok, info = future.result()
            if ok:
                print(f"  [{i}/9] done → {info}")
            else:
                print(f"  [{i}/9] ERROR: {info[:200]}")
                sys.exit(1)


if __name__ == "__main__":
    main()
