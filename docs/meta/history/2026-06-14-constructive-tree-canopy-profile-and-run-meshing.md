# Constructive Tree Canopy Profile and Run Meshing

**Date:** 2026-06-14  
**Status:** Implemented  
**Affects:** `ConstTreeConfig`, `trees/const_skeleton.py`, `trees/surface.py`

## Context

The constructive tree grower previously described the crown with
`crown_height_fraction`, `crown_spread`, and `crown_taper`.  That gave some
control over the silhouette, but it mixed three different artistic ideas:
overall tree height, the bare trunk height, and the top/bottom crown endpoint
shape.

The branch surface builder also emitted one capped curved tube per skeleton
edge.  That made intermediate guide nodes visible as geometry boundaries even
when those nodes existed only to reserve space or stagger later branch points.

## Canopy Parameterization

The constructive canopy profile should be controlled by:

- `height_max_mm` / `height_mm`: total tree height target.
- `trunk_height_mm`: height of the bare trunk before branches begin.
- `crown_radius_mm`: maximum crown width.
- `top_pointiness` and `top_curve`: upper endpoint silhouette, from round to a
  strict taper.
- `bottom_pointiness` and `bottom_curve`: lower endpoint silhouette, from round
  to a strict taper.

The implementation models the crown radius at normalised crown height `t` as the
minimum of two endpoint envelopes:

```text
radius(t) = crown_radius_mm * normalise(
    min(bottom_envelope(t), top_envelope(1 - t))
)
```

Each endpoint envelope blends between a quarter-round arc and a linear taper.
`pointiness=0` favours the round arc; `pointiness=1` favours the strict taper.
`curve` controls how quickly the endpoint reaches full width.  A normalisation
pass keeps `crown_radius_mm` as the actual maximum even with asymmetric top and
bottom settings.

If `trunk_height_mm` is omitted, the old `crown_height_fraction` still derives
the trunk height.  Old `crown_spread` and `crown_taper` constructor arguments map
to `bottom_curve` and `top_curve` for compatibility.

## Smooth Branch Runs

The skeleton still keeps intermediate unary nodes.  Those nodes are important:
they mark occupied space, stagger future branching events, and shape the path a
branch should follow.

The surface builder now treats them as guide nodes rather than separate visible
pieces.  It extracts maximal runs from one real branch point to the next real
branch point or tip:

```text
branch point/root -> unary guide -> unary guide -> branch point/tip
```

Each run is rendered as one continuous Hermite tube from the run start to the
run endpoint.  Guide nodes influence the endpoint tangents and growth/radius
bookkeeping, but they are not mandatory visible waypoints.  Caps are emitted
only at the run endpoints.  This keeps the branch smooth from its base to the
actual next branch point while preserving the guide-node spacing used by the
growth algorithm.

## Expected Result

Artists get direct control over height, bare trunk height, crown width, and
top/bottom crown character.  Geometry reads as smoother because intermediate
growth nodes influence branch curvature without introducing visible capped
segment boundaries.
