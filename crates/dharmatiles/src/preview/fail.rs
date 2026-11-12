//! One error shape for everything the preview's server says went wrong, and
//! the one place that makes it.  A reply that is not a success carries
//! `{"error": {kind, message, help, causes, backtrace, context}}`, whatever
//! failed: a route, a build, a draft, a panic.  The page turns it into one
//! error with the server's story inside (`web/preview/js/server-error.js`)
//! and shows it in the one error panel (`web/preview/js/problems.js`).
//!
//! Three kinds, which decide how the page puts it:
//! - `refused`: the request was wrong (no such tile, a name that cannot be a
//!   file); the page shows the message where it asked.
//! - `failure`: something the program tried did not work (a file it could
//!   not write); the page shows the message and help as the headline.
//! - `bug`: a panic; the page says something went wrong inside dharmatiles.
//!
//! The backtrace is always captured, on every kind, so a report never lacks
//! one.  A panic's is taken in the panic hook, where the stack still is, and
//! picked up where the panic is caught.

use std::backtrace::Backtrace;
use std::cell::RefCell;
use std::panic::PanicHookInfo;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// How the page should put an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The request was wrong; the page shows it where it asked.
    Refused,
    /// The program tried and could not; the message is the headline.
    Failure,
    /// A panic: a bug in dharmatiles.
    Bug,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Failure => "failure",
            Self::Bug => "bug",
        }
    }
}

/// What the server was doing: the request (`POST /api/tiles/ID/save`) or the
/// worker's job, and the tile it was for.
#[derive(Debug, Clone, Default)]
pub struct Doing {
    pub route: Option<String>,
    pub tile: Option<String>,
}

thread_local! {
    /// The request this thread is answering (set by [`answering`]).
    static DOING: RefCell<Doing> = RefCell::new(Doing::default());
    /// The last panic on this thread: its message, place and backtrace.
    static PANIC: RefCell<Option<Panic>> = const { RefCell::new(None) };
}
/// The last panic anywhere, for one that unwinds on another thread than the
/// catch (a rayon worker's, say).
static LAST_PANIC: Mutex<Option<Panic>> = Mutex::new(None);

#[derive(Debug, Clone)]
struct Panic {
    message: String,
    location: String,
    backtrace: String,
}

/// Run `f` as the answer to `route` (and `tile`), so every error made inside
/// it says what the server was doing.
pub fn answering<T>(route: String, tile: Option<String>, f: impl FnOnce() -> T) -> T {
    let before = DOING.with(|d| {
        d.replace(Doing {
            route: Some(route),
            tile,
        })
    });
    let out = f();
    DOING.with(|d| *d.borrow_mut() = before);
    out
}

/// Called from the panic hook: keep the panic's message, place and the stack
/// as it is now.
pub fn record(info: &PanicHookInfo<'_>) {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned());
    let location = info.location().map_or_else(
        || "an unknown place".to_owned(),
        |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
    );
    let p = Panic {
        message,
        location,
        backtrace: Backtrace::force_capture().to_string(),
    };
    *LAST_PANIC.lock().unwrap_or_else(PoisonError::into_inner) = Some(p.clone());
    PANIC.with(|c| *c.borrow_mut() = Some(p));
}

/// Make panics recorded even where `main`'s hook is not installed (the
/// tests'): wraps whatever hook is there, once.
#[cfg(test)]
pub fn install_recorder() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            record(info);
            previous(info);
        }));
    });
}

fn take_panic() -> Option<Panic> {
    PANIC.with(|c| c.borrow_mut().take()).or_else(|| {
        LAST_PANIC
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    })
}

fn now_ms() -> u64 {
    // a clock before 1970 is not a case; 0 then, in a field only the page's dedupe reads
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// One failure, as the page gets it.
#[derive(Debug, Clone)]
pub struct ServerError {
    pub kind: Kind,
    pub status: &'static str,
    pub message: String,
    pub help: Option<String>,
    /// The error chain, outermost first (`source()`), or a panic's place.
    pub causes: Vec<String>,
    pub backtrace: String,
    pub doing: Doing,
    pub at_ms: u64,
}

impl ServerError {
    /// A new error of `kind`, with the backtrace of here and the request this
    /// thread is answering.
    #[must_use]
    pub fn new(kind: Kind, status: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            status,
            message: message.into(),
            help: None,
            causes: Vec::new(),
            backtrace: Backtrace::force_capture().to_string(),
            doing: DOING.with(|d| d.borrow().clone()),
            at_ms: now_ms(),
        }
    }

    /// The request was wrong.
    #[must_use]
    pub fn refused(status: &'static str, message: impl Into<String>) -> Self {
        Self::new(Kind::Refused, status, message)
    }

    /// The program could not do what was asked.
    #[must_use]
    pub fn failure(status: &'static str, message: impl Into<String>) -> Self {
        Self::new(Kind::Failure, status, message)
    }

    /// A caught panic: its words, where it happened and the stack at the
    /// panic.  `doing` says what the worker was at, when no request is.
    #[must_use]
    pub fn bug(panic: &(dyn std::any::Any + Send), doing: Option<Doing>) -> Self {
        let mut e = Self::new(
            Kind::Bug,
            "500 Internal Server Error",
            super::panic_message(panic),
        );
        e.help = Some(
            "This is a bug in dharmatiles, not in your tile. Copy the details into a bug report."
                .to_owned(),
        );
        if let Some(d) = doing {
            e.doing = d;
        }
        match take_panic() {
            Some(p) => {
                e.message = p.message;
                e.causes.push(format!("panicked at {}", p.location));
                e.backtrace = p.backtrace;
            }
            None => e.causes.push(
                "the panic hook did not record this panic; the stack is where it was caught"
                    .to_owned(),
            ),
        }
        e
    }

    /// Say what to do about it.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Say what the server was doing (when the request does not).
    #[must_use]
    pub fn doing(mut self, route: impl Into<String>, tile: Option<&str>) -> Self {
        self.doing = Doing {
            route: Some(route.into()),
            tile: tile.map(str::to_owned),
        };
        self
    }

    /// Add `err` and everything under it as the causes.
    #[must_use]
    pub fn caused_by(mut self, err: &(dyn std::error::Error + 'static)) -> Self {
        let mut e: Option<&(dyn std::error::Error + 'static)> = Some(err);
        while let Some(x) = e {
            self.causes.push(x.to_string());
            e = x.source();
        }
        self
    }

    /// The error as JSON, the body of `{"error": ...}`.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "message": self.message,
            "help": self.help,
            "causes": self.causes,
            "backtrace": self.backtrace,
            "context": {
                "route": self.doing.route,
                "tile": self.doing.tile,
                "version": env!("DT_ENGINE_VERSION"),
                "at_ms": self.at_ms,
            },
        })
    }

    /// The reply: the status and `{"error": ...}`.
    #[must_use]
    pub fn reply(&self) -> (&'static str, Value) {
        (self.status, json!({ "error": self.json() }))
    }

    /// The line for the server's log.
    #[must_use]
    pub fn line(&self) -> String {
        match &self.doing.route {
            Some(r) => format!("{r}: {}: {}", self.status, self.message),
            None => format!("{}: {}", self.status, self.message),
        }
    }
}

impl From<ServerError> for (&'static str, Value) {
    fn from(e: ServerError) -> Self {
        e.reply()
    }
}

/// A request the page got wrong: the status, the words.
#[must_use]
pub fn refused(status: &'static str, message: impl Into<String>) -> (&'static str, Value) {
    ServerError::refused(status, message).reply()
}

/// A refusal that also says something besides: `extra`'s keys sit beside
/// `error` in the body (the text a conflicting save lost to).
#[must_use]
pub fn refused_with(
    status: &'static str,
    message: impl Into<String>,
    extra: Value,
) -> (&'static str, Value) {
    let (status, mut body) = ServerError::refused(status, message).reply();
    if let (Some(b), Value::Object(x)) = (body.as_object_mut(), extra) {
        b.extend(x);
    }
    (status, body)
}

/// Something the program could not do: the status, the words.
#[must_use]
pub fn failure(status: &'static str, message: impl Into<String>) -> (&'static str, Value) {
    ServerError::failure(status, message).reply()
}

/// As [`failure`], with the error under it as the causes.
#[must_use]
pub fn failure_from(
    status: &'static str,
    message: impl Into<String>,
    err: &(dyn std::error::Error + 'static),
) -> (&'static str, Value) {
    ServerError::failure(status, message).caused_by(err).reply()
}

/// An error with a source under it, for the routes that fail on purpose and
/// the tests: the chain `source()` walks.
#[cfg(any(test, feature = "test-routes"))]
#[derive(Debug)]
pub struct Layered(&'static str, Option<Box<Layered>>);

#[cfg(any(test, feature = "test-routes"))]
impl Layered {
    #[must_use]
    pub fn demo() -> Self {
        Self(
            "could not save the tile",
            Some(Box::new(Self(
                "the folder is read-only",
                Some(Box::new(Self("permission denied (os error 13)", None))),
            ))),
        )
    }
}

#[cfg(any(test, feature = "test-routes"))]
impl std::fmt::Display for Layered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[cfg(any(test, feature = "test-routes"))]
impl std::error::Error for Layered {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.1.as_deref().map(|e| e as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn a_caught_panic_is_a_bug_with_its_place_and_stack() {
        install_recorder();
        let panic = catch_unwind(AssertUnwindSafe(|| panic!("the kernel fell over"))).unwrap_err();
        let doing = Doing {
            route: Some("build worker".to_owned()),
            tile: Some("t.dtile".to_owned()),
        };
        let e = ServerError::bug(panic.as_ref(), Some(doing));
        let v = e.json();
        assert_eq!(v["kind"], "bug");
        assert_eq!(v["message"], "the kernel fell over");
        assert!(v["causes"][0].as_str().unwrap().contains("fail.rs"), "{v}");
        assert!(
            v["backtrace"].as_str().unwrap().contains("a_caught_panic"),
            "the stack is the panic's, taken in the hook: {}",
            v["backtrace"]
        );
        assert_eq!(v["context"]["route"], "build worker");
        assert_eq!(v["context"]["tile"], "t.dtile");
        assert!(
            v["context"]["version"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        );
    }

    #[test]
    fn a_failure_carries_its_chain_of_causes_and_a_stack() {
        let outer = Layered::demo();
        let (status, body) = failure_from("500 Internal Server Error", "cannot write x", &outer);
        assert_eq!(status, "500 Internal Server Error");
        assert_eq!(body["error"]["kind"], "failure");
        assert_eq!(
            body["error"]["causes"],
            json!([
                "could not save the tile",
                "the folder is read-only",
                "permission denied (os error 13)"
            ])
        );
        assert!(!body["error"]["backtrace"].as_str().unwrap().is_empty());
        let (_, body) = refused_with("409 Conflict", "no", json!({ "text": "t" }));
        assert_eq!(body["error"]["kind"], "refused");
        assert_eq!(body["text"], "t");
    }
}
