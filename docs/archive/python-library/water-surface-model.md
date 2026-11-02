# Water Surface Displacement Model

**Status:** Implemented — `layers/water.py:build_water_surface_displacement`

---

## 1. Overview

The water surface is a displacement height field added to the calm water level.
It is _not_ random noise. It models the structured behaviour of a shallow,
wind-driven water body: overlapping wave trains, shoaling compression near the
shore, and coupled disturbances around submerged obstacles.

```
H(x, y) = water_height
         + [PrimaryWaves(x,y) + CapillaryRipples(x,y)] · AmpField(x,y)
         + BowWaves(x,y)
         + MeniscusRings(x,y)
```

`AmpField` is a spatial scalar that combines shore amplification with rock wake
suppression and is computed before the sinusoidal terms.

---

## 2. Coordinate conventions

All computations are in mm at cell-centre resolution `(grid_h, grid_w)`.  The
final displacement is bilinear-interpolated to vertex-corner resolution
`(grid_h+1, grid_w+1)` for the water-volume mesh.

```
x increases rightward  (+column direction)
y increases upward     (+row direction)
z is displacement above calm water level (positive = crest, negative = trough)
```

---

## 3. Amplitude field

`AmpField(x,y)` is factored out so shore amplification and rock wake damping are
applied consistently to every wave component.

### 3.1 Shore proximity

```
dist_shore   = distance in mm to nearest non-water cell (EDT on water_mask)
shore_prox   = clamp(1 - dist_shore / compress_dist, 0, 1)
shore_smooth = smoothstep(shore_prox)          # 3t² − 2t³

amp_shore    = lerp(1, shore_amplitude_factor, shore_smooth)
freq_shore   = lerp(1, shore_freq_factor,      shore_smooth)
```

`freq_shore` compresses the wave phase near shore (shorter apparent wavelength
without creating phase discontinuities), reproducing the wave-shoaling effect
visible as denser texture near the waterline.

### 3.2 Rock wake

For each submerged rock at `(cx, cy)` with mean radius `r`:

```
downstream  = max(0, flow_proj − r)              # 0 at rock back face
wake_sigma  = r + downstream · 0.3              # cone widens downstream

wake_weight = (1 − exp(−downstream / r))         # ramps up from 0 at rock edge
            · exp(−0.5 · (perp_proj / wake_sigma)²)   # Gaussian width
            · exp(−downstream / (wake_length · r))     # decays far downstream
```

The combined wake factor (multiplied across all rocks):

```
wake_amp = product over rocks of: 1 − (1 − rock_wake_amp_factor) · wake_weight
AmpField = amp_shore · wake_amp
```

---

## 4. Primary wave field

`N` sinusoidal trains superimposed at slightly different angles produce natural
interference patterns and avoid a single-direction look.

```
θᵢ = primary_dir + (i − N/2) · dir_spread
λᵢ = primary_wavelength · U(1−spread, 1+spread)
φᵢ = random phase offset

proj_i  = x·cos(θᵢ) + y·sin(θᵢ)
k_local = (2π / λᵢ) · freq_shore(x, y)          # shore-compressed wave number

wave_i  = primary_amplitude · sin(k_local · proj_i + φᵢ)
```

---

## 5. Capillary ripples

Short-wavelength waves at random directions, representing wind-driven surface
texture and the secondary sparkle layer visible on real water.

```
for i in range(n_capillary):
    θ    ~ Uniform(0, 2π)
    λ    ~ Uniform(λ_min, λ_max)
    φ    ~ Uniform(0, 2π)
    A    ~ capillary_amplitude · Uniform(0.7, 1.3)
    proj  = x·cos(θ) + y·sin(θ)
    z    += A · sin(2π·proj/λ + φ)
```

---

## 6. Rock interactions

### 6.1 Bow wave

Water piles up just upstream of each rock. Modelled as an elliptical Gaussian
centred slightly upstream of the rock, stronger on the upstream face:

```
centre_bow = rock_centre − 0.4·r·flow_dir

perp_b = component of (vertex − centre_bow) perpendicular to flow
flow_b = component of (vertex − centre_bow) along flow

bow_z = bow_amplitude · exp(−0.5·(perp_b²/σ_perp² + flow_b²/σ_flow²))
      · upstream_taper(flow_b)       # full strength upstream, 0.3× downstream
```

### 6.2 Meniscus ring

Water climbs slightly up the rock face, creating a raised contact ring:

```
ring_dist = |dist_from_rock_centre − r|
men_z     = meniscus_amplitude · exp(−(ring_dist / meniscus_sigma)²)
```

### 6.3 Flow deflection

Not explicitly implemented. The superimposed primary-wave trains already produce
curved wavefronts through phase cancellation near the rock (the wake's amplitude
suppression creates the visual break in wave crests). Full potential-flow
deflection is a future enhancement.

---

## 7. Configuration — `WaterSurfaceConfig`

| Parameter | Default | Notes |
|---|---|---|
| `n_primary` | 3 | wave train count |
| `primary_dir` | 0.52 rad (~30°) | dominant wave direction |
| `primary_dir_spread` | 0.30 rad | angular spread between trains |
| `primary_wavelength_mm` | 12.0 | dominant wavelength (mm on tile) |
| `primary_wavelength_spread` | 0.30 | relative λ variation (±30%) |
| `primary_amplitude_mm` | 0.22 | half crest-to-trough amplitude |
| `shore_compress_dist_mm` | 5.0 | shore band width for shoaling effect |
| `shore_amplitude_factor` | 1.35 | amplitude multiplier at shore |
| `shore_freq_factor` | 1.60 | wave-number multiplier at shore |
| `n_capillary` | 10 | capillary ripple count |
| `capillary_wavelength_min_mm` | 2.5 | shortest capillary λ |
| `capillary_wavelength_max_mm` | 5.0 | longest capillary λ |
| `capillary_amplitude_mm` | 0.045 | capillary half-amplitude |
| `rock_bow_amplitude_mm` | 0.18 | peak bow-wave height |
| `rock_wake_length_factor` | 4.0 | wake length as multiple of rock radius |
| `rock_wake_amp_factor` | 0.45 | wave amplitude inside wake (0=dead calm) |
| `rock_meniscus_amplitude_mm` | 0.10 | contact-ring lift |
| `rock_meniscus_sigma_mm` | 0.70 | contact-ring Gaussian width |

Physical scale note: at 1:42.86 (35 mm tile = 1.5 m game square), the
12 mm default wavelength represents ~0.5 m real water waves, appropriate for
a shallow wind-driven pond. The 2.5–5 mm capillary band represents 10–20 cm
real ripples.

---

## 8. Mesh integration

`build_water_surface_displacement` returns a `(grid_h, grid_w)` cell-centre
displacement array.  `make_water_volume` bilinear-interpolates it to vertex
corners, clips the result so the surface never dips below the riverbed, then
builds the closed volume solid.

The terrain solid below the water is unchanged — the water volume sits on top.
Where a wave trough would expose the riverbed (trough z < terrain_z at a vertex)
the surface is clamped to the riverbed, creating a shallow-water look at no
extra cost.

---

## 9. Future work

- **Flow deflection:** potential-flow phase perturbation around each rock
  (Ψ = −r²/d² · cos(2θ) phase offset on primary waves).
- **Depth-dependent dispersion:** amplitude decays in shallower-than-wavelength
  water following linear wave theory.
- **Gerstner waves:** add horizontal displacement for the peaking profile of
  real ocean waves (low priority on a still pool).
- **Per-region config override:** expose `WaterSurfaceConfig` fields in the
  `.tile` spec `water` layer params.
