# walls-e5-textures.tile.py — walls campaign: surface-texture options.
#
# Four parallel wall runs on a 2x2, one per texture preset, front to
# back: chipped (m50 cut stone), worn (m24 cryptstone), hewn (m40
# rough), dressed (near-ashlar).  Runs sit in the tile interior, so
# both ends of every wall show textured brick ends with inset mortar.
# Lower height (32 mm) so the back rows stay visible in one render.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall

_H = 32.0
_X0, _X1 = 1.85, 0.15          # walk -x so the body (left side) faces south


def _wall(y: float, texture: str, seed: int) -> CutStoneWall:
    return CutStoneWall(spine=[(_X0, y), (_X1, y)], height_mm=_H,
                        texture=texture, seed=seed)


tile = Tile(
    surface=SurfaceConfig(seed=31, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            _wall(0.42, 'chipped', 41),
            _wall(0.88, 'worn',    42),
            _wall(1.34, 'hewn',    43),
            _wall(1.80, 'dressed', 44),
        ]),
    ],
)
