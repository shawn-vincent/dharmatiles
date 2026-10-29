# shorelab — Meadow Meets Water: Requirements

*v0, 2026-10-28.  This lab combines meadowlab (grass, rocks, bare earth,
hills) with waterlab's swell-chaos water.  It inherits all of their
requirements and only defines the shore and how the parts interact.  The
labs stay standalone and are imported unchanged, apart from small
optional hooks.*

## Scene

A 2 × 2 DB tile:
- a grassy meadow on rises-only hills on one side;
- a shoreline across the middle;
- swell-chaos water on the other side;
- rocks on the land, in the shallows and in the water.

## Requirements

- **REQ-S-1 [S]** One watertight STL per seed that survives the STL reload
  check.  The DB base is included.
- **REQ-S-2 [R] A real bank.**  The land slopes from the meadow down to
  the waterline and on to the underwater bed.  The shoreline is irregular
  and follows the hills, so it is never a straight line.
- **REQ-S-3 [R] The shore varies along its length.**
  - In some stretches grass grows right to the water and laps over it.
  - In others a strip of wet bare mud (0–3 mm) separates grass from
    water, carrying the usual soil texture and pebbles.
  - Reeds grow in a few clumps at the waterline.
- **REQ-S-4 [R] Grass near water leans to it.**  Grass near the waterline
  points at the water or runs along the shore, never away from it.  This
  is the same idea as the tile-rim rule.
- **REQ-S-5 [R] The water reacts to the shore,** using swell-chaos: land
  is a strong emitter.  Rocks standing in the water emit their own
  rings.  Rocks on dry land are only meadow rocks.
- **REQ-S-6 [S] Reeds are simple, chunky blades.**  Each clump has 5–12
  blades with a lemon or diamond cross-section, tapering moderately from a
  chunky base (about 3 mm wide) to a blunt point, 8–13 mm tall.  They fan
  out from a common base, leaning 40° or less from vertical.  There are no heads and no extra sophistication.  Every
  blade roots below the ground or water surface and is fused.
  *(Shawn, 2026-10-28.)*
- **REQ-S-7 [S]** The heights stay coherent:
  - water level 2.6 mm (waterlab);
  - meadow ground at least 3.4 mm (grasslab), with mean grass in the DB
    band;
  - bed about 0.4 mm.

- **REQ-S-8 [R] Shore grass is grasslab's stroke blades on dirt**
  (grasslab REQ-B-*): short separate blades with undercut tips.  Since
  2026-10-29 that is every lab's grass; there is no shore-only override.

## Failure modes

| # | Failure | Looks like |
|---|---|---|
| S1 | **Pasted water** | the water as a flat sheet meeting grass at a vertical cut |
| S2 | **Lawn edge** | grass ending in a clean line at the water everywhere |
| S3 | **Moat** | a uniform mud strip along the whole shore |
| S4 | **Reed fence** | reeds in a regular row, or too many |
| S5 | **Drowned meadow** | grass growing under water |
