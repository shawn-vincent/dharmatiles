#!/usr/bin/env python3
"""
Process a heightmap texture for DungeonBlocks tiles.
  - Crops to the top-left portion (crop_fraction controls how much of the image to use)
  - Applies a cosine fade to all four edges so height drops to zero at tile boundaries

Usage:
  python3 process-texture.py <input.png> <output.png> [crop_fraction] [fade_fraction]

  crop_fraction  fraction of image to crop to (0.5 = top-left quarter)  default: 0.5
  fade_fraction  fraction of tile width/height used for the fade ramp    default: 0.15
"""

import sys
import numpy as np
from PIL import Image


def process_texture(input_path, output_path, crop_fraction=0.5, fade_fraction=0.15):
    img = Image.open(input_path).convert("L")
    arr = np.array(img, dtype=float)
    h, w = arr.shape

    # Crop to the desired region
    ch = int(h * crop_fraction)
    cw = int(w * crop_fraction)
    arr = arr[:ch, :cw]
    h, w = arr.shape

    # Cosine ramp: 0 at edge, 1 at interior
    def cosine_ramp(n, fade_n):
        mask = np.ones(n)
        for i in range(fade_n):
            v = 0.5 - 0.5 * np.cos(np.pi * i / fade_n)
            mask[i] = v
            mask[n - 1 - i] = v
        return mask

    fade_px = max(1, int(min(w, h) * fade_fraction))
    mask = np.outer(cosine_ramp(h, fade_px), cosine_ramp(w, fade_px))
    arr = arr * mask

    Image.fromarray(arr.astype(np.uint8)).save(output_path)
    print(f"Saved {output_path} ({w}x{h})")


if __name__ == "__main__":
    args = sys.argv[1:]
    input_path    = args[0] if len(args) > 0 else "../textures/grass-foliage-256.png"
    output_path   = args[1] if len(args) > 1 else "../textures/grass-foliage-256-faded.png"
    crop_fraction = float(args[2]) if len(args) > 2 else 0.5
    fade_fraction = float(args[3]) if len(args) > 3 else 0.15
    process_texture(input_path, output_path, crop_fraction, fade_fraction)
