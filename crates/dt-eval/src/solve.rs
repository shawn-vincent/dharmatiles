//! The Solve phase of `check`: a tile's bodies, tops and pins turned into
//! bands on a lattice, with the refusals and lints of the band solver.
//! Fields only, no meshes: tens of milliseconds.
//!
//! Each body of the stack (dirt, water, cliff, flagstones, planks) takes its top, or its
//! element's default top; its pins are the library defaults' first (the
//! land defaults for the first land layer only, the water default for every
//! water), then the tile's own, so the tile's win where they overlap.  The
//! body's `bottom` is the surface under it, the higher of what the bodies
//! before it made inside their regions (zero at first).  A height is
//! worked out as an interval, so a range stays a band; `x` and `y` in a
//! pin are the node itself, which for a pin's place is its nearest point.

use dt_doc::consts::ConstTable;
use dt_doc::expr::{Algebra, BinOp, Expr, Func, Odds, UnitWord, sin_deg};
use dt_doc::model::PlaceRef;
use dt_doc::schema::Body;
use dt_kit::{
    Band, Ease, Field, Frame, Iv, Lint, Mask, OddsBuilder, Pin, Refusal, lints, solve_traced,
};
use dt_lang::smooth;
use dt_value::{Dist, Weighting};

use crate::Options;
use crate::coords::Coords;
use crate::diag::Diag;
use crate::doc::{DefaultBody, PinDecl, Tile};
use crate::places::Places;
use crate::regions::RegionTree;

/// The deepest chain of constants naming each other.
const DEPTH: usize = 64;

/// How one pin fared.
#[derive(Clone, Debug, PartialEq)]
pub struct PinReport {
    /// The pin as written.
    pub text: String,
    /// Its name, if it has one.
    pub name: Option<String>,
    /// True for a library default.
    pub default: bool,
    /// Where it is written, `file:line`.
    pub at: String,
    /// Nodes in its place.
    pub place_nodes: usize,
    /// Nodes where it wins.
    pub wins: usize,
}

/// What a body is to the bodies round it, from what its element writes
/// ([`dt_elements::Plan::writes`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Land: it lies on the surface so far and raises it where it is higher.
    Land,
    /// A fluid: it fills what lies below its top inside its region.
    Fluid,
}

/// The `top=` a body was solved from, and where it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolvedTop {
    /// The formula as written.
    pub text: String,
    /// The library file it is a default of (`lib:parts/edges.dtile`); `None`
    /// when the tile writes it.
    pub default_from: Option<String>,
}

/// One solved body.
#[derive(Clone, Debug)]
pub struct SolvedBody {
    /// Its place in the tile's stack.
    pub entry: usize,
    /// Land or a fluid.
    pub role: Role,
    /// The element.
    pub element: String,
    /// Its name, if written.
    pub label: Option<String>,
    /// The band the body fills, world mm on the lattice.
    pub band: Band,
    /// Nodes in its region.
    pub region_nodes: usize,
    /// The surface laid before it (the higher of every land body so far),
    /// world mm on the lattice: what `bottom` meant when it was solved.
    pub below: Band,
    /// The lowest the surface before it is anywhere, at least 1 mm: what a
    /// pieces body's stones are counted from.
    pub land_mm: f64,
    /// Its pins, defaults first.
    pub pins: Vec<PinReport>,
    /// The tile writes its band: a `top=` or a pin of its own (the debug
    /// layer draws only these).
    pub written: bool,
    /// The `top=` this body's band was solved from, as written.
    pub top: SolvedTop,
    /// How many times laying this body into the surface under later bodies
    /// was left out (its band came out degenerate): 0 or 1.
    pub left_out: usize,
    nx: usize,
    ny: usize,
    per: f64,
}

impl SolvedBody {
    /// The solved band at a point in squares from the south-west corner.
    #[must_use]
    pub fn at(&self, x: f64, y: f64) -> Iv {
        let node = |v: f64, n: usize| dt_kit::node_of(self.per, v, n);
        self.band.at(node(y, self.ny) * self.nx + node(x, self.nx))
    }

    /// The odds along the band at a point in squares from the south-west
    /// corner: `None` where it is the plain range (a pin of one height or a
    /// plain range holds there, or the top is no cloud).
    #[must_use]
    pub fn odds_at(&self, x: f64, y: f64) -> Option<&Weighting> {
        let node = |v: f64, n: usize| dt_kit::node_of(self.per, v, n);
        self.band
            .weighting(node(y, self.ny) * self.nx + node(x, self.nx))
    }
}

/// What the Solve phase found.
#[derive(Clone, Debug, Default)]
pub struct SolveReport {
    /// The lattice: cells per square.
    pub cells_per_square: u32,
    /// The bodies, in stack order.
    pub bodies: Vec<SolvedBody>,
    /// For each stack entry (the connector last), the body it is.
    pub by_entry: Vec<Option<usize>>,
    /// Where the first land body meets a tile edge, mm: its solved band
    /// where the library's edge pin holds the rim; `None` when that pin
    /// holds no rim node (no land reaches an edge, or the tile's own pins
    /// cover it all).
    pub edge_mm: Option<f64>,
    /// The calm water level, mm, when the tile has a fluid.
    pub calm_mm: Option<f64>,
    /// Lints and notes: the tile solves, but look here.
    pub notes: Vec<Diag>,
    /// What `check --explain` says.
    pub explain: Vec<String>,
}

fn node_xy(k: usize, nx: usize, per: f64) -> (f64, f64) {
    #[allow(clippy::cast_precision_loss)]
    ((k % nx) as f64 / per, (k / nx) as f64 / per)
}

/// What an expression depends on: the point, the surface under.
#[derive(Clone, Copy, Default)]
struct Needs {
    xy: bool,
    bottom: bool,
}

pub(crate) struct Vars {
    pub(crate) x: Iv,
    pub(crate) y: Iv,
    pub(crate) bottom: Iv,
    /// The node, for the coordinates of format 1.
    pub(crate) k: usize,
}

/// A tree's own region: a disc, in squares from the south-west corner.  Its
/// coordinates are worked out where they are asked, not on the lattice.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Disc {
    pub(crate) centre: [f64; 2],
    pub(crate) radius: f64,
}

pub(crate) struct Eval<'a> {
    pub(crate) table: &'a ConstTable,
    /// Format 1: the regions' coordinates, and the region `$region` is.
    pub(crate) coords: Option<(&'a Coords<'a>, usize)>,
    /// Where `$region` is a tree's disc, the disc (it has no mask).
    pub(crate) disc: Option<Disc>,
}

impl Eval<'_> {
    /// A coordinate of the tree's own disc at the point `v`.
    fn on_disc(&self, c: &Coords<'_>, d: Disc, name: &str, unit: UnitWord, v: &Vars) -> f64 {
        let (x, y) = (v.x.lo, v.y.lo);
        let (dx, dy) = (x - d.centre[0], y - d.centre[1]);
        let dist = dx.hypot(dy);
        match name {
            "x" => c.unit_of(x - (d.centre[0] - d.radius), 2.0 * d.radius, unit),
            "y" => c.unit_of(d.centre[1] + d.radius - y, 2.0 * d.radius, unit),
            "d" if unit == UnitWord::Pct => (dist / d.radius.max(1e-9)).min(1.0),
            "d" => c.unit_of((d.radius - dist).max(0.0), 0.0, unit),
            _ => {
                let turn = ((-dy).atan2(dx) / std::f64::consts::TAU).rem_euclid(1.0);
                match unit {
                    UnitWord::Deg => turn * 360.0,
                    UnitWord::Rad => turn * std::f64::consts::TAU,
                    _ => turn,
                }
            }
        }
    }

    fn needs(&self, e: &Expr, depth: usize) -> Result<Needs, String> {
        if depth > DEPTH {
            return Err("constants refer to each other in a loop".to_owned());
        }
        let mut n = Needs::default();
        let names = e.height_names();
        // the coordinates are worked out at a node too
        e.visit_calls(&mut |f| n.xy |= matches!(f, "x" | "y" | "d" | "a"));
        for name in names {
            match name {
                "x" | "y" | "$x" | "$y" | "$d" | "$a" | "$_y" => n.xy = true,
                "bottom" | "$bottom" => n.bottom = true,
                "$region" | "$tile" => {}
                c => {
                    let entry = self
                        .table
                        .get(c)
                        .ok_or_else(|| format!("no height called `{c}`"))?;
                    let m = self.needs(&entry.expr, depth + 1)?;
                    n.xy |= m.xy;
                    n.bottom |= m.bottom;
                }
            }
        }
        Ok(n)
    }

    /// The formula at one node: its heights, and the odds along them when it
    /// is a cloud.  The one evaluation of an expression, in the algebra of
    /// intervals with odds ([`Cloud`]).
    pub(crate) fn iv(&self, e: &Expr, v: &Vars, depth: usize) -> Result<Val, Flaw> {
        e.eval(&mut Cloud {
            eval: self,
            v,
            depth,
        })
    }
}

/// Evaluating a formula at one node: the algebra of intervals with the odds
/// along them.  A range is the interval it spans and its odds; two ranges
/// are two independent rolls, so their sum has the odds of the sum of two
/// rolls (a triangle for two plain ranges), not those of either.
struct Cloud<'a, 'b, 'c> {
    eval: &'a Eval<'b>,
    v: &'c Vars,
    depth: usize,
}

impl Algebra for Cloud<'_, '_, '_> {
    type Val = Val;
    type Err = Flaw;

    fn shortcut(&mut self, _e: &Expr) -> Option<Result<Val, Flaw>> {
        (self.depth > DEPTH)
            .then(|| Err("constants refer to each other in a loop".to_owned().into()))
    }

    fn raw_call(&mut self, _e: &Expr, name: &str, args: &[Expr]) -> Option<Result<Val, Flaw>> {
        if !matches!(name, "x" | "y" | "d" | "a") {
            return None;
        }
        let (eval, v) = (self.eval, self.v);
        Some((|| {
            let Some((c, own)) = eval.coords else {
                return Err(format!("`{name}(…)` is read only in format 1").into());
            };
            let (Some(region), Some(Expr::Unit(unit))) = (args.first(), args.get(1)) else {
                return Err(format!("`{name}` takes a region and a unit").into());
            };
            let id = c.region(region, own, eval.table)?;
            if let (Some(disc), true) = (eval.disc, id == own) {
                return Ok(Val::num(Iv::point(eval.on_disc(c, disc, name, *unit, v))));
            }
            Ok(Val::num(Iv::point(match name {
                "x" => c.x(id, v.k, *unit),
                "y" => c.y(id, v.k, *unit),
                "d" => c.d(id, v.k, *unit),
                _ => c.a(id, v.k, *unit),
            })))
        })())
    }

    fn number(&mut self, value: f64) -> Result<Val, Flaw> {
        Ok(Val::num(Iv::point(value)))
    }

    fn name(&mut self, n: &str) -> Result<Val, Flaw> {
        let (eval, v) = (self.eval, self.v);
        Ok(match n {
            "x" | "$x" => Val::num(v.x),
            // `$y` counts south from the north-west corner: `y($tile, sq)`;
            // `$_y` is the geometry's own, running north
            "$y" => Val::num(match eval.coords {
                Some((c, _)) => Iv::point(c.y(RegionTree::TILE, v.k, UnitWord::Sq)),
                None => v.y,
            }),
            "y" | "$_y" => Val::num(v.y),
            "$d" | "$a" => {
                let Some((c, own)) = eval.coords else {
                    return Err(format!("`{n}` is read only in format 1").into());
                };
                if let Some(disc) = eval.disc {
                    let name = if n == "$d" { "d" } else { "a" };
                    Val::num(Iv::point(eval.on_disc(c, disc, name, UnitWord::Pct, v)))
                } else {
                    Val::num(Iv::point(if n == "$d" {
                        c.d(own, v.k, UnitWord::Pct)
                    } else {
                        c.a(own, v.k, UnitWord::Pct)
                    }))
                }
            }
            "bottom" | "$bottom" => Val::num(v.bottom),
            c => {
                let entry = eval
                    .table
                    .get(c)
                    .ok_or_else(|| format!("no height called `{c}`"))?;
                self.depth += 1;
                let r = entry.expr.eval(self);
                self.depth -= 1;
                return r;
            }
        })
    }

    fn unit(&mut self, unit: UnitWord) -> Result<Val, Flaw> {
        Err(format!("`{}` is a unit, not a number here", unit.word()).into())
    }

    fn neg(&mut self, a: Val) -> Result<Val, Flaw> {
        Ok(a.negated())
    }

    fn binary(&mut self, e: &Expr, op: BinOp, a: Val, b: Val) -> Result<Val, Flaw> {
        arith(e, op, a, b)
    }

    fn range(&mut self, outer: &Expr, l: Val, h: Val, odds: Option<Odds<'_>>) -> Result<Val, Flaw> {
        let range = match outer {
            Expr::Weighted { range, .. } => range.as_ref(),
            other => other,
        };
        if l.odds.is_some() || h.odds.is_some() {
            return Err(Flaw::cloud(
                format!("`{range}` has a cloud at the end of a range"),
                "the ends of a range are numbers: write the cloud around the whole range",
            ));
        }
        let hull = Val {
            iv: l.iv.hull(h.iv),
            odds: None,
            ranged: true,
        };
        let Some(o) = odds else {
            return Ok(hull);
        };
        // the plain range is a band: `flat(…)`, and `low` or `high` of
        // ratio 1, are what a range alone is
        if outer.weighted().is_none() {
            return Ok(hull);
        }
        let w = if o.name == "mix" {
            let mut made = Vec::with_capacity(o.parts.len());
            for ((n, a), weight) in o.parts.iter().zip(o.args) {
                made.push((
                    *weight,
                    Weighting::from_call(n, a).map_err(Flaw::weighting)?,
                ));
            }
            Weighting::mix(made)
        } else {
            Weighting::from_call(o.name, o.args)
        }
        .map_err(Flaw::weighting)?;
        Ok(Val {
            iv: hull.iv,
            odds: (!w.is_flat()).then_some(w),
            ranged: true,
        })
    }

    fn list(&mut self, e: &Expr, items: Vec<(Val, Option<f64>)>) -> Result<Val, Flaw> {
        let mut values = Vec::with_capacity(items.len());
        for (k, (x, weight)) in items.into_iter().enumerate() {
            if x.ranged || x.iv.width() > 0.0 {
                let item = match e {
                    Expr::List { items } => {
                        items.get(k).map_or_else(String::new, |i| i.0.to_string())
                    }
                    _ => String::new(),
                };
                return Err(Flaw::cloud(
                    format!("`{item}` is a range inside a list: the values of a list are numbers"),
                    "write numbers: [1dmm:4, 2dmm:5, 3dmm:1]",
                ));
            }
            values.push((x.iv.lo, weight));
        }
        let d = Dist::list(&values).map_err(Flaw::weighting)?;
        Ok(Val {
            iv: Iv::new(d.lo, d.hi),
            odds: Some(d.weighting),
            ranged: true,
        })
    }

    fn call(&mut self, e: &Expr, name: &str, args: Vec<Val>) -> Result<Val, Flaw> {
        function(e, name, args)
    }
}

/// A formula at one node: the heights it can be, the odds along them if it
/// is a cloud (the plain range has none), and whether it holds a range.
#[derive(Clone, Debug)]
pub(crate) struct Val {
    pub(crate) iv: Iv,
    pub(crate) odds: Option<Weighting>,
    pub(crate) ranged: bool,
}

impl Val {
    const fn num(iv: Iv) -> Self {
        Self {
            iv,
            odds: None,
            ranged: false,
        }
    }

    /// Every height times -1: low and high swap, and the odds turn round
    /// with them, so low stays the lower surface.
    fn negated(self) -> Self {
        Self {
            iv: -self.iv,
            odds: self.odds.map(|w| w.mirrored()),
            ranged: self.ranged,
        }
    }

    fn is_cloud(&self) -> bool {
        self.odds.is_some()
    }
}

/// Why a formula cannot be worked out: the diagnostic's code, what is wrong
/// and what to write.
pub(crate) struct Flaw {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) help: Option<String>,
}

impl From<String> for Flaw {
    fn from(message: String) -> Self {
        Self {
            code: dt_lang::codes::VALUE,
            message,
            help: None,
        }
    }
}

impl Flaw {
    /// Arithmetic a cloud does not have (`dt::cloud`).
    fn cloud(message: String, help: &str) -> Self {
        Self {
            code: dt_lang::codes::CLOUD,
            message,
            help: Some(help.to_owned()),
        }
    }

    fn weighting(w: dt_value::WeightingError) -> Self {
        Self {
            code: w.code,
            message: w.message,
            help: Some(w.help),
        }
    }
}

/// What a cloud may be written with (the help of every refusal).
const CLOUD_RULE: &str = "a cloud may be shifted and scaled by numbers (`+ - * /`) and held by `min`, `max`, `clamp` and `smooth` of numbers; write the result as one range with its odds, such as `high($bottom .. $bottom + 20dmm)`";

/// `a op b` where either may be a cloud: a number added or multiplied moves
/// or stretches both surfaces and keeps the weighting; a negative factor swaps
/// them, with the odds turned round.  Two ranges are two independent rolls: a
/// sum or product of them has the odds of the combination, worked out on a
/// grid of each range's quantiles ([`independent`]).
fn arith(e: &Expr, op: BinOp, a: Val, b: Val) -> Result<Val, Flaw> {
    let cloud = a.is_cloud() || b.is_cloud();
    let ranged = a.ranged || b.ranged;
    // two ranges with width: each its own roll
    let both = a.ranged && b.ranged && a.iv.width() > 0.0 && b.iv.width() > 0.0;
    if both && op == BinOp::Div {
        return Err(Flaw::cloud(
            format!("`{e}` divides by a range: the odds of its result are not a weighting"),
            "divide a range by a number, not by a range",
        ));
    }
    if both && op == BinOp::Mul && cloud {
        return Err(Flaw::cloud(
            format!("`{e}` multiplies a cloud by something that is a range at the point"),
            "multiply a cloud by a number; the factor must be one value at each point",
        ));
    }
    Ok(match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul if both => {
            let iv = match op {
                BinOp::Add => a.iv + b.iv,
                BinOp::Sub => a.iv - b.iv,
                _ => a.iv * b.iv,
            };
            Val {
                iv,
                odds: independent(&[&a, &b], iv, |x| match op {
                    BinOp::Add => x[0] + x[1],
                    BinOp::Sub => x[0] - x[1],
                    _ => x[0] * x[1],
                }),
                ranged,
            }
        }
        BinOp::Add => Val {
            iv: a.iv + b.iv,
            odds: a.odds.or(b.odds),
            ranged,
        },
        BinOp::Sub => Val {
            iv: a.iv - b.iv,
            odds: a.odds.or_else(|| b.odds.map(|w| w.mirrored())),
            ranged,
        },
        BinOp::Mul => {
            let (cloud_side, k) = if a.is_cloud() { (&a, &b) } else { (&b, &a) };
            if cloud && k.iv.width() > 0.0 {
                return Err(Flaw::cloud(
                    format!("`{e}` multiplies a cloud by something that is a range at the point"),
                    "multiply a cloud by a number; the factor must be one value at each point",
                ));
            }
            let mut odds = cloud_side.odds.clone();
            if k.iv.lo < 0.0 {
                odds = odds.map(|w| w.mirrored());
            }
            if cloud && k.iv.lo == 0.0 {
                odds = None;
            }
            Val {
                iv: a.iv * b.iv,
                odds,
                ranged,
            }
        }
        BinOp::Div => {
            if b.iv.contains(0.0) {
                return Err("dividing by something that can be zero".to_owned().into());
            }
            if b.is_cloud() {
                return Err(Flaw::cloud(
                    format!(
                        "`{e}` divides by a cloud: the odds of its inverse are not a weighting"
                    ),
                    CLOUD_RULE,
                ));
            }
            if a.is_cloud() && b.iv.width() > 0.0 {
                return Err(Flaw::cloud(
                    format!("`{e}` divides a cloud by something that is a range at the point"),
                    "divide a cloud by a number; the divisor must be one value at each point",
                ));
            }
            let odds = if b.iv.hi < 0.0 {
                a.odds.map(|w| w.mirrored())
            } else {
                a.odds
            };
            Val {
                iv: a.iv * Iv::new(1.0 / b.iv.hi, 1.0 / b.iv.lo),
                odds,
                ranged,
            }
        }
    })
}

/// Quantile points taken along each range, and bins the combined odds are
/// cut into (a `steps` weighting takes 32 at most).
const GRID: usize = 32;
const BINS: usize = 32;

/// The odds along `iv` of `f` of independent rolls, one in each of `vals`
/// (each rolled as its own range and odds say): the rolls are met on a grid
/// of `GRID` quantiles each, midpoints so no end is ever claimed, and counted
/// into `BINS` equal bins across `iv`.  The same values give the same odds
/// whatever the machine: only `+ - * /` and the weightings' own curves.
fn independent(vals: &[&Val], iv: Iv, f: impl Fn(&[f64]) -> f64) -> Option<Weighting> {
    let width = iv.hi - iv.lo;
    if width.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let quantile = |v: &Val, i: usize| {
        #[allow(clippy::cast_precision_loss)]
        let u = (i as f64 + 0.5) / GRID as f64;
        let t = v.odds.as_ref().map_or(u, |w| w.unit(u));
        v.iv.lo + v.iv.width() * t
    };
    let mut counts = vec![0.0_f64; BINS];
    let mut at = vec![0_usize; vals.len()];
    let mut x = vec![0.0_f64; vals.len()];
    loop {
        for (k, v) in vals.iter().enumerate() {
            x[k] = quantile(v, at[k]);
        }
        let r = f(&x);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let bin = (((r - iv.lo) / width * BINS as f64).floor().max(0.0) as usize).min(BINS - 1);
        counts[bin] += 1.0;
        // the next point of the grid
        let mut k = 0;
        while k < at.len() {
            at[k] += 1;
            if at[k] < GRID {
                break;
            }
            at[k] = 0;
            k += 1;
        }
        if k == at.len() {
            break;
        }
    }
    Weighting::steps(&counts).ok()
}

/// A function of values, any of which may be a cloud: `min`, `max`, `clamp`
/// and `smooth` act on its two surfaces and keep the weighting; `sin`, `cos`
/// and `abs` can fold the surfaces over each other, so a cloud is refused
/// there.
fn function(e: &Expr, name: &str, vals: Vec<Val>) -> Result<Val, Flaw> {
    let clouds = vals.iter().filter(|x| x.is_cloud()).count();
    if clouds > 0 && matches!(name, "sin" | "cos" | "abs") {
        return Err(Flaw::cloud(
            format!(
                "`{e}` takes `{name}` of a cloud: it can fold the low and high surfaces over each other"
            ),
            "write the function inside the range, over its ends, and the cloud round the result: high(sin(…) .. sin(…))",
        ));
    }
    let func = Func::of(name, vals.len())?;
    let ivs: Vec<Iv> = vals.iter().map(|x| x.iv).collect();
    let iv = call(func, &ivs);
    let wide: Vec<&Val> = vals
        .iter()
        .filter(|x| x.ranged && x.iv.width() > 0.0)
        .collect();
    // two ranges are two rolls: the function of them has the odds of both
    let odds = if wide.len() > 1 {
        let args: Vec<(&Val, bool)> = vals
            .iter()
            .map(|x| (x, x.ranged && x.iv.width() > 0.0))
            .collect();
        independent(&wide, iv, |roll| {
            let mut next = roll.iter();
            let numbers: Vec<f64> = args
                .iter()
                .map(|(v, rolled)| {
                    if *rolled {
                        next.next().copied().unwrap_or(f64::NAN)
                    } else {
                        0.5 * (v.iv.lo + v.iv.hi)
                    }
                })
                .collect();
            func.of_numbers(&numbers)
        })
    } else {
        vals.iter().find_map(|x| x.odds.clone())
    };
    Ok(Val {
        iv,
        odds,
        ranged: vals.iter().any(|x| x.ranged),
    })
}

/// Whether the angles `[a, b]` (degrees) pass `center` plus a whole turn.
fn passes(a: f64, b: f64, center: f64) -> bool {
    let k = ((a - center) / 360.0).ceil();
    center + 360.0 * k <= b
}

fn sin_iv(a: Iv) -> Iv {
    if a.width() >= 360.0 {
        return Iv::new(-1.0, 1.0);
    }
    let (s0, s1) = (sin_deg(a.lo), sin_deg(a.hi));
    let mut r = Iv::new(s0, s1);
    if passes(a.lo, a.hi, 90.0) {
        r.hi = 1.0;
    }
    if passes(a.lo, a.hi, 270.0) {
        r.lo = -1.0;
    }
    r
}

/// A function over intervals: the surfaces it can make.
fn call(func: Func, a: &[Iv]) -> Iv {
    match func {
        Func::Sin => sin_iv(a[0]),
        Func::Cos => sin_iv(a[0] + Iv::point(90.0)),
        Func::Abs => {
            let t = a[0];
            if t.lo >= 0.0 {
                t
            } else if t.hi <= 0.0 {
                -t
            } else {
                Iv::new(0.0, (-t.lo).max(t.hi))
            }
        }
        Func::Min => Iv::new(a[0].lo.min(a[1].lo), a[0].hi.min(a[1].hi)),
        Func::Max => Iv::new(a[0].lo.max(a[1].lo), a[0].hi.max(a[1].hi)),
        Func::Clamp => {
            let (t, lo, hi) = (a[0], a[1], a[2]);
            let up = Iv::new(t.lo.max(lo.lo), t.hi.max(lo.hi));
            Iv::new(up.lo.min(hi.lo), up.hi.min(hi.hi))
        }
        Func::Smooth => Iv::new(smooth(a[0].lo), smooth(a[0].hi)),
    }
}

/// The lattice and the stack so far, shared by every body.
struct Ground<'a> {
    nx: usize,
    ny: usize,
    step: f64,
    per: f64,
    scale: f64,
    /// Format 1: the regions' coordinates.
    coords: Option<&'a Coords<'a>>,
}

impl Ground<'_> {
    /// A top as a band at every node in `only` (every node if `None`), world
    /// mm; `under` is what `bottom` means.  Heights are written at the
    /// reference square and scale with the system.
    fn band(
        &self,
        table: &ConstTable,
        e: &Expr,
        under: &Band,
        only: Option<&Mask>,
        own: usize,
    ) -> Result<Band, Flaw> {
        let eval = Eval {
            table,
            coords: self.coords.map(|c| (c, own)),
            disc: None,
        };
        let needs = eval.needs(e, 0)?;
        let none = Vars {
            x: Iv::point(0.0),
            y: Iv::point(0.0),
            bottom: Iv::point(0.0),
            k: 0,
        };
        let odds = std::cell::RefCell::new(OddsBuilder::new(self.nx * self.ny));
        if !needs.xy && !needs.bottom {
            let val = eval.iv(e, &none, 0)?;
            let iv = val.iv;
            if val.odds.is_some() {
                let mut o = odds.borrow_mut();
                for k in 0..self.nx * self.ny {
                    o.set(k, val.odds.as_ref());
                }
            }
            return Ok(Band::flat(
                self.nx,
                self.ny,
                self.step,
                iv.lo * self.scale,
                iv.hi * self.scale,
            )
            .with_odds(odds.into_inner().finish()));
        }
        let err: std::cell::RefCell<Option<Flaw>> = std::cell::RefCell::new(None);
        let band = Band::from_fn(self.nx, self.ny, self.step, |px, py| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, j) = (
                (px / self.step).round() as usize,
                (py / self.step).round() as usize,
            );
            let k = j * self.nx + i;
            if only.is_some_and(|m| !m.bits[k]) {
                return Iv::point(0.0);
            }
            let (x, y) = node_xy(k, self.nx, self.per);
            let b = under.at(k);
            let vars = Vars {
                x: Iv::point(x),
                y: Iv::point(y),
                bottom: Iv::new(b.lo / self.scale, b.hi / self.scale),
                k,
            };
            match eval.iv(e, &vars, 0) {
                Ok(val) => {
                    odds.borrow_mut().set(k, val.odds.as_ref());
                    Iv::new(val.iv.lo * self.scale, val.iv.hi * self.scale)
                }
                Err(m) => {
                    err.borrow_mut().get_or_insert(m);
                    Iv::point(0.0)
                }
            }
        });
        match err.into_inner() {
            Some(f) => Err(f),
            None => Ok(band.with_odds(odds.into_inner().finish())),
        }
    }
}

fn loc(src: &crate::diag::Src, span: miette::SourceSpan, path: &str) -> String {
    let text = src.inner();
    let off = span.offset().min(text.len());
    let line = text[..off].matches('\n').count() + 1;
    format!("{path}:{line}")
}

fn text_of(src: &crate::diag::Src, span: miette::SourceSpan) -> String {
    let t = src.inner();
    let (a, b) = (
        span.offset().min(t.len()),
        (span.offset() + span.len()).min(t.len()),
    );
    t[a..b].split_whitespace().collect::<Vec<_>>().join(" ")
}

fn mm(v: f64) -> String {
    format!("{v:.2} mm")
}

/// **solve**: the Solve phase, on `f`'s own lattice, the one the bodies are
/// made on, so a band a build fills is the band `check` solved.  What each
/// body is (land, fluid or pieces) is the element schema's word.
///
/// # Errors
/// What cannot be solved: an unknown place, a height that cannot be worked
/// out, a pin with no ground, a layer that can never be thick enough.
#[allow(clippy::too_many_lines)]
pub fn solve(
    tile: &Tile,
    f: &Frame,
    opts: &Options,
    regions: Option<&RegionTree>,
    masks: &[Mask],
) -> Result<SolveReport, Vec<Diag>> {
    let h = &tile.heights;
    let cells = f.cells_per_square;
    let (nx, ny) = f.nodes();
    let coords = regions.map(|t| Coords::new(t, f.square_mm, opts.printer.layer_mm));
    let g = Ground {
        nx,
        ny,
        step: f.step_mm(),
        per: f64::from(cells),
        scale: f.square_mm / dt_kit::REFERENCE_SQUARE_MM,
        coords: coords.as_ref(),
    };
    let places = Places::with_masks(tile, f, masks);
    let mut errors = Vec::new();
    let mut notes = Vec::new();
    let mut explain = Vec::new();
    let mut bodies: Vec<SolvedBody> = Vec::new();
    let mut edge_mm: Option<f64> = None;
    // the surface so far: the higher of every body laid, inside its region
    let mut under = Band::flat(nx, ny, g.step, 0.0, 0.0);
    let mut under_lo = Field::filled(nx, ny, g.step, 0.0);
    let mut under_hi = under_lo.clone();
    let mut first_land = true;
    let skin = 3.0 * opts.printer.layer_mm;

    for o in h.consts.overridden() {
        explain.push(format!("override {} {} → {}", o.name, o.from, o.to));
    }
    // overrides made inside blocks (format 1), once each
    let mut seen = h.consts.overridden().len();
    for b in tile.stack.iter().filter(|e| e.is_body()) {
        for o in b
            .table
            .overridden()
            .iter()
            .skip(seen.min(b.table.overridden().len()))
        {
            explain.push(format!("override {} {} → {}", o.name, o.from, o.to));
        }
        seen = seen.max(b.table.overridden().len());
    }

    for (entry, b) in tile.stack.iter().enumerate().filter(|(_, e)| e.is_body()) {
        let element = b.what.value.as_str();
        // what the schema says the body is: land, fluid or pieces
        let Some(kind) = dt_doc::schema::element(element).and_then(|e| e.body) else {
            errors.push(Diag::at(
                &b.src,
                b.what.span,
                dt_lang::codes::INTERNAL,
                format!("`{element}` is a body of the stack but the schema gives it no body"),
                "here",
            ));
            continue;
        };
        let land = !matches!(kind, Body::Fluid { .. });
        // a floor or a cliff is a pieces body: it takes its own defaults
        // wherever it is laid (a floor's edge pin is its edge contract), and
        // never uses up the first land layer's, which are the dirt's
        let pieces = kind == Body::Pieces;
        let default: Option<&DefaultBody> = h
            .defaults
            .iter()
            .find(|d| d.element == element)
            .filter(|_| !land || pieces || first_land);
        if land && !pieces {
            first_land = false;
        }
        let region = match b.area.as_ref().map(|a| &a.value) {
            None => Mask::new(nx, ny, true),
            Some(r) => match places.mask(&PlaceRef::Name(r.clone())) {
                Ok(m) => m,
                Err(msg) => {
                    errors.push(Diag::at(
                        &b.src,
                        b.what.span,
                        dt_lang::codes::PLACE,
                        msg,
                        "here",
                    ));
                    continue;
                }
            },
        };
        let top = b
            .top
            .as_ref()
            .map(|t| (&t.expr, t.span))
            .or_else(|| default.and_then(|d| d.top.as_ref()).map(|(e, sp)| (e, *sp)));
        let Some((top, top_span)) = top else {
            errors.push(
                Diag::at(
                    &tile.src,
                    b.span,
                    dt_lang::codes::MISSING_CONTROL,
                    format!("`{}` needs a `top=`", element),
                    "no top",
                )
                .help("write the band it fills, such as `top=\"$dirtEdgeHeight + 35dmm\"`"),
            );
            continue;
        };
        let top_src = match &b.top {
            Some(t) => &t.src,
            None => default.map_or(&b.src, |d| &d.from.src),
        };
        let region_id = regions.map_or(0, |t| t.of_entry.get(entry).copied().unwrap_or(0));
        let own = match g.band(&b.table, top, &under, None, region_id) {
            Ok(x) => x,
            Err(f) => {
                let mut d = Diag::at(
                    top_src,
                    top_span,
                    f.code,
                    format!("can't work out this top: {}", f.message),
                    "here",
                );
                d.help = f.help;
                errors.push(d);
                continue;
            }
        };
        if let Some(d) = default.filter(|_| b.top.is_none()) {
            explain.push(format!(
                "default {} top={} ({})",
                element,
                text_of(top_src, top_span),
                loc(&d.from.src, top_span, &d.from.path)
            ));
        }
        // the pins: the library's, then the tile's
        let decls: Vec<&PinDecl> = default
            .iter()
            .flat_map(|d| d.pins.iter())
            .chain(b.pins.iter())
            .collect();
        let mut pins: Vec<Pin> = Vec::new();
        let mut ok = true;
        for p in &decls {
            let place = match places.mask(&p.at) {
                Ok(m) => m,
                Err(msg) => {
                    errors.push(Diag::at(
                        &p.from.src,
                        p.at_span,
                        dt_lang::codes::PLACE,
                        msg,
                        "here",
                    ));
                    ok = false;
                    continue;
                }
            };
            let band = match g.band(&b.table, &p.top, &under, Some(&place), region_id) {
                Ok(x) => x,
                Err(f) => {
                    let mut d = Diag::at(
                        &p.from.src,
                        p.top_span,
                        f.code,
                        format!("can't work out this pin's top: {}", f.message),
                        "here",
                    );
                    d.help = f.help;
                    errors.push(d);
                    ok = false;
                    continue;
                }
            };
            match Pin::new(place, band) {
                Ok(pin) => pins.push(pin),
                Err(r) => {
                    errors.push(Diag::plain(r.code(), r.to_string()));
                    ok = false;
                }
            }
        }
        if !ok {
            continue;
        }
        let ease = match kind {
            // a floor's slabs and a cliff's stones step: a pin holds its
            // place, nothing eases, unless the body is told how steeply its
            // pieces stand
            Body::Pieces => b.slope.map_or(Ease::Step, Ease::Slope),
            Body::Fluid { settle_mm } => Ease::Settle(settle_mm * g.scale),
            Body::Land { slope_deg } => Ease::Slope(b.slope.unwrap_or(slope_deg)),
        };
        let solved = match solve_traced(&own, &pins, ease) {
            Ok(s) => s,
            Err(Refusal::EmptyPlace { pin }) => {
                let p = decls[pin];
                errors.push(
                    Diag::at(
                        &p.from.src,
                        p.at_span,
                        dt_lang::codes::EMPTY,
                        format!("this pin's place `{}` covers nothing", p.at),
                        "no ground here",
                    )
                    .help("its region is empty: check the lines and fill points that make it"),
                );
                continue;
            }
            Err(r) => {
                errors.push(Diag::at(
                    &b.src,
                    b.what.span,
                    r.code(),
                    r.to_string(),
                    "here",
                ));
                continue;
            }
        };
        // a layer that can never be thick enough
        if land
            && !bodies.is_empty()
            && let Err(Refusal::Thin {
                at,
                available,
                needed,
            }) = solved.band.thin(&under, &region, skin)
        {
            errors.push(
                Diag::at(
                    &tile.src,
                    b.span,
                    dt_lang::codes::THIN,
                    format!(
                        "the {} is at most {} thick at ({:.2}, {:.2}) mm, under the {} the printer's skin needs",
                        element,
                        mm(available),
                        at[0],
                        at[1],
                        mm(needed)
                    ),
                    "too thin",
                )
                .help("raise the top's high end, or lower what it rests on"),
            );
        }
        // lints (a body that follows what is under it, a film on stone, is
        // as steep as that is)
        let follows = top.height_names().contains(&"bottom");
        let slope = match ease {
            Ease::Slope(d) => Some(d),
            _ => None,
        };
        for lint in lints(&solved, &pins, slope.unwrap_or(89.9)) {
            match lint {
                Lint::Steep(s) if slope.is_some() && !follows => {
                    let lead = s.pins.iter().flatten().next().map(|&i| decls[i]);
                    let (src, span) = lead.map_or((&b.src, b.what.span), |p| (&p.from.src, p.span));
                    let mut named: Vec<String> = s
                        .pins
                        .iter()
                        .flatten()
                        .map(|&i| pin_label(decls[i]))
                        .collect();
                    named.dedup();
                    notes.push(
                        Diag::at(
                            src,
                            span,
                            dt_lang::codes::STEEP,
                            format!(
                                "the {} is forced to {:.1}°, steeper than its {:.0}° slope: {} up to {} over {}, between ({:.2}, {:.2}) and ({:.2}, {:.2}) mm ({} places)",
                                element,
                                s.deg,
                                s.limit_deg,
                                mm(s.from_mm),
                                mm(s.to_mm),
                                mm(s.run_mm),
                                s.at[0],
                                s.at[1],
                                s.to[0],
                                s.to[1],
                                s.pairs
                            ),
                            "pulls hard",
                        )
                        .help(format!(
                            "{}: soften the heights, spread the places apart, or allow it with `slope`",
                            named.join(" and ")
                        )),
                    );
                }
                Lint::Steep(_) => {}
                Lint::Shadowed(sh) if is_default_pin(decls[sh.pin]) => {}
                Lint::Shadowed(sh) => {
                    let p = decls[sh.pin];
                    let by: Vec<String> =
                        sh.covered_by.iter().map(|&i| pin_label(decls[i])).collect();
                    notes.push(
                        Diag::at(
                            &p.from.src,
                            p.span,
                            dt_lang::codes::SHADOWED,
                            format!(
                                "this pin wins nowhere: later pins cover all of `{}` ({})",
                                p.at,
                                by.join(", ")
                            ),
                            "never wins",
                        )
                        .help("write it after the pins that cover it, or remove it"),
                    );
                }
            }
        }
        // where the tile's own pins move the rim off the edge height
        if let Some(edge) = decls
            .iter()
            .position(|p| p.from.default && p.at == PlaceRef::Name("edges".into()))
        {
            if land && !pieces && edge_mm.is_none() {
                // where the library's edge pin holds the rim, as solved
                edge_mm = (0..nx * ny)
                    .filter(|&k| {
                        let (i, j) = (k % nx, k / nx);
                        (i == 0 || j == 0 || i == nx - 1 || j == ny - 1)
                            && region.bits[k]
                            && solved.owner[k] == Some(edge)
                    })
                    .map(|k| solved.band.at(k).lo)
                    .reduce(f64::min);
            }
            let off: Vec<usize> = (0..nx * ny)
                .filter(|&k| {
                    let (i, j) = (k % nx, k / nx);
                    (i == 0 || j == 0 || i == nx - 1 || j == ny - 1)
                        && region.bits[k]
                        && solved.owner[k].is_some_and(|o| o != edge)
                        // a pin of the tile's own that holds the library's
                        // edge height keeps the tile standard
                        && {
                            let (b, e) = (solved.band.at(k), pins[edge].band.at(k));
                            (b.lo - e.lo).abs() > 1e-6 || (b.hi - e.hi).abs() > 1e-6
                        }
                })
                .collect();
            if let Some(&first) = off.first() {
                let owner = solved.owner[first].map(|o| decls[o]);
                // where along each edge, in squares
                let mut stretches: Vec<String> = Vec::new();
                for (name, on, along) in [
                    ("south", 1_u8, 0_u8),
                    ("north", 2, 0),
                    ("west", 3, 1),
                    ("east", 4, 1),
                ] {
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for &k in &off {
                        let (i, j) = (k % nx, k / nx);
                        let here = match on {
                            1 => j == 0,
                            2 => j == ny - 1,
                            3 => i == 0,
                            _ => i == nx - 1,
                        };
                        if here {
                            let t = if along == 0 { i } else { j };
                            #[allow(clippy::cast_precision_loss)]
                            let t = t as f64 / g.per;
                            lo = lo.min(t);
                            hi = hi.max(t);
                        }
                    }
                    if lo <= hi {
                        stretches.push(format!("{name} edge {lo:.2} to {hi:.2}"));
                    }
                }
                let (src, span) = owner.map_or((&b.src, b.what.span), |p| (&p.from.src, p.span));
                notes.push(
                    Diag::at(
                        src,
                        span,
                        dt_lang::codes::EDGE,
                        format!(
                            "the {} meets the tile's edge at a height of its own, not the library's, along the {} (squares): it no longer butts a standard tile there",
                            element,
                            stretches.join(", ")
                        ),
                        "wins at the edge",
                    )
                    .help("add a pin at the edge after it to keep the standard edge height"),
                );
            }
        }
        // the report
        let mut reports = Vec::new();
        for (i, p) in decls.iter().enumerate() {
            let wins = solved.owner.iter().filter(|&&o| o == Some(i)).count();
            let place_nodes = pins[i].place.bits.iter().filter(|&&x| x).count();
            let text = text_of(&p.from.src, p.span);
            let at = loc(&p.from.src, p.span, &p.from.path);
            if p.from.default {
                explain.push(format!(
                    "default {} {text} ({at}): wins {wins} of {place_nodes} nodes",
                    element
                ));
            } else {
                explain.push(format!(
                    "{} {text}: wins {wins} of {place_nodes} nodes",
                    element
                ));
            }
            reports.push(PinReport {
                text,
                name: p.name.clone(),
                default: p.from.default,
                at,
                place_nodes,
                wins,
            });
        }
        // what is under it, before it is laid
        let below = under.clone();
        // a pieces body's stones are counted from here and stand a mm lower:
        // the lowest the surface is anywhere, never under 1 mm (the base)
        let land_mm = (0..nx * ny)
            .map(|k| below.at(k).lo)
            .reduce(f64::min)
            .unwrap_or(0.0)
            .max(1.0);
        let mut left_out = 0;
        // lay the body on the surface
        if land {
            for k in 0..nx * ny {
                if region.bits[k] {
                    let s = solved.band.at(k);
                    under_lo.z[k] = under_lo.z[k].max(s.lo);
                    under_hi.z[k] = under_hi.z[k].max(s.hi);
                }
            }
            match Band::new(under_lo.clone(), under_hi.clone()) {
                Ok(b) => under = b,
                // degenerate: the surface under later bodies keeps what it
                // had, and the body counts it
                Err(_) => left_out = 1,
            }
        }
        bodies.push(SolvedBody {
            entry,
            role: if land { Role::Land } else { Role::Fluid },
            element: b.what.value.clone(),
            label: b.label.clone(),
            band: solved.band,
            region_nodes: region.bits.iter().filter(|&&x| x).count(),
            below,
            land_mm,
            written: b.top.is_some() || !b.pins.is_empty(),
            top: SolvedTop {
                text: text_of(top_src, top_span),
                default_from: b
                    .top
                    .is_none()
                    .then(|| default.map(|d| d.from.path.clone()))
                    .flatten(),
            },
            pins: reports,
            left_out,
            nx,
            ny,
            per: g.per,
        });
    }
    // the calm level exists only where the tile has water. Having water and
    // failing to read the level is a mistake the document must hear of, not
    // water that quietly behaves as if uncalmed
    let calm_mm = match bodies.iter().find(|b| b.role == Role::Fluid) {
        None => None,
        Some(fluid) => {
            let calm = "$waterSurfaceHeight";
            match h
                .consts
                .scalar(&Expr::Name(calm.to_owned()), 0.5, &|_| None)
            {
                Ok(v) => Some(v * g.scale),
                Err(msg) => {
                    let at = tile.stack.get(fluid.entry).map(|e| (&e.src, e.what.span));
                    let message = format!("the calm water level `{calm}` cannot be read: {msg}");
                    let help = "water is calmed to this constant; the library's heights part defines it, and a tile may `override` it with a plain number or length";
                    errors.push(match at {
                        Some((src, span)) => {
                            Diag::at(src, span, dt_lang::codes::CALM, message, "this water")
                                .help(help)
                        }
                        None => Diag::plain(dt_lang::codes::CALM, message).help(help),
                    });
                    None
                }
            }
        }
    };
    // one for each entry, and the connector last when the tile has one
    let mut by_entry = vec![None; tile.stack.len() + usize::from(tile.has_base)];
    for (i, b) in bodies.iter().enumerate() {
        if let Some(slot) = by_entry.get_mut(b.entry) {
            *slot = Some(i);
        }
    }
    if errors.is_empty() {
        Ok(SolveReport {
            cells_per_square: cells,
            bodies,
            by_entry,
            edge_mm,
            calm_mm,
            notes,
            explain,
        })
    } else {
        Err(errors)
    }
}

/// A pin the library's defaults supply. A tile overrides one by covering it
/// with a pin of its own, so being shadowed is what it is for: `dt::shadowed`
/// is said of the tile's own pins only (the language guide says so, and a
/// test in `tests/heights.rs` holds it).
fn is_default_pin(p: &PinDecl) -> bool {
    p.from.default
}

fn pin_label(p: &PinDecl) -> String {
    p.name.as_ref().map_or_else(
        || format!("the pin at `{}`", p.at),
        |n| format!("pin `{n}`"),
    )
}
