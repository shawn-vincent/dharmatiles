# Grass Layer — Functional Requirements

Functional requirements for the grass generation layer.
Not a design document — describes *what* the system must do, not *how*.

---

## TL;DR

Seeds carry everything: direction, step size, target length, width, curl — all
set at planting time from the species config plus clump-level random direction
and curl, with per-seed jitter.
Generation runs in two passes:

**Pass 1 — Grow.**  All blades advance one step per round.  Each step samples
the occupancy heightmap at the new footprint, places the spine at
`floor + clearance`, stamps all covered grid cells at the new top-surface z,
and records the spine position.  Step size is much larger than the grid cell, so
each step mostly lands in fresh cells; if the sample overlaps the blade's last
stamp, that stamp is ignored so the blade does not climb itself.  If the required
rise exceeds the rise cap, the blade stops.

**Pass 2 — Build.**  All complete spine paths are converted to meshes in one
batch.  (Future: path smoothing or other per-path processing happens here,
between the two passes.  V1: paths feed directly into mesh construction.)

Different species use different step logic.  The shared contract is:
read heightmap → advance → write heightmap → record path.

---

## 1. Output

**REQ-OUT-1.** The layer produces one or more closed, watertight, manifold triangle
meshes representing grass blades on the tile surface.

**REQ-OUT-2.** No vertex of any grass mesh may lie outside the tile's XY footprint
`[0, tile_w] × [0, tile_h]`.

**REQ-OUT-3.** No grass geometry may protrude outside of the tile's bottom.

**REQ-OUT-4.** The layer writes the top surface z of each placed blade into
`scene.support_z` so subsequent layers that stack on top of grass are placed
correctly.

---

## 2. Blade geometry

**REQ-BLD-1.** Each blade is a single continuous ribbon growing outward from a seed
point in a single direction, tapering to a sharp sub-nozzle point at the tip.

**REQ-BLD-2.** Blade width is set at seed creation time (sampled from a per-species
range) and tapers toward the tip.

**REQ-BLD-3.** Blade length is set at seed creation time (sampled from a per-species
range); the blade grows until it reaches its target length, hits an impassable
obstacle, or reaches the region boundary.

**REQ-BLD-4.** Blade direction and lateral curl are set at seed creation time from
the seed's clump.  Each clump chooses a random base direction and curl from the
species' allowed ranges; each seed inherits those values with per-seed jitter.
The growth algorithm does not consult any direction field; it reads only from
the seed.

**REQ-BLD-5.** Blades lie predominantly in the horizontal plane.  Vertical
displacement is driven only by terrain height and obstacles, not by the blade's
own geometry.  Blades do not stand upright.

**REQ-BLD-6.** The blade body must be printable on an FDM printer with a 0.4 mm
nozzle: no overhangs steeper than ~45° without support.  The final pointed tip
may taper below nozzle width so it slices as a visually sharp end rather than a
blunt rectangle.

**REQ-BLD-7.** Blades must embed slightly below the terrain surface (configurable
root depth) so they appear to grow from the soil rather than sitting on top of it.
At the blade base, all four ribbon corners must be coincident with or below the
raw terrain surface.

---

## 3. Growth model

**REQ-GRW-1.** Growth is step-based: a blade advances one step at a time, where
step length is a physical distance in mm set by the species.  Step length is
independent of the heightmap grid resolution, and must be significantly larger
than the grid cell size so each step lands in cells not yet stamped by any prior
step.

**REQ-GRW-2.** After each step, the blade stamps its physical footprint — all
heightmap cells covered by the blade's cross-section at the new step position —
into the occupancy grid.  The stamp records the blade's top-surface z at each
covered cell.

**REQ-GRW-3.** Different species may have fundamentally different growth behaviors
(e.g. straight ribbon, curling frond, branching reed).  The growth algorithm is
not fixed by the framework.  The common contract is: read from a seed, advance in
steps, read the occupancy heightmap to determine z, write back to the occupancy
heightmap after each step.

---

## 4. Two-pass pipeline

**REQ-PIP-1.** Generation is split into two explicit passes with a clean boundary
between them.

**Pass 1 — Growth.**  All blades are grown simultaneously, round by round.  Each
blade accumulates a list of spine positions (its *path*).  The occupancy heightmap
is updated after every step.  Pass 1 ends when all blades have either reached
their target length or stopped.  No mesh geometry is produced during this pass.

**Pass 2 — Mesh build.**  All paths from Pass 1 are converted to triangle meshes
in a single batch.  The heightmap is not read or written during this pass.

**REQ-PIP-2.** The boundary between passes is a well-defined data handoff: a list
of complete spine paths (and their per-blade parameters) passed from Pass 1 to
Pass 2.  Nothing else crosses the boundary.

**REQ-PIP-3.** Path post-processing (smoothing, resampling, analysis) may be
inserted between Pass 1 and Pass 2 in future without changing either pass.
V1 has no post-processing: paths feed directly into mesh construction.

---

## 5. Terrain following

**REQ-TRN-1.** Each above-ground point along a blade spine sits at or above the
terrain surface at its XY position.  The underground root anchor (REQ-BLD-7) is
exempt — it is intentionally below terrain and inside the tile slab.

**REQ-TRN-2.** On flat terrain with no obstacles a blade lies flat — its spine stays
at a constant, near-zero height above the terrain surface (configurable clearance,
default ≈ 0.01 mm).

**REQ-TRN-6.** Two blades running alongside each other on flat terrain must both lie
flat.  One blade must not force an adjacent parallel blade to jitter up and down.

**REQ-TRN-3.** When a blade encounters an obstacle (stone or previously-placed
blade) it rises smoothly over it.

**REQ-TRN-4.** After passing an obstacle a blade returns to terrain level — it does
not remain elevated.

**REQ-TRN-5.** A blade that cannot clear an obstacle within a configurable rise
limit per unit length stops growing rather than rising steeply.

---

## 6. Obstacle interaction

**REQ-OBS-1.** Blades do not grow through stones. A blade either clears a stone by
rising over it or stops before it.

**REQ-OBS-2.** Blades do not grow through other blades. A blade that would intersect
a previously placed blade rises above it.

**REQ-OBS-3.** A blade does not climb its immediately previous segment.  Each
growing blade tracks only its last footprint stamp; if the next support sample
hits cells raised only by that last stamp, those cells are read from the
pre-grass support surface instead.  Older self-crossings are treated as
obstacles and are limited by the rise cap.

**REQ-OBS-4.** Stacking depth is configurable: blades reject seed positions and stop
growing if the obstacle height above terrain exceeds the configured limit.

---

## 7. Region masking

**REQ-REG-1.** Blades only grow within the grass region defined by `grass_mask`.
A blade stops if its next step would leave the grass region.

**REQ-REG-2.** Blades are seeded only within the grass region. Seeds on stones or
outside the grass region are rejected.

**REQ-REG-3.** The grass region boundary should be fully covered — blades growing
inward from the edge fill any gaps at the perimeter.

---

## 8. Placement / density

**REQ-DEN-1.** Blades are placed in groups (clumps). Group density and group size
range are configurable per square.

**REQ-DEN-2.** Group centres are random sites inside the grass mask.  The grass
region is partitioned into Voronoi-style clump cells so the active clumps cover
the whole mask without requiring grid-aligned placement.

**REQ-DEN-3.** Blades within a group share a common clump direction and curl;
per-seed jitter keeps them from being identical.

---

## 9. Multi-scale compatibility

**REQ-SCL-1.** The layer must produce correct output at both DungeonBlocks scale
(35 mm/square) and OpenLOCK scale (25.4 mm/square) from the same configuration.

**REQ-SCL-2.** All physical dimensions (blade width, length, obstacle clearance) are
specified in mm and scale correctly with `square_mm`.

---

## 10. Performance

**REQ-PRF-1.** Generation time for a 1×1 tile at default density must be under 5 s
on a modern desktop CPU (single-threaded reference).

**REQ-PRF-2.** Generation time must scale no worse than linearly with tile area
(cols × rows).

---

## 11. Mesh quality

**REQ-MSH-1.** Each blade mesh must be closed (no open edges) and watertight.

**REQ-MSH-2.** No two blade meshes from the same tile may share coplanar overlapping
faces (z-fighting).

**REQ-MSH-3.** The combined grass mesh must not contain non-manifold edges or
degenerate (zero-area) faces.

**REQ-MSH-4.** The pointed tip must remain non-zero width internally so each blade
stays a closed solid for boolean union, while still tapering below nozzle width
so it slices as a sharp visual point.

---

## 12. Species

**REQ-SPE-1.** Blade geometry parameters (width range, length range, curl range,
cross-section shape) belong to a **species** — a named template from which seeds
are created.

**REQ-SPE-2.** Multiple species may be active on the same tile simultaneously.
Each species has its own density and geometry parameters.

**REQ-SPE-3.** Each species defines its own growth behavior.  The framework
provides the occupancy heightmap and the step contract (REQ-GRW-3); the species
implements how a seed of that type advances each step.

---

## 13. Configuration

**REQ-CFG-1.** A top-level config holds global parameters (stack limit, clearance,
RNG seed) and a list of species configs.  Each species config holds its own
geometry ranges (width, length, curl, cross-section) and placement parameters
(density and group size).

**REQ-CFG-2.** A fixed random seed produces identical output across runs.
