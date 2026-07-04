# stone-showcase.tile.py — the breadth of the faceted-stone system on one
# 2x2 print (70 mm square).  Rows back→front: fresh shards → aged loafs.
#
#   back row    fresh cleaved shards & slab (roundover 0, crisp facets)
#   mid rows    working range: lumps & slabs at light/medium weathering,
#               with scars, seams, crown plateaus, egg profiles
#   front row   hero-class aged boulders (Letipea loaf, Doane hump)
#
# Every stone is the SAME primitive — only knobs differ.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.scatter.stones import FacetedStones, StoneSpec

S = StoneSpec
_stones = [
    # ── back row (y≈58): fresh, crisp, cleaved ──────────────────────────
    S(x=12, y=58, footprint_mm=8.0, height_mm=15.0, aspect=0.65, facets=11,
      yaw_deg=20, lean_deg=9, lean_dir_deg=200, burial=0.9, egg=0.1,
      roundover_mm=0.0, seed=3),                          # tall shard
    S(x=30, y=58, footprint_mm=9.0, height_mm=7.5, aspect=0.85, facets=10,
      yaw_deg=70, burial=1.0, roundover_mm=0.0, seed=7),  # fresh lump
    S(x=47, y=58, footprint_mm=10.0, height_mm=5.5, aspect=0.9, facets=9,
      yaw_deg=140, burial=1.05, roundover_mm=0.0, seed=12),  # fresh slab
    S(x=61, y=57, footprint_mm=6.0, height_mm=10.5, aspect=0.7, facets=10,
      yaw_deg=310, lean_deg=13, lean_dir_deg=40, burial=0.85, egg=0.1,
      roundover_mm=0.1, seed=41),                         # leaning splinter

    # ── mid row (y≈38): light-to-medium weathering, incidents ───────────
    S(x=10, y=38, footprint_mm=10.0, height_mm=8.0, aspect=0.8, facets=12,
      yaw_deg=15, burial=0.95, roundover_mm=0.45,
      spall_scars=1, seed=23),                            # lightly worn lump
    S(x=28, y=38, footprint_mm=11.0, height_mm=7.0, aspect=0.75, facets=12,
      yaw_deg=95, burial=1.0, roundover_mm=0.8, crown_flat=0.6,
      seam_z=0.5, seed=17),                               # seamed loaf
    S(x=46, y=38, footprint_mm=9.0, height_mm=8.5, aspect=0.85, facets=13,
      yaw_deg=200, burial=0.9, egg=0.25, roundover_mm=0.65,
      spall_scars=2, seed=8),                             # scarred boulder
    S(x=61, y=38, footprint_mm=7.0, height_mm=5.0, aspect=0.9, facets=10,
      yaw_deg=260, burial=1.1, roundover_mm=1.2, seed=29),  # river cobble

    # ── front row (y≈15): hero-class aged boulders ──────────────────────
    S(x=14, y=15, footprint_mm=16.0, height_mm=12.0, aspect=0.68, facets=14,
      yaw_deg=30, burial=0.78, egg=0.18, roundover_mm=1.2, crown_flat=0.7,
      spall_scars=1, seam_z=0.42, seed=41),               # Letipea loaf
    S(x=37, y=13, footprint_mm=13.0, height_mm=11.0, aspect=0.8, facets=13,
      yaw_deg=70, lean_deg=6, lean_dir_deg=210, burial=0.95, egg=0.28,
      roundover_mm=0.9, crown_flat=0.5, spall_scars=2, seed=17),  # Sylt dome
    S(x=57, y=14, footprint_mm=14.0, height_mm=9.0, aspect=0.75, facets=15,
      yaw_deg=100, burial=1.05, egg=0.3, roundover_mm=1.5, crown_flat=0.55,
      spall_scars=1, seed=29),                            # Doane hump
]

tile = Tile(
    surface=SurfaceConfig(seed=9, cols=2, rows=2),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet(), FacetedStones(stones=_stones)],
        ),
    ],
)
