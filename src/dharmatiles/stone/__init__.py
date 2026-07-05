"""Shared stone-making primitives.

Everything that turns "a solid" into "a stone" lives here, family-
independent: scatter rocks (``scatter/stones.py``), the cut-stone wall,
and the fieldstone wall (``walls/``) all build on these.  A family is a
SHAPE source (faceted hull, jittered box, crack-outline sphere-morph)
plus this shared finishing/guarantee machinery.

Review + plan: docs/meta/history/2026-07-05-walls-rocks-refactor-review.md.
"""
from .finish import aged_relief
from .shape import fibonacci_sphere, round_edges, rubble_stone
from .solidify import clip_to_box, separate_pinches, survives_stl32
from .surface import blur_remesh, relief_field

__all__ = [
    'aged_relief',
    'blur_remesh',
    'clip_to_box',
    'fibonacci_sphere',
    'relief_field',
    'round_edges',
    'rubble_stone',
    'separate_pinches',
    'survives_stl32',
]
