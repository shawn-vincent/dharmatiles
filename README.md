<p align="center">
  <img src="src/dharmatiles/assets/dharmatiles-logo.png" alt="dharmatiles logo" width="180"/>
</p>

# 🪷 dharmatiles

> *Form arises from constraint.*
> *Beauty arises from form.*
> *And then… you print it.*

---

## What this is

`dharmatiles` is a system for generating **3D-printable terrain tiles**.

Not by sculpting meshes.
Not by kitbashing assets.

But by defining **rules**, and letting terrain emerge.

You don't build tiles.

You describe a **world**, and a tile falls out of it.

---

## What it makes

Given a `.tile` spec, this system produces:

* 🌱 Grass that *grows*, bends, and negotiates with its neighbors
* 🪨 Stones that occupy space and influence everything around them
* 🌊 Water that carries structure: waves, shore compression, wake
* 🟫 Soil that supports, erodes, and quietly does most of the work

All fused into a **single printable solid**, compatible with:

* DungeonBlocks
* OpenLOCK (experimental, slightly rebellious)

Output: STL files that don't just print — they **feel right in the hand**.

---

## What it actually is (under the hood)

A **terrain grammar engine** pretending to be a tile generator.

```
spec → regions → fields → growth → mesh → object
```

Everything flows from a few simple ideas:

* A **height field** defines reality
* A **flow field** defines direction
* **Layers** express behavior
* Geometry is a *consequence*, not an input

---

## Design philosophy

### 🧘 Constraint is the source

Tiles are small.
Printers are blunt.
Plastic is unforgiving.

Good.

The system embraces that.

Everything is built so that:

* it prints cleanly
* it paints well
* it survives scale

No illusions. Just form.

---

### 🌊 Deterministic emergence

No chaos for its own sake.

* Same input → same output
* Every seed is accountable
* Every blade of grass can be traced back to its cause

This is not randomness.

It's **causality made visible**.

---

### 🌿 Growth, not placement

Grass is not "scattered."

Each blade:

* has a seed
* has intent
* grows step-by-step
* avoids obstacles
* settles where it must

It does not ask permission.
It simply **finds a way**.

---

### 🪨 Everything participates

Nothing is decoration.

* Stones influence growth
* Terrain shapes water
* Boundaries define behavior
* Even empty space matters

The system doesn't assemble pieces.

It lets them **interfere with each other**.

---

## Quick start

```bash
pip install -e .

# Generate everything
generate-tile-stl

# Generate one tile
generate-tile-stl --spec "src/tiles/soil+grass.tile"

# Custom output
generate-tile-stl --spec src/tiles/foo.tile -o stl/custom.stl
```

Then:

* Slice it
* Print it
* Paint it
* Hold it

That last step is important.

---

## Project structure

```
src/dharmatiles/
  core/        primitives: grid, flow, mesh, config
  layers/      grass, soil, stones, water
  terrains/    composition + CLI entry
  bases/       DungeonBlocks, OpenLOCK

src/tiles/     terrain specs (this is where the magic starts)
stl/           generated output
docs/          design notes (the real story)
```

---

## Tile specs (the language)

You don't sculpt terrain.
You define **regions** and **boundaries**.

Example:

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

That's it.

Everything else is derived.

---

## What this is for

* Tabletop terrain
* Dioramas
* Procedural art
* Or just… making things that didn't exist before

You don't need a game.

You just need the moment where you look at a tile and think:

> "Yeah. That's a place."

---

## Status

Actively evolving.

* OpenLOCK support: **experimental**
* Terrain grammar: **emerging**
* Water + vegetation systems: **deepening**
* Perfection: **not the goal**

---

## Final note

This project exists because:

> doing something *because it has no point*
> turned out to be the point

---

If you use it, extend it, or print something from it…

take a second before you move on.

Look at the tile.

That's the whole thing.
