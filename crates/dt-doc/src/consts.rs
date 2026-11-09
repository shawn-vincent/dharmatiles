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

/// An expression as text, for notes.
fn show(e: &Expr) -> String {
    e.to_string()
}

impl ConstTable {
    /// The library's constants, then the tile's `let`s, then its `override`s
    /// (replacing in place; an override reads the table as it stands).
    #[must_use]
    pub fn build(library: &[Const], tile: &[Const]) -> Self {
        let mut t = Self::default();
        for c in library {
            t.push(c, Origin::Library, None);
        }
        for c in tile {
            match c.kind {
                ConstKind::Let => t.push(c, Origin::Tile, Some(c.span)),
                ConstKind::Override => {
                    if let Some(&i) = t.index.get(&c.name.value) {
                        t.overridden.push(Overridden {
                            name: c.name.value.clone(),
                            from: show(&t.entries[i].expr),
                            to: show(&c.value.value),
                            span: c.span,
                        });
                        t.entries[i] = Entry {
                            name: c.name.value.clone(),
                            expr: c.value.value.clone(),
                            dim: c.dim,
                            origin: Origin::Override,
                            text: show(&c.value.value),
                            span: Some(c.span),
                        };
                    }
                }
            }
        }
        t
    }

    fn push(&mut self, c: &Const, origin: Origin, span: Option<SourceSpan>) {
        self.index.insert(c.name.value.clone(), self.entries.len());
        self.entries.push(Entry {
            name: c.name.value.clone(),
            expr: c.value.value.clone(),
            dim: c.dim,
            origin,
            text: show(&c.value.value),
            span,
        });
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
    fn override_moves_what_reads_it() {
        let k = standard();
        let mine =
            tile_consts("tile { let a 2\n override waterSurfaceHeight 3.2\n stack { dirt } }");
        let t = ConstTable::build(k.consts(), &mine);
        let vars = |n: &str| match n {
            "x" | "y" => Some(0.0),
            _ => None,
        };
        let e = &t.get("waterEdgeHeight").unwrap().expr;
        // 3.2 + 0.6 * cos(0)
        assert!((t.scalar(e, 0.0, &vars).unwrap() - 3.8).abs() < 1e-12);
        assert_eq!(t.overridden().len(), 1);
        assert_eq!(t.overridden()[0].from, "2.6");
        assert_eq!(t.get("a").unwrap().origin, Origin::Tile);
        assert_eq!(t.get("groundHeight").unwrap().origin, Origin::Library);
        assert_eq!(
            t.get("waterSurfaceHeight").unwrap().origin,
            Origin::Override
        );
    }
}
