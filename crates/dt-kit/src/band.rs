//! Bands and the band solver.
//!
//! A top is not one height but two: at every node of the lattice a **low**
//! and a **high**, and the body's surface may sit anywhere between them
//! ([`Band`]; a single height is a band of zero width). A body makes its
//! surface only through [`Band::fit`], which clamps a raw field into the
//! band and returns a [`Banded`] field, so "the surface is inside its band"
//! is a property of the types for height-field bodies.
//!
//! A [`Pin`] holds the band to its own height or range at its place.
//! [`solve`] turns a body's own band and its pins into the band the body
//! fills:
//!
//! 1. Pins are read in file order, library defaults first. Where places
//!    overlap, the later pin wins: at a node of its place a pin is exact.
//! 2. Outside its place a pin eases back to the body's own band in the
//!    body's way ([`Ease`]): dirt over `1.5 x drop / tan(slope)` (one eased
//!    pin peaks at 1.5 times its mean slope), water over a fixed distance,
//!    cliffs and floors not at all (their stones and slabs step).
//! 3. Several pins pull by odds: with `w = smooth(1 - distance / reach)`
//!    and `o = w / (1 - w)`, the band is `(own + sum o * pin) / (1 + sum o)`,
//!    low and high separately, so every pin is exact at its place whatever
//!    else pulls there, and `lo <= hi` survives the blend.
//!
//! **The drop** of a pin is how far the band it is written over must come
//! down (or up) to meet the pin's: that band is the body's own, or the band
//! of the earlier pin holding the same node (a bed written over its banks).
//! It is zero where the two bands overlap, since there the pin's edge needs
//! no easing (a stream's banks, a hill drawn as a wide band); otherwise the
//! greatest distance from an end of that band to the pin's band, read at the
//! pin's nearest place node.
//!
//! Refusals ([`Refusal`]) and lints ([`Lint`]) carry numbers: where, how
//! much, which pin. Nothing here draws a random number.

use std::sync::Arc;

use dt_geom::{Iv, Mask, distance_nearest};
use dt_lang::smooth;
use dt_value::Weighting;

use crate::Field;

/// Why a band could not be made or a surface does not fit, with the numbers
/// to say so. Positions are mm from the tile's south-west corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// Fields on different lattices (a bug in the caller).
    Lattice,
    /// A band whose low is above its high.
    Inverted {
        /// Where.
        at: [f64; 2],
        /// The low there.
        lo: f64,
        /// The high there.
        hi: f64,
    },
    /// A slope that is not between 0 and 90 degrees, or a negative or
    /// not-finite settling distance.
    Ease,
    /// A pin whose place holds no node (`dt::empty`).
    EmptyPlace {
        /// The pin, by its place in file order.
        pin: usize,
    },
    /// A body's surface outside its band (`dt::band`, a bug: field bodies
    /// cannot do it, pieces bodies are checked).
    Band {
        /// Where, at its worst.
        at: [f64; 2],
        /// The surface there.
        value: f64,
        /// The band's low there.
        lo: f64,
        /// The band's high there.
        hi: f64,
        /// How far outside it, mm.
        by: f64,
    },
    /// A layer that can never be thick enough (`dt::thin`).
    Thin {
        /// Where, at its worst.
        at: [f64; 2],
        /// The most thickness the bands allow there, mm.
        available: f64,
        /// What is needed (the printer's skin), mm.
        needed: f64,
    },
}

impl Refusal {
    /// The diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Lattice | Self::Inverted { .. } | Self::Ease => dt_lang::codes::BUG,
            Self::EmptyPlace { .. } => dt_lang::codes::EMPTY,
            Self::Band { .. } => dt_lang::codes::BAND,
            Self::Thin { .. } => dt_lang::codes::THIN,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lattice => f.write_str("fields on different lattices"),
            Self::Inverted { at, lo, hi } => write!(
                f,
                "a band's low ({lo:.2} mm) is above its high ({hi:.2} mm) at ({:.2}, {:.2})",
                at[0], at[1]
            ),
            Self::Ease => f.write_str("an ease needs a slope between 0 and 90 degrees"),
            Self::EmptyPlace { pin } => write!(f, "pin {} has an empty place", pin + 1),
            Self::Band {
                at,
                value,
                lo,
                hi,
                by,
            } => write!(
                f,
                "the surface is {value:.2} mm at ({:.2}, {:.2}), {by:.2} mm outside its band \
                 ({lo:.2} to {hi:.2} mm)",
                at[0], at[1]
            ),
            Self::Thin {
                at,
                available,
                needed,
            } => write!(
                f,
                "the layer is at most {available:.2} mm thick at ({:.2}, {:.2}), under the \
                 {needed:.2} mm it needs",
                at[0], at[1]
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// What the solve notes without refusing.
#[derive(Clone, Debug, PartialEq)]
pub enum Lint {
    /// Pins force the surface past the body's slope (`dt::steep`).
    Steep(Steep),
    /// A pin that wins nowhere, because later pins cover all of its place
    /// (`dt::shadowed`).
    Shadowed(Shadowed),
}

impl Lint {
    /// The diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Steep(_) => dt_lang::codes::STEEP,
            Self::Shadowed(_) => dt_lang::codes::SHADOWED,
        }
    }
}

/// Where the bands force a slope past the limit: the worst pair of
/// neighbouring nodes, and how many pairs there are.
#[derive(Clone, Debug, PartialEq)]
pub struct Steep {
    /// The lower node of the worst pair, mm.
    pub at: [f64; 2],
    /// The upper node, mm.
    pub to: [f64; 2],
    /// The least slope any surface in the band can have between them, degrees.
    pub deg: f64,
    /// The limit, degrees.
    pub limit_deg: f64,
    /// The height at `at` that faces the other node, mm.
    pub from_mm: f64,
    /// The height at `to` that faces `at`, mm.
    pub to_mm: f64,
    /// The distance between them, mm.
    pub run_mm: f64,
    /// The pin pulling hardest at each node (`None`: none reaches).
    pub pins: [Option<usize>; 2],
    /// Neighbouring node pairs over the limit.
    pub pairs: usize,
}

/// A pin that wins nowhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadowed {
    /// The pin, by its place in file order.
    pub pin: usize,
    /// The later pins that cover its place.
    pub covered_by: Vec<usize>,
}

/// A body's top: a low and a high at every node, `lo <= hi` everywhere.
///
/// A band with [`Odds`] is a **cloud**: a low surface, a high surface and a
/// weighting that says how often each height between them comes up. The
/// body supplies a roll from 0 to 1 at each node and the weighting places
/// it ([`Band::rolled`]). A band with none is the plain range: every height
/// between equally, and the body's own field is kept as it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    lo: Field,
    hi: Field,
    odds: Option<Arc<Odds>>,
}

/// The weighting at a node meaning "no odds": the plain range.
const FLAT: u32 = u32::MAX;

/// The odds of a cloud at every node of the lattice: a few weightings and,
/// for each node, which one (or none: the plain range there). A formula
/// whose factor changes sign puts a weighting and its mirror side by side.
#[derive(Clone, Debug, PartialEq)]
pub struct Odds {
    table: Vec<Weighting>,
    id: Vec<u32>,
}

impl Odds {
    /// The weighting at node `k`, `None` for the plain range.
    #[must_use]
    pub fn at(&self, k: usize) -> Option<&Weighting> {
        match self.id.get(k).copied() {
            Some(i) if i != FLAT => self.table.get(i as usize),
            _ => None,
        }
    }

    /// The weightings in use, each once.
    #[must_use]
    pub fn weightings(&self) -> &[Weighting] {
        &self.table
    }

    /// A number that is the same for the same odds: what a cache keys on.
    #[must_use]
    pub fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for w in &self.table {
            format!("{w:?}").hash(&mut h);
        }
        self.id.hash(&mut h);
        h.finish()
    }
}

/// Builds [`Odds`] one node at a time.
#[derive(Debug)]
pub struct OddsBuilder {
    table: Vec<Weighting>,
    id: Vec<u32>,
    last: Option<u32>,
}

impl OddsBuilder {
    /// Odds over `n` nodes, every one the plain range.
    #[must_use]
    pub fn new(n: usize) -> Self {
        Self {
            table: Vec::new(),
            id: vec![FLAT; n],
            last: None,
        }
    }

    /// Node `k` has weighting `w` (`None` or a flat one: the plain range).
    ///
    /// # Panics
    /// If `k` is not a node, or there are over four billion distinct
    /// weightings.
    pub fn set(&mut self, k: usize, w: Option<&Weighting>) {
        let Some(w) = w.filter(|w| !w.is_flat()) else {
            self.id[k] = FLAT;
            return;
        };
        let same = |i: u32| self.table.get(i as usize).is_some_and(|t| t == w);
        let i = match self.last.filter(|&i| same(i)) {
            Some(i) => i,
            None => match self.table.iter().position(|t| t == w) {
                Some(i) => u32::try_from(i).expect("fewer weightings than nodes"),
                None => {
                    self.table.push(w.clone());
                    u32::try_from(self.table.len() - 1).expect("fewer weightings than nodes")
                }
            },
        };
        self.last = Some(i);
        self.id[k] = i;
    }

    /// The odds, `None` when no node has any.
    #[must_use]
    pub fn finish(self) -> Option<Odds> {
        if self.table.is_empty() {
            return None;
        }
        Some(Odds {
            table: self.table,
            id: self.id,
        })
    }
}

/// A field surface inside its band. Only [`Band::fit`] makes one, so a
/// field body cannot hand the kit a surface outside its band.
///
/// **Absent layers.** A layer is absent wherever its surface lies at or
/// under the shown top of the layers below it (within `dt_geom::stack`'s
/// 0.1 micron): the stack mesher then shows nothing of it there, and it has
/// no side panel. A band whose low is under the layer below can therefore
/// leave the layer absent where the body chooses to lie low; a band whose
/// low is above it keeps the layer present everywhere.
#[derive(Clone, Debug, PartialEq)]
pub struct Banded(Field);

impl Banded {
    /// The surface.
    #[must_use]
    pub fn field(&self) -> &Field {
        &self.0
    }
}

fn same_lattice(a: &Field, b: &Field) -> bool {
    a.nx == b.nx && a.ny == b.ny && a.step.to_bits() == b.step.to_bits()
}

fn xy(g: &Field, k: usize) -> [f64; 2] {
    #[allow(clippy::cast_precision_loss)]
    [(k % g.nx) as f64 * g.step, (k / g.nx) as f64 * g.step]
}

impl Band {
    /// A band from its low and high fields.
    ///
    /// # Errors
    /// [`Refusal::Lattice`] when the fields are on different lattices,
    /// [`Refusal::Inverted`] where a low is above its high.
    pub fn new(lo: Field, hi: Field) -> Result<Self, Refusal> {
        if !same_lattice(&lo, &hi) {
            return Err(Refusal::Lattice);
        }
        if let Some(k) = (0..lo.z.len()).find(|&k| lo.z[k] > hi.z[k] || lo.z[k].is_nan()) {
            return Err(Refusal::Inverted {
                at: xy(&lo, k),
                lo: lo.z[k],
                hi: hi.z[k],
            });
        }
        Ok(Self { lo, hi, odds: None })
    }

    /// The band of zero width at `height`: the surface is fixed.
    #[must_use]
    pub fn point(height: &Field) -> Self {
        Self {
            lo: height.clone(),
            hi: height.clone(),
            odds: None,
        }
    }

    /// The same band `lo` to `hi` at every node of a lattice.
    #[must_use]
    pub fn flat(nx: usize, ny: usize, step: f64, lo: f64, hi: f64) -> Self {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        Self {
            lo: Field::filled(nx, ny, step, lo),
            hi: Field::filled(nx, ny, step, hi),
            odds: None,
        }
    }

    /// The band `iv(x, y)` at every node (mm).
    #[must_use]
    pub fn from_fn(nx: usize, ny: usize, step: f64, iv: impl Fn(f64, f64) -> Iv) -> Self {
        let mut lo = Field::filled(nx, ny, step, 0.0);
        let mut hi = lo.clone();
        for k in 0..nx * ny {
            let [x, y] = xy(&lo, k);
            let v = iv(x, y);
            lo.z[k] = v.lo.min(v.hi);
            hi.z[k] = v.lo.max(v.hi);
        }
        Self { lo, hi, odds: None }
    }

    /// The same band as a cloud with `odds` (`None`: the plain range).
    #[must_use]
    pub fn with_odds(mut self, odds: Option<Odds>) -> Self {
        self.odds = odds.map(Arc::new);
        self
    }

    /// The cloud's odds, `None` for the plain range everywhere.
    #[must_use]
    pub fn odds(&self) -> Option<&Odds> {
        self.odds.as_deref()
    }

    /// The weighting at node `k`, `None` for the plain range there.
    #[must_use]
    pub fn weighting(&self, k: usize) -> Option<&Weighting> {
        self.odds.as_deref().and_then(|o| o.at(k))
    }

    /// **rolled**: where the band has odds, the height each node's roll
    /// gives. `roll` is the body's own field of rolls, on any scale: it is
    /// **ranked** over the nodes `take` accepts that have odds, the least
    /// roll first, and the rank (0 to 1) is what the weighting reads, so
    /// the heights come up as often as the weighting says whatever the
    /// field's own distribution. A node with no odds, or `take` refuses, is
    /// `None`: the body keeps its own value there. Equal rolls rank in node
    /// order, so the result never depends on a thread.
    ///
    /// # Panics
    /// If `roll` is not one value per node.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn rolled(&self, roll: &[f64], take: impl Fn(usize) -> bool) -> Vec<Option<f64>> {
        let n = self.lo.z.len();
        assert_eq!(roll.len(), n, "rolled: one roll per node");
        let mut out = vec![None; n];
        let Some(odds) = self.odds.as_deref() else {
            return out;
        };
        let mut nodes: Vec<usize> = (0..n)
            .filter(|&k| odds.at(k).is_some() && take(k) && roll[k].is_finite())
            .collect();
        nodes.sort_by(|&a, &b| roll[a].total_cmp(&roll[b]).then(a.cmp(&b)));
        let m = nodes.len() as f64;
        for (rank, &k) in nodes.iter().enumerate() {
            let Some(w) = odds.at(k) else { continue };
            let t = w.unit((rank as f64 + 0.5) / m);
            out[k] = Some(self.lo.z[k] + (self.hi.z[k] - self.lo.z[k]) * t);
        }
        out
    }

    /// The low field.
    #[must_use]
    pub fn lo(&self) -> &Field {
        &self.lo
    }

    /// The high field.
    #[must_use]
    pub fn hi(&self) -> &Field {
        &self.hi
    }

    /// The band at node `k` (row-major).
    #[must_use]
    pub fn at(&self, k: usize) -> Iv {
        Iv {
            lo: self.lo.z[k],
            hi: self.hi.z[k],
        }
    }

    /// Whether `other` lies on this band's lattice.
    #[must_use]
    pub fn same_lattice(&self, other: &Self) -> bool {
        same_lattice(&self.lo, &other.lo)
    }

    /// **fit**: clamp a raw surface into the band. The only way to make a
    /// [`Banded`].
    ///
    /// # Panics
    /// If `raw` is on another lattice than the band (a bug in the caller).
    #[must_use]
    pub fn fit(&self, raw: &Field) -> Banded {
        assert!(same_lattice(&self.lo, raw), "fit: another lattice");
        let z = (0..raw.z.len())
            .map(|k| {
                let v = raw.z[k];
                // not a number: the band's low, so the surface still exists
                if v.is_nan() {
                    self.lo.z[k]
                } else {
                    v.clamp(self.lo.z[k], self.hi.z[k])
                }
            })
            .collect();
        Banded(raw.with_z(z))
    }

    /// `dt::band`: whether a surface (a pieces body's, not made by
    /// [`Band::fit`]) stays inside the band, within `tol_mm`.
    ///
    /// # Errors
    /// [`Refusal::Band`] at the node farthest outside it.
    pub fn check(&self, surface: &Field, tol_mm: f64) -> Result<(), Refusal> {
        if !same_lattice(&self.lo, surface) {
            return Err(Refusal::Lattice);
        }
        let mut worst: Option<(usize, f64)> = None;
        for k in 0..surface.z.len() {
            let v = surface.z[k];
            let by = (self.lo.z[k] - v).max(v - self.hi.z[k]);
            if (by > tol_mm || v.is_nan()) && worst.is_none_or(|(_, b)| by > b) {
                worst = Some((k, by));
            }
        }
        worst.map_or(Ok(()), |(k, by)| {
            Err(Refusal::Band {
                at: xy(surface, k),
                value: surface.z[k],
                lo: self.lo.z[k],
                hi: self.hi.z[k],
                by,
            })
        })
    }

    /// `dt::thin`: whether this band, as the top of a layer over `bottom`,
    /// can be at least `skin_mm` thick at every node of `region`: the most
    /// the bands allow is `hi - bottom.lo`.
    ///
    /// # Errors
    /// [`Refusal::Thin`] at the thinnest node.
    pub fn thin(&self, bottom: &Self, region: &Mask, skin_mm: f64) -> Result<(), Refusal> {
        if !self.same_lattice(bottom) || region.bits.len() != self.lo.z.len() {
            return Err(Refusal::Lattice);
        }
        let mut worst: Option<(usize, f64)> = None;
        for k in 0..region.bits.len() {
            if !region.bits[k] {
                continue;
            }
            let available = self.hi.z[k] - bottom.lo.z[k];
            if available < skin_mm && worst.is_none_or(|(_, a)| available < a) {
                worst = Some((k, available));
            }
        }
        worst.map_or(Ok(()), |(k, available)| {
            Err(Refusal::Thin {
                at: xy(&self.lo, k),
                available,
                needed: skin_mm,
            })
        })
    }
}

/// A place and a top, nothing else: the band holds at every node of the
/// place. The band's fields are read at the place's nodes only (an
/// expression of x and y there takes the nearest point of the place).
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    /// Where the pin holds.
    pub place: Mask,
    /// How high.
    pub band: Band,
}

impl Pin {
    /// A pin of `band` over `place`.
    ///
    /// # Errors
    /// [`Refusal::Lattice`] when the place is not on the band's lattice.
    pub fn new(place: Mask, band: Band) -> Result<Self, Refusal> {
        if place.nx != band.lo.nx || place.ny != band.lo.ny {
            return Err(Refusal::Lattice);
        }
        Ok(Self { place, band })
    }

    /// A pin of one band, `lo` to `hi` (equal: an exact height), at every
    /// node of `place`, on a lattice of `step` mm.
    #[must_use]
    pub fn flat(place: Mask, step: f64, lo: f64, hi: f64) -> Self {
        let band = Band::flat(place.nx, place.ny, step, lo, hi);
        Self { place, band }
    }
}

/// How a body meets a pin: the body's rule, never written by the tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    /// Dirt: no steeper than this many degrees, over
    /// `1.5 x drop / tan(slope)` from the pin (0 where the bands overlap).
    Slope(f64),
    /// Water: settles over this many mm, whatever the drop (world mm, so
    /// the caller scales the 9 mm of the 35 mm square).
    Settle(f64),
    /// Cliffs and floors: no easing; the stones and slabs step.
    Step,
}

/// The factor between a pin's drop over its reach and the slope it peaks
/// at: the smoothstep's steepest point is 1.5 times its mean.
const PEAK: f64 = 1.5;

impl Ease {
    fn check(self) -> Result<(), Refusal> {
        match self {
            Self::Slope(d) if !(d > 0.0 && d < 90.0) => Err(Refusal::Ease),
            Self::Settle(mm) if !(mm >= 0.0 && mm.is_finite()) => Err(Refusal::Ease),
            _ => Ok(()),
        }
    }

    /// How far a pin of band `pin` eases into the band `own` it is written
    /// over (the body's, or an earlier pin's at the same node), mm.
    #[must_use]
    pub fn reach(self, own: Iv, pin: Iv) -> f64 {
        match self {
            Self::Slope(deg) => {
                let drop = drop_mm(own, pin);
                if drop > 0.0 {
                    PEAK * drop / libm::tan(deg.to_radians())
                } else {
                    0.0
                }
            }
            Self::Settle(mm) => mm,
            Self::Step => 0.0,
        }
    }
}

/// How far the body's own band must move to meet a pin's: 0 where they
/// overlap, else the greatest distance from an end of `own` to `pin`.
#[must_use]
pub fn drop_mm(own: Iv, pin: Iv) -> f64 {
    if own.overlaps(pin) {
        0.0
    } else {
        pin.dist(own.lo).max(pin.dist(own.hi))
    }
}

/// The solved band and who pulled where.
#[derive(Clone, Debug, PartialEq)]
pub struct Solved {
    /// The band the body fills.
    pub band: Band,
    /// At each node, the pin that wins it (the last pin whose place holds
    /// the node), else `None`.
    pub owner: Vec<Option<usize>>,
    /// At each node, the pin pulling hardest (the owner where there is
    /// one), `None` where no pin reaches.
    pub lead: Vec<Option<usize>>,
}

/// A weight this close to 1 is 1: the pin is exact there.
const EXACT: f64 = 1.0 - 1e-12;

/// **solve**: the band a body fills, from its own band and its pins in file
/// order (library defaults first; the later pin wins where places overlap),
/// each pin eased into the body's band by `ease`.
///
/// # Errors
/// [`Refusal::Lattice`] for fields on different lattices,
/// [`Refusal::Ease`] for a bad ease, [`Refusal::EmptyPlace`] for a pin with
/// no node.
pub fn solve(own: &Band, pins: &[Pin], ease: Ease) -> Result<Band, Refusal> {
    solve_traced(own, pins, ease).map(|s| s.band)
}

/// [`solve`], with the owner and the leading pin of every node (what the
/// lints and `check --explain` read).
///
/// # Errors
/// As [`solve`].
pub fn solve_traced(own: &Band, pins: &[Pin], ease: Ease) -> Result<Solved, Refusal> {
    ease.check()?;
    let (nx, ny, step) = (own.lo.nx, own.lo.ny, own.lo.step);
    let n = nx * ny;
    for p in pins {
        if p.place.nx != nx
            || p.place.ny != ny
            || !same_lattice(&p.band.lo, &own.lo)
            || !same_lattice(&p.band.hi, &own.lo)
        {
            return Err(Refusal::Lattice);
        }
    }
    // who wins each node: the last pin whose place holds it
    let mut owner: Vec<Option<usize>> = vec![None; n];
    for (i, p) in pins.iter().enumerate() {
        if !p.place.bits.contains(&true) {
            return Err(Refusal::EmptyPlace { pin: i });
        }
        for (k, &b) in p.place.bits.iter().enumerate() {
            if b {
                owner[k] = Some(i);
            }
        }
    }
    // the odds blend over the nodes no place holds
    let mut num_lo = own.lo.z.clone();
    let mut num_hi = own.hi.z.clone();
    let mut den = vec![1.0; n];
    let mut best = vec![0.0_f64; n];
    let mut lead: Vec<Option<usize>> = owner.clone();
    // what each pin is written over: the body's own band, then the bands of
    // the earlier pins at their places (never their tails)
    let mut under: Vec<Iv> = (0..n).map(|k| own.at(k)).collect();
    // a weight that rounds to 1 away from every place: that pin's band
    let mut exact: Vec<Option<(f64, f64)>> = vec![None; n];
    // and which pin, at which of its nodes, the odds of that band come from
    let mut exact_from: Vec<Option<(usize, usize)>> = vec![None; n];
    for (i, p) in pins.iter().enumerate() {
        // a pin reaches out from where it wins: a later pin over its whole
        // place leaves it nothing to ease from (its tail would pull the
        // surface up beside the pin that covers it)
        let wins: Vec<bool> = owner.iter().map(|&o| o == Some(i)).collect();
        let (dist, near) = if wins.contains(&true) {
            distance_nearest(&wins, nx, ny)
        } else {
            (vec![f64::INFINITY; n], vec![None; n])
        };
        for k in 0..n {
            if owner[k].is_some() {
                continue;
            }
            let Some(q) = near[k] else { continue };
            let reach = ease.reach(under[q], p.band.at(q));
            if reach <= 0.0 {
                continue;
            }
            let w = smooth(1.0 - dist[k] * step / reach);
            if w <= 0.0 {
                continue;
            }
            if w >= EXACT {
                exact[k] = Some((p.band.lo.z[q], p.band.hi.z[q]));
                exact_from[k] = Some((i, q));
                lead[k] = Some(i);
                continue;
            }
            let o = w / (1.0 - w);
            num_lo[k] += o * p.band.lo.z[q];
            num_hi[k] += o * p.band.hi.z[q];
            den[k] += o;
            if w >= best[k] {
                best[k] = w;
                if exact[k].is_none() {
                    lead[k] = Some(i);
                }
            }
        }
        for (k, &b) in p.place.bits.iter().enumerate() {
            if b {
                under[k] = p.band.at(k);
            }
        }
    }
    let mut lo = own.lo.clone();
    let mut hi = own.hi.clone();
    for k in 0..n {
        if let Some(i) = owner[k] {
            lo.z[k] = pins[i].band.lo.z[k];
            hi.z[k] = pins[i].band.hi.z[k];
        } else if let Some((l, h)) = exact[k] {
            lo.z[k] = l;
            hi.z[k] = h;
        } else {
            lo.z[k] = num_lo[k] / den[k];
            hi.z[k] = num_hi[k] / den[k];
        }
    }
    // the odds: a pin's where it holds (a plain range if it has none: the
    // pin replaces the cloud there), the body's own everywhere else
    let odds = if own.odds.is_some() || pins.iter().any(|p| p.band.odds.is_some()) {
        let mut b = OddsBuilder::new(n);
        for k in 0..n {
            let w = if let Some(i) = owner[k] {
                pins[i].band.weighting(k)
            } else if let Some((i, q)) = exact_from[k].filter(|_| exact[k].is_some()) {
                pins[i].band.weighting(q)
            } else {
                own.weighting(k)
            };
            b.set(k, w);
        }
        b.finish()
    } else {
        None
    };
    Ok(Solved {
        band: Band {
            lo,
            hi,
            odds: odds.map(Arc::new),
        },
        owner,
        lead,
    })
}

/// The lints of a solve: pins that win nowhere (`dt::shadowed`) and the
/// worst place the bands force the surface past `slope_deg` (`dt::steep`).
#[must_use]
pub fn lints(solved: &Solved, pins: &[Pin], slope_deg: f64) -> Vec<Lint> {
    let mut out = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        if solved.owner.contains(&Some(i)) {
            continue;
        }
        let mut covered_by: Vec<usize> = pin
            .place
            .bits
            .iter()
            .zip(&solved.owner)
            .filter(|&(&b, _)| b)
            .filter_map(|(_, &o)| o)
            .collect();
        covered_by.sort_unstable();
        covered_by.dedup();
        out.push(Lint::Shadowed(Shadowed { pin: i, covered_by }));
    }
    if let Some(s) = steepest(solved, slope_deg) {
        out.push(Lint::Steep(s));
    }
    out
}

/// The worst forced slope between neighbouring nodes (the 4 and the 2
/// diagonals of each node's forward neighbours), if any passes `limit_deg`.
fn steepest(solved: &Solved, limit_deg: f64) -> Option<Steep> {
    let b = &solved.band;
    let (nx, ny, step) = (b.lo.nx, b.lo.ny, b.lo.step);
    let limit = libm::tan(limit_deg.to_radians());
    let mut worst: Option<(usize, usize, f64)> = None;
    let mut pairs = 0;
    let mut visit = |k: usize, m: usize, run: f64| {
        let rise = b.at(k).gap(b.at(m));
        let s = rise / run;
        if s > limit * (1.0 + 1e-9) + 1e-12 {
            pairs += 1;
            if worst.is_none_or(|(_, _, w)| s > w) {
                worst = Some((k, m, s));
            }
        }
    };
    let diag = step * std::f64::consts::SQRT_2;
    for j in 0..ny {
        for i in 0..nx {
            let k = j * nx + i;
            if i + 1 < nx {
                visit(k, k + 1, step);
            }
            if j + 1 < ny {
                visit(k, k + nx, step);
                if i + 1 < nx {
                    visit(k, k + nx + 1, diag);
                }
                if i > 0 {
                    visit(k, k + nx - 1, diag);
                }
            }
        }
    }
    let (k, m, s) = worst?;
    // name the nodes by which lies lower
    let (a, c) = if b.at(k).hi <= b.at(m).lo {
        (k, m)
    } else {
        (m, k)
    };
    let (from_mm, to_mm) = (b.at(a).hi, b.at(c).lo);
    let (pa, pc) = (xy(&b.lo, a), xy(&b.lo, c));
    Some(Steep {
        at: pa,
        to: pc,
        deg: libm::atan(s).to_degrees(),
        limit_deg,
        from_mm,
        to_mm,
        run_mm: libm::hypot(pa[0] - pc[0], pa[1] - pc[1]),
        pins: [solved.lead[a], solved.lead[c]],
        pairs,
    })
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;

    const W: f64 = 35.0;
    const STEP: f64 = 0.5;

    /// A `cols` by 1 tile of 35 mm squares on 0.5 mm cells.
    fn lattice(cols: usize) -> (usize, usize) {
        (cols * 70 + 1, 71)
    }

    fn mask(cols: usize, f: impl Fn(f64, f64) -> bool) -> Mask {
        let (nx, ny) = lattice(cols);
        let mut m = Mask::new(nx, ny, false);
        for j in 0..ny {
            for i in 0..nx {
                m.bits[j * nx + i] = f(i as f64 * STEP, j as f64 * STEP);
            }
        }
        m
    }

    fn flat(cols: usize, lo: f64, hi: f64) -> Band {
        let (nx, ny) = lattice(cols);
        Band::flat(nx, ny, STEP, lo, hi)
    }

    fn pin(_cols: usize, place: Mask, lo: f64, hi: f64) -> Pin {
        Pin::flat(place, STEP, lo, hi)
    }

    fn node(cols: usize, x: f64, y: f64) -> usize {
        let (nx, _) = lattice(cols);
        (y / STEP).round() as usize * nx + (x / STEP).round() as usize
    }

    fn at(b: &Band, cols: usize, x: f64, y: f64) -> Iv {
        b.at(node(cols, x, y))
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near_iv(a: Iv, lo: f64, hi: f64) -> bool {
        near(a.lo, lo) && near(a.hi, hi)
    }

    fn tan24() -> f64 {
        libm::tan(24f64.to_radians())
    }

    fn rim(cols: usize) -> Mask {
        let w = cols as f64 * W;
        mask(cols, move |x, y| x == 0.0 || x == w || y == 0.0 || y == W)
    }

    fn smooth_mix(own: f64, pin: f64, d: f64, reach: f64) -> f64 {
        let w = smooth(1.0 - d / reach);
        own * (1.0 - w) + pin * w
    }

    /// Every band a solve returns has its low at or under its high.
    fn well_formed(b: &Band) {
        assert!(Band::new(b.lo().clone(), b.hi().clone()).is_ok());
    }

    /// The steepest slope along a row, for one edge of the band.
    fn max_row_slope(edge: &Field, cols: usize, y: f64) -> f64 {
        let (nx, _) = lattice(cols);
        let j = (y / STEP).round() as usize;
        (0..nx - 1)
            .map(|i| (edge.z[j * nx + i + 1] - edge.z[j * nx + i]).abs() / STEP)
            .fold(0.0, f64::max)
    }

    #[test]
    fn no_pins_leaves_the_own_band() {
        let own = flat(1, 5.0, 7.5);
        assert_eq!(solve(&own, &[], Ease::Slope(24.0)).unwrap(), own);
    }

    /// bodies.html, "Dirt": the rim default pins the foundation dirt to
    /// dirtEdgeHeight 3.4; the band [5, 7.5] eases back over
    /// 1.5 x 4.1 / tan 24 = 14 mm, and over 5.4 mm for a flat top at 5.
    #[test]
    fn the_rim_eases_over_14_mm_or_5_4_flat() {
        let rim_pin = Iv::point(3.4);
        let r = Ease::Slope(24.0).reach(Iv::new(5.0, 7.5), rim_pin);
        assert!((r - 13.81).abs() < 0.01, "{r}");
        let flat_r = Ease::Slope(24.0).reach(Iv::point(5.0), rim_pin);
        assert!((flat_r - 5.39).abs() < 0.01, "{flat_r}");

        let own = flat(1, 5.0, 7.5);
        let pins = [pin(1, rim(1), 3.4, 3.4)];
        let b = solve(&own, &pins, Ease::Slope(24.0)).unwrap();
        well_formed(&b);
        // exact at the rim, the own band beyond the reach, the smoothstep between
        assert!(near_iv(at(&b, 1, 0.0, 17.5), 3.4, 3.4));
        assert!(near_iv(at(&b, 1, 17.5, 17.5), 5.0, 7.5));
        assert!(near_iv(at(&b, 1, 14.5, 17.5), 5.0, 7.5));
        let v = at(&b, 1, 6.0, 17.5);
        assert!(near(v.lo, smooth_mix(5.0, 3.4, 6.0, r)));
        assert!(near(v.hi, smooth_mix(7.5, 3.4, 6.0, r)));
        // dirt lies at its slope: the high edge peaks at tan 24 (1.5 x the mean)
        let peak = max_row_slope(b.hi(), 1, 17.5);
        assert!(peak <= tan24() * 1.0001 && peak > tan24() * 0.95, "{peak}");
        // the flat top: the same rule gives today's 5.4 mm rim ramp
        let b = solve(&flat(1, 5.0, 5.0), &pins, Ease::Slope(24.0)).unwrap();
        assert!(near_iv(at(&b, 1, 5.5, 17.5), 5.0, 5.0), "past 5.4 mm: flat");
        assert!(at(&b, 1, 5.0, 17.5).lo < 5.0, "inside it: eased");
    }

    /// bodies.html, "Tops are bands": [5, 7.5] with the rim at 3.4, an exact
    /// pin at 22 mm (9) squeezes it to zero thickness at one point; a range
    /// pin at 50 mm (6 to 9) holds it to its own 3 mm.
    #[test]
    fn the_band_section_of_the_proposal() {
        let own = flat(2, 5.0, 7.5);
        let line = |x0: f64| mask(2, move |x, _| x == x0);
        let pins = [
            pin(2, rim(2), 3.4, 3.4),
            pin(2, line(22.0), 9.0, 9.0),
            pin(2, line(50.0), 6.0, 9.0),
        ];
        let b = solve(&own, &pins, Ease::Slope(24.0)).unwrap();
        well_formed(&b);
        assert!(near_iv(at(&b, 2, 22.0, 17.5), 9.0, 9.0), "zero thickness");
        assert!(near_iv(at(&b, 2, 50.0, 17.5), 6.0, 9.0), "its own 3 mm");
        assert!(near_iv(at(&b, 2, 36.0, 17.5), 5.0, 7.5), "own: 2.5 mm");
        // the exact pin at 22 reaches 1.5 x 4 / tan 24 = 13.48 mm
        let reach = 1.5 * 4.0 / tan24();
        let v = at(&b, 2, 15.5, 17.5);
        assert!(near(v.hi - v.lo, 2.5 * (1.0 - smooth(1.0 - 6.5 / reach))));
        // a pin whose band overlaps the body's needs no easing (the spec's
        // drawing eases the 50 mm pin; the written rule does not)
        assert!(near_iv(at(&b, 2, 50.5, 17.5), 5.0, 7.5));
    }

    /// bodies.html, "Dirt and a path": the path's band, 3.8 to 4.2 mm, sits
    /// under the land's 5 to 6. The page says it eases over
    /// 1.5 x 0.8 / tan 24 = 2.7 mm (0.8 is the gap between the bands), but
    /// the land's high edge at 6 must come down 1.8 to the path's top, and
    /// over 2.7 mm that is 45 degrees. The solver eases over the worst drop
    /// (1.8): 6.06 mm, and no edge is steeper than 24.
    #[test]
    fn the_path_eases_over_its_worst_drop_not_its_gap() {
        let (own_iv, path_iv) = (Iv::new(5.0, 6.0), Iv::new(3.8, 4.2));
        assert!((own_iv.gap(path_iv) - 0.8).abs() < 1e-12);
        assert!((drop_mm(own_iv, path_iv) - 1.8).abs() < 1e-12);
        let reach = Ease::Slope(24.0).reach(own_iv, path_iv);
        assert!((reach - 6.06).abs() < 0.01, "{reach}");
        let own = flat(1, 5.0, 6.0);
        let place = mask(1, |x, _| (14.75..=23.75).contains(&x));
        let b = solve(&own, &[pin(1, place, 3.8, 4.2)], Ease::Slope(24.0)).unwrap();
        assert!(near_iv(at(&b, 1, 19.0, 17.5), 3.8, 4.2));
        assert!(near_iv(at(&b, 1, 14.0 - 6.2, 17.5), 5.0, 6.0));
        for e in [b.lo(), b.hi()] {
            assert!(max_row_slope(e, 1, 17.5) <= tan24() * 1.0001);
        }
        assert!(max_row_slope(b.hi(), 1, 17.5) > tan24() * 0.95);
        // 2.7 mm would be steeper than the dirt's slope
        let steep = libm::atan(1.5 * 1.8 / 2.7).to_degrees();
        assert!(steep > 24.0);
    }

    /// bodies.html, "Banks are drawn": a stream whose banks run from the
    /// water's bed to the land. The banks' band overlaps the land's at its
    /// edge, so nothing eases; the bed, written later, wins inside the bed.
    #[test]
    fn a_stream_bank_overlaps_the_land_so_nothing_eases() {
        let own = flat(2, 5.0, 6.0);
        let in_banks = |y: f64| (y - 17.5).abs() <= 4.5;
        let in_bed = |y: f64| (y - 17.5).abs() <= 2.0;
        let pins = [
            pin(2, mask(2, move |_, y| in_banks(y)), 0.4, 6.0),
            pin(2, mask(2, move |_, y| in_bed(y)), 0.4, 0.7),
        ];
        let s = solve_traced(&own, &pins, Ease::Slope(24.0)).unwrap();
        let b = &s.band;
        assert!(
            near_iv(at(b, 2, 20.0, 17.5), 0.4, 0.7),
            "bed wins inside the bed"
        );
        assert!(near_iv(at(b, 2, 20.0, 21.0), 0.4, 6.0), "banks");
        assert!(
            near_iv(at(b, 2, 20.0, 22.5), 5.0, 6.0),
            "the land, with no easing"
        );
        assert!(near_iv(at(b, 2, 20.0, 12.5), 5.0, 6.0));
        assert!(
            lints(&s, &pins, 24.0).is_empty(),
            "no forced slope, no shadow"
        );
        // the page's 4.6 mm rise over 10 mm: about 25 degrees
        let rise = own.lo().z[0] - 0.4;
        assert!(near(rise, 4.6));
        assert!((libm::atan(rise / 10.0).to_degrees() - 24.7).abs() < 0.1);
        // the bed's pin owns the bed, the banks' the rest of the banks
        assert_eq!(s.owner[node(2, 20.0, 17.5)], Some(1));
        assert_eq!(s.owner[node(2, 20.0, 21.0)], Some(0));
        assert_eq!(s.owner[node(2, 20.0, 25.0)], None);
    }

    /// bodies.html, "Dirt and grass: a named hill": a meadow, a knoll pinned
    /// wide from the meadow up to the summit, and a ring inside it.
    #[test]
    fn the_hill_is_drawn_nothing_eases_and_nothing_is_steep() {
        let own = flat(1, 5.0, 6.0);
        let c = |x: f64, y: f64| libm::hypot(x - 17.5, y - 17.5);
        let pins = [
            pin(1, mask(1, move |x, y| c(x, y) <= 15.75), 5.0, 12.5),
            pin(
                1,
                mask(1, move |x, y| (4.0..=5.0).contains(&c(x, y))),
                9.0,
                10.0,
            ),
        ];
        let s = solve_traced(&own, &pins, Ease::Slope(40.0)).unwrap();
        well_formed(&s.band);
        assert!(
            near_iv(at(&s.band, 1, 17.5 + 4.5, 17.5), 9.0, 10.0),
            "the ring"
        );
        assert!(
            near_iv(at(&s.band, 1, 17.5 + 10.0, 17.5), 5.0, 12.5),
            "free inside the knoll"
        );
        assert!(near_iv(at(&s.band, 1, 17.5, 17.5), 5.0, 12.5), "the summit");
        assert!(
            near_iv(at(&s.band, 1, 17.5 + 16.0, 17.5), 5.0, 6.0),
            "the meadow"
        );
        assert!(lints(&s, &pins, 40.0).is_empty());
        // the page's numbers: the ring is at most 5 up and 10.75 mm inside the foot
        let run = 15.75 - 5.0;
        assert!((run - 10.75_f64).abs() < 1e-12);
        let avg = libm::atan(5.0 / run).to_degrees();
        assert!((avg - 25.0).abs() < 0.5, "{avg}");
        // at its steepest, 1.5 times the mean slope: 34.9 (the page says about 37)
        let peak = libm::atan(1.5 * 5.0 / run).to_degrees();
        assert!((peak - 34.9).abs() < 0.1, "{peak}");
        // ring and summit: 2.5 mm over 6.75 mm, about 20 degrees
        assert!((libm::atan(2.5 / 6.75).to_degrees() - 20.0).abs() < 0.5);
    }

    /// The same hill with the summit raised past the knoll's band: it no
    /// longer fits, and the solve notes where the dirt passes 40 degrees.
    #[test]
    fn a_summit_past_the_knoll_is_steep() {
        let own = flat(1, 5.0, 6.0);
        let c = |x: f64, y: f64| libm::hypot(x - 17.5, y - 17.5);
        let pins = [
            pin(1, mask(1, move |x, y| c(x, y) <= 15.75), 5.0, 12.5),
            pin(
                1,
                mask(1, move |x, y| (4.0..=5.0).contains(&c(x, y))),
                9.0,
                10.0,
            ),
            pin(1, mask(1, move |x, y| c(x, y) <= 2.0), 13.0, 14.0),
        ];
        let s = solve_traced(&own, &pins, Ease::Slope(40.0)).unwrap();
        let ls = lints(&s, &pins, 40.0);
        let [Lint::Steep(st)] = &ls[..] else {
            panic!("{ls:?}")
        };
        assert_eq!(ls[0].code(), dt_lang::codes::STEEP);
        assert!((st.deg - 45.0).abs() < 0.01, "{}", st.deg);
        assert!(near(st.from_mm, 12.5) && near(st.to_mm, 13.0) && near(st.run_mm, 0.5));
        assert_eq!(st.pins, [Some(0), Some(2)], "the knoll and the summit");
        assert!(st.pairs > 4);
    }

    /// bodies.html, "Water": settles over 9 mm round the rim's tide.
    #[test]
    fn water_settles_over_nine_mm() {
        let own = flat(1, 1.85, 4.8);
        let pins = [pin(1, rim(1), 2.6, 2.6)];
        let b = solve(&own, &pins, Ease::Settle(9.0)).unwrap();
        well_formed(&b);
        assert!(near_iv(at(&b, 1, 0.0, 17.5), 2.6, 2.6));
        let mid = at(&b, 1, 4.5, 17.5);
        assert!(near(mid.lo, 0.5 * (1.85 + 2.6)) && near(mid.hi, 0.5 * (4.8 + 2.6)));
        assert!(near_iv(at(&b, 1, 9.5, 17.5), 1.85, 4.8));
        // the drop does not matter: a pin inside the band settles too
        let inside = [pin(1, rim(1), 3.0, 3.2)];
        let b = solve(&own, &inside, Ease::Settle(9.0)).unwrap();
        assert!(at(&b, 1, 4.5, 17.5).lo > 1.85);
    }

    /// Cliffs and floors step: the pin holds its place, nothing eases.
    #[test]
    fn cliffs_and_floors_step() {
        let own = flat(2, 33.0, 45.0);
        let strip = mask(2, |x, _| (20.0..=30.0).contains(&x));
        let b = solve(&own, &[pin(2, strip, 28.0, 28.0)], Ease::Step).unwrap();
        assert!(near_iv(at(&b, 2, 25.0, 10.0), 28.0, 28.0));
        assert!(near_iv(at(&b, 2, 30.5, 10.0), 33.0, 45.0));
        assert!(near_iv(at(&b, 2, 19.5, 10.0), 33.0, 45.0));
        // a floor meeting the edge at its own height is its own band
        let floor = flat(1, 7.4, 7.4);
        let b = solve(&floor, &[pin(1, rim(1), 7.4, 7.4)], Ease::Step).unwrap();
        assert_eq!(b, floor);
    }

    /// Blending is by odds: with two pins pulling, `(own + sum o x pin) /
    /// (1 + sum o)`, and each pin is exact at its own place.
    #[test]
    fn pins_blend_by_odds_and_stay_exact() {
        let own = flat(2, 0.0, 0.0);
        let pins = [
            pin(2, mask(2, |x, _| x == 0.0), 10.0, 10.0),
            pin(2, mask(2, |x, _| x == 20.0), 20.0, 24.0),
        ];
        let b = solve(&own, &pins, Ease::Settle(30.0)).unwrap();
        well_formed(&b);
        assert!(near_iv(at(&b, 2, 0.0, 5.0), 10.0, 10.0));
        assert!(near_iv(at(&b, 2, 20.0, 5.0), 20.0, 24.0));
        let w = smooth(1.0 - 10.0 / 30.0);
        let o = w / (1.0 - w);
        let v = at(&b, 2, 10.0, 5.0);
        assert!(near(v.lo, (o * 10.0 + o * 20.0) / (1.0 + 2.0 * o)), "{v:?}");
        assert!(near(v.hi, (o * 10.0 + o * 24.0) / (1.0 + 2.0 * o)));
        // far from both: the body's own
        assert!(near_iv(at(&b, 2, 60.0, 5.0), 0.0, 0.0));
    }

    /// bodies.html, "Pins": the later pin wins where places overlap. The
    /// ford, written after the stream's bed, wins on the ford.
    #[test]
    fn a_pin_covered_all_over_leaves_no_tail() {
        // a rim pin at 3.4 is written first and a later pin holds the same
        // strip at 0.4: nothing is left of the first to ease from, so the
        // surface beside the strip never heads for 3.4
        let strip = mask(1, |_, y| y < 1e-9);
        let own = flat(1, 0.4, 0.4);
        let first = pin(1, strip.clone(), 3.4, 3.4);
        let second = pin(1, strip, 0.4, 0.4);
        let b = solve(&own, &[first, second], Ease::Slope(24.0)).unwrap();
        for y in [0.5, 1.0, 2.0] {
            assert!(
                near_iv(at(&b, 1, 10.0, y), 0.4, 0.4),
                "{:?}",
                at(&b, 1, 10.0, y)
            );
        }
    }

    #[test]
    fn the_later_pin_wins_a_ford() {
        let own = flat(2, 5.0, 6.0);
        let bed = |_: usize| mask(2, |_, y| (y - 17.5).abs() <= 2.0);
        let ford = mask(2, |x, _| (30.0..=36.0).contains(&x));
        let pins = [pin(2, bed(0), 0.4, 0.7), pin(2, ford.clone(), 1.6, 1.6)];
        let s = solve_traced(&own, &pins, Ease::Slope(24.0)).unwrap();
        assert!(
            near_iv(at(&s.band, 2, 33.0, 17.5), 1.6, 1.6),
            "the ford wins in the bed"
        );
        assert!(near_iv(at(&s.band, 2, 20.0, 17.5), 0.4, 0.7));
        assert!(
            lints(&s, &pins, 24.0)
                .iter()
                .all(|l| l.code() != dt_lang::codes::SHADOWED)
        );
        // the other way round the bed wins the ford's crossing
        let pins = [pin(2, ford, 1.6, 1.6), pin(2, bed(0), 0.4, 0.7)];
        let b = solve(&own, &pins, Ease::Slope(24.0)).unwrap();
        assert!(near_iv(at(&b, 2, 33.0, 17.5), 0.4, 0.7));
        assert!(near_iv(at(&b, 2, 33.0, 5.0), 1.6, 1.6));
    }

    #[test]
    fn a_pin_that_wins_nowhere_is_shadowed() {
        let own = flat(2, 5.0, 6.0);
        let pins = [
            pin(
                2,
                mask(2, |x, y| libm::hypot(x - 33.0, y - 17.5) <= 1.5),
                1.6,
                1.6,
            ),
            pin(2, mask(2, |_, y| (y - 17.5).abs() <= 2.0), 0.4, 0.7),
        ];
        let s = solve_traced(&own, &pins, Ease::Slope(24.0)).unwrap();
        let ls = lints(&s, &pins, 24.0);
        assert_eq!(
            ls,
            vec![Lint::Shadowed(Shadowed {
                pin: 0,
                covered_by: vec![1]
            })]
        );
        assert_eq!(ls[0].code(), dt_lang::codes::SHADOWED);
        // the later pin's band shows where the shadowed one would have
        assert!(near_iv(at(&s.band, 2, 33.0, 17.5), 0.4, 0.7));
        assert_eq!(s.lead[node(2, 33.0, 17.5)], Some(1));
    }

    #[test]
    fn refusals_carry_numbers() {
        let own = flat(1, 5.0, 6.0);
        // dt::empty
        let empty = Pin::flat(Mask::new(own.lo().nx, own.lo().ny, false), STEP, 1.0, 1.0);
        let r = solve(&own, &[pin(1, rim(1), 3.4, 3.4), empty], Ease::Slope(24.0));
        assert_eq!(r, Err(Refusal::EmptyPlace { pin: 1 }));
        assert_eq!(r.unwrap_err().code(), dt_lang::codes::EMPTY);
        // another lattice, a bad slope
        let wide = flat(2, 5.0, 6.0);
        assert_eq!(
            solve(&own, &[pin(2, rim(2), 3.4, 3.4)], Ease::Step),
            Err(Refusal::Lattice)
        );
        assert_eq!(solve(&wide, &[], Ease::Slope(0.0)), Err(Refusal::Ease));
        assert_eq!(solve(&wide, &[], Ease::Settle(-1.0)), Err(Refusal::Ease));
        // a band upside down
        let hi = Field::filled(71, 71, STEP, 1.0);
        let mut lo = hi.clone();
        lo.z[node(1, 3.0, 2.0)] = 1.5;
        assert_eq!(
            Band::new(lo, hi),
            Err(Refusal::Inverted {
                at: [3.0, 2.0],
                lo: 1.5,
                hi: 1.0
            })
        );
        // dt::band: the worst node outside, by how much
        let mut surface = Field::filled(71, 71, STEP, 5.5);
        surface.z[node(1, 4.0, 8.0)] = 6.3;
        surface.z[node(1, 9.0, 1.0)] = 4.9;
        let e = own.check(&surface, 1e-9).unwrap_err();
        let Refusal::Band {
            at,
            value,
            lo,
            hi,
            by,
        } = e.clone()
        else {
            panic!()
        };
        assert_eq!((at, value, lo, hi), ([4.0, 8.0], 6.3, 5.0, 6.0));
        assert!(near(by, 0.3));
        assert_eq!(e.code(), dt_lang::codes::BAND);
        assert!(e.to_string().contains("0.30 mm outside"), "{e}");
        assert!(own.check(&own.fit(&surface).field().clone(), 0.0).is_ok());
        // dt::thin: a layer on [1, 1.3] over a bottom [1, 1] with a 0.6 mm skin
        let top = flat(1, 1.0, 1.3);
        let bottom = flat(1, 1.0, 1.0);
        let region = mask(1, |x, y| x > 10.0 && x < 12.0 && y == 5.0);
        let e = top.thin(&bottom, &region, 0.6).unwrap_err();
        assert_eq!(e.code(), dt_lang::codes::THIN);
        let Refusal::Thin {
            at,
            available,
            needed,
        } = e
        else {
            panic!()
        };
        assert_eq!((at, needed), ([10.5, 5.0], 0.6));
        assert!(near(available, 0.3));
        assert!(top.thin(&bottom, &region, 0.3).is_ok());
        assert!(top.thin(&bottom, &Mask::new(71, 71, false), 5.0).is_ok());
    }

    #[test]
    fn fit_keeps_the_surface_inside_the_band() {
        let band = Band::from_fn(71, 71, STEP, |x, _| Iv::new(5.0, 5.0 + x / 10.0));
        let raw = Field::from_fn_for_test(71, 71, STEP, |x, y| 5.0 + 4.0 * libm::sin(x + 2.0 * y));
        let mut raw = raw;
        raw.z[3] = f64::NAN;
        raw.z[4] = f64::INFINITY;
        raw.z[5] = f64::NEG_INFINITY;
        let fit = band.fit(&raw);
        for k in 0..fit.field().z.len() {
            let v = fit.field().z[k];
            assert!(band.at(k).contains(v), "{k} {v}");
        }
        assert_eq!(fit.field().z[3], 5.0);
        assert!(band.check(fit.field(), 0.0).is_ok());
        // where the band is a point the surface is fixed
        assert_eq!(fit.field().z[0], 5.0);
    }

    trait FromFn {
        fn from_fn_for_test(nx: usize, ny: usize, step: f64, f: impl Fn(f64, f64) -> f64) -> Field;
    }
    impl FromFn for Field {
        fn from_fn_for_test(nx: usize, ny: usize, step: f64, f: impl Fn(f64, f64) -> f64) -> Field {
            let mut g = Field::filled(nx, ny, step, 0.0);
            for k in 0..nx * ny {
                let [x, y] = xy(&g, k);
                g.z[k] = f(x, y);
            }
            g
        }
    }

    /// A cloud over a 100 by 100 lattice: `lo` to `hi` everywhere with `w`.
    fn cloud(lo: f64, hi: f64, w: &Weighting) -> Band {
        let mut b = OddsBuilder::new(100 * 100);
        for k in 0..100 * 100 {
            b.set(k, Some(w));
        }
        Band::flat(100, 100, 1.0, lo, hi).with_odds(b.finish())
    }

    /// The roll of a smooth-looking field that is far from uniform: its
    /// square, so the ranks are what make the heights come up as they say.
    fn rolls() -> Vec<f64> {
        (0..100 * 100)
            .map(|k| {
                let (i, j) = ((k % 100) as f64, (k / 100) as f64);
                let v = (0.07 * i).sin() * (0.05 * j).cos() + 0.001 * i;
                v * v * v
            })
            .collect()
    }

    #[test]
    fn a_cloud_is_rolled_by_rank_so_the_heights_come_up_as_the_weighting_says() {
        let w = Weighting::Low(4.0);
        let band = cloud(0.0, 10.0, &w);
        let rolled = band.rolled(&rolls(), |_| true);
        let heights: Vec<f64> = rolled.iter().map(|v| v.unwrap()).collect();
        assert!(heights.iter().all(|h| (0.0..=10.0).contains(h)));
        // the share below the middle is the weighting's own mass there:
        // (1 - 4^-0.5) / (1 - 4^-1) = 2/3
        let low = heights.iter().filter(|&&h| h < 5.0).count() as f64 / heights.len() as f64;
        assert!((low - 2.0 / 3.0).abs() < 0.01, "{low}");
        // the roll's order is kept: a higher roll is never a lower height
        let r = rolls();
        let mut by_roll: Vec<usize> = (0..r.len()).collect();
        by_roll.sort_by(|&a, &b| r[a].total_cmp(&r[b]).then(a.cmp(&b)));
        assert!(by_roll.windows(2).all(|p| heights[p[0]] <= heights[p[1]]));
        // a mirrored weighting is the mirror image: mostly high
        let high = cloud(0.0, 10.0, &w.mirrored()).rolled(&rolls(), |_| true);
        let above = high.iter().filter(|v| v.unwrap() > 5.0).count() as f64 / 10_000.0;
        assert!((above - 2.0 / 3.0).abs() < 0.01, "{above}");
    }

    #[test]
    fn the_plain_range_and_the_nodes_not_taken_are_not_rolled() {
        let plain = Band::flat(100, 100, 1.0, 0.0, 10.0);
        assert!(plain.rolled(&rolls(), |_| true).iter().all(Option::is_none));
        let flat = cloud(0.0, 10.0, &Weighting::Flat);
        assert!(flat.odds().is_none(), "a flat weighting is no odds at all");
        let band = cloud(0.0, 10.0, &Weighting::High(4.0));
        let rolled = band.rolled(&rolls(), |k| k % 2 == 0);
        assert!(
            rolled
                .iter()
                .enumerate()
                .all(|(k, v)| v.is_some() == (k % 2 == 0))
        );
    }

    #[test]
    fn a_pin_replaces_the_cloud_and_the_body_eases_back_into_it() {
        let w = Weighting::Low(4.0);
        let own = cloud(5.0, 9.0, &w);
        // a pin of one height over the middle column, and a cloud of its own
        // over the last
        let mut place = Mask::new(100, 100, false);
        let mut cloudy = Mask::new(100, 100, false);
        for j in 0..100 {
            place.bits[j * 100 + 50] = true;
            cloudy.bits[j * 100 + 99] = true;
        }
        let one = Pin::new(place, Band::flat(100, 100, 1.0, 12.0, 12.0)).unwrap();
        let mut b = OddsBuilder::new(100 * 100);
        for j in 0..100 {
            b.set(j * 100 + 99, Some(&Weighting::High(4.0)));
        }
        let two = Pin::new(
            cloudy,
            Band::flat(100, 100, 1.0, 12.0, 14.0).with_odds(b.finish()),
        )
        .unwrap();
        let solved = solve(&own, &[one, two], Ease::Slope(24.0)).unwrap();
        let at = |i: usize| i + 100 * 20;
        assert_eq!(
            solved.weighting(at(50)),
            None,
            "a pin of one height has none"
        );
        assert_eq!(solved.weighting(at(99)), Some(&Weighting::High(4.0)));
        assert_eq!(
            solved.weighting(at(5)),
            Some(&w),
            "away from the pins: the body's"
        );
    }
}
