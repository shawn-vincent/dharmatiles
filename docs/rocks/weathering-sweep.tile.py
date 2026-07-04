# weathering-sweep.tile.py — E6: the weathering knob, angular → cobble.
# Four same-size stones in a row; only facet count (and egg) varies.
# Left = sharp shard-cut (facets 8), right = weathered cobble (facets 22).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec

# Weathering = roundover: the SAME stone four times (identical seed and
# shape), edges and corners progressively rounded over — randomized per
# corner so it reads geological, not CNC.
sweep = [
    StoneSpec(x=6.0,  y=17.5, footprint_mm=8.0, height_mm=7.5,
              aspect=0.85, facets=10, roundover_mm=0.0,
              yaw_deg=15.0, burial=1.0, seed=21),
    StoneSpec(x=14.5, y=17.5, footprint_mm=8.0, height_mm=7.5,
              aspect=0.85, facets=10, roundover_mm=0.5,
              yaw_deg=15.0, burial=1.0, seed=21),
    StoneSpec(x=22.5, y=17.5, footprint_mm=8.0, height_mm=7.5,
              aspect=0.85, facets=10, roundover_mm=1.0,
              yaw_deg=15.0, burial=1.0, seed=21),
    StoneSpec(x=30.0, y=17.5, footprint_mm=8.0, height_mm=7.5,
              aspect=0.85, facets=10, roundover_mm=1.7,
              yaw_deg=15.0, burial=1.0, seed=21),
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
