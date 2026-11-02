# Walls Greenfield — First Pass: References, Baseline, Open Questions (2026-07-04)

Fourth run of the design-first method (leaves → grass → rocks → walls).
This entry is the pre-interview record: Shawn's sketch, the baseline
probe, reference analysis, architecture facts, and the interview
questions.  Requirements and design come after the interview.

**Shawn's sketch (verbatim intent):** walls are a region with dimensions
the footprint of the wall, then a texture applied (rocky cliff, bricks,
etc.).

Reference assets: `docs/reference/walls/` (annotated README) + the
commercial cliff idiom already in `docs/rocks/rocks-reference-*.png`.
Baseline: `docs/walls/wall-baseline-2026-07-04.png` (spec
`docs/walls/wall-baseline.tile.py`).

---

## Step 1 — baseline judgment (what a wall is TODAY)

A `Region` with `height_mm=30` builds fine: the heightmap extrudes it as
a sheer block whose plan follows the organic boundary curve (nice), the
soil texture lands on the TOP only, and the vertical faces are dead
flat with visible heightmap striations.  Named defects:

1. **Faces are untextured and untexturable in-place** — the terrain is a
   single-valued heightmap; vertical faces exist only as the extrusion
   between adjacent cells.  No heightmap operation can put brick courses
   or strata on them.
2. **The top is soil** — a wall top wants a cap course, walkway,
   crenellations, or broken rubble core, none of which exist.
3. **No base integration** — the wall meets the ground at a hard line
   (no skirt, no rubble at the foot).

## Architecture facts that will shape the design

- Region heights IDW-blend across boundaries; a wall region's height
  jump renders as a sharp cliff when the boundary is crossed directly
  (good) — the slope machinery is water-specific.
- Trees/stones already prove the "separate solid, stamped into
  support/obstacle fields" pattern; stones prove the full surface
  pipeline (blur-remesh, broadband relief, crack lofts, audit gates).
  A wall body could be built as a solid the same way, with the region
  footprint as its plan — faces then get texture by construction
  (per-stone bricks like scatter stones, or engraved joints like
  cracks) rather than by post-hoc displacement.
- Boundary strips (`width_mm > 0`) already express "a run with
  thickness" — candidate authoring surface for wall spines, with
  Region reserved for bastions/outcrops/arbitrary plans.
- `stone_audit.py` and the demo-STL-every-round protocol carry over.

## What the references say (pre-analysis, to be confirmed)

1. A wall is **two faces + a core + a top**; texture wraps ends.
2. **Courses are horizontal** in every family (fieldstone, brick,
   ashlar, cliff strata) — the unifying structure across "textures".
3. **Joints are negative relief** (washes) and stones sit slightly
   proud, each unique (drybrush) — the rocks lessons apply per-unit.
4. **The grid must be broken** — spalls, missing units, ragged top.
5. **Walls seat into terrain** — skirt at the base, rubble at the foot
   of ruins (scatter stones already exist for this).

## Interview questions for Shawn

**Q1 — authoring model.**  Confirm the footprint model: is a wall a
`Region` (arbitrary plan, flood-filled), a `Boundary` strip with
`width_mm` (a spine with thickness — most wall runs), or both?  What
thickness range matters (2–8 mm scale walls? full 35 mm square-wide
cliffs)?

**Q2 — texture families for gen-1.**  Candidate set: rocky cliff
(quarry strata), fieldstone/drystone, coursed cut stone, brick.  Which
one first, and which are must-haves for gen-1 vs later?

**Q3 — wall anatomy.**  Tops: flat cap / walkable walkway / crenellated
parapet / broken-rubble ruin — which for gen-1?  Do minis need to stand
on wall tops (functional width)?  Standard wall heights — is there a
DungeonBlocks-compatible height the system should hit (and should
height quantize to it)?

**Q4 — tiling.**  Walls will run across tile edges (like corridors).
Does the texture need to continue seamlessly across the seam (shared
course heights at edges), or is a clean vertical cut acceptable?

**Q5 — scope check.**  Gen-1 = solid wall runs with textured faces +
top on flat terrain?  Explicitly later: ruin states, gates/doors/
windows/arches, towers, wall-follows-slope, interior rooms?  Any of
those actually gen-1-critical for your table?

Constraints assumed carried over unless contradicted: supportless FDM
(vertical faces are friendly; relief depth just needs overhang-safe
angles), paint-catching relief is an explicit requirement, perf parity
(walls are one solid — should be cheap), DB + OL scales.

---

## Shawn's answers (2026-07-04, mid-research)

1. **Q1/compat:** DungeonBlocks-compatible **by default**, but
   configurable if needed to have freedom.
2. **Q2/families:** do analyses of **all** of the different texture
   families found (→ `docs/reference/walls/commercial-sets-analysis.md`).
3. **Q3/tops:** **flat cap**, but textured like the top of whatever
   the wall texture is — so you see the tops of bricks, for example.
4. **Q4/tiling:** just **butt-join** — no seamless course matching at
   tile seams.
5. **Q5/scope:** gen-1 = **straight runs and corners on flat ground**.

Additional references supplied during the research (all analyzed in
the sets-analysis doc): Hirst Arts (m50 chipped stone, painting
tutorial, fieldstone/big-block/small-block molds — PDFs saved in
`docs/reference/walls/hirst-arts/`), Fat Dragon DRAGONLOCK dungeon
(fdg0160) + caverns (fdg0170), The Dragon's Rest "HQ01" foundation
set and "AP006 High Ground 01" catalog PDFs.
