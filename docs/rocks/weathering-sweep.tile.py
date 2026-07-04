# weathering-sweep.tile.py — E6: the weathering knob, angular → cobble.
# Four same-size stones in a row; only facet count (and egg) varies.
# Left = sharp shard-cut (facets 8), right = weathered cobble (facets 22).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec

sweep = [
    StoneSpec(x=6.0,  y=17.5, footprint_mm=6.5, height_mm=6.0,
              aspect=0.85, facets=8,  yaw_deg=15.0, burial=1.0, seed=21),
    StoneSpec(x=14.0, y=17.5, footprint_mm=6.5, height_mm=6.0,
              aspect=0.85, facets=12, yaw_deg=80.0, burial=1.0, seed=22),
    StoneSpec(x=22.0, y=17.5, footprint_mm=6.5, height_mm=6.0,
              aspect=0.85, facets=16, yaw_deg=150.0, burial=1.0, seed=23),
    StoneSpec(x=30.0, y=17.5, footprint_mm=6.5, height_mm=6.0,
              aspect=0.85, facets=22, yaw_deg=220.0, burial=1.0, seed=24),
]

tile = Tile(
    surface=SurfaceConfig(seed=3),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet(), FacetedStones(stones=sweep)],
        ),
    ],
)
