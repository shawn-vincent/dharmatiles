# Baseline probe: what a "wall" is TODAY — a tall region, flat extrusion.
from dharmatiles.spec import Tile, Region, Boundary, FloodFill, SurfaceConfig, Edge
from dharmatiles.layers import SoilCarpet

tile = Tile(
    surface=SurfaceConfig(seed=5),
    areas=[
        Boundary(id='wall-face', from_anchor=Edge.LEFT(0.6),
                 to_anchor=Edge.RIGHT(0.6), path='organic',
                 amplitude_mm=1.5),
        Region(id='ground', selector=FloodFill(0.5, 0.15),
               layers=[SoilCarpet()], height_mm=5.0),
        Region(id='wall', selector=FloodFill(0.5, 0.85),
               layers=[SoilCarpet()], height_mm=30.0),
    ],
)
