# Grass — What It Should Do

A plain-language description of the intended grass behavior.
No implementation details — just the mental model.

---

## The big picture

The grass layer covers terrain with flat, lying-down grass blades that look like
a field of grass that has been blown or pressed down by wind.  Blades are not
upright.  They lie almost flat on the ground, growing outward from a root point,
curving gently to one side, and tapering to a point at the tip.

The result should look natural at tabletop gaming scale (roughly 28 mm figures):
patches of grass clumped together, all leaning roughly the same direction in a
given area, covering the terrain surface without obvious regularity.

---

## Seeds and species

Each blade starts as a **seed** — a point on the terrain with a direction, a
species, and a full set of geometry parameters baked in at creation time.

**A seed knows everything about its blade before growth starts:**
direction, curl, target length, width, cross-section shape.  The growth
algorithm is generic; it just follows the seed's instructions.

Different **species** can coexist on the same tile.  A species is a template
for seeds: it sets the ranges for blade width, length, curl intensity, and shape.
Grass, reeds, ground cover — each is a different species with different parameters.
All species share the same growth algorithm.

---

## Clumping

Seeds are not scattered uniformly.  They are planted in **groups** (clumps), with
multiple seeds placed close together near a group centre.

- Group centres are distributed across the tile using a slightly randomised grid —
  spread out enough to cover the whole tile, irregular enough to look natural.
- Seeds within a group are scattered within a small radius of the centre.
- All seeds in a group share a common growth direction and curl sign, with small
  per-seed jitter.  This makes a clump look like it belongs together.

The density of groups, the size of groups, and the spread of seeds within a group
are all tunable per species.

---

## Growth

Each blade grows outward from its seed point one step at a time, following its
baked-in direction and curling gently to one side.

The blade **reads the terrain and occupancy surface** at each step to find its
Z height.  The rule is simple:

> Lie as flat as possible.  Rise only when something is in the way.  Drop back
> as soon as it's clear.

On open flat terrain, a blade lies almost perfectly flat — its spine is just a
hair's width above the ground.

When a blade reaches a stone, it rises over it (up to a configured slope limit),
then drops back to the ground on the far side.

When a blade reaches another blade that's already lying there, it rises above it
and continues.

If the obstacle is too steep or the stack is already too deep, the blade stops
growing rather than piling up steeply.

---

## The self-trail problem

One subtlety: a blade must not treat its own previous segments as obstacles.
As a blade grows, it stamps its presence into the occupancy surface.  If it
then reads that stamp at the next step, it will think something is blocking it
and rise — which creates the next stamp even higher, and so on.  The blade
would staircase upward on flat terrain.

The growth algorithm must distinguish *its own prior stamps* from *real external
obstacles* (other blades, stones).  Only external obstacles trigger a rise.

---

## Region boundaries

Blades only grow within the designated grass region.  If the next step would leave
the grass region (into a water zone, bare soil, or the tile edge), the blade stops.

The boundary should look clean — no obvious gap of bare terrain just inside the
edge.  Extra seeds planted along the inner edge of the boundary fill this in.

---

## What a finished tile looks like

- Blades lie flat, covering most of the grass region.
- In any small area, blades lean in roughly the same direction, with gentle
  variation — like a field caught in a light breeze.
- Clumps are visible: groups of 20–30 blades lying close together, with slight
  gaps between groups that make the coverage feel organic rather than uniform.
- Stones poke out through the grass; blades curve around them or rise over them.
- Near the edge of the grass region, blades grow inward and fill the boundary.
- Blades taper to a fine tip.  The widest part is at the root.

---

## What it should NOT do

- Blades should not stand upright or point at steep angles.
- Blades should not form obvious rows, grids, or radial patterns.
- Blades should not pile up into tall stacks on flat terrain.
- Blades should not staircase upward across flat ground.
- Blades should not poke through the tile boundary or through stones.
- Adjacent blades running alongside each other should lie flat — not jitter
  up and down.
