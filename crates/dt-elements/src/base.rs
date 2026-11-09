//! The base, shared by both connectors: the tile's footprint at zero, and
//! where a cut may not go into the connector.
//!
//! The footprint at zero is the tile's plan with its four vertical corners
//! rounded by [`dt_kit::Frame::corner_round_mm`], on the very nodes the
//! terrain's rim has there ([`dt_kit::Frame::corner_arc_mm`]), so the
//! terrain sits flush on the base. Each connector is
//! built from it: OpenLOCK extrudes it down as its slab, and every
//! DungeonBlocks square takes its part of it as the top of its flare.
//!
//! The keep-out boxes say what a cut into the base may not touch (see
//! [`KeepOut`], [`refuse_cut`]); the build refuses with `dt::connector`
//! when cuts exist to check (they come after the rebuild).

use dt_kit::{Frame, Outline};

/// A box, mm, `[x, y, z]` low and high corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    /// Low corner.
    pub min: [f64; 3],
    /// High corner.
    pub max: [f64; 3],
}

impl Box3 {
    /// True when the two boxes share volume.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        (0..3).all(|k| self.min[k] < other.max[k] && other.min[k] < self.max[k])
    }
}

/// A part of the connector a cut may not reach: a box (a conservative
/// bound, wall thickness included) and what it protects.
#[derive(Clone, Debug, PartialEq)]
pub struct KeepOut {
    /// The box.
    pub at: Box3,
    /// What it protects, for the message.
    pub what: &'static str,
}

/// The first keep-out a cut's bounds touch, if any: the `dt::connector`
/// refusal, for the build to word once cuts exist. A cut wholly above zero
/// never touches one.
#[must_use]
pub fn refuse_cut<'a>(keep: &'a [KeepOut], cut: &Box3) -> Option<&'a KeepOut> {
    keep.iter().find(|k| k.at.overlaps(cut))
}

/// One closed ring of the plan rectangle `lo..hi`, counter-clockwise from
/// the south-west, with each corner whose `round[k]` is set (0 SW, 1 SE,
/// 2 NE, 3 NW) replaced by the corner `arc` ([`arc`], south-west, turned
/// to the corner); a corner not rounded is one point. The point count depends only on `round`.
#[must_use]
pub fn ring(lo: [f64; 2], hi: [f64; 2], arc: &[[f64; 2]], round: [bool; 4]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for (k, corner) in [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]]
        .into_iter()
        .enumerate()
    {
        if !round[k] {
            out.push(corner);
            continue;
        }
        // the arc runs west to south; mirrored in x or y it runs the other
        // way round, and is reversed to stay counter-clockwise
        let (sx, sy) = ([1.0, -1.0, -1.0, 1.0][k], [1.0, 1.0, -1.0, -1.0][k]);
        let pts = arc
            .iter()
            .map(|p| [corner[0] + sx * p[0], corner[1] + sy * p[1]]);
        if sx * sy > 0.0 {
            out.extend(pts);
        } else {
            out.extend(pts.rev());
        }
    }
    out
}

/// Segments in a corner's arc: odd, so a vertex of the polygon (not the
/// middle of a side) is at the terrain's corner node, which then lies
/// inside the polygon, not on its edge.
const ARC_SEGMENTS: usize = 7;

/// The plan of the tile's south-west corner, mm from it, from the west edge
/// to the south edge (counter-clockwise): a polygon of [`ARC_SEGMENTS`]
/// sides circumscribed about the quarter circle of
/// [`dt_kit::Frame::corner_round_mm`], so it lies outside the terrain's rim
/// there (whose nodes are on the circle) by no more than the lift's error
/// bound: the terrain sits on the base with no ledge to speak of, and no
/// vertex of the two meets.
#[must_use]
pub fn arc(frame: &Frame) -> Vec<[f64; 2]> {
    let r = frame.corner_round_mm();
    if r <= 0.0 {
        return vec![[0.0, 0.0]];
    }
    #[allow(clippy::cast_precision_loss)]
    let theta = std::f64::consts::FRAC_PI_2 / ARC_SEGMENTS as f64;
    let big = r / libm::cos(theta / 2.0);
    (0..ARC_SEGMENTS)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let a = std::f64::consts::PI + (i as f64 + 0.5) * theta;
            [r + big * libm::cos(a), r + big * libm::sin(a)]
        })
        .collect()
}

/// The tile's footprint at zero: its plan, the four vertical corners rounded
/// as the terrain's are.
#[must_use]
pub fn footprint(frame: &Frame) -> Outline {
    let [w, h] = frame.size_mm();
    Outline {
        contours: vec![ring([0.0, 0.0], [w, h], &arc(frame), [true; 4])],
    }
}

/// Which corners of square `(col, row)` are corners of the tile.
#[must_use]
pub fn tile_corners(frame: &Frame, col: u32, row: u32) -> [bool; 4] {
    let (west, east) = (col == 0, col + 1 == frame.cols);
    let (south, north) = (row == 0, row + 1 == frame.rows);
    [west && south, east && south, east && north, west && north]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn the_footprint_is_the_square_with_its_corners_rounded() {
        let f = Frame::dungeonblocks(2, 1);
        let o = footprint(&f);
        let p = &o.contours[0];
        assert_eq!(p.len(), 4 * arc(&f).len());
        // inside the tile, and no corner left
        let [w, h] = f.size_mm();
        assert!(
            p.iter()
                .all(|q| q[0] >= 0.0 && q[0] <= w && q[1] >= 0.0 && q[1] <= h)
        );
        assert!(!p.contains(&[0.0, 0.0]));
        // counter-clockwise, and its area the rectangle less four corners of
        // about r^2 (1 - pi/4)
        let r = f.corner_round_mm();
        let a: f64 = p
            .iter()
            .zip(p.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum::<f64>()
            / 2.0;
        let want = w * h - 4.0 * r * r * (1.0 - std::f64::consts::FRAC_PI_4);
        assert!((a - want).abs() < 0.05, "{a} vs {want}");
    }

    #[test]
    fn only_the_tile_corners_are_rounded() {
        let f = Frame::dungeonblocks(2, 2);
        assert_eq!(tile_corners(&f, 0, 0), [true, false, false, false]);
        assert_eq!(tile_corners(&f, 1, 1), [false, false, true, false]);
        let one = Frame::dungeonblocks(1, 1);
        assert_eq!(tile_corners(&one, 0, 0), [true; 4]);
        let arc = arc(&one);
        let n_pts = |c| ring([0.0; 2], [35.0; 2], &arc, c).len();
        assert_eq!(n_pts([false; 4]), 4);
        assert_eq!(n_pts([true, false, false, false]), 3 + arc.len());
    }

    #[test]
    fn a_cut_is_refused_where_it_meets_a_keep_out() {
        let keep = [KeepOut {
            at: Box3 {
                min: [0.0, 0.0, -5.0],
                max: [1.0, 10.0, 0.0],
            },
            what: "a wall",
        }];
        let cut = |x0: f64, x1: f64| Box3 {
            min: [x0, 2.0, -3.0],
            max: [x1, 4.0, 1.0],
        };
        assert!(refuse_cut(&keep, &cut(0.5, 3.0)).is_some());
        assert!(refuse_cut(&keep, &cut(1.0, 3.0)).is_none());
    }
}
