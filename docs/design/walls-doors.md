# Walls — Openings: doors, windows, hatches (design, rev 2)

**Rev 2, 2026-07-07.**  Rev 1 approved by Shawn ("All of your thoughts
here sound good") with major additions folded in below: openings of
arbitrary size and position; doors/windows unified; surrounds allowed
to rise ABOVE low wall tops; arch AND lintel for fieldstone AND
brick; a leaf taxonomy including open-at-angle and SLOT-LOADED
swappable leaves.  References: `docs/reference/walls/doors/` (10
photos + truths) and the official DungeonBlocks pieces.

## Evidence recap

Official pieces: a doorway occupies a full square between jamb
columns (clear width 19–27 mm); RR-102 builds its arch from
individual voussoirs off quoin-stacked jambs; **RR-069/070 "Double
Door Left/Right" are SEPARATE LEAF pieces** (85 mm tall) that pair
with the Tall Arch walls — the officials already play the
swappable-leaf game.

Photo truths: doorway = opening + structurally distinct SURROUND +
optional leaf; the bond flows AROUND the surround; voussoirs are
radial (thin slabs in rubble work, ref-03/05; dressed wedges in 1–3
orders, ref-05/06; bricks-on-end rowlock/segmental, ref-10); leaves
are planked rectangles or arch-tops; sill slabs at window bases;
**portcullis grooves are vertical channels cut into the jamb faces**
(ref-08/09) — the medieval precedent for slot-loaded leaves.

## The model

One concept covers doors, windows, and floor hatches:

```python
Opening(at,                  # centre along the spine, in SQUARES (float)
        width_mm  = 22.0,    # clear width — arbitrary
        sill_mm   = 0.0,     # 0 = door; > 0 = window sill height
        head_mm   = 36.0,    # clear opening top above the seat — arbitrary;
                             # MAY exceed the wall height (see low walls)
        head      = 'arch',  # 'arch' | 'lintel' — both, for EVERY family
        rise_mm   = None,    # None = semicircular; less = segmental arch
        profile   = 'auto',  # 'auto' (rect + head) | 'circle' |
                             # [(t, z), …] custom polygon — the ARCH IS A
                             # SPECIAL CASE of one boundary-lining rule:
                             # near-vertical boundary → jamb stacks,
                             # curved/horizontal → radial voussoirs.  A
                             # circle has no verticals → a full voussoir
                             # ring: an OCULUS in a wall, a WELL on a
                             # floor (Shawn: round holes in the floor;
                             # arbitrary shaped holes)
        leaf      = None,    # None | Leaf(...) — see taxonomy
        slot      = False)   # portcullis-style channel for swap-in leaves

CutStoneWall(spine=…, openings=[Opening(at=0.5), Opening(at=1.5,
             sill_mm=14, head_mm=26, width_mm=12, head='lintel')])
```

On a `laid_flat` wall (i.e. a floor) the same `Opening` is a HATCH: a
rectangular gap in the pavement with an optional trapdoor leaf.

**Surround style** (`surround=`, 2026-07-07 Shawn): ``'jambs'`` is
the classic door construction (quoin-style jamb stacks + arch/lintel
head + sill slab); ``'ring'`` cuts the rectangle out and lines it
with a FRAME of small units — the circle's voussoir ring generalized
to a square (a row of small bricks per edge + square corner blocks).
``'auto'``: hatches in a pavement take ``'ring'`` (the jambs+lintel
construction chopped the floor into weird slabs); standing walls
take ``'jambs'``.  Not floor-specific — a single wall slab with a
door in it takes the same frame via ``surround='ring'``.  ``'ring'``
requires ``profile='auto'`` with ``head='lintel'`` (a circle/custom
profile already gets a full voussoir ring); other combinations raise
in ``Opening.__post_init__`` rather than silently falling back to
jambs.

## Mechanism (unchanged in essence, approved rev 1)

Openings live at the LAYOUT level — the crenellation/quoin/
throughstone machinery generalized.  Every VISIBLE part is a placed
unit (the surround and the fitted bond blocks — no boolean carves the
faces).  The only boolean is on the hidden recessed CORE: the passage
prism (dilated by the reveal) is differenced out of the mortar core,
so the core edge stays behind the surround ring and jamb reveals are
real masonry, never a bare core plane.

1. **Exclusion** in (t, z): drop cells inside, trim straddlers —
   trimmed ends become textured 'face'.
2. **Jambs**: forced quoin-style stacks flanking the opening from
   sill to springing.  Fieldstone: larger squared stones (ref-05);
   brick: closer bricks; cut: dressed quoins.
3. **Head**, per family × style (all six combinations supported):
   | | `arch` | `lintel` |
   |---|---|---|
   | cut stone | dressed voussoir wedges, keystone | single dressed lintel block |
   | fieldstone | thin SLAB voussoirs radiating (ref-03/05) | one big rough lintel slab |
   | brick | rowlock bricks-on-end, segmental via `rise_mm` (ref-10) | timber lintel beam (WOOD) or stone lintel |
   Voussoirs are ordinary unit cells with a per-cell ROTATION angle —
   the only new geometry in the whole campaign.
4. **Sill** (windows): one projecting slab cell under the opening.
5. **Bond flows around** by construction (surround cells are
   pre-claimed intervals in the layout solver).

## Low walls: the surround rises ABOVE the top (new, Shawn)

The point of low walls is to imply tall walls while keeping miniature
access.  So an `Opening` whose `head_mm` exceeds the wall height is
LEGAL and default-friendly: the jamb stacks and the arch/lintel are
built to the opening's own height, proud of the wall top — a low wall
with a full-height arch rising out of it (the walled-garden-gate
read, ref-06/07).  Consequences by construction:

- surround cells are exempt from the wall-height clip and from the
  ruin envelope (a ruined wall keeps its surviving arch — the classic
  ruin picture, and exactly what RR-126 "Tall Arch Broken" sells);
- the exposed back/top of the above-wall surround is textured like
  any face (real modeled surfaces, R2);
- no auto-tall promotion needed (rev 1's proposal is DEAD): the
  default wall stays at the official 33.1 top and the doorway rises
  to its own head height.  Tall walls remain available for enclosed
  doorways with masonry above (TS-015/MT1-042 style).

## Leaves (the word is "leaf")

Door leaf / window leaf (shutter) / hatch leaf (trapdoor).  A leaf is
a separate solid fitted to the opening's PROFILE (Shawn, 2026-07-07:
arched doorways get arch-top doors; round openings take leaves too —
a round grille in an oculus, a round lid on a well.  Supersedes rev
2's leaves-stay-rectangular/tympanum idea).  Construction reuses the
WALL strategy at leaf scale: ``union(core, planks)`` — a thin profile
prism recessed behind both faces + full-thickness planks with reveal
gaps, so the board grooves read identically on BOTH faces; plank
faces carry carved wood grain (ridged, gently wavy, along the board).

**Types** (each a small parametric generator, WOOD-tagged unless
noted):

Implemented in O5 (`kind=`): `planks`, `shutters`, `bars`,
`trapdoor`.  The rest are later stages — `build_leaf` raises on them.

| Type | Read | Use | Status |
|---|---|---|---|
| `planks` | vertical planks + 2–3 ledges (battens) + stud grid + ring | the default door | O5 ✅ |
| `shutters` | pair of small side-hinged plank leaves | windows | O5 ✅ |
| `bars` | vertical round bars + frame (ROCK/metal tone) | prisons, window grilles | O5 ✅ |
| `trapdoor` | planked square + ring, flush | floor hatches | O5 ✅ |
| `double` | two `planks` leaves meeting at centre | gates (2-square openings) | later |
| `portcullis` | square grid lattice, pointed feet | gates | O6 |
| `broken` | `planks` with a ragged missing corner | ruins | later |

**States**: `open_deg=0` (closed) or an angle — the leaf solid is
rotated about its hinge EDGE and unioned standing open at any angle
(ref-06's red door).  `hinge='left'`/`'right'` swings the free
vertical edge inward through the wall; `'foot'`/`'head'` tips about a
horizontal edge (a trapdoor lifts about its foot, a shutter awning
tips out about its head).  `leaf=None` = empty opening.

**Slot system** (Shawn's swap idea; portcullis precedent ref-08/09;
O6 — `Opening(slot=True)` currently raises `NotImplementedError`):
`slot=True` changes the game —

- the jamb inner faces get vertical GROOVES (channel width =
  leaf thickness + ~0.5 mm clearance, depth ≈ 1.2 mm, chamfered
  lead-in), running from the surround TOP down to the sill;
- the head leaves a through-slot at the top of the channel: the leaf
  plane sits mid-thickness, BEHIND the arch/lintel face order, so the
  slot enters cleanly from above — trivially so on above-wall
  surrounds, where the slot mouth is proud of the wall top;
- the leaf becomes a SEPARATE PRINT: `Leaf.standalone()` emits a
  matching STL (tongue edges sized to the grooves, same width/head
  params), so one wall accepts many leaves — closed door, portcullis,
  bars, broken door, or nothing — swapped during play.  This is
  exactly the official RR Double-Door-and-Tall-Arch pairing, done
  parametrically.

## Compatibility defaults

Clear width 22 mm (officials 19–27); door head 36 mm above seat;
window default sill 14, head 26, width 12; slot clearance 0.5 mm
total (tune on first print); leaf thickness 2.6 mm.

## Implementation stages (each shippable, per the campaign method)

- **O1** ✅ layout exclusion + jambs + LINTEL heads, all three
  families, open passage (no leaf) (`walls/openings.py` +
  `_apply_openings`/`_surround_cells`/`_place_posed` on the chassis).
- **O2** ✅ arch heads (voussoir cells with rotation + wedge taper),
  all families; circle profile = full voussoir ring (oculus/well).
- **O3** ✅ above-wall surrounds + ruin exemption (surrounds live in
  `posed`, which `_ruin_cells` never sees).
- **O4** ✅ windows (sills) + floor hatches/wells via laid_flat
  (demos: `walls-e13-openings`, `walls-e14-hatch`).
- **O5** ✅ integrated leaves (`walls/leaf.py`): planks/shutters/
  bars/trapdoor, closed and open-at-angle about any hinge edge; WOOD
  group (bars ROCK); fused to the jambs by construction (demo:
  `walls-e15-leaves`).
- **O6** slot system + standalone leaf prints (portcullis, swaps) —
  NOT STARTED.  `Opening(slot=True)` raises until O6 lands.  The
  plumbing seam exists: `_posed_cell(split=…)` splits a surround
  unit into a front/back `_Posed` pair around the `slot_gap_mm`
  channel; O6 wires `slot_gap_mm = leaf.thickness_mm + clearance`,
  grooves the jamb inner faces, and adds `Leaf.standalone()`.

**Surround finish note (2026-07-07)**: surround units get their own
per-family finish knobs on the chassis — `surround_chip`/
`surround_ro` (the body texture's chip budget and roundover ate the
2–5 mm units: the E13 bead-chain arch), `surround_frac` (unit width ÷
pitch; fieldstone 1.10 = pressed drystone contact, the union fuses),
`surround_proud_mm` (surrounds stand proud of both faces — the
distinct-order read, and the well curb on floors).

**Bond-to-surround fit (2026-07-07, Shawn; solver in `walls/fit.py`)**:
opening-adjacent wall units are RESHAPED to fill the space against a
curved surround.  Per course band the bond is trimmed against the
actual surround unit rectangles (toothing into the quoin alternation);
too-narrow remnants are never dropped — the fallback chain absorbs
them into a course neighbour (a mason's cut unit, never a column of
exposed core), else a thin end unit at a wall end, else a short brick
in the wide sub-band of a bimodal blocker.  Each cut side gets a
SINGLE LINEAR ANGLED CUT (least-squares support line of the surround
region — units dilated by `joint − surround_bond_press` — within the
band), not a literal curve trace.  The cut is one extra plane in the
block kernel's smooth-max, so the cut arris gets the same roundover as
every other edge; fieldstone applies the same line to its crack
outline and the sphere-morph rounds it natively.  Scaled keystones
grow outward only: nothing hangs below the arch soffit (FDM).

Known constraint: the fit decides which cells to reshape from the
surround UNIT extents per band, not the passage profile — correct for
the jambs/ring/arch recipes (which flank every band the passage
touches), but a custom concave `profile` could expose bare core in a
band with no surround unit.  Custom profiles are not yet a supported
surround recipe.

## Remaining questions for Shawn

1. Gates (2-square double doors) — in scope after O5?
2. (resolved 2026-07-07 — Shawn: WOOD colour group for leaves is fine.)
3. (resolved — the "images" were harness artifacts, none were sent.)
