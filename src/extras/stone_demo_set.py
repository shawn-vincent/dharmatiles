#!/usr/bin/env python3
"""Build the stone/walls demo set: STLs + renders + watertight report.

The acceptance harness for the 2026-07-05 walls/rocks DRY refactor
(docs/meta/history/2026-07-05-walls-rocks-refactor-review.md): seven
scenes spanning the range of configurability of scatter rocks, the
cut-stone wall, and the fieldstone wall.  Run it after every refactor
stage; compare the renders against the previous stage / the
`walls-fieldstone-e25` baseline.

Usage:
    python src/extras/stone_demo_set.py OUT_DIR [--only NAME ...]

STLs go to stl/test/demo-<name>-db.stl; renders to OUT_DIR/<name>.png.
Each scene is built ONCE (STL export and render share the build).
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

sys.path.insert(0, 'src')

SCENES = {
    'rocks-weathering':  ('docs/rocks/weathering-sweep.tile.py',   (45, -135)),
    'rocks-showcase':    ('docs/rocks/stone-showcase.tile.py',     (45, -135)),
    'rocks-field':       ('docs/rocks/stone-field.tile.py',        (45, -135)),
    'wall-cut-textures': ('docs/walls/walls-e5-textures.tile.py',  (30, -160)),
    'wall-cut-corner':   ('docs/walls/walls-e2-corner.tile.py',    (20, 135)),
    'wall-fieldstone':   ('docs/walls/walls-e6-fieldstone.tile.py', (20, 135)),
    'wall-brick':        ('docs/walls/walls-e8-brick.tile.py',     (20, 135)),
    'wall-tops':         ('docs/walls/walls-e9-tops.tile.py',      (25, -155)),
    'wall-variants':     ('docs/walls/walls-e7-variants.tile.py',  (30, -160)),
    'wall-meadow':       ('docs/walls/walls-e4-meadow.tile.py',    (35, -155)),
}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument('out_dir', type=pathlib.Path)
    ap.add_argument('--only', nargs='*', default=None,
                    help='scene names to build (default: all)')
    args = ap.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=True)
    stl_dir = pathlib.Path('stl/test')
    stl_dir.mkdir(parents=True, exist_ok=True)

    import trimesh
    from dharmatiles.spec import load_tile
    from dharmatiles.terrains.tile import build_tile_from_spec
    from dharmatiles.terrains.reporter import SilentReporter
    from dharmatiles.render import render

    names = args.only or list(SCENES)
    failures = []
    for name in names:
        spec_path, (elev, azim) = SCENES[name]
        t0 = time.perf_counter()
        tile = load_tile(pathlib.Path(spec_path))[0]
        tile.systems = [s for s in tile.systems if s.suffix == 'db']
        out_stl = stl_dir / f'demo-{name}-db.stl'
        _main, render_meshes, _d = build_tile_from_spec(
            tile, system_paths={'db': out_stl}, reporter=SilentReporter())

        m = trimesh.load(out_stl)
        wt = m.is_watertight
        if not wt:
            failures.append(name)
        print(f'{name}: {len(m.faces):,} faces  watertight={wt}  '
              f'({time.perf_counter() - t0:.0f}s)')

        out_png = args.out_dir / f'{name}.png'
        for attempt in range(3):          # pyglet cocoa init is flaky
            try:
                render(render_meshes, out_png, elev=elev, azim=azim,
                       resolution=(1200, 1000),
                       grid_square_mm=tile.surface.square_mm, label=name)
                break
            except Exception as exc:      # noqa: BLE001
                if attempt == 2:
                    print(f'  render failed: {exc}')

    if failures:
        print(f'\nNOT WATERTIGHT: {", ".join(failures)}')
        sys.exit(1)
    print('\nall watertight')


if __name__ == '__main__':
    main()
