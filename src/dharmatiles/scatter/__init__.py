"""Things you scatter inside a ``ScatterLayer``."""
from .config    import ScatterConfig
from .seed      import RockSeed
from .prototype import Rocks, Grass
from .layer     import ScatterLayer

__all__ = [
    'Rocks',
    'Grass',
    'ScatterConfig',
    'ScatterLayer',
    'RockSeed',
]
