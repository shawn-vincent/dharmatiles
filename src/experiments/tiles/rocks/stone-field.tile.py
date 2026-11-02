# stone-field.tile.py — acceptance scene S2 for the rocks rework (E4).
# Bare soil, sampled clusters: dominant stones + companions, no hand
# authoring.  Compare against docs/rocks/rocks-current-2026-07-03.png.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter import Uniform
from dharmatiles.scatter.stones import StoneField

tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[
                SoilCarpet(),
                StoneField(placement=Uniform(count_per_square=3)),
            ],
        ),
    ],
)
