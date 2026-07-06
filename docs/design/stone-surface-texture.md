# Stone Surface Texture — the common relief pass

**2026-07-05, after Shawn rejected the plane-wave relief on floor
slabs** ("it doesn't look good" — MeshLab close-up read as choppy
water / wrinkled fabric).  Fifth run of the design-first method.

## Reference truths

Photo references (`docs/reference/stone-texture/`, Wikimedia Commons):

1. Worked stone faces (sawn sandstone ashlar, ref-01) are NEARLY FLAT
   — colour does most of the work; physical relief is sparse: edge
   chips, faint strata, occasional pits.
2. Weathered rough stone (limestone, ref-04) = granular field
   punctuated by DISCRETE oval pits of varied size — one-sided
   (material removed), never symmetric waves.
3. Granite (ref-03) reads as uniform fine grain — at miniature scale
   this is effectively flat with micro-tooth.

Official DungeonBlocks floors, MEASURED (ray-cast height grids over
the walking surface; `docs/reference/walls/commercial-sets-analysis.md`
scratch analysis 2026-07-05):

| Piece | RMS | p5..p95 | skew | spectrum |
|---|---|---|---|---|
| TS-019 basic floor | 0.32 mm | −0.80..+0.24 | **−0.99** | peak ≈ 3 mm, long tail |
| UD-057 prison floor | 0.21 mm | −0.31..+0.37 | +0.19 | peak ≈ 14 mm (broad dish) |
| RR-015 grass ground | 0.36 mm | −0.41..+0.69 | +0.66 | mounds bulge UP |

**The stone read is a CALM PLATEAU CARVED DOWNWARD** (negative skew ≈
−1): recesses, worn channels, dished patches — with almost NO relief
power below 2 mm wavelength (0.03–0.09 of peak).  Grass is the mirror
image (mounds up).  Sum-of-plane-waves noise fails all three ways:
symmetric (skew 0), everywhere-active (no plateaus), and on flat faces
its phase coherence reads as directional corduroy / chop.

## Mechanism (by construction)

`stone/noise.py: fbm(p, seed, scale_mm, octaves)` — isotropic VALUE
NOISE (hashed integer lattice, smoothstep-interpolated, octave sum).
No plane waves anywhere: value noise has no global phase to cohere
into stripes, and it is evaluated in 3D object space so it works
identically on flat slabs and curved pebbles.

`stone/finish.py: stone_relief(body, rng, ...)`:

1. **carve** (the signature term): `−carve_mm · max(0, n − t)^shape`
   where `n` = fbm at `scale_mm` (default ≈ 7).  Threshold `t` keeps
   ~55 % of the surface an untouched plateau; the carved level sets of
   fbm read as connected worn recesses, not dents.  One-sided by
   construction → skew ≈ −1.
2. **dish**: low-amplitude two-sided fbm at footprint scale — the
   broad wear that keeps big faces from being dead planes (UD-057's
   14 mm peak).
3. curvature damping along Taubin-smoothed normals (unchanged from
   the aged pass — protects crack roots and tight features).

Discrete features stay separate mechanisms: chips (hull corner pulls),
cracks (stone/cracks.py), spalls (weather bites).  Together with the
carve field they supply the "incident" against the calm.

Acceptance: measured slab stats within the official floor band
(RMS 0.2–0.4, skew ≤ −0.6, ≤ 0.1 relative power at 2 mm), plus the
render/MeshLab read.

## Rollout

Floors and cut-stone/brick blocks first (this round); fieldstone and
scatter rocks migrate next with side-by-side renders (their approved
looks must survive re-parameterization — amplitude/scale per family,
same mechanism).
