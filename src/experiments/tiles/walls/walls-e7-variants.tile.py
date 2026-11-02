# walls-e7-variants.tile.py — walls campaign: fieldstone configurability.
#
# Four parallel wall runs on a 2x2, front to back, spanning the family
# space the promoted FieldstoneWall knobs open up (stone refactor
# stage 4).  Same chassis, same crack-network mechanism — only config:
#
#   slabs    thin squared courses, near-flat beds, tight contacts —
#            the Cotswold coursed-slab read (fieldstone refs 03/04)
#   default  the approved E25 drystone look, untouched knobs
#   cobbles  tall round stones, big roundovers, rugged faces
#   dressed  the cut-stone family on the same chassis, for the full
#            rock -> fieldstone -> worked-stone axis in one scene

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, FieldstoneWall

_X0, _X1 = 1.85, 0.15          # walk -x so the body (left side) faces south


def _run(y: float) -> list[tuple[float, float]]:
    return [(_X0, y), (_X1, y)]


tile = Tile(
    surface=SurfaceConfig(seed=33, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            FieldstoneWall(_run(0.42), seed=51,
                           course_mm=(2.0, 3.6),
                           roundover_mm=(0.7, 1.4),
                           bed_flat_exp=(0.18, 0.32),
                           head_overlap_mm=(0.25, 0.55),
                           bed_overlap_mm=(0.20, 0.45),
                           proud_mm=(0.05, 0.35),
                           wobble_amp_mm=(0.10, 0.22)),
            FieldstoneWall(_run(0.88), seed=52),
            FieldstoneWall(_run(1.34), seed=53,
                           course_mm=(3.6, 6.5),
                           roundover_mm=(2.0, 3.2),
                           bed_flat_exp=(0.75, 1.05),
                           proud_mm=(0.25, 1.00),
                           wobble_amp_mm=(0.25, 0.50)),
            CutStoneWall(_run(1.80), seed=54,
                         texture='dressed'),
        ]),
    ],
)
