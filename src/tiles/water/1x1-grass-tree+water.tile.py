"""1x1 grass meadow with a tree and a water pool; rocks in water, shore, and grass."""

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Tree, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(seed=53),
    areas=[
        # Pool first so its rocks are stamped into obstacle_mask before grass grows
        Region(
            id='pool',
            selector=FloodFill(0.5, 0.25),
            height_mm=3.0,
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=3),
                        r=D[2.5:4.5],
                        flat=D[1.5:1.8],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Boundary(
            id='shoreline',
            from_anchor=Edge.LEFT(0.48),
            to_anchor=Edge.RIGHT(0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=12.0,
            width_mm=3.0,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=80),
                        r=D[0.8:2.5].power(1.5),
                    ),
                ),
            ],
        ),
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.75),
            layers=[
                GrassCarpet(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
                Scatter(
                    # Rocks first — stamp obstacle_mask so tree and grass steer around them
                    Rocks(
                        r=D[2.0:3.5],
                        placement=Uniform(count_per_square=4),
                    ),
                    Tree(
                        height_mm=35.0,
                        canopy_radius_mm=12.0,
                        canopy_base_radius_mm=11.0,
                        placement=Uniform(count_per_square=1),
                        n_attractors=32,
                        foliage_clusters=True,
                        leaf_length_mm=3.88,
                        leaf_width_mm=2.42,
                        leaf_cap_count=6,
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
