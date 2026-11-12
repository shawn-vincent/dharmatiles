//! What a build shows of its own working, only when asked, in two kinds.
//! The editor's *clouds* (a body's low and high height sheets, the span at
//! each tree tip or stone) are what a selected `top=` or pin is drawn from:
//! collected whenever the build is for the editor.  The debug layer's
//! *marks* (classed points and polylines: grass starts, spines) only when
//! the page asks for the layer.  Elements add them through
//! [`Scene::clouds`] and [`Scene::marks`]; with a kind off the accessor is
//! `None` and nothing is made.  A mark is never geometry and never draws
//! from a random stream, so a build is the same with or without them: same
//! solids, same hashes.
//!
//! [`Scene::clouds`]: crate::Scene::clouds
//! [`Scene::marks`]: crate::Scene::marks

use dt_geom::Weighting;

use crate::{EntryId, Field, Mask};

/// Which marks a build collects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Collect {
    /// Clouds: height sheets and the span at a point ([`Debug::sheets`],
    /// [`Debug::columns`]).
    pub clouds: bool,
    /// The debug layer's marks: classed points and polylines
    /// ([`Debug::dots`], [`Debug::trails`]).
    pub marks: bool,
}

impl Collect {
    /// Whether everything `want` asks for is collected here.
    #[must_use]
    pub fn covers(self, want: Collect) -> bool {
        (self.clouds || !want.clouds) && (self.marks || !want.marks)
    }

    /// Whether anything is collected.
    #[must_use]
    pub fn any(self) -> bool {
        self.clouds || self.marks
    }
}

/// The marks of one build, in tile mm (z is world mm).
#[derive(Clone, Debug, Default)]
pub struct Debug {
    /// Classed points.
    pub dots: Vec<Dot>,
    /// Classed polylines.
    pub trails: Vec<Trail>,
    /// Translucent height sheets.
    pub sheets: Vec<Sheet>,
    /// Clouds at a point: a tree's branch tip, a rock's middle.
    pub columns: Vec<Column>,
}

/// A point, classed by what it is (`grew`, `crowded`, …).
#[derive(Clone, Debug)]
pub struct Dot {
    /// The stack entry that made it.
    pub owner: EntryId,
    /// Its class: the legend's key.
    pub class: &'static str,
    /// Where, mm.
    pub at: [f64; 3],
}

/// A polyline, classed (`spine`, `flow`, …).
#[derive(Clone, Debug)]
pub struct Trail {
    /// The stack entry that made it.
    pub owner: EntryId,
    /// Its class: the legend's key.
    pub class: &'static str,
    /// Its points, mm.
    pub points: Vec<[f64; 3]>,
}

/// A height sheet over a region: a band's low or high.
#[derive(Clone, Debug)]
pub struct Sheet {
    /// The stack entry it belongs to.
    pub owner: EntryId,
    /// Its class (`band-lo`, `band-hi`).
    pub class: &'static str,
    /// Heights, world mm.
    pub z: Field,
    /// Where it is drawn: the element's region, on the same lattice.
    pub region: Mask,
    /// How often the height this sheet stands at comes up at each node, 0
    /// to 1 ([`shade_of`]): one per node of `z`, 0.5 where the cloud has no
    /// odds.
    pub shade: Vec<f32>,
}

/// A cloud at one place, a tree's branch tip or a rock's middle: its low and
/// high, the height the element's roll placed it at and how often each of
/// the three comes up.
#[derive(Clone, Debug)]
pub struct Column {
    /// The stack entry that made it.
    pub owner: EntryId,
    /// Its class (`tip-cloud`, `stone-cloud`).
    pub class: &'static str,
    /// Where, mm from the tile's south-west corner.
    pub at: [f64; 2],
    /// The low and high surfaces there, world mm.
    pub span: [f64; 2],
    /// The height the roll gave, world mm.
    pub rolled: f64,
    /// [`shade_of`] at the low, at the high and at the rolled height.
    pub shade: [f32; 3],
}

/// How often the height at position `u` along a cloud's range (0 the low
/// surface, 1 the high) comes up, 0 to 1: the weighting's density there as a
/// multiple of a flat range's, `d`, squeezed as `d / (1 + d)`, so a flat
/// range (or none) is 0.5, a height that never comes up 0 and a likely one
/// near 1.  A list has values, not slices: the share of its draws within a
/// slice's width of `u`, as a multiple of a slice's share.
#[must_use]
pub fn shade_of(w: Option<&Weighting>, u: f64) -> f32 {
    const BINS: usize = 20;
    let Some(w) = w else {
        return 0.5;
    };
    let u = u.clamp(0.0, 1.0);
    let d = if let Some(masses) = w.masses() {
        let half = 0.5 / BINS as f64;
        masses
            .iter()
            .filter(|(t, _)| (t - u).abs() <= half)
            .map(|(_, share)| share)
            .sum::<f64>()
            * BINS as f64
    } else if let Some(bins) = w.density(BINS) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = ((u * BINS as f64) as usize).min(BINS - 1);
        bins[k]
    } else {
        1.0
    };
    #[allow(clippy::cast_possible_truncation)]
    {
        (d / (1.0 + d)) as f32
    }
}

impl Debug {
    /// Add a point.
    pub fn dot(&mut self, owner: &EntryId, class: &'static str, at: [f64; 3]) {
        self.dots.push(Dot {
            owner: owner.clone(),
            class,
            at,
        });
    }

    /// Add a polyline; fewer than two points add nothing.
    pub fn trail(&mut self, owner: &EntryId, class: &'static str, points: Vec<[f64; 3]>) {
        if points.len() >= 2 {
            self.trails.push(Trail {
                owner: owner.clone(),
                class,
                points,
            });
        }
    }

    /// Add a height sheet over `region`.
    pub fn sheet(
        &mut self,
        owner: &EntryId,
        class: &'static str,
        z: Field,
        region: Mask,
        shade: Vec<f32>,
    ) {
        self.sheets.push(Sheet {
            owner: owner.clone(),
            class,
            z,
            region,
            shade,
        });
    }

    /// Add a cloud at a point.
    pub fn column(&mut self, c: Column) {
        self.columns.push(c);
    }
}
