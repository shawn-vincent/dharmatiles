//! The small expression language used inside quoted values.
//!
//! ```text
//! value  := sum ( ".." sum )?        a range "lo..hi", drawn afresh per thing
//! sum    := prod ( ("+" | "-") prod )*
//! prod   := unary ( ("*" | "/") unary )*
//! unary  := "-" unary | atom
//! atom   := number "%"? | name | "(" sum ")" | weighting
//! weighting := ("flat" | "low" | "high" | "bell" | "peak")
//!              "(" range ( "," number )* ")"
//! range  := sum ".." sum | "[" sum ".." sum "]"
//! ```
//!
//! Units are never written: every control knows its own unit, and
//! sizes are millimetres at the DungeonBlocks reference.  `%` is the one
//! suffix, because a share is a percent (`cover="85%"`).  An old
//! `"4.5mm"` suffix and the `"1..6^2"` skew are gone; both give an error
//! that says what to write instead (`4.5`, and `size="low(1..6, 4)"`).
//!
//! A name is a parameter of the enclosing definition or, where a level is
//! expected, a named level (`ground`, `wall-top`, a tile's own `levels`).
//! A weighting call (`low(1..6, 4)`) weights a range; it is read in both
//! dialects.  Nothing else is a call in a quoted control value.

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
    /// A range `lo..hi`, drawn afresh for each thing; every range in a value
    /// is its own draw.  The range itself is uniform: a [`Expr::Weighted`]
    /// call around it says otherwise.
    Range {
        /// Low end.
        lo: Box<Expr>,
        /// High end.
        hi: Box<Expr>,
    },
    /// A weighted range, `low(1..6, 4)`: the range and the odds along it
    /// (`name` is one of [`WEIGHTINGS`], `args` the numbers after the
    /// range).  The odds are `dt_geom::Weighting`'s; they are checked where
    /// the value is drawn.
    Weighted {
        /// The function: `flat`, `low`, `high`, `bell` or `peak`.
        name: String,
        /// The numbers after the range.
        args: Vec<f64>,
        /// The plain range it weights.
        range: Box<Expr>,
    },
    /// A function call, `cos(720 * x)`: `sin`, `cos` (degrees), `abs`,
    /// `min`, `max`, `clamp` and `smooth`.  Only in [`parse_v2`] values.
    Call {
        /// The function.
        name: String,
        /// Its arguments.
        args: Vec<Expr>,
    },
}

/// One range as it is about to be drawn: see [`Expr::draw`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw<'a> {
    /// The range's position among the value's ranges (0 for the first to
    /// finish evaluating).
    pub ordinal: usize,
    /// Its low end, evaluated.
    pub lo: f64,
    /// Its high end, evaluated.
    pub hi: f64,
    /// The weighting written round it: the function and its numbers.
    pub weighting: Option<(&'a str, &'a [f64])>,
}

/// A parse failure, with the byte range inside the expression text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    /// What went wrong.
    pub message: String,
    /// The diagnostic code when it is not the reader's own (`dt::weighting`,
    /// `dt::value`).
    pub code: Option<&'static str>,
    /// What to write instead, when the parser knows.
    pub help: Option<String>,
    /// Start byte within the expression text.
    pub start: usize,
    /// End byte within the expression text.
    pub end: usize,
}

impl ExprError {
    /// A problem with its own code and help.
    fn coded(
        code: &'static str,
        message: String,
        help: String,
        (start, end): (usize, usize),
    ) -> Self {
        Self {
            message,
            code: Some(code),
            help: Some(help),
            start,
            end,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64, bool),
    Name(String),
    Op(char),
    DotDot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

fn lex(src: &str, v2: bool) -> Result<Vec<(Tok, usize, usize)>, ExprError> {
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
                code: None,
                help: None,
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
                    code: None,
                    help: None,
                    start: unit_start,
                    end: i,
                });
            }
            out.push((Tok::Num(value, percent), start, i));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            // Names written in camelCase (v2) never hold a hyphen: it subtracts.
            while i < b.len()
                && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || (b[i] == b'-' && !v2))
            {
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
            b'[' => {
                i += 1;
                Tok::LBracket
            }
            b']' => {
                i += 1;
                Tok::RBracket
            }
            b',' => {
                i += 1;
                Tok::Comma
            }
            _ => {
                let ch = src[i..].chars().next().unwrap_or('?');
                return Err(ExprError {
                    message: format!("unexpected `{ch}`"),
                    code: None,
                    help: None,
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
    v2: bool,
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
            code: None,
            help: None,
            start,
            end,
        })
    }

    /// The start of the token before the current one to the end of it.
    fn prev_end(&self) -> usize {
        self.toks
            .get(self.pos.wrapping_sub(1))
            .map_or(self.len, |t| t.2)
    }

    fn value(&mut self) -> Result<Expr, ExprError> {
        let lo = self.sum()?;
        if self.peek() != Some(&Tok::DotDot) {
            return Ok(lo);
        }
        if self.v2 {
            return self.err("a range is written in brackets: [low .. high]");
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
                let at = self.here();
                self.pos += 1;
                if self.peek() == Some(&Tok::LParen) {
                    if WEIGHTINGS.iter().any(|(w, _)| *w == n) {
                        return self.weighted(n);
                    }
                    if self.v2 && FUNCTIONS.iter().any(|(f, _)| *f == n) {
                        return self.call(n);
                    }
                    return Err(unknown_function(&n, at, self.v2));
                }
                Ok(Expr::Name(n))
            }
            Some(Tok::LBracket) if self.v2 => self.bracket_range(),
            Some(Tok::LBracket | Tok::RBracket | Tok::Comma) if !self.v2 => {
                let ch = match self.peek() {
                    Some(Tok::LBracket) => '[',
                    Some(Tok::RBracket) => ']',
                    _ => ',',
                };
                self.err(format!("unexpected `{ch}`"))
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

/// An unknown function, with the ones there are.
fn unknown_function(name: &str, (start, end): (usize, usize), v2: bool) -> ExprError {
    let names: Vec<&str> = WEIGHTINGS.iter().map(|(w, _)| *w).collect();
    let weightings = match names.split_last() {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    let help = if v2 {
        let fns = FUNCTIONS
            .iter()
            .map(|(f, _)| *f)
            .collect::<Vec<_>>()
            .join(", ");
        format!("functions are {fns}; weightings are {weightings}")
    } else {
        format!("weightings are {weightings}: size=\"low(1..6, 4)\"")
    };
    ExprError::coded(
        "dt::value",
        format!("unknown function `{name}`"),
        help,
        (start, end),
    )
}

impl Parser<'_> {
    /// `[ low .. high ]`, at the `[`.
    fn bracket_range(&mut self) -> Result<Expr, ExprError> {
        self.pos += 1;
        let lo = self.sum()?;
        if self.peek() != Some(&Tok::DotDot) {
            return self.err("a range has two ends: [low .. high]");
        }
        self.pos += 1;
        let hi = self.sum()?;
        if self.peek() != Some(&Tok::RBracket) {
            return self.err("missing `]`");
        }
        self.pos += 1;
        Ok(Expr::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
        })
    }

    /// A weighting call, `low(1..6, 4)`: the name is read, the `(` is next.
    fn weighted(&mut self, name: String) -> Result<Expr, ExprError> {
        let want = WEIGHTINGS
            .iter()
            .find(|(w, _)| *w == name)
            .map_or(0, |(_, n)| *n);
        let wrap = |span: (usize, usize)| {
            ExprError::coded(
                "dt::weighting",
                "a weighting wraps a plain range".to_owned(),
                format!(
                    "write {name}(1..6): the first thing in the call is a range, not a number or another weighting"
                ),
                span,
            )
        };
        self.pos += 1; // the `(`
        let first = self.here();
        // low(bell(..)): a weighting inside a weighting
        if let (Some(Tok::Name(inner)), Some((Tok::LParen, ..))) =
            (self.peek(), self.toks.get(self.pos + 1))
            && WEIGHTINGS.iter().any(|(w, _)| w == inner)
        {
            return Err(wrap(first));
        }
        let range = if self.peek() == Some(&Tok::LBracket) {
            self.bracket_range()?
        } else {
            let lo = self.sum()?;
            if self.peek() != Some(&Tok::DotDot) {
                return Err(wrap((first.0, self.prev_end())));
            }
            self.pos += 1;
            let hi = self.sum()?;
            Expr::Range {
                lo: Box::new(lo),
                hi: Box::new(hi),
            }
        };
        let mut args = Vec::new();
        while self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
            let from = self.here().0;
            let mut x = self.sum()?;
            if self.peek() == Some(&Tok::DotDot) {
                self.pos += 1;
                let hi = self.sum()?;
                x = Expr::Range {
                    lo: Box::new(x),
                    hi: Box::new(hi),
                };
            }
            let span = (from, self.prev_end());
            if x.is_range() || !x.names().is_empty() {
                return Err(ExprError::coded(
                    "dt::weighting",
                    format!("the numbers after the range in `{name}` are plain numbers"),
                    format!("write a number such as {name}(1..6, 2), not a range or a name"),
                    span,
                ));
            }
            let v = x
                .sample(&|_| None, 0.0)
                .map_err(|m| ExprError::coded("dt::weighting", m, String::new(), span))?;
            if args.len() == want {
                return Err(ExprError::coded(
                    "dt::weighting",
                    format!("`{name}` takes the range and {want} number(s) at most"),
                    format!("remove `{x}`"),
                    span,
                ));
            }
            args.push(v);
        }
        if self.peek() != Some(&Tok::RParen) {
            return self.err("expected `,` or `)`");
        }
        self.pos += 1;
        Ok(Expr::Weighted {
            name,
            args,
            range: Box::new(range),
        })
    }

    /// `name(` arguments `)`: the name is already read.
    fn call(&mut self, name: String) -> Result<Expr, ExprError> {
        self.pos += 1; // the `(`
        let mut args = Vec::new();
        if self.peek() != Some(&Tok::RParen) {
            loop {
                args.push(self.sum()?);
                match self.peek() {
                    Some(Tok::Comma) => self.pos += 1,
                    Some(Tok::RParen) => break,
                    _ => return self.err("expected `,` or `)`"),
                }
            }
        }
        self.pos += 1; // the `)`
        Ok(Expr::Call { name, args })
    }
}

/// Parse an expression string.
///
/// # Errors
/// Returns the first problem with its byte range inside `src`.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    parse_with(src, false)
}

/// Parse a height or length of the heights language: names are camelCase
/// (a hyphen subtracts), a range is `[low .. high]` anywhere, and the
/// functions `sin cos abs min max clamp smooth` are called with
/// parentheses.
///
/// # Errors
/// Returns the first problem with its byte range inside `src`.
pub fn parse_v2(src: &str) -> Result<Expr, ExprError> {
    parse_with(src, true)
}

fn parse_with(src: &str, v2: bool) -> Result<Expr, ExprError> {
    let toks = lex(src, v2)?;
    let mut p = Parser {
        toks: &toks,
        pos: 0,
        len: src.len(),
        v2,
    };
    let e = p.value()?;
    if p.pos < toks.len() {
        return match p.peek() {
            Some(Tok::DotDot) => p.err("only one `..` is allowed: a range has two ends"),
            Some(Tok::Op('^')) => p.err(SKEW_HELP),
            Some(Tok::LBracket) if !v2 => p.err("unexpected `[`"),
            Some(Tok::RBracket) if !v2 => p.err("unexpected `]`"),
            Some(Tok::Comma) if !v2 => p.err("unexpected `,`"),
            _ => p.err("unexpected text after the value"),
        };
    }
    Ok(e)
}

/// The functions of the heights language: name and argument count.
pub const FUNCTIONS: [(&str, usize); 7] = [
    ("sin", 1),
    ("cos", 1),
    ("abs", 1),
    ("min", 2),
    ("max", 2),
    ("clamp", 3),
    ("smooth", 1),
];

/// The weighting functions and how many numbers follow the range.  The odds
/// themselves are `dt_geom::Weighting`'s (dt-doc holds no geometry); a test in
/// dt-eval keeps the two lists the same.
pub const WEIGHTINGS: [(&str, usize); 5] = [
    ("flat", 0),
    ("low", 1),
    ("high", 1),
    ("bell", 2),
    ("peak", 2),
];

/// The sine of an angle in degrees, from `+ - * /` only (a Taylor series
/// on the quarter wave), so a build gives the same bits on every platform.
#[must_use]
pub fn sin_deg(deg: f64) -> f64 {
    let mut r = deg % 360.0;
    if r > 180.0 {
        r -= 360.0;
    } else if r < -180.0 {
        r += 360.0;
    }
    if r > 90.0 {
        r = 180.0 - r;
    } else if r < -90.0 {
        r = -180.0 - r;
    }
    let x = r * (std::f64::consts::PI / 180.0);
    let x2 = x * x;
    // x - x^3/3! + x^5/5! - ... through x^19/19!
    let mut term = x;
    let mut sum = x;
    let mut n = 1.0;
    for _ in 0..9 {
        term = -term * x2 / ((n + 1.0) * (n + 2.0));
        sum += term;
        n += 2.0;
    }
    sum
}

/// The cosine of an angle in degrees (see [`sin_deg`]).
#[must_use]
pub fn cos_deg(deg: f64) -> f64 {
    sin_deg(deg + 90.0)
}

/// Smoothstep of `t` clamped to 0..1.
#[must_use]
pub fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn call_f64(name: &str, a: &[f64]) -> Result<f64, String> {
    let want = FUNCTIONS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| format!("unknown function `{name}`"))?
        .1;
    if a.len() != want {
        return Err(format!("`{name}` takes {want} value(s), not {}", a.len()));
    }
    Ok(match name {
        "sin" => sin_deg(a[0]),
        "cos" => cos_deg(a[0]),
        "abs" => a[0].abs(),
        "min" => a[0].min(a[1]),
        "max" => a[0].max(a[1]),
        "clamp" => a[0].max(a[1]).min(a[2]),
        _ => smooth(a[0]),
    })
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
            Expr::Weighted { range, .. } => range.visit(f),
            Expr::Call { args, .. } => args.iter().for_each(|a| a.visit(f)),
        }
    }

    /// Evaluate with every range at the position `u` (0 gives the low end,
    /// 1 the high end, 0.5 the middle): the ends and the middle of a value,
    /// not a draw.  A weighting is not applied here (it only matters when a
    /// range is drawn); a value with one range is `lo + (hi - lo) * u`.  A
    /// percent is divided by 100.  Names resolve through `env`.  Only
    /// `+ - * /` are used, so the result is bit-identical on every platform.
    ///
    /// # Errors
    /// Returns the first name `env` does not know.
    pub fn sample(&self, env: &dyn Fn(&str) -> Option<f64>, u: f64) -> Result<f64, String> {
        self.draw(env, &mut |_| u)
    }

    /// Evaluate with every range its own draw: `at` is asked once for each
    /// range, in the order the ranges finish evaluating (a range's ends
    /// first), and says where along it the draw lands, 0 to 1.  `Draw::ordinal`
    /// is that position, so a caller keys each range's draw by the value's key
    /// and the ordinal: editing one value never moves another's draws, and a
    /// value with one range (ordinal 0) draws as it always did.
    ///
    /// # Errors
    /// Returns the first name `env` does not know.
    pub fn draw(
        &self,
        env: &dyn Fn(&str) -> Option<f64>,
        at: &mut dyn FnMut(&Draw<'_>) -> f64,
    ) -> Result<f64, String> {
        self.draw_at(env, at, &mut 0, None)
    }

    fn draw_at<'a>(
        &'a self,
        env: &dyn Fn(&str) -> Option<f64>,
        at: &mut dyn FnMut(&Draw<'_>) -> f64,
        next: &mut usize,
        weighting: Option<(&'a str, &'a [f64])>,
    ) -> Result<f64, String> {
        Ok(match self {
            Expr::Num { value, percent } => {
                if *percent {
                    value / 100.0
                } else {
                    *value
                }
            }
            Expr::Name(n) => env(n).ok_or_else(|| format!("unknown name `{n}`"))?,
            Expr::Neg(e) => -e.draw_at(env, at, next, None)?,
            Expr::Bin { op, lhs, rhs } => {
                let a = lhs.draw_at(env, at, next, None)?;
                let b = rhs.draw_at(env, at, next, None)?;
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                }
            }
            Expr::Range { lo, hi } => {
                let a = lo.draw_at(env, at, next, None)?;
                let b = hi.draw_at(env, at, next, None)?;
                let ordinal = *next;
                *next += 1;
                let t = at(&Draw {
                    ordinal,
                    lo: a,
                    hi: b,
                    weighting,
                });
                a + (b - a) * t
            }
            Expr::Weighted { name, args, range } => {
                range.draw_at(env, at, next, Some((name, args)))?
            }
            Expr::Call { name, args } => {
                let vals = args
                    .iter()
                    .map(|a| a.draw_at(env, at, next, None))
                    .collect::<Result<Vec<_>, _>>()?;
                call_f64(name, &vals)?
            }
        })
    }

    /// The value written in the heights language with every weighting
    /// dropped, its ranges in brackets: what to write on a band.
    #[must_use]
    pub fn unweighted_text(&self) -> String {
        match self {
            Expr::Range { lo, hi } => {
                format!("[{} .. {}]", lo.unweighted_text(), hi.unweighted_text())
            }
            Expr::Weighted { range, .. } => range.unweighted_text(),
            Expr::Neg(e) => format!("-{}", e.unweighted_text()),
            Expr::Bin { op, lhs, rhs } => {
                let c = match op {
                    BinOp::Add => '+',
                    BinOp::Sub => '-',
                    BinOp::Mul => '*',
                    BinOp::Div => '/',
                };
                format!("{} {c} {}", lhs.unweighted_text(), rhs.unweighted_text())
            }
            Expr::Call { name, args } => {
                let a: Vec<String> = args.iter().map(Expr::unweighted_text).collect();
                format!("{name}({})", a.join(", "))
            }
            Expr::Num { .. } | Expr::Name(_) => self.to_string(),
        }
    }

    /// How many ranges the value holds: each is its own draw.
    #[must_use]
    pub fn range_count(&self) -> usize {
        let mut n = 0;
        self.visit(&mut |e| n += usize::from(matches!(e, Expr::Range { .. })));
        n
    }

    /// The first weighted range that is not the plain range (`flat`, or
    /// `low` or `high` with a ratio of 1), as written: what a band refuses.
    #[must_use]
    pub fn weighted(&self) -> Option<&Expr> {
        let mut found = None;
        self.visit(&mut |e| {
            if let Expr::Weighted { name, args, .. } = e
                && found.is_none()
                && !(name == "flat"
                    || (matches!(name.as_str(), "low" | "high") && args.first() == Some(&1.0)))
            {
                found = Some(e);
            }
        });
        found
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
            Expr::Weighted { name, args, range } => {
                write!(f, "{name}({range}")?;
                for a in args {
                    write!(f, ", {a}")?;
                }
                f.write_str(")")
            }
            Expr::Call { name, args } => {
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
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

    #[test]
    fn heights_language() {
        let e = parse_v2("groundHeight + [1 .. 2]").unwrap();
        assert!(e.is_range());
        assert_eq!(e.names(), vec!["groundHeight"]);
        // a hyphen subtracts, it is never part of a name
        let e = parse_v2("groundHeight-2").unwrap();
        assert_eq!(e.names(), vec!["groundHeight"]);
        let env = |n: &str| (n == "groundHeight").then_some(5.0);
        assert!(close(e.sample(&env, 0.0).unwrap(), 3.0));
        // a bare range is refused, with the brackets named
        let err = parse_v2("1 .. 2").unwrap_err();
        assert!(err.message.contains("brackets"), "{err:?}");
        assert!(parse_v2("[1 .. 2").is_err());
        assert!(parse_v2("[1]").is_err());
        let e = parse_v2("clamp(x, 0, 1) + max(1, 2)").unwrap();
        let env = |n: &str| (n == "x").then_some(3.0);
        assert!(close(e.sample(&env, 0.0).unwrap(), 3.0));
        assert!(parse_v2("sin(1, 2)").unwrap().sample(&none, 0.0).is_err());
        let err = parse_v2("tan(1)").unwrap_err();
        assert_eq!(err.code, Some("dt::value"));
        assert!(err.message.contains("unknown function `tan`"), "{err:?}");
        assert!(err.help.as_deref().unwrap().contains("sin, cos"), "{err:?}");
    }

    fn weighted(src: &str, v2: bool) -> (String, Vec<f64>, Expr) {
        let e = if v2 { parse_v2(src) } else { parse(src) }.unwrap();
        let Expr::Weighted { name, args, range } = e else {
            panic!("{src}: not a weighting: {e:?}")
        };
        (name, args, *range)
    }

    #[test]
    fn weightings_parse_in_both_dialects() {
        for (src, name, args) in [
            ("low(1..6, 11)", "low", vec![11.0]),
            ("low([1 .. 6], 11)", "low", vec![11.0]),
            ("high(1..6)", "high", vec![]),
            ("bell(1..6, 0.3, 0.4)", "bell", vec![0.3, 0.4]),
            ("peak( 2 .. 8 , 0.5 )", "peak", vec![0.5]),
            ("flat(1..6)", "flat", vec![]),
            ("low(1..6, -2)", "low", vec![-2.0]),
        ] {
            for v2 in [false, true] {
                let (n, a, r) = weighted(src, v2);
                assert_eq!((n.as_str(), &a), (name, &args), "{src}");
                assert!(matches!(r, Expr::Range { .. }), "{src}");
            }
        }
        // ends are expressions, names and ranges' arithmetic included
        let e = parse_v2("low([groundHeight .. groundHeight + 2.5], 3)").unwrap();
        assert_eq!(e.names(), vec!["groundHeight", "groundHeight"]);
        assert_eq!(e.range_count(), 1);
        // a weighting sits in arithmetic in the heights language
        let e = parse_v2("2 * low(1..6) + [-0.3 .. 0.3]").unwrap();
        assert_eq!(e.range_count(), 2);
        // it reads back
        for s in ["low(1..6, 11)", "bell(1..6, 0.2, 0.5)", "flat(2..4)"] {
            let e = parse(s).unwrap();
            assert_eq!(parse(&e.to_string()).unwrap(), e, "{s}");
        }
        // a weighted range has its ends and its middle like a plain one
        let e = parse("low(1..6, 11)").unwrap();
        assert_eq!(e.bounds(&none).unwrap(), (1.0, 6.0));
        assert!(close(e.sample(&none, 0.5).unwrap(), 3.5));
        assert!(e.is_range());
    }

    #[test]
    fn weighting_mistakes_have_their_code() {
        for (src, v2, what) in [
            ("low(bell(1..6))", false, "wraps a plain range"),
            ("low(bell(1..6))", true, "wraps a plain range"),
            ("low(3)", false, "wraps a plain range"),
            ("low(3, 2)", true, "wraps a plain range"),
            ("low(1..6, 1..2)", false, "plain numbers"),
            ("low(1..6, x)", true, "plain numbers"),
            ("low(1..6, 2, 3)", false, "at most"),
            ("flat(1..6, 2)", true, "at most"),
        ] {
            let err = if v2 { parse_v2(src) } else { parse(src) }.unwrap_err();
            assert_eq!(err.code, Some("dt::weighting"), "{src}: {err:?}");
            assert!(err.message.contains(what), "{src}: {err:?}");
            assert!(err.help.is_some(), "{src}");
        }
        for v2 in [false, true] {
            let err = if v2 {
                parse_v2("bel(1..6)")
            } else {
                parse("bel(1..6)")
            }
            .unwrap_err();
            assert_eq!(err.code, Some("dt::value"));
            assert!(err.message.contains("unknown function `bel`"));
            assert!(err.help.unwrap().contains("flat, low, high, bell or peak"));
        }
        // the rest of the grammar stays as it was in a control value
        assert!(
            parse("[1 .. 6]")
                .unwrap_err()
                .message
                .contains("unexpected `[`")
        );
        assert!(
            parse("1, 2")
                .unwrap_err()
                .message
                .contains("unexpected `,`")
        );
        assert!(parse("low(1..6").is_err());
    }

    #[test]
    fn a_band_refuses_what_is_not_flat() {
        assert!(parse("low(1..6)").unwrap().weighted().is_some());
        assert!(parse("bell(1..6)").unwrap().weighted().is_some());
        assert!(parse("flat(1..6)").unwrap().weighted().is_none());
        assert!(parse("low(1..6, 1)").unwrap().weighted().is_none());
        assert!(parse("1..6").unwrap().weighted().is_none());
        let e = parse_v2("low([groundHeight .. groundHeight + 2.5]) + 1").unwrap();
        assert_eq!(
            e.unweighted_text(),
            "[groundHeight .. groundHeight + 2.5] + 1"
        );
    }

    #[test]
    fn every_range_is_its_own_draw() {
        // two ranges, each at its own position: [1 .. 2] - [3 .. 4] is not -2
        let e = parse_v2("[1 .. 2] - [3 .. 4]").unwrap();
        let mut seen = Vec::new();
        let at = [0.0, 1.0];
        let v = e
            .draw(&none, &mut |d| {
                seen.push((d.ordinal, d.lo, d.hi));
                at[d.ordinal]
            })
            .unwrap();
        assert!(close(v, 1.0 - 4.0));
        assert_eq!(seen, vec![(0, 1.0, 2.0), (1, 3.0, 4.0)]);
        // `sample` is every range at one position: the ends and the middle
        assert!(close(e.sample(&none, 0.0).unwrap(), -2.0));
        // a value with one range draws as it always did: lo + (hi - lo) * u
        let e = parse("1..6").unwrap();
        let mut ord = Vec::new();
        let v = e
            .draw(&none, &mut |d| {
                ord.push(d.ordinal);
                0.3
            })
            .unwrap();
        assert_eq!(v.to_bits(), (1.0 + (6.0 - 1.0) * 0.3_f64).to_bits());
        assert_eq!(ord, vec![0]);
        // the weighting is handed to the draw, with its numbers
        let e = parse("low(1..6, 11)").unwrap();
        e.draw(&none, &mut |d| {
            assert_eq!(d.weighting, Some(("low", &[11.0][..])));
            0.0
        })
        .unwrap();
    }

    #[test]
    fn trigonometry_is_ours() {
        for k in -20..=20 {
            let d = f64::from(k) * 37.5;
            let (s, c) = (sin_deg(d), cos_deg(d));
            assert!((s - d.to_radians().sin()).abs() < 1e-12, "sin {d}");
            assert!((c - d.to_radians().cos()).abs() < 1e-12, "cos {d}");
        }
        assert_eq!(sin_deg(0.0), 0.0);
        assert!((cos_deg(720.0) - 1.0).abs() < 1e-15);
        assert!((cos_deg(360.0 * 3.5) + 1.0).abs() < 1e-15);
    }
}
