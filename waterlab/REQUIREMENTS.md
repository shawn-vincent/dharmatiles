# waterlab — Fantasy Water Scene: Requirements

*v0, 2026-10-28.  Exploratory: Shawn hasn't found a water he likes yet,
so this lab compares several generators on one fixed scene.  Inputs:
- dharmatiles' current water (`src/dharmatiles/layers/water.py`,
  `docs/design/water-surface-model.md`), the closest to liked so far;
- the ChatGPT design-space handoff (2026-10-28), whose negative design
  space is summarised in §4.*

## 1. Scene (fixed across all options)

- **Tile**: a 2 × 2 DungeonBlocks tile (70 × 70 mm) on the standard socket base.
- **Land**: a rise along one side, with an irregular shoreline and a
  sloped bank into the water.  The land is plain ground; it is context,
  not the subject.
- **Rock**: one faceted rock (from rocklab) standing in the water.
- **Water**: everything else.  The water surface is the subject.

## 2. Requirements

- **REQ-W-1 [S] One STL per option.**  Each is one watertight body that
  survives the STL reload check (float32, merge by position), with the DB
  base.  Deterministic per seed.
- **REQ-W-2 [S] Single-material sculpture.**  The water is sculpted
  relief on the solid top surface, a single-valued height field with no
  overhangs, printable supportless with a 0.4 mm nozzle.
  - Crests are at least about 1 mm wide.
  - No needles and no sub-0.3 mm detail.
- **REQ-W-3 [S] Heights.**  The calm water level is about 2.6 mm above the
  terrain datum.  Relief stays within about −0.8…+1.2 mm of it.  The land
  rises to about 6 mm (the DB grass band).
- **REQ-W-4 [R] Reads as fantasy water before paint.**  It is exaggerated
  and sculptural, but recognisably water.  A monochrome render must
  already read as water.
- **REQ-W-5 [R] Obstacles organise the water.**  The rock and the shore
  generate structure: wavelets that hug the contact, fade outward and
  break irregularly.  The strongest relief is near obstacles.
- **REQ-W-6 [R] Open water is quieter, but still sculptural.**  There is
  enough relief to drybrush.  It is statistically direction-neutral: no
  global orientation, and acceptable after rotating 90°.
- **REQ-W-7 [R] One vocabulary.**  Open water, rock contact and shore
  contact are one continuous surface.  There are no pasted-on decals and
  no seams between treatments, and where the rock and shore fields meet,
  they merge without spikes.
- **REQ-W-8 [R] Drybrush affordance.**  Broad, rounded crests with
  smoother, wider troughs: brush-catching forms, not engraved lines.
- **REQ-W-9 [S] Rim.**  The water meets the tile edge at a consistent
  calm level, so adjacent water tiles butt cleanly.

## 3. Options to compare (v0)

Each option is one water generator applied to the identical scene:

| Option | Idea | Source |
|---|---|---|
| **legacy** | Current dharmatiles water: band-limited noise plus scattered-segment sine ripples on a smoothed distance-from-obstacle field | the closest-to-liked baseline |
| **lapping** | A few broad, rounded, *broken* crests following the distance from rock ∪ shore, with spacing that widens and height that decays outward, over quiet, low, broad swell | ChatGPT's best untested idea plus the dharmatiles ripple lineage |
| **crescents** | The lapping crests near obstacles, plus sparse finite crescent crests scattered at random orientations in open water | the ChatGPT B/D families |

More options are added as iteration suggests.

## 4. Failure modes (negative design space, inherited)

| # | Failure | Looks like |
|---|---|---|
| W1 | **Cells / bowls** | a honeycomb or Voronoi network of rounded depressions with bright rims |
| W2 | **Targets** | perfect concentric rings; raindrop rings in open water |
| W3 | **Wave rows** | parallel directional swells or corduroy (seen in dharmatiles' wave-train model) |
| W4 | **Swirls** | spiral or whirlpool ornament |
| W5 | **Busy lines** | many thin ridges everywhere; reads as stormy foam |
| W6 | **Lumpy porridge** | isotropic noise as the main generator, with no structure |
| W7 | **Pasted decal** | the ripple zone visibly bounded and stuck onto a different background |
| W8 | **Clones** | options that are only scale variants of each other |

## 5. Process

Exploratory, with Shawn's eye as the judge:
1. Claude generates the options and self-checks them with monochrome
   renders, rejecting obvious failures.
2. Shawn looks at a shortlist.
3. Together we choose a direction, then iterate.
