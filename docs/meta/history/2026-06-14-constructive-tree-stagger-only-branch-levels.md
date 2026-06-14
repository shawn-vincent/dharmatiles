# Constructive Tree Stagger-Only Branch Levels

**Date:** 2026-06-14  
**Status:** Design before implementation  
**Affects:** `src/dharmatiles/trees/const_skeleton.py`, `tests/test_const_tree.py`

## Context

Constructive tree branching currently uses two vertical scheduling systems at
once:

- `n_levels` creates evenly spaced crown levels.
- `branch_stagger` subdivides each level so parents in the same topological
  generation can branch at slightly different heights.

That means the crown still reads as level-first.  Stagger can break up a level,
but it cannot globally decide where all branch points should happen.

## Desired Model

`n_levels` should describe branching topology only: how many generations of
branching the tree will produce.

`branch_stagger` should describe vertical branchpoint scheduling:

- `branch_stagger = 0.0` uses `n_levels` vertical branch bands, matching the
  old simultaneous-per-generation structure.
- `branch_stagger = 1.0` uses one vertical branch band per actual branchpoint.
- Intermediate values interpolate between those counts.

The implementation first samples the constructive branching topology for the
requested `n_levels`, counts the total branchpoints that will exist, then maps
`branch_stagger` from:

```text
0..1 -> n_levels..total_branchpoints
```

## Canopy-Slope Distribution

The selected vertical branch bands are not distributed evenly.  They are placed
through the crown according to the canopy envelope slope:

- Where the envelope is more horizontal, branch bands should be denser.
- Where the envelope is more vertical, branch bands should be sparser.

Practically, this means sampling the crown-radius profile and placing branch
bands at quantiles of a density derived from the profile's horizontal change.
The side-profile tangent becomes "more horizontal" when radius changes quickly
relative to height, so those crown sections receive more branch levels.

## Expected Result

Low stagger produces familiar generation bands.  High stagger produces a more
organic vertical distribution where branchpoints are globally staggered across
the crown, with extra branching detail concentrated around the canopy sections
whose silhouette is spreading or tapering most strongly.
