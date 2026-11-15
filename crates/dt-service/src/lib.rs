//! The editor's service: what sits between a transport (HTTP in the program, a web worker in the browser) and the
//! engine.
//!
//! It knows no disk, no sockets and no HTTP: files come through a [`storage::Storage`] the host gives it (the
//! program's disk, or a [`storage::MemStorage`] in a worker), and time through a [`clock::Clock`], so the same code
//! builds for `wasm32-unknown-unknown`.  `cargo build -p dt-service --target wasm32-unknown-unknown` holds that.
//!
//! # What the owner, a patch and the tier book read of the binary
//!
//! Each binary type they use is one of three kinds: state or contract (it lives here), a transport concern (it stays
//! in the binary, and the service reaches it through a type parameter or a trait the binary implements), or engine
//! (it is already in a lower crate).
//!
//! | Binary type | Kind | Where it is |
//! |---|---|---|
//! | `Slot`, `State`, `Sketch`, `TileId`, `Row`, `Facts`, `Patch` | state | here: [`tile`], [`patch`]; `Row::changed` is given the time-of-file lookup, not read |
//! | `api::{TileSummary, Progress, Shown, SketchRef, ErrorBody, ErrorDetail, Snapshot, StreamEvent, RootJson, FolderJson}` | contract | here: [`api`] |
//! | `plan::*`, `scene::{SceneJson, Note, Placement}`, `resources::{Places, Target, List, Resource, ResourceEvent}` | contract | here: [`plan`], [`scene`], [`resources`] |
//! | `message::*`, `StackItem` and its parts, the inventory's wire types | contract | here: [`message`], [`stack`], [`inventory`] |
//! | `Storage`, `Entry`, `Clock` | seam | here: [`storage`], [`clock`] |
//! | `TileRecord`, `Mistake`, `Options`, `Diag`, `AuditSummary`, `Tier` | engine | `dt-eval`, `dt-kit`, `dt-store`, used from there |
//! | `Events`, `Change`, `Told` | state | here: [`events`]; an event that cannot be written goes to the [`host::Alarm`] the host gives |
//! | `Root`, `RootKind`, `Role`, `lib_group`, the folder scans | state | here: [`roots`]; the library's listing comes through [`storage::Storage::library_tiles`], a scan's alarms through [`host::Alarm`] |
//! | `queue::{Queue, Job, Lane, Draft, Outcome, Taking, Taken, Side}` | state | here: [`queue`], generic over a [`host::Host`]; a side lane's job is the host's own typed value ([`host::Host::Job`]), so the owner holds no closures |
//! | `tiers::{TierBook, BookOp}` | state | here: [`tiers`]; the length of a job is read from the owner's [`clock::Clock`] |
//! | `owner::{Core, Command, Owner}`, `Subscription`, `encode` | state | here: [`owner`]; `Core::apply` and `Core::handle` build everywhere, the mpsc thread loop (`Owner`) only off wasm |
//! | `Program` (the host: alarms to stderr, `fail::now_ms`, `SideJob`, a failed draft's `ServerError`) | transport | stays in the binary: `Core<Program>` is the program's instance (`preview/owner.rs`), `Owner::start` is called there |
//! | the typed verb table: [`call::Verb`], [`call::Table`] | seam | here: a call is a verb and a typed request in, a typed response out; the program builds its table from `calls!` (the calls that are not about a tile) and maps the answer to an HTTP reply |
//! | `Shared`, the build lanes, `SideJob::run`, `Cancelled`, `Superseded`, `catch_unwind` | transport | stay in the binary: the lanes take a typed job from the owner and run it on `Shared` |
//! | the tile calls that need no transport: [`tile_calls`] (`check`), [`edit_calls`] (`edit`, `save`), [`build_calls`] (`build`) | seam | here: each is one function the program's HTTP handler and the web worker's `Service` (dt-web) both call, written on [`tile_host::TileHost`] (documents, storage, library, `want`, and a check's options and report); a refusal is a typed [`call::Refusal`] the transport maps; [`docs`] holds the documents' revisions and lock |
//! | `tell!`, `lock`, `watch::mtime`, `fail`, the HTTP `Reply` and `Body`, the other tile verbs of `calls!` (places, plates, draft, step) and the build lanes' jobs | transport | stay in the binary; moving those handlers behind `TileHost` is what is left |
//!
//! The wire types are written once, here, and `web/preview/js/api.d.ts` is generated from them (`ts-rs`), so a
//! rename fails `tsc` in the page.

pub mod api;
pub mod build_calls;
pub mod call;
pub mod clock;
pub mod docs;
pub mod edit_calls;
pub mod events;
pub mod host;
pub mod inventory;
pub mod message;
pub mod owner;
pub mod patch;
pub mod plan;
pub mod queue;
pub mod resources;
pub mod roots;
pub mod scene;
pub mod stack;
pub mod storage;
pub mod tiers;
pub mod tile;
pub mod tile_calls;
pub mod tile_host;
