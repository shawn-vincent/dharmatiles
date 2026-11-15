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
//! | `EditReq`, `EditOp`, `EditRes`, `NodeJson`, `text_rev`, and the one function that makes an edit | contract | here: [`edit`]; `/api/edit` and the browser engine's `edit_tile` both call [`edit::apply`] |
//! | `message::*`, `StackItem` and its parts, the inventory's wire types | contract | here: [`message`], [`stack`], [`inventory`] |
//! | the mesh payload the page reads (`header::MeshHeader` and its parts, `payload::{encode, from_built, for_map, from_draft, sketch, debug_marks, clouds_of}`) | contract | here: [`header`], [`payload`]; the program's `/api/.../mesh` and the web worker's `Service` both make the page's mesh bytes with them |
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
//! | the draft, reading and place calls: [`draft_calls`] (`draft`, `step`, on the host's draft lane), [`read_calls`] (`names`, `tokens`, `checkValue`, `printed`, `printedSet`, `offers`, `about`, `elements`, `language`, `text`; the schema's JSON is [`elements_json`]), [`place_calls`] (`newTile`, `mine`) | seam | here: written on [`tile_host::TileHost`] like the others; the host also gives its `version`, the first `snapshot`, My tiles (`home`) and the listing of what was written (`listed`) |
//! | the print list and map calls: [`list_calls`] (`listsNew`, `listsAdd`, `listsCount`, `listsRemove`, `listsRestore`, and a list's `check`), [`map_calls`] (`mapsAdd`, `mapsSet`, `mapsBatch`, `mapsDup`, `mapsRemove`, `mapsPin`, `mapsUnpin`, `mapsUnpinAll`), with [`scene_read`] (a map read as a scene, no geometry) and [`placing`] (the nearest free square, a fresh seed) | seam | here: written on [`list_calls::ListHost`] (a [`scene_read::SceneHost`] and a [`folder_calls::PlaceHost`]: where My tiles is, where a piece's tile is read from, a printer's machine, the shelf, what a reading remembers, and the plates planned again after an edit) |
//! | planning a print list or a map: [`plates_calls`] (`rebuild`, `platesShow`, `platesSet`, a map's crowns) | seam | here: written on [`plates_calls::PlatesHost`] (a [`list_calls::ListHost`] that also takes the owner's patches, builds a piece, knows the files' times and keeps its own clock); the program builds on its lanes and disk cache, the worker in its own turn |
//! | the inventory: [`inventory_calls`] (the one reader and writer of `inventory.dinv`, `inventoryHave`, `inventoryRecord`, `inventoryGone`, `inventoryRelink`, `inventoryReprint`, an opened `.dinv`) | seam | here: written on [`inventory_calls::InventoryHost`] (a [`plates_calls::PlatesHost`] that also knows today's date, says a message and gives a list's plan); the files are the host's [`docs::Docs`] over its [`storage::Storage`] |
//! | `tell!`, `lock`, `watch::mtime`, `fail`, the HTTP `Reply` and `Body`, the plates' export, the printers handlers of `calls!` and the build lanes' jobs | transport | stay in the binary; moving those handlers behind a host is what is left |
//!
//! The wire types are written once, here, and `web/preview/js/api.d.ts` is generated from them (`ts-rs`), so a
//! rename fails `tsc` in the page.

pub mod api;
pub mod build_calls;
pub mod call;
pub mod clock;
pub mod docs;
pub mod draft_calls;
pub mod edit;
pub mod edit_calls;
pub mod elements_json;
pub mod events;
pub mod folder_calls;
pub mod header;
pub mod host;
pub mod inventory;
pub mod inventory_calls;
pub mod list_calls;
pub mod map_calls;
pub mod message;
pub mod owner;
pub mod patch;
pub mod payload;
pub mod place_calls;
pub mod placing;
pub mod plan;
pub mod plates_calls;
pub mod queue;
pub mod read_calls;
pub mod resources;
pub mod roots;
pub mod scene;
pub mod scene_read;
pub mod stack;
pub mod storage;
pub mod target_calls;
pub mod tiers;
pub mod tile;
pub mod tile_calls;
pub mod tile_host;
