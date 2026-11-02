# monolith-trio.tile.py — acceptance scene S1 for the rocks rework.
# One composed group on bare soil: tall leaning shard + lump + slab,
# matching docs/rocks/rocks-reference-monolith-trio.png.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec

trio = [
    # Tall shard, leaning gently toward the viewer-left like the reference.
    StoneSpec(x=17.5, y=19.0, footprint_mm=7.5, height_mm=16.0,
              aspect=0.72, facets=12, yaw_deg=25.0,
              lean_deg=10.0, lean_dir_deg=200.0, burial=0.9, egg=0.12,
              seed=3),
    # Chunky lump, front-left, nestled against the shard base.
    StoneSpec(x=13.2, y=16.2, footprint_mm=6.5, height_mm=8.0,
              aspect=0.85, facets=11, yaw_deg=70.0,
              lean_deg=4.0, lean_dir_deg=250.0, burial=1.0, seed=7),
    # Low wide stone, front-right, touching the shard.
    StoneSpec(x=23.0, y=13.6, footprint_mm=6.0, height_mm=5.0,
              aspect=0.9, facets=9, yaw_deg=140.0,
              lean_deg=0.0, burial=1.05, seed=12),
]

tile = Tile(
    surface=SurfaceConfig(seed=11),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet(), FacetedStones(stones=trio)],
        ),
    ],
)
