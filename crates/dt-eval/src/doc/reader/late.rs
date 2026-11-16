//! A parameter's value, told its control late.
//!
//! A definition's parameter fills a control the definition names (`pebble
//! size=s`), and the decoder types its default and every call's value by that
//! control.  When the definition's only use of it is on a thing parameter's
//! entry (`piece size=s`), which control that is depends on the thing the
//! caller passed, so it is known only here, when the entry is bound.  The
//! value was kept as written ([`dt_doc::model::Late`]); each control the entry
//! fills with the parameter types it now, with the decoder's own function
//! ([`dt_doc::decode::type_late`]), so the value gets exactly the checks it
//! gets written on the element, with the same code, words and help, pointing
//! at the text the caller wrote.  A parameter that fills two controls that are
//! not alike is refused at the parameter, as the decoder refuses it when both
//! uses are written in the definition.

use std::borrow::Cow;
use std::sync::Arc;

use dt_doc::decode::{Fill, Thing, bind_thing, two_kinds, type_late};
use dt_doc::model::{self, Callee, Late, Param, Spanned, Value};

use crate::diag::Diag;

use super::controls::bind;
use super::*;

/// What is known of a parameter's value beyond the value: how it was
/// written, what the definition found it to fill, and where it is declared.
pub(super) struct Arg {
    /// Names this value among the reader's records of what told it a
    /// control.
    id: usize,
    /// The value as written, when no control typed it.
    late: Option<Late>,
    /// What the definition found the parameter to fill, if anything.
    declared: Option<Fill>,
    /// The parameter, in the definition's text.
    param: Spanned<String>,
    file: Arc<File>,
}

/// One control a value was told, and what came of it.
pub(super) struct Told {
    id: usize,
    fill: Fill,
    /// False when the control refused the value, which was reported.
    ok: bool,
    /// The value typed by the control.
    value: Option<Spanned<Value>>,
}

/// An entry as the thing it calls, and the scope it is read in.
pub(super) struct Bind<'e, 's> {
    pub entry: Cow<'e, model::Entry>,
    pub scope: Cow<'s, Scope>,
}

impl Reader<'_> {
    /// What a call binds a parameter to, with what is known of how it was
    /// written.  A call's value for a parameter whose control the
    /// definition found is typed by it (the decoder typed it already, unless
    /// the definition was not decoded yet at the call).
    pub(super) fn arg(
        &mut self,
        p: &Param,
        written: Option<&model::Setting>,
        file: &Arc<File>,
        scope: &Scope,
    ) -> Option<Bound> {
        let mut b = match written.and_then(|s| s.value.as_ref()) {
            Some(v) => bind(v, &scope.file.src, scope),
            None => {
                let v = p.default.as_ref()?;
                Bound {
                    value: v.clone(),
                    src: Arc::clone(&file.src),
                    file: Arc::clone(file),
                    arg: None,
                }
            }
        };
        if b.arg.is_some() {
            // a name of the caller's own parameter: what tells that one
            // tells this
            return Some(b);
        }
        let late = match written {
            Some(s) => s.late.clone(),
            None => p.late.clone(),
        };
        if let (Some(l), Some(fill)) = (&late, &p.fill)
            && let Some(v) = self.type_by(l, fill, &b)
        {
            b.value = v;
        }
        self.arg_ids += 1;
        b.arg = Some(Arc::new(Arg {
            id: self.arg_ids,
            late: late.filter(|_| p.fill.is_none()),
            declared: p.fill.clone(),
            param: p.name.clone(),
            file: Arc::clone(file),
        }));
        Some(b)
    }

    /// A value typed by a control, or `None` once the mistakes are reported.
    fn type_by(&mut self, late: &Late, fill: &Fill, b: &Bound) -> Option<Spanned<Value>> {
        match type_late(late, fill, &b.file.src, &b.file.known) {
            Ok(v) => Some(v),
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                None
            }
        }
    }

    /// An entry as the thing it calls: for a thing parameter's entry, what
    /// was written on it typed now the thing is known, by the decoder's own
    /// function ([`dt_doc::decode::bind_thing`]), so a property gets exactly
    /// the checks it gets written on the element or definition itself; any
    /// other entry as it is.  The parameters of the definition that fill its
    /// controls are told them, and the scope has their values typed.  `None`
    /// once the mistakes are reported.
    pub(super) fn bound<'e, 's>(
        &mut self,
        e: &'e model::Entry,
        thing: Thing<'_>,
        scope: &'s Scope,
    ) -> Option<Bind<'e, 's>> {
        let Callee::Param(p) = &e.callee else {
            return Some(Bind {
                entry: Cow::Borrowed(e),
                scope: Cow::Borrowed(scope),
            });
        };
        let b = match bind_thing(e, p, thing, &scope.file.src, &scope.file.known) {
            Ok(b) => b,
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                return None;
            }
        };
        let mut typed = Cow::Borrowed(scope);
        let mut ok = true;
        for fill in &b.fills {
            ok &= self.tell(fill, scope, &mut typed);
        }
        ok.then_some(Bind {
            entry: Cow::Owned(b.entry),
            scope: typed,
        })
    }

    /// Tell the parameter `fill` names its control: type its value by it, and
    /// refuse it if what it fills is not alike.  False once reported.
    fn tell(&mut self, fill: &Fill, scope: &Scope, typed: &mut Cow<'_, Scope>) -> bool {
        // no value passed: reading the parameter reports that
        let Some(b) = scope.params.get(&fill.param) else {
            return true;
        };
        let Some(a) = &b.arg else { return true };
        // a use already told (an entry read twice) has its verdict
        if let Some(t) = self
            .told
            .iter()
            .find(|t| t.id == a.id && std::ptr::eq(t.fill.control, fill.control))
        {
            if let Some(v) = t.value.clone() {
                Self::set(typed, &fill.param, v);
            }
            return t.ok;
        }
        let other = self
            .told
            .iter()
            .filter(|t| t.id == a.id)
            .map(|t| &t.fill)
            .chain(a.declared.as_ref())
            .find(|f| !f.control.alike(fill.control))
            .cloned();
        if let Some(first) = other {
            self.errors.push(
                Diag::at(
                    &a.file.src,
                    a.param.span,
                    dt_lang::codes::TYPE,
                    two_kinds(&a.param.value, &first, fill),
                    "two kinds",
                )
                .help("a parameter is one kind of value, so it fills controls of one kind; use two parameters"),
            );
            self.told.push(Told {
                id: a.id,
                fill: fill.clone(),
                ok: false,
                value: None,
            });
            return false;
        }
        // typed by the control the definition found, when there is one: this
        // control's own range is held when the value is read
        let value = match &a.late {
            Some(late) => self.type_by(late, fill, b),
            None => None,
        };
        let ok = a.late.is_none() || value.is_some();
        if let Some(v) = &value {
            Self::set(typed, &fill.param, v.clone());
        }
        self.told.push(Told {
            id: a.id,
            fill: fill.clone(),
            ok,
            value,
        });
        ok
    }

    fn set(scope: &mut Cow<'_, Scope>, param: &str, value: Spanned<Value>) {
        if let Some(b) = scope.to_mut().params.get_mut(param) {
            b.value = value;
        }
    }
}
