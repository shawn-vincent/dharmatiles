//! The constants a tile sees: the library's heights, then the tile's own
//! `let`s, with `override`s applied to the whole table (so overriding
//! `waterSurfaceHeight` moves `waterEdgeHeight` with it).

use std::collections::HashMap;

use miette::SourceSpan;

use crate::dim::Dim;
use crate::expr::Expr;
use crate::model::{Const, ConstKind};

/// Where a constant's value was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The library (`library/parts/heights.dtile`).
    Library,
    /// The tile's own `let`.
    Tile,
    /// The tile's `override` of a library or tile constant.
    Override,
}

/// One constant.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Its name.
    pub name: String,
    /// Its value, an expression of other constants.
    pub expr: Expr,
    /// What it measures.
    pub dim: Dim,
    /// Where its value was written.
    pub origin: Origin,
    /// The text of that value, for notes.
    pub text: String,
    /// Where it was written, in the tile's text (`None` for the library).
    pub span: Option<SourceSpan>,
}

/// A replaced constant, for `check --explain`.
#[derive(Debug, Clone)]
pub struct Overridden {
    /// The name.
    pub name: String,
    /// The value before.
    pub from: String,
    /// The value now.
    pub to: String,
    /// Where the override is written.
    pub span: SourceSpan,
}

/// The constant table of one tile.
#[derive(Debug, Clone, Default)]
pub struct ConstTable {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    overridden: Vec<Overridden>,
}

/// What is wrong with a `let` or an `override` entering a scope.
#[derive(Debug, Clone)]
pub struct ScopeError {
    /// The diagnostic code.
    pub code: &'static str,
    /// What is wrong.
    pub message: String,
    /// The label under the name.
    pub label: &'static str,
    /// Where the name is written.
    pub span: SourceSpan,
    /// What to do about it.
    pub help: Option<String>,
    /// A near word for the name at `span` (a suggestion, with its fix, is made where the text is known).
    pub near: Option<String>,
}

/// Why a formula cannot be read in a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadFlaw {
    /// No caller installed the name (`near`: the closest that is).
    Unknown {
        /// The name.
        name: String,
        /// The closest name there is.
        near: Option<String>,
    },
    /// A position or height under it, worked out at each point, where a
    /// value is drawn once.
    PerPoint {
        /// The name, `$x`, `$y` or `$bottom`.
        name: String,
    },
    /// A name the element that calls the formula does not set (a tree's top
    /// has no `$x`: it is worked out once, not at each point).
    NotSet {
        /// The name, or the coordinate call.
        name: String,
        /// The element.
        element: String,
        /// What it does set.
        sets: Vec<String>,
    },
    /// Names that read each other round a loop.
    Loop,
}

/// Why a `parent(…)` cannot be satisfied where a formula is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParentFlaw {
    /// `parent` of the tile: it has no parent.
    Tile,
    /// `parent` taken `hops` times from `$region`, which has only `parents`
    /// above it before the tile.
    Past {
        /// How many `parent`s are written.
        hops: usize,
        /// How many parents the thing's region has.
        parents: usize,
    },
}

/// Where a region value starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RegionBase {
    Own,
    Tile,
}

fn depth0() -> usize {
    0
}

/// An expression as text, for notes.
fn show(e: &Expr) -> String {
    e.to_string()
}

impl ConstTable {
    /// The context at the root: the library's heights as `$` names (each
    /// written with its `$`, and reading `$x` and the like).
    #[must_use]
    pub fn root(library: &[Const]) -> Self {
        let mut t = Self::default();
        for c in library {
            t.index.insert(c.name.value.clone(), t.entries.len());
            t.entries.push(Entry {
                text: show(&c.value.value),
                name: c.name.value.clone(),
                expr: c.value.value.clone(),
                dim: c.dim,
                origin: Origin::Library,
                span: None,
            });
        }
        t
    }

    /// The scope a block makes: this context with the block's `let`s and
    /// `override`s, in written order; the old values are back when the block
    /// ends, since this table is not changed.
    ///
    /// A `$` name is read where it is used, so a `let` that holds a formula
    /// follows an `override` made below it.  A plain name is lexical: its
    /// formula is captured here, so what it reads is what is written.  An
    /// `override` that reads its own name reads the value it replaces.
    #[must_use]
    pub fn enter(&self, consts: &[Const]) -> (Self, Vec<ScopeError>) {
        let mut t = self.clone();
        let mut errors = Vec::new();
        for c in consts {
            let name = c.name.value.as_str();
            let expr = {
                let here = &t;
                c.value.value.map_names(&mut |n| {
                    if n.starts_with('$') {
                        None
                    } else {
                        here.get(n).map(|e| e.expr.clone())
                    }
                })
            };
            let has = t.index.contains_key(name);
            match (c.kind, name.starts_with('$')) {
                (ConstKind::Let, true) if has => errors.push(ScopeError {
                    code: "dt::name",
                    message: format!("the context already has `{name}`"),
                    label: "already installed",
                    span: c.name.span,
                    help: Some(format!(
                        "`override {name} …` replaces it for this scope and everything called from it"
                    )),
                    near: None,
                }),
                (ConstKind::Override, true) if !has => {
                    let names: Vec<&str> = t
                        .entries
                        .iter()
                        .map(|e| e.name.as_str())
                        .filter(|n| n.starts_with('$'))
                        .collect();
                    let near = crate::schema::did_you_mean(name, names.iter().copied());
                    errors.push(ScopeError {
                        code: "dt::name",
                        message: format!("no caller installed `{name}` to override"),
                        label: "not in the context",
                        span: c.name.span,
                        help: Some(match near {
                            Some(_) => format!("or `let {name} …` installs a new one"),
                            None => format!(
                                "`let {name} …` installs a new `$` name; `override` replaces one a caller installed"
                            ),
                        }),
                        near: near.map(str::to_owned),
                    });
                }
                (ConstKind::Override, true) => {
                    let prev = t.entries[t.index[name]].clone();
                    // an override reading its own name reads the value it replaces
                    let expr = expr.map_names(&mut |n| (n == name).then(|| prev.expr.clone()));
                    t.overridden.push(Overridden {
                        name: name.to_owned(),
                        from: show(&prev.expr),
                        to: show(&expr),
                        span: c.span,
                    });
                    let i = t.index[name];
                    t.entries[i] = Entry {
                        name: name.to_owned(),
                        text: show(&expr),
                        expr,
                        dim: c.dim,
                        origin: Origin::Override,
                        span: Some(c.span),
                    };
                }
                (ConstKind::Override, false) => {}
                (ConstKind::Let, _) => {
                    let e = Entry {
                        name: name.to_owned(),
                        text: show(&expr),
                        expr,
                        dim: c.dim,
                        origin: Origin::Tile,
                        span: Some(c.span),
                    };
                    match t.index.get(name) {
                        // a plain name of an outer scope the decoder let by
                        // (a definition's own, called from a scope with one)
                        Some(&i) => t.entries[i] = e,
                        None => {
                            t.index.insert(name.to_owned(), t.entries.len());
                            t.entries.push(e);
                        }
                    }
                }
            }
        }
        (t, errors)
    }

    /// Whether a formula can be read here: every `$` name it reaches, through
    /// the names it reads in turn, is in the context, and a position or a
    /// height under (`$x $y $bottom`) is only read where `per_point` says a
    /// height is worked out at each point.
    ///
    /// # Errors
    /// The first name that cannot be read.
    pub fn check_reads(&self, expr: &Expr, per_point: bool) -> Result<(), ReadFlaw> {
        self.reads_at(expr, per_point.then_some(None), depth0())
    }

    /// [`ConstTable::check_reads`] for a formula an element calls at each
    /// point: it may read only the `$` names that element sets
    /// ([`crate::words::sets`]).
    ///
    /// # Errors
    /// The first name that cannot be read.
    pub fn check_reads_for(&self, expr: &Expr, element: &str) -> Result<(), ReadFlaw> {
        self.reads_at(expr, Some(Some(element)), depth0())
    }

    /// `per`: `None` where a value is drawn once; `Some(None)` where a height
    /// is worked out at each point by no one in particular; `Some(Some(el))`
    /// by `el`, which sets what [`crate::words::sets`] says.
    fn reads_at(
        &self,
        expr: &Expr,
        per: Option<Option<&str>>,
        depth: usize,
    ) -> Result<(), ReadFlaw> {
        if depth > 64 {
            return Err(ReadFlaw::Loop);
        }
        let not_set = |name: &str, el: &str| ReadFlaw::NotSet {
            name: name.to_owned(),
            element: el.to_owned(),
            sets: crate::words::sets(el)
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        };
        // the coordinates `x(…)`, `y(…)`, `d(…)`, `a(…)` are worked out at a
        // point too
        let mut coord: Option<String> = None;
        expr.visit_calls(&mut |n| {
            if matches!(n, "x" | "y" | "d" | "a") {
                coord.get_or_insert_with(|| n.to_owned());
            }
        });
        if let Some(c) = coord.as_deref() {
            match per {
                None => {
                    return Err(ReadFlaw::PerPoint {
                        name: format!("{c}(…)"),
                    });
                }
                Some(Some(el)) if !crate::words::sets(el).contains(&"$x") => {
                    return Err(not_set(&format!("{c}(…)"), el));
                }
                Some(_) => {}
            }
        }
        for n in expr.names() {
            if crate::words::REGION_NAMES.contains(&n) {
                // a region is the context's own: always there
            } else if crate::words::POINT_NAMES.contains(&n) || n == "$_y" {
                match per {
                    None => return Err(ReadFlaw::PerPoint { name: n.to_owned() }),
                    Some(Some(el)) if !crate::words::sets(el).contains(&n) => {
                        return Err(not_set(n, el));
                    }
                    Some(_) => {}
                }
            } else if let Some(e) = self.get(n) {
                self.reads_at(&e.expr, per, depth + 1)?;
            } else if n.starts_with('$') {
                let names: Vec<&str> = self
                    .entries
                    .iter()
                    .map(|e| e.name.as_str())
                    .filter(|k| k.starts_with('$'))
                    .collect();
                return Err(ReadFlaw::Unknown {
                    name: n.to_owned(),
                    near: crate::schema::did_you_mean(n, names.iter().copied()).map(str::to_owned),
                });
            }
        }
        Ok(())
    }

    /// Whether every `parent(…)` in `expr`, reached through the names it reads,
    /// has a parent where it is called: `parents` is how many regions lie
    /// above the thing's own `$region` before the tile (0 for a thing whose
    /// region is the tile).
    #[must_use]
    pub fn parent_flaw(&self, expr: &Expr, parents: usize) -> Option<ParentFlaw> {
        self.parent_flaw_at(expr, parents, 0)
    }

    fn parent_flaw_at(&self, e: &Expr, parents: usize, depth: usize) -> Option<ParentFlaw> {
        if depth > 64 {
            return None;
        }
        let again = |x: &Expr| self.parent_flaw_at(x, parents, depth);
        match e {
            Expr::Num { .. } | Expr::Unit(_) => None,
            Expr::Name(n) => self
                .get(n)
                .and_then(|c| self.parent_flaw_at(&c.expr, parents, depth + 1)),
            Expr::Neg(a) => again(a),
            Expr::Bin { lhs, rhs, .. } => again(lhs).or_else(|| again(rhs)),
            Expr::Range { lo, hi } => again(lo).or_else(|| again(hi)),
            Expr::Weighted { range, .. } => again(range),
            Expr::List { items } => items.iter().find_map(|(v, _)| again(v)),
            Expr::Call { name, args } => {
                if name == "parent"
                    && let Some((base, hops)) = self.region_chain(e, depth)
                {
                    match base {
                        RegionBase::Tile => return Some(ParentFlaw::Tile),
                        RegionBase::Own if hops > parents => {
                            return Some(ParentFlaw::Past { hops, parents });
                        }
                        RegionBase::Own => {}
                    }
                }
                args.iter().find_map(again)
            }
        }
    }

    /// A region value: where it starts and how many `parent`s are taken.
    fn region_chain(&self, e: &Expr, depth: usize) -> Option<(RegionBase, usize)> {
        if depth > 64 {
            return None;
        }
        match e {
            Expr::Name(n) if n == "$region" => Some((RegionBase::Own, 0)),
            Expr::Name(n) if n == "$tile" => Some((RegionBase::Tile, 0)),
            Expr::Name(n) => self
                .get(n)
                .and_then(|c| self.region_chain(&c.expr, depth + 1)),
            Expr::Call { name, args } if name == "parent" && args.len() == 1 => {
                self.region_chain(&args[0], depth).map(|(b, h)| (b, h + 1))
            }
            _ => None,
        }
    }

    /// A constant by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.index.get(name).map(|&i| &self.entries[i])
    }

    /// Every constant, in the order declared.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// What the tile overrides, in order.
    #[must_use]
    pub fn overridden(&self) -> &[Overridden] {
        &self.overridden
    }

    /// True when `expr` holds a range, or names a constant that does: a
    /// distribution, drawn where it is used.
    #[must_use]
    pub fn draws(&self, expr: &Expr) -> bool {
        self.draws_at(expr, 0)
    }

    fn draws_at(&self, expr: &Expr, depth: usize) -> bool {
        depth <= 64
            && (expr.range_count() > 0
                || expr.names().iter().any(|n| {
                    self.get(n)
                        .is_some_and(|e| self.draws_at(&e.expr, depth + 1))
                }))
    }

    /// The first weighted range, other than the plain range, that `expr`
    /// holds or reaches through a constant, as written: what a band refuses.
    #[must_use]
    pub fn weighted_in(&self, expr: &Expr) -> Option<String> {
        self.weighted_at(expr, 0)
    }

    fn weighted_at(&self, expr: &Expr, depth: usize) -> Option<String> {
        if depth > 64 {
            return None;
        }
        if let Some(w) = expr.weighted() {
            return Some(w.to_string());
        }
        expr.names().iter().find_map(|n| {
            self.get(n)
                .and_then(|e| self.weighted_at(&e.expr, depth + 1))
        })
    }

    /// `expr` as one number, a range drawn at `u` (0 low, 1 high); names
    /// are constants, or `vars` (`x`, `y`, `bottom`).
    ///
    /// # Errors
    /// An unknown name, or constants that refer to each other round a loop.
    pub fn scalar(
        &self,
        expr: &Expr,
        u: f64,
        vars: &dyn Fn(&str) -> Option<f64>,
    ) -> Result<f64, String> {
        self.scalar_at(expr, u, vars, 0)
    }

    fn scalar_at(
        &self,
        expr: &Expr,
        u: f64,
        vars: &dyn Fn(&str) -> Option<f64>,
        depth: usize,
    ) -> Result<f64, String> {
        if depth > 64 {
            return Err("constants refer to each other in a loop".to_owned());
        }
        let env = |n: &str| -> Option<f64> {
            vars(n).or_else(|| {
                let e = self.get(n)?;
                self.scalar_at(&e.expr, u, vars, depth + 1).ok()
            })
        };
        expr.sample(&env, u).map_err(|m| {
            if depth > 0 {
                m
            } else {
                format!("{m}, or constants that refer to each other in a loop")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Known, decode};

    const HEIGHTS: &str = include_str!("../../../library/parts/heights.dtile");

    fn standard() -> Known {
        let mut k = Known::new();
        k.add_heights("library/parts/heights.dtile", HEIGHTS)
            .unwrap();
        k
    }

    fn tile_consts(text: &str) -> Vec<Const> {
        let doc = decode("t.dtile", text, &standard()).unwrap();
        doc.tile().unwrap().consts.clone()
    }

    #[test]
    fn an_override_in_a_scope_replaces_the_library_height_and_is_recorded() {
        let k = standard();
        let root = ConstTable::root(k.consts());
        let mine = tile_consts(
            "dharmatiles version=1\ntile size=\"1x1\" { let pad \"2dmm\"\n override $groundHeight \"3dmm\"\n stack { dirt } }",
        );
        let (t, errors) = root.enter(&mine);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(t.overridden().len(), 1);
        assert_eq!(t.get("pad").unwrap().origin, Origin::Tile);
        assert_eq!(t.get("$groundHeight").unwrap().origin, Origin::Override);
        assert_eq!(root.get("$groundHeight").unwrap().origin, Origin::Library);
    }
}
