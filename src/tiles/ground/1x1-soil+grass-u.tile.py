# soil+grass-u.tile.py
#
# Grass wraps three sides (left arm, top-right, bottom-right); a bare-soil
# clearing opens from the right edge — like a glade backed by meadow.
#
# Three boundaries enclose the clearing:
#   left-wall  — vertical at x≈0.6, spans full tile height
#   upper-arm  — diagonal from the top anchor of left-wall to the right edge
#   lower-arm  — diagonal from the bottom anchor of left-wall to the right edge
# Together they create a fully enclosed pocket accessible only from the right.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

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


tile = Tile(
    surface=SurfaceConfig(seed=102),
    areas=[
        Region(
            id='clearing',
            selector=FloodFill(0.85, 0.5),
            layers=[SoilCarpet()],
        ),
        _meadow('meadow-left',         0.3,  0.5),
        _meadow('meadow-top-right',    0.85, 0.9),
        _meadow('meadow-bottom-right', 0.85, 0.1),
        # Left wall of the clearing pocket (shared anchor with both arm boundaries)
        Boundary(
            id='left-wall',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.BOTTOM(0.6),
            path='organic',
            amplitude_mm=1.5,
            wavelength_mm=10.0,
        ),
        # Upper arm — from shared top anchor to upper-right edge
        Boundary(
            id='upper-arm',
            from_anchor=Edge.TOP(0.6),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
        ),
        # Lower arm — from shared bottom anchor to lower-right edge
        Boundary(
            id='lower-arm',
            from_anchor=Edge.BOTTOM(0.6),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=2.0,
            wavelength_mm=8.0,
        ),
    ],
)
