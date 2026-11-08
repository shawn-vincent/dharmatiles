# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** builds procedural 3D-printable terrain tiles as STL files from short `.dtile` documents (the tile language, KDL v2 underneath). Each tile has a DungeonBlocks-compatible socket-peg underside (or an OpenLOCK clip base), dirt with clods and pebbles, rocks, grass blades, water, walls, floors and trees. Tiles are for tabletop gaming (35 mm square grid, DungeonBlocks standard; OpenLOCK is 25.4 mm). Every build runs through a printability gate: watertight after the float32 reload, no lost parts, overhang held to 55 degrees (small budgeted patches to 65 and past it).

dharmatiles is a pure Rust program (the `dharmatiles` binary). The only other code is `extras/`: a paint organizer, its stand and two STL helpers, standalone Python scripts that are not part of the program. The Python library, the labs, the parity harness and the old design history were deleted on 4 Nov 2026; the tag `before-rust-only` holds them. New elements are designed in Rust.

## Commands

```bash
cargo build --release                      # the dharmatiles binary: target/release/dharmatiles
dharmatiles build lib:tiles/ground/1x1-grass.dtile          # one library tile  -> out/db/ground/1x1-grass-db.stl
dharmatiles build my-tile.dtile --system ol --png            # a file, for OpenLOCK, with four preview PNGs (overhang red); --printer NAME picks a profile from library/printers/ or $DT_HOME/printers/
dharmatiles build my-tile.dtile --3mf                        # also a 3MF painted by material; a Bambu Studio project when ~/dharmatiles/bambu.3mf (or --bambu-template) exists
dharmatiles build --all [--system ol] [--printer fine]       # every library tile for one target -> out/<system>[-<printer>]/<group>/ (one build is one target; there is no `both`)
dharmatiles check my-tile.dtile                              # read and plan only
dharmatiles elements [name]                                # the elements and their controls
dharmatiles watch my-tile.dtile                              # rebuild on every save
dharmatiles preview my-tile.dtile --open                     # live painted 3D view in the browser (127.0.0.1:7878); overhang red; also takes .stl
dharmatiles                                                 # no command = preview --open: browse My tiles ($DT_HOME, else ~/dharmatiles) and the Library; folders as args work too

cargo test --release --workspace            # all tests (insta golden hashes included), under 5 minutes
cargo test --release -p dt-eval --test openlock -- --ignored   # release check: every library tile through the gate on both systems
cargo test --release -p dt-eval --test baseline -- --ignored   # every library tile on DungeonBlocks against tests/baseline/library-db.tsv (hash, triangles, repairs, overhang); DT_BASELINE=write rewrites it
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo xtask sheet <tile|name> [--seeds …]   # contact sheet: four views per seed, seeds 1-4 -> out/sheet/<tile>/contact-sheet.png
cargo xtask pictures [group …]              # rebuild the element pages' pictures from docs/elements/demo/
cargo xtask docs [--check]                  # fill docs/elements/ pages' controls tables from the element schema
cargo xtask release 0.10.0 "<summary>"      # number the changelog's Unreleased entry, bump Cargo.toml, check, commit, tag v0.10.0, push
```

`lib:` paths read `library/` from disk: this checkout's when run inside it (new and edited library tiles count at once, no rebuild), else `$DT_LIBRARY`, else the copy installed beside the program. Nothing is compiled in.

Output goes to `out/` (git-ignored).

CI is off. Run the tests locally on this Mac only; do not wait on or re-enable GitHub Actions. Linux, Windows and cross-OS hashes are a later stretch goal.

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
| `dt-geom` | Kernels: mesh backend (Manifold via `manifold-csg`), rays (obvhs), keyed seeds and noise, grids, distance transform, adaptive lift, convex hulls. No element knowledge. |
| `dt-kit` | The vocabulary elements are written in: `Scene` of tagged solids, `Field` and its morphology, `scatter`, `loft`, `extrude`, `carve`, `union`, forms (level sets, hulls, relief), graphs (trees), `stand`, rays, the audit gate (`audit`), the STL writer with its build record, the CPU preview renderer. |
| `dt-elements` | The built-in elements, each written only in `dt-kit`'s public operations: `ground`, `dirt`, `rock` (and `stone`, `boulder`, `pebble`, `scatter`), `water`, `grass`, `reeds`, `flowers`, `cliff`, `wall`, `floor`, `tree`, and the DungeonBlocks / OpenLOCK connectors. |
| `dt-doc` | KDL v2 documents: decoding with spans and `miette` errors, the element schema (`elements.rs`), edits. |
| `dt-eval` | Reads a document, plans it (`plan.rs` orders the stack by what each element reads and writes), builds it into a `Scene`, audits, exports. Library tiles are bundled from `library/`. |
| `dharmatiles` | The CLI. |

Also: `xtask/` (`docs`, `pictures`, `release`, `sheet`), `library/` (`.dtile` documents: `tiles/`, `parts/`, `printers/`, `palettes/`), `web/preview/` (the preview's page), `docs/` (the site, HTML), `extras/` (Python, not part of the program).

### How a build runs

`document -> dt-doc (decode, check against the element schema) -> dt-eval::check (plan: surfaces first, then lift, then solids, in the order each element's reads and writes allow) -> build each element into the Scene -> union -> audit -> STL`. An element says what it reads and writes in `Element::plan`; the scheduler orders the stack and refuses reads nothing can satisfy (a grass line listed before the rocks it must grow round is fine: the plan runs the rocks first).

Every leaf solid takes a slot reserved in sequence, so its source id (and so the material of every output triangle, and every hash) is fixed by index whatever thread builds it. Builds are identical whatever the threads; the golden tests check it by building each golden tile again on two threads (`crates/dt-eval/tests/golden.rs`, insta snapshots in `crates/dt-eval/tests/snapshots/`).

### Tile documents

```kdl
tile size="1x1" seed=97 {
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

Lines and fill points make regions; the stack paints surfaces (`ground`, `dirt`, `floor`, `water`) and stands things on them (`rock`, `scatter`, `grass`, `reeds`, `wall`, `tree`). Sizes are mm at the 35 mm square and scale with the system. Read `docs/language.html` for the language and `docs/elements/` for each element.

### Library

`library/tiles/<group>/…kdl` are the shipped tiles, `library/tiles/test/` are the golden and language tests' tiles, `library/parts/` are shared definitions (`grass`, `trees`, `edges`, `levels`).

## Standing rules

- **Overhang is red.** 55 degrees is every element's target. 55-65 degrees is the band, painted orange, allowed up to 0.1% of the downward-facing area; over 65 is red, allowed up to another 0.1%; more fails. Both angles are the printer profile's `overhang` and `steepest` (library/printers/default.dtile). The printability gate is a gate: never loosen a threshold or cull geometry to pass it; fix the geometry where it is made.
- **KDL is v2 only.**
- **One visual system.** The docs site and the preview share `docs/assets/theme.css` (tokens, self-hosted fonts, lotus, controls); the preview serves it from inside the program. New colours, fonts or controls go there, never into one page or the preview alone. Red and orange mean overhang only; tiles are shown on the plate (#D9E0EB). See `docs/standards/visual-system.html`.
- **Docs are HTML** under `docs/`, never `.md` copies; lead with what a tile user needs; no commercial product images.
- **The site is current state only.** It says what the program is and does now: the reference, the specification and the rules. No decision logs, histories, rulings with dates, lab write-ups or reference-photo galleries; git history holds how we got here. The one exception is the changelog, `docs/changelog.html`, a hand-written static page (no feed, nothing generated); changes go under its **Unreleased** entry at the top as they land; only when Shawn asks for a release does that entry get a number and date and `version` in `Cargo.toml` get bumped, always with `cargo xtask release <version> "<summary>"`, which also tags every release `v<version>` (`--no-tests` while a known test failure stands). When a rule changes, update `docs/standards/rules.html` (project-wide), the element page's Rules list, or the language guide's rules, in place. The Rust code is the reference for how things are implemented; pages link to it rather than restate it.
- **The site is the tile language's reference.** `docs/language.html` is the guide; `docs/elements/<name>.html` is one page per element (every node a document can write). The code uses the same word: the `Element` trait, the `dt-elements` crate, the element schema in `dt-doc/src/elements.rs`. Each element page's controls block is generated: run `cargo xtask docs` after changing an element's schema, and add a page for any new element (the generator refuses an element without one). The preview links each stack entry to its element page.
- **Pronouns:** use they/them for Shawn.
- **Contact sheets:** make one with `cargo xtask sheet` and look at it yourself at full resolution before calling an element done. Element pictures come from `cargo xtask pictures`; look at them too.
- **Builds are heavy.** Avoid many release tree or wall builds in parallel.
- Before asking Shawn a design question, read `docs/standards/rules.html`, the element page's Rules, the language guide's rules and the other `docs/standards/` pages. Many answers are settled (for example the edge level is 3.4 mm). Older reasoning is in the tag `before-rust-only` (`docs/rust/plan.html` there has the dated decisions log).
