"""
Named SpeciesConfig presets — canonical, reusable blade-geometry definitions.

Import these in ``.tile.py`` files and pass them to both ``GrassCarpet``
and ``Grass`` so the carpet and 3D blades share identical geometry.

Example::

    from tiles.shared.species import LUSH_GRASS

    tile = Tile(
        surface=SurfaceConfig(seed=42),
        areas=[
            Region(id='meadow', selector=FloodFill(0.5, 0.5), layers=[
                GrassCarpet(species=LUSH_GRASS),
                Grass(species=LUSH_GRASS),
            ]),
        ],
    )
"""
from dharmatiles.spec import SpeciesConfig, D

# Standard floppy grass — the default SpeciesConfig.
DEFAULT_GRASS = SpeciesConfig()

# Lush long-bladed grass (2x2 tree tiles, decorative meadows).
LUSH_GRASS = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

# Tall upright grass (wetland edges, uncut meadows).
TALL_GRASS = SpeciesConfig(
    blade_length=D[12:18],
    blade_curl=D[0.2:0.45],
    blade_clearance=0.15,
)
