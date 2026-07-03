# rocks-judgment.tile.py — step-1 judgment scene for the rocks rework.
# Bare soil, three size bands of current-kernel rocks (boulder / medium /
# pebble), no grass, so the stones can be judged on their own.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter import Rocks, Uniform

tile = Tile(
    surface=SurfaceConfig(seed=7),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[
                SoilCarpet(),
                Rocks(r=D[4.5:6.0], flat=D[0.8:1.2], n_cuts=5,
                      placement=Uniform(count_per_square=2, gap_mm=10)),
                Rocks(r=D[2.0:3.2],
                      placement=Uniform(count_per_square=6, gap_mm=5)),
                Rocks(r=D[0.7:1.4],
                      placement=Uniform(count_per_square=18, gap_mm=2)),
            ],
        ),
    ],
)
