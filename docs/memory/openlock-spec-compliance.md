---
name: openlock-spec-compliance
description: OpenLOCK spec compliance status — what passes, what fails, fix priorities
metadata:
  type: project
---

OpenLOCK output from `src/dharmatiles/bases/openlock.py` — updated status as of 2026-06-08:

**All known spec items FIXED**:
- Clip socket is T-shaped (14→12→10 mm stepped pocket) — clips lock correctly
- Grid is 25.4 mm imperial (was incorrect 25.0 mm metric)
- Terrain regenerated natively at 25.4 mm; no XY scale-down
- Clip retention side cuts present: `RETENTION_OFFSET_MM = 8.35`, `RETENTION_WIDTH_MM = 4.7`,
  `RETENTION_DEPTH_MM = 9.0` (half of the 18 mm centred cube in the spec §3c).
  Both ±8.35 mm sides are cut, which is slightly over-spec but harmless
  (provides symmetric retention; doesn't prevent clip seating).

**Known warnings**:
- Mesh is not watertight before base attachment (unmerged heightmap+blade verts); slicers auto-repair

**How to apply:** See `meta/history/2026-06-04-openlock-regen-review.md` for post-fix analysis.
Full initial review: `meta/history/2026-06-03-openlock-spec-compliance-review.md`.
Spec: `docs/openlock-spec.md`.

[[terminology-tile-square-cell]]
