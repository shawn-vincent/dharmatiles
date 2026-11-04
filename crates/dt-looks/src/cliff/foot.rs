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
const END_MM: f64 = 4.0;
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

/// What a tile asks of the foot.
#[derive(Clone, Copy, Debug)]
pub struct FootAsk {
    pub bank_h: (f64, f64),
    pub bank_r: (f64, f64),
    pub scree: f64,
    pub boulders: f64,
    pub grit: f64,
}

impl Default for FootAsk {
    fn default() -> Self {
        Self {
            bank_h: APRON_H_MM,
            bank_r: APRON_R_MM,
            scree: 55.0,
            boulders: 2.5,
            grit: 45.0,
        }
    }
}

/// Canonical-to-world placement.
pub struct Place<'a> {
    pub world: &'a dyn Fn([f64; 2]) -> [f64; 2],
    pub matrix: [f64; 12],
    pub ground: &'a dt_kit::Field,
    pub foot: f64,
    pub scale: f64,
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn build(
    scene: &mut Scene,
    owner: &str,
    rng: &mut Rng,
    fm: &Form,
    s: &Field,
    zone: &Zones,
    pl: &Place<'_>,
    ask: &FootAsk,
    seed: u64,
) -> Result<(), LookError> {
    let (nx, ny) = (s.nx, s.ny);
    let sc = pl.scale;
    let rock = super::util::Mask {
        nx,
        ny,
        v: s.v.iter().map(|&z| z > FOOT + 0.5).collect(),
    };
    let dist = {
        let d = edt_out(&rock);
        Field {
            nx,
            ny,
            v: d.v.iter().map(|x| x * GRID_MM).collect(),
        }
    };
    // the bank
    let near = dilate_disc(s, ask.bank_r.1 * sc / GRID_MM);
    let mut bank = Field::new(nx, ny, 0.0);
    let (hmin, hmax) = (ask.bank_h.0 * sc, ask.bank_h.1 * sc);
    let (rmin, rmax) = (ask.bank_r.0 * sc, ask.bank_r.1 * sc);
    for j in 0..ny {
        for i in 0..nx {
            let x = (i as f64 + 0.5) * GRID_MM;
            let u = (zone.at(x) - ZONE_FLOOR) / (1.0 - ZONE_FLOOR);
            let e = smooth01(x.min(fm.w - x) / (2.0 * END_MM * sc));
            let hh = APRON_END.0 * sc + e * (lerp((hmin, hmax), u) - APRON_END.0 * sc);
            let rr = APRON_END.1 * sc + e * (lerp((rmin, rmax), u) - APRON_END.1 * sc);
            let q = j * nx + i;
            let b = hh * (1.0 - dist.v[q] / rr).clamp(0.0, 1.0).powf(APRON_EXP);
            bank.v[q] = b.min(APRON_SHARE * (near.v[q] - FOOT).max(0.0));
        }
    }
    // the ground (world relief) plus the bank, in the cliff's frame
    let mut top = Field::new(nx, ny, pl.foot - 0.2);
    let reach = rmax + 1.0;
    let [tw, th] = [
        (pl.ground.nx as f64 - 1.0) * pl.ground.step,
        (pl.ground.ny as f64 - 1.0) * pl.ground.step,
    ];
    for j in 0..ny {
        for i in 0..nx {
            let q = j * nx + i;
            if dist.v[q] <= reach {
                let w = (pl.world)([(i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM]);
                let g = pl.ground.sample(w[0].clamp(0.0, tw), w[1].clamp(0.0, th));
                top.v[q] = (pl.foot - 0.2).max(g + bank.v[q] - 0.15);
            }
        }
    }
    let zb = pl.foot - 1.0;
    let depth: Vec<f64> = top.v.iter().map(|&t| t - zb).collect();
    let mut bm = hung(&depth, nx, ny, GRID_MM, 0.02).map_err(LookError::build)?;
    for p in &mut bm.positions {
        *p = [p[0] + 0.5 * GRID_MM, p[1] + 0.5 * GRID_MM, p[2] + zb];
    }
    let n_slots = 1;
    let slots = scene
        .slots(n_slots, Material::Earth)
        .map_err(LookError::build)?;
    let piece = Piece::new(slots.get(0), &bm).map_err(LookError::build)?;
    scene.add(
        Tag {
            kind: "bank",
            owner: owner.to_owned(),
        },
        piece.transformed(&pl.matrix_xy()).into_solid(),
    );
    // scree: ground cells touching the rock, each with its outward direction
    let mut foot_cells: Vec<([f64; 2], [f64; 2])> = Vec::new();
    for j in 1..ny - 1 {
        for i in 1..nx - 1 {
            let d = dist.at(i, j);
            if d > 0.0 && d <= 0.5 {
                let gx = dist.at(i + 1, j) - dist.at(i - 1, j);
                let gy = dist.at(i, j + 1) - dist.at(i, j - 1);
                let l = libm::hypot(gx, gy).max(1e-9);
                foot_cells.push((
                    [(i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM],
                    [gx / l, gy / l],
                ));
            }
        }
    }
    if foot_cells.is_empty() {
        return Ok(());
    }
    let wts: Vec<f64> = foot_cells.iter().map(|c| 0.5 + zone.at(c.0[0])).collect();
    let per = foot_cells.len() as f64 * GRID_MM / (SQUARE_MM * sc);
    // (cell, D, reach, block, grit)
    let mut sites: Vec<(usize, f64, f64, bool, bool)> = Vec::new();
    let nsc = (ask.scree * per).round() as usize;
    for _ in 0..nsc {
        let i = rng.pick(&wts);
        let d = lerp(SCREE_D_MM, rng.random().powf(SCREE_D_EXP));
        sites.push((i, d, rng.expo(SCREE_REACH_MM), false, false));
    }
    for _ in 0..(ask.boulders * per).round() as usize {
        let i = rng.pick(&wts);
        sites.push((
            i,
            rng.uniform(SCREE_BLOCK_MM.0, SCREE_BLOCK_MM.1),
            rng.expo(0.3 * SCREE_REACH_MM),
            true,
            false,
        ));
    }
    for _ in 0..(ask.grit * per).round() as usize {
        let i = rng.pick(&wts);
        sites.push((
            i,
            rng.uniform(GRIT_D_MM.0, GRIT_D_MM.1),
            rng.uniform(0.0, rmin),
            false,
            true,
        ));
    }
    // shuffle, then big first
    for k in (1..sites.len()).rev() {
        let j = rng.int(0, k as i64) as usize;
        sites.swap(k, j);
    }
    sites.sort_by(|a, b| b.1.total_cmp(&a.1));
    let g = Grain {
        amp_mm: ROCK_GRAIN_MM,
        ..Grain::default()
    };
    let mut placed: Vec<(f64, f64, f64)> = Vec::new();
    let mut made: Vec<(dt_kit::TriMesh, [f64; 2], f64, f64)> = Vec::new();
    for (ci, d, rch, _block, grit_) in sites {
        let (p, o) = foot_cells[ci];
        let x = p[0] + o[0] * (d * (0.5 - SCREE_LEAN) + rch);
        let y = p[1] + o[1] * (d * (0.5 - SCREE_LEAN) + rch);
        if !(d / 2.0 + 0.3 < x
            && x < fm.w - d / 2.0 - 0.3
            && d / 2.0 + 0.3 < y
            && y < fm.h - d / 2.0 - 0.3)
        {
            continue;
        }
        if placed
            .iter()
            .any(|&(a, b, dd)| libm::hypot(x - a, y - b) < SCREE_GAP * (d + dd))
        {
            continue;
        }
        let stone = crate::rock::make_stone(rng, d, SCREE_MIX)?.scaled(sc, 1.0);
        let mesh = grain(
            &stone.convex,
            &g,
            seed ^ (placed.len() as u64).wrapping_mul(0x9E37_79B9),
        )
        .map_err(LookError::build)?;
        // seat on the lowest ground (bank included) under it
        let r = 0.5 * d * sc;
        let (i0, i1) = (
            ((x - r) / GRID_MM).max(0.0) as usize,
            (((x + r) / GRID_MM) as usize).min(nx - 1),
        );
        let (j0, j1) = (
            ((y - r) / GRID_MM).max(0.0) as usize,
            (((y + r) / GRID_MM) as usize).min(ny - 1),
        );
        let mut zmin = f64::INFINITY;
        for j in j0..=j1 {
            for i in i0..=i1 {
                zmin = zmin.min(top.at(i, j));
            }
        }
        let mut z0 = zmin - SCREE_SINK_MM;
        if grit_ {
            z0 -= rng.uniform(GRIT_BURY.0, GRIT_BURY.1) * d;
        }
        let yaw = rng.uniform(0.0, 360.0);
        made.push((mesh, [x, y], z0, yaw));
        placed.push((x, y, d));
    }
    let stone_slots = scene
        .slots(made.len() as u32, Material::Rock)
        .map_err(LookError::build)?;
    for (k, (mesh, at, z0, yaw)) in made.into_iter().enumerate() {
        let w = (pl.world)(at);
        let sol = form::solid(stone_slots.get(k as u32), &mesh)
            .map_err(LookError::build)?
            .turn(yaw, 1.0)
            .translate([w[0], w[1], z0]);
        scene.add(
            Tag {
                kind: "rock",
                owner: owner.to_owned(),
            },
            sol,
        );
    }
    Ok(())
}

impl Place<'_> {
    /// The map with the z shift removed (the bank holds absolute z).
    fn matrix_xy(&self) -> [f64; 12] {
        let mut m = self.matrix;
        m[11] = 0.0;
        m
    }
}

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
