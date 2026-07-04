# hero-boulder.tile.py — the "that's clearly a big rock" archetype.
# One glacial-erratic loaf on plain ground, per
# docs/reference/big-boulders/README.md: rounded but not spherical,
# widest near the base with a buried contact line, a few LARGE soft
# facets meeting at rounded arrises, gently domed top.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec
import os

_seed = int(os.environ.get('HERO_SEED', '5'))

tile = Tile(
    surface=SurfaceConfig(seed=9),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[
                SoilCarpet(),
                FacetedStones(stones=[
                    StoneSpec(x=17.5, y=17.5,
                              footprint_mm=18.0, height_mm=11.0,
                              aspect=0.7, facets=11,
                              yaw_deg=35.0, burial=1.0,
                              egg=0.2, roundover_mm=1.1,
                              spall_scars=2, crown_flat=0.8,
                              seed=_seed),
                ]),
            ],
        ),
    ],
)
