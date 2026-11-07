//! The small expression language used inside quoted values.
//!
//! ```text
//! value  := sum ( ".." sum )?        a range "lo..hi", drawn afresh per thing
//! sum    := prod ( ("+" | "-") prod )*
//! prod   := unary ( ("*" | "/") unary )*
//! unary  := "-" unary | atom
//! atom   := number "%"? | name | "(" sum ")"
//! ```
//!
//! Units are never written: every control knows its own unit, and
//! sizes are millimetres at the DungeonBlocks reference.  `%` is the one
//! suffix, because a share is a percent (`cover="85%"`).  An old
//! `"4.5mm"` suffix and the `"1..6^2"` skew are gone; both give an error
//! that says what to write instead (`4.5`, and `mostly=small`).
//!
//! A name is a parameter of the enclosing definition or, where a level is
//! expected, a named level (`ground`, `wall-top`, a tile's own `levels`).
//! There are no calls, loops or assignments.

use std::fmt;

/// A type annotation written before a plain value: `(mm)4.5` or `(h)1.2`.
///
/// `(mm)` is only a check: the value is millimetres either way.  `(h)`
/// is the opt-in wall-relative scale, where 1 is the 33.1 mm wall top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// `(mm)`: millimetres at the DungeonBlocks reference (a check only).
    Mm,
    /// `(h)`: fractions of the 33.1 mm wall top.
    H,
}

impl Unit {
    /// Parse an annotation word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mm" => Some(Self::Mm),
            "h" => Some(Self::H),
            _ => None,
        }
    }

    /// The annotation as written, without parentheses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::H => "h",
        }
    }
}

/// Arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
}

/// A parsed value expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal number.
    Num {
        /// The number as written (a percent not yet divided).
        value: f64,
        /// Written with `%`: the value is divided by 100 when sampled.
        percent: bool,
    },
    /// A parameter or a named level.
    Name(String),
    /// Unary minus.
    Neg(Box<Expr>),
    /// Binary arithmetic.
    Bin {
        /// The operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
    },
    /// A range `lo..hi`, drawn afresh for each thing.  A scatter's or a
    /// thing's `mostly=` skews the draw; the range itself is uniform.
    Range {
        /// Low end.
        lo: Box<Expr>,
        /// High end.
        hi: Box<Expr>,
    },
}

/// A parse failure, with the byte range inside the expression text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    /// What went wrong.
    pub message: String,
    /// Start byte within the expression text.
    pub start: usize,
    /// End byte within the expression text.
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64, bool),
    Name(String),
    Op(char),
    DotDot,
    LParen,
    RParen,
}

fn lex(src: &str) -> Result<Vec<(Tok, usize, usize)>, ExprError> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            // A '.' is a decimal point only when a digit follows: "8..14".
            if i < b.len() && b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text = &src[start..i];
            let value: f64 = text.parse().map_err(|_| ExprError {
                message: format!("`{text}` is not a number"),
                start,
                end: i,
            })?;
            let percent = b.get(i) == Some(&b'%');
            if percent {
                i += 1;
            } else if b.get(i).is_some_and(u8::is_ascii_alphabetic) {
                // A unit glued on: "4.5mm".  Units are not written.
                let unit_start = i;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let unit = &src[unit_start..i];
                return Err(ExprError {
                    message: format!(
                        "units are not written: `{text}{unit}` is just `{text}`; every control knows its unit (sizes are mm)"
                    ),
                    start: unit_start,
                    end: i,
                });
            }
            out.push((Tok::Num(value, percent), start, i));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            // A trailing '-' belongs to arithmetic, not the name.
            while src[start..i].ends_with('-') {
                i -= 1;
            }
            out.push((Tok::Name(src[start..i].to_owned()), start, i));
            continue;
        }
        let tok = match c {
            b'.' if b.get(i + 1) == Some(&b'.') => {
                i += 2;
                Tok::DotDot
            }
            b'+' | b'-' | b'*' | b'/' | b'^' => {
                i += 1;
                Tok::Op(c as char)
            }
            b'(' => {
                i += 1;
                Tok::LParen
            }
            b')' => {
                i += 1;
                Tok::RParen
            }
            _ => {
                let ch = src[i..].chars().next().unwrap_or('?');
                return Err(ExprError {
                    message: format!("unexpected `{ch}`"),
                    start,
                    end: start + ch.len_utf8(),
                });
            }
        };
        out.push((tok, start, i));
    }
    Ok(out)
}

struct Parser<'a> {
    toks: &'a [(Tok, usize, usize)],
    pos: usize,
    len: usize,
}

const SKEW_HELP: &str =
    "a range is skewed with `mostly=small`, `even` or `large` beside it, not with `^`";

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn here(&self) -> (usize, usize) {
        self.toks
            .get(self.pos)
            .map_or((self.len, self.len), |t| (t.1, t.2))
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, ExprError> {
        let (start, end) = self.here();
        Err(ExprError {
            message: message.into(),
            start,
            end,
        })
    }

    fn value(&mut self) -> Result<Expr, ExprError> {
        let lo = self.sum()?;
        if self.peek() != Some(&Tok::DotDot) {
            return Ok(lo);
        }
        self.pos += 1;
        let hi = self.sum()?;
        Ok(Expr::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
        })
    }

    fn sum(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.prod()?;
        while let Some(Tok::Op(c @ ('+' | '-'))) = self.peek() {
            let op = if *c == '+' { BinOp::Add } else { BinOp::Sub };
            self.pos += 1;
            let rhs = self.prod()?;
            lhs = Expr::Bin {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn prod(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(c @ ('*' | '/'))) = self.peek() {
            let op = if *c == '*' { BinOp::Mul } else { BinOp::Div };
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::Bin {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.peek() == Some(&Tok::Op('-')) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, ExprError> {
        match self.peek().cloned() {
            Some(Tok::Num(value, percent)) => {
                self.pos += 1;
                Ok(Expr::Num { value, percent })
            }
            Some(Tok::Name(n)) => {
                self.pos += 1;
                Ok(Expr::Name(n))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let e = self.sum()?;
                if self.peek() != Some(&Tok::RParen) {
                    return self.err("missing `)`");
                }
                self.pos += 1;
                Ok(e)
            }
            Some(Tok::DotDot) => self.err("a range needs a low end, as in \"8..14\""),
            Some(Tok::Op('^')) => self.err(SKEW_HELP),
            Some(_) => self.err("expected a number or a name"),
            None => self.err("expected a value"),
        }
    }
}

/// Parse an expression string.
///
/// # Errors
/// Returns the first problem with its byte range inside `src`.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks: &toks,
        pos: 0,
        len: src.len(),
    };
    let e = p.value()?;
    if p.pos < toks.len() {
        return match p.peek() {
            Some(Tok::DotDot) => p.err("only one `..` is allowed: a range has two ends"),
            Some(Tok::Op('^')) => p.err(SKEW_HELP),
            _ => p.err("unexpected text after the value"),
        };
    }
    Ok(e)
}

impl Expr {
    /// A plain number, as a literal.
    #[must_use]
    pub fn num(value: f64) -> Self {
        Self::Num {
            value,
            percent: false,
        }
    }

    /// Every name the expression refers to.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.visit(&mut |e| {
            if let Expr::Name(n) = e {
                out.push(n.as_str());
            }
        });
        out
    }

    /// True when any literal is written with `%`.
    #[must_use]
    pub fn has_percent(&self) -> bool {
        let mut any = false;
        self.visit(&mut |e| any |= matches!(e, Expr::Num { percent: true, .. }));
        any
    }

    /// True when every literal is written with `%` (and there is one).
    #[must_use]
    pub fn all_percent(&self) -> bool {
        let (mut all, mut seen) = (true, false);
        self.visit(&mut |e| {
            if let Expr::Num { percent, .. } = e {
                seen = true;
                all &= *percent;
            }
        });
        all && seen
    }

    /// True when the expression is (or contains) a range.
    #[must_use]
    pub fn is_range(&self) -> bool {
        let mut any = false;
        self.visit(&mut |e| any |= matches!(e, Expr::Range { .. }));
        any
    }

    fn visit<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::Num { .. } | Expr::Name(_) => {}
            Expr::Neg(e) => e.visit(f),
            Expr::Bin { lhs, rhs, .. } | Expr::Range { lo: lhs, hi: rhs } => {
                lhs.visit(f);
                rhs.visit(f);
            }
        }
    }

    /// Evaluate with `u` as the uniform draw for every range (0 gives the
    /// low end, 1 the high end).  A percent is divided by 100.  Names
    /// resolve through `env`.  Only `+ - * /` are used, so the result is
    /// bit-identical on every platform.
    ///
    /// # Errors
    /// Returns the first name `env` does not know.
    pub fn sample(&self, env: &dyn Fn(&str) -> Option<f64>, u: f64) -> Result<f64, String> {
        Ok(match self {
            Expr::Num { value, percent } => {
                if *percent {
                    value / 100.0
                } else {
                    *value
                }
            }
            Expr::Name(n) => env(n).ok_or_else(|| format!("unknown name `{n}`"))?,
            Expr::Neg(e) => -e.sample(env, u)?,
            Expr::Bin { op, lhs, rhs } => {
                let (a, b) = (lhs.sample(env, u)?, rhs.sample(env, u)?);
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                }
            }
            Expr::Range { lo, hi } => {
                let (a, b) = (lo.sample(env, u)?, hi.sample(env, u)?);
                a + (b - a) * u
            }
        })
    }

    /// The low and high ends (`sample` at 0 and 1).
    ///
    /// # Errors
    /// As [`Expr::sample`].
    pub fn bounds(&self, env: &dyn Fn(&str) -> Option<f64>) -> Result<(f64, f64), String> {
        Ok((self.sample(env, 0.0)?, self.sample(env, 1.0)?))
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Num { value, percent } => {
                write!(f, "{value}")?;
                if *percent {
                    f.write_str("%")?;
                }
                Ok(())
            }
            Expr::Name(n) => write!(f, "{n}"),
            Expr::Neg(e) => write!(f, "-{e}"),
            Expr::Bin { op, lhs, rhs } => {
                let c = match op {
                    BinOp::Add => '+',
                    BinOp::Sub => '-',
                    BinOp::Mul => '*',
                    BinOp::Div => '/',
                };
                write!(f, "({lhs} {c} {rhs})")
            }
            Expr::Range { lo, hi } => write!(f, "{lo}..{hi}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<f64> {
        None
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn plain_number() {
        assert_eq!(parse("4.5").unwrap(), Expr::num(4.5));
    }

    #[test]
    fn units_are_not_written() {
        let err = parse("4.5mm").unwrap_err();
        assert_eq!((err.start, err.end), (3, 5));
        assert!(err.message.contains("units are not written"), "{err:?}");
        assert!(err.message.contains("just `4.5`"), "{err:?}");
        assert!(parse("1..6deg").is_err());
    }

    #[test]
    fn percent_divides() {
        let e = parse("20%").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 0.2));
        assert!(e.all_percent());
        assert!(!parse("20").unwrap().has_percent());
    }

    #[test]
    fn range_is_not_a_decimal() {
        let e = parse("8..14").unwrap();
        assert!(e.is_range());
        assert_eq!(e.bounds(&none).unwrap(), (8.0, 14.0));
        assert!(close(e.sample(&none, 0.5).unwrap(), 11.0));
    }

    #[test]
    fn skew_points_at_mostly() {
        let err = parse("1..6^2").unwrap_err();
        assert!(err.message.contains("mostly=small"), "{err:?}");
        assert_eq!((err.start, err.end), (4, 5));
    }

    #[test]
    fn decimal_and_percent_ranges() {
        let e = parse("0.2..0.4").unwrap();
        assert!(close(e.sample(&none, 1.0).unwrap(), 0.4));
        let e = parse("20%..50%").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 0.2));
        assert!(e.all_percent());
    }

    #[test]
    fn arithmetic_and_precedence() {
        let e = parse("1 + 2 * 3 - -1").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 8.0));
        let e = parse("(1 + 2) * 3 / 2").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 4.5));
    }

    #[test]
    fn names_resolve_through_env() {
        let env = |n: &str| match n {
            "ground" => Some(5.0),
            "rocks" => Some(3.0),
            _ => None,
        };
        let e = parse("ground - 1").unwrap();
        assert!(close(e.sample(&env, 0.0).unwrap(), 4.0));
        let e = parse("rocks..rocks*2").unwrap();
        assert!(close(e.sample(&env, 1.0).unwrap(), 6.0));
        assert_eq!(e.names(), vec!["rocks", "rocks"]);
        assert!(parse("missing").unwrap().sample(&env, 0.0).is_err());
    }

    #[test]
    fn dashed_names() {
        let e = parse("wall-top - 1").unwrap();
        assert_eq!(e.names(), vec!["wall-top"]);
    }

    #[test]
    fn errors_point_inside_the_text() {
        let err = parse("8..14..20").unwrap_err();
        assert_eq!((err.start, err.end), (5, 7));
        assert!(err.message.contains("only one"));

        let err = parse("(1 + 2").unwrap_err();
        assert!(err.message.contains(')'));

        let err = parse("2 ^ 3").unwrap_err();
        assert!(err.message.contains("mostly"));

        let err = parse("").unwrap_err();
        assert!(err.message.contains("expected a value"));
    }

    #[test]
    fn display_round_trips_meaning() {
        for s in ["1..6", "20%..50%", "ground - 1"] {
            let e = parse(s).unwrap();
            assert_eq!(parse(&e.to_string()).unwrap(), e, "{s}");
        }
    }
}
