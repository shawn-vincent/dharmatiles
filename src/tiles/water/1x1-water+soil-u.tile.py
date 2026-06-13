# water+soil-u.tile.py
#
# A water cove indents from the right edge; soil banks wrap the left, top,
# and bottom sides — like an inlet cutting into dry land on three sides.
#
# Three shoreline boundaries enclose the cove:
#   left-shore  — vertical at x≈0.6 across full tile height
#   upper-shore — diagonal from top anchor to upper-right edge
#   lower-shore — diagonal from bottom anchor to lower-right edge

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform

_shore_rocks = lambda: Scatter(Rocks(placement=Uniform(count_per_square=50), r=D[0.8:2.0].power(1.5)))

tile = Tile(
    surface=SurfaceConfig(seed=54),
    areas=[
        Region(
            id='cove',
            selector=FloodFill(0.85, 0.5),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=2),
                        r=D[2.5:4.5],
                        flat=D[1.665:1.81],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(id='bank-left',         selector=FloodFill(0.3,  0.5),  layers=[SoilCarpet()]),
        Region(id='bank-top-right',    selector=FloodFill(0.85, 0.9),  layers=[SoilCarpet()]),
        Region(id='bank-bottom-right', selector=FloodFill(0.85, 0.1),  layers=[SoilCarpet()]),
        Boundary(
            id='left-shore',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.BOTTOM(0.6),
            path='organic',
            amplitude_mm=1.5,
            wavelength_mm=10.0,
            width_mm=2.0,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
        Boundary(
            id='upper-shore',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
            width_mm=2.0,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
        Boundary(
            id='lower-shore',
            from_anchor=Edge.BOTTOM(0.6),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
            width_mm=2.0,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
    ],
)
