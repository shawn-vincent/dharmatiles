//! What a call binds a definition's parameters to.
//!
//! A parameter is a name of the kind it was declared with (`kind=`, else its
//! default's), and the decoder has checked every use against it, so a value
//! bound here is already of the right kind.  A number a call passes is
//! closed over the caller's names, and the definition's body reads it as a
//! `let` of the same name (`scoped_def`): in a formula (`count="n + 1"`) as
//! well as a control's whole value (`count=n`).  A thing or a layer passed by
//! name is an entry's callee (`bound`).

use std::borrow::Cow;
use std::sync::Arc;

use dt_doc::decode::{Callable, bind_generator};
use dt_doc::model::{self, Callee, Definition, Num, Param, Value};

use crate::diag::Diag;

use super::controls::bind;
use super::*;

/// An entry as the thing it calls, and the scope it is read in.
pub(super) struct Bind<'e, 's> {
    pub entry: Cow<'e, model::Entry>,
    pub scope: Cow<'s, Scope>,
}

impl Reader<'_> {
    /// What a call binds a parameter to: the call's argument, else the
    /// parameter's default.  A number is closed over the names the caller
    /// sees, so it means the same wherever the body reads it.
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
                }
            }
        };
        if written.is_some()
            && let Value::Num(n) = &b.value.value
            && let Some(table) = scope.lets.table.as_ref()
        {
            let expr = n.expr.map_names(&mut |name| {
                if name.starts_with('$') {
                    None
                } else {
                    table.get(name).map(|e| e.expr.clone())
                }
            });
            b.value.value = Value::Num(Num { expr });
        }
        Some(b)
    }

    /// `scope` inside a call of `def`: the parameters that hold a number are
    /// `let`s of the body, then the definition's own `let`s and `override`s.
    pub(super) fn scoped_def(&mut self, scope: &Scope, def: &Definition) -> Scope {
        let params: Vec<model::Const> = def
            .params
            .iter()
            .filter_map(|p| p.as_const(&scope.params.get(&p.name.value)?.value))
            .collect();
        let scope = self.scoped(scope, &params);
        let scope = self.scoped(&scope, &def.consts);
        self.scoped(&scope, &def.stack_consts)
    }

    /// An entry as the thing it calls: for a thing parameter's entry, what
    /// was written on it typed now the thing is known, by the decoder's own
    /// function ([`dt_doc::decode::bind_generator`]), so a property gets exactly
    /// the checks it gets written on the element or definition itself; any
    /// other entry as it is.  `None` once the mistakes are reported.
    pub(super) fn bound<'e, 's>(
        &mut self,
        e: &'e model::Entry,
        thing: Callable<'_>,
        scope: &'s Scope,
    ) -> Option<Bind<'e, 's>> {
        let Callee::Param(p) = &e.callee else {
            return Some(Bind {
                entry: Cow::Borrowed(e),
                scope: Cow::Borrowed(scope),
            });
        };
        match bind_generator(e, p, thing, &scope.file.src, &scope.file.known) {
            Ok(entry) => Some(Bind {
                entry: Cow::Owned(entry),
                scope: Cow::Borrowed(scope),
            }),
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                None
            }
        }
    }
}
