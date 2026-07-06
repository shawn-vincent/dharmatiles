"""Shared stone-making primitives.

Everything that turns "a solid" into "a stone" lives here, family-
independent: scatter rocks (``scatter/stones.py``), the cut-stone wall,
and the fieldstone wall (``walls/``) all build on these.  A family is a
SHAPE source (faceted hull, jittered box, crack-outline sphere-morph)
plus this shared finishing/guarantee machinery.

Review + plan: docs/meta/history/2026-07-05-walls-rocks-refactor-review.md.
"""
from .finish import stone_relief
from .noise import fbm, value_noise
from .shape import (fibonacci_sphere, round_edges, rounded_box,
                    rubble_stone)
from .solidify import clip_to_box, separate_pinches, survives_stl32
from .surface import blur_remesh

__all__ = [
    'blur_remesh',
    'clip_to_box',
    'fbm',
    'fibonacci_sphere',
    'round_edges',
    'rounded_box',
    'rubble_stone',
    'separate_pinches',
    'stone_relief',
    'survives_stl32',
    'value_noise',
]
