import tempfile
import textwrap
import warnings
from pathlib import Path
from unittest import TestCase

import numpy as np

from dharmatiles.core.region import build_region_mask
from dharmatiles.spec import load_spec


class LoadSpecTests(TestCase):
    def test_load_spec_supports_relative_helper_imports(self):
        with tempfile.TemporaryDirectory() as tmp:
            spec_dir = Path(tmp)
            (spec_dir / "shared_helpers.py").write_text(
                textwrap.dedent(
                    """
                    from dharmatiles.spec import SurfaceConfig

                    SURFACE = SurfaceConfig(seed=123)
                    """
                )
            )
            (spec_dir / "_lib").mkdir()
            (spec_dir / "_lib" / "__init__.py").write_text("")
            (spec_dir / "_lib" / "shared_surface.py").write_text(
                textwrap.dedent(
                    """
                    from dharmatiles.spec import SurfaceConfig

                    SURFACE = SurfaceConfig(seed=456)
                    """
                )
            )
            relative_spec = spec_dir / "relative.tile.py"
            relative_spec.write_text(
                textwrap.dedent(
                    """
                    from dharmatiles.spec import Tile
                    from . import shared_helpers
                    from ._lib import shared_surface

                    tile = Tile(surface=shared_helpers.SURFACE)
                    tiles = [tile, Tile(surface=shared_surface.SURFACE)]
                    """
                )
            )

            tiles = load_spec(relative_spec)

        self.assertEqual([t.surface.seed for t in tiles], [123, 456])

    def test_all_tile_specs_build_region_masks(self):
        tile_paths = sorted(Path("src/tiles").glob("*/*.tile.py"))

        self.assertTrue(tile_paths)

        for path in tile_paths:
            with self.subTest(path=str(path)):
                with warnings.catch_warnings(record=True) as caught:
                    warnings.simplefilter("always")
                    for tile in load_spec(path):
                        mask = build_region_mask(tile)
                        self.assertNotIn(-2, np.unique(mask))

                self.assertEqual([str(w.message) for w in caught], [])
