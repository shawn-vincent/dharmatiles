"""Things you scatter inside a ``Scatter`` layer."""
from .config    import Uniform, Grouped
from .seed      import RockSeed
from .prototype import Rocks, Grass
from .flowers   import Flowers

__all__ = [
    'Rocks',
    'Grass',
    'Flowers',
    'Tree',
    'Uniform',
    'Grouped',
    'RockSeed',
]


def __getattr__(name):
    if name == 'Tree':
        from ..trees import Tree
        return Tree
    raise AttributeError(name)
