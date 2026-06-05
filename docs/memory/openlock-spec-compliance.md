---
name: openlock-spec-compliance
description: OpenLOCK spec compliance status — what passes, what fails, fix priorities
metadata:
  type: project
---

OpenLOCK output from `src/dharmatiles/bases/openlock.py` — updated status as of 2026-06-04:

**FIXED (2026-06-04)**:
- Clip socket is now T-shaped (14→12→10 mm stepped pocket) — clips lock correctly
- Grid is 25.4 mm imperial (was incorrect 25.0 mm metric)
- Terrain regenerated natively at 25.4 mm; no XY scale-down

**Still failing**:
1. **Missing clip retention side cut** — no 4.7 mm × 18 mm slot at 8.35 mm offset for clip spring arm; clips may not seat or may crack socket on insertion

**Known warnings**:
- Mesh is not watertight (~4.1 M open edges from unmerged heightmap+blade verts); slicers auto-repair

**How to apply:** See `meta/history/2026-06-04-openlock-regen-review.md` for post-fix analysis.
Full initial review: `meta/history/2026-06-03-openlock-spec-compliance-review.md`.
Spec: `docs/openlock-spec.md`.

[[terminology-tile-square-cell]]
