# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** builds procedural 3D-printable terrain tiles as STL files from short KDL documents. Each tile has a DungeonBlocks-compatible socket-peg underside (or an OpenLOCK clip base), dirt with clods and pebbles, rocks, grass blades, water, walls, floors and trees. Tiles are for tabletop gaming (35 mm square grid, DungeonBlocks standard; OpenLOCK is 25.4 mm). Every build runs through a printability gate: watertight after the float32 reload, no lost parts, no overhang past 55 degrees.

The program is Rust (the `dharmatiles` binary). The Python library in `src/dharmatiles` is the labs' support library and is no longer where tiles are made; the labs (`dirtlab`, `rocklab`, `waterlab`, `grasslab`, `meadowlab`, `shorelab`, `clifflab`) are still where new looks are designed. Until Shawn approves deleting them (phase P6), `src/tiles/` still holds the Python specs of the tiles whose Rust twins are not yet signed off; those are history, not the workflow.

## Commands

```bash
cargo build --release                      # the dharmatiles binary: target/release/dharmatiles
dharmatiles build lib:tiles/ground/1x1-grass.kdl          # one library tile  -> out/db/ground/1x1-grass-db.stl
dharmatiles build my-tile.kdl --system ol --png            # a file, for OpenLOCK, with four preview PNGs (overhang red)
dharmatiles build --all --system both                      # every library tile, both systems -> out/{db,ol}/<group>/
dharmatiles check my-tile.kdl                              # read and plan only
dharmatiles looks [name]                                   # the looks and their controls
dharmatiles watch my-tile.kdl                              # rebuild on every save

cargo test --release --workspace            # all tests (insta golden hashes included)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo xtask parity <look>                   # Python lab beside Rust, seeds 1-4 -> out/parity/<look>/contact-sheet.png
```

Output goes to `out/` (git-ignored). Python tile output in `stl/` is the frozen reference: **never regenerate it, and never pass `-o` / `--output` to the old Python generator.**

CI is off. Run the tests locally on this Mac only; do not wait on or re-enable GitHub Actions. Linux, Windows, wasm and cross-OS hashes are phase P9, a stretch goal.

## Spatial Terminology (canonical — do not deviate)

| Level | Name | Size | Key symbols |
|---|---|---|---|
| Full printed output | **tile** | cols × rows × square_mm | `tile` node, `Frame`, `out/` |
| One 35 mm DungeonBlocks unit | **square** | 35 × 35 mm (or 25.4 for OL) | `cols`, `rows`, `per-square` |
| One heightmap subdivision | **cell** | square_mm / cells_per_square | `Frame::step_mm`, `Field` |

**Never call a 35 mm unit a "tile".** Densities are `per-square`.

## Architecture

Six crates under `crates/`:

| Crate | Role |
|---|---|
| `dt-geom` | Kernels: mesh backend (Manifold via `manifold-csg`), rays (obvhs), keyed seeds and noise, grids, distance transform, adaptive lift, convex hulls. No look knowledge. |
| `dt-kit` | The vocabulary looks are written in: `Scene` of tagged solids, `Field` and its morphology, `scatter`, `loft`, `extrude`, `carve`, `union`, forms (level sets, hulls, relief), graphs (trees), `stand`, rays, the audit gate (`audit`), the STL writer with its build record, the CPU preview renderer. |
| `dt-looks` | The built-in looks, each written only in `dt-kit`'s public operations: `ground`, `dirt`, `rock` (and `stone`, `boulder`, `pebble`, `scatter`), `water`, `grass`, `reeds`, `wall`, `floor`, `tree`, and the DungeonBlocks / OpenLOCK connectors. |
| `dt-doc` | KDL v2 documents: decoding with spans and `miette` errors, the look schema (`looks.rs`), edits. |
| `dt-eval` | Reads a document, plans it (`plan.rs` orders the stack by what each look reads and writes), builds it into a `Scene`, audits, exports. Library tiles are bundled from `library/`. |
| `dharmatiles` | The CLI. |

Also: `xtask/` (the parity harness), `library/` (KDL documents: `tiles/`, `parts/`, `printers/`), `tests/parity/` (`run_lab.py`, the Python side of the harness), `docs/` (the site, HTML).

### How a build runs

`document -> dt-doc (decode, check against the look schema) -> dt-eval::check (plan: surfaces first, then lift, then solids, in the order each look's reads and writes allow) -> build each look into the Scene -> union -> audit -> STL`. A look says what it reads and writes in `Look::plan`; the scheduler orders the stack and refuses reads nothing can satisfy (a grass line listed before the rocks it must grow round is fine: the plan runs the rocks first).

Every leaf solid takes a slot reserved in sequence, so its source id (and so the material of every output triangle, and every hash) is fixed by index whatever thread builds it. Builds are identical at 1, 4 and 16 threads; the golden tests check it (`crates/dt-eval/tests/golden.rs`, insta snapshots in `crates/dt-eval/tests/snapshots/`).

### Tile documents

```kdl
tile water+grass size="1x1" seed=97 {
    line shore wobble=5 { through "0,0.38" "1,0.42" }
    region pool { bounded-by shore; fill "0.5,0.25" }
    region bank { bounded-by shore; fill "0.5,0.75" }
    stack {
        dirt in=bank
        grass in=bank
        ground in=pool level="water - 2.2"
        scatter in=pool count=2 { rock size="6..10" }
        water
    }
}
```

Lines and fill points make regions; the stack paints surfaces (`ground`, `dirt`, `floor`, `water`) and stands things on them (`rock`, `scatter`, `grass`, `reeds`, `wall`, `tree`). Sizes are mm at the 35 mm square and scale with the system. Read `docs/rust/language.html` for the language and `docs/looks/` for each look.

### Library

`library/tiles/<group>/…kdl` are the shipped tiles (43; 42 build, the cliff run waits for P7), `library/tiles/test/` are the parity and golden tests' tiles, `library/parts/` are shared definitions (`grass`, `trees`, `edges`, `levels`).

## Standing rules

- **Overhang is red.** Every render paints overhang faces red at 55 degrees. Trees only: 55-65 degrees is orange, within the tree's band budget; over 65 is red and fails. Every other look keeps 55 as a hard limit. The printability gate is a gate: never loosen a threshold or cull geometry to pass it; fix the geometry where it is made.
- **KDL is v2 only.**
- **Docs are HTML** under `docs/`, never `.md` copies; lead with what a tile user needs; no commercial product images. `docs/status.json` is kept current on `main`.
- **Pronouns:** use they/them for Shawn.
- **Contact sheets:** look at each one yourself at full resolution before calling a look done.
- **Builds are heavy.** Avoid many release tree or wall builds in parallel.
- Before asking Shawn a design question, search the decisions log in `docs/rust/plan.html` and the pages in `docs/standards/`. Many answers are settled (for example the edge level is 3.4 mm, Default 12).

## The labs and the Python library

The labs are standalone Python experiments (`grasslab/`, `rocklab/`, ...) that import `src/dharmatiles` (`core/mesh`, `stone/*`, `bases/*`). Keep them working; do not port Rust ideas back into them. A look approved in a lab gets its own port phase using `cargo xtask parity`. The rest of `src/dharmatiles` (`layers/`, `grass/`, `scatter/`, `terrains/`) is frozen and unused by the Rust program (see `docs/rust/plan.html`, phase P6).
