import tempfile
import textwrap
import warnings
from pathlib import Path
from unittest import TestCase

import numpy as np

from dharmatiles.core.region import build_region_mask
from dharmatiles.spec import load_tile


class LoadTileTests(TestCase):
    def test_load_tile_supports_relative_helper_imports(self):
        with tempfile.TemporaryDirectory() as tmp:
            tile_dir = Path(tmp)
            (tile_dir / "shared_helpers.py").write_text(
                textwrap.dedent(
                    """
                    from dharmatiles.spec import SurfaceConfig

                    SURFACE = SurfaceConfig(seed=123)
                    """
                )
            )
            (tile_dir / "_lib").mkdir()
            (tile_dir / "_lib" / "__init__.py").write_text("")
            (tile_dir / "_lib" / "shared_surface.py").write_text(
                textwrap.dedent(
                    """
                    from dharmatiles.spec import SurfaceConfig

                    SURFACE = SurfaceConfig(seed=456)
                    """
                )
            )
            relative_tile = tile_dir / "relative.tile.py"
            relative_tile.write_text(
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

            tiles = load_tile(relative_tile)

        self.assertEqual([t.surface.seed for t in tiles], [123, 456])
