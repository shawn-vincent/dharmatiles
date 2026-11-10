//! What a build shows of its own working, only when asked: the editor's
//! debug layer.  Elements add typed marks (classed points, classed
//! polylines, translucent height sheets) through [`Scene::debug`]; with the
//! layer off it is `None` and nothing is made.  A mark is never geometry and
//! never draws from a random stream, so a build is the same with or without
//! it: same solids, same hashes.
//!
//! [`Scene::debug`]: crate::Scene::debug

use crate::{Field, Mask};

/// The marks of one build, in tile mm (z is world mm).
#[derive(Clone, Debug, Default)]
pub struct Debug {
    /// Classed points.
    pub dots: Vec<Dot>,
    /// Classed polylines.
    pub trails: Vec<Trail>,
    /// Translucent height sheets.
    pub sheets: Vec<Sheet>,
}

/// A point, classed by what it is (`grew`, `crowded`, …).
#[derive(Clone, Debug)]
pub struct Dot {
    /// The stack entry that made it (`stack/2:grass`).
    pub owner: String,
    /// Its class: the legend's key.
    pub class: &'static str,
    /// Where, mm.
    pub at: [f64; 3],
}

/// A polyline, classed (`spine`, `flow`, …).
#[derive(Clone, Debug)]
pub struct Trail {
    /// The stack entry that made it.
    pub owner: String,
    /// Its class: the legend's key.
    pub class: &'static str,
    /// Its points, mm.
    pub points: Vec<[f64; 3]>,
}

/// A height sheet over a region: a band's low or high.
#[derive(Clone, Debug)]
pub struct Sheet {
    /// The stack entry it belongs to.
    pub owner: String,
    /// Its place in the stack, for its hue.
    pub entry: usize,
    /// Its class (`band-lo`, `band-hi`).
    pub class: &'static str,
    /// Heights, world mm.
    pub z: Field,
    /// Where it is drawn: the element's region, on the same lattice.
    pub region: Mask,
}

impl Debug {
    /// Add a point.
    pub fn dot(&mut self, owner: &str, class: &'static str, at: [f64; 3]) {
        self.dots.push(Dot {
            owner: owner.to_owned(),
            class,
            at,
        });
    }

    /// Add a polyline; fewer than two points add nothing.
    pub fn trail(&mut self, owner: &str, class: &'static str, points: Vec<[f64; 3]>) {
        if points.len() >= 2 {
            self.trails.push(Trail {
                owner: owner.to_owned(),
                class,
                points,
            });
        }
    }

    /// Add a height sheet over `region`.
    pub fn sheet(
        &mut self,
        owner: &str,
        entry: usize,
        class: &'static str,
        z: Field,
        region: Mask,
    ) {
        self.sheets.push(Sheet {
            owner: owner.to_owned(),
            entry,
            class,
            z,
            region,
        });
    }
}
