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
algorithm is generic; it just follows the seed's instructions.  Direction and
curl come from the seed's clump: each clump chooses a random base direction and
curl, then each seed receives a small per-seed jitter.  After that the seed is
self-contained.

Different **species** can coexist on the same tile.  A species is a template
for seeds: it sets the ranges for blade width, length, curl intensity, and shape.
Grass, reeds, ground cover — each is a different species with different parameters.

Species can also have fundamentally different **growth behaviors** — not just
different parameter values.  A flat grass blade, a curling reed, a spreading
ground cover — these may grow in structurally different ways.  What they share
is the contract: start from a seed, advance in steps, read and write the
occupancy heightmap.

---

## Clumping

Seeds are not scattered uniformly.  They are planted in **groups** (clumps), with
multiple seeds placed close together near a group centre.

- Group centres are random sites inside the grass mask.  Their Voronoi-style
  cells partition the whole grass region so every part of the mask belongs to
  one clump.
- Seeds within a group are scattered across that group's cell, not just within
  a fixed radius around the centre.
- Each group chooses a random base growth direction and curl.  All seeds in the
  group inherit those values with small per-seed jitter, so the clump looks
  coherent without becoming uniform.

The density of groups and the number of seeds per group are tunable per species.

---

## Growth

Each blade grows outward from its seed point one step at a time, following its
baked-in direction and curling gently to one side.

**Step size is a physical distance in mm — it has nothing to do with the
heightmap grid resolution.**  A step might be 0.5 mm; the heightmap cell might
be 0.14 mm.  After each step, every grid cell covered by the blade's physical
footprint at that position is stamped into the occupancy grid.  The grid is a
rasterised record of where blades have been placed; the blade itself lives in
continuous mm-space.

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

## The self-trail non-problem

Because a blade's sampled footprint may overlap the segment it just stamped,
each blade remembers only its **last stamp**.  On the next step, cells raised
only by that last stamp are ignored and the blade samples the raw terrain/stone
support instead.  This lets the blade flop back down immediately after crossing
its own previous footprint.

The only edge case is a blade that curls so tightly it doubles back across its
older path.  Older self-crossings are treated as obstacles; if the blade cannot
clear them within the rise limit, it stops.

---

## Region boundaries

Blades only grow within the designated grass region.  If the next step would leave
the grass region (into a water zone, bare soil, or the tile edge), the blade stops.

The boundary should look clean — no obvious gap of bare terrain just inside the
edge.  Extra seeds planted along the inner edge of the boundary fill this in.

---

## Two passes, not one

Generation happens in two distinct phases.

**Pass 1 — Grow.**  All blades advance simultaneously, round by round.  Each
blade just builds up a list of spine positions (its path).  The occupancy
heightmap grows as blades stamp their positions.  Nothing is rendered yet.

**Pass 2 — Build.**  Once all paths are complete, every blade's path is turned
into a mesh in one batch.  No heightmap reads happen here — this pass only
knows about geometry.

The clean split exists so that future work can insert path processing between
the two passes — smoothing jagged paths, tapering, analysing blade length
distributions, whatever.  V1 skips all of that; paths go straight from growth
to mesh.

---

## What a finished tile looks like

- Blades lie flat, covering most of the grass region.
- In any small area, blades lean in roughly the same direction, with gentle
  variation — like a field caught in a light breeze.
- Clumps are visible: groups of 20–30 blades lying close together, with slight
  gaps between groups that make the coverage feel organic rather than uniform.
- Stones poke out through the grass; blades curve around them or rise over them.
- Near the edge of the grass region, blades grow inward and fill the boundary.
- Blades taper to a sharp sub-nozzle point at the tip.  The widest part is at
  the root.

---

## What it should NOT do

- Blades should not stand upright or point at steep angles.
- Blades should not form obvious rows, grids, or radial patterns.
- Blades should not pile up into tall stacks on flat terrain.
- Blades should not staircase upward across flat ground.
- Blades should not poke through the tile boundary or through stones.
- Adjacent blades running alongside each other should lie flat — not jitter
  up and down.
