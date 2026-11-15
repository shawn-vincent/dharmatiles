//! The service's one way in: a verb and a typed request in, a typed response out.
//!
//! A transport (the program's HTTP server, a web worker's messages) resolves what it was sent to a [`Verb`] and its
//! request type, and calls [`Table::call`]; it maps what comes back to its own reply (an HTTP status and body, a
//! message to the page).  The verbs are rows of a [`Table`] the program builds once from its calls: each row is a
//! plain function of the context `C` it runs on (the program's `Shared`) and the typed request.  No transport's words
//! (a route, a method, a status) are in here: a failure is the table's own error type `E`, which the transport maps.

use std::any::Any;
use std::collections::BTreeMap;
use std::marker::PhantomData;

/// A call, named by what it does, with the type it takes and the type it gives.  Made once, as a constant, where the
/// call's types are written.
pub struct Verb<Q, S> {
    pub name: &'static str,
    types: PhantomData<fn(Q) -> S>,
}

impl<Q, S> Verb<Q, S> {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            types: PhantomData,
        }
    }
}

/// How a refusal is told, in the service's words: a transport maps each to its own (HTTP: 400, 403, 404, 409, 422, 500).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The request is not a request (a missing reading, a bad name).
    BadRequest,
    /// Refused: not that file.
    Forbidden,
    /// There is no such thing.
    NotFound,
    /// The file moved on.
    Conflict,
    /// The document does not read.
    Unprocessable,
    /// The service could not do it.
    Internal,
}

/// Whose fault: what the user asked, or what the program tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Refused,
    Failure,
}

/// A call that ran and said no, with everything its HTTP or message form carries: the transport turns it into its own
/// reply.  The one error type of the tile verbs, so a handler's refusal is not a transport's reply.
#[derive(Debug)]
pub struct Refusal {
    pub status: Status,
    pub fault: Fault,
    pub message: String,
    /// What was under it (an I/O error's words).
    pub causes: Vec<String>,
    /// A conflict: the file as it is now, and its revision.
    pub text: Option<String>,
    pub doc_rev: Option<u64>,
    /// An unreadable document: its mistakes, each at its place.
    pub errors: Option<Vec<dt_eval::Mistake>>,
}

impl Refusal {
    /// The request was wrong.
    #[must_use]
    pub fn refused(status: Status, message: impl Into<String>) -> Self {
        Self::new(status, Fault::Refused, message)
    }

    /// The program could not.
    #[must_use]
    pub fn failure(status: Status, message: impl Into<String>) -> Self {
        Self::new(status, Fault::Failure, message)
    }

    fn new(status: Status, fault: Fault, message: impl Into<String>) -> Self {
        Self {
            status,
            fault,
            message: message.into(),
            causes: Vec::new(),
            text: None,
            doc_rev: None,
            errors: None,
        }
    }

    /// With the error under it, as the causes.
    #[must_use]
    pub fn caused_by(mut self, err: &(dyn std::error::Error + 'static)) -> Self {
        let mut e: Option<&(dyn std::error::Error + 'static)> = Some(err);
        while let Some(x) = e {
            self.causes.push(x.to_string());
            e = x.source();
        }
        self
    }

    /// A document that does not read: 422, its mistakes each at its place, and the first one's words as the message.
    #[must_use]
    pub fn of_mistakes(errs: &[dt_eval::Diag]) -> Self {
        let message = errs.first().map_or_else(
            || "the document does not read".to_owned(),
            |d| d.message.clone(),
        );
        let mut r = Self::refused(Status::Unprocessable, message);
        r.errors = Some(errs.iter().map(dt_eval::Mistake::from).collect());
        r
    }
}

/// Why a call did not run to an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum CallError<E> {
    /// No row of the table has this name: a transport that resolved it wrongly.
    NoSuchVerb(&'static str),
    /// The row's types are not the ones the call named: a verb made with other types than its row.
    WrongTypes(&'static str),
    /// The call ran and refused.
    Refused(E),
}

/// A row with its types erased: `None` when the request is not of the row's type.
type Erased<C, E> = Box<dyn Fn(&C, Box<dyn Any>) -> Option<Result<Box<dyn Any>, E>> + Send + Sync>;

/// The calls the service answers, by name.
pub struct Table<C, E> {
    rows: BTreeMap<&'static str, Erased<C, E>>,
}

impl<C: 'static, E: 'static> Default for Table<C, E> {
    fn default() -> Self {
        Self {
            rows: BTreeMap::new(),
        }
    }
}

impl<C: 'static, E: 'static> Table<C, E> {
    /// A row: `verb` is answered by `f` on the context.  A name taken twice is a mistake in the table, not a
    /// replacement.
    ///
    /// # Panics
    /// The name is already a row: the table is written once, in code, so this is found by the first run.
    #[must_use]
    pub fn with<Q: 'static, S: 'static>(
        mut self,
        verb: &Verb<Q, S>,
        f: fn(&C, Q) -> Result<S, E>,
    ) -> Self {
        let name = verb.name;
        let row: Erased<C, E> = Box::new(move |ctx, req| {
            let Ok(req) = req.downcast::<Q>() else {
                return None;
            };
            Some(f(ctx, *req).map(|s| Box::new(s) as Box<dyn Any>))
        });
        assert!(
            self.rows.insert(name, row).is_none(),
            "the call {name} is a row of the table twice"
        );
        self
    }

    /// Whether `name` is a row.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.rows.contains_key(name)
    }

    /// Answer `verb` for `req` on `ctx`.
    ///
    /// # Errors
    /// [`CallError`]: the verb is no row, its types are not the row's, or the call refused.
    pub fn call<Q: 'static, S: 'static>(
        &self,
        ctx: &C,
        verb: &Verb<Q, S>,
        req: Q,
    ) -> Result<S, CallError<E>> {
        let row = self
            .rows
            .get(verb.name)
            .ok_or(CallError::NoSuchVerb(verb.name))?;
        // a request of other types than the row's is told apart before anything runs
        let out = match row(ctx, Box::new(req)) {
            None => return Err(CallError::WrongTypes(verb.name)),
            Some(Ok(out)) => out,
            Some(Err(e)) => return Err(CallError::Refused(e)),
        };
        out.downcast::<S>()
            .map(|s| *s)
            .map_err(|_| CallError::WrongTypes(verb.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOUBLE: Verb<u32, u32> = Verb::new("double");
    const SHOUT: Verb<String, String> = Verb::new("shout");

    fn double(offset: &u32, n: u32) -> Result<u32, String> {
        if n > 100 {
            return Err("too big".to_owned());
        }
        Ok(n * 2 + offset)
    }

    #[test]
    fn a_verb_and_a_typed_request_give_a_typed_answer_or_the_refusal() {
        let table = Table::default().with(&DOUBLE, double);
        assert_eq!(table.call(&1, &DOUBLE, 4), Ok(9));
        assert_eq!(
            table.call(&1, &DOUBLE, 400),
            Err(CallError::Refused("too big".to_owned()))
        );
        assert_eq!(
            table.call(&1, &SHOUT, "a".to_owned()),
            Err(CallError::NoSuchVerb("shout"))
        );
        // the same name with other types is a wrong call, not a panic
        let wrong: Verb<u32, String> = Verb::new("double");
        assert_eq!(
            table.call(&1, &wrong, 4),
            Err(CallError::WrongTypes("double"))
        );
        let wrong_in: Verb<String, u32> = Verb::new("double");
        assert_eq!(
            table.call(&1, &wrong_in, "x".to_owned()),
            Err(CallError::WrongTypes("double"))
        );
    }
}
