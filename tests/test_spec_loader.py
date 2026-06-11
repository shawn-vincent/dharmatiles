import tempfile
import textwrap
from pathlib import Path
from unittest import TestCase

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
