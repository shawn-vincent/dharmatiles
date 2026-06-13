#!/usr/bin/env python3
"""Render a tile spec to a PNG using pyrender (proper Z-buffer, smooth shading).

Usage:
    python src/extras/render_tile.py src/tiles/ground/soil+grass.tile.py
    python src/extras/render_tile.py src/tiles/water/water+grass.tile.py [OUTPUT.png]
    python src/extras/render_tile.py SPEC.tile.py --elev 45 --azim -135

OUTPUT defaults to /tmp/<spec-stem>.png.
"""
from __future__ import annotations

import argparse
import pathlib
import sys


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('spec',   type=pathlib.Path, help='.tile.py spec file')
    p.add_argument('output', type=pathlib.Path, nargs='?')
    p.add_argument('--elev',   type=float, default=45.0)
    p.add_argument('--azim',   type=float, default=-135.0)
    p.add_argument('--width',  type=int,   default=1200)
    p.add_argument('--height', type=int,   default=1000)
    args = p.parse_args()

    spec = args.spec
    if not spec.exists():
        print(f'not found: {spec}', file=sys.stderr)
        sys.exit(1)

    stem = spec.stem.removesuffix('.tile')
    out  = args.output or pathlib.Path('/tmp') / (stem + '.png')

    print(f'Building {spec.name} …')
    from dharmatiles.terrains.tile import build_meshes_for_render
    from dharmatiles.spec import load_spec
    meshes = build_meshes_for_render(spec)
    total  = sum(len(m.faces) for m in meshes)
    print(f'  {len(meshes)} mesh parts, {total:,} faces')

    tiles     = load_spec(spec)
    surface   = tiles[0].surface if tiles else None
    square_mm = surface.square_mm if surface else 35.0
    cols      = surface.cols if surface else 1
    rows      = surface.rows if surface else 1

    # Build a display label: "category/NxM-name" matching the batch PNG convention.
    spec_name = spec.stem.removesuffix('.tile')
    category  = spec.parent.name
    size_tag  = f"{cols}x{rows}"
    if category and category not in ('tiles',):
        label = f"{category}/{size_tag}-{spec_name}"
    else:
        label = f"{size_tag}-{spec_name}"

    from dharmatiles.render import render
    render(meshes, out, elev=args.elev, azim=args.azim,
           resolution=(args.width, args.height),
           grid_square_mm=square_mm, label=label)


if __name__ == '__main__':
    main()
