# water.tile.py
#
# Entire surface is a water pool at 3 mm (2 mm below the 5 mm ground level).

from dharmatiles.spec import Tile, Region, SurfaceConfig
from dharmatiles.layers import WaterLayer

tile = Tile(
    surface=SurfaceConfig(seed=17),
    sizes=[(1, 1), (3, 3)],
    regions=[
        Region(
            id='pool',
            contains=(0.5, 0.5),
            height_mm=3.0,
            layers=[
                WaterLayer(),
            ],
        ),
    ],
)
