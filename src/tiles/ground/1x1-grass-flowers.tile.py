# 1x1-grass-flowers.tile.py
#
# A 1×1 grass meadow with scattered 3D wildflowers.
#
# Flowers are placed first so their footprints are stamped into support_z
# before grass blades are planted — blades grow around (not through) flowers.
#
# Shared SpeciesConfig drives both the GrassCarpet stamp texture and the 3D
# blade geometry so the two passes use identical blade proportions.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Flowers, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:15],
    blade_curl=D[0.4:0.7],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(seed=7),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
                Scatter(
                    # Flowers first — stamp support_z so grass steers around them
                    Flowers(
                        n_petals=5,
                        center_radius_mm=1.5,
                        outer_radius_mm=2.5,
                        column_height_mm=1.0,   # support column below each dome
                        dome_thickness_mm=1.0,  # thin centre dome cap height
                        petal_thickness_mm=0.5, # thin petal cap height
                        placement=Uniform(count_per_square=4),
                    ),
                    Grass(
                        species=_species,
                        placement=Grouped(groups_per_square=3),
                    ),
                ),
            ],
        ),
    ],
)
