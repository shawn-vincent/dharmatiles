"""Things you scatter inside a ``Scatter`` layer."""
from .config    import Uniform, Grouped
from .seed      import RockSeed
from .prototype import Rocks, Grass
from .flowers   import Flowers

__all__ = [
    'Rocks',
    'Grass',
    'Flowers',
    'CloudTree',
    'Uniform',
    'Grouped',
    'RockSeed',
]


def __getattr__(name):
    if name == 'CloudTree':
        from ..trees import CloudTree
        return CloudTree
    raise AttributeError(name)
