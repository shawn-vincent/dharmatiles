"""Scatter placement system for rocks, grass, and other surface features."""
from .config    import ScatterConfig
from .seed      import RockSeed
from .prototype import RockPrototype, GrassPrototype
from .layer     import ScatterLayer

__all__ = [
    'ScatterConfig',
    'RockSeed',
    'RockPrototype',
    'GrassPrototype',
    'ScatterLayer',
]
