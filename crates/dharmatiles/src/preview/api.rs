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
//!   [message](crate::message) is its own document, shared with the command line and an assistant
//!   (`GET /api/messages`), and keeps `snake_case` everywhere it appears, in the stream and out of it.  Every
//!   field of it, and of the stream's events, is a type here, generated into `api.d.ts`, so the page cannot read a
//!   name the program does not send: `tsc` fails on it.
//! - One error shape ([`ErrorBody`]), with the honest status: 400 for a
//!   request that is not a request, 403 for one refused, 404 for what is not
//!   there, 409 for a file that moved on, 422 for a document that does not
//!   read, 500 for what the program could not do.  A 200 is a success.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::fail;

/// What to send: the status and the body.
pub(super) type Reply = (&'static str, Value);

/// What a route's function gives: its answer, or the reply that refuses.
pub(super) type Out<T> = Result<T, Reply>;

/// A JSON value the contract does not describe further (a mesh's build
/// record, an element's schema: read by the page as data, never by name).
pub type Json = Value;

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
    match serde_json::to_value(t) {
        Ok(v) => ("200 OK", v),
        Err(e) => fail::failure_from(
            "500 Internal Server Error",
            format!("cannot write the answer: {e}"),
            &e,
        ),
    }
}

/// A route's outcome as the reply to send.
pub(super) fn reply<T: Serialize>(r: Out<T>) -> Reply {
    r.map_or_else(|e| e, |t| ok(&t))
}

/// The error body every route that fails sends (see [`fail`]): the error,
/// and beside it what a refusal also says (the text a conflicting edit lost
/// to, a document's mistakes).
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: ErrorDetail,
    /// A 409: the file as it is now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A 409: the revision the file has now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_rev: Option<u64>,
    /// A 409 for a map: the map as it now reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<super::scene::SceneJson>,
    /// A 422: the document's mistakes, each at its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<Mistake>>,
    /// A 409 for a draft: a newer draft took its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
}

/// The error itself.
#[derive(Serialize, Default, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    /// `refused`, `failure` or `bug` (see [`fail`]).
    pub kind: String,
    pub message: String,
    pub help: Option<String>,
    pub causes: Vec<String>,
    pub backtrace: String,
    pub context: ErrorContext,
}

/// What the server was doing.
#[derive(Serialize, Default, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ErrorContext {
    pub route: Option<String>,
    pub tile: Option<String>,
    pub version: String,
    pub at_ms: u64,
}

/// One mistake of a document, at its place.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Mistake {
    pub code: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub help: Option<String>,
    /// A failure of the program, not of the document: shown as a bug.
    pub unexpected: bool,
}

impl From<&dt_eval::Diag> for Mistake {
    fn from(d: &dt_eval::Diag) -> Self {
        let (file, line, column) = d
            .location()
            .map_or((None, None, None), |(f, l, c)| (Some(f), Some(l), Some(c)));
        Self {
            code: d.code.to_owned(),
            message: d.message.clone(),
            file,
            line,
            column,
            help: d.help.clone(),
            unexpected: !crate::build::is_document_error(d.code),
        }
    }
}

/// Keys that hold data (an element's or a stage's name), not fields: what is
/// under them keeps its keys.
const DATA_MAPS: [&str; 3] = ["stage_secs", "stats", "left_out"];

/// A JSON document the engine writes (a tile's plan, a build's record, a
/// mesh's header) with every field name in the contract's one convention,
/// `camelCase`.  Done where such a document leaves for the page, so the
/// engine keeps its own names and the page sees one style.
#[must_use]
pub fn camel_keys(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    let inner = if DATA_MAPS.contains(&k.as_str()) {
                        x.clone()
                    } else {
                        camel_keys(x)
                    };
                    (camel(k), inner)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(camel_keys).collect()),
        other => other.clone(),
    }
}

/// `size_mm` as `sizeMm`; a name with no underscore as it is.
fn camel(k: &str) -> String {
    let mut out = String::with_capacity(k.len());
    let mut up = false;
    for c in k.chars() {
        if c == '_' && !out.is_empty() {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The mistakes of a document as the contract's type.
#[must_use]
pub fn mistakes(errs: &[dt_eval::Diag]) -> Vec<Mistake> {
    errs.iter().map(Mistake::from).collect()
}

/// A build's progress while it runs (a `progress` event's body).
#[derive(Serialize, Clone, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// What the work is: `building` (a tile's stages) or `loading` (a map's pieces).  Work in flight for a document
    /// is this one record, whatever the work: the bar, the running row and the status line read nothing else.
    pub what: String,
    pub fraction: f64,
    pub label: String,
    pub key: String,
    /// What the stage is counting: `[what, done, total]`; a total of 0 is just the step.
    pub count: Option<(String, usize, usize)>,
    pub elapsed_s: f64,
    pub eta_s: Option<f64>,
    pub estimate_s: Option<f64>,
}

/// One tile (or print list, or the inventory) as the page reads it.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TileSummary {
    pub id: String,
    pub path: String,
    pub name: String,
    /// The place it is listed under: an index into `roots`.
    pub root: usize,
    pub group: Option<String>,
    pub size: Option<String>,
    /// `idle`, `building`, `ready` or `failed`.
    pub status: String,
    /// What the plan pass or the plates' layout said (the page reads it by name; see `js/doc-views.js`).
    pub plan: Option<Json>,
    /// The build's record: audit, stages, timings.
    pub record: Option<Json>,
    pub errors: Vec<Mistake>,
    /// A build that panicked or could not be encoded: an error body's `error`.
    pub failure: Option<ErrorDetail>,
    pub progress: Option<Progress>,
    pub mesh_version: Option<u64>,
    /// The revision of the document the mesh is of: the mesh is fetched by it.
    pub doc_rev: Option<u64>,
    pub mesh_bytes: Option<usize>,
    pub sketches: Vec<SketchRef>,
    /// While the first build runs: the mesh on show is an earlier one (`kind`, `note`).
    pub shown: Option<Json>,
}

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SketchRef {
    pub id: u64,
    pub rough: bool,
}

/// One place the list shows.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RootJson {
    pub index: usize,
    pub label: String,
    /// `files`, `dir` or `library`.
    pub kind: String,
    pub path: Option<String>,
    pub role: String,
    pub writable: bool,
    pub exists: bool,
}

/// A folder of a place, by its path below it.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FolderJson {
    pub root: usize,
    pub path: String,
}

/// Every tile's state and what the page lists them with: `GET /api/tiles` and the
/// stream's first event.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The contract stamp of this program (`contract()`): a page that imported a different one is from another build.
    pub contract: String,
    pub rev: u64,
    pub epoch: String,
    pub system: String,
    pub printer: String,
    pub printers: Vec<Json>,
    pub overhang_deg: f64,
    pub steepest_deg: f64,
    pub square_mm: f64,
    pub roots: Vec<RootJson>,
    pub folders: Vec<FolderJson>,
    pub home: Option<String>,
    pub inventory: Json,
    pub tiles: Vec<TileSummary>,
}

/// This program's contract stamp: a hash of the contract (`web/preview/js/api.d.ts`), read at compile time from
/// `web/preview/js/contract.js`, the same file the page imports when it loads.  A page served from files newer
/// or older than the running program (a page read from disk after a merge, or a tab left open across a restart)
/// imported a different stamp, sees it differ in the first snapshot, and shows only that.  The test
/// `the_page_contract_is_current` writes and checks the file with `api.d.ts`.
pub fn contract() -> &'static str {
    const FILE: &str = include_str!("../../../../web/preview/js/contract.js");
    // the file is one generated line, `export const CONTRACT = '…';` (checked by the contract test); were it ever
    // malformed, the whole file would stand as the stamp, match no page, and every page would say so
    let value = FILE
        .split("CONTRACT = '")
        .nth(1)
        .and_then(|rest| rest.split('\'').next());
    value.unwrap_or(FILE)
}

/// The calls the page makes, each with its request and response type.  Makes
/// the dispatch (`post`, for the `POST`s, and `tile_post` for those under
/// `/api/tiles/ID/`) and, in tests, the TypeScript of the whole contract.
///
/// A function of the first kind is `fn(&Shared, Req) -> Out<Res>`, of the
/// second `fn(&Shared, &TileRef, Req) -> Out<Res>`.
macro_rules! calls {
    (
        post { $( $path:literal => $f:path : $req:ty => $res:ty; )* }
        tile { $( $what:literal => $tf:path : $treq:ty => $tres:ty; )* }
        get { $( $gpath:literal => $gres:ty; )* }
    ) => {
        /// The reply to a `POST` that is not under a tile, when `path` is one of the calls.
        pub(super) fn post(shared: &Shared, path: &str, body: &[u8]) -> Option<Reply> {
            match path {
                $( $path => Some(match request::<$req>(path, body) {
                    Ok(r) => reply($f(shared, r)),
                    Err(e) => e,
                }), )*
                _ => None,
            }
        }

        /// The reply to a `POST` under `/api/tiles/ID/`, when `what` is one of the calls.
        pub(super) fn tile_post(shared: &Shared, tile: &TileRef, what: &str, path: &str, body: &[u8]) -> Option<Reply> {
            match what {
                $( $what => Some(match request::<$treq>(path, body) {
                    Ok(r) => reply($tf(shared, tile, r)),
                    Err(e) => e,
                }), )*
                _ => None,
            }
        }

        /// The TypeScript of every type the calls (and the other typed routes) name.
        #[cfg(test)]
        pub(super) fn typescript() -> String {
            use ts_rs::TS;
            let mut routes = String::new();
            let mut all = crate::preview::api::Collect::default();
            $(
                all.add::<$req>();
                all.add::<$res>();
                routes.push_str(&format!("  \"POST {}\": {{ req: {}; res: {} }};\n", $path, <$req>::name(&all.cfg), <$res>::name(&all.cfg)));
            )*
            $(
                all.add::<$treq>();
                all.add::<$tres>();
                routes.push_str(&format!("  \"POST /api/tiles/:id/{}\": {{ req: {}; res: {} }};\n", $what, <$treq>::name(&all.cfg), <$tres>::name(&all.cfg)));
            )*
            let mut reads = String::new();
            $(
                all.add::<$gres>();
                reads.push_str(&format!("  \"GET {}\": {};\n", $gpath, <$gres>::name(&all.cfg)));
            )*
            crate::preview::api::extra_types(&mut all, &mut routes);
            all.finish(&routes, &reads)
        }
    };
}
pub(super) use calls;

/// The tile a call under `/api/tiles/ID/` is for.
#[derive(Clone)]
pub(super) struct TileRef {
    pub id: String,
    pub path: String,
    pub files: std::collections::BTreeMap<String, Option<std::time::SystemTime>>,
}

impl TileRef {
    /// The file a call is for: the tile's own document, or a file its last
    /// build read.
    ///
    /// # Errors
    /// 403: not a file of this tile.
    pub(super) fn file(&self, asked: Option<&str>) -> Out<String> {
        match asked {
            None => Ok(self.path.clone()),
            Some(f) if f == self.path || self.files.contains_key(f) => Ok(f.to_owned()),
            Some(_) => Err(fail::refused("403 Forbidden", "not a file of this tile")),
        }
    }
}

/// The types of the contract, gathered with everything they name.
#[cfg(test)]
pub(super) struct Collect {
    pub cfg: ts_rs::Config,
    seen: std::collections::BTreeSet<std::any::TypeId>,
    decls: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
impl Default for Collect {
    fn default() -> Self {
        Self {
            // JSON numbers are numbers: the page has no bigint
            cfg: ts_rs::Config::default().with_large_int("number"),
            seen: std::collections::BTreeSet::new(),
            decls: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
impl Collect {
    pub(super) fn add<T: ts_rs::TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(std::any::TypeId::of::<T>()) {
            return;
        }
        // a builtin (a number, a string, an array) has no declaration of its own
        if T::output_path().is_some() {
            self.decls.insert(T::ident(&self.cfg), T::decl(&self.cfg));
        }
        struct V<'a>(&'a mut Collect);
        impl ts_rs::TypeVisitor for V<'_> {
            fn visit<U: ts_rs::TS + 'static + ?Sized>(&mut self) {
                self.0.add::<U>();
            }
        }
        T::visit_dependencies(&mut V(self));
    }

    pub(super) fn finish(self, routes: &str, reads: &str) -> String {
        let mut out = String::from(
            "// Generated from crates/dharmatiles/src/preview (api.rs and the types it names) by\n\
             // `DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current`.\n\
             // Do not edit: a test fails when this file is not what the Rust types say.\n\n\
             ",
        );
        for d in self.decls.values() {
            out.push_str(d);
            out.push_str("\n\n");
        }
        out.push_str("/** Every call that writes: its request body and its answer. */\nexport interface Calls {\n");
        out.push_str(routes);
        out.push_str("}\n\n/** Every read: its answer. */\nexport interface Reads {\n");
        out.push_str(reads);
        out.push_str(
            "}\n\n/** The request body of call `R`. */\nexport type Req<R extends keyof Calls> = Calls[R][\"req\"];\n\n\
             /** The answer of call `R`. */\nexport type Res<R extends keyof Calls> = Calls[R][\"res\"];\n",
        );
        out
    }
}

/// Types and routes that are in the contract without being a table row: the
/// draft, which answers a mesh's bytes on success.
#[cfg(test)]
pub(super) fn extra_types(all: &mut Collect, routes: &mut String) {
    use ts_rs::TS;
    all.add::<super::edit::DraftReq>();
    all.add::<ErrorBody>();
    all.add::<super::resources::ResourceEvent>();
    all.add::<StreamEvent>();
    all.add::<crate::message::MessagesRes>();
    all.add::<crate::message::GateData>();
    all.add::<crate::message::BuildData>();
    all.add::<crate::message::ServerData>();
    all.add::<crate::message::DiagData>();
    routes.push_str(&format!(
        "  \"POST /api/tiles/:id/draft\": {{ req: {}; res: ArrayBuffer }};\n",
        super::edit::DraftReq::name(&all.cfg)
    ));
}

/// One event of the stream (`GET /api/events`), tagged by `type`: the page's stream reader is typed by this one
/// type, so a field renamed here fails `tsc` in the page's reader.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    /// The whole state: the first event of a stream that cannot resume.
    Snapshot(Box<Snapshot>),
    /// A stream that resumed: what changed since is sent after this.
    Resumed { rev: u64, epoch: String },
    /// One tile's state changed.
    Tile { rev: u64, tile: Box<TileSummary> },
    /// A build's tick (not a revision: it changes nothing a page resumes from).
    Progress {
        rev: u64,
        id: String,
        progress: Progress,
    },
    /// A message was said (or its state changed).
    Message {
        rev: u64,
        message: crate::message::MessageJson,
    },
    /// A map's tier is built at `doc_rev` (or why it could not be).
    Tier {
        rev: u64,
        key: String,
        doc_rev: u64,
        error: Option<String>,
    },
    /// A resource changed.
    Resource(super::resources::ResourceEvent),
}
