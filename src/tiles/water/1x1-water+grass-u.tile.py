# water+grass-u.tile.py
#
# A water cove indents from the right edge; grass meadow wraps the left,
# top, and bottom sides — a lagoon enclosed by meadow on three sides.
#
# Three shoreline boundaries enclose the cove:
#   left-shore  — vertical at x≈0.6 across full tile height
#   upper-shore — diagonal from top anchor to upper-right edge
#   lower-shore — diagonal from bottom anchor to lower-right edge

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

_species = SpeciesConfig()


def _meadow(region_id: str, x: float, y: float) -> Region:
    return Region(
        id=region_id,
        selector=FloodFill(x, y),
        layers=[
            GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
            Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
        ],
    )


_shore_rocks = lambda: Scatter(Rocks(placement=Uniform(count_per_square=50), r=D[0.8:2.0].power(1.5)))

tile = Tile(
    surface=SurfaceConfig(seed=63),
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
        _meadow('meadow-left',         0.3,  0.5),
        _meadow('meadow-top-right',    0.85, 0.9),
        _meadow('meadow-bottom-right', 0.85, 0.1),
        Boundary(
            id='left-shore',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.BOTTOM(0.6),
            path='organic',
            amplitude_mm=1.5,
            wavelength_mm=10.0,
            width_mm=2.5,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
        Boundary(
            id='upper-shore',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
            width_mm=2.5,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
        Boundary(
            id='lower-shore',
            from_anchor=Edge.BOTTOM(0.6),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
            width_mm=2.5,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
    ],
)
