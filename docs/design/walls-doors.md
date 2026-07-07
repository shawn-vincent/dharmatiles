# Walls — Doorways (design analysis)

**2026-07-06, design-first round for Shawn's ask: "an elegant solution
on how to incorporate doors into my walls."**  References:
`docs/reference/walls/doors/` (photo truths in its README) + the
official DungeonBlocks door/arch pieces measured below.  No
implementation yet — this doc proposes the mechanism and lists the
open decisions.

## What the official pieces do

Measured/rendered (scratchpad `ref-{ud-door,rr-arch,mt1-door}.png`):

| Piece | Read | Numbers |
|---|---|---|
| UD-010 Door | Floor square + freestanding arched DOORWAY assembly: round-top planked leaf, ring handle, hinges, thin surround with quoined jamb | 35×35 plan |
| RR-102 Arch | Floor square + freestanding pointed arch: quoin-stacked jamb columns rising into individual wedge VOUSSOIRS | clear width ≈ 19 mm, piece 74 tall |
| TS-015 / MT1-042 Door | TALL wall (77+ bbox) with integrated door + masonry above | clear width ≈ 26 mm |
| RR-069/070 Double Door | Two-square gate leaves | 85 tall |

**The official design language: a doorway occupies a full square.**
The opening spans between two jamb columns at the square's edges;
plain wall squares butt on either side.  Doors imply the TALL wall
format — an opening plus headroom does not fit the standard height.

## Photo truths (see reference README)

1. Doorway = opening + structurally distinct SURROUND (+ optional
   leaf).  2. The wall's bond flows AROUND the surround — courses butt
into jamb stones; nothing is carved out of blocks.  3. Voussoirs are
radial wedges from the jamb springing, keystone at apex; fieldstone
arches are thin slabs radiating (ref-03/05), dressed arches larger
wedges in 1–3 orders.  4. Leaves are planked rectangles (battens,
studs, ring), square-headed under a tympanum or arch-topped.
5. Threshold slab at the base.

## The elegant mechanism: openings live in the LAYOUT

The chassis already contains the answer twice over.  Crenellation IS
an opening machine (it deletes cells inside crenel intervals at the
top edge and re-cuts straddlers with textured 'face' flanks), and
quoins/throughstones already force special cells.  A doorway is the
same move in the middle of the wall — **no booleans, no carved
blocks, everything stays a placed unit**:

```python
CutStoneWall(spine=…, height_mm=…,
             openings=[Doorway(at=0.5,          # squares along spine
                               width_mm=22.0, height_mm=36.0,
                               style='arch',    # 'arch'|'lintel'|'open'
                               leaf='planks')]) # 'planks'|None
```

Build order per opening, all at the `_cells` stage:

1. **Exclusion**: drop cells inside the opening's (t, z) region;
   trim straddlers to its edges — trimmed ends become 'face' (they
   are textured automatically, like every wall end today).
2. **Jambs**: force a quoin-style stack of dressed cells flanking the
   opening (alternating depths — the existing quoin read).  For
   fieldstone: larger squared stones, exactly ref-05.
3. **Head**:
   - `lintel`: one merged long cell spanning the opening (the
     throughstone merge pattern) — the fieldstone/rustic default.
   - `arch`: NEW but contained geometry — voussoir cells along the
     arc, each an ordinary unit built in a PER-CELL ROTATED frame
     (cells gain an optional `angle`; `_place_block` applies it).
     Semicircular or segmental; keystone = middle voussoir enlarged.
     Family-appropriate: dressed wedges (cut stone), thin radiating
     slabs (fieldstone, ref-03), brick-on-end rowlock (brick).
4. **Bond flows around** by construction: the layout solver treats
   jamb/arch cells as pre-claimed intervals (same mechanism that
   keeps bay cuts out of corner cells), so ordinary courses butt into
   the surround.
5. **Leaf** (optional): a separate solid in the opening plane —
   vertical planks + two battens + stud grid + ring, square- or
   arch-topped; recessed half the wall thickness.  Tagged WOOD so it
   colors separately.
6. **Threshold**: one flat slab cell at the base; the door's floor
   opening is walkable (matters for laid_flat later: the same
   `openings` list on a FLOOR is a hatch/pit surround for free).

Guarantees preserved: everything is still `union(units + core)` —
watertight, reveals over the core (the core gets the same exclusion,
inset by reveal → the opening's inner faces are real textured jamb
faces with mortar behind, not core planes), FDM-printable (arch
voussoirs self-support; lintel spans are bridged by the core sheet
behind).

## Compatibility numbers (proposed defaults)

- Opening clear width **22 mm** (officials 19–27), centered on a
  square; one doorway per square, like the official language.
- Clear height **36 mm** above the seat — which requires the TALL
  wall: a Doorway on a default-height wall auto-promotes it to
  `top_mm=72.3` (official tall) unless the author explicitly says
  otherwise.  (A 33.1-top wall has no headroom over a usable door.)
- Threshold at pavement level; works on soil or `StoneFloor`.

## Open questions for Shawn

1. **Head style default**: arch (officials, most references) or flat
   lintel (simpler, very drystone)?  Proposal: `arch` for cut
   stone/brick, `lintel` for fieldstone, both available.
2. **Leaf default**: integrated closed planked door (officials do
   this) vs open passage?  Proposal: open (`leaf=None`) by default,
   `leaf='planks'` opt-in.
3. **Auto-tall**: OK that adding a Doorway promotes a default wall to
   the official tall height?
4. Double-door / gate (2-square opening) in gen-1 or later?
5. Your reference image from this thread didn't reach my cache — I
   couldn't see it.  What did it show / should it join the set?
