"""
Smoke-test: build the 1×1 grass-trees tile, render a PNG, then visually inspect
the result for tree-like properties (trunk pixels, overall geometry).

Run with:
    python -m pytest tests/test_tree_render.py -s

The rendered PNG is saved to /tmp/test_tree_render.png so you can open it after
the test to verify the tree looks correct.
"""
from __future__ import annotations

import pathlib
import unittest

SPEC = pathlib.Path("src/tiles/ground/1x1-grass-trees.tile.py")
OUT  = pathlib.Path("/tmp/test_tree_render.png")


class TreeRenderTest(unittest.TestCase):

    def test_tree_tile_builds_meshes(self) -> None:
        """The tree tile must produce meshes with geometry."""
        self.assertTrue(SPEC.exists(), f"spec not found: {SPEC}")

        from dharmatiles.terrains.tile import build_meshes_for_render
        meshes = build_meshes_for_render(SPEC)

        self.assertTrue(meshes, "build_meshes_for_render returned empty list")
        total_faces = sum(len(m.faces) for m in meshes)
        total_verts = sum(len(m.vertices) for m in meshes)
        print(f"\n  meshes={len(meshes)}, faces={total_faces:,}, verts={total_verts:,}")
        self.assertGreater(total_faces, 0, "no faces in any mesh part")

    def test_tree_tile_renders_to_png(self) -> None:
        """Build the tree tile and render to a PNG; check for brown (trunk) pixels."""
        self.assertTrue(SPEC.exists(), f"spec not found: {SPEC}")

        from dharmatiles.terrains.tile import build_meshes_for_render
        from dharmatiles.render import render

        meshes = build_meshes_for_render(SPEC)
        render(
            meshes, OUT,
            elev=35.0, azim=-120.0,
            resolution=(800, 700),
            quiet=True,
            grid_square_mm=35.0,
            label="ground/1x1-grass-trees",
        )

        self.assertTrue(OUT.exists(), f"PNG not written to {OUT}")
        file_size = OUT.stat().st_size
        self.assertGreater(file_size, 10_000, "PNG suspiciously small")

        # Check the image contains brown pixels (trunk + branches)
        import numpy as np
        from PIL import Image

        img = Image.open(OUT).convert('RGB')
        arr = np.array(img, dtype=float)

        # Brown: R > 120, G < 130, B < 80, and R significantly > B
        brown = (
            (arr[:, :, 0] > 120) &
            (arr[:, :, 1] < 130) &
            (arr[:, :, 2] < 80) &
            (arr[:, :, 0] > arr[:, :, 2] + 50)
        )
        brown_count = int(brown.sum())
        print(f"  brown (trunk) pixels: {brown_count}")
        self.assertGreater(brown_count, 50,
                           "fewer than 50 brown pixels — trunk/branches may be missing")

        print(f"\n→ tree render saved to {OUT}  ({file_size // 1024} KB)")
        print("  Open the PNG to verify the tree has a trunk + crown of branches.")


if __name__ == '__main__':
    unittest.main(verbosity=2)
