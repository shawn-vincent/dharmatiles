#!/usr/bin/env python3
"""
Process a heightmap texture for DungeonBlocks tiles.
  - Crops to the top-left portion (crop_fraction controls how much of the image to use)
  - Upsamples to output_size using bicubic resampling for smooth geometry
  - Applies Gaussian blur to eliminate pixel-step blockiness
  - Applies a cosine fade to all four edges

Usage:
  python3 prepare-heightmap.py <input.png> <output.png> [crop_fraction] [fade_fraction] [fade_floor] [blur_radius] [output_size]

  crop_fraction  fraction of image to crop to (0.5 = top-left quarter)  default: 0.5
  fade_fraction  fraction of tile width/height used for the fade ramp    default: 0.15
  fade_floor     minimum value the fade reaches (0.0 = black, 0.1 = 10%) default: 0.0
  blur_radius    Gaussian blur radius in pixels to smooth mesh steps      default: 1.5
  output_size    pixel dimensions of output image                         default: 256
"""

import sys
import numpy as np
from PIL import Image, ImageFilter


def process_texture(input_path, output_path, crop_fraction=0.5, fade_fraction=0.15, fade_floor=0.0, blur_radius=1.5, output_size=256):
    img = Image.open(input_path).convert("L")
    arr = np.array(img, dtype=float)
    h, w = arr.shape

    # Crop to the desired region
    ch = int(h * crop_fraction)
    cw = int(w * crop_fraction)
    arr = arr[:ch, :cw]

    # Upsample to output_size using bicubic resampling for smooth geometry
    img_crop = Image.fromarray(arr.astype(np.uint8))
    img_crop = img_crop.resize((output_size, output_size), Image.BICUBIC)

    # Gaussian blur to smooth pixel-step edges
    if blur_radius > 0:
        img_crop = img_crop.filter(ImageFilter.GaussianBlur(radius=blur_radius))

    arr = np.array(img_crop, dtype=float)
    h, w = arr.shape

    # Ramp: fade_floor at edge, 1.0 at interior
    # convex=True: rises quickly then levels off (sine); False: rises slowly then fast (cosine)
    def edge_ramp(n, fade_n, convex=True):
        mask = np.ones(n)
        for i in range(fade_n):
            t = i / fade_n
            curve = np.sin(np.pi / 2 * t) if convex else (0.5 - 0.5 * np.cos(np.pi * t))
            v = fade_floor + (1.0 - fade_floor) * curve
            mask[i] = v
            mask[n - 1 - i] = v
        return mask

    fade_px = max(1, int(min(w, h) * fade_fraction))
    mask = np.outer(edge_ramp(h, fade_px), edge_ramp(w, fade_px))
    arr = arr * mask

    # Shift so the lowest point sits at zero — prevents surface from hovering above base
    arr = arr - arr.min()

    Image.fromarray(arr.astype(np.uint8)).save(output_path)
    print(f"Saved {output_path} ({w}x{h})")


if __name__ == "__main__":
    args = sys.argv[1:]
    input_path    = args[0] if len(args) > 0 else "../textures/grass-foliage-256.png"
    output_path   = args[1] if len(args) > 1 else "../textures/grass-foliage-256-faded.png"
    crop_fraction = float(args[2]) if len(args) > 2 else 0.5
    fade_fraction = float(args[3]) if len(args) > 3 else 0.15
    fade_floor    = float(args[4]) if len(args) > 4 else 0.0
    blur_radius   = float(args[5]) if len(args) > 5 else 1.5
    output_size   = int(args[6])   if len(args) > 6 else 128
    process_texture(input_path, output_path, crop_fraction, fade_fraction, fade_floor, blur_radius, output_size)
