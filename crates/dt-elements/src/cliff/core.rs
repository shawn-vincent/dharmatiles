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

use dt_kit::{Frame, KitError, Mask, hung, rays::RayMesh};

use super::faces::Column;
use super::form::{Form, GRID_MM};
use super::stack::Placed;
use super::util::{Field, bounds};

const CORE_SET_MM: f64 = 2.0;
const CORE_RIM_MM: f64 = 2.8;
const CORE_END_RIM_MM: f64 = 0.5;
const CORE_DEEP_MM: f64 = 12.0;
pub const CORE_DROP_MM: f64 = 0.5;
/// How far inside the stones the core's edge stands, and how often down it
/// is tried, mm.
const CORE_CLEAR_MM: f64 = 1.0;
/// How far the core's edge is drawn back from a face beside it, mm.
const CORE_DRAW_BACK_MM: f64 = 0.8;
const CORE_LEVELS_MM: f64 = 1.0;
/// The least top (above the foot) that holds a core, mm.
const CORE_MIN_TOP_MM: f64 = 0.3;
/// The least top (above the foot) a cell of the pile is rock at, mm.
pub const ROCK_MIN_MM: f64 = 0.5;

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
                for (c, &p) in cells.iter_mut().zip(&placed[st].cells.z) {
                    *c |= p;
                }
            }
            let rr = (CORE_SET_MM + r) / GRID_MM;
            let e = s.erode(rr);
            for ((z, &e), &on) in c.z.iter_mut().zip(&e.z).zip(&cells) {
                if on {
                    *z = z.min(e);
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
    let zb = -1.5;
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

/// The core as a mask on the lattice and its top at every node (world mm):
/// the core field set down, inside the rims and above the foot.
pub fn core_mask(
    frame: &Frame,
    fm: &Form,
    core_field: &Field,
    s: &Field,
    foot: f64,
) -> (Mask, Vec<f64>) {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let [w, h] = frame.size_mm();
    let rims = [
        rim_mm(fm, 0, false),
        rim_mm(fm, 0, true),
        rim_mm(fm, 1, false),
        rim_mm(fm, 1, true),
    ];
    let mut mask = Mask::new(nx, ny, false);
    let mut z = vec![0.0; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let (x, y) = (i as f64 * step, j as f64 * step);
            let Some(c) = super::node_cell(x, y, s.nx, s.ny) else {
                continue;
            };
            let inside = x > rims[0] && x < w - rims[1] && y > rims[2] && y < h - rims[3];
            let ct = core_field.z[c] - CORE_DROP_MM;
            if inside && ct > CORE_MIN_TOP_MM {
                mask.bits[j * nx + i] = true;
                z[j * nx + i] = ct + foot;
            }
        }
    }
    (mask, z)
}

/// The core's edge kept clear of the stones' faces: a core wall that
/// grazed a face would leave a hair of it loose between the two. Every node
/// near the core is tried against the finished stones at several heights
/// (deep: inside them all the way down); a node of the core's edge with a
/// deep and a not-deep node within the clearance of it has a face beside
/// it, and the edge is drawn back from it.
///
/// This is the one place the core is cleared from a face; the erosion of
/// [`core_field`] only sets it back from where the pile is planned to end.
pub fn draw_back_from_faces(
    frame: &Frame,
    mask: &mut Mask,
    z: &[f64],
    foot: f64,
    stones: &[RayMesh],
) {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let (to_core, near_core) = dt_kit::distance(frame, &mask.bits);
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (k, near) in near_core.iter().enumerate().take(nx * ny) {
        if to_core.z[k] > 2.0 * CORE_CLEAR_MM {
            continue;
        }
        let Some(src) = *near else { continue };
        let (x, y) = ((k % nx) as f64 * step, (k / nx) as f64 * step);
        let from = pts.len();
        let mut zz = z[src] - foot - CORE_MIN_TOP_MM;
        while zz > ROCK_MIN_MM {
            pts.push([x, y, zz]);
            zz -= CORE_LEVELS_MM;
        }
        pts.push([x, y, ROCK_MIN_MM]);
        spans.push((k, from, pts.len()));
    }
    let mut hit = vec![false; pts.len()];
    for m in stones {
        let rest: Vec<usize> = (0..pts.len()).filter(|&i| !hit[i]).collect();
        if rest.is_empty() {
            break;
        }
        let sub: Vec<[f64; 3]> = rest.iter().map(|&i| pts[i]).collect();
        for (j, h) in m.contains(&sub).into_iter().enumerate() {
            if h {
                hit[rest[j]] = true;
            }
        }
    }
    let mut deep = vec![false; nx * ny];
    let mut tried = vec![false; nx * ny];
    for &(k, a, b) in &spans {
        tried[k] = true;
        deep[k] = hit[a..b].iter().all(|&h| h);
    }
    let (to_deep, _) = dt_kit::distance(frame, &deep);
    let shallow: Vec<bool> = (0..nx * ny).map(|k| tried[k] && !deep[k]).collect();
    let (to_shallow, _) = dt_kit::distance(frame, &shallow);
    let mut bad = vec![false; nx * ny];
    for j in 1..ny - 1 {
        for i in 1..nx - 1 {
            let k = j * nx + i;
            let edge = mask.bits[k]
                && !(mask.bits[k - 1]
                    && mask.bits[k + 1]
                    && mask.bits[k - nx]
                    && mask.bits[k + nx]);
            if edge {
                let other = if deep[k] {
                    to_shallow.z[k]
                } else {
                    to_deep.z[k]
                };
                bad[k] = other < CORE_CLEAR_MM;
            }
        }
    }
    if bad.iter().any(|&b| b) {
        let (d, _) = dt_kit::distance(frame, &bad);
        for k in 0..nx * ny {
            mask.bits[k] &= d.z[k] >= CORE_DRAW_BACK_MM;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cliff::util::cell_grid;

    /// A form standing against some tile sides, with a pile 10 mm high
    /// everywhere: only the rims decide where the core stands.
    fn pile(frame: &Frame, rims: Vec<(usize, bool)>) -> (Form, Field) {
        let [w, h] = frame.size_mm();
        let (nx, ny) = ((w / GRID_MM).ceil() as usize, (h / GRID_MM).ceil() as usize);
        let field = cell_grid(nx, ny, 10.0);
        let form = Form {
            w,
            h,
            t: field.clone(),
            lo: field.clone(),
            hi: field.clone(),
            ztop: None,
            free_standing: rims.is_empty(),
            mass: 1.0,
            along_y: cell_grid(nx, ny, false),
            kind_p: None,
            rims,
        };
        (form, field)
    }

    #[test]
    fn the_core_stands_inside_the_rims_and_nearer_where_the_pile_ends() {
        for frame in [Frame::dungeonblocks(1, 1), Frame::openlock(2, 1)] {
            let (nx, ny) = frame.nodes();
            let [w, h] = frame.size_mm();
            // the cliff stands against the north and west sides
            let rims = vec![(0, false), (1, true)];
            let (fm, field) = pile(&frame, rims);
            let (mask, z) = core_mask(&frame, &fm, &field, &field, 0.0);
            let step = frame.step_mm();
            let (mut west, mut east, mut south, mut north) =
                (f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::INFINITY);
            for j in 0..ny {
                for i in 0..nx {
                    if !mask.bits[j * nx + i] {
                        continue;
                    }
                    let (x, y) = (i as f64 * step, j as f64 * step);
                    west = west.min(x);
                    east = east.min(w - x);
                    south = south.min(y);
                    north = north.min(h - y);
                    assert!(z[j * nx + i] > 0.0);
                }
            }
            // the faces on the rims stand CORE_RIM_MM back, the ends nearer
            assert!(west > CORE_RIM_MM, "west {west}");
            assert!(north > CORE_RIM_MM, "north {north}");
            assert!(east > CORE_END_RIM_MM && east < CORE_RIM_MM, "east {east}");
            assert!(
                south > CORE_END_RIM_MM && south < CORE_RIM_MM,
                "south {south}"
            );
        }
    }
}
