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

impl Status {
    /// The status as HTTP numbers it (and as a worker's reply numbers it, so the page reads one set of numbers).
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::Unprocessable => 422,
            Self::Internal => 500,
        }
    }
}

/// Whose fault: what the user asked, or what the program tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Refused,
    Failure,
    /// A panic or a bug in the program: it travels with the words of the panic ([`Refusal::backtrace`]).
    Bug,
}

/// What a bug says beyond its words: what to do about it, and where it happened.
#[derive(Debug, Clone, Default)]
pub struct BugDetail {
    /// What to do about it (copy the details into a report).
    pub help: Option<String>,
    /// The stack at the panic.
    pub backtrace: String,
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
    /// A conflict: the file as it is now, and its revision (boxed with the rest of what almost no refusal carries).
    pub conflict: Option<Box<Conflict>>,
    /// An unreadable document: its mistakes, each at its place.
    pub errors: Option<Box<[dt_eval::Mistake]>>,
    /// What a bug adds (boxed: a refusal is returned by every call, and almost none is a bug).
    pub bug: Option<Box<BugDetail>>,
    /// A newer request of the same kind took this one's place: nothing is wrong, nothing is said.
    pub superseded: bool,
    /// What the program was doing when it failed, when the call's own route does not say (a lane's work: the draft worker).
    pub doing: Option<Box<Doing>>,
}

/// A file that moved on: what it holds now and the revision that is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub text: String,
    pub doc_rev: u64,
}

/// What the program was at when it failed: the work (`draft worker`) and the tile it was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doing {
    pub route: String,
    pub tile: Option<String>,
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
            conflict: None,
            errors: None,
            bug: None,
            superseded: false,
            doing: None,
        }
    }

    /// A bug: the program panicked doing it.  `message` is the panic's words, `backtrace` its place.
    #[must_use]
    pub fn bug(status: Status, message: impl Into<String>, backtrace: impl Into<String>) -> Self {
        let mut r = Self::new(status, Fault::Bug, message);
        r.bug = Some(Box::new(BugDetail {
            help: Some(
                "This is a bug in dharmatiles, not in your tile. Copy the details into a bug report."
                    .to_owned(),
            ),
            backtrace: backtrace.into(),
        }));
        r
    }

    /// A newer request took this one's place (a conflict that is not a mistake).
    #[must_use]
    pub fn superseded(message: impl Into<String>) -> Self {
        let mut r = Self::refused(Status::Conflict, message);
        r.superseded = true;
        r
    }

    /// Say what the program was doing, where the call's own route does not: a lane's work, for a tile.
    #[must_use]
    pub fn doing(mut self, route: impl Into<String>, tile: Option<&str>) -> Self {
        self.doing = Some(Box::new(Doing {
            route: route.into(),
            tile: tile.map(str::to_owned),
        }));
        self
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

/// A refusal as a worker tells the page: the status number and the error body, the same body an HTTP error carries
/// (`ErrorBody`), so the page reads one shape from either host.
#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct RefusalReply {
    pub status: u16,
    pub body: crate::api::ErrorBody,
}

impl Refusal {
    /// This refusal as the page reads it: `route` is the call, `tile` the tile it was about, and `at_ms` the time.
    #[must_use]
    pub fn reply(self, route: &str, tile: Option<&str>, version: &str, at_ms: u64) -> RefusalReply {
        let kind = match self.fault {
            Fault::Refused => "refused",
            Fault::Failure => "failure",
            Fault::Bug => "bug",
        };
        let bug = self.bug.unwrap_or_default();
        // the work that failed, when it is named, is what the error says it was doing
        let (route, tile) = match self.doing {
            Some(d) => (d.route, d.tile.or_else(|| tile.map(str::to_owned))),
            None => (route.to_owned(), tile.map(str::to_owned)),
        };
        RefusalReply {
            status: self.status.code(),
            body: crate::api::ErrorBody {
                error: crate::api::ErrorDetail {
                    kind: kind.to_owned(),
                    message: self.message,
                    help: bug.help,
                    causes: self.causes,
                    backtrace: bug.backtrace,
                    context: crate::api::ErrorContext {
                        route: Some(route),
                        tile,
                        version: version.to_owned(),
                        at_ms,
                    },
                },
                text: self.conflict.as_ref().map(|c| c.text.clone()),
                doc_rev: self.conflict.as_ref().map(|c| c.doc_rev),
                scene: None,
                errors: self.errors.map(Vec::from),
                superseded: self.superseded.then_some(true),
            },
        }
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

    /// What the program was doing travels with the refusal: the reply says the work and its tile, not the call's route.
    #[test]
    fn a_refusal_says_what_the_program_was_doing() {
        let plain = Refusal::failure(Status::Internal, "no").reply("draft", Some("a"), "v", 1);
        assert_eq!(plain.body.error.context.route.as_deref(), Some("draft"));
        let at = Refusal::failure(Status::Internal, "no")
            .doing("draft worker", Some("b"))
            .reply("draft", Some("a"), "v", 1);
        assert_eq!(at.body.error.context.route.as_deref(), Some("draft worker"));
        assert_eq!(at.body.error.context.tile.as_deref(), Some("b"));
        // a work that names no tile keeps the call's
        let some = Refusal::failure(Status::Internal, "no")
            .doing("build worker", None)
            .reply("build", Some("a"), "v", 1);
        assert_eq!(some.body.error.context.tile.as_deref(), Some("a"));
    }

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
