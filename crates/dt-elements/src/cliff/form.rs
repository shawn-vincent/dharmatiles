//! 1 Form: the footprint the stones fill (the body's region), the target
//! height field T(x, y) the pile grows toward (the body's heaved shape in
//! its band).
//! Heights are above the foot (0). Lengths that size masses scale with `s`
//! (the system's scale, 1 on DungeonBlocks); details never do.

use dt_kit::{Band, Field, Frame, Mask, Rng};

use super::util::{Cells, cell_grid, edt_in};

pub const GRID_MM: f64 = 0.25;
// The pile is planned in its own heights: its ground is 0, and the stones
// are raised to the land they stand on (`foot`) as they are laid.

pub const MIN_H_MM: f64 = 3.5;
/// Past this a band has room (a thinner one is a pin).
const ROOM_MM: f64 = 0.02;
/// The mass of a stone against a tile edge and of a free-standing one,
/// against the reference stone size (`massive` is 1).
const MASS_EDGE: f64 = 1.35;
const MASS_FREE: f64 = 1.2;
/// A tile edge is a rim of the cliff when this share of it is rock.
const RIM_SHARE: f64 = 0.5;
/// A mass along a face, mm at the reference square.
const MASS_W_MM: (f64, f64) = (6.0, 15.0);
/// The share of masses that stand to the full depth, with no shelf.
const FULL_P: f64 = 0.35;
/// How wide a shelf is, mm at the reference square.
const SHELF_MM: (f64, f64) = (4.0, 10.0);
/// A shelf's height, as a share of the top.
const TIER_Z: (f64, f64) = (0.3, 0.7);
/// How far a shelf's height wanders about that, mm.
const TIER_STEP_MM: f64 = 5.0;
/// Shelves are only drawn where the pile is at least this tall, mm.
const SHELF_MIN_TOP_MM: f64 = 2.0 * MIN_H_MM + 3.0;
/// Sub-steps along a mass, and the least width of each, mm at the reference.
const TIER_STEPS: (i64, i64) = (1, 3);
const MIN_STEP_MM: f64 = 6.0;

/// The form: what the pile grows toward.
pub struct Form {
    pub w: f64,
    pub h: f64,
    pub t: Field,
    /// The band's low and high over every cell, above the foot: a stone
    /// that reaches the band stands inside it, so no stone grows over a
    /// place the band jumps across.
    pub lo: Field,
    pub hi: Field,
    /// The one height of a flat band: the plane every stone top touches.
    pub ztop: Option<f64>,
    /// No tile side stands against the pile: it is free-standing (round it,
    /// not along a face).
    pub free_standing: bool,
    pub mass: f64,
    pub along_y: Cells,
    pub kind_p: Option<[f64; 5]>,
    /// (axis, high side) of the tile-boundary planes the cliff stands against.
    pub rims: Vec<(usize, bool)>,
}

/// What a tile asks for.
pub struct Ask<'a> {
    pub frame: &'a Frame,
    /// The body's region, on the lattice.
    pub mask: &'a Mask,
    /// Its solved band.
    pub band: &'a Band,
    /// The body's heaved shape inside the band (world mm), on the lattice.
    pub shape: &'a Field,
    /// The lowest the land under the cliff reaches (world mm): heights are
    /// counted from it.
    pub foot: f64,
    pub mass_gain: f64,
    pub kinds: Option<[f64; 5]>,
    /// Shelves on the free faces: 0 none, 1 the usual, 2 a second step.
    pub ledges: u32,
    /// The height of the shelves, a share of the top, when it is written.
    pub ledge_height: Option<f64>,
    /// The system's scale.
    pub s: f64,
}

pub fn grid_dims(w: f64, h: f64) -> (usize, usize) {
    (
        (w / GRID_MM).round() as usize,
        (h / GRID_MM).round() as usize,
    )
}

/// The form of the pile in `ask`'s region and band.
pub fn make_form(rng: &mut Rng, ask: &Ask<'_>) -> Form {
    let [w, h] = ask.frame.size_mm();
    let (nx, ny) = grid_dims(w, h);
    let (lx, ly) = ask.frame.nodes();
    let step = ask.frame.step_mm();
    let node = |x: f64, y: f64| -> usize {
        let i = ((x / step).round().max(0.0) as usize).min(lx - 1);
        let j = ((y / step).round().max(0.0) as usize).min(ly - 1);
        j * lx + i
    };
    let mut t = cell_grid(nx, ny, 0.0);
    let mut lo = cell_grid(nx, ny, 0.0);
    let mut hi = cell_grid(nx, ny, 0.0);
    let mut want = cell_grid(nx, ny, false);
    for j in 0..ny {
        for i in 0..nx {
            let (x, y) = (super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM));
            let q = j * nx + i;
            // the band over the cell: what it is at every node touching it
            let (is, js) = super::fit::touching(i, j, step, lx, ly);
            let (mut l, mut hh) = (f64::NEG_INFINITY, f64::INFINITY);
            for nj in js {
                for ni in is.clone() {
                    let k = nj * lx + ni;
                    if ask.mask.bits[k] {
                        let iv = ask.band.at(k);
                        l = l.max(iv.lo);
                        hh = hh.min(iv.hi);
                    }
                }
            }
            lo.z[q] = l - ask.foot;
            hi.z[q] = hh - ask.foot;
            if ask.mask.bits[node(x, y)] {
                let top = ask.shape.sample(x, y) - ask.foot;
                if top > MIN_H_MM {
                    t.z[q] = top;
                    want.z[q] = true;
                }
            }
        }
    }
    // a flat band over the whole region: one plane the stones' tops touch
    let (mut flo, mut fhi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut room = false;
    for (k, &b) in ask.mask.bits.iter().enumerate() {
        if b {
            let iv = ask.band.at(k);
            flo = flo.min(iv.hi);
            fhi = fhi.max(iv.hi);
            room |= iv.hi - iv.lo > ROOM_MM;
        }
    }
    let ztop = (!room && fhi - flo < 1e-6 && fhi.is_finite()).then_some(fhi - ask.foot);
    // the tile edges it stands against
    let mut rims = Vec::new();
    for (ax, high) in [(0, false), (0, true), (1, false), (1, true)] {
        let (n, along) = if ax == 0 { (ly, lx) } else { (lx, ly) };
        let at = if high { along - 1 } else { 0 };
        let on = (0..n)
            .filter(|&k| {
                let q = if ax == 0 { k * lx + at } else { at * lx + k };
                ask.mask.bits[q]
            })
            .count();
        if on as f64 >= RIM_SHARE * n as f64 {
            rims.push((ax, high));
        }
    }
    // the stones' long side runs along the face: a face looking east or
    // west has its stones long in y
    let dist = edt_in(&want).blur(2.0 / GRID_MM);
    let mut ay = cell_grid(nx, ny, false);
    for j in 0..ny {
        for i in 0..nx {
            let (i0, i1) = (i.saturating_sub(1), (i + 1).min(nx - 1));
            let (j0, j1) = (j.saturating_sub(1), (j + 1).min(ny - 1));
            let gx = dist.at(i1, j) - dist.at(i0, j);
            let gy = dist.at(i, j1) - dist.at(i, j0);
            ay.z[j * nx + i] = gx.abs() > gy.abs();
        }
    }
    let free_standing = rims.is_empty();
    if ask.ledges > 0 {
        shelves(rng, ask, &want, &mut t, &ay, free_standing);
    }
    Form {
        w,
        h,
        t,
        lo,
        hi,
        ztop,
        free_standing,
        mass: if free_standing { MASS_FREE } else { MASS_EDGE } * ask.mass_gain,
        along_y: ay,
        kind_p: ask.kinds,
        rims,
    }
}

/// Shelves: along a free face the pile is made in masses, and the outer
/// strip of most of them stands lower than the rest, in one to three steps,
/// so a face steps back in tiers a miniature can stand on. Masses are
/// counted along the face (or round it, for a free-standing pile).
fn shelves(
    rng: &mut Rng,
    ask: &Ask<'_>,
    want: &Cells,
    t: &mut Field,
    ay: &Cells,
    free_standing: bool,
) {
    let s = ask.s;
    let (nx, ny) = (want.nx, want.ny);
    let dist = edt_in(want);
    let hi = t.z.iter().copied().fold(0.0, f64::max);
    if hi < SHELF_MIN_TOP_MM {
        return;
    }
    // the coordinate along the face of every cell
    let (cx, cy) = {
        let mut n = 0.0_f64;
        let (mut sx, mut sy) = (0.0, 0.0);
        for q in 0..nx * ny {
            if want.z[q] {
                sx += super::util::cx(q % nx, GRID_MM);
                sy += super::util::cx(q / nx, GRID_MM);
                n += 1.0;
            }
        }
        (sx / n.max(1.0), sy / n.max(1.0))
    };
    let radius =
        libm::sqrt(want.z.iter().filter(|&&b| b).count() as f64 / std::f64::consts::PI) * GRID_MM;
    let along = |q: usize| -> f64 {
        let (x, y) = (
            super::util::cx(q % nx, GRID_MM),
            super::util::cx(q / nx, GRID_MM),
        );
        if free_standing {
            libm::atan2(y - cy, x - cx).rem_euclid(std::f64::consts::TAU) * radius
        } else if ay.z[q] {
            y
        } else {
            x
        }
    };
    let length = if free_standing {
        std::f64::consts::TAU * radius
    } else {
        (nx.max(ny) as f64) * GRID_MM
    };
    let mut edges = vec![0.0];
    while edges.last().copied().unwrap_or(0.0) < length {
        let l = edges.last().copied().unwrap_or(0.0);
        edges.push(l + rng.uniform(MASS_W_MM.0 * s, MASS_W_MM.1 * s));
    }
    let n = edges.len();
    edges[n - 1] = length;
    // each mass: a shelf width and, along it, steps of their own height
    struct Mass {
        inset: f64,
        steps: Vec<(f64, f64)>,
    }
    let masses: Vec<Mass> = (0..edges.len() - 1)
        .map(|k| {
            let (a, b) = (edges[k], edges[k + 1]);
            let inset = if rng.random() < FULL_P {
                0.0
            } else {
                rng.uniform(SHELF_MM.0 * s, SHELF_MM.1 * s)
            };
            let tz = ask
                .ledge_height
                .map_or_else(|| rng.uniform(TIER_Z.0, TIER_Z.1), |h| h.clamp(0.1, 0.9));
            let z_mid = tz * hi;
            let mut k_steps = rng.int(TIER_STEPS.0, TIER_STEPS.1);
            k_steps = k_steps
                .min(((b - a) / (MIN_STEP_MM * s)).floor() as i64)
                .max(1);
            let mut cuts: Vec<f64> = vec![a];
            if k_steps > 1 {
                let free = (b - a) - k_steps as f64 * MIN_STEP_MM * s;
                let mut r: Vec<f64> = (0..k_steps - 1)
                    .map(|_| rng.uniform(0.0, free.max(0.0)))
                    .collect();
                r.sort_by(f64::total_cmp);
                cuts.extend(
                    (1..k_steps)
                        .zip(r)
                        .map(|(m, e)| a + MIN_STEP_MM * s * m as f64 + e),
                );
            }
            let steps = cuts
                .iter()
                .map(|&c0| {
                    let z = (z_mid + rng.uniform(-TIER_STEP_MM, TIER_STEP_MM))
                        .clamp(MIN_H_MM + 1.0, hi - MIN_H_MM - 1.0);
                    (c0, z)
                })
                .collect();
            Mass { inset, steps }
        })
        .collect();
    for q in 0..nx * ny {
        if !want.z[q] {
            continue;
        }
        let a = along(q);
        let k = edges.partition_point(|&e| e <= a).saturating_sub(1);
        let m = &masses[k.min(masses.len() - 1)];
        let d = dist.z[q] * GRID_MM;
        // dist is to the nearest open cell, 1 at the face
        if m.inset <= 0.0 || d > m.inset {
            if ask.ledges < 2 || m.inset <= 0.0 || d > 2.0 * m.inset {
                continue;
            }
            // a second, higher step between the shelf and the top
            let z2 = m.steps.iter().rev().find(|s| s.0 <= a).map_or(hi, |s| s.1);
            t.z[q] = t.z[q].min(0.5 * (z2 + hi));
            continue;
        }
        let z2 = m.steps.iter().rev().find(|s| s.0 <= a).map_or(hi, |s| s.1);
        t.z[q] = t.z[q].min(z2);
    }
}

/// Shattered-zone intensity along the run, ZONE_FLOOR..1.
pub struct Zones {
    cx: Vec<f64>,
    sg: Vec<f64>,
}

pub const ZONE_FLOOR: f64 = 0.25;
const ZONES_PER_SQUARE: f64 = 1.5;
const ZONE_SIGMA_MM: (f64, f64) = (5.0, 10.0);

impl Zones {
    pub fn new(
        rng: &mut Rng,
        w: f64,
        s: f64,
        patches: Option<f64>,
        width: Option<(f64, f64)>,
    ) -> Self {
        let k = patches.map_or(
            ((w / (dt_kit::REFERENCE_SQUARE_MM * s) * ZONES_PER_SQUARE).round() as usize).max(1),
            |p| p.max(0.0) as usize,
        );
        let sig = width.map_or(ZONE_SIGMA_MM, |(a, b)| (0.5 * a, 0.5 * b));
        let cx: Vec<f64> = (0..k).map(|_| rng.uniform(0.0, w)).collect();
        let sg: Vec<f64> = (0..k)
            .map(|_| rng.uniform(sig.0, sig.1.max(sig.0 + 1e-6)))
            .collect();
        Self { cx, sg }
    }
    pub fn at(&self, x: f64) -> f64 {
        let g = self
            .cx
            .iter()
            .zip(&self.sg)
            .map(|(c, s)| libm::exp(-0.5 * ((x - c) / s).powi(2)))
            .fold(0.0, f64::max);
        ZONE_FLOOR + (1.0 - ZONE_FLOOR) * g
    }
}
