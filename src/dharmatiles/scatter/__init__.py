"""Scatter things: direct tile layers that place elements into a region.

``Rocks``, ``Grass``, ``Flowers``, and ``Tree`` each implement the
``TileLayer`` protocol (``apply()``) and may be placed directly in
``Region.layers`` alongside ``SoilCarpet``, ``GrassCarpet``, and ``Water``.
Ordering in the list is the author's contract for state dependencies:
put ``Rocks`` before ``Grass`` so blades steer around rock footprints.
"""
from .config    import Uniform, Grouped
from .seed      import RockSeed
from .prototype import Rocks, Grass
from .flowers   import Flowers

__all__ = [
    'Rocks',
    'Grass',
    'Flowers',
    'BarkConfig',
    'Tree',
    'Uniform',
    'Grouped',
    'RockSeed',
]


def __getattr__(name):
    if name == 'Tree':
        from ..trees import Tree
        return Tree
    if name == 'BarkConfig':
        from ..trees import BarkConfig
        return BarkConfig
    raise AttributeError(name)
