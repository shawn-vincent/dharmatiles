# grass-carpet.tile.py
#
# Full-square grass carpet only — no 3D blades, no soil texture.
# Use this to inspect the embossed 2D grass-carpet layer in isolation.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import GrassCarpet

tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(),
            ],
        ),
    ],
)
