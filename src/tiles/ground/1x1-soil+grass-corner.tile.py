# soil+grass-corner.tile.py
#
# A patch of grass in the bottom-left corner; the rest is bare soil.
# Works as a transition tile where a meadow ends at a path corner.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

tile = Tile(
    surface=SurfaceConfig(cols=1, rows=1, seed=99),
    areas=[
        Region(
            id='patch',
            selector=FloodFill(0.15, 0.15),
            layers=[
                GrassCarpet(placement=Grouped(groups_per_square=240)),
                Scatter(
                    Grass(placement=Grouped(groups_per_square=240)),
                ),
            ],
        ),
        Region(
            id='floor',
            selector=FloodFill(0.75, 0.75),
            layers=[
                SoilCarpet(),
            ],
        ),
        Boundary(
            id='corner-cut',
            from_anchor=Edge.LEFT(0.5),
            to_anchor=Edge.BOTTOM(0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
        ),
    ],
)
