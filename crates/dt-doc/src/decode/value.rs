//! Values: one entry's value, typed by the control it sets.

use kdl::{KdlEntry, KdlValue};

use super::{Ctx, Decoder, sub_span, units, value_span};
use crate::elements::TAGS;
use crate::expr::{self, Expr, Unit};
use crate::model::{Num, Spanned, Value};
use crate::schema::{Axis, Control, Refusal, Word, did_you_mean};

fn as_f64(v: &KdlValue) -> Option<f64> {
    match v {
        KdlValue::Integer(i) => i.to_string().parse().ok(),
        KdlValue::Float(f) => Some(*f),
        _ => None,
    }
}

impl Decoder<'_> {
    /// A control's value: a word, a parameter, or a value of its axis.
    pub(crate) fn value(
        &mut self,
        e: &KdlEntry,
        k: &'static Control,
        owner: &str,
        name: &str,
        ctx: &Ctx,
    ) -> Option<Spanned<Value>> {
        let axis = k.value?;
        let span = value_span(e);
        if let Some(s) = e.value().as_string() {
            if ctx.params.iter().any(|p| p == s) {
                self.param_controls.entry(s.to_owned()).or_insert(k);
                return Some(Spanned {
                    value: Value::Param(s.to_owned()),
                    span,
                });
            }
            if let Some(w) = k.words.iter().find(|w| w.word == s) {
                if e.ty().is_some() {
                    self.err(
                        dt_lang::codes::UNIT,
                        span,
                        format!("`{s}` is a word, so it takes no annotation"),
                        "annotated word",
                    );
                    return None;
                }
                return Some(Spanned {
                    value: Value::Word(w),
                    span,
                });
            }
            match axis {
                Axis::Word => {
                    self.bad_word(span, s, k.words, owner, name);
                    return None;
                }
                Axis::Tag => {
                    if TAGS.contains(&s) {
                        return Some(Spanned {
                            value: Value::Tag(s.to_owned()),
                            span,
                        });
                    }
                    self.err(
                        dt_lang::codes::UNKNOWN_NAME,
                        span,
                        format!("`{s}` is not a kind of thing"),
                        "unknown kind",
                    )
                    .suggest_or(
                        span,
                        did_you_mean(s, TAGS.iter().copied()),
                        || format!("things gather against: {}", TAGS.join(", ")),
                    );
                    return None;
                }
                Axis::Bool => {
                    self.err(
                        dt_lang::codes::VALUE,
                        span,
                        format!("`{name}` is #true or #false"),
                        "a string",
                    )
                    .help = Some(format!("write {name}=#{s}"));
                    return None;
                }
                _ => {}
            }
        }
        if let KdlValue::Bool(b) = e.value() {
            if axis == Axis::Bool {
                return Some(Spanned {
                    value: Value::Bool(*b),
                    span,
                });
            }
            self.err(
                dt_lang::codes::VALUE,
                span,
                format!("`{owner} {name}` is not a switch"),
                "a switch",
            )
            .help = Some(describe(k));
            return None;
        }
        if axis.is_numeric() {
            // a control may name the tile's own `let`s (a range one holds is
            // drawn where it is used), though not a library height
            let n = self.num(
                e,
                axis,
                false,
                axis != Axis::Share,
                Some(k),
                owner,
                name,
                ctx,
            )?;
            return Some(Spanned {
                value: Value::Num(n.value),
                span: n.span,
            });
        }
        self.err(
            dt_lang::codes::VALUE,
            span,
            format!("`{owner} {name}` is {}", axis.describe()),
            "wrong kind of value",
        )
        .help = Some(describe(k));
        None
    }

    fn bad_word(
        &mut self,
        span: miette::SourceSpan,
        s: &str,
        words: &'static [Word],
        owner: &str,
        name: &str,
    ) {
        let list: Vec<&str> = words.iter().map(|w| w.word).collect();
        self.err(
            dt_lang::codes::CHOICE,
            span,
            format!("`{owner} {name}` can't be `{s}`"),
            "not one of its words",
        )
        .suggest_or(span, did_you_mean(s, list.iter().copied()), || {
            format!("`{name}` is one of: {}", list.join(", "))
        });
    }

    /// A number on `axis`: a KDL number, or a quoted expression, range or
    /// percent.  `k` supplies words for hints and limits.  `lets` lets the
    /// number name the tile's `let` heights (a line's `wobble`); no element
    /// control does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn num(
        &mut self,
        e: &KdlEntry,
        axis: Axis,
        lets: bool,
        tile_lets: bool,
        k: Option<&'static Control>,
        owner: &str,
        name: &str,
        ctx: &Ctx,
    ) -> Option<Spanned<Num>> {
        let span = value_span(e);
        let kind = if lets {
            "a length: a number with a unit, a `$` name, or an expression like \"$groundHeight - 1dmm\""
        } else {
            axis.describe()
        };
        if let Some(t) = e.ty() {
            self.err(
                dt_lang::codes::UNIT,
                t.span(),
                format!("`({})` is not written in format 1", t.value()),
                "an annotation",
            )
            .help = Some(
                "write the unit after the number, in a string: `\"4.5dmm\"`; `(h)` is gone"
                    .to_owned(),
            );
            return None;
        }
        let unit = match e.ty() {
            None => None,
            Some(t) => match Unit::parse(t.value()) {
                Some(u) if axis == Axis::Dmm => Some(u),
                Some(u) => {
                    self.err(
                        dt_lang::codes::UNIT,
                        span,
                        format!("`({})` doesn't fit `{owner} {name}`", u.as_str()),
                        format!("{name} is {kind}"),
                    )
                    .help = Some(format!(
                        "(mm) and (h) are for sizes and levels; write `{name}` without it"
                    ));
                    return None;
                }
                None => {
                    self.err(
                        dt_lang::codes::UNIT,
                        t.span(),
                        format!("`({})` is not an annotation", t.value()),
                        "not (mm) or (h)",
                    )
                    .help = Some(UNIT_HELP.to_owned());
                    return None;
                }
            },
        };
        let (x, inner) = match e.value() {
            KdlValue::Null => {
                self.err(
                    dt_lang::codes::VALUE,
                    span,
                    format!("`{name}` can't be #null"),
                    "null",
                )
                .help = Some(format!("leave `{name}` out to use its default"));
                return None;
            }
            KdlValue::Bool(_) => {
                self.err(
                    dt_lang::codes::VALUE,
                    span,
                    format!("`{owner} {name}` is {kind}"),
                    "a switch",
                );
                return None;
            }
            v @ (KdlValue::Integer(_) | KdlValue::Float(_)) => {
                (Expr::num(as_f64(v).unwrap_or(f64::NAN)), span)
            }
            KdlValue::String(s) => {
                let at = self.inner_at(e, s);
                let parsed = expr::parse_units(s);
                match parsed {
                    Ok(x) => (x, sub_span(span, at, at + s.len())),
                    Err(err) => {
                        let help = err.help.clone().unwrap_or_else(|| {
                            k.map_or_else(|| format!("`{name}` is {kind}"), describe)
                        });
                        self.err(
                            err.code.unwrap_or(dt_lang::codes::EXPR),
                            sub_span(span, at + err.start, at + err.end),
                            format!("can't read `{owner} {name}`: {}", err.message),
                            "here",
                        )
                        .help = Some(help);
                        return None;
                    }
                }
            }
        };
        if let Some((code, r)) = percent_flaw(&x, axis, k, (owner, name), kind) {
            self.err(code, span, r.message, r.label).help = r.help;
            return None;
        }
        // Names: parameters, and the tile's `let`s where a height is expected.
        // the tile's own `let`s: the constants that are not the library's
        let own: Vec<&str> = if tile_lets {
            ctx.consts
                .iter()
                .filter(|(l, d)| {
                    *d != crate::dim::Dim::Squares
                        && !l.starts_with('$')
                        && !self.known.consts().iter().any(|c| c.name.value == *l)
                })
                .map(|(l, _)| l.as_str())
                .collect()
        } else {
            Vec::new()
        };
        for n in x.names() {
            if n.starts_with('$') {
                let at = {
                    let text = &self.text()[inner.offset()..inner.offset() + inner.len()];
                    text.find(n).map_or(inner, |i| {
                        miette::SourceSpan::new((inner.offset() + i).into(), n.len())
                    })
                };

                if crate::words::REGION_NAMES.contains(&n) {
                    self.err(
                        dt_lang::codes::TYPE,
                        at,
                        format!("`{n}` is a region, and `{owner} {name}` is a number"),
                        "a region is not a number",
                    )
                    .help =
                        Some("a region goes to `parent(…)`; nothing else reads one yet".to_owned());
                    return None;
                }
                if crate::words::POINT_NAMES.contains(&n)
                    || crate::words::RESERVED_POINT_NAMES.contains(&n)
                {
                    self.err(
                        dt_lang::codes::TYPE,
                        at,
                        format!("`{n}` is worked out at each point, and `{owner} {name}` is drawn once"),
                        "a size or count is not per point",
                    )
                    .help = Some(
                        "a position or a height under it goes where a height is worked out at each point: a top, a pin or a level".to_owned(),
                    );
                    return None;
                }
                // any other `$` name is read from the context of whoever
                // calls: `check` settles where it is installed
                continue;
            }
            let ok = ctx.params.iter().any(|p| p == n)
                || own.contains(&n)
                || (lets
                    && ctx
                        .consts
                        .iter()
                        .any(|(l, d)| l == n && *d != crate::dim::Dim::Squares));
            if ok {
                continue;
            }
            let mut cands: Vec<&str> = ctx.params.iter().map(String::as_str).collect();
            if lets {
                cands.extend(ctx.consts.iter().map(|(l, _)| l.as_str()));
            }
            cands.extend(own.iter().copied());
            if let Some(k) = k {
                cands.extend(k.words.iter().map(|w| w.word));
            }
            let what = if lets {
                "height or parameter"
            } else if !own.is_empty() {
                "parameter or `let`"
            } else if k.is_some_and(|k| !k.words.is_empty()) {
                "word or parameter"
            } else {
                "parameter"
            };
            let near = did_you_mean(n, cands.iter().copied());
            let at = self.word_span(inner, n);
            self.err(
                dt_lang::codes::UNKNOWN_NAME,
                at,
                format!("no {what} is called `{n}`"),
                "unknown name",
            )
            .suggest_or(at, near, || {
                let help = k.map_or_else(|| format!("`{name}` is {kind}"), describe);
                if lets && !ctx.consts.is_empty() {
                    let list: Vec<&str> = ctx.consts.iter().map(|(l, _)| l.as_str()).collect();
                    format!(
                        "{help}; heights: {}; a tile adds its own with `let name mm`",
                        list.join(", ")
                    )
                } else {
                    help
                }
            });
            return None;
        }
        let x = {
            let what = format!("`{owner} {name}`");
            self.check_lower(&x, super::units::want_of(axis), ctx, span, &what)?
                .0
        };
        if x.names().is_empty()
            && let Some(r) = range_flaw(&x, unit, axis, k, (owner, name))
        {
            self.err(dt_lang::codes::RANGE, span, r.message, r.label)
                .help = r.help;
            return None;
        }
        Some(Spanned {
            value: Num { expr: x, unit },
            span,
        })
    }

    /// A value nothing types: a parameter default whose parameter fills
    /// no known control, or an argument to such a parameter.
    pub(crate) fn untyped(&mut self, e: &KdlEntry) -> Option<Spanned<Value>> {
        let span = value_span(e);
        let unit = e.ty().and_then(|t| Unit::parse(t.value()));
        let value = match e.value() {
            KdlValue::Bool(b) => Value::Bool(*b),
            v @ (KdlValue::Integer(_) | KdlValue::Float(_)) => Value::Num(Num {
                expr: Expr::num(as_f64(v).unwrap_or(f64::NAN)),
                unit,
            }),
            KdlValue::String(s) => {
                // format 1: a value with a unit is read in the references
                // (a length in design millimetres, an angle in degrees)
                let parsed = expr::parse_units(s).and_then(|x| {
                    crate::units::lower(&x, crate::units::Base::Dmm, self.known.units(), None)
                        .map_err(|ke| expr::ExprError {
                            message: ke.message,
                            code: None,
                            help: ke.help,
                            start: 0,
                            end: 0,
                        })
                });
                match parsed {
                    Ok(x) if x.names().is_empty() => Value::Num(Num { expr: x, unit }),
                    _ => Value::Text(s.clone()),
                }
            }
            KdlValue::Null => {
                self.err(
                    dt_lang::codes::VALUE,
                    span,
                    "a value can't be #null",
                    "null",
                );
                return None;
            }
        };
        Some(Spanned { value, span })
    }
}

/// A number written as a percent where a share is, and not elsewhere: what
/// is wrong with `x` for this control, with its diagnostic code.
fn percent_flaw(
    x: &Expr,
    axis: Axis,
    k: Option<&'static Control>,
    (owner, name): (&str, &str),
    kind: &str,
) -> Option<(&'static str, Refusal)> {
    // a share is a percent or a plain fraction; a percent is refused elsewhere
    if axis != Axis::Share && x.has_percent() {
        return Some((
            dt_lang::codes::PERCENT,
            Refusal {
                message: format!("`{owner} {name}` is {kind}, not a percent"),
                label: "a percent",
                help: Some(k.map_or_else(|| format!("write `{name}` as a plain number"), describe)),
            },
        ));
    }
    None
}

/// What is wrong with a number with no names in it, against the control's
/// limits (`(h)` is in wall tops, checked in mm).
fn range_flaw(
    x: &Expr,
    unit: Option<Unit>,
    axis: Axis,
    k: Option<&'static Control>,
    (owner, name): (&str, &str),
) -> Option<Refusal> {
    // Counts are whole; sensible limits catch slips.
    let (lo, hi) = x.bounds(&|_| None).ok()?;
    let mult = if unit == Some(Unit::H) {
        crate::schema::H_MM
    } else {
        1.0
    };
    let (lo, hi) = (lo * mult, hi * mult);
    match k {
        Some(k) => k.check(owner, lo, hi, x.is_range()),
        None => crate::schema::check(axis, None, true, (owner, name), (lo, hi, x.is_range())),
    }
    .err()
}

/// What a person typed for a control, checked as the reader checks a
/// written one: what is wrong with it, or `None` (also for anything the
/// reader resolves later, such as an expression with names, and for what is
/// not a number or a range, such as a word).  A length carries its unit
/// (`8dmm`, `0.5sq`), an angle is `deg` or `rad`, and a bare number is
/// refused as the reader refuses it.  `units` says what `mm`, `in` and
/// `layer` are worth on the build target.
#[must_use]
pub fn check_text_in(
    k: &'static Control,
    owner: &str,
    typed: &str,
    units: crate::units::Units,
) -> Option<Refusal> {
    let axis = k.value?;
    let t = typed.trim();
    if t.is_empty() {
        return None;
    }
    let x = match expr::parse_units(t) {
        Ok(x) => x,
        Err(_) if !axis.is_numeric() => return None,
        Err(e) => {
            return Some(Refusal {
                message: format!("`{t}` does not read as a number: {}", e.message),
                label: "not a number",
                help: e.help.or_else(|| Some(describe(k))),
            });
        }
    };
    if let Some((_, r)) = percent_flaw(&x, axis, Some(k), (owner, k.name), axis.describe()) {
        return Some(r);
    }
    if !axis.is_numeric() {
        return None;
    }
    // what a unit-free `$name` or a `let` is worth is the reader's to say
    // (`check`, with the document); the kinds and units of what is written
    // are checked here
    let names = |_: &str| None;
    if let Err(ke) = crate::units::expect(&x, units::want_of(axis), &names, false) {
        return Some(Refusal {
            message: ke.message,
            label: "wrong kind",
            help: ke.help,
        });
    }
    let lowered = match crate::units::lower(&x, crate::units::Base::Dmm, units, None) {
        Ok(l) => l,
        Err(ke) => {
            return Some(Refusal {
                message: ke.message,
                label: "wrong unit",
                help: ke.help,
            });
        }
    };
    if !lowered.names().is_empty() {
        return None;
    }
    range_flaw(&lowered, None, axis, Some(k), (owner, k.name))
}

/// What the reader says of an annotation that is not a unit.
const UNIT_HELP: &str =
    "units are not written; (mm)4.5 checks a size, and (h)1.2 is in wall tops of 33.1 mm";

/// One line describing what a control takes, for help text.
pub(crate) fn describe(k: &'static Control) -> String {
    let words: Vec<&str> = k.words.iter().map(|w| w.word).collect();
    match (k.value, words.is_empty()) {
        (Some(Axis::Word), _) => format!("`{}` is one of: {}", k.name, words.join(", ")),
        (Some(a), true) => format!("`{}` is {}", k.name, a.describe()),
        (Some(a), false) => format!(
            "`{}` is {}, or one of: {}",
            k.name,
            a.describe(),
            words.join(", ")
        ),
        (None, _) => format!(
            "`{}` is a group: {}",
            k.name,
            k.subs.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")
        ),
    }
}
