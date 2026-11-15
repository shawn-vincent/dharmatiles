//! The contract between the preview's server and its page: every call the
//! page makes has one request type and one response type, written here once
//! (or beside the code that answers, and listed in [`calls!`]), and nothing
//! else says what a route takes or gives.
//!
//! - The server reads a request body into its type (an unknown or missing
//!   field is a 400 that names it) and writes an answer by serializing its
//!   type: no route builds JSON by hand.
//! - `web/preview/js/api.d.ts` is generated from these types (`ts-rs`) by
//!   the test `the_page_contract_is_current`, which fails when the checked-in
//!   file differs; `DT_CONTRACT=write cargo test -p dharmatiles --bins
//!   the_page_contract_is_current` rewrites it.  The page's one request layer
//!   (`js/host.js`) is checked against that file by `tsc` (`npm test`), so
//!   renaming a field here fails on the page too, in the code that used it.
//! - One field-name convention for every JSON the page reads: `camelCase`, with one stated exception: a
//!   [message](dt_service::message) is its own document, shared with the command line and an assistant
//!   (`GET /api/messages`), and keeps `snake_case` everywhere it appears, in the stream and out of it.  Every
//!   field of it, and of the stream's events, is a type here, generated into `api.d.ts`, so the page cannot read a
//!   name the program does not send: `tsc` fails on it.
//! - One error shape ([`ErrorBody`]), with the honest status: 400 for a
//!   request that is not a request, 403 for one refused, 404 for what is not
//!   there, 409 for a file that moved on, 422 for a document that does not
//!   read, 500 for what the program could not do.  A 200 is a success.

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::fail;

/// What to send: the status and the body.
pub(super) type Reply = (&'static str, Body);

/// A reply's body, typed until it is written to the socket: a success is the response type serialized once
/// ([`ok`]), a refusal is the [`ErrorBody`] itself.  Nobody holds the JSON of a reply as a `Value`; the server asks
/// [`Body::wire`] for the text, and a test asks [`Body::read`] for the route's response type.
#[derive(Debug)]
pub enum Body {
    /// The JSON of a route's response type.
    Done(String),
    /// Every reply that is not a success.
    Failed(Box<ErrorBody>),
}

impl Body {
    /// The JSON text for the socket.
    ///
    /// # Errors
    /// An error body that cannot be written: the program's own failure.
    pub fn wire(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::Done(text) => Ok(text.clone()),
            Self::Failed(e) => serde_json::to_string(e),
        }
    }

    /// The body as the route's response type.
    ///
    /// # Errors
    /// The body is an error body, or is not that type.
    #[cfg(test)]
    pub(super) fn read<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.wire()?)
    }

    /// The error body of a refusal or failure, `None` for a success.
    pub fn failed(&self) -> Option<&ErrorBody> {
        match self {
            Self::Failed(e) => Some(e),
            Self::Done(_) => None,
        }
    }
}

/// What a route's function gives: its answer, or the reply that refuses.
pub(super) type Out<T> = Result<T, Reply>;

/// A request body as its type.
///
/// # Errors
/// A 400 that names what is wrong: not JSON, a field missing, a field that
/// is not one of this call's, a field of the wrong kind.
pub(super) fn request<T: DeserializeOwned>(path: &str, body: &[u8]) -> Result<T, Reply> {
    serde_json::from_slice(body)
        .map_err(|e| fail::refused("400 Bad Request", format!("{path} does not take this: {e}")))
}

/// A success as the reply: the type serialized.  A value that cannot be
/// written is the program's failure, said loudly.
pub(super) fn ok<T: Serialize>(t: &T) -> Reply {
    match serde_json::to_string(t) {
        Ok(v) => ("200 OK", Body::Done(v)),
        Err(e) => fail::failure_from(
            "500 Internal Server Error",
            format!("cannot write the answer: {e}"),
            &e,
        ),
    }
}

/// What the service's table answered, as the reply to send: a success is its type written, a refusal is its own
/// reply, and a call the table could not make is the program's own failure, said loudly.
pub(super) fn called<T: Serialize>(r: Result<T, dt_service::call::CallError<Reply>>) -> Reply {
    use dt_service::call::CallError;
    match r {
        Ok(t) => ok(&t),
        Err(CallError::Refused(reply)) => reply,
        Err(e) => fail::failure(
            "500 Internal Server Error",
            format!("the call table is wrong: {e:?}"),
        ),
    }
}

/// A route's outcome as the reply to send.
pub(super) fn reply<T: Serialize>(r: Out<T>) -> Reply {
    match r {
        Ok(t) => ok(&t),
        Err(e) => e,
    }
}

pub use dt_service::api::*;

/// The calls the page makes, each named by what it does and with its request and response type.  Makes the
/// dispatch (`post_verb`, for the calls that are not about one tile, and `tile_verb` for those that are) and, in
/// tests, the TypeScript of the whole contract, keyed by those names.  Which method and route a name is, is the
/// HTTP transport's business ([`super::routes`]).
///
/// A function of the first kind is `fn(&Shared, Req) -> Out<Res>`, of the
/// second `fn(&Shared, &TileRef, Req) -> Out<Res>`.
macro_rules! calls {
    (
        post { $( $verb:literal => $f:path : $req:ty => $res:ty; )* }
        tile { $( $tverb:literal => $tf:path : $treq:ty => $tres:ty; )* }
        get { $( $gverb:literal => $gres:ty; )* }
    ) => {
        /// The reply to the call `verb`, when it is one that is not about a tile.  `path` names the request in a refusal.
        pub(super) fn post_verb(shared: &Shared, verb: &str, path: &str, body: &[u8]) -> Option<Reply> {
            match verb {
                $( $verb => Some(match request::<$req>(path, body) {
                    Ok(r) => called(table().call(shared, &dt_service::call::Verb::<$req, $res>::new($verb), r)),
                    Err(e) => e,
                }), )*
                _ => None,
            }
        }

        /// The calls that are not about a tile, as the service's table of verbs: each row is the call's function on
        /// the program's state ([`dt_service::call::Table`]).
        fn table() -> &'static dt_service::call::Table<Shared, Reply> {
            static TABLE: std::sync::LazyLock<dt_service::call::Table<Shared, Reply>> =
                std::sync::LazyLock::new(|| {
                    dt_service::call::Table::default()
                    $( .with(
                        &dt_service::call::Verb::<$req, $res>::new($verb),
                        |shared: &Shared, r: $req| $f(shared, r),
                    ) )*
                });
            &TABLE
        }

        /// The reply to the call `verb` about `tile`.
        pub(super) fn tile_verb(shared: &Shared, tile: &TileRef, verb: &str, path: &str, body: &[u8]) -> Option<Reply> {
            match verb {
                $( $tverb => Some(match request::<$treq>(path, body) {
                    Ok(r) => reply($tf(shared, tile, r)),
                    Err(e) => e,
                }), )*
                _ => None,
            }
        }

        /// Every typed call's name: the table's rows (a test holds that each has a route).
        #[cfg(test)]
        pub(super) const VERBS: &[&str] = &[ $( $verb, )* $( $tverb, )* $( $gverb, )* ];

        /// The TypeScript of every type the calls (and the other typed routes) name.
        #[cfg(test)]
        pub(super) fn typescript() -> String {
            use ts_rs::TS;
            let mut routes = String::new();
            let mut all = crate::preview::api::Collect::default();
            $(
                all.add::<$req>();
                all.add::<$res>();
                routes.push_str(&format!("  \"{}\": {{ req: {}; res: {} }};\n", $verb, <$req>::name(&all.cfg), <$res>::name(&all.cfg)));
            )*
            $(
                all.add::<$treq>();
                all.add::<$tres>();
                routes.push_str(&format!("  \"{}\": {{ req: {}; res: {} }};\n", $tverb, <$treq>::name(&all.cfg), <$tres>::name(&all.cfg)));
            )*
            let mut reads = String::new();
            $(
                all.add::<$gres>();
                reads.push_str(&format!("  \"{}\": {};\n", $gverb, <$gres>::name(&all.cfg)));
            )*
            crate::preview::api::extra_types(&mut all, &mut routes);
            crate::preview::api::finish(&all, &routes, &reads)
        }
    };
}
pub(super) use calls;

/// The tile a call under `/api/tiles/ID/` is for.
pub(crate) use dt_service::tile_host::TileRef;

/// What the program's handlers still say of a [`TileRef`]: its file, as a reply.
pub(crate) trait TileRefReply {
    /// The file a call is for: the tile's own document, or a file its last build read.
    ///
    /// # Errors
    /// 403: not a file of this tile.
    fn file(&self, asked: Option<&str>) -> Out<String>;
}

impl TileRefReply for TileRef {
    fn file(&self, asked: Option<&str>) -> Out<String> {
        self.resolve_file(asked).map_err(refusal_reply)
    }
}

/// A service call's refusal as the HTTP reply: the status line, and the same error body every route sends.
pub(super) fn refusal_reply(r: dt_service::call::Refusal) -> Reply {
    use dt_service::call::{Fault, Status};
    let status = match r.status {
        Status::BadRequest => "400 Bad Request",
        Status::Forbidden => "403 Forbidden",
        Status::NotFound => "404 Not Found",
        Status::Conflict => "409 Conflict",
        Status::Unprocessable => "422 Unprocessable Entity",
        Status::Internal => "500 Internal Server Error",
    };
    let mut e = match r.fault {
        Fault::Refused => fail::ServerError::refused(status, r.message),
        Fault::Failure => fail::ServerError::failure(status, r.message),
    };
    e.causes = r.causes;
    e.reply_with(|b| {
        b.text = r.text;
        b.doc_rev = r.doc_rev;
        b.errors = r.errors;
    })
}

/// What a service call gave, as an [`Out`].
pub(super) fn out<T>(r: Result<T, dt_service::call::Refusal>) -> Out<T> {
    r.map_err(refusal_reply)
}

/// The gatherer of the contract's types is `dt_doc::contract::Contract`, shared with the site's engine contract.
#[cfg(test)]
pub(super) use dt_doc::contract::Contract as Collect;

/// The head of the generated file, then every declaration the calls name, then the route tables.
#[cfg(test)]
pub(super) fn finish(all: &Collect, routes: &str, reads: &str) -> String {
    let mut out = String::from(
        "// Generated from crates/dharmatiles/src/preview (api.rs and the types it names) by\n\
         // `DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current`.\n\
         // Do not edit: a test fails when this file is not what the Rust types say.\n\n\
         ",
    );
    out.push_str(&all.declarations());
    out.push_str("/** Every call made with a body, by its name: its request body and its answer. */\nexport interface Calls {\n");
    out.push_str(routes);
    out.push_str("}\n\n/** Every read, by its name: its answer. */\nexport interface Reads {\n");
    out.push_str(reads);
    out.push_str(
        "}\n\n/** The request body of call `R`. */\nexport type Req<R extends keyof Calls> = Calls[R][\"req\"];\n\n\
         /** The answer of call `R`. */\nexport type Res<R extends keyof Calls> = Calls[R][\"res\"];\n",
    );
    out
}

/// Types and routes that are in the contract without being a table row: the
/// draft, which answers a mesh's bytes on success.
#[cfg(test)]
pub(super) fn extra_types(all: &mut Collect, routes: &mut String) {
    use ts_rs::TS;
    all.add::<super::edit::DraftReq>();
    all.add::<dt_eval::map::PosterMap>();
    all.add::<ErrorBody>();
    // the header of every mesh: the page reads it off the front of the bytes a draft, a step or a mesh route sends
    all.add::<super::header::MeshHeader>();
    // `GET /api/elements` answers a list of them, which names no declaration of its own
    all.add::<dt_doc::wire::ElementJson>();
    all.add::<dt_doc::wire::LanguageJson>();
    all.add::<super::resources::ResourceEvent>();
    all.add::<StreamEvent>();
    all.add::<dt_service::message::MessagesRes>();
    all.add::<crate::report::Report>();
    all.add::<dt_kit::AuditSummary>();
    all.add::<dt_service::message::BuildData>();
    all.add::<dt_service::message::ServerData>();
    all.add::<dt_service::message::DiagData>();
    routes.push_str(&format!(
        "  \"draft\": {{ req: {}; res: ArrayBuffer }};\n",
        super::edit::DraftReq::name(&all.cfg)
    ));
    all.add::<super::edit::StepReq>();
    all.add::<super::edit::StepHead>();
    routes.push_str(&format!(
        "  \"step\": {{ req: {}; res: ArrayBuffer }};\n",
        super::edit::StepReq::name(&all.cfg)
    ));
}
