//! The foot of the cliff: a dirt bank in the angle of cliff and ground,
//! and the scree (fallen stones, big blocks, half-buried grit) gathered
//! against the face (rockpile.py :1968-2060). The bank is earth heaped
//! highest against the face, falling away concave, higher and wider in
//! the shattered zones, the same at the tile ends for every seed (S4).

use dt_kit::{Grain, Material, Piece, Rng, Scene, Tag, form, grain, hung};

use super::form::{FOOT, Form, GRID_MM, SQUARE_MM, ZONE_FLOOR, Zones};
use super::util::{Draw, Field, dilate_disc, edt_out, lerp, smooth01};
use crate::LookError;

const APRON_H_MM: (f64, f64) = (1.5, 3.5);
const APRON_R_MM: (f64, f64) = (3.0, 6.5);
const APRON_END: (f64, f64) = (2.5, 4.5);
pub const END_MM: f64 = 4.0;
const APRON_EXP: f64 = 1.6;
const APRON_SHARE: f64 = 0.25;
const GRIT_D_MM: (f64, f64) = (1.0, 2.2);
const GRIT_BURY: (f64, f64) = (0.2, 0.5);
const SCREE_D_MM: (f64, f64) = (1.0, 6.0);
const SCREE_D_EXP: f64 = 2.0;
const SCREE_REACH_MM: f64 = 4.0;
const SCREE_LEAN: f64 = 0.35;
const SCREE_BLOCK_MM: (f64, f64) = (4.5, 8.0);
const SCREE_GAP: f64 = 0.38;
const SCREE_SINK_MM: f64 = 0.3;
const SCREE_MIX: [f64; 4] = [0.0, 0.2, 0.45, 0.35];
const ROCK_GRAIN_MM: f64 = 0.05;

/// The dirt's lattice sits off the core's by this much, so no node of one
/// lies on a node of the other (coincident lattices pinch on float32 reload).
const OFF: (f64, f64) = (0.0137, 0.0091);

// ---- dirt lodged on the rock tops (rockpile.py :2092-2146) ---------------
const DIRT_FILL_MM: f64 = 3.0;
const DIRT_FILL_DEPTH_MM: f64 = 1.5;
const DIRT_FILM_MM: (f64, f64) = (0.1, 0.5);
const DIRT_STEP_MM: f64 = 0.5;
const DIRT_SLOPE_DEG: f64 = 35.0;
const DIRT_RAMP_MM: f64 = 1.5;
const DIRT_BLUR_MM: f64 = 0.4;
const DIRT_LOW_MM: f64 = 5.0;
const DIRT_SINK_MM: f64 = 0.3;
const DIRT_PLUG_MM: (f64, f64) = (1.5, 3.0);

fn edges(z: &Field, step: f64) -> super::util::Mask {
    let (nx, ny) = (z.nx, z.ny);
    let sm = super::util::blur(z, 0.5 / GRID_MM);
    let tan = DIRT_SLOPE_DEG.to_radians().tan();
    let mut e = super::util::Mask::new(nx, ny, false);
    for j in 0..ny {
        for i in 0..nx {
            let (i0, i1) = (i.saturating_sub(1), (i + 1).min(nx - 1));
            let (j0, j1) = (j.saturating_sub(1), (j + 1).min(ny - 1));
            let gx = (sm.at(i1, j) - sm.at(i0, j)) / (((i1 - i0).max(1)) as f64 * GRID_MM);
            let gy = (sm.at(i, j1) - sm.at(i, j0)) / (((j1 - j0).max(1)) as f64 * GRID_MM);
            let mut v = z.at(i, j) <= FOOT + 1.0 || libm::hypot(gx, gy) > tan;
            if !v {
                'n: for dj in -1i64..=1 {
                    for di in -1i64..=1 {
                        if di == 0 && dj == 0 {
                            continue;
                        }
                        let (a, b) = (
                            (i as i64 + di).clamp(0, nx as i64 - 1) as usize,
                            (j as i64 + dj).clamp(0, ny as i64 - 1) as usize,
                        );
                        if (z.at(a, b) - z.at(i, j)).abs() > step {
                            v = true;
                            break 'n;
                        }
                    }
                }
            }
            e.set(i, j, v);
        }
    }
    e
}

fn closing(z: &Field, r_mm: f64) -> Field {
    let c = super::util::erode_disc(&dilate_disc(z, r_mm / GRID_MM), r_mm / GRID_MM);
    Field {
        nx: z.nx,
        ny: z.ny,
        v: z.v.iter().zip(&c.v).map(|(a, b)| a.max(*b)).collect(),
    }
}

/// Dirt resting on the rock tops, from `pile` (the finished cliff in its
/// own frame): a film over open stone, filling joints and hollows, thinning
/// to nothing at lips and steps. `None` when nothing lies on a top.
pub fn dirt_top(
    pile: &dt_kit::TriMesh,
    nx: usize,
    ny: usize,
    seed: u64,
    slot: dt_kit::Slot,
) -> Result<Option<Piece>, LookError> {
    let rm = dt_kit::rays::RayMesh::new(pile);
    let mut org = Vec::with_capacity(nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            org.push([
                (i as f64 + 0.5) * GRID_MM,
                (j as f64 + 0.5) * GRID_MM,
                1000.0,
            ]);
        }
    }
    let hits = rm.first_hits(&org, &vec![[0.0, 0.0, -1.0]; org.len()]);
    let mut z = Field::new(nx, ny, FOOT - 1.0);
    for (q, h) in hits.iter().enumerate() {
        if let Some(h) = h {
            z.v[q] = h.point[2];
        }
    }
    let b = {
        let e = super::util::erode_disc(&z, 1.0);
        Field {
            nx,
            ny,
            v: e.v.iter().map(|v| v - DIRT_SINK_MM).collect(),
        }
    };
    let zc = closing(&z, 0.5 * DIRT_PLUG_MM.0);
    let ed = edges(&zc, DIRT_PLUG_MM.0);
    let not_edge = super::util::Mask {
        nx,
        ny,
        v: ed.v.iter().map(|&e| !e).collect(),
    };
    let inner = edt_in_mm(&not_edge);
    for q in 0..nx * ny {
        if inner.v[q] >= DIRT_PLUG_MM.0 && zc.v[q] - z.v[q] < DIRT_PLUG_MM.1 {
            z.v[q] = zc.v[q];
        }
    }
    let blurred = super::util::blur(&z, DIRT_BLUR_MM / GRID_MM);
    let base = Field {
        nx,
        ny,
        v: blurred.v.iter().zip(&z.v).map(|(a, b)| a.min(*b)).collect(),
    };
    let zcl = closing(&z, DIRT_FILL_MM);
    let zf = Field {
        nx,
        ny,
        v: zcl
            .v
            .iter()
            .zip(&base.v)
            .map(|(a, b)| a.min(b + DIRT_FILL_DEPTH_MM))
            .collect(),
    };
    let lip = edges(&zf, DIRT_STEP_MM);
    let not_lip = super::util::Mask {
        nx,
        ny,
        v: lip.v.iter().map(|&e| !e).collect(),
    };
    let dl = super::util::edt_in(&not_lip);
    let mut d = Field::new(nx, ny, 0.0);
    let mut any = false;
    for j in 0..ny {
        for i in 0..nx {
            let q = j * nx + i;
            let ramp = smooth01((dl.v[q] - 1.5) * GRID_MM / DIRT_RAMP_MM);
            let p = [(i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM, 0.0];
            let rel = 0.5 + 0.5 * form::fbm3(p, seed, 2.0, 2);
            let mut thick =
                ((zf.v[q] - base.v[q]).max(0.0) + DIRT_FILM_MM.0 + DIRT_FILM_MM.1 * rel.max(0.0))
                    * ramp;
            thick *= smooth01((z.v[q] - FOOT - DIRT_LOW_MM) / 3.0);
            d.v[q] = if lip.v[q] {
                b.v[q] - 0.05
            } else if thick > 0.02 {
                (base.v[q] + thick).max(z.v[q] + 0.12)
            } else {
                base.v[q] + thick
            };
            any |= d.v[q] > z.v[q];
        }
    }
    if !any {
        return Ok(None);
    }
    let zb = FOOT - 2.0;
    let up = hung(
        &d.v.iter()
            .map(|&v| v.max(zb + 0.05) - zb)
            .collect::<Vec<_>>(),
        nx,
        ny,
        GRID_MM,
        0.04,
    )
    .map_err(LookError::build)?;
    // under: the rock top (padded by a cell), from zb - 1
    let (px, py) = (nx + 2, ny + 2);
    let mut under_d = vec![0.0; px * py];
    for j in 0..py {
        for i in 0..px {
            let (a, c) = (
                i.saturating_sub(1).min(nx - 1),
                j.saturating_sub(1).min(ny - 1),
            );
            under_d[j * px + i] = b.at(a, c) - (zb - 1.0);
        }
    }
    let mut un = hung(&under_d, px, py, GRID_MM, 0.04).map_err(LookError::build)?;
    for p in &mut un.positions {
        *p = [
            p[0] - 0.5 * GRID_MM + OFF.0,
            p[1] - 0.5 * GRID_MM + OFF.1,
            p[2] + zb - 1.0,
        ];
    }
    let mut upm = up;
    for p in &mut upm.positions {
        *p = [
            p[0] + 0.5 * GRID_MM + OFF.0,
            p[1] + 0.5 * GRID_MM + OFF.1,
            p[2] + zb,
        ];
    }
    let piece = Piece::new(slot, &upm).map_err(LookError::build)?;
    let piece = piece.minus(&[un]).map_err(LookError::build)?;
    if piece.is_empty() {
        return Ok(None);
    }
    Ok(Some(piece))
}

fn edt_in_mm(m: &super::util::Mask) -> Field {
    let e = super::util::edt_in(m);
    Field {
        nx: e.nx,
        ny: e.ny,
        v: e.v.iter().map(|x| x * GRID_MM).collect(),
    }
}
