# water.tile.py
#
# Entire surface is a water pool at 3 mm (2 mm below the 5 mm ground level).

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, SurfaceConfig,
)

tile = TileSpec(
    surface=SurfaceConfig(seed=17),
    sizes=[(1, 1), (3, 3)],
    regions=[
        RegionSpec(
            id='pool',
            contains=(0.5, 0.5),
            height_mm=3.0,
            layers=[
                LayerSpec(type='water'),
            ],
        ),
    ],
)
