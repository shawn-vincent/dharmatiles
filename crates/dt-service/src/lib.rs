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
//! | `Events`, `Change`, `resources::Told` | state | to move: `Events::send` reports a failure to write an event through `tell!` (stderr), so it needs an alarm the host gives it |
//! | `Root`, `RootKind`, `Role`, `lib_group` | state | to move: the library's listing is `crate::all_library_tiles`, the binary's, so it needs to come through [`storage::Storage`] or the engine |
//! | `queue::{Queue, Job, Lane, Draft, Outcome, Taking, Taken}` | state | to move, generic over the host type `H`: the `Side` jobs are closures over `Shared` (`FnOnce(&H, &Arc<AtomicBool>)`) |
//! | `tiers::{TierBook, BookOp}` | state | to move with `Command`: `BookOp` and `Command` name each other |
//! | `owner::{Core, Command}` and the thread loop | state | to move last; the mpsc loop goes behind `cfg(not(target_arch = "wasm32"))` |
//! | `Shared`, the build lanes, `Cancelled`, `Superseded`, `catch_unwind` | transport | stay in the binary: `Core<Shared>` is the program's instance |
//! | `tell!`, `lock`, `watch::mtime`, `fail`, the HTTP `Reply` and `Body` | transport | stay in the binary |
//!
//! The wire types are written once, here, and `web/preview/js/api.d.ts` is generated from them (`ts-rs`), so a
//! rename fails `tsc` in the page.

pub mod api;
pub mod clock;
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
