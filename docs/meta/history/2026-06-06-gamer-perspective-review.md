# Gamer Perspective Review
*2026-06-06*

Reviewer persona: **Reeve** — nonbinary, mid-30s, runs a homebrew 5e campaign,
has a resin printer and an FDM Bambu, knows their way around PrusaSlicer, has
opinions about terrain, mildly impatient with anything that makes their hobby
harder than it needs to be.  Reads docs the way most people read IKEA instructions:
skims first, goes back to the confusing parts.

---

## First reaction

*"OK so this generates grass tiles for DungeonBlocks.  Fine, I use DungeonBlocks.
What does it look like?  Where's the picture?"*

**Gap:** Neither document has a single example image, render, or even a description
of a finished tile from a gamer's perspective ("a 35 mm square with flat grass
blades lying across it, roughly 1–2 mm long, in clumps of 20–30").  Someone
evaluating whether they want to use this has nothing to go on visually.

---

## On blade size and coverage

*"2.5 mm average blade length on a 35 mm tile... is that going to look like
anything?  That's tiny.  Will it just look like texture?"*

The behavior doc says blades are 1–2 mm long on average.  A 35 × 35 mm tile is
1225 mm².  With 80 blades at ~1.5 mm wide and ~2.5 mm long, coverage is maybe
80 × 1.5 × 2.5 = 300 mm² — roughly 25% of the tile surface.  Three-quarters bare.

**Question not answered by either document:** what does a finished tile actually
look like at table scale with a 28 mm figure standing on it?  Is 25% coverage
intentional (sparse, patchy grass) or is this a density problem that needs fixing?
The requirements say density is "configurable" but give no sense of what the
default should achieve visually.

---

## On blade cross-section

*"Flat ribbon lying on the ground — OK so it's basically like a mat of leaves,
not upright grass.  Is that intentional?  Like pressed-down grass or dead grass?"*

The behavior doc establishes this clearly: *"blades lie almost flat... like a
field of grass that has been blown or pressed down by wind."*  That's a reasonable
aesthetic choice, but a gamer choosing between terrain options would want to know:
is this the only option, or can they get more upright-looking grass with a
different species config?  The species model (REQ-SPE) suggests yes — but neither
doc says so explicitly.

---

## On species

*"Wait, species?  So I can have like, grass AND rushes on the same tile?  That's
actually cool.  How do I configure that?  Is there a default set of species or do
I have to invent them myself?"*

The requirements define the species model but don't describe what species ship
out of the box.  A user-facing question: is there a library of ready-made species
(grass, rush, clover, dead grass, etc.) or does the user have to build their own
from scratch by tuning width/curl/cross-section values?  The gamer just wants it
to look good without writing config code.

---

## On stones

*"Blades rise over stones and then drop back — nice.  But does that actually look
good at print scale?  Like, is the rise visible?  Or is it just 0.1 mm and you
can't see it?"*

The requirements say blades rise over obstacles.  They don't say anything about
what "rise over a stone" looks like as finished geometry.  At FDM print resolution
(0.15–0.3 mm layer height), a 1–2 mm stone sticking through a mat of flat grass
blades should look good.  But the docs don't confirm whether this is actually
verified or just theoretically correct.

---

## On generation time

*"5 seconds for a 1×1 tile — fine.  But if I want a 3×3 dungeon room that's...
45 seconds?  Or linear so 9 × 5 = 45 s?  That's kind of a lot when I'm iterating
on a scene."*

REQ-PRF-2 says "no worse than linear with tile area."  A 3×3 tile at linear
scaling = 9× the 1×1 time = 45 s.  That's noticeable.  A gamer generating a full
dungeon might want 20–30 tiles.  At 45 s each for multi-square tiles that's
15–20 minutes.  The requirements don't give a target for larger tiles.

---

## On DungeonBlocks compatibility

*"It says DungeonBlocks scale and OpenLOCK scale.  Does that mean it works with
the actual DungeonBlocks socket system?  What about other systems — Dragonlock,
Dwarven Forge, OpenForge?"*

The requirements call out DungeonBlocks and OpenLOCK but don't say whether the
output is compatible with any other terrain system.  For a gamer this is a big
question.  The answer is probably "you can configure the square_mm for any system"
but that's not stated.

---

## On workflow

*"So I clone a Python repo, install it, run a command, and get an STL.  Then I
slice and print.  OK.  What if I want to change the seed?  What if I want a
different flow direction?  Do I edit a YAML file?  Do I need to know Python?"*

Neither doc describes the user-facing workflow at all.  The grass layer is one
component of a pipeline, and a gamer just wants to know: how do I get a tile?
The requirements are pure internal specifications.  That's appropriate for a
functional requirements doc — but it means there's a missing layer: a "how to use
this" document.

---

## Things the gamer would actually care about that aren't in either doc

1. **What does the output look like?**  No images, no renders, no "here's what
   you get."  This is the first question anyone asks.

2. **Does it print well?**  "Watertight manifold mesh" is engineer-speak.  A gamer
   wants to know: does it slice cleanly in PrusaSlicer?  Any common slicer issues?

3. **Colour?**  The STL uses per-face colour encoding for PrusaSlicer.  Is that
   documented anywhere accessible?  What colour is the grass?

4. **How much filament?**  A 35 mm tile at 0.2 mm layers — how long does it print?
   How much filament?  The requirements say nothing about output geometry size.

5. **Can I mix grass and water on the same tile?**  Yes (there's a grass-and-water
   spec) but you'd only know that from the CLAUDE.md, not from these docs.

6. **What if I want denser grass?**  "Groups per square is configurable" — how?
   Where?  YAML file?  Python?

---

## Overall verdict from Reeve

*"The behavior doc is actually pretty readable and tells me what the thing does.
The requirements doc is not for me — it's for whoever's writing the code.  But
neither of them tells me whether I'd actually want to use this.  I need to see
what a printed tile looks like before I care about any of this.  The species idea
is genuinely interesting — if I can mix grass and rushes and clover on one tile
without writing code, I'm in.  But right now it reads like it's being built for
the engineer, not for the person who wants to run a game."*

---

## Actionable gaps for the project

- A user-facing README or "getting started" doc with at least one image
- Default species library (named presets that produce good-looking output)
- Confirmation that default density produces visually satisfying coverage
  (not just "configurable")
- Performance target for common multi-square sizes (3×3, 4×4)
- A plain statement of which terrain systems are supported and how to add others
