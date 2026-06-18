<p align="center">
  <img src="src/dharmatiles/assets/dharmatiles-logo.png" alt="dharmatiles logo" width="180"/>
</p>

# 🪷 dharmatiles

A procedural terrain generator for 3D printable tabletop tiles.

The basic idea is simple:

Instead of sculpting terrain directly, you describe a world in terms of regions, boundaries, materials, and growth rules, and the geometry emerges from that.

The output is a printable STL.

The interesting part is how it gets there.

---

## Why?

Most terrain systems work by placing objects.

Put a rock here.

Scatter some grass there.

Drop a tree somewhere nearby.

That works, but it tends to produce terrain that looks assembled.

I wanted something that behaved more like a place.

A rock should affect the grass around it.

Water should influence nearby terrain.

Boundaries should matter.

Things should interact.

So DharmaTiles treats terrain as a collection of systems rather than a collection of assets.

---

## How it works

At a high level:

```text
tile
  → regions
  → fields
  → growth
  → mesh
  → STL
```

A tile defines regions and boundaries.

Those regions generate fields such as:

* height
* slope
* flow
* distance-to-edge
* influence masks

Layers then use those fields to generate geometry.

Grass grows.

Water shapes itself to terrain.

Stones occupy space and influence nearby systems.

Everything contributes to a single final mesh.

The goal is that geometry is mostly a consequence of rules rather than explicit placement.

---

## Current systems

### Grass

Grass is generated from seed points.

Each blade grows incrementally through a flow field while responding to nearby obstacles and neighboring blades.

The result is not physically accurate, but it tends to produce clusters and directional behavior that feel more natural than random scatter.

### Stones

Stones are generated as terrain features rather than decorative objects.

They occupy space and create influence fields that other systems can react to.

### Water

Water is represented as a terrain layer rather than a separate object.

Shorelines, boundaries, and nearby features all contribute to its final shape.

### Soil

Soil is the boring layer that quietly makes everything else work.

Most terrain transitions, support geometry, and blending behavior ultimately happen here.

---

## Design goals

### Printable first

The primary output is FDM printed terrain.

Everything is designed with printing in mind:

* minimal unsupported geometry
* durable features
* paintable surfaces
* clean silhouettes at tabletop scale

A terrain feature that looks great in Blender but prints poorly is considered a bug.

### Deterministic generation

Given the same inputs and seed, the output should be identical.

Terrain generation is procedural, but it should also be reproducible.

### Emergence over placement

Whenever possible, systems should produce results from local rules rather than explicit instructions.

Instead of saying:

> Put grass here.

the system tries to answer:

> Given this terrain, where would grass end up?

---

## Example tile

```yaml
surface:
  cols: 1
  rows: 1
  seed: 7

regions:
  meadow:
    contains: [0.25, 0.5]
    layers:
      - type: grass
        groups_per_square: 240

  lake:
    contains: [0.75, 0.5]
    layers:
      - type: water
        depth_mm: 2.0

boundaries:
  shoreline:
    from: {edge: top, t: 0.55}
    to:   {edge: bottom, t: 0.45}
    path: organic
    width_mm: 4.0
    layer:
      type: soil
```

The tile description is intentionally small.

Most of the complexity comes from how the systems interact.

---

## Quick start

```bash
pip install -e .

# Generate everything
dharmatiles-gen

# Generate a single tile
dharmatiles-gen --tile src/tiles/example.tile

# Custom output
dharmatiles-gen --tile src/tiles/example.tile -o stl/output.stl
```

Then print it.

Paint it if you want.

Or don't.

The point is that the tile should feel like a place rather than a collection of terrain pieces.

---

## Project structure

```text
src/dharmatiles/
  core/        grids, fields, mesh generation
  layers/      grass, soil, stones, water
  terrains/    composition and generation
  bases/       DungeonBlocks, OpenLOCK

src/tiles/     tile definitions
stl/           generated output
docs/          design notes
```

---

## Status

Very much a work in progress.

Current focus areas:

* terrain grammar
* vegetation systems
* water behavior
* printable detail generation
* OpenLOCK support

The architecture is fairly stable.

The ideas are not.

That's the fun part.
