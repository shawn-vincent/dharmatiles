//! **Land**: the laid terrain as the things standing on it see it.
//!
//! One view over the land as it is laid (every land layer, the grain set in
//! and added back, the edge-round) and the solids that stand on it. Three
//! questions, and nothing else:
//!
//! - [`Land::top`]: the height of the land at a point.
//! - [`Land::seat`]: where a thing's flat foot must be set to stand on that
//!   land, so it neither hovers (no air under its foot: an overhang the
//!   printer would have to bridge) nor sinks further than the rule says.
//! - [`Land::things_at`]: the real horizontal sections of the things standing
//!   at a height: what water laps against at its surface.
//!
//! [`Land::hover`] measures a standing thing's seat: the gap under its foot.

use dt_geom::{Mask, fill_polygons};

use crate::{Field, Frame, Kind, Part, Solid};

/// What stands, as opposed to what is laid or grows over: the kinds
/// [`Land::things_at`] sections.
pub const THINGS: [Kind; 5] = [
    Kind::Rock,
    Kind::Reeds,
    Kind::Flowers,
    Kind::Tree,
    Kind::Wall,
];

/// The footprint a thing seats by: a disc round its centre, world mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    /// Its centre.
    pub at: [f64; 2],
    /// Its radius: the farthest its foot or body reaches from the centre.
    pub r: f64,
}

/// How a thing sits: how far under the land it is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatRule {
    /// At least this far under the lowest land its disc touches, mm (so no
    /// dip or slope exposes the foot).
    pub below_min: f64,
    /// And at least this far under the median of that land, mm.
    pub below_median: f64,
}

/// A seat: the height of the foot, and the land it was found from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    /// The height to set the foot at, world mm; never under 0 (the base).
    pub z: f64,
    /// The lowest land the disc touches.
    pub lowest: f64,
    /// The median of the land the disc touches.
    pub median: f64,
    /// The highest land the disc touches.
    pub highest: f64,
}

/// The land and what stands on it.
pub struct Land<'a> {
    frame: Frame,
    top: Field,
    parts: &'a [Part],
}

impl<'a> Land<'a> {
    pub(crate) fn new(frame: Frame, top: Field, parts: &'a [Part]) -> Self {
        Self { frame, top, parts }
    }

    /// The land's height at every node, world mm.
    #[must_use]
    pub const fn field(&self) -> &Field {
        &self.top
    }

    /// The height of the land at `(x, y)`: between nodes, as the lift
    /// triangulates it (bilinear), within [`crate::LIFT_TOL_MM`].
    #[must_use]
    pub fn top(&self, x: f64, y: f64) -> f64 {
        self.top.sample(x, y)
    }

    /// **seat**: the height to set a foot at, so that it stands in the land
    /// under `disc` (see [`seat_on`]).
    #[must_use]
    pub fn seat(&self, disc: Disc, rule: SeatRule) -> Seat {
        seat_on(&self.top, disc, rule)
    }

    /// **things at**: the nodes inside the section, at height `z`, of any
    /// thing standing in the scene (rocks, reeds, flowers, trees, walls): a
    /// thing that does not reach `z` has no section there.
    #[must_use]
    pub fn things_at(&self, z: f64) -> Mask {
        let (nx, ny) = self.frame.nodes();
        let step = self.frame.step_mm();
        let mut out = Mask::new(nx, ny, false);
        for p in self.parts.iter().filter(|p| THINGS.contains(&p.tag.kind)) {
            let m = fill_polygons(nx, ny, step, &p.solid.slice(z));
            out.bits.iter_mut().zip(m.bits).for_each(|(o, b)| *o |= b);
        }
        out
    }

    /// **hover**: how far the land lies under the foot of a standing
    /// `solid`: the most, over the nodes inside its foot and along its
    /// outline, that its lowest point is above the land. Zero when it rests
    /// on the land everywhere.
    #[must_use]
    pub fn hover(&self, solid: &Solid) -> f64 {
        let Some((lo, _)) = solid.bounds() else {
            return 0.0;
        };
        let foot = lo[2];
        // the outline just over the foot (a foot is flat, its section the
        // same a hair above it)
        let rings = solid.slice(foot + 0.05);
        let (nx, ny) = self.frame.nodes();
        let inside = fill_polygons(nx, ny, self.frame.step_mm(), &rings);
        let mut gap = 0.0_f64;
        for (k, _) in inside.bits.iter().enumerate().filter(|(_, b)| **b) {
            gap = gap.max(foot - self.top.z[k]);
        }
        for p in rings.iter().flatten() {
            gap = gap.max(foot - self.top.sample(p[0], p[1]));
        }
        gap
    }
}

/// **seat on** a ground field: the height of a foot over `disc`, set under
/// the lowest node within the disc by `rule.below_min` (so no dip or slope
/// exposes its flat foot) and under their median by `rule.below_median`,
/// whichever is lower; never under 0, the tile's base. [`Land::seat`] on the
/// laid land; dirt's bedded stones seat on the surface they are laid into.
#[must_use]
pub fn seat_on(ground: &Field, disc: Disc, rule: SeatRule) -> Seat {
    let [x, y] = disc.at;
    let (i0, j0) = ground.node(x - disc.r, y - disc.r);
    let (i1, j1) = ground.node(x + disc.r, y + disc.r);
    let mut under = Vec::new();
    for j in j0..=j1 {
        for i in i0..=i1 {
            let (dx, dy) = (ground.x(i) - x, ground.y(j) - y);
            if dx * dx + dy * dy <= disc.r * disc.r {
                under.push(ground.at(i, j));
            }
        }
    }
    if under.is_empty() {
        under.push(ground.sample(x, y));
    }
    under.sort_by(f64::total_cmp);
    let (lowest, median, highest) = (under[0], under[under.len() / 2], under[under.len() - 1]);
    Seat {
        z: (lowest - rule.below_min)
            .min(median - rule.below_median)
            .max(0.0),
        lowest,
        median,
        highest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OnFrame;

    fn land(f: &Field) -> Land<'static> {
        Land::new(crate::Frame::dungeonblocks(1, 1), f.clone(), &[])
    }

    #[test]
    fn a_seat_beds_below_the_lowest_dip() {
        let f = crate::Frame::dungeonblocks(1, 1);
        let rule = SeatRule {
            below_min: 0.25,
            below_median: 0.55,
        };
        let disc = Disc {
            at: [17.5, 17.5],
            r: 5.0,
        };
        let g = Field::from_fn(&f, |x, _| 4.0 + 0.01 * x);
        let seat = land(&g).seat(disc, rule);
        assert!((seat.z - (4.175 - 0.55)).abs() < 0.03, "{seat:?}");
        let steep = Field::from_fn(&f, |x, _| 4.0 + 0.5 * x);
        let seat = land(&steep).seat(disc, rule);
        assert!(seat.z <= 4.0 + 0.5 * 12.5 - 0.25 + 0.3, "{seat:?}");
        assert!(seat.z <= seat.lowest - 0.25 + 1e-9);
        let flat = Field::new(&f, 0.2);
        assert!(land(&flat).seat(disc, rule).z.abs() < 1e-12);
    }
}
