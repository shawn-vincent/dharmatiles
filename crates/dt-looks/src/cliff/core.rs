//! The core: the pile's height field S set in CORE_SET from every open
//! face and CORE_DROP under every top, its sides on the tile boundary
//! CORE_RIM in. Cleaving never cuts into it; it shows only where
//! neighbours' rounded corners leave a gap, as the gap's floor, never a
//! hole through the pile (rockpile.py :1800-1826).

use dt_kit::{Convex, KitError, Piece, Slot, hung};

use super::faces::Column;
use super::form::{FOOT, Form, GRID_MM};
use super::stack::Placed;
use super::util::{Field, bounds, dilate_disc, erode_disc};

const CORE_SET_MM: f64 = 2.0;
const CORE_RIM_MM: f64 = 2.8;
const CORE_END_RIM_MM: f64 = 0.5;
const CORE_DEEP_MM: f64 = 12.0;
const CORE_DROP_MM: f64 = 0.5;

pub fn core_mesh( s: &Field, cols: &[Column], placed: &[Placed]) -> Result<dt_kit::TriMesh, KitError> {
    let (nx, ny) = (s.nx, s.ny);
    let r0 = CORE_SET_MM / GRID_MM;
    let mut c = erode_disc(s, r0);
    for col in cols {
        let b = bounds(&col.poly);
        let r = col.step + 0.5 * col.back.iter().map(|x| x.1).fold(0.0, f64::max);
        if r > 0.5 && (b[2] - b[0]).min(b[3] - b[1]) >= CORE_DEEP_MM {
            let mut cells = vec![false; nx * ny];
            for &st in &col.stones {
                for q in 0..nx * ny {
                    cells[q] |= placed[st].cells.v[q];
                }
            }
            let rr = (CORE_SET_MM + r) / GRID_MM;
            let e = erode_disc(s, rr);
            for q in 0..nx * ny {
                if cells[q] {
                    c.v[q] = c.v[q].min(e.v[q]);
                }
            }
        }
    }
    let _ = dilate_disc;
    let zb = FOOT - 1.5;
    let depth: Vec<f64> = c.v.iter().map(|&v| (v - CORE_DROP_MM).max(zb + 0.05) - zb).collect();
    let mut m = hung(&depth, nx, ny, GRID_MM, 0.01)?;
    for p in &mut m.positions {
        *p = [p[0] + 0.5 * GRID_MM, p[1] + 0.5 * GRID_MM, p[2] + zb];
    }
    Ok(m)
}

/// The core solid, clipped to the tile less its rim.
pub fn core_piece(slot: Slot, mesh: &dt_kit::TriMesh, f: &Form) -> Result<Piece, KitError> {
    let zb = FOOT - 1.5;
    let rim: Vec<f64> = [false, true]
        .iter()
        .flat_map(|&hi| {
            [0usize, 1].map(move |ax| {
                if f.rims.is_empty() || f.rims.contains(&(ax, hi)) {
                    CORE_RIM_MM
                } else {
                    CORE_END_RIM_MM
                }
            })
        })
        .collect();
    let core = Piece::new(slot, mesh)?;
    let bx = Convex::cuboid([rim[0], rim[1], zb - 1.0], [f.w - rim[2], f.h - rim[3], zb - 1.0 + 200.0])
        .to_tri_mesh();
    core.within(&bx)
}
