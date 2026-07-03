"""1x1 grass meadow with one Tree, a few big rocks, and 3D grass blades."""

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet
from dharmatiles.scatter import Rocks, Grass, Tree, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                # Obstacle-stampers (Rocks, Tree) before the carpet and grass:
                # carpet tubes end at footprints instead of being impaled.
                # Crown rule for rocks in thatch grass: r*flat - sink must
                # beat mounds + blade stack (~3.4 mm) or the stones drown.
                Rocks(
                    r=D[3.6:4.6].power(1.3),
                    flat=D[1.5:1.7],
                    sink=0.3,
                    n_cuts=5,
                    cut=D[0.82:0.96],
                    placement=Uniform(count_per_square=3, gap_mm=7),
                ),
                # Tree next — trunk footprint stamped before grass is planted
                Tree(
                    height_mm=35.0,
                    canopy_radius_mm=12.0,
                    canopy_base_radius_mm=11.0,
                    placement=Uniform(count_per_square=1),
                    n_attractors=32,
                    foliage_clusters=True,
                    leaf_length_mm=4.5,
                    leaf_width_mm=3.0,
                ),
                GrassCarpet(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
                # Grass last — grows around rocks and tree trunk
                Grass(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
            ],
        ),
    ],
)
