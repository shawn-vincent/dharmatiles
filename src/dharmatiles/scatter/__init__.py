"""Things you scatter inside a ``Scatter`` layer."""
from .config    import Uniform, Grouped
from .seed      import RockSeed
from .prototype import Rocks, Grass

__all__ = [
    'Rocks',
    'Grass',
    'Uniform',
    'Grouped',
    'RockSeed',
]
