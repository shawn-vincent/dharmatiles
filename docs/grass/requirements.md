# Grass Layer — Functional Requirements

Functional requirements for the grass generation layer.
Not a design document — describes *what* the system must do, not *how*.

---

## 1. Output

**REQ-OUT-1.** The layer produces one or more closed, watertight, manifold triangle
meshes representing grass blades on the tile surface.

**REQ-OUT-2.** No vertex of any grass mesh may lie outside the tile's XY footprint
`[0, tile_w] × [0, tile_h]`.

**REQ-OUT-3.** No grass geometry may protrude outside of the tile's bottom.

**REQ-OUT-4.** The layer writes the final grass occupancy (top surfaces of all blades)
into `scene.support_z` so subsequent layers can build on it.

---

## 2. Blade geometry

**REQ-BLD-1.** Each blade is a single continuous ribbon growing outward from a seed
point, tapering to a narrower tip.

**REQ-BLD-2.** Blade width is configurable (min/max range); each blade has a single
width sampled at seed time that tapers near the tip.

**REQ-BLD-3.** Blade length is configurable (min/max range); each blade grows until
it reaches its target length, hits an impassable obstacle, or reaches the region
boundary.

**REQ-BLD-4.** Blades lean/curve in the direction of the tile's flow field.
Per-blade direction jitter is configurable.

**REQ-BLD-5.** Blades have a configurable lateral curl (curvature along their
length), driven by the flow field with per-blade variation.

**REQ-BLD-6.** The blade cross-section must be printable on an FDM printer with a
0.4 mm nozzle: minimum feature width ≥ 0.4 mm, no overhangs steeper than ~45°
without support.

**REQ-BLD-7.** Blades must embed slightly below the terrain surface (configurable
root depth) so they appear to grow from the soil rather than sitting on top of it.

---

## 3. Terrain following

**REQ-TRN-1.** Each point along a blade spine sits at or above the terrain surface
at its XY position.

**REQ-TRN-2.** On flat terrain with no obstacles a blade lies flat — its spine stays
at a constant, near-zero height above the terrain surface.

**REQ-TRN-3.** When a blade encounters an obstacle (stone or previously-placed
blade) it rises smoothly over it.

**REQ-TRN-4.** After passing an obstacle a blade returns to terrain level — it does
not remain elevated.

**REQ-TRN-5.** A blade that cannot clear an obstacle within a configurable rise
limit per unit length stops growing rather than rising steeply.

---

## 4. Obstacle interaction

**REQ-OBS-1.** Blades do not grow through stones. A blade either clears a stone by
rising over it or stops before it.

**REQ-OBS-2.** Blades do not grow through other blades. A blade that would intersect
a previously placed blade rises above it.

**REQ-OBS-3.** A blade does not climb its own body — it must not treat its own
previously-placed segments as obstacles.

**REQ-OBS-4.** Stacking depth is configurable: blades reject seed positions and stop
growing if the obstacle height above terrain exceeds the configured limit.

---

## 5. Region masking

**REQ-REG-1.** Blades only grow within the grass region defined by `grass_mask`.
A blade stops if its next step would leave the grass region.

**REQ-REG-2.** Blades are seeded only within the grass region. Seeds on stones or
outside the grass region are rejected.

**REQ-REG-3.** The grass region boundary should be fully covered — blades growing
inward from the edge fill any gaps at the perimeter.

---

## 6. Placement / density

**REQ-DEN-1.** Blades are placed in groups (clumps). Group density, group size
range, and spatial spread within a group are configurable per square.

**REQ-DEN-2.** Group centres are distributed with jittered-grid spacing so coverage
is roughly uniform without being regular.

**REQ-DEN-3.** Blades within a group share a common flow direction and curl sign;
per-blade jitter keeps them from being identical.

---

## 7. Multi-scale compatibility

**REQ-SCL-1.** The layer must produce correct output at both DungeonBlocks scale
(35 mm/square) and OpenLOCK scale (25.4 mm/square) from the same configuration.

**REQ-SCL-2.** All physical dimensions (blade width, length, obstacle clearance) are
specified in mm and scale correctly with `square_mm`.

---

## 8. Performance

**REQ-PRF-1.** Generation time for a 1×1 tile at default density must be under 5 s
on a modern desktop CPU (single-threaded reference).

**REQ-PRF-2.** Generation time must scale no worse than linearly with tile area
(cols × rows).

---

## 9. Mesh quality

**REQ-MSH-1.** Each blade mesh must be closed (no open edges) and watertight.

**REQ-MSH-2.** No two blade meshes from the same tile may share coplanar overlapping
faces (z-fighting).

**REQ-MSH-3.** The combined grass mesh must not contain non-manifold edges or
degenerate (zero-area) faces.

---

## 10. Configuration

**REQ-CFG-1.** All tunable parameters (density, width, length, curl, rise limit,
stack limit, etc.) are declared in a single config dataclass with documented units
and defaults.

**REQ-CFG-2.** A fixed random seed produces identical output across runs.
