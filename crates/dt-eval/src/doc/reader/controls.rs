//! Filling in the controls of one entry: as written, or the schema's default.

use std::sync::Arc;

use dt_doc::consts::ConstTable;
use dt_doc::model::{self, Spanned, Value};
use dt_doc::schema::{Control, Element};
use dt_kit::EntryId;
use miette::SourceSpan;

use crate::diag::{Diag, Src};
use crate::lower::{Lets, Lowered, element_value};

use super::*;

impl Reader<'_> {
    /// The controls a built element takes, as written or by default, and for
    /// a tree or a rock the `top=` its formula stands for; `None` when one
    /// cannot be given yet (it is in `pending`).  `owner` is the entry the
    /// controls are for, and the thing of it when they are a scatter's.
    pub(super) fn controls(
        &mut self,
        e: &model::Entry,
        element: &'static Element,
        scope: &Scope,
        pending: &mut Vec<Diag>,
        owner: (&EntryId, Option<usize>),
    ) -> Option<(Vec<Setting>, Option<Top>)> {
        let src = &scope.file.src;
        let mut out = Vec::new();
        let mut ok = true;
        let mut cloud_top = None;
        // a tree's height is its `top`: the outermost call's, else its own
        if dt_doc::words::takes_cloud_top(element.name)
            && let Some((t, at)) = scope
                .top
                .clone()
                .or_else(|| e.top.clone().map(|t| (t, Arc::clone(src))))
        {
            if let Some(table) = scope.lets.table.clone()
                && !self.readable(&table, &t.value, t.span, &at, Reads::At(element.name))
            {
                return None;
            }
            // a top that reads no place keeps the old checks of a tree's
            // height (20 to 120dmm over its foot, `bottom` added once)
            let table = scope
                .lets
                .table
                .clone()
                .unwrap_or_else(|| Arc::new(ConstTable::default()));
            let expr = expand_names(&t.value, &table, 0);
            let per_point = {
                let mut calls = false;
                expr.visit_calls(&mut |f| calls |= matches!(f, "x" | "y" | "d" | "a"));
                calls
                    || expr
                        .names()
                        .iter()
                        .any(|n| matches!(*n, "x" | "y" | "$x" | "$y" | "$d" | "$a" | "$_y"))
            };
            if element.name == "tree"
                && !per_point
                && let Err(why) = crate::lower::tree_top(&t.value, &scope.lets)
            {
                let mut d = Diag::at(&at, t.span, why.code, why.message, "here");
                d.help = why.help;
                if why.hard {
                    self.errors.push(d);
                } else {
                    pending.push(d);
                }
                return None;
            }
            out.push(Setting {
                origin: Origin::Written,
                name: "top".to_owned(),
                value: dt_elements::Value::Formula(dt_elements::Formula {
                    owner: owner.0.clone(),
                    child: owner.1,
                    key: expr.to_string(),
                }),
                span: t.span,
                src: Arc::clone(&at),
            });
            cloud_top = Some(Top {
                expr: t.value.clone(),
                span: t.span,
                prop: property_span(&at, t.span, "top"),
                src: at,
                crown: Some(expr),
            });
        }
        for s in &e.controls {
            // the control's own value, then its sub-controls as `control/sub`
            let mut items: Vec<Item<'_>> = Vec::new();
            // numbers written with no word adjust the default word's numbers
            // (`rock columns=50` leans the mixed cliff), so that word comes first
            if s.value.is_none()
                && !s.subs.is_empty()
                && let Some(c) = s.control
                && !c.words.is_empty()
                && let Some(raw) = c.default_value()
                && let Ok(Lowered { value, .. }) = element_value(&raw, &scope.lets)
            {
                push_setting(
                    &mut out,
                    Origin::Default,
                    &s.name.value,
                    &raw,
                    value,
                    s.span,
                    src,
                );
            }
            if let Some(v) = &s.value {
                items.push(Item {
                    name: s.name.value.clone(),
                    owner: element.name,
                    value: v,
                    span: s.span,
                    control: s.control,
                });
            }
            for sub in &s.subs {
                let name = format!("{}/{}", s.name.value, sub.name.value);
                match &sub.value {
                    Some(v) if element.takes(&name) => {
                        items.push(Item {
                            name,
                            owner: &s.name.value,
                            value: v,
                            span: sub.span,
                            control: sub.control,
                        });
                    }
                    _ => {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            sub.name.span,
                            &format!("sub-controls of `{}` are", s.name.value),
                        ));
                    }
                }
            }
            if s.value.is_none() && s.subs.is_empty() {
                ok = false;
                pending.push(not_yet(
                    src,
                    s.name.span,
                    &format!("the group `{}` on `{}` is", s.name.value, element.name),
                ));
            }
            for Item {
                name,
                owner,
                value: v,
                span,
                control,
            } in items
            {
                let b = bind(v, src, scope);
                // format 1: a count or size reads installed names, and none
                // that is worked out at each point
                if let Value::Num(n) = &b.value.value
                    && let Some(table) = scope.lets.table.clone()
                    && !self.readable(&table, &n.expr, b.value.span, &b.src, Reads::Once)
                {
                    ok = false;
                    continue;
                }
                let value = match value(&b, &scope.lets) {
                    Ok(Lowered { value, note }) => {
                        if let Some(note) = note {
                            self.notes.push(Diag::at(
                                &b.src,
                                b.value.span,
                                dt_lang::codes::WEIGHTING,
                                note,
                                "here",
                            ));
                        }
                        value
                    }
                    Err((why, hard)) => {
                        ok = false;
                        // a mistake is shown where it was made; a name with
                        // no value yet waits for its caller
                        if hard {
                            self.errors.push(*why);
                        } else {
                            pending.push(*why);
                        }
                        continue;
                    }
                };
                if let Some(c) = control
                    && let Err(why) = checked(c, owner, &value, &b)
                {
                    self.errors.push(*why);
                    continue;
                }
                if !element.takes(&name) {
                    // a control written at its default changes nothing
                    let default = control.and_then(|c| default_value(c, &scope.lets));
                    if default.as_ref() != Some(&value) {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            span,
                            &format!("`{}`'s `{}` is", element.name, s.name.value),
                        ));
                    }
                    continue;
                }
                push_setting(
                    &mut out,
                    Origin::Written,
                    &name,
                    &b.value.value,
                    value,
                    b.value.span,
                    &b.src,
                );
            }
        }
        for c in element.controls {
            if element.takes(c.name)
                && e.control(c.name).is_none()
                && let Some(raw) = c.default_value()
                && let Ok(Lowered { value, .. }) = element_value(&raw, &scope.lets)
            {
                push_setting(
                    &mut out,
                    Origin::Default,
                    c.name,
                    &raw,
                    value,
                    e.what.span,
                    src,
                );
            }
        }
        ok.then_some((out, cloud_top))
    }
}

/// One value written for a control: its own, or a sub-control's
/// (`crown/width`, owned by `crown`).
pub(super) struct Item<'a> {
    name: String,
    owner: &'a str,
    value: &'a Spanned<Value>,
    span: SourceSpan,
    control: Option<&'static Control>,
}

/// The schema's check of a value once its names are resolved (decode
/// checked it already when it was a plain number).
pub(super) fn checked(
    c: &'static Control,
    owner: &str,
    v: &dt_elements::Value,
    b: &Bound,
) -> Result<(), Box<Diag>> {
    let (lo, hi, range) = match v {
        dt_elements::Value::Number(x) => (*x, *x, false),
        dt_elements::Value::Range(lo, hi) => (*lo, *hi, true),
        dt_elements::Value::Dist(d) => (d.lo, d.hi, true),
        dt_elements::Value::Draw(d) => {
            let (lo, hi) = d.bounds();
            (lo, hi, true)
        }
        dt_elements::Value::Word(_) | dt_elements::Value::Formula(_) => return Ok(()),
    };
    c.check(owner, lo, hi, range).map_err(|r| {
        let mut d = Diag::at(
            &b.src,
            b.value.span,
            dt_lang::codes::RANGE,
            r.message,
            r.label,
        );
        d.help = r.help;
        Box::new(d)
    })
}

/// One setting for an element, and, for a word that stands for numbers
/// (`clods some`), each number as `control.name` (`clods.density`).
pub(super) fn push_setting(
    out: &mut Vec<Setting>,
    origin: Origin,
    name: &str,
    raw: &Value,
    value: dt_elements::Value,
    span: SourceSpan,
    src: &Src,
) {
    if let Value::Word(w) = raw {
        for (k, v) in w.sets {
            out.push(Setting {
                origin: Origin::Word,
                name: format!("{name}.{k}"),
                value: dt_elements::Value::Number(*v),
                span,
                src: Arc::clone(src),
            });
        }
    }
    out.push(Setting {
        origin,
        name: name.to_owned(),
        value,
        span,
        src: Arc::clone(src),
    });
}

/// A written value for a built element: a range stays a range (the element
/// draws from it), a weighted range keeps its odds.
pub(super) fn value(b: &Bound, lets: &Lets) -> Result<Lowered, (Box<Diag>, bool)> {
    element_value(&b.value.value, lets).map_err(|why| {
        let mut d = Diag::at(&b.src, b.value.span, why.code, why.message, "here");
        d.help = why.help;
        (Box::new(d), why.hard)
    })
}

/// `v` with a parameter replaced by what the caller bound to it.
pub(super) fn bind(v: &Spanned<Value>, src: &Src, scope: &Scope) -> Bound {
    if let Value::Param(p) = &v.value
        && let Some(b) = scope.params.get(p)
    {
        return b.clone();
    }
    Bound {
        value: v.clone(),
        src: Arc::clone(src),
    }
}

/// A control's schema default, resolved; `None` without one (or for a
/// range, which no built element takes yet).
pub(super) fn default_value(c: &'static Control, lets: &Lets) -> Option<dt_elements::Value> {
    element_value(&c.default_value()?, lets)
        .ok()
        .map(|l| l.value)
}

/// The whole property `name=value` whose value is written at `value`: the
/// value's span widened back over the `=` and the name (and the spaces KDL
/// allows round the `=`).  The decoder keeps a value's own span; the name is
/// found in front of it, in the same text, by looking at the two tokens
/// before it.  A value not written as that property keeps its own span.
pub(super) fn property_span(src: &Src, value: SourceSpan, name: &str) -> SourceSpan {
    let text = src.inner();
    let start = value.offset();
    let named = text
        .get(..start)
        .map(str::trim_end)
        .and_then(|b| b.strip_suffix('='))
        .map(str::trim_end)
        .and_then(|b| b.strip_suffix(name))
        // `stop=` is not `top=`
        .filter(|b| {
            b.chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || c == '{' || c == ';')
        });
    match named {
        Some(before) => SourceSpan::new(before.len().into(), start + value.len() - before.len()),
        None => value,
    }
}
