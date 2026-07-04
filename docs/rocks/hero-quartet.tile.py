# hero-quartet.tile.py — self-comparison scene (E9): one stone per
# reference archetype in docs/reference/big-boulders/, selected by the
# HERO env var so each can be rendered alone at high resolution.
#
#   letipea — rounded loaf, broad flattish top, one wrap seam, sits low
#   sylt    — taller dome, a couple of soft facet planes, lichen-era age
#   yeager  — rounded-cuboid, domed top, softly rounded vertical faces
#   doane   — low hunched asymmetric hump, half-emerged from the ground

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec
import os

_H = os.environ.get('HERO', 'letipea')

_specs = {
    'letipea': StoneSpec(x=17.5, y=17.5, footprint_mm=19.0, height_mm=14.5,
                         aspect=0.68, facets=11, yaw_deg=30.0,
                         burial=0.75, egg=0.18, roundover_mm=1.2,
                         crown_flat=0.7, spall_scars=1, seam_z=0.42,
                         seed=41),
    'sylt':    StoneSpec(x=17.5, y=17.5, footprint_mm=15.0, height_mm=14.0,
                         aspect=0.8, facets=10, yaw_deg=70.0,
                         lean_deg=6.0, lean_dir_deg=210.0,
                         burial=0.95, egg=0.28, roundover_mm=0.9,
                         crown_flat=0.5, spall_scars=2, seed=17),
    'yeager':  StoneSpec(x=17.5, y=17.5, footprint_mm=16.0, height_mm=13.0,
                         aspect=0.9, facets=9, yaw_deg=15.0,
                         burial=0.9, egg=0.12, roundover_mm=0.8,
                         crown_flat=0.7, spall_scars=2, seed=8),
    'doane':   StoneSpec(x=17.5, y=17.5, footprint_mm=20.0, height_mm=12.0,
                         aspect=0.75, facets=12, yaw_deg=100.0,
                         burial=1.05, egg=0.3, roundover_mm=1.5,
                         crown_flat=0.55, spall_scars=1, seed=29),
}

tile = Tile(
    surface=SurfaceConfig(seed=9),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet(), FacetedStones(stones=[_specs[_H]])],
        ),
    ],
)
