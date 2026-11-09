//! The core: the pile's height field S set in CORE_SET from every open
//! face and CORE_DROP under every top, its sides on the tile boundary
//! CORE_RIM in. Cleaving never cuts into it; it shows only where
//! neighbours' rounded corners leave a gap, as the gap's floor, never a
//! hole through the pile.
//!
//! The core is the cliff's rock layer in the terrain, laid with the land
//! before the lift: a height field with a vertical face, never carved or
//! grained, so it costs a layer of the lift and not a mesh of millions of
//! triangles. It is also what the dirt film on the tops lies on.

use dt_kit::{KitError, hung};

use super::faces::Column;
use super::form::{FOOT, Form, GRID_MM};
use super::stack::Placed;
use super::util::{Field, bounds};

const CORE_SET_MM: f64 = 2.0;
const CORE_RIM_MM: f64 = 2.8;
const CORE_END_RIM_MM: f64 = 0.5;
const CORE_DEEP_MM: f64 = 12.0;
pub const CORE_DROP_MM: f64 = 0.5;

/// The core's height field: the pile set in from its open faces.
pub fn core_field(s: &Field, cols: &[Column], placed: &[Placed]) -> Field {
    let (nx, ny) = (s.nx, s.ny);
    let r0 = CORE_SET_MM / GRID_MM;
    let mut c = s.erode(r0);
    for col in cols {
        let b = bounds(&col.poly);
        let r = col.step + 0.5 * col.back.iter().map(|x| x.1).fold(0.0, f64::max);
        if r > 0.5 && (b[2] - b[0]).min(b[3] - b[1]) >= CORE_DEEP_MM {
            let mut cells = vec![false; nx * ny];
            for &st in &col.stones {
                for q in 0..nx * ny {
                    cells[q] |= placed[st].cells.z[q];
                }
            }
            let rr = (CORE_SET_MM + r) / GRID_MM;
            let e = s.erode(rr);
            for q in 0..nx * ny {
                if cells[q] {
                    c.z[q] = c.z[q].min(e.z[q]);
                }
            }
        }
    }
    c
}

/// How far in from a tile boundary the core stands: the faces on the
/// cliff's rims carry the stone's relief and stand back from the plane;
/// where the pile ends the core comes nearer.
pub fn rim_mm(f: &Form, ax: usize, hi: bool) -> f64 {
    if f.rims.is_empty() || f.rims.contains(&(ax, hi)) {
        CORE_RIM_MM
    } else {
        CORE_END_RIM_MM
    }
}

/// A mesh of the core field, for the cleaving guard.
pub fn core_mesh(c: &Field) -> Result<dt_kit::TriMesh, KitError> {
    let (nx, ny) = (c.nx, c.ny);
    let zb = FOOT - 1.5;
    let depth: Vec<f64> =
        c.z.iter()
            .map(|&v| (v - CORE_DROP_MM).max(zb + 0.05) - zb)
            .collect();
    let mut m = hung(
        &Field {
            nx,
            ny,
            step: GRID_MM,
            z: depth,
        },
        0.01,
    )?;
    for p in &mut m.positions {
        *p = [p[0] + 0.5 * GRID_MM, p[1] + 0.5 * GRID_MM, p[2] + zb];
    }
    Ok(m)
}
