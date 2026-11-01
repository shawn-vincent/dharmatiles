//! The small expression language used for parameter values.
//!
//! ```text
//! value  := sum ( ".." sum ( "^" number )? )?     a distribution "lo..hi^skew"
//! sum    := prod ( ("+" | "-") prod )*
//! prod   := unary ( ("*" | "/") unary )*
//! unary  := "-" unary | atom
//! atom   := number unit? | name | "(" sum ")"
//! unit   := "mm" | "sq" | "h" | "deg" | "%"
//! ```
//!
//! A name is a parameter of the enclosing definition or a named height
//! (`ground`, `wall`, a tile's own `heights`).  There are no calls, loops
//! or assignments.  A distribution draws `lo + (hi - lo) * u^skew` for a
//! uniform `u` in 0..1, so a skew above 1 favours the low end, the way
//! rockpile's `SCREE_D_EXP` does.

use std::fmt;

/// A unit written after a number (`4.5mm`) or as a KDL type annotation
/// (`(mm)4.5`).  Usually omitted: the control's axis supplies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// True millimetres: print-scale details.
    Mm,
    /// Squares: layout across.
    Sq,
    /// Fractions of the wall height: layout up.
    H,
    /// Degrees.
    Deg,
    /// Percent; the value is divided by 100.
    Percent,
}

impl Unit {
    /// Parse a unit word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "mm" => Self::Mm,
            "sq" => Self::Sq,
            "h" => Self::H,
            "deg" => Self::Deg,
            "%" => Self::Percent,
            _ => return None,
        })
    }

    /// The unit as written.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Sq => "sq",
            Self::H => "h",
            Self::Deg => "deg",
            Self::Percent => "%",
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
    /// A literal number, with the unit if one was written.
    Num {
        /// The number as written (percent not yet divided).
        value: f64,
        /// The written unit.
        unit: Option<Unit>,
    },
    /// A parameter or a named height.
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
    /// A distribution `lo..hi^skew`.
    Range {
        /// Low end.
        lo: Box<Expr>,
        /// High end.
        hi: Box<Expr>,
        /// Power applied to the uniform draw; `None` is uniform.
        skew: Option<f64>,
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
    Num(f64, Option<Unit>),
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
            let value: f64 = src[start..i].parse().map_err(|_| ExprError {
                message: format!("`{}` is not a number", &src[start..i]),
                start,
                end: i,
            })?;
            // An optional unit glued to the number.
            let unit_start = i;
            if i < b.len() && b[i] == b'%' {
                i += 1;
            } else {
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
            }
            let unit = if i > unit_start {
                let word = &src[unit_start..i];
                Some(Unit::parse(word).ok_or_else(|| ExprError {
                    message: format!("unknown unit `{word}`; units are mm, sq, h, deg and %"),
                    start: unit_start,
                    end: i,
                })?)
            } else {
                None
            };
            out.push((Tok::Num(value, unit), start, i));
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
        let mut skew = None;
        if self.peek() == Some(&Tok::Op('^')) {
            self.pos += 1;
            match self.peek() {
                Some(&Tok::Num(k, None)) if k > 0.0 => {
                    self.pos += 1;
                    skew = Some(k);
                }
                _ => {
                    return self.err("a skew `^k` needs a plain positive number, as in \"1..6^2\"");
                }
            }
        }
        Ok(Expr::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            skew,
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
            Some(Tok::Num(value, unit)) => {
                self.pos += 1;
                Ok(Expr::Num { value, unit })
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
            Some(Tok::Op('^')) => p.err("`^` only skews a range, as in \"1..6^2\""),
            _ => p.err("unexpected text after the value"),
        };
    }
    Ok(e)
}

impl Expr {
    /// Every unit written anywhere in the expression.
    #[must_use]
    pub fn units(&self) -> Vec<Unit> {
        let mut out = Vec::new();
        self.visit(&mut |e| {
            if let Expr::Num { unit: Some(u), .. } = e {
                out.push(*u);
            }
        });
        out
    }

    /// Every name the expression refers to.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.visit_ref(&mut out);
        out
    }

    fn visit_ref<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Expr::Name(n) => out.push(n),
            Expr::Num { .. } => {}
            Expr::Neg(e) => e.visit_ref(out),
            Expr::Bin { lhs, rhs, .. } => {
                lhs.visit_ref(out);
                rhs.visit_ref(out);
            }
            Expr::Range { lo, hi, .. } => {
                lo.visit_ref(out);
                hi.visit_ref(out);
            }
        }
    }

    fn visit(&self, f: &mut impl FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Num { .. } | Expr::Name(_) => {}
            Expr::Neg(e) => e.visit(f),
            Expr::Bin { lhs, rhs, .. } => {
                lhs.visit(f);
                rhs.visit(f);
            }
            Expr::Range { lo, hi, .. } => {
                lo.visit(f);
                hi.visit(f);
            }
        }
    }

    /// Evaluate with `u` as the uniform draw for every range (0 gives the
    /// low end, 1 the high end).  Names resolve through `env`.
    ///
    /// # Errors
    /// Returns the first name `env` does not know.
    pub fn sample(&self, env: &dyn Fn(&str) -> Option<f64>, u: f64) -> Result<f64, String> {
        Ok(match self {
            Expr::Num { value, unit } => {
                if *unit == Some(Unit::Percent) {
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
            Expr::Range { lo, hi, skew } => {
                let (a, b) = (lo.sample(env, u)?, hi.sample(env, u)?);
                a + (b - a) * u.powf(skew.unwrap_or(1.0))
            }
        })
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Num { value, unit } => {
                write!(f, "{value}")?;
                if let Some(u) = unit {
                    write!(f, "{}", u.as_str())?;
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
            Expr::Range { lo, hi, skew } => {
                write!(f, "{lo}..{hi}")?;
                if let Some(k) = skew {
                    write!(f, "^{k}")?;
                }
                Ok(())
            }
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
        assert_eq!(
            parse("4.5").unwrap(),
            Expr::Num {
                value: 4.5,
                unit: None
            }
        );
    }

    #[test]
    fn number_with_unit() {
        let e = parse("4.5mm").unwrap();
        assert_eq!(e.units(), vec![Unit::Mm]);
        assert!(close(e.sample(&none, 0.0).unwrap(), 4.5));
    }

    #[test]
    fn percent_divides() {
        let e = parse("20%").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 0.2));
    }

    #[test]
    fn range_is_not_a_decimal() {
        let e = parse("8..14").unwrap();
        assert!(matches!(e, Expr::Range { skew: None, .. }));
        assert!(close(e.sample(&none, 0.0).unwrap(), 8.0));
        assert!(close(e.sample(&none, 1.0).unwrap(), 14.0));
        assert!(close(e.sample(&none, 0.5).unwrap(), 11.0));
    }

    #[test]
    fn skewed_range_favours_low_end() {
        // rockpile: D = lerp(1..6, u ** 2)
        let e = parse("1..6^2").unwrap();
        assert!(close(e.sample(&none, 0.5).unwrap(), 1.0 + 5.0 * 0.25));
    }

    #[test]
    fn decimal_ranges() {
        let e = parse("0.2..0.4^1.4").unwrap();
        assert!(close(e.sample(&none, 1.0).unwrap(), 0.4));
        let e = parse("20%..50%").unwrap();
        assert!(close(e.sample(&none, 0.0).unwrap(), 0.2));
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
            "ground" => Some(0.151),
            "rocks" => Some(3.0),
            _ => None,
        };
        let e = parse("ground + 0.02").unwrap();
        assert!(close(e.sample(&env, 0.0).unwrap(), 0.171));
        let e = parse("rocks..rocks*2").unwrap();
        assert!(close(e.sample(&env, 1.0).unwrap(), 6.0));
        assert_eq!(e.names(), vec!["rocks", "rocks"]);
        assert!(parse("missing").unwrap().sample(&env, 0.0).is_err());
    }

    #[test]
    fn dashed_names() {
        let e = parse("wall-top - 0.1").unwrap();
        assert_eq!(e.names(), vec!["wall-top"]);
    }

    #[test]
    fn errors_point_inside_the_text() {
        let err = parse("8..14..20").unwrap_err();
        assert_eq!((err.start, err.end), (5, 7));
        assert!(err.message.contains("only one"));

        let err = parse("4.5cm").unwrap_err();
        assert_eq!((err.start, err.end), (3, 5));
        assert!(err.message.contains("unknown unit"));

        let err = parse("1..6^x").unwrap_err();
        assert!(err.message.contains("skew"));

        let err = parse("(1 + 2").unwrap_err();
        assert!(err.message.contains(")"));

        let err = parse("2 ^ 3").unwrap_err();
        assert!(err.message.contains("only skews"));

        let err = parse("").unwrap_err();
        assert!(err.message.contains("expected a value"));
    }

    #[test]
    fn display_round_trips_meaning() {
        let e = parse("1..6^2").unwrap();
        assert_eq!(e.to_string(), "1..6^2");
        assert_eq!(parse(&e.to_string()).unwrap(), e);
    }
}
